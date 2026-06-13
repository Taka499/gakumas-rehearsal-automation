# Resume an interrupted automation run ("Continue" feature)

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

This repository's ExecPlan conventions live in `docs/PLANS.md` (relative to the repository root). This document must be maintained in accordance with `docs/PLANS.md`.

## Purpose / Big Picture

This tool (`gakumas-screenshot`) is a Windows system-tray + GUI app that automates the game "Gakumas" rehearsal screen: it runs a chosen number of rehearsal cycles ("runs"), capturing one screenshot per run into a timestamped session folder and extracting nine scores per screenshot with OCR. A "run" is one full rehearsal cycle that produces one screenshot and one row of scores in `results.csv`.

Today, if a run series stops early — because the game got stuck, the network hiccuped (a timeout/error), or the user pressed the abort hotkey — the only option is to start a brand-new series from zero. All the work already done is stranded in the old session folder.

After this change, the user can **continue** an interrupted series so it finishes the remaining runs, appending into the **same** session folder. Iteration numbering continues where it left off, and the unified `results.csv`, `screenshots/`, `session.log`, and regenerated charts cover the whole series as if it had never stopped. Crucially, this works **even after closing and reopening the app**: the app discovers interrupted series on disk and offers to resume them.

You can see it working like this: start a series of 5 runs, abort after 2, observe the GUI report "中断 (2/5回 完了)" with a "続行 (残り 3回)" button; click it (or, after restarting the app, pick the session from a list and resume); the app performs runs 3, 4, 5 into the same folder; `results.csv` ends with 5 data rows and one header; `run-meta.json` ends with `"status": "completed"`.

## Progress

- [x] (prerequisite, already in working tree) Accurate interrupted-run reporting: `AutomationOutcome` enum + `LAST_OUTCOME` in `src/automation/runner.rs`; `completed_iterations` counter in `src/automation/state.rs`; GUI `AutomationStatus` variants carry `completed`/`total`/`session_path` with `finalize_status` in `src/gui/mod.rs`. Verify present before starting (see Context).
- [x] (2026-06-13) M1: Persisted session metadata module `src/automation/session_meta.rs` (write/read `run-meta.json`, `count_captured`, `list_resumable`). Registered in `mod.rs`; `cargo check` clean.
- [x] (2026-06-13) M2: Resume-capable engine: `start_iteration` in the state machine; `resume_automation` + `start_automation_inner` in the runner; metadata writes at start and end. `cargo check` clean (only an expected `unused import: resume_automation` warning until M3 wires it).
- [x] (2026-06-13) M3: GUI quick "Continue last run" button driven by in-memory status. `resumable()` helper, `render_controls` returns 3-tuple, `handle_continue` wired in `update()`. `cargo check` clean.
- [x] (2026-06-13) M4: GUI "resume a previous session" picker driven by on-disk metadata (restart survival). `render_resume_picker`, `scan_resumable_sessions` (called in `new`, on finalize, on 更新), `handle_resume_selected`. `cargo build --release` succeeds; binary at `target\release\gakumas-screenshot.exe`. **Pending: manual acceptance scenarios A–D require the running game.**

Use timestamps (UTC) when checking off items, e.g. `- [x] (2026-06-06 14:00Z) ...`.

## Surprises & Discoveries

- Observation: When a per-character score reaches 1,000,000+, OCR could merge tokens — unrelated to this feature but note the OCR pipeline is in `src/ocr/`. Not in scope here.
- Observation: The originally requested run count (`total`) is held only in GUI memory (`GuiState`), never persisted. This is the sole reason restart-survival needs a metadata file; `completed` is recoverable from the screenshots on disk but `total` is not.
  Evidence: `src/automation/runner.rs` `start_automation` takes `max_iterations` and stores it only in the in-process atomic `TOTAL_ITERATIONS`; nothing on disk records it.
- (Add findings here as you implement, with short evidence snippets.)

## Decision Log

- Decision: Resume appends into the original session folder rather than creating a new one.
  Rationale: Keeps all data (screenshots, CSV, charts, log) for one logical series unified; existing code already supports append (see Context: `init_csv`, OCR worker, `set_session_log`).
  Date/Author: 2026-06-06 / planning.
- Decision: Persist a small `run-meta.json` per session and add an on-disk picker so resume survives app restarts.
  Rationale: User explicitly requested restart survival; `total` is not otherwise recoverable.
  Date/Author: 2026-06-06 / planning.
