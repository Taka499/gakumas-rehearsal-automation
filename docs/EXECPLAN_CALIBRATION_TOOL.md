# Calibration Tool for Button Position Configuration

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. This document must be maintained in accordance with `docs/PLANS.md`.


## Purpose / Big Picture

After this change, users can interactively define the button positions needed for automation, without manually editing JSON coordinates. The user runs the application, selects "Calibrate Regions..." from the tray menu, positions their mouse cursor over the game window, and presses hotkeys to record each location. The tool converts screen positions to relative coordinates (0.0-1.0) that work regardless of window size, then saves everything to `config.json`.

A key feature is visual preview: during calibration, after each item is defined, the tool captures a screenshot with the newly defined item highlighted so users can immediately verify accuracy. Additionally, a "Preview Regions" menu item provides a one-shot preview showing all configured items overlaid on the current game screen.

**Important**: Score regions are NO LONGER needed for calibration. The OCR module (Phase 2) uses full-image processing with pattern matching, making region-based score extraction unnecessary. The calibration wizard now only collects 3 items:
1. Start button position (for automation clicks)
2. Skip button position (for automation clicks)
3. Skip button brightness region (for loading state detection)


## Progress

- [x] Milestone 1: ~~CLI argument parsing with clap~~ Tray menu integration
- [x] Milestone 2: Calibration mode infrastructure
- [x] Milestone 3: Preview rendering system
- [x] Milestone 4: Interactive point/region capture with per-step preview
- [x] Milestone 5: ~~Live preview mode (--preview)~~ "Preview Regions" menu item (one-shot)
- [x] Milestone 6: Config serialization and verification
- [x] Milestone 7: End-to-end validation (tested manually)
- [x] Milestone 8: Simplified calibration (removed score regions for full-image OCR approach)


## Surprises & Discoveries

- The application requires administrator privileges (from manifest), so CLI arguments don't work well with elevated processes. Switched to tray menu integration instead.
- Score region calibration was unnecessary - the OCR module from gakumas-tools uses full-image processing with pattern matching, which is resolution-independent and doesn't need pre-defined regions.


## Decision Log

- Decision: Use tray menu instead of CLI arguments for calibration entry
  Rationale: The application requires administrator privileges (manifest), making CLI arguments problematic with UAC and terminal elevation. Tray menu integrates naturally with the existing elevated process and provides familiar UX.
  Date/Author: 2026-01-14 / Implementation change

- Decision: Use hotkey-based calibration rather than GUI overlay
  Rationale: Building a transparent overlay window with click handling is complex. Hotkeys are simpler and the application already has hotkey infrastructure. User can see the game window directly without overlay interference.
  Date/Author: 2026-01-13 / Initial design

- Decision: Remove score region calibration entirely
  Rationale: The gakumas-tools project demonstrated that full-image OCR with pattern matching works better than region-based extraction. This approach is resolution-independent and doesn't require any score region calibration. Calibration reduced from 15 steps to 3 steps.
  Date/Author: 2026-01-14 / Simplification for Phase 2 OCR approach

- Decision: Save preview images to files and open with system default viewer
  Rationale: Building a custom preview window adds complexity (Win32 GDI, message loop). Using the system image viewer is simpler and users are familiar with it. Preview images are saved as PNG files which can also be shared for debugging.
  Date/Author: 2026-01-13 / Enhancement


## Outcomes & Retrospective

**Completed: 2026-01-14**

### What was delivered
- Tray menu integration with "Calibrate Regions..." and "Preview Regions" options
- Interactive calibration wizard with 3 items (simplified from original 15):
  1. Start button position
  2. Skip button position
  3. Skip button brightness region
- Hotkey-based capture: F1 (point), F2 (top-left), F3 (bottom-right), Y (confirm), N (redo), Enter (skip), Escape (abort)
- Visual preview after each step showing captured items highlighted
- Config serialization to config.json
- Skip functionality to keep existing values from config.json

