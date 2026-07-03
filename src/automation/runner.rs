//! Automation runner - main entry point for the automation loop.
//!
//! Coordinates the automation thread and OCR worker thread.
//! Spawns threads, runs the state machine, and handles completion.

use anyhow::{anyhow, Result};
use chrono::Local;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread;

use crate::automation::config::get_config;
use crate::automation::csv_writer::init_csv;
use crate::automation::ocr_worker::run_ocr_worker;
use crate::automation::queue::create_work_queue;
use crate::automation::state::{reset_abort_flag, AutomationContext, AutomationState, LiveGame};
use crate::capture::find_gakumas_window;

/// Global flag indicating if automation is currently running.
///
/// Kept as a separate atomic (not a `PROGRESS` field) because its `swap` is
/// the mutual-exclusion guard for starting a run.
static AUTOMATION_RUNNING: AtomicBool = AtomicBool::new(false);

/// One row of live OCR scores for the in-progress run's distribution view.
///
/// `flagged` is true when overlap-recovery could not confidently reconstruct the
/// row (its worst stage came back `flagged`). Such rows are kept here but excluded
/// from the live statistics until verified, so the live box plot is never skewed
/// by an unconfirmed value.
#[derive(Clone, Copy, Debug)]
pub struct LiveScoreRow {
    /// The nine per-character scores: `[stage][slot]`.
    pub scores: [[u32; 3]; 3],
    /// Whether this row's OCR was flagged for review (excluded from live stats).
    pub flagged: bool,
}

/// Live score buffer for the in-progress run, read by the GUI thread to render
/// the live distribution figure. Reset on a fresh run, seeded from the existing
/// CSV on resume/extend. Mirrors the `CURRENT_STATE_DESC` mutex pattern.
static LIVE_SCORES: Mutex<Vec<LiveScoreRow>> = Mutex::new(Vec::new());

/// Records one completed iteration's scores into the live buffer (called from the
/// OCR worker thread). `flagged` rows are kept but excluded from live statistics.
pub fn record_live_score(scores: [[u32; 3]; 3], flagged: bool) {
    if let Ok(mut v) = LIVE_SCORES.lock() {
        v.push(LiveScoreRow { scores, flagged });
    }
}

/// Returns a clone of the current live score buffer (for the GUI to compute stats
/// without holding the lock while rendering).
pub fn get_live_scores() -> Vec<LiveScoreRow> {
    LIVE_SCORES.lock().map(|v| v.clone()).unwrap_or_default()
}

/// Number of rows currently in the live buffer (cheap change-detection for the GUI).
pub fn live_score_count() -> usize {
    LIVE_SCORES.lock().map(|v| v.len()).unwrap_or(0)
}

/// Empties the live score buffer (called at the start of every run).
fn clear_live_scores() {
    if let Ok(mut v) = LIVE_SCORES.lock() {
        v.clear();
    }
}

/// Replaces the live score buffer with the rows currently in a session's CSV.
///
/// Used after the review window saves manual corrections or verifications, so the
/// live distribution figure reflects the fixed/verified values (and re-includes rows
/// whose `flagged` state was cleared by verification). Safe to call when not running.
pub fn reload_live_scores_from_csv(session_dir: &Path) {
    clear_live_scores();
    seed_live_scores_from_csv(session_dir);
}

/// Seeds the live score buffer from an existing session's CSV so a resumed/extended
/// run's live figure reflects the whole series, not just newly-added points. A
/// missing or unreadable CSV must not abort the run, so errors are logged and the
/// buffer is left as-is. Flagged rows are seeded with `flagged: true` (excluded
/// from live stats until verified).
fn seed_live_scores_from_csv(session_dir: &Path) {
    match crate::automation::results_edit::load_review_rows(session_dir) {
        Ok(rows) => {
            if let Ok(mut v) = LIVE_SCORES.lock() {
                for r in rows {
                    v.push(LiveScoreRow {
                        scores: r.scores,
                        flagged: r.recovery == "flagged",
                    });
                }
            }
        }
        Err(e) => {
            crate::log(&format!(
                "Live distribution: could not seed from existing CSV ({}); starting empty",
                e
            ));
        }
    }
}

/// Final outcome of the most recent automation run (for GUI to read after the
/// automation thread exits). Distinguishes a full completion from a timeout/error
/// or a user abort, and records how many of the requested runs actually finished.
#[derive(Clone, Debug)]
pub enum AutomationOutcome {
    /// All requested runs completed successfully.
    Completed { completed: u32, total: u32 },
    /// User aborted before all runs finished.
    Aborted { completed: u32, total: u32 },
    /// Automation stopped early due to a timeout or error.
    Error {
        completed: u32,
        total: u32,
        message: String,
    },
}

