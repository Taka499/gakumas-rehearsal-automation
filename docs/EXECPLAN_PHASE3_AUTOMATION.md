# Phase 3: Asynchronous Automation Loop with Event Queue

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. This document must be maintained in accordance with `docs/PLANS.md`.


## Purpose / Big Picture

After this change, the user can run automated rehearsal data collection by pressing a hotkey. The automation clicks through the game UI (Start → wait for loading → Skip), captures the result screen, and repeats N times. Screenshots are processed by a separate OCR worker thread, which extracts scores and appends them to a CSV file. The automation thread is never blocked by slow OCR processing.

This decoupled architecture means:
- Automation runs at maximum speed (limited only by game loading)
- OCR processes screenshots from a queue, writing results as they complete
- If automation finishes before OCR, the queue drains naturally
- CSV file accumulates results and survives crashes
- Chart generation (Phase 4) reads the CSV independently


## Progress

- [x] Milestone 1: Event queue infrastructure
- [x] Milestone 2: Automation state machine
- [x] Milestone 3: OCR worker thread
- [x] Milestone 4: CSV writer
- [x] Milestone 5: Hotkey integration and abort mechanism
- [x] Milestone 5b: Two-phase loading detection (histogram + brightness)
- [ ] Milestone 5c: Click "終了" button to return to rehearsal page
- [ ] Milestone 6: End-to-end testing


## Surprises & Discoveries

- HWND (Windows handle) contains a raw pointer that is not `Send`-safe, requiring extraction of the raw value as `usize` and reconstruction in the spawned thread. This is safe because Windows handles are valid across threads.
- The `work_sender` field in AutomationContext needed to be moved when dropped to signal the OCR worker to finish, requiring careful ownership handling.
- **Two-phase loading detection required**: Simple brightness detection was insufficient because:
  1. After clicking Start, the brightness check would fire immediately on the old screen (before page change)
  2. The Skip button is visible but dimmed during loading, then becomes enabled when ready
  3. Solution: Phase 1 uses histogram comparison to detect Skip button appearance, Phase 2 uses brightness to detect Skip button enabled state
- Abort check needed inside `wait_for_loading` loop, not just at state machine step boundaries, otherwise Ctrl+Shift+Q wouldn't work during the loading wait.


## Decision Log

- Decision: Use std::sync::mpsc channel for event queue
  Rationale: Standard library, no additional dependencies, sufficient for single-producer single-consumer pattern.
  Date/Author: 2026-01-13 / Initial design

- Decision: Store screenshot file path in queue, not raw image data
  Rationale: Reduces memory pressure. Screenshots are already saved to disk. If OCR fails, the file remains for manual retry. Also enables crash recovery.
  Date/Author: 2026-01-13 / Initial design

- Decision: CSV as intermediate format, not database
  Rationale: Simple, human-readable, easy to import into Excel/Python. Append-only is crash-safe. No additional dependencies.
  Date/Author: 2026-01-13 / Initial design

- Decision: Automation and OCR threads communicate via queue only, no shared mutable state
  Rationale: Simplifies reasoning about concurrency. No locks needed except channel internals.
  Date/Author: 2026-01-13 / Initial design

- Decision: Automation runs on a separate thread, not blocking main message loop
  Rationale: Main thread must continue processing Windows messages (tray icon, hotkeys). Automation is CPU-bound during waits and would freeze the UI if on main thread.
  Date/Author: 2026-01-13 / Clarification

- Decision: Use Ctrl+Shift+Q for abort instead of Escape
  Rationale: Escape as a global hotkey interferes with normal application use (closing dialogs, etc.). Ctrl+Shift+Q is less likely to conflict and follows the pattern of other hotkeys in the app.
  Date/Author: 2026-01-13 / Clarification

- Decision: Re-focus game window before each click action
  Rationale: User might click elsewhere during automation. Each click operation should call SetForegroundWindow to ensure game receives input. Small delay (50ms) after focus before click.
  Date/Author: 2026-01-13 / Clarification

- Decision: Abort automation if game window disappears
  Rationale: If game crashes or is closed, continuing automation is pointless and could cause errors. Check window validity before each state transition.
  Date/Author: 2026-01-13 / Clarification

- Decision: Two-phase loading detection using histogram comparison + brightness threshold
  Rationale: Single-phase brightness detection failed because it would detect the previous screen's brightness before the page changed. The Skip button appears immediately but dimmed (disabled) during loading, then becomes enabled (brighter) when ready. Phase 1 uses histogram comparison against a reference image to detect button appearance. Phase 2 uses brightness threshold to detect button enabled state.
  Date/Author: 2026-01-14 / Implementation fix

