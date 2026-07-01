# Phase 5: GUI Implementation

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. This document must be maintained in accordance with `docs/PLANS.md`.


## Purpose / Big Picture

After this change, the user can interact with the application through a graphical window instead of only through system tray menus and hotkeys. The window displays visual instructions showing how to navigate to the rehearsal page in the game, provides a number input to set how many automation iterations to run, and offers Start/Stop buttons to control the automation. A progress bar shows real-time status during automation. After automation completes, charts are automatically generated. Users can also manually generate charts from existing data or open the output folder.

Each automation session creates its own timestamped output folder (e.g., `output/20260115_143025/`) containing CSV results, JSON statistics, and chart images. This prevents overwriting previous runs and allows users to compare results across sessions.

Developer features like calibration wizard, reference image capture, and OCR testing are hidden by default and only appear when `developer_mode` is enabled in the config file. This keeps the interface simple for regular users while preserving advanced functionality for developers.

The application still runs as a system tray application, but the tray menu is simplified to just "ウィンドウを表示" (Show Window) and "終了" (Exit). The window can be closed to minimize to tray, keeping hotkeys active.


## Progress

- [x] (2026-01-15 14:00) Milestone 1: Add egui/eframe dependencies - Added eframe 0.29 and tray-icon 0.19
- [x] (2026-01-15 14:10) Milestone 2: Create basic GUI window structure - Created src/gui/ module with mod.rs, state.rs, render.rs
- [x] (2026-01-15 14:15) Milestone 3: Implement instruction panel with guide images - Placeholder images embedded, render_instructions() implemented
- [x] (2026-01-15 14:20) Milestone 4: Implement iteration input and control buttons - DragValue, Start/Stop buttons with enable/disable logic
- [x] (2026-01-15 14:25) Milestone 5: Implement progress display - Progress bar, status text, elapsed time, added runner progress tracking
- [x] (2026-01-16 12:00) Milestone 6: Implement session-based output folders - Session folders with timestamps, auto-chart generation on completion
- [x] (2026-01-15 14:30) Milestone 7: Simplify tray menu and integrate GUI - GUI launches by default, tray app available in developer mode
- [x] (2026-01-15 14:35) Milestone 8: Implement developer mode toggle - Added developer_mode field to config, main() checks config flag
- [x] (2026-01-16 00:10) Milestone 9: End-to-end testing - Fixed COM conflict, added Japanese font support, verified automation works


## Surprises & Discoveries

- Observation: eframe uses its own event loop that blocks, making integration with existing Windows tray app complex
  Evidence: eframe::run_native() takes ownership of the main thread
  Resolution: Used conditional startup - GUI mode by default, tray app only when developer_mode=true

- Observation: The tray-icon crate was added but not yet integrated with the GUI
  Evidence: GUI runs standalone without tray icon
  Resolution: Integrated tray-icon with GUI mode using background message-only window for menu events

- Observation: Console window appeared alongside GUI window
  Evidence: Windows subsystem defaulted to console
  Resolution: Added `#![windows_subsystem = "windows"]` to hide console in GUI mode

- Observation: Global hotkeys (Ctrl+Shift+S, Ctrl+Shift+Q) didn't work in GUI mode
  Evidence: Hotkeys were only registered in developer/tray app mode
  Resolution: Created background thread with message-only window to handle hotkeys via RegisterHotKey API

- Observation: COM initialization conflict between Windows Graphics Capture API and eframe/winit
  Evidence: Panic "OleInitialize failed! Result was: RPC_E_CHANGED_MODE" - RoInitialize(multithreaded) conflicts with winit's OleInitialize (single-threaded) for drag-and-drop
  Resolution: Disabled drag-and-drop in eframe with `.with_drag_and_drop(false)` since we don't need it

- Observation: egui's default font doesn't support Japanese characters (CJK)
  Evidence: All Japanese text displayed as boxes (□)
  Resolution: Load Windows system font (Yu Gothic/Meiryo/MS Gothic) at startup via `ctx.set_fonts()`

- Observation: Tesseract OCR spawned visible console windows during processing
  Evidence: Brief terminal window flash every time OCR ran on Windows
  Resolution: Added CREATE_NO_WINDOW flag (0x08000000) to Command::new() via creation_flags()