/// Everything the GUI needs to display about the current (or most recent) run,
/// updated together under one lock so a reader never sees a mixed state (e.g.
/// the new run's iteration paired with the old run's session path).
#[derive(Clone, Debug)]
pub struct Progress {
    /// Current iteration number (0 before the first iteration starts).
    pub current_iteration: u32,
    /// Total iterations requested for the current run.
    pub total_iterations: u32,
    /// Japanese description of the state machine's current state.
    pub state_desc: String,
    /// Session folder of the current/most recent run.
    pub session_path: Option<PathBuf>,
    /// Outcome of the most recently finished run. `None` while a run is in
    /// progress (cleared at run start) or before any run has finished.
    pub last_outcome: Option<AutomationOutcome>,
}

/// Single source of truth for run progress, read by the GUI once per frame.
static PROGRESS: Mutex<Progress> = Mutex::new(Progress {
    current_iteration: 0,
    total_iterations: 0,
    state_desc: String::new(),
    session_path: None,
    last_outcome: None,
});

/// Default number of iterations if not specified in config.
const DEFAULT_ITERATIONS: u32 = 10;

/// Checks if automation is currently running.
pub fn is_automation_running() -> bool {
    AUTOMATION_RUNNING.load(Ordering::SeqCst)
}

/// Returns a snapshot of the current run progress. Callers should take one
/// snapshot per frame/decision and read fields from it, rather than calling
/// repeatedly, so all values describe the same moment.
pub fn get_progress() -> Progress {
    PROGRESS
        .lock()
        .map(|p| p.clone())
        .unwrap_or_else(|poisoned| poisoned.into_inner().clone())
}

/// The single mutation path for `PROGRESS` (runner-internal).
fn progress_update(f: impl FnOnce(&mut Progress)) {
    match PROGRESS.lock() {
        Ok(mut p) => f(&mut p),
        Err(poisoned) => f(&mut poisoned.into_inner()),
    }
}

/// Starts a fresh automation run in a background thread.
///
/// Returns immediately after spawning the automation thread.
/// Use `is_automation_running()` to check if automation is still active.
///
/// # Arguments
/// * `max_iterations` - Number of iterations to run (uses config default if None)
///
/// # Errors
/// Returns an error if:
/// - Automation is already running
/// - Game window cannot be found
pub fn start_automation(max_iterations: Option<u32>) -> Result<()> {
    let iterations = max_iterations.unwrap_or(DEFAULT_ITERATIONS);
    start_automation_inner(iterations, 1, None)
}

/// Resumes a previously interrupted run, appending into its existing folder.
///
/// Continues iteration numbering from `completed + 1` up to the original
/// `total`, reusing the same screenshots/, results.csv, and session.log.
pub fn resume_automation(session_dir: PathBuf, completed: u32, total: u32) -> Result<()> {
    if completed >= total {
        return Err(anyhow!("Nothing to resume: {}/{} already completed", completed, total));
    }
    if !session_dir.exists() {
        return Err(anyhow!(
            "Cannot resume: session folder no longer exists: {}",
            session_dir.display()
        ));
    }
    start_automation_inner(total, completed + 1, Some(session_dir))
}

/// Extends a finished run with `additional` brand-new iterations, appending
/// into its existing folder.
///
/// The number of already-captured runs is recomputed from the screenshots on
/// disk (the crash-proof source of truth), so the caller need only say how
/// many *more* runs to perform. New iterations are numbered `completed + 1`
/// through `completed + additional`, reusing the same screenshots/,
/// results.csv, session.log, and run-meta.json (whose `total` becomes the new,
/// larger value).
pub fn extend_automation(session_dir: PathBuf, additional: u32) -> Result<()> {
    if additional == 0 {
        return Err(anyhow!("Nothing to add: requested 0 additional runs"));
    }
    if !session_dir.exists() {
        return Err(anyhow!(
            "Cannot extend: session folder no longer exists: {}",
            session_dir.display()
        ));
    }
    let completed = crate::automation::session_meta::count_captured(&session_dir);
    let new_total = completed + additional;
    start_automation_inner(new_total, completed + 1, Some(session_dir))
}