- Decision: Reference image captured via tray menu "Capture Skip Reference"
  Rationale: Allows user to calibrate the reference for their specific game resolution and settings. Falls back to brightness-only detection if reference doesn't exist.
  Date/Author: 2026-01-14 / Implementation


## Outcomes & Retrospective

(To be filled upon completion)


## Context and Orientation

This plan depends on:
- Calibration Tool (`docs/EXECPLAN_CALIBRATION_TOOL.md`) - provides button and score region positions
- Phase 2 OCR (`docs/EXECPLAN_PHASE2_OCR.md`) - provides `ocr_all_scores()` function

The automation flow follows this sequence for each iteration:

    1. Click "開始する" (Start) button
    2. Wait for Skip button to appear (histogram comparison with reference image)
    3. Wait for Skip button to become enabled (brightness threshold)
    4. Click "スキップ" (Skip) button
    5. Wait for result screen to stabilize (fixed delay)
    6. Capture screenshot
    7. Push screenshot path to OCR queue
    8. [NOT YET IMPLEMENTED] Click "終了" button to return to rehearsal page
    9. Repeat from step 1

NOTE: Step 8 is not yet implemented. Currently the automation expects the game to
automatically return to the rehearsal page after the result screen.

The OCR worker runs in parallel:

    1. Pop screenshot path from queue (blocking)
    2. Load image from disk
    3. Run OCR to extract 9 scores (3 stages × 3 breakdown scores)
    4. Append row to CSV file
    5. Repeat

Key existing files:

    src/automation/input.rs        - click_at_relative() for mouse input
    src/automation/detection.rs    - measure_region_brightness() for loading detection
    src/automation/config.rs       - AutomationConfig with button positions and thresholds
    src/capture/screenshot.rs      - capture_gakumas_to_buffer() for window capture
    src/ocr/mod.rs                 - ocr_screenshot() extracts 9 scores from image

Terms used in this document:

- Event queue: A thread-safe FIFO (first-in, first-out) data structure for passing work items between threads. Implemented with `std::sync::mpsc::channel`.
- Producer: The automation thread that creates work items (screenshot paths).
- Consumer: The OCR worker thread that processes work items.
- State machine: A programming pattern where behavior depends on current state, and inputs cause transitions between states.
- Backpressure: When the consumer is slower than the producer, the queue grows. We may optionally limit queue size to apply backpressure.

Threading model:

    Main Thread (message loop)
        │
        ├─► Handles hotkeys, tray icon, Windows messages
        │
        └─► On Ctrl+Shift+A: spawns Automation Thread
                │
                ├─► Runs state machine loop
                │
                └─► Spawns OCR Worker Thread
                        │
                        └─► Processes screenshots from queue

All three threads run concurrently. Main thread never blocks.


## Plan of Work

### Milestone 1: Event Queue Infrastructure

Create `src/automation/queue.rs` to manage the screenshot processing queue:

    use std::sync::mpsc::{channel, Sender, Receiver};
    use std::path::PathBuf;

    /// A work item for the OCR worker.
    pub struct OcrWorkItem {
        /// Path to the screenshot file
        pub screenshot_path: PathBuf,
        /// Iteration number (1-based)
        pub iteration: u32,
        /// Timestamp when screenshot was captured
        pub captured_at: DateTime<Local>,
    }

    /// Creates a new work queue. Returns (sender for automation, receiver for OCR worker).
    pub fn create_work_queue() -> (Sender<OcrWorkItem>, Receiver<OcrWorkItem>);

    /// Signal sent when automation is complete and no more items will be added.
    pub struct EndOfWork;

The channel will be unbounded (no limit) initially. If memory becomes a concern, we can add bounded channels with backpressure.


### Milestone 2: Automation State Machine