- Decision: `completed` for resume is recomputed from the screenshots directory (`count_captured`), treated as authoritative over any stored value.
  Rationale: Screenshots are written synchronously in the `Capturing` state before asynchronous OCR, so they are crash-proof and never lag; a hard crash that left stale metadata is still handled correctly.
  Date/Author: 2026-06-06 / planning.
- Decision: The first iteration of a resumed run must behave like a fresh start (no End-button retry), achieved by gating on `current_iteration > start_iteration` instead of `> 1`.
  Rationale: On resume the user has navigated the game back to the rehearsal start page; the automation has not just clicked the End button, so the End-click retry path must not run for the first resumed iteration.
  Date/Author: 2026-06-06 / planning.

## Outcomes & Retrospective

To be completed at the end of each milestone and at full completion. Compare against Purpose: can the user resume an interrupted series in-session and after restart, with unified output?

- 2026-06-13: M1–M4 implemented as specified in the plan; the prerequisite reporting work was confirmed present before starting. Compile gate met at each milestone (`cargo check` clean for M1–M3, `cargo build --release` clean for M4 — only pre-existing-style unused `pub use` warnings remain, including the intentional `resume_automation` re-export from M2). Behavioral acceptance (Scenarios A–D) is still **pending** — those require the running `gakumas.exe` and admin elevation, which cannot be exercised from the build environment. Next step for the user: run `target\release\gakumas-screenshot.exe` elevated with the game on the rehearsal start page and walk Scenarios A–D.

## Context and Orientation

You are working in a Rust 2024-edition Windows application. Build with `cargo build` / `cargo build --release` from the repository root (`C:\Work\GitRepos\gakumas-screenshot`). The executable carries an administrator manifest, so `cargo test` cannot launch the test binary (it fails with an elevation error). Therefore the compile gate for this work is `cargo check`, and behavioral acceptance is manual (running the app against the game). Treat `cargo check` passing + the manual scenarios in "Validation and Acceptance" as success.

Key directories and files you must understand:

- `src/automation/runner.rs` — Entry point for automation. Spawns a background thread running the state machine, plus an OCR worker thread. Owns process-global state via atomics and mutexes: `AUTOMATION_RUNNING` (bool), `CURRENT_ITERATION`/`TOTAL_ITERATIONS` (progress for the GUI), `CURRENT_SESSION_PATH` (the active session folder), and — added by the prerequisite work — `LAST_OUTCOME` (the result of the last finished run). Public functions include `start_automation(max_iterations: Option<u32>) -> anyhow::Result<()>`, `is_automation_running() -> bool`, `get_current_iteration() -> u32`, `get_total_iterations() -> u32`, `get_current_state_description() -> String`, `get_current_session_path() -> Option<PathBuf>`, and `get_last_outcome() -> Option<AutomationOutcome>`.
- `src/automation/state.rs` — The state machine. `enum AutomationState` has working states (`Idle`, `WaitingForStartPage`, `ClickingStart`, `WaitingForLoading`, `ClickingSkip`, `WaitingForResult`, `Capturing`, `ClickingEnd`, `CheckingLoop`) and terminal states (`Complete`, `Error(String)`, `Aborted`). `struct AutomationContext` holds `current_iteration: u32` (1-based index of the run being attempted), `completed_iterations: u32` (added by prerequisite work; counts runs that captured a result), `max_iterations: u32`, and more. `AutomationContext::new(hwnd, config, max_iterations, work_sender, screenshot_dir)` constructs it; `step(&mut self) -> Result<bool>` advances one transition, returning `Ok(false)` when terminal.
- `src/automation/csv_writer.rs` — `init_csv(path)` writes the header **only if the file is missing or empty** (it is a no-op when the file already has content, so it is safe to call when resuming). `append_to_csv` / `append_to_raw_csv` always append. No changes needed here.
- `src/automation/ocr_worker.rs` — Background thread; reads screenshots from a channel, OCRs them, and appends rows to `results.csv`. Always appends. No changes needed.
- `src/automation/config.rs` — `AutomationConfig` (timeouts, button positions, `score_regions`). `get_config()` returns the loaded config.
- `src/automation/mod.rs` — Module list and re-exports (`pub use runner::{is_automation_running, request_abort, start_automation};`).
- `src/paths.rs` — `get_output_dir()` returns the `output/` directory; `relative_display(path)` formats a path for logs without leaking absolute user paths.
- `src/main.rs` — Global logging. `set_session_log(Some(path))` activates per-session logging; `log()` writes to both the global log and the active session log, always in **append** mode (so re-activating an existing `session.log` on resume keeps prior history). `set_session_log(None)` deactivates.
- `src/gui/mod.rs` — The egui application (`struct GuiApp`). `update()` runs each frame: it polls automation status via `update_automation_status()`, then lays out a three-column UI; column three calls `render::render_controls`, `render::render_progress`, `render::render_actions`. Button handlers: `handle_start`, `handle_stop`, `handle_generate_charts`, `handle_open_folder`. The prerequisite work added `finalize_status(...)` which converts the runner's `AutomationOutcome` into an `AutomationStatus`.
- `src/gui/state.rs` — `enum AutomationStatus { Idle, Running{current,total,state_description,start_time}, Completed{completed,total,session_path}, Aborted{completed,total,session_path:Option<PathBuf>}, Error{completed,total,message,session_path:Option<PathBuf>} }` and `struct GuiState { iterations, status, latest_session_path, automation_start_time }`. `status_text()` and `progress()` already render `completed/total`.
- `src/gui/render.rs` — Rendering helpers. `render_controls(ui, &mut GuiState) -> (bool, bool)` currently returns `(start_clicked, stop_clicked)`.