/// Shared setup for fresh and resumed runs.
///
/// * `iterations`     - total runs; the loop stops once this is reached
/// * `start_iteration`- 1-based iteration to begin from (1 fresh; completed+1 resume)
/// * `existing_session` - reuse this folder if Some (resume); else create new (fresh)
fn start_automation_inner(
    iterations: u32,
    start_iteration: u32,
    existing_session: Option<PathBuf>,
) -> Result<()> {
    if AUTOMATION_RUNNING.swap(true, Ordering::SeqCst) {
        return Err(anyhow!("Automation is already running"));
    }

    reset_abort_flag();
    // Clear the previous run's outcome so the GUI never reads a stale result.
    progress_update(|p| p.last_outcome = None);
    clear_live_scores();

    let hwnd = match find_gakumas_window() {
        Ok(hwnd) => hwnd,
        Err(e) => {
            AUTOMATION_RUNNING.store(false, Ordering::SeqCst);
            return Err(anyhow!("Failed to find game window: {}", e));
        }
    };

    let config = get_config().clone();
    let is_resume = existing_session.is_some();

    let session_dir = match existing_session {
        Some(dir) => dir,
        None => {
            let timestamp = Local::now().format("%Y%m%d_%H%M%S").to_string();
            crate::paths::get_output_dir().join(&timestamp)
        }
    };

    if let Err(e) = fs::create_dir_all(&session_dir) {
        AUTOMATION_RUNNING.store(false, Ordering::SeqCst);
        return Err(anyhow!("Failed to create session directory: {}", e));
    }

    let session_dir_for_progress = session_dir.clone();
    progress_update(move |p| p.session_path = Some(session_dir_for_progress));

    let session_log_path = session_dir.join("session.log");
    crate::set_session_log(Some(session_log_path.clone()));

    let screenshot_dir = session_dir.join("screenshots");
    let csv_path = session_dir.join("results.csv");

    if let Err(e) = fs::create_dir_all(&screenshot_dir) {
        AUTOMATION_RUNNING.store(false, Ordering::SeqCst);
        return Err(anyhow!("Failed to create screenshot directory: {}", e));
    }

    if let Err(e) = init_csv(&csv_path) {
        AUTOMATION_RUNNING.store(false, Ordering::SeqCst);
        return Err(anyhow!("Failed to initialize CSV file: {}", e));
    }

    // On resume/extend, pre-fill the live distribution buffer from the rows already
    // in this session's CSV so the live figure reflects the whole series.
    if is_resume {
        seed_live_scores_from_csv(&session_dir);
    }

    // Seed progress with already-completed runs so the bar resumes correctly.
    progress_update(|p| {
        p.current_iteration = start_iteration.saturating_sub(1);
        p.total_iterations = iterations;
        p.state_desc = (if is_resume { "再開中..." } else { "開始中..." }).to_string();
    });

    // Record metadata so this run can be discovered/resumed later (M1 module).
    crate::automation::session_meta::write_meta(
        &session_dir,
        &crate::automation::session_meta::RunMeta {
            total: iterations,
            completed: start_iteration.saturating_sub(1),
            status: "running".to_string(),
            message: None,
            dismissed: false,
        },
    );

    if is_resume {
        crate::log(&format!(
            "Resuming automation from iteration {}/{} (Ctrl+Shift+Q to abort)",
            start_iteration, iterations
        ));
    } else {
        crate::log(&format!(
            "Starting automation: {} iterations (Ctrl+Shift+Q to abort)",
            iterations
        ));
    }
    crate::log(&format!("Session folder: {}", crate::paths::relative_display(&session_dir)));
    crate::log(&format!("Screenshots: {}", crate::paths::relative_display(&screenshot_dir)));
    crate::log(&format!("Results CSV: {}", crate::paths::relative_display(&csv_path)));

    // Extract raw pointer value to pass across thread boundary
    // SAFETY: HWND is just a pointer wrapper, and Windows handles are valid
    // across threads. We reconstruct it in the spawned thread.
    let hwnd_raw = hwnd.0 as usize;

    // Spawn automation thread
    thread::spawn(move || {
        // Reconstruct HWND from raw pointer value
        let hwnd = windows::Win32::Foundation::HWND(hwnd_raw as *mut std::ffi::c_void);
        run_automation_loop(hwnd, config, iterations, start_iteration, screenshot_dir, csv_path);
        AUTOMATION_RUNNING.store(false, Ordering::SeqCst);
        crate::log("Automation thread finished");
    });

    Ok(())
}