- Observation: Tesseract failed to create TSV output with "Can't open tsv" error
  Evidence: prepare-tesseract.ps1 only copied eng.traineddata, not tessdata/configs/tsv
  Resolution: Changed from .arg("tsv") config file to .arg("-c").arg("tessedit_create_tsv=1") flag


## Decision Log

- Decision: Use egui with eframe for GUI framework
  Rationale: egui is an immediate-mode GUI library that is simple to use, well-documented, and requires no external dependencies. It integrates well with Rust and produces a reasonable binary size. The "tool application" aesthetic fits a utility like this. Alternative was native-windows-gui but it is more verbose and less flexible.
  Date/Author: 2026-01-15 / Initial design

- Decision: Japanese-only UI
  Rationale: The target game (Gakumas) is Japanese and the primary users are Japanese-speaking. Keeping the UI in Japanese matches user expectations and simplifies implementation. Internationalization can be added later if needed.
  Date/Author: 2026-01-15 / User requirement

- Decision: Session-based output with timestamped folders
  Rationale: Previous implementation overwrote results.csv on each run, losing historical data. Timestamped folders (output/YYYYMMDD_HHMMSS/) preserve all sessions. Users can compare results across runs and never lose data accidentally.
  Date/Author: 2026-01-15 / User requirement

- Decision: Developer mode via config file flag
  Rationale: Hiding calibration and advanced features reduces UI clutter for regular users. Config file is simple to edit for developers. No need for hidden keyboard shortcuts or complex toggle mechanisms.
  Date/Author: 2026-01-15 / User requirement

- Decision: Close button minimizes to tray instead of exiting
  Rationale: Keeps hotkeys active (especially Ctrl+Shift+Q for abort). User must explicitly select "終了" from tray menu to exit. This is standard behavior for tray applications.
  Date/Author: 2026-01-15 / Initial design

- Decision: Embed guide images as resources
  Rationale: Bundling images in the executable ensures they are always available. Users don't need to manage separate image files. Images are placed in `resources/guide/` and embedded using `include_bytes!`.
  Date/Author: 2026-01-15 / Initial design

- Decision: Dual-mode startup (GUI vs Tray) based on developer_mode config
  Rationale: eframe's event loop is incompatible with the existing Windows message loop. Rather than complex thread integration, we use conditional startup: normal users get the GUI, developers get the tray app with all features. This is simpler and maintains both interfaces.
  Date/Author: 2026-01-15 / Implementation

- Decision: Three-column layout for GUI
  Rationale: Guide images are portrait orientation (from mobile game). Two-column layout caused second image to overflow below first. Three columns (image1, image2, controls) allows all content to be visible without scrolling at default window size (800x580).
  Date/Author: 2026-01-16 / Implementation


## Outcomes & Retrospective

### What Went Well
- egui/eframe provided a clean, simple GUI with minimal code
- Three-column layout works well for the portrait guide images + controls
- Session-based output folders prevent data loss between runs
- Auto-chart generation on completion improves user experience
- Tray icon integration allows hotkeys to remain active when window is closed

### What Was Challenging
- COM initialization conflicts required disabling drag-and-drop
- Hotkey handling required a separate background thread with message-only window
- The embedded Tesseract package was missing config files, causing silent OCR failures
- Windows console window flashing required platform-specific Command flags

### Lessons Learned
- When bundling external tools, verify all required config files are included
- Test OCR end-to-end early to catch integration issues
- Windows GUI apps need explicit flags to prevent console window appearance
- egui's immediate mode makes state management simple but requires explicit repaint requests

### Final State
Phase 5 is complete. The application now has a fully functional GUI with:
- Visual guide images with step-by-step instructions
- Iteration count input and Start/Stop controls
- Real-time progress bar and status display
- Session-based output with auto-generated charts
- Tray icon with hotkey support (Ctrl+Shift+S screenshot, Ctrl+Shift+Q abort)
- Developer mode toggle via config file


## Context and Orientation

This plan builds upon Phases 1-4, which implemented:

- Phase 1: Mouse click simulation, region capture, brightness detection
- Phase 2: OCR integration with embedded Tesseract
- Phase 3: Automation loop with state machine, page detection, CSV output
- Phase 4: Statistics calculation and chart generation

The application currently runs as a system tray application. Users interact via right-click context menu and global hotkeys. This works but requires users to memorize hotkeys and navigate menus.