Create `src/automation/state.rs` with the state machine:

    pub enum AutomationState {
        /// Waiting to start (initial state)
        Idle,
        /// Clicking the Start button
        ClickingStart,
        /// Waiting for loading to complete
        WaitingForLoading,
        /// Clicking the Skip button
        ClickingSkip,
        /// Waiting for result screen to stabilize
        WaitingForResult,
        /// Capturing the result screenshot
        Capturing,
        /// Checking if we should continue or stop
        CheckingLoop,
        /// All iterations complete
        Complete,
        /// Error occurred
        Error(String),
        /// User requested abort
        Aborted,
    }

    pub struct AutomationContext {
        pub state: AutomationState,
        pub hwnd: HWND,
        pub config: AutomationConfig,
        pub current_iteration: u32,
        pub max_iterations: u32,
        pub work_sender: Sender<OcrWorkItem>,
        pub start_time: Instant,
    }

    impl AutomationContext {
        /// Advances the state machine by one step.
        /// Returns true if automation should continue, false if complete/error/aborted.
        pub fn step(&mut self) -> Result<bool>;
    }

State transitions:

    Idle → ClickingStart (on start)
    ClickingStart → WaitingForLoading (after click)
    WaitingForLoading → ClickingSkip (when loading complete)
    WaitingForLoading → Error (on timeout)
    ClickingSkip → WaitingForResult (after click)
    WaitingForResult → Capturing (after delay)
    Capturing → CheckingLoop (after capture + queue push)
    CheckingLoop → ClickingStart (if iteration < max)
    CheckingLoop → Complete (if iteration >= max)
    Any → Aborted (on abort signal)
    Any → Error (if game window invalid)

Window validation (checked before each state transition):

    fn is_window_valid(hwnd: HWND) -> bool {
        unsafe { IsWindow(hwnd).as_bool() }
    }

Re-focus before click:

    fn click_with_focus(hwnd: HWND, rel_x: f32, rel_y: f32) -> Result<()> {
        if !is_window_valid(hwnd) {
            return Err(anyhow!("Game window no longer exists"));
        }
        unsafe { SetForegroundWindow(hwnd); }
        std::thread::sleep(Duration::from_millis(50));
        click_at_relative(hwnd, rel_x, rel_y)
    }


### Milestone 3: OCR Worker Thread

Create `src/automation/ocr_worker.rs`:

    use std::sync::mpsc::Receiver;
    use std::path::PathBuf;

    /// Runs the OCR worker loop. Processes items until channel is closed.
    /// Writes results to the CSV file.
    pub fn run_ocr_worker(
        receiver: Receiver<OcrWorkItem>,
        csv_path: PathBuf,
        config: AutomationConfig,
    );

The worker loop:

    fn run_ocr_worker(...) {
        loop {
            match receiver.recv() {
                Ok(work_item) => {
                    // Load screenshot
                    let img = image::open(&work_item.screenshot_path)?.to_rgba8();

                    // Run OCR (this is the slow part)
                    let scores = ocr::ocr_screenshot(&img, config.ocr_threshold)?;

                    // Append to CSV
                    append_to_csv(&csv_path, &work_item, &scores)?;

                    log(&format!(
                        "OCR complete for iteration {}: {:?}",
                        work_item.iteration, scores
                    ));
                }
                Err(_) => {
                    // Channel closed, automation complete
                    log("OCR worker: channel closed, exiting");
                    break;
                }
            }
        }
    }


### Milestone 4: CSV Writer

Create `src/automation/csv_writer.rs`:

    use std::path::Path;
    use std::fs::OpenOptions;
    use std::io::Write;

    /// CSV header row (9 breakdown scores: 3 stages × 3 criteria)
    const CSV_HEADER: &str = "iteration,timestamp,screenshot,\
        s1c1,s1c2,s1c3,\
        s2c1,s2c2,s2c3,\
        s3c1,s3c2,s3c3";

    /// Initializes CSV file with header if it doesn't exist.
    pub fn init_csv(path: &Path) -> Result<()>;

    /// Appends one result row to the CSV file.
    pub fn append_to_csv(
        path: &Path,
        work_item: &OcrWorkItem,
        scores: &[[u32; 3]; 3],
    ) -> Result<()>;

CSV format:

    iteration,timestamp,screenshot,s1c1,s1c2,s1c3,s2c1,s2c2,s2c3,s3c1,s3c2,s3c3
    1,2026-01-13T14:30:00,screenshots/001.png,50339,50796,70859,64997,168009,128450,122130,105901,96776
    2,2026-01-13T14:30:45,screenshots/002.png,48000,52000,71000,65000,170000,130000,120000,106000,97000

The CSV file is opened in append mode for each write, ensuring crash safety.


### Milestone 5: Hotkey Integration, Abort Mechanism, and Progress Feedback

Integrate automation into the main application:

1. Add new hotkey: Ctrl+Shift+A to start automation
2. Add abort hotkey: Ctrl+Shift+Q to stop automation (not Escape, which interferes with other apps)
3. Add iteration count configuration to config.json
4. Update tray icon tooltip to show progress