Assumptions you rely on (verify before starting): the prerequisite "accurate reporting" work is present in the working tree. Concretely, `src/automation/runner.rs` must already contain `pub enum AutomationOutcome { Completed{completed,total}, Aborted{completed,total}, Error{completed,total,message} }`, `static LAST_OUTCOME`, and `get_last_outcome`/`set_last_outcome`/`clear_last_outcome`; `src/automation/state.rs` must already have `completed_iterations` on `AutomationContext` (initialized to `0` in `new`, incremented in the `Capturing` arm after the screenshot is saved); and `src/gui/state.rs` must already have the `Completed`/`Aborted`/`Error` variants shaped as above. If any of these are missing, implement them first (they are small) — but in the normal case they already exist and you build directly on them.

Terms used in this plan, defined plainly: a "session folder" is one directory under `output/` named with a timestamp like `20260606_141500` that holds one series' `screenshots/`, `results.csv`, `session.log`, and (after this change) `run-meta.json`. "Resume"/"continue" means run the remaining iterations of an interrupted series into its existing session folder. "Captured" means a screenshot was saved to disk for that run (the unit we count as completed).

## Plan of Work

The work is four milestones. Each compiles cleanly with `cargo check` and is independently verifiable. M1 adds persistence with no behavior change. M2 makes the engine able to start partway through and writes metadata. M3 exposes one-click continue for the run you just watched fail. M4 adds disk discovery so resume survives a restart.

### Milestone M1 — Persisted session metadata

Goal: a new module that writes and reads `run-meta.json` and can list interrupted sessions on disk. Nothing calls it yet, so behavior is unchanged; success is `cargo check` plus a tiny manual read/write check.