Key existing files relevant to this phase:

    src/main.rs                    - Entry point, tray icon, message loop, hotkey handling
    src/paths.rs                   - Centralized path resolution
    src/automation/runner.rs       - Automation entry point (start_automation function)
    src/automation/state.rs        - AutomationState enum, abort flag
    src/automation/config.rs       - Configuration loading
    src/analysis/mod.rs            - Chart generation entry point (generate_analysis function)
    config.json                    - Application configuration

Current automation flow:
1. User presses Ctrl+Shift+A or selects "Start Automation" from tray menu
2. Automation runs for DEFAULT_ITERATIONS (10) unless modified in code
3. Results saved to results.csv (overwriting previous data)
4. User manually generates charts via tray menu

New flow with GUI:
1. User opens GUI window (double-click tray or "ウィンドウを表示")
2. User sees instructions and sets iteration count
3. User clicks "開始" (Start) button
4. Progress bar shows real-time status
5. On completion, charts auto-generate to session folder
6. User can click "フォルダを開く" to view results

Terms used in this document:

- egui: An immediate-mode GUI library for Rust. "Immediate mode" means the UI is rebuilt every frame, simplifying state management.
- eframe: A framework that provides window creation and event handling for egui applications.
- Session folder: A timestamped directory (e.g., output/20260115_143025/) containing all output from one automation run.
- Tray icon: The small icon in the Windows system notification area (bottom-right of taskbar).
- Developer mode: A configuration flag that, when enabled, shows advanced features like calibration.


## Plan of Work

### Milestone 1: Add egui/eframe Dependencies

Add the egui and eframe crates to Cargo.toml. These provide the GUI framework. Also add the image crate feature for loading embedded PNG images.

In `Cargo.toml`, add under `[dependencies]`:

    eframe = "0.29"

The eframe crate includes egui and provides window management. Version 0.29 is the latest stable release as of January 2026.


### Milestone 2: Create Basic GUI Window Structure

Create a new module `src/gui/` with the main application struct and window creation logic.

Module structure:

    src/gui/
    ├── mod.rs          - Module exports, GuiApp struct
    ├── state.rs        - Application state (iteration count, running status)
    └── render.rs       - UI rendering functions

The GuiApp struct implements eframe::App trait, which requires an `update` method called every frame to render the UI.

Integration with existing code:
- The GUI runs in the main thread
- Automation runs in a background thread (already implemented)
- Communication via atomic flags (AUTOMATION_RUNNING, ABORT_REQUESTED)
- GUI polls automation status each frame

Window properties:
- Title: "学マス リハーサル統計自動化ツール"
- Initial size: 600x500 pixels
- Resizable: Yes
- Min size: 400x400 pixels


### Milestone 3: Implement Instruction Panel with Guide Images

Create two instruction images showing:
1. How to navigate to the contest/rehearsal mode in the game
2. The rehearsal preparation screen where automation should start

Place images in `resources/guide/`:

    resources/guide/
    ├── step1_contest_mode.png     - Screenshot showing contest mode navigation
    └── step2_rehearsal_page.png   - Screenshot showing rehearsal preparation page

Embed images using include_bytes! macro and load as egui textures. Display side-by-side with Japanese captions:

    ┌─────────────────────┐  ┌─────────────────────┐
    │                     │  │                     │
    │  [Screenshot 1]     │  │  [Screenshot 2]     │
    │                     │  │                     │
    └─────────────────────┘  └─────────────────────┘
      ① コンテストモードへ     ② リハーサル画面で待機

Note: The actual guide images need to be created by the user (screenshots from the game). Placeholder images will be used initially, with documentation on how to replace them.


### Milestone 4: Implement Iteration Input and Control Buttons

Add UI elements for:
- Number input for iteration count (範囲: 1-9999)
- Start button (開始) - disabled while running
- Stop button (停止) - enabled only while running

Layout:

    ┌────────────────────────────────────────────┐
    │  実行回数: [____100____]  回               │
    │                                            │
    │  [ 開始 ]           [ 停止 ]               │
    └────────────────────────────────────────────┘

The iteration count is stored in GuiApp state and passed to start_automation(). The Stop button triggers the abort flag.

Input validation:
- Minimum: 1
- Maximum: 9999
- Default: 100
- Non-numeric input rejected


### Milestone 5: Implement Progress Display

Show real-time automation progress:
- Status text: "待機中", "実行中 (45/100)", "完了", "中断"
- Progress bar: 0-100%
- Current state description