In `src/main.rs`, add:

    const HOTKEY_AUTOMATION: i32 = 6;  // Ctrl+Shift+A
    const HOTKEY_ABORT: i32 = 7;       // Ctrl+Shift+Q

    // Register hotkeys
    RegisterHotKey(hwnd, HOTKEY_AUTOMATION, MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT, 0x41)?;  // A
    RegisterHotKey(hwnd, HOTKEY_ABORT, MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT, 0x51)?;      // Q

    // In window_proc
    WM_HOTKEY if wparam.0 as i32 == HOTKEY_AUTOMATION => {
        if !AUTOMATION_RUNNING.load(Ordering::SeqCst) {
            start_automation();
        } else {
            log("Automation already running");
        }
    }

    WM_HOTKEY if wparam.0 as i32 == HOTKEY_ABORT => {
        if AUTOMATION_RUNNING.load(Ordering::SeqCst) {
            request_abort();
            log("Abort requested");
        }
    }

Abort mechanism uses an atomic flag:

    static ABORT_REQUESTED: AtomicBool = AtomicBool::new(false);

    // In state machine step()
    if ABORT_REQUESTED.load(Ordering::SeqCst) {
        self.state = AutomationState::Aborted;
        return Ok(false);
    }

Progress feedback via tray icon tooltip:

    fn update_progress(iteration: u32, max: u32, state: &str) {
        let tooltip = format!(
            "Gakumas: {}/{} - {}",
            iteration, max, state
        );
        update_tray_tooltip(&tooltip);
    }

    // Called from state machine:
    // "Gakumas: 3/10 - Waiting for loading..."
    // "Gakumas: 3/10 - Capturing result"
    // "Gakumas: Complete (10 iterations)"


### Milestone 6: End-to-End Testing

Create integration tests that:

1. Mock the game window (or use sample images)
2. Run automation for 3 iterations
3. Verify CSV contains 3 rows with expected format
4. Verify screenshots were saved
5. Test abort mechanism

Manual testing checklist:
- [ ] Start automation with game at rehearsal page
- [ ] Verify Start button is clicked
- [ ] Verify loading detection works
- [ ] Verify Skip button is clicked
- [ ] Verify screenshot is captured
- [ ] Verify OCR processes in background
- [ ] Verify CSV accumulates results
- [ ] Test abort with Ctrl+Shift+Q
- [ ] Test running 10+ iterations for stability
- [ ] Verify tray tooltip shows progress
- [ ] Test closing game window during automation (should abort gracefully)


## Concrete Steps

All commands run from repository root: `C:\Work\GitRepos\gakumas-rehearsal-automation`


### Step 1: Create automation module files

Create these new files:

    src/automation/queue.rs
    src/automation/state.rs
    src/automation/ocr_worker.rs
    src/automation/csv_writer.rs
    src/automation/runner.rs      # Main automation entry point

Update `src/automation/mod.rs` to export new modules.

Verify:

    cargo build --release

Expected: Build succeeds.


### Step 2: Implement event queue

Implement `src/automation/queue.rs`.

Verify with unit test:

    cargo test automation::queue::tests -- --nocapture

Expected: Test sends and receives work items correctly.


### Step 3: Implement state machine

Implement `src/automation/state.rs`.

Verify state transitions:

    cargo test automation::state::tests -- --nocapture

Expected: State machine transitions correctly for all paths.


### Step 4: Implement CSV writer

Implement `src/automation/csv_writer.rs`.

Verify:

    cargo test automation::csv_writer::tests -- --nocapture

Expected: Creates CSV with correct header and appends rows.


### Step 5: Implement OCR worker

Implement `src/automation/ocr_worker.rs`.

This integrates with Phase 2 OCR module.


### Step 6: Implement automation runner

Implement `src/automation/runner.rs` that ties everything together:

    pub fn start_automation(max_iterations: u32) -> Result<()> {
        // 1. Find game window
        // 2. Create work queue
        // 3. Spawn OCR worker thread
        // 4. Run state machine loop
        // 5. Close channel when done
        // 6. Wait for OCR worker to finish
    }


### Step 7: Integrate hotkeys

Update `src/main.rs`:

1. Register Ctrl+Shift+A hotkey
2. Add handler that calls `start_automation()`
3. Add Escape handler for abort

Verify:

    cargo build --release
    .\target\release\gakumas-rehearsal-automation.exe