Create `src/automation/session_meta.rs` with this content:

    //! Persistent per-session run metadata and resumable-session discovery.
    //!
    //! Each automation run writes `run-meta.json` into its session folder
    //! (e.g. `output/20260606_141500/`). It records the originally requested run
    //! count (`total`), which is otherwise only held in GUI memory, so an
    //! interrupted run can be resumed even after the app restarts.

    use serde::{Deserialize, Serialize};
    use std::path::{Path, PathBuf};

    /// File name written inside each session folder.
    const META_FILENAME: &str = "run-meta.json";

    /// Persisted metadata describing one automation run.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RunMeta {
        /// Total number of runs originally requested.
        pub total: u32,
        /// Number of runs whose result was captured (best-effort snapshot).
        pub completed: u32,
        /// One of: "running", "completed", "aborted", "error".
        pub status: String,
        /// Optional human-readable error/abort detail.
        #[serde(default)]
        pub message: Option<String>,
    }

    /// A session folder that was interrupted before all runs finished.
    #[derive(Debug, Clone)]
    pub struct ResumableSession {
        pub path: PathBuf,
        pub total: u32,
        pub completed: u32,
    }

    /// Writes `run-meta.json` into `session_dir` (overwrites any existing file).
    /// Failures are logged but never panic — metadata is best-effort.
    pub fn write_meta(session_dir: &Path, meta: &RunMeta) {
        let path = session_dir.join(META_FILENAME);
        match serde_json::to_string_pretty(meta) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    crate::log(&format!("Failed to write run-meta.json: {}", e));
                }
            }
            Err(e) => crate::log(&format!("Failed to serialize run-meta: {}", e)),
        }
    }

    /// Reads `run-meta.json` from `session_dir`; None if missing or invalid.
    pub fn read_meta(session_dir: &Path) -> Option<RunMeta> {
        let path = session_dir.join(META_FILENAME);
        let json = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&json).ok()
    }

    /// Counts captured screenshots in `session_dir/screenshots` (files ending
    /// `.png`). This is the crash-proof source of truth for completed runs:
    /// screenshots are saved synchronously in the `Capturing` state before any
    /// asynchronous OCR, so they never lag behind actual progress.
    pub fn count_captured(session_dir: &Path) -> u32 {
        let dir = session_dir.join("screenshots");
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => return 0,
        };
        entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|x| x.eq_ignore_ascii_case("png"))
                    .unwrap_or(false)
            })
            .count() as u32
    }

    /// Scans `output_dir` for interrupted runs that can be resumed.
    ///
    /// A folder qualifies if it has a readable `run-meta.json` and its captured
    /// count (recomputed from screenshots) is below `total`. Folders predating
    /// this feature have no metadata and are skipped. Returned newest-first.
    pub fn list_resumable(output_dir: &Path) -> Vec<ResumableSession> {
        let mut out = Vec::new();
        let entries = match std::fs::read_dir(output_dir) {
            Ok(e) => e,
            Err(_) => return out,
        };
        let mut dirs: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        // Folder names are timestamps (YYYYMMDD_HHMMSS) and sort chronologically.
        dirs.sort();
        dirs.reverse();
        for dir in dirs {
            if let Some(meta) = read_meta(&dir) {
                let completed = count_captured(&dir);
                if completed < meta.total {
                    out.push(ResumableSession { path: dir, total: meta.total, completed });
                }
            }
        }
        out
    }

Then register the module in `src/automation/mod.rs` by adding, alongside the other `pub mod` lines, `pub mod session_meta;`. (serde and serde_json are already dependencies in `Cargo.toml`; no Cargo edits are needed. `crate::log` is the global logger from `src/main.rs`.)

### Milestone M2 — Resume-capable engine + metadata writes

Goal: the state machine can begin at an arbitrary iteration, the runner exposes `resume_automation`, and every run writes `run-meta.json` at start and at finish. After this milestone, calling `resume_automation` programmatically would correctly append runs; the GUI wiring comes in M3/M4.

In `src/automation/state.rs`:

First, add a `start_iteration` field to `AutomationContext`. Find the struct fields `current_iteration`, `completed_iterations`, `max_iterations` and add `start_iteration` immediately after `completed_iterations`:

    /// 1-based iteration this run begins at (1 for fresh, completed+1 for resume)
    pub start_iteration: u32,

Next, change `AutomationContext::new` to accept `start_iteration` and seed the counters. The current signature is `pub fn new(hwnd, config, max_iterations, work_sender, screenshot_dir) -> Self`. Add `start_iteration: u32` as a parameter after `max_iterations`. In the returned struct literal, change `current_iteration: 0,` to remain `current_iteration: 0,`, change `completed_iterations: 0,` to `completed_iterations: start_iteration.saturating_sub(1),`, and add `start_iteration,`. The full literal becomes:

    Self {
        state: AutomationState::Idle,
        hwnd,
        config,
        current_iteration: 0,
        completed_iterations: start_iteration.saturating_sub(1),
        start_iteration,
        max_iterations,
        work_sender,
        start_time: Instant::now(),
        screenshot_dir,
        start_button_ref,
        skip_button_ref,
        end_button_ref,
    }

Then, in `step`, the `AutomationState::Idle` arm currently sets `self.current_iteration = 1;`. Change it to:

    self.current_iteration = self.start_iteration;