### What worked well
- Tray menu approach was simpler than CLI args given admin privilege requirements
- Hotkey-based capture worked smoothly without needing complex overlay UI
- Preview images in system viewer provided immediate visual feedback
- Removing score region calibration drastically simplified the user experience

### Lessons learned
- Windows apps requiring elevation don't work well with CLI arguments from terminals
- Region capture needs both TopLeft and BottomRight steps; redo (N) must rewind to TopLeft
- Global Mutex with HWND requires storing as isize for Send+Sync safety
- Full-image OCR with pattern matching is superior to region-based extraction for this use case


## Context and Orientation

The gakumas-rehearsal-automation application is a Windows system tray tool that captures screenshots of the game "Gakuen iDOLM@STER". It currently supports:

- Finding the game window by process name (`gakumas.exe`)
- Capturing the window via Windows Graphics Capture API
- Sending mouse clicks to relative coordinates
- Detecting loading state via brightness analysis

Key files:

    src/main.rs                    - Entry point, message loop, hotkey handling
    src/capture/window.rs          - Window discovery (find_gakumas_window)
    src/capture/screenshot.rs      - Full window capture
    src/capture/region.rs          - Partial region capture
    src/automation/config.rs       - Configuration types and loading
    src/calibration/wizard.rs      - Calibration wizard implementation
    src/calibration/preview.rs     - Preview rendering
    src/calibration/state.rs       - Calibration state tracking
    config.json                    - Runtime configuration file

The current `config.json` structure (after simplification):

    {
      "start_button": { "x": 0.5, "y": 0.85 },
      "skip_button": { "x": 0.82, "y": 0.82 },
      "skip_button_region": { "x": 0.7, "y": 0.80, "width": 0.22, "height": 0.04 },
      "brightness_threshold": 150.0,
      "loading_timeout_ms": 30000,
      "capture_delay_ms": 500,
      "test_click_position": { "x": 0.92, "y": 0.84 }
    }

Note: The `score_regions` and `stage_total_regions` fields have been removed. They are no longer needed because the OCR module uses full-image processing with pattern matching.

Terms used in this document:

- Relative coordinates: Values from 0.0 to 1.0 representing position as fraction of window size. (0.0, 0.0) is top-left, (1.0, 1.0) is bottom-right.
- Client area: The drawable portion of a window, excluding title bar and borders.
- HWND: Windows handle to a window object.


## Plan of Work

### Milestone 1: Tray Menu Integration

The application uses tray menu items instead of CLI arguments (due to admin privilege requirements):

- "Calibrate Regions..." - Starts the calibration wizard
- "Preview Regions" - Shows a one-shot preview of configured items


### Milestone 2: Calibration Mode Infrastructure

The `src/calibration/` module contains:

- `state.rs` - CalibrationStep enum and CalibrationItems struct
- `wizard.rs` - Main calibration logic with hotkey handling
- `coords.rs` - Screen-to-relative coordinate conversion
- `preview.rs` - Preview rendering functions


### Milestone 3: Preview Rendering System

The preview module draws items on screenshots:
- Button positions: Red crosshairs (15px arms) labeled implicitly by position
- Skip button brightness region: Yellow rectangle with 2px border
- Highlighted item (current step): Orange with thicker border (4px or 20px arms)


### Milestone 4: Interactive Point/Region Capture

Calibration flow:
1. Display instructions in console for current step
2. User positions cursor over game window
3. For points (buttons): Press F1 to record cursor position
4. For regions: Press F2 at top-left corner, then F3 at bottom-right corner
5. Convert screen coordinates to relative coordinates
6. Show preview with newly defined item highlighted
7. User confirms (Y to accept, N to redo)
8. Advance to next step
9. Press Escape to abort, Enter to skip (keep existing value)


### Milestone 5: Preview Regions Menu Item

The "Preview Regions" tray menu option:
1. Captures the current game screen
2. Draws all configured items on the screenshot
3. Saves as `regions_preview.png`
4. Opens in default image viewer


