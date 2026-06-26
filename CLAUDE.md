# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## ExecPlans

When writing complex features or significant refactors, use an ExecPlan (as described in docs/PLANS.md) from design to implementation.

When working on execution plans (ExecPlans), always read the full plan document completely before beginning any implementation or summarization. Confirm understanding by listing all milestones/phases before proceeding.

Active ExecPlans (keep their `Progress` sections current; each is self-contained):
- `docs/EXECPLAN_RESUME_AUTOMATION.md` - resume an interrupted automation run. Complete (committed d968a4a, acceptance passed).
- `docs/EXECPLAN_GUI_STATE_DRIVEN_PANEL.md` - redesign the GUI third column into a state-driven control panel. Code-complete (merged f8e5230); manual acceptance pending. See its Progress section for status.
- `docs/EXECPLAN_ADDITIONAL_RUNS_AND_PRESETS.md` - add "追加実行" (extend a finished series into the same folder) and 100/200/500/1000 preset run-count buttons. Code-complete (M1–M3); manual acceptance pending. See its Progress section for status.
- `docs/EXECPLAN_OVERLAP_SCORE_RECOVERY.md` - recover per-character scores when two ≥1,000,000 values overlap in the rehearsal UI (one digit collides; right number's leading "1" is always lost). Uses a structural re-split plus the on-screen `total = c1+c2+c3+bonus` checksum to reconstruct/flag. Not started; see its Progress section.
- `docs/EXECPLAN_REVIEW_INLINE_STAGE_CROPS.md` - refine the OCR review window: replace the right-hand whole-screenshot preview with inline, expand-on-demand, per-stage crops (character icons + printed scores) placed under each stage's editable columns, sized dynamically to the column-group width. Crop derived from `score_regions` + a configurable `ReviewCropAdjust` offset (so the dev's future horizontal re-layout tracks from one source). Not started; see its Progress section.


## Project Overview

Windows screenshot tool that captures the client area of `gakumas.exe` using Windows Graphics Capture API. Runs as a system tray application with global hotkey support. Includes rehearsal automation with embedded Tesseract OCR.

## COMMIT DISCIPLINE
- Follow Git-flow workflow to manage the branches
- Use small, frequent commits rather than large, infrequent ones
- Only add and commit affected files. Keep untracked other files as are
- Never add Claude Code attribution in commit

## Build Commands

Build emits ~30 expected warnings (unused `pub use` re-exports, OCR dead code); these are not regressions. Filter with `cargo check 2>&1 | grep "^error"` to find real failures.

```powershell
# Build release (optimized with LTO). PREFER the guarded wrapper: a running app
# instance locks gakumas-screenshot.exe, so a bare `cargo build --release` only
# fails at the LINK step after a full multi-minute compile ("failed to remove
# file ... gakumas-screenshot.exe"). build.ps1 checks for a running instance
# FIRST and aborts in ~1s (pass -Kill to stop it automatically). Always run this
# guard (or check `Get-Process gakumas-screenshot`) before building.
powershell -ExecutionPolicy Bypass -File scripts/build.ps1          # cargo build --release
powershell -ExecutionPolicy Bypass -File scripts/build.ps1 -Kill    # stop a running instance first
cargo build --release                                                # bare form (only safe if the app is closed)

# Create release package with proper folder structure (also guards the running app)
powershell -ExecutionPolicy Bypass -File scripts/package-release.ps1

# Run
.\target\release\gakumas-screenshot.exe
```

## Architecture

Multi-module Rust application with these key components:

- **src/main.rs**: Entry point, initializes GUI or legacy tray mode
- **src/paths.rs**: Centralized path resolution (logs/, screenshots/, output/, template/, tesseract/)
- **src/gui/**: egui-based GUI window. The third column is a single state-driven panel: `render.rs::render_control_panel` branches on `AutomationStatus` and returns a `PanelActions` struct that `mod.rs::update()` dispatches to `handle_*` methods. Add controls by emitting a button → setting a `PanelActions` field → dispatching it, not by rendering everything unconditionally.
- **src/capture/**: Window discovery and screenshot capture via Windows Graphics Capture API
- **src/automation/**: Rehearsal automation state machine, button detection, OCR worker, session metadata/resume (`session_meta.rs`). Every "run N iterations" variant — `start_automation` (fresh), `resume_automation` (finish remaining), `extend_automation` (add more to a finished series) — delegates to `runner.rs::start_automation_inner(iterations, start_iteration, existing_session)`; wrap it rather than duplicating the window/CSV/log/meta/thread setup. After starting, the GUI reads the live total/current from runner atomics (`get_total_iterations`/`get_current_iteration`), not by recomputing.
- **src/calibration/**: Interactive calibration wizard for button positions
- **src/ocr/**: Tesseract integration with per-stage crop→threshold→OCR→extract pipeline
- **src/analysis/**: Statistics calculation and chart generation (plotters)

Key technical details:
- **Window Discovery**: `EnumWindows` + `QueryFullProcessImageNameW` to find target process
- **Screen Capture**: Windows Graphics Capture (WGC) API via `IGraphicsCaptureItemInterop::CreateForWindow`
- **GPU Pipeline**: D3D11 device creates staging texture, copies captured frame, maps for CPU read
- **Embedded Tesseract**: `include_bytes!` embeds tesseract.zip, extracted on first run to exe directory
- **OCR Pipeline**: Per-stage cropping (`score_regions` in config) → brightness thresholding → Tesseract `--psm 6` → sanitize leading garbage chars → regex extraction. Each stage processed independently to avoid cross-stage noise. Crop regions are tightened to exclude horizontal UI divider lines that confuse Tesseract layout analysis
- **Session folders**: Each automation series writes to `output/YYYYMMDD_HHMMSS/` holding `screenshots/`, `results.csv`, `session.log`, `charts/`, and `run-meta.json`. `run-meta.json` (written by `session_meta.rs`) records `total`/`completed`/`status`/`dismissed` so an interrupted series can resume into the same folder; `completed` is authoritatively recomputed from the screenshot count (crash-proof), not trusted from the file. `dismissed: true` (set via `dismiss_session`) hides a session from the resume picker without deleting its data

## Key Constants and Hotkeys

- Process matching: exact match `"gakumas.exe"` (case-insensitive)
- `HOTKEY_ID` (1): Ctrl+Shift+S - Screenshot
- `HOTKEY_AUTOMATION` (6): Ctrl+Shift+A - Start automation
- `HOTKEY_ABORT` (7): Ctrl+Shift+Q - Abort automation
- `HOTKEY_CLICK_TEST` (2): Ctrl+Shift+F9 - PostMessage click test
- `HOTKEY_SENDINPUT_TEST` (3): Ctrl+Shift+F10 - SendInput click test
- Output: `screenshots/gakumas_YYYYMMDD_HHMMSS.png`
- Log: `logs/gakumas_screenshot.log`
- Reference images: `resources/template/rehearsal/*.png`

## Windows API Notes

- Uses Rust 2024 edition requiring explicit `unsafe` blocks inside `unsafe fn`
- `EnumWindows` returns FALSE when callback stops early - don't treat as error
- `windows` crate v0.58 feature flags must match APIs used (see Cargo.toml)
- `SendInput` with `SetForegroundWindow` is required for game input (PostMessage is ignored)
- Must run as Administrator if game runs elevated (UIPI restriction)
- egui render fns: matching on `state.status` (or iterating `state.resumable_sessions`) while mutating sibling `GuiState` fields trips `E0502`. Clone the status (`let status = state.status.clone();`) or snapshot the list into an owned `Vec` first, then mutate freely

## Design Constraints

- **Admin privileges required**: The executable has a Windows manifest (`gakumas-screenshot.exe.manifest`) that requires administrator elevation. This is necessary for `SendInput` to work with elevated game processes.
- **No command-line arguments**: This is a system tray application, not a CLI tool. All functionality should be accessed via tray menu, hotkeys, or config file. Do not add command-line argument handling.
- **Testing limitations**: The admin manifest normally makes the `cargo test` harness require elevation (os error 740). Build tests with `GAKUMAS_NO_MANIFEST=1 cargo test` to skip embedding the manifest so unit tests run unelevated (the gate is in `build.rs`; normal/release builds still embed it). Pure-logic modules (`ocr::extract`, `ocr::reconcile`, `ocr::engine` parsing, `analysis`, `csv_writer`) are covered this way. Tesseract-dependent end-to-end checks are `#[ignore]`d and run explicitly, e.g. `GAKUMAS_NO_MANIFEST=1 cargo test ocr_overlap_recovery_e2e -- --ignored` (uses the embedded Tesseract + sample PNGs under `temp/`). Anything that drives the live tray app/hotkeys still must be tested manually.

## Roadmap

See `docs/ROADMAP_AUTOMATION.md` for the full automation feature roadmap. Current status:
- Phase 1: UI automation (clicking buttons) - complete
- Phase 2: OCR integration (Tesseract) - complete with embedded Tesseract
- Phase 3: Automation loop - complete with state machine
- Phase 4: Statistics and visualization - complete (CSV, charts, JSON)
- Phase 5: User interface - in progress (egui GUI implemented; resume of interrupted runs added; third-column UI redesign pending, see Active ExecPlans)