Layout:

    ┌────────────────────────────────────────────┐
    │  状態: 実行中 - スキップボタン待機          │
    │                                            │
    │  進捗: 45 / 100 回                         │
    │  ━━━━━━━━━━━━━━━░░░░░░░░░░ 45%             │
    │                                            │
    │  経過時間: 12:34                           │
    └────────────────────────────────────────────┘

Progress tracking requires exposing current iteration from automation thread. Add a new atomic counter in runner.rs or use a channel.


### Milestone 6: Implement Session-Based Output Folders

Modify the automation and analysis flow to create timestamped session folders.

Current output structure:

    exe_dir/
    ├── results.csv
    └── output/
        ├── chart_s1c1.png
        └── ...

New output structure:

    exe_dir/
    └── output/
        └── 20260115_143025/          # Session timestamp
            ├── results.csv           # This session's raw data
            ├── statistics.json       # Statistics for this session
            └── charts/
                ├── chart_s1c1.png
                └── ...

Changes required:
1. In runner.rs: Create session folder at start, pass path to CSV writer
2. In analysis/mod.rs: Accept session folder path, output charts there
3. Auto-generate charts when automation completes
4. Add "フォルダを開く" button to open latest session folder


### Milestone 7: Simplify Tray Menu and Integrate GUI

Replace the current complex tray menu with a simplified version.

Current menu (complex):

    ├── Take Screenshot
    ├── Test OCR
    ├── Start Automation
    ├── ─────────────
    ├── Calibration
    │   ├── Run Calibration Wizard
    │   ├── Capture Start Reference
    │   ├── Capture Skip Reference
    │   └── Capture End Reference
    ├── ─────────────
    ├── Generate Charts
    └── Exit

New menu (simplified):

    ├── ウィンドウを表示
    └── 終了

Integration:
- Double-click tray icon: Show window
- "ウィンドウを表示": Show window (bring to front if already open)
- "終了": Exit application
- Close window: Minimize to tray (not exit)

Keep hotkeys:
- Ctrl+Shift+S: Screenshot (useful independently)
- Ctrl+Shift+Q: Abort automation (critical safety feature)

Remove or disable hotkeys:
- Ctrl+Shift+A: No longer needed (GUI has Start button)
- Test hotkeys (F9, F10, F11, F12): Developer-only


### Milestone 8: Implement Developer Mode Toggle

Add a configuration option to show/hide developer features.

Config file addition in config.json:

    {
      "developer_mode": false,
      ...existing config...
    }

When developer_mode is true:
- Tray menu shows additional "開発者ツール" submenu
- GUI shows collapsible "開発者オプション" section at bottom

Developer features:
- Calibration wizard
- Reference image capture (Start, Skip, End)
- OCR test
- Test click hotkeys

Implementation:
- Add `developer_mode: bool` field to AutomationConfig struct
- Check flag when building tray menu
- Check flag when rendering GUI


### Milestone 9: End-to-End Testing

Manual testing checklist:
- Application starts and shows tray icon
- Double-click tray icon opens GUI window
- GUI displays instruction images correctly
- Iteration count input works (accepts 1-9999)
- Start button begins automation
- Progress bar updates during automation
- Stop button aborts automation
- Charts generated automatically on completion
- Session folder created with correct structure
- "フォルダを開く" opens session folder
- Close button minimizes to tray
- "終了" from tray menu exits application
- Developer mode shows/hides features correctly
- Ctrl+Shift+Q abort works during automation
- Multiple automation sessions create separate folders


## Concrete Steps

All commands run from repository root: `C:\Work\GitRepos\gakumas-rehearsal-automation`


### Step 1: Add eframe dependency

Edit `Cargo.toml` to add:

    [dependencies]
    eframe = "0.29"

Verify:

    cargo build --release

Expected: Build succeeds. eframe and dependencies download and compile (may take a few minutes first time).


### Step 2: Create GUI module structure

Create these files:

    src/gui/mod.rs
    src/gui/state.rs
    src/gui/render.rs

Add to `src/main.rs`:

    mod gui;

Minimal mod.rs:

    pub mod state;
    pub mod render;

    pub struct GuiApp {
        // state fields
    }

    impl eframe::App for GuiApp {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("学マス リハーサル自動化ツール");
            });
        }
    }

Verify:

    cargo build --release