### Milestone 6: Config Serialization

After all items are captured:
1. Build the complete AutomationConfig struct
2. Serialize to JSON with serde_json::to_string_pretty
3. Write to config.json next to executable
4. Show final preview with all items


## Validation and Acceptance

The calibration tool is complete when:

1. Running "Calibrate Regions..." walks through 3 items (2 buttons + 1 brightness region) with visual preview after each step

2. After completing calibration, `config.json` contains all fields with reasonable values (all between 0.0 and 1.0)

3. Running "Preview Regions" produces an image showing:
   - Red crosshairs at button positions
   - Yellow rectangle for skip button brightness region

4. The existing screenshot hotkey (Ctrl+Shift+S) still works in normal mode


## Idempotence and Recovery

- Calibration can be rerun at any time; it overwrites `config.json`
- If calibration is aborted (Escape), existing `config.json` is not modified
- If the game window is not found, calibration exits with a clear error message
- Each recorded position is confirmed in the console before advancing


## Artifacts and Notes

Sample expected `config.json` after calibration:

    {
      "start_button": { "x": 0.50, "y": 0.85 },
      "skip_button": { "x": 0.82, "y": 0.82 },
      "skip_button_region": { "x": 0.70, "y": 0.80, "width": 0.22, "height": 0.04 },
      "brightness_threshold": 150.0,
      "loading_timeout_ms": 30000,
      "capture_delay_ms": 500,
      "test_click_position": { "x": 0.92, "y": 0.84 }
    }


## Interfaces and Dependencies

### Module: src/calibration/mod.rs

    pub mod state;
    pub mod coords;
    pub mod preview;
    pub mod wizard;

    pub use preview::{render_preview, render_preview_with_highlight, show_preview};
    pub use wizard::{
        handle_calibration_hotkey, start_calibration, show_preview_once,
        HOTKEY_CAL_F1, HOTKEY_CAL_F2, HOTKEY_CAL_F3, HOTKEY_CAL_Y, HOTKEY_CAL_N,
        HOTKEY_CAL_ESCAPE, HOTKEY_CAL_ENTER,
    };


### Types: src/calibration/state.rs

    #[derive(Clone, Default)]
    pub struct CalibrationItems {
        pub start_button: Option<ButtonConfig>,
        pub skip_button: Option<ButtonConfig>,
        pub skip_button_region: Option<RelativeRect>,
    }

    #[derive(Clone, Debug, PartialEq)]
    pub enum CalibrationStep {
        StartButton,
        SkipButton,
        SkipButtonRegionTopLeft,
        SkipButtonRegionBottomRight,
        Complete,
    }


### Preview: src/calibration/preview.rs

    pub enum HighlightedItem {
        StartButton,
        SkipButton,
        SkipButtonRegion,
    }

    pub fn render_preview(
        screenshot: &ImageBuffer<Rgba<u8>, Vec<u8>>,
        config: &AutomationConfig,
    ) -> ImageBuffer<Rgba<u8>, Vec<u8>>;

    pub fn render_preview_with_highlight(
        screenshot: &ImageBuffer<Rgba<u8>, Vec<u8>>,
        config: &AutomationConfig,
        highlight: &HighlightedItem,
    ) -> ImageBuffer<Rgba<u8>, Vec<u8>>;

    pub fn show_preview(img: &ImageBuffer<Rgba<u8>, Vec<u8>>, filename: &str) -> Result<()>;


---

## Revision History

- 2026-01-13: Initial ExecPlan created
- 2026-01-13: Added visual preview system with three modes
- 2026-01-14: Completed initial implementation with 15 calibration steps
- 2026-01-14: Major simplification - removed score region calibration
  - Score regions and stage total regions removed from config and calibration
  - Calibration reduced from 15 steps to 3 steps
  - Rationale: Phase 2 OCR uses full-image processing with pattern matching (gakumas-tools approach), which doesn't require pre-defined score regions
  - This makes calibration much simpler for users while also being more robust