Press Ctrl+Shift+A (with game running at rehearsal page).

Expected: Console shows automation progress:

    [14:30:00.000] Starting automation: 10 iterations (Ctrl+Shift+Q to abort)
    [14:30:00.100] Iteration 1/10: Clicking Start button
    [14:30:00.200] Iteration 1/10: Waiting for loading...
    [14:30:15.000] Iteration 1/10: Loading complete, clicking Skip
    [14:30:15.500] Iteration 1/10: Capturing result
    [14:30:15.600] Iteration 1/10: Screenshot saved, queued for OCR
    [14:30:15.700] Iteration 2/10: Clicking Start button
    ...
    [14:30:20.000] OCR complete for iteration 1: [[50339, 50796, 70859], ...]

Tray tooltip during automation: "Gakumas: 2/10 - Waiting for loading..."


### Step 8: End-to-end testing

Run full automation test with the game:

1. Start game, navigate to rehearsal page
2. Run `.\target\release\gakumas-rehearsal-automation.exe`
3. Press Ctrl+Shift+A
4. Observe automation running
5. After completion, check `results.csv`

Verify CSV contains correct data:

    type results.csv

Expected: Header row plus N data rows with scores.


## Validation and Acceptance

The automation loop is complete when:

1. Pressing Ctrl+Shift+A starts automation (if game window found)

2. Automation correctly sequences: Start → Wait → Skip → Capture → Repeat

3. Screenshots are saved to `screenshots/` directory with sequential names

4. OCR worker processes screenshots without blocking automation

5. CSV file accumulates results with all 9 score columns

6. Pressing Ctrl+Shift+Q during automation stops it gracefully

7. After automation completes, OCR worker finishes processing remaining queue items

8. Running 10 iterations completes without crashes or memory leaks

9. Log file shows clear progress for debugging

10. Tray icon tooltip updates to show current progress during automation

11. Automation aborts gracefully if game window is closed during run


## Idempotence and Recovery

- Starting automation when already running is ignored (with log message)
- CSV file is append-only; rerunning adds new rows without destroying old data
- If automation is aborted, partial results are still saved
- If OCR fails for one screenshot, error is logged but worker continues with next
- Screenshots are kept even after OCR, allowing manual retry of failed OCR


## Artifacts and Notes

### Expected Directory Structure After Automation

    gakumas-rehearsal-automation/
    ├── gakumas-rehearsal-automation.exe
    ├── config.json
    ├── gakumas_screenshot.log
    ├── results.csv                 # OCR results
    └── screenshots/
        ├── 001_20260113_143000.png
        ├── 002_20260113_143045.png
        └── ...


### Config Additions

Add to `config.json` (flat structure, consistent with existing config):

    {
      "automation_iterations": 10,
      "result_delay_ms": 500,
      "screenshot_dir": "screenshots",
      "csv_path": "results.csv"
    }


### Thread Architecture Diagram

    ┌─────────────────────────────────────────────────────────────────┐
    │                         Main Thread                              │
    │  (Message loop, hotkey handling, tray icon)                     │
    └─────────────────────────────────────────────────────────────────┘
                │
                │ Ctrl+Shift+A pressed
                ▼
    ┌─────────────────────────────────────────────────────────────────┐
    │                     Automation Thread                            │
    │                                                                  │
    │  ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐  │
    │  │  Click   │───▶│   Wait   │───▶│  Click   │───▶│ Capture  │  │
    │  │  Start   │    │ Loading  │    │   Skip   │    │  Screen  │  │
    │  └──────────┘    └──────────┘    └──────────┘    └────┬─────┘  │
    │       ▲                                               │         │
    │       └───────────────────────────────────────────────┘         │
    │                          (loop N times)               │         │
    └───────────────────────────────────────────────────────┼─────────┘
                                                            │
                                            mpsc::channel   │
                                                            ▼
    ┌─────────────────────────────────────────────────────────────────┐
    │                       OCR Worker Thread                          │
    │                                                                  │
    │  ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐  │
    │  │ Receive  │───▶│   Load   │───▶│   OCR    │───▶│  Write   │  │
    │  │   Item   │    │  Image   │    │  Scores  │    │   CSV    │  │
    │  └──────────┘    └──────────┘    └──────────┘    └──────────┘  │
    │       │                                                         │
    │       └─────────────────── (loop until channel closed) ─────────│
    └─────────────────────────────────────────────────────────────────┘


## Interfaces and Dependencies