/// Runs the automation loop (called from the automation thread).
fn run_automation_loop(
    hwnd: windows::Win32::Foundation::HWND,
    config: crate::automation::config::AutomationConfig,
    max_iterations: u32,
    start_iteration: u32,
    screenshot_dir: PathBuf,
    csv_path: PathBuf,
) {
    // Create work queue
    let (sender, receiver) = create_work_queue();

    // Spawn OCR worker thread
    let regions = config.ocr_regions();
    let csv_path_clone = csv_path.clone();
    let ocr_handle = thread::spawn(move || {
        run_ocr_worker(receiver, csv_path_clone, regions);
    });

    // Create and run state machine, driving the live game window.
    let game = LiveGame::new(hwnd, config);
    let mut ctx = AutomationContext::new(
        game, max_iterations, start_iteration, sender, screenshot_dir,
    );

    // Run state machine until complete
    loop {
        // Update progress counters for GUI
        progress_update(|p| {
            p.current_iteration = ctx.current_iteration;
            p.state_desc = ctx.state.description_ja();
        });

        match ctx.step() {
            Ok(true) => {
                // Continue running
            }
            Ok(false) => {
                // Complete, error, or aborted
                break;
            }
            Err(e) => {
                crate::log(&format!("Automation error: {}", e));
                break;
            }
        }
    }

    // Log final state and record the outcome for the GUI. `completed_iterations`
    // counts runs that actually captured a result, so the GUI can report exactly
    // how far the automation got when it stops early.
    let completed = ctx.completed_iterations;
    let outcome = match &ctx.state {
        AutomationState::Complete => {
            crate::log(&format!(
                "Automation completed successfully: {}/{} iterations",
                completed, max_iterations
            ));
            AutomationOutcome::Completed {
                completed,
                total: max_iterations,
            }
        }
        AutomationState::Aborted => {
            crate::log(&format!(
                "Automation aborted: {}/{} iterations completed",
                completed, max_iterations
            ));
            AutomationOutcome::Aborted {
                completed,
                total: max_iterations,
            }
        }
        AutomationState::Error(msg) => {
            crate::log(&format!(
                "Automation failed after {}/{} iterations: {}",
                completed, max_iterations, msg
            ));
            AutomationOutcome::Error {
                completed,
                total: max_iterations,
                message: msg.clone(),
            }
        }
        other => {
            // Loop exited from an unexpected state (e.g. step() returned Err).
            crate::log(&format!(
                "Automation stopped in unexpected state {} after {}/{} iterations",
                other, completed, max_iterations
            ));
            AutomationOutcome::Error {
                completed,
                total: max_iterations,
                message: format!("Stopped unexpectedly ({})", other),
            }
        }
    };

    // Persist the final status so this session is correctly classified on disk
    // (no longer "running"; resumable only if it stopped short of `total`).
    let (meta_status, meta_message) = match &outcome {
        AutomationOutcome::Completed { .. } => ("completed", None),
        AutomationOutcome::Aborted { .. } => ("aborted", None),
        AutomationOutcome::Error { message, .. } => ("error", Some(message.clone())),
    };
    if let Some(session_dir) = csv_path.parent() {
        crate::automation::session_meta::write_meta(
            session_dir,
            &crate::automation::session_meta::RunMeta {
                total: max_iterations,
                completed,
                status: meta_status.to_string(),
                message: meta_message,
                dismissed: false,
            },
        );
    }
    progress_update(move |p| p.last_outcome = Some(outcome));

    // Drop the sender to signal OCR worker to finish
    drop(ctx.work_sender);

    // Wait for OCR worker to finish processing remaining items
    crate::log("Waiting for OCR worker to finish...");
    if let Err(e) = ocr_handle.join() {
        crate::log(&format!("OCR worker thread panicked: {:?}", e));
    }

    crate::log("All processing complete");

    // Deactivate per-session logging
    crate::set_session_log(None);
}

/// Re-export request_abort for convenience.
pub use crate::automation::state::request_abort;

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: `LIVE_SCORES` is process-global. This is the only test that touches it;
    // it clears the buffer at the top so it is self-contained even if other tests in
    // this binary run concurrently without referencing the live buffer.
    #[test]
    fn live_score_buffer_records_and_excludes_flagged() {
        clear_live_scores();
        assert_eq!(live_score_count(), 0);

        record_live_score([[1, 2, 3], [4, 5, 6], [7, 8, 9]], false);
        record_live_score([[10, 11, 12], [13, 14, 15], [16, 17, 18]], false);
        record_live_score([[0; 3]; 3], true);

        let rows = get_live_scores();
        assert_eq!(rows.len(), 3);
        assert_eq!(live_score_count(), 3);

        let flags: Vec<bool> = rows.iter().map(|r| r.flagged).collect();
        assert_eq!(flags, vec![false, false, true]);
        assert_eq!(rows[0].scores, [[1, 2, 3], [4, 5, 6], [7, 8, 9]]);

        // Filtering out flagged rows (as the live stats will) leaves the two trusted rows.
        let included: Vec<_> = rows.iter().filter(|r| !r.flagged).collect();
        assert_eq!(included.len(), 2);

        clear_live_scores();
    }
}