Expected: Build succeeds with stub GUI module.


### Step 3: Create placeholder guide images

Create directory and placeholder images:

    resources/guide/step1_contest_mode.png
    resources/guide/step2_rehearsal_page.png

For now, create simple placeholder images (can be any PNG). The user will replace these with actual game screenshots later.

Document the expected image content in a README:

    resources/guide/README.md


### Step 4: Implement GuiApp with instruction panel

Update src/gui/mod.rs with:
- Image loading from embedded bytes
- Texture creation
- Side-by-side image display with captions

Test by running the application and checking that images display correctly.


### Step 5: Add iteration input and buttons

Update src/gui/render.rs with:
- DragValue for iteration count
- Start/Stop buttons with proper enable/disable states
- Button click handlers that call start_automation() and set abort flag

Test by verifying input validation and button states.


### Step 6: Add progress display

Add to src/automation/runner.rs:
- CURRENT_ITERATION atomic counter
- get_current_iteration() function

Update GUI to poll and display:
- Current iteration / total
- Progress bar percentage
- Elapsed time
- Status text based on automation state

Test during actual automation run.


### Step 7: Implement session-based output

Modify src/automation/runner.rs:
- Create timestamped session folder at start
- Pass session path to CSV writer and analysis

Modify src/analysis/mod.rs:
- Accept session folder path
- Create charts/ subdirectory

Add to GUI:
- Store latest session path
- "フォルダを開く" button that opens folder in Explorer

Test by running automation and checking folder structure.


### Step 8: Simplify tray menu

In src/main.rs:
- Remove most menu items
- Keep only "ウィンドウを表示" and "終了"
- Add show_window() function
- Handle window close as minimize

Test by right-clicking tray icon and verifying menu.


### Step 9: Add developer mode

In src/automation/config.rs:
- Add developer_mode field (default false)

In src/main.rs:
- Check developer_mode when building menu
- Add developer submenu if enabled

In src/gui/render.rs:
- Check developer_mode when rendering
- Add collapsible developer section if enabled

Test both modes.


### Step 10: Integration and final testing

Run through complete testing checklist from Milestone 9.

Verify:
- All UI elements function correctly
- Automation works end-to-end
- Session folders created correctly
- Developer mode toggles correctly


## Validation and Acceptance

The GUI implementation is complete when:

1. Application launches with tray icon and GUI window

2. GUI window displays:
   - Two instruction images with Japanese captions
   - Iteration count input (1-9999)
   - Start button (enabled when idle)
   - Stop button (enabled when running)
   - Progress bar and status text

3. Start button launches automation with specified iteration count

4. Progress updates in real-time during automation

5. Stop button (or Ctrl+Shift+Q) aborts automation

6. Charts auto-generate to session folder on completion