Finally, in the `AutomationState::WaitingForStartPage` arm, the End-button retry is gated by `let click_retry = if self.current_iteration > 1 {`. Change the condition to:

    let click_retry = if self.current_iteration > self.start_iteration {

This makes the first iteration of any run (fresh `start_iteration == 1`, or resumed `start_iteration == k`) skip the End-click retry, because at that point the game is sitting on the rehearsal start page and End has not just been clicked. Subsequent iterations still retry as before.

In `src/automation/runner.rs`, replace the single `start_automation` function with a thin public wrapper plus a shared inner function, and add `resume_automation`. The current `start_automation` body (the one that finds the window, creates the timestamped folder, sets up CSV/log, stores counters, logs, and spawns the thread) becomes the body of `start_automation_inner`, parameterized to optionally reuse an existing folder and to start at an arbitrary iteration. Use exactly this:

    /// Starts a fresh automation run in a background thread.
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

        let hwnd_raw = hwnd.0 as usize;

        thread::spawn(move || {
            let hwnd = windows::Win32::Foundation::HWND(hwnd_raw as *mut std::ffi::c_void);
            run_automation_loop(hwnd, config, iterations, start_iteration, screenshot_dir, csv_path);
            AUTOMATION_RUNNING.store(false, Ordering::SeqCst);
            crate::log("Automation thread finished");
        });

        Ok(())
    }

Update `run_automation_loop` to accept `start_iteration` and pass it to the context. Change its signature to add `start_iteration: u32` (after `max_iterations`), and change the `AutomationContext::new(...)` call to include it:

    let mut ctx = AutomationContext::new(
        hwnd, config, max_iterations, start_iteration, sender, screenshot_dir,
    );

In the same function, where the prerequisite work sets `LAST_OUTCOME` after the loop, also write the final metadata. Locate the block that builds `outcome` from `ctx.state` (binding `let completed = ctx.completed_iterations;`) and calls `set_last_outcome(outcome)`. Immediately before `set_last_outcome(outcome);`, derive the persisted status from the outcome and write it (deriving the session folder from `csv_path.parent()`):

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
            },
        );
    }
    set_last_outcome(outcome);

Finally, add `resume_automation` to the re-exports in `src/automation/mod.rs`: change the runner re-export line to `pub use runner::{is_automation_running, request_abort, resume_automation, start_automation};`.

### Milestone M3 — One-click "Continue last run" in the GUI

Goal: after a run ends as `Aborted` or `Error` with runs left, a "続行 (残り N回)" button appears; clicking it resumes the in-memory session.

In `src/gui/state.rs`, add an import near the top: `use crate::automation::session_meta::ResumableSession;`. Add a helper method on `AutomationStatus` (inside its `impl`):

    /// If this terminal state can be resumed (interrupted with runs remaining and
    /// a known session folder), returns (completed, total, session_path).
    pub fn resumable(&self) -> Option<(u32, u32, std::path::PathBuf)> {
        match self {
            Self::Aborted { completed, total, session_path: Some(p) }
            | Self::Error { completed, total, session_path: Some(p), .. }
                if *completed < *total =>
            {
                Some((*completed, *total, p.clone()))
            }
            _ => None,
        }
    }

Also add two fields to `struct GuiState` (for the M4 picker; harmless to add now): `resumable_sessions: Vec<ResumableSession>,` and `selected_resume: Option<usize>,`. In its `Default` impl, initialize them to `Vec::new()` and `None`.

In `src/gui/render.rs`, change `render_controls` to also report a continue click. Change its return type to `(bool, bool, bool)` and its body to track a third flag and render the button when resumable and not running. After the existing Start/Stop `ui.horizontal(...)` block, add:

    let mut continue_clicked = false;
    if let Some((completed, total, _)) = state.status.resumable() {
        let remaining = total.saturating_sub(completed);
        ui.add_space(8.0);
        ui.add_enabled_ui(!state.status.is_running(), |ui| {
            if ui
                .button(RichText::new(format!("⏵ 続行 (残り {}回)", remaining)).size(16.0))
                .clicked()
            {
                continue_clicked = true;
            }
        });
    }

and change the final `(start_clicked, stop_clicked)` to `(start_clicked, stop_clicked, continue_clicked)`.