### New Files

    src/automation/queue.rs
    src/automation/state.rs
    src/automation/ocr_worker.rs
    src/automation/csv_writer.rs
    src/automation/runner.rs


### Key Types: src/automation/queue.rs

    use std::path::PathBuf;
    use chrono::{DateTime, Local};

    /// Work item for OCR processing
    pub struct OcrWorkItem {
        pub screenshot_path: PathBuf,
        pub iteration: u32,
        pub captured_at: DateTime<Local>,
    }

    /// Creates producer/consumer pair for work queue
    pub fn create_work_queue() -> (Sender<OcrWorkItem>, Receiver<OcrWorkItem>);


### Key Types: src/automation/state.rs

    use windows::Win32::Foundation::HWND;
    use std::sync::mpsc::Sender;

    pub enum AutomationState {
        Idle,
        ClickingStart,
        WaitingForLoading,
        ClickingSkip,
        WaitingForResult,
        Capturing,
        CheckingLoop,
        Complete,
        Error(String),
        Aborted,
    }

    pub struct AutomationContext {
        pub state: AutomationState,
        pub hwnd: HWND,
        pub config: AutomationConfig,
        pub current_iteration: u32,
        pub max_iterations: u32,
        pub work_sender: Sender<OcrWorkItem>,
    }

    impl AutomationContext {
        pub fn new(hwnd: HWND, config: AutomationConfig, max_iterations: u32, sender: Sender<OcrWorkItem>) -> Self;
        pub fn step(&mut self) -> Result<bool>;
    }


### Key Functions: src/automation/csv_writer.rs

    /// Initialize CSV file with header
    pub fn init_csv(path: &Path) -> Result<()>;

    /// Append one result row (scores is [[u32; 3]; 3] from OCR)
    pub fn append_to_csv(path: &Path, item: &OcrWorkItem, scores: &[[u32; 3]; 3]) -> Result<()>;


### Key Functions: src/automation/ocr_worker.rs

    /// Run OCR worker loop until channel closes
    pub fn run_ocr_worker(receiver: Receiver<OcrWorkItem>, csv_path: PathBuf, config: AutomationConfig);


### Key Functions: src/automation/runner.rs

    /// Start the automation loop. Spawns OCR worker thread.
    pub fn start_automation(max_iterations: u32) -> Result<()>;

    /// Request abort of running automation
    pub fn request_abort();

    /// Check if automation is currently running
    pub fn is_automation_running() -> bool;


### Updated src/automation/mod.rs

    pub mod config;
    pub mod detection;
    pub mod input;
    pub mod queue;
    pub mod state;
    pub mod ocr_worker;
    pub mod csv_writer;
    pub mod runner;

    pub use config::{AutomationConfig, ButtonConfig, RelativeRect, get_config, init_config};
    pub use runner::{start_automation, request_abort, is_automation_running};


### Hotkey Constants in src/main.rs

    const HOTKEY_ID: i32 = 1;              // Ctrl+Shift+S - Screenshot
    const HOTKEY_CLICK_TEST: i32 = 2;      // Ctrl+Shift+F9 - Click test
    const HOTKEY_SENDINPUT_TEST: i32 = 3;  // Ctrl+Shift+F10 - SendInput test
    const HOTKEY_RELATIVE_CLICK: i32 = 4;  // Ctrl+Shift+F12 - Relative click test
    const HOTKEY_BRIGHTNESS_TEST: i32 = 5; // Ctrl+Shift+F11 - Brightness test
    const HOTKEY_AUTOMATION: i32 = 6;      // Ctrl+Shift+A - Start automation
    const HOTKEY_ABORT: i32 = 7;           // Ctrl+Shift+Q - Abort automation


---

## Revision History

- 2026-01-13: Initial ExecPlan created
- 2026-01-13: Clarifications added:
  - Threading model diagram and explanation (main/automation/OCR threads)
  - Changed abort hotkey from Escape to Ctrl+Shift+Q (avoid conflicts)
  - Added window re-focus before each click action
  - Added window validity check (abort if game closes)
  - Added tray icon progress feedback
  - Updated Validation section with progress and window-close handling
- 2026-01-14: Updated to align with Phase 2 OCR implementation:
  - Fixed OCR module references (src/ocr/mod.rs, ocr_screenshot())
  - Changed hotkey IDs to 6 and 7 (4 and 5 already used)
  - Simplified CSV to 9 score columns (no stage totals)
  - Changed config to flat structure (automation_iterations, etc.)
