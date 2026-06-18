//! Automation runner - main entry point for the automation loop.
//!
//! Coordinates the automation thread and OCR worker thread.
//! Spawns threads, runs the state machine, and handles completion.

use anyhow::{anyhow, Result};
use chrono::Local;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;
use std::thread;

use crate::automation::config::get_config;
use crate::automation::csv_writer::init_csv;
use crate::automation::ocr_worker::run_ocr_worker;
use crate::automation::queue::create_work_queue;
use crate::automation::state::{reset_abort_flag, AutomationContext, AutomationState};
use crate::capture::find_gakumas_window;

/// Global flag indicating if automation is currently running.
static AUTOMATION_RUNNING: AtomicBool = AtomicBool::new(false);

/// Current iteration counter (for GUI progress display).
static CURRENT_ITERATION: AtomicU32 = AtomicU32::new(0);

/// Total iterations for current run (for GUI progress display).
static TOTAL_ITERATIONS: AtomicU32 = AtomicU32::new(0);

/// Current state description (for GUI progress display).
static CURRENT_STATE_DESC: Mutex<String> = Mutex::new(String::new());

/// Current session folder path (for GUI to access after completion).
static CURRENT_SESSION_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

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

/// Outcome of the most recently finished automation run.
static LAST_OUTCOME: Mutex<Option<AutomationOutcome>> = Mutex::new(None);

/// Default number of iterations if not specified in config.
const DEFAULT_ITERATIONS: u32 = 10;

/// Checks if automation is currently running.
pub fn is_automation_running() -> bool {
    AUTOMATION_RUNNING.load(Ordering::SeqCst)
}

/// Gets the current iteration number (0-based, for GUI progress display).
pub fn get_current_iteration() -> u32 {
    CURRENT_ITERATION.load(Ordering::SeqCst)
}

/// Gets the total number of iterations for current run.
pub fn get_total_iterations() -> u32 {
    TOTAL_ITERATIONS.load(Ordering::SeqCst)
}

/// Gets the current state description (for GUI progress display).
pub fn get_current_state_description() -> String {
    CURRENT_STATE_DESC
        .lock()
        .map(|s| s.clone())
        .unwrap_or_else(|_| "不明".to_string())
}

/// Updates the current state description (called from automation thread).
fn update_state_description(desc: &str) {
    if let Ok(mut s) = CURRENT_STATE_DESC.lock() {
        *s = desc.to_string();
    }
}

/// Gets the current session folder path (for GUI to access).
pub fn get_current_session_path() -> Option<PathBuf> {
    CURRENT_SESSION_PATH
        .lock()
        .ok()
        .and_then(|p| p.clone())
}

/// Sets the current session folder path (called at automation start).
fn set_current_session_path(path: PathBuf) {
    if let Ok(mut p) = CURRENT_SESSION_PATH.lock() {
        *p = Some(path);
    }
}

/// Gets the outcome of the most recently finished automation run.
///
/// Returns `None` if no run has finished yet (e.g. one is still in progress, or
/// the outcome was cleared at the start of a new run).
pub fn get_last_outcome() -> Option<AutomationOutcome> {
    LAST_OUTCOME.lock().ok().and_then(|o| o.clone())
}

/// Sets the outcome of the just-finished automation run (called from the
/// automation thread before it clears the running flag).
fn set_last_outcome(outcome: AutomationOutcome) {
    if let Ok(mut o) = LAST_OUTCOME.lock() {
        *o = Some(outcome);
    }
}

/// Clears the stored outcome (called when a new run starts) so the GUI never
/// reads a stale result from a previous run.
fn clear_last_outcome() {
    if let Ok(mut o) = LAST_OUTCOME.lock() {
        *o = None;
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
    clear_last_outcome();

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

    set_current_session_path(session_dir.clone());

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

    // Seed progress with already-completed runs so the bar resumes correctly.
    CURRENT_ITERATION.store(start_iteration.saturating_sub(1), Ordering::SeqCst);
    TOTAL_ITERATIONS.store(iterations, Ordering::SeqCst);
    update_state_description(if is_resume { "再開中..." } else { "開始中..." });

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
    let score_regions = config.score_regions;
    let total_regions = config.total_regions;
    let bonus_regions = config.bonus_regions;
    let csv_path_clone = csv_path.clone();
    let ocr_handle = thread::spawn(move || {
        run_ocr_worker(receiver, csv_path_clone, score_regions, total_regions, bonus_regions);
    });

    // Create and run state machine
    let mut ctx = AutomationContext::new(
        hwnd, config, max_iterations, start_iteration, sender, screenshot_dir,
    );

    // Run state machine until complete
    loop {
        // Update progress counters for GUI
        CURRENT_ITERATION.store(ctx.current_iteration, Ordering::SeqCst);
        update_state_description(&ctx.state.description_ja());

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
    set_last_outcome(outcome);

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