In `src/gui/mod.rs`, update the import to include `resume_automation`: change `use crate::automation::runner::{get_last_outcome, is_automation_running, start_automation, AutomationOutcome};` to also import `resume_automation`. Add a handler method on `GuiApp`:

    fn handle_continue(&mut self) {
        if let Some((completed, total, session_path)) = self.state.status.resumable() {
            match resume_automation(session_path.clone(), completed, total) {
                Ok(()) => {
                    self.state.latest_session_path =
                        crate::automation::runner::get_current_session_path();
                    self.state.status = AutomationStatus::Running {
                        current: completed,
                        total,
                        state_description: "再開中...".to_string(),
                        start_time: std::time::Instant::now(),
                    };
                    self.state.automation_start_time = Some(std::time::Instant::now());
                    crate::log(&format!("GUI: Resuming automation from {}/{}", completed, total));
                }
                Err(e) => {
                    self.state.status = AutomationStatus::Error {
                        completed,
                        total,
                        message: e.to_string(),
                        session_path: Some(session_path),
                    };
                    crate::log(&format!("GUI: Failed to resume automation: {}", e));
                }
            }
        }
    }

Wire it in `update()` where `render_controls` is called. Change the call to destructure three values and handle the new one:

    let (start_clicked, stop_clicked, continue_clicked) =
        render::render_controls(ui, &mut self.state);
    if start_clicked { self.handle_start(); }
    if stop_clicked { self.handle_stop(); }
    if continue_clicked { self.handle_continue(); }

### Milestone M4 — Resume any interrupted session after restart