7. Session folders have structure:
   output/YYYYMMDD_HHMMSS/
   ├── results.csv
   ├── statistics.json
   └── charts/*.png

8. "フォルダを開く" button opens latest session folder

9. Tray menu simplified to "ウィンドウを表示" and "終了"

10. Close button minimizes to tray (application keeps running)

11. Developer mode shows/hides advanced features based on config

12. Multiple automation sessions create separate folders


## Idempotence and Recovery

- Closing and reopening GUI window preserves state (iteration count)
- Aborting automation leaves partial session folder (useful for debugging)
- Running "Generate Charts" button works even if automation was aborted
- Session folders are never deleted automatically
- Config file is read fresh when showing developer features
- Guide images can be replaced without rebuilding (future enhancement)


## Artifacts and Notes

### Expected GUI Layout (Japanese)

    ┌──────────────────────────────────────────────────────────────┐
    │  学マス リハーサル自動化ツール                                │
    ├──────────────────────────────────────────────────────────────┤
    │                                                              │
    │  ┌─────────────────────┐  ┌─────────────────────┐           │
    │  │                     │  │                     │           │
    │  │   [Screenshot 1]    │  │   [Screenshot 2]    │           │
    │  │                     │  │                     │           │
    │  └─────────────────────┘  └─────────────────────┘           │
    │   ① コンテストモードへ      ② この画面で待機                 │
    │                                                              │
    ├──────────────────────────────────────────────────────────────┤
    │                                                              │
    │   実行回数:  [____100____]  回                               │
    │                                                              │
    │   [ ▶ 開始 ]                    [ ◼ 停止 ]                  │
    │                                                              │
    ├──────────────────────────────────────────────────────────────┤
    │                                                              │
    │   状態: 待機中                                               │
    │   進捗: ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  0%            │
    │                                                              │
    ├──────────────────────────────────────────────────────────────┤
    │                                                              │
    │   [ 📊 グラフを生成 ]     [ 📁 フォルダを開く ]              │
    │                                                              │
    └──────────────────────────────────────────────────────────────┘


### Expected Tray Menu (Normal Mode)

    ┌─────────────────────┐
    │ ウィンドウを表示     │
    ├─────────────────────┤
    │ 終了                │
    └─────────────────────┘


### Expected Tray Menu (Developer Mode)

    ┌─────────────────────────┐
    │ ウィンドウを表示         │
    ├─────────────────────────┤
    │ 開発者ツール          ▶ │ → キャリブレーション
    │                         │ → OCRテスト
    │                         │ → スクリーンショット
    │                         │ → Start参照画像
    │                         │ → Skip参照画像
    │                         │ → End参照画像
    ├─────────────────────────┤
    │ 終了                    │
    └─────────────────────────┘


### Session Folder Structure

    output/
    └── 20260115_143025/
        ├── results.csv           # Raw iteration data
        │   iteration,timestamp,screenshot,s1c1,s1c2,...
        │   1,2026-01-15T14:30:25,001.png,50000,51000,...
        │
        ├── statistics.json       # Calculated statistics
        │   {
        │     "total_runs": 100,
        │     "columns": [...]
        │   }
        │
        └── charts/
            ├── chart_s1c1.png    # Per-column charts
            ├── chart_s1c2.png
            ├── ...
            ├── chart_s3c3.png
            └── chart_combined.png


### Config File Addition

Add to config.json:

    {
      "developer_mode": false,
      ...existing fields...
    }


## Interfaces and Dependencies

### Dependencies (Cargo.toml additions)

    eframe = "0.29"


### New Files

    src/gui/mod.rs              - GuiApp struct, eframe::App impl
    src/gui/state.rs            - GuiState struct (iteration count, status)
    src/gui/render.rs           - UI rendering functions
    resources/guide/step1_contest_mode.png
    resources/guide/step2_rehearsal_page.png
    resources/guide/README.md


### Modified Files

    src/main.rs                 - Tray menu simplification, GUI launch
    src/automation/runner.rs    - Session folder creation, progress counter
    src/automation/config.rs    - Add developer_mode field
    src/analysis/mod.rs         - Accept session folder path
    config.json                 - Add developer_mode field


### Key Types: src/gui/mod.rs

    use eframe::egui;

    pub struct GuiApp {
        /// Number of iterations to run
        iterations: u32,

        /// Path to latest session folder
        latest_session: Option<PathBuf>,

        /// Loaded guide images
        guide_images: [Option<egui::TextureHandle>; 2],
    }

    impl eframe::App for GuiApp {
        fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame);
    }

    impl GuiApp {
        pub fn new(cc: &eframe::CreationContext) -> Self;
    }


### Key Types: src/gui/state.rs

    /// Automation status for display
    pub enum AutomationStatus {
        Idle,
        Running { current: u32, total: u32, state: String },
        Completed { session_path: PathBuf },
        Aborted,
        Error(String),
    }

    impl AutomationStatus {
        pub fn get_current() -> Self;
    }


### Key Functions: src/automation/runner.rs (additions)

    /// Current iteration counter (for progress display)
    static CURRENT_ITERATION: AtomicU32 = AtomicU32::new(0);

    /// Get current iteration (0 if not running)
    pub fn get_current_iteration() -> u32;

    /// Get total iterations for current run
    pub fn get_total_iterations() -> u32;

    /// Get current state description
    pub fn get_current_state_description() -> String;


### Key Functions: src/main.rs (modifications)

    /// Show the GUI window (create if needed, bring to front if exists)
    fn show_gui_window();

    /// Handle window close event (minimize to tray)
    fn handle_window_close();


### Config Addition: src/automation/config.rs

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct AutomationConfig {
        // ...existing fields...

        /// Enable developer mode (shows calibration, OCR test, etc.)
        #[serde(default)]
        pub developer_mode: bool,
    }


---

## Revision History

- 2026-01-15: Initial ExecPlan created for Phase 5 GUI implementation