Goal: a picker lists interrupted sessions found on disk (via M1's `list_resumable`) and resumes the chosen one, so resume works even after the app was closed and reopened.

In `src/gui/render.rs`, add a new render function for the picker. It reads `state.resumable_sessions`, lets the user select one (storing the index in `state.selected_resume`), and reports refresh/resume clicks:

    /// Render the "resume a previous session" picker.
    /// Returns (refresh_clicked, resume_clicked).
    pub fn render_resume_picker(ui: &mut egui::Ui, state: &mut GuiState) -> (bool, bool) {
        let mut refresh_clicked = false;
        let mut resume_clicked = false;
        let is_running = state.status.is_running();

        ui.add_space(16.0);
        ui.heading("中断したセッションを再開");
        ui.add_space(4.0);
        ui.label(
            RichText::new("ゲームをリハーサル開始画面に戻してから再開してください")
                .small(),
        );
        ui.add_space(4.0);

        if ui.button("🔄 更新").clicked() {
            refresh_clicked = true;
        }

        if state.resumable_sessions.is_empty() {
            ui.label(RichText::new("再開可能なセッションはありません").weak());
            return (refresh_clicked, resume_clicked);
        }

        let selected_label = state
            .selected_resume
            .and_then(|i| state.resumable_sessions.get(i))
            .map(|s| {
                let name = s.path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
                format!("{} — {}/{}", name, s.completed, s.total)
            })
            .unwrap_or_else(|| "選択してください".to_string());

        egui::ComboBox::from_id_source("resume_session_combo")
            .selected_text(selected_label)
            .show_ui(ui, |ui| {
                for (i, s) in state.resumable_sessions.iter().enumerate() {
                    let name = s.path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
                    let label = format!("{} — {}/{}", name, s.completed, s.total);
                    ui.selectable_value(&mut state.selected_resume, Some(i), label);
                }
            });

        ui.add_space(4.0);
        ui.add_enabled_ui(!is_running && state.selected_resume.is_some(), |ui| {
            if ui.button(RichText::new("▶ 選択を再開").size(16.0)).clicked() {
                resume_clicked = true;
            }
        });

        (refresh_clicked, resume_clicked)
    }

In `src/gui/mod.rs`, add a scan helper and a selected-resume handler on `GuiApp`:

    fn scan_resumable_sessions(&mut self) {
        let dir = crate::paths::get_output_dir();
        self.state.resumable_sessions =
            crate::automation::session_meta::list_resumable(&dir);
        // Keep selection valid; default to the newest when none chosen.
        match self.state.selected_resume {
            Some(i) if i >= self.state.resumable_sessions.len() => {
                self.state.selected_resume = None;
            }
            _ => {}
        }
        if self.state.selected_resume.is_none() && !self.state.resumable_sessions.is_empty() {
            self.state.selected_resume = Some(0);
        }
    }

    fn handle_resume_selected(&mut self) {
        let chosen = self
            .state
            .selected_resume
            .and_then(|i| self.state.resumable_sessions.get(i).cloned());
        if let Some(s) = chosen {
            match resume_automation(s.path.clone(), s.completed, s.total) {
                Ok(()) => {
                    self.state.latest_session_path =
                        crate::automation::runner::get_current_session_path();
                    self.state.status = AutomationStatus::Running {
                        current: s.completed,
                        total: s.total,
                        state_description: "再開中...".to_string(),
                        start_time: std::time::Instant::now(),
                    };
                    self.state.automation_start_time = Some(std::time::Instant::now());
                    crate::log(&format!(
                        "GUI: Resuming session {} from {}/{}",
                        s.path.display(), s.completed, s.total
                    ));
                }
                Err(e) => {
                    crate::log(&format!("GUI: Failed to resume selected session: {}", e));
                    // Refresh the list in case the folder vanished.
                    self.scan_resumable_sessions();
                }
            }
        }
    }

Populate the list when the app starts: in `GuiApp::new`, after constructing the struct, scan once before returning it. Change the tail of `new` from `Self { ... }` to:

    let mut app = Self { ... };
    app.scan_resumable_sessions();
    app

Refresh the list when a run finishes: in `update_automation_status`, in the branch that transitions a `Running` status to a terminal one (right after assigning `self.state.status = self.finalize_status(...)`), add `self.scan_resumable_sessions();` so the just-interrupted (or completed) session immediately appears/disappears in the picker.

Render and wire the picker in `update()`, in column three after `render::render_actions(...)`:

    let (refresh_clicked, resume_selected_clicked) =
        render::render_resume_picker(ui, &mut self.state);
    if refresh_clicked { self.scan_resumable_sessions(); }
    if resume_selected_clicked { self.handle_resume_selected(); }

## Concrete Steps

Run all commands from the repository root `C:\Work\GitRepos\gakumas-screenshot` in PowerShell.

1. Implement M1, then compile-check:

    cargo check

   Expected: it finishes with `Finished` and no errors (pre-existing warnings about unused OCR items are fine). If `session_meta` is reported as an unknown module, confirm you added `pub mod session_meta;` to `src/automation/mod.rs`.

2. Implement M2, then:

    cargo check

   Expected: no errors. Common mistakes: forgetting to pass `start_iteration` to `AutomationContext::new` (mismatched-arguments error pointing at `src/automation/runner.rs`), or leaving the old `self.current_iteration = 1;` in the `Idle` arm.

3. Implement M3, then:

    cargo check

   Expected: no errors. If you see "expected 2 elements, found 3" at the `render_controls` call site, you missed updating the destructuring in `update()`.

4. Implement M4, then build a runnable binary:

    cargo build --release

   Expected: `Finished release` with no errors. The binary is `target\release\gakumas-screenshot.exe`.

## Validation and Acceptance

Because the executable requires administrator elevation, automated `cargo test` cannot run; acceptance is the following manual scenarios. Launch the built app (it must run elevated if the game runs elevated):

    .\target\release\gakumas-screenshot.exe

Scenario A — In-session continue (M2+M3). With the game on the rehearsal start page, set 実行回数 to 5 and press 開始. After 2 runs complete, press the abort hotkey Ctrl+Shift+Q. Observe the progress line read "中断 (2/5回 完了)" in amber and a "⏵ 続行 (残り 3回)" button appear. Put the game back on the rehearsal start page and click the button. The app performs runs 3–5. When done, the status reads "完了 (5/5回) → <folder>". Open the session folder (📁 フォルダを開く) and confirm: `screenshots/` contains files numbered `001`–`005`; `results.csv` has exactly one header line plus five data rows (no duplicate header); `run-meta.json` contains `"total": 5`, `"completed": 5`, `"status": "completed"`. This proves resume appends into the same folder with continuous numbering.

    Expected results.csv (abbreviated):
    iteration,timestamp,screenshot,s1c1,...,s3c3
    1,...,...,...
    2,...,...,...
    3,...,...,...
    4,...,...,...
    5,...,...,...

Scenario B — Restart survival (M1+M4). Repeat Scenario A but, after aborting at 2/5, fully close the app instead of clicking 続行. Reopen the app. In the "中断したセッションを再開" section, click 🔄 更新, confirm the interrupted session appears in the dropdown labeled like "20260606_141500 — 2/5", select it, ensure the game is on the rehearsal start page, and click "▶ 選択を再開". The app performs runs 3–5 into that same folder; verify the same end state as Scenario A. This proves resume works across restarts using only on-disk metadata.

Scenario C — Timeout path (engine + reporting). Start a series, then make the game unresponsive (e.g., switch the game away from the rehearsal flow) so a wait times out. Confirm the status becomes "エラー (k/N回 完了): … Timeout …" in red with a partial progress bar (not 100%), and that 続行 appears. This confirms the timeout no longer reports false completion and is resumable.

Scenario D — Negative. With no interrupted sessions present (or all complete), confirm the picker shows "再開可能なセッションはありません" and no 続行 button is shown while idle.

## Idempotence and Recovery

All steps are safe to repeat. `cargo check`/`cargo build` are idempotent. The metadata writer overwrites `run-meta.json` wholesale, so re-running a session simply refreshes it. `init_csv` never clobbers existing rows, and screenshot/log writes are append-only, so resuming the same folder multiple times only ever adds data. `count_captured` reading the screenshots directory makes `completed` self-correcting even if a previous run crashed before writing final metadata (status would remain "running", but the folder still lists as resumable with the correct captured count). If a chosen session folder was deleted between scan and resume, `resume_automation` returns an error, which the GUI logs and then refreshes the list.

To return to a clean tree during development, you can delete test session folders under `output/` (each is self-contained). Do not delete folders you want to keep — there is no undo.

## Artifacts and Notes

Representative `run-meta.json` after a completed resume:

    {
      "total": 5,
      "completed": 5,
      "status": "completed",
      "message": null
    }

Representative log lines distinguishing a fresh start from a resume (in `session.log` and the global log):

    [14:15:00.123] Starting automation: 5 iterations (Ctrl+Shift+Q to abort)
    ...
    [14:20:30.456] Resuming automation from iteration 3/5 (Ctrl+Shift+Q to abort)

## Interfaces and Dependencies

Use the existing crates already in `Cargo.toml`: `serde` + `serde_json` for metadata, `eframe`/`egui` for UI, `anyhow` for errors. No new dependencies.

In `src/automation/session_meta.rs`, the following must exist at the end of M1:

    pub struct RunMeta { pub total: u32, pub completed: u32, pub status: String, pub message: Option<String> }
    pub struct ResumableSession { pub path: std::path::PathBuf, pub total: u32, pub completed: u32 }
    pub fn write_meta(session_dir: &std::path::Path, meta: &RunMeta);
    pub fn read_meta(session_dir: &std::path::Path) -> Option<RunMeta>;
    pub fn count_captured(session_dir: &std::path::Path) -> u32;
    pub fn list_resumable(output_dir: &std::path::Path) -> Vec<ResumableSession>;

In `src/automation/runner.rs`, the following must exist at the end of M2:

    pub fn start_automation(max_iterations: Option<u32>) -> anyhow::Result<()>;
    pub fn resume_automation(session_dir: std::path::PathBuf, completed: u32, total: u32) -> anyhow::Result<()>;
    fn start_automation_inner(iterations: u32, start_iteration: u32, existing_session: Option<std::path::PathBuf>) -> anyhow::Result<()>;
    fn run_automation_loop(hwnd: HWND, config: AutomationConfig, max_iterations: u32, start_iteration: u32, screenshot_dir: PathBuf, csv_path: PathBuf);

In `src/automation/state.rs`, `AutomationContext` must gain `pub start_iteration: u32`, and `AutomationContext::new` must take `start_iteration: u32` after `max_iterations`.

In `src/gui/state.rs`, `AutomationStatus` must gain `pub fn resumable(&self) -> Option<(u32, u32, std::path::PathBuf)>`, and `GuiState` must gain `resumable_sessions: Vec<ResumableSession>` and `selected_resume: Option<usize>`.

In `src/gui/render.rs`, `render_controls` must return `(bool, bool, bool)` and a new `pub fn render_resume_picker(ui: &mut egui::Ui, state: &mut GuiState) -> (bool, bool)` must exist.

In `src/gui/mod.rs`, `GuiApp` must gain `fn handle_continue(&mut self)`, `fn handle_resume_selected(&mut self)`, and `fn scan_resumable_sessions(&mut self)`, and must import `resume_automation`.

## Revision Notes

- 2026-06-06: Initial ExecPlan authored from the approved plan `pure-swinging-moore.md`. Scope: restart-survivable resume via persisted `run-meta.json` and a GUI picker, plus an in-session one-click continue. Built atop the already-present interrupted-run reporting work (`AutomationOutcome`, `completed_iterations`, updated `AutomationStatus`); the prerequisite is documented in Context so the plan remains self-contained. Reason for the design: the requested run count is not otherwise recoverable from disk, so it must be persisted to enable resume after a restart.
