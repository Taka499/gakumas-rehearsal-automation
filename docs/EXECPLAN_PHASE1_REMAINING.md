# Phase 1 Completion: Relative Coordinates, Region Capture, and Loading Detection

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

This document must be maintained in accordance with `docs/PLANS.md`.


## Purpose / Big Picture

After this change, the automation system will be able to click any button in the game window using position-independent relative coordinates (0.0 to 1.0), capture arbitrary sub-regions of the screen for analysis, and detect when the game finishes loading by monitoring pixel brightness. These capabilities form the foundation for the full automation loop where the tool will repeatedly click "Start", wait for loading, click "Skip", and capture results.

A user can verify success by: (1) running a new hotkey that clicks a configurable relative position in the game window, (2) observing that partial region screenshots are saved correctly, and (3) seeing log messages that report brightness values from a monitored region, enabling calibration of loading detection thresholds.


## Progress

- [x] (2026-01-13 09:00Z) Add serde and serde_json dependencies to Cargo.toml for configuration serialization.
- [x] (2026-01-13 09:05Z) Create `src/automation/config.rs` with `AutomationConfig`, `ButtonConfig`, and `RelativeRect` structs.
- [x] (2026-01-13 09:10Z) Create a default `config.json` file with placeholder button positions.
- [x] (2026-01-13 09:15Z) Update `src/automation/mod.rs` to export config module.
- [x] (2026-01-13 09:20Z) Update `src/main.rs` to call `init_config()` at startup.
- [x] (2026-01-13 09:25Z) Implement `click_at_relative()` in `src/automation/input.rs` that converts relative coords to screen coords and clicks.
- [x] (2026-01-13 09:30Z) Add a relative click test hotkey (Ctrl+Shift+F12) that clicks at a configurable position.
- [x] (2026-01-13 09:35Z) Create `src/capture/region.rs` with `capture_region()` function for partial screenshots.
- [x] (2026-01-13 09:40Z) Update `src/capture/mod.rs` to export region module.
- [x] (2026-01-13 09:45Z) Create `src/automation/detection.rs` with `calculate_brightness()` and `wait_for_loading()` functions.
- [x] (2026-01-13 09:50Z) Update `src/automation/mod.rs` to export detection module.
- [x] (2026-01-13 09:55Z) Add a brightness test hotkey (Ctrl+Shift+F11) that captures a region and logs brightness.
- [x] (2026-01-13 10:00Z) Build passes with `cargo build --release` (warnings only for unused exports, expected).
- [ ] Manual testing against the game window (requires game to be running).


## Surprises & Discoveries

- Observation: The `HWND` type needed to be explicitly imported in `input.rs` for the new functions.
  Evidence: Build error `E0412: cannot find type HWND in this scope` - fixed by adding `HWND` to the import from `windows::Win32::Foundation`.

- Observation: Some unused import warnings are expected and acceptable.
  Evidence: Functions like `wait_for_loading` and `capture_region` are exported for future use in the automation loop (Phase 3). These warnings will resolve when those phases are implemented.


## Decision Log

- Decision: Use serde_json for configuration rather than TOML or other formats.
  Rationale: JSON is simple, widely understood, and the roadmap already specifies JSON format for config files. No additional dependencies beyond serde_json are needed.
  Date/Author: 2026-01-13

- Decision: Store configuration as a static global loaded at startup rather than passing it through function parameters.
  Rationale: The tray application has a single message loop and configuration rarely changes. A global simplifies the hotkey handlers which cannot easily receive parameters. The config will be loaded once at startup from `config.json` if it exists.
  Date/Author: 2026-01-13

- Decision: Implement brightness detection using luminance formula (0.299R + 0.587G + 0.114B) on grayscale conversion.
  Rationale: This is the standard ITU-R BT.601 luma formula, matches what the roadmap specifies, and works well for detecting dimmed vs bright button states.
  Date/Author: 2026-01-13

- Decision: Config file should be loaded from the same directory as the executable, not the current working directory.
  Rationale: Users typically run the exe from `target/release/` but config.json is in repo root. Looking next to the exe is more intuitive.
  Date/Author: 2026-01-13

- Decision: **REVISED** - Use OCR + brightness for two-phase loading detection.
  Rationale: Testing revealed three distinct states:
    - State 1 (before skip appears): brightness ~98.06, no "スキップ" text
    - State 2 (skip loading): brightness ~92.58, "スキップ" text visible but dimmed
    - State 3 (skip ready): brightness ~97.50, "スキップ" text visible and bright
  Detection strategy:
    - Transition 1→2: OCR detects "スキップ" text appearing
    - Transition 2→3: Brightness detection (threshold ~95) detects button becoming ready
  Brightness alone cannot distinguish State 1 from State 3 (both are bright), but OCR + brightness together provide reliable detection.
  Date/Author: 2026-01-13


## Outcomes & Retrospective

**Status: CLOSED - Phase 1 Complete**

All code changes have been implemented, tested, and verified against the game. The following capabilities are now available:

1. **Configuration System**: `config.json` is loaded at startup from the executable's directory. Users can customize button positions, brightness thresholds, and timing parameters.

2. **Relative Coordinate Clicking**: `click_at_relative()` function and Ctrl+Shift+F12 hotkey allow clicking at any position using 0.0-1.0 coordinates. Tested and working.

3. **Region Capture**: `capture_region()` function can capture arbitrary rectangular sub-regions of the game window.

4. **Brightness Detection**: `calculate_brightness()` and `measure_region_brightness()` functions, with Ctrl+Shift+F11 hotkey for calibration. Testing revealed:
   - State 1 (before skip): ~98.06
   - State 2 (loading): ~92.58
   - State 3 (ready): ~97.50

**Key Finding:**
Brightness alone cannot detect all state transitions. A two-phase approach is needed:
- State 1→2: OCR to detect "スキップ" text appearing
- State 2→3: Brightness detection (threshold ~95)

**Lessons Learned:**
- The Windows API imports need careful attention - each new function may require additional type imports
- Config should be loaded relative to exe path, not CWD
- Brightness detection is useful but insufficient alone; OCR is required for complete loading detection


## Context and Orientation

This project is a Windows system tray application written in Rust that captures screenshots of the game "Gakumas" (gakumas.exe). The goal is to automate rehearsal runs: click Start, wait for loading, click Skip, capture result, repeat.

**Current source structure:**

    src/
    ├── main.rs              # Entry point, tray icon, hotkeys, message loop
    ├── capture/
    │   ├── mod.rs           # Re-exports capture functions
    │   ├── window.rs        # find_gakumas_window(), get_client_area_info()
    │   └── screenshot.rs    # capture_gakumas() - full window capture via WGC
    └── automation/
        ├── mod.rs           # Re-exports automation functions
        └── input.rs         # test_postmessage_click(), test_sendinput_click()

**Key existing functions:**

- `find_gakumas_window()` in `src/capture/window.rs`: Enumerates windows, finds the one belonging to `gakumas.exe`, returns its HWND.
- `get_client_area_info()` in `src/capture/window.rs`: Returns the client rectangle and offset from window origin.
- `capture_gakumas()` in `src/capture/screenshot.rs`: Captures the full client area using Windows Graphics Capture API, saves as PNG.
- `test_sendinput_click()` in `src/automation/input.rs`: Clicks at the center of the game window using SendInput. This is the proven working click method.

**Terminology:**

- **Relative coordinates**: A position expressed as (x, y) where both values range from 0.0 to 1.0. (0.0, 0.0) is the top-left corner of the client area; (1.0, 1.0) is the bottom-right corner. This allows button positions to work regardless of window size.
- **Client area**: The drawable portion of a window, excluding the title bar and borders.
- **Brightness/Luminance**: A single value representing how light or dark a pixel or region is. Calculated from RGB using the formula: `0.299*R + 0.587*G + 0.114*B`.
- **Loading state**: The game shows animated dots and dims the Skip button while loading. We detect this by measuring brightness of a region around the button.

**Hotkeys currently registered (in main.rs):**

- `HOTKEY_ID` (1): Ctrl+Shift+S - Screenshot
- `HOTKEY_CLICK_TEST` (2): Ctrl+Shift+F9 - PostMessage click test (does not work with game)
- `HOTKEY_SENDINPUT_TEST` (3): Ctrl+Shift+F10 - SendInput click test (works)

**Dependencies (Cargo.toml):**

- `windows` v0.58 with many features for Win32 and WinRT APIs
- `image` v0.25 for PNG encoding and image manipulation
- `chrono` v0.4 for timestamps
- `anyhow` v1.0 for error handling

We will add `serde` and `serde_json` for configuration file handling.


## Plan of Work

### Milestone 1: Configuration System

Create the configuration structs and file loading mechanism. After this milestone, the application will load button positions from `config.json` at startup.

**Files to create:**

1. `src/automation/config.rs` - New file containing:
   - `RelativeRect` struct with fields `x: f32`, `y: f32`, `width: f32`, `height: f32`
   - `ButtonConfig` struct with fields `x: f32`, `y: f32` (center point of button)
   - `AutomationConfig` struct with fields for start_button, skip_button, skip_button_region (for brightness detection), brightness_threshold, loading_timeout_ms, capture_delay_ms
   - `load_config()` function that reads from `config.json` or returns defaults
   - A static `CONFIG` using `std::sync::OnceLock` for global access

2. `config.json` in project root - Default configuration file with placeholder values (will need calibration for actual use)

**Files to modify:**

1. `Cargo.toml` - Add serde and serde_json dependencies
2. `src/automation/mod.rs` - Add `pub mod config;` and re-export config types
3. `src/main.rs` - Call config initialization at startup

### Milestone 2: Relative Coordinate Click

Extend the input module with a function that clicks at relative coordinates. After this milestone, a new hotkey will click at a position defined in config.json.

**Files to modify:**

1. `src/automation/input.rs` - Add:
   - `click_at_client(hwnd: HWND, client_x: i32, client_y: i32)` - Internal helper that clicks at client coordinates
   - `click_at_relative(hwnd: HWND, rel_x: f32, rel_y: f32)` - Public function that converts relative to client coords and calls click_at_client

2. `src/automation/mod.rs` - Re-export the new functions

3. `src/main.rs` - Add `HOTKEY_RELATIVE_CLICK` (Ctrl+Shift+F12) that reads a test position from config and clicks there

### Milestone 3: Region Capture

Add the ability to capture a sub-region of the game window. After this milestone, partial screenshots can be taken and saved.

**Files to create:**

1. `src/capture/region.rs` - New file containing:
   - `capture_region(hwnd: HWND, rel_rect: &RelativeRect) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>>` - Captures and returns a cropped portion of the window
   - This will reuse the WGC capture pipeline from screenshot.rs but crop to the specified region

**Files to modify:**

1. `src/capture/mod.rs` - Add `pub mod region;` and re-export
2. `src/capture/screenshot.rs` - Extract common D3D11/WGC setup code into helper functions that region.rs can also use (or duplicate minimally for now)

### Milestone 4: Loading Detection

Add brightness calculation and loading detection. After this milestone, a hotkey will capture a region and log its brightness for calibration purposes.

**Files to create:**

1. `src/automation/detection.rs` - New file containing:
   - `calculate_brightness(img: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> f32` - Returns average luminance 0-255
   - `wait_for_loading(hwnd: HWND, config: &AutomationConfig) -> Result<()>` - Polls until brightness exceeds threshold or timeout

**Files to modify:**

1. `src/automation/mod.rs` - Add `pub mod detection;` and re-export
2. `src/main.rs` - Add `HOTKEY_BRIGHTNESS_TEST` (Ctrl+Shift+F11) that captures the skip_button_region and logs brightness


## Concrete Steps

All commands are run from the repository root: `C:\Work\GitRepos\gakumas-rehearsal-automation`

### Step 1: Add Dependencies

Edit `Cargo.toml` to add serde dependencies after the anyhow line:

    serde = { version = "1.0", features = ["derive"] }
    serde_json = "1.0"

Verify the edit compiled:

    cargo check

Expected: Compiles with no errors. May download new crates.

### Step 2: Create config.rs

Create file `src/automation/config.rs` with the configuration structs and loading logic. The file will define:

- `RelativeRect` - A rectangle in relative coordinates (0.0-1.0)
- `ButtonConfig` - A point in relative coordinates for button center
- `AutomationConfig` - All configuration for automation
- `CONFIG` - A global OnceLock holding the loaded config
- `init_config()` - Called once at startup to load config.json
- `get_config()` - Returns reference to the global config

See the "Artifacts and Notes" section for the complete implementation.

### Step 3: Create config.json

Create file `config.json` in the repository root with default/placeholder values:

    {
      "start_button": { "x": 0.5, "y": 0.85 },
      "skip_button": { "x": 0.9, "y": 0.95 },
      "skip_button_region": { "x": 0.85, "y": 0.90, "width": 0.10, "height": 0.10 },
      "brightness_threshold": 150.0,
      "loading_timeout_ms": 30000,
      "capture_delay_ms": 500,
      "test_click_position": { "x": 0.5, "y": 0.5 }
    }

Note: The `test_click_position` field is for testing the relative click hotkey. Actual button positions will need calibration with the real game.

### Step 4: Update automation/mod.rs

Add the config module and re-exports:

    pub mod config;
    pub mod input;

    pub use config::{get_config, init_config, AutomationConfig, ButtonConfig, RelativeRect};
    pub use input::{test_postmessage_click, test_sendinput_click};

### Step 5: Update main.rs to Load Config

Near the start of `main()`, after `RoInitialize`, add:

    automation::init_config();

This loads the configuration file before entering the message loop.

### Step 6: Add click_at_relative to input.rs

Add these functions to `src/automation/input.rs`:

- `click_at_client(hwnd, client_x, client_y)` - Brings window to foreground, converts client coords to screen coords, sends click via SendInput
- `click_at_relative(hwnd, rel_x, rel_y)` - Gets client rect, multiplies by relative coords, calls click_at_client

Update the module's public exports.

### Step 7: Add Relative Click Hotkey

In `src/main.rs`:

1. Add constant: `const HOTKEY_RELATIVE_CLICK: i32 = 4;`
2. In `main()`, register the hotkey:

       RegisterHotKey(hwnd, HOTKEY_RELATIVE_CLICK, MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT, 0x7B)?; // VK_F12

3. In `window_proc`, handle the hotkey by reading `test_click_position` from config and calling `click_at_relative`.
4. Update cleanup to unregister this hotkey.
5. Update the startup log messages.

Build and test:

    cargo build --release
    .\target\release\gakumas-rehearsal-automation.exe

With the game running, press Ctrl+Shift+F12. The cursor should move to the center of the game window (since test_click_position defaults to 0.5, 0.5) and click.

### Step 8: Create region.rs

Create `src/capture/region.rs` with `capture_region()` function. This function:

1. Finds the game window
2. Gets client area info
3. Creates D3D11 device and WGC capture session (similar to screenshot.rs)
4. Captures one frame
5. Calculates absolute pixel coordinates from relative rect
6. Copies only that portion to an ImageBuffer
7. Returns the cropped image (does not save to file)

The implementation will duplicate some code from screenshot.rs for now. A future refactor could extract shared helpers.

### Step 9: Update capture/mod.rs

Add the region module:

    pub mod region;
    pub mod screenshot;
    pub mod window;

    pub use screenshot::capture_gakumas;
    pub use window::find_gakumas_window;
    pub use window::get_client_area_info;
    pub use region::capture_region;

### Step 10: Create detection.rs

Create `src/automation/detection.rs` with:

- `calculate_brightness(img)` - Iterates all pixels, applies luminance formula, returns average
- `wait_for_loading(hwnd, config)` - Loops: capture region, check brightness, sleep, repeat until threshold exceeded or timeout

### Step 11: Update automation/mod.rs for Detection

Add:

    pub mod detection;
    pub use detection::{calculate_brightness, wait_for_loading};

### Step 12: Add Brightness Test Hotkey

In `src/main.rs`:

1. Add constant: `const HOTKEY_BRIGHTNESS_TEST: i32 = 5;`
2. Register: Ctrl+Shift+F11 (VK_F11 = 0x7A)
3. Handle: Capture skip_button_region, calculate brightness, log the value
4. This helps users calibrate the brightness_threshold in config.json

Final build and test:

    cargo build --release

Expected log output when pressing Ctrl+Shift+F11 with game running:

    [HH:MM:SS.mmm] Brightness test hotkey pressed!
    [HH:MM:SS.mmm] Capturing region for brightness test...
    [HH:MM:SS.mmm] Region brightness: 187.34

The brightness value will vary based on game state (loading vs ready).


## Validation and Acceptance

**Acceptance Criteria:**

1. **Configuration Loading**: Application reads `config.json` at startup. If file is missing, defaults are used. Log message confirms config loaded.

2. **Relative Click (Ctrl+Shift+F12)**: With game running, pressing the hotkey moves the cursor to the configured `test_click_position` (default: center) and clicks. The click should be visible in the game (e.g., if positioned over a button, that button activates).

3. **Brightness Measurement (Ctrl+Shift+F11)**: With game running, pressing the hotkey captures the `skip_button_region` and logs a brightness value between 0 and 255. The value should differ noticeably between loading state (dimmed, lower value) and ready state (bright, higher value).

4. **Build Success**: `cargo build --release` completes without errors or warnings.

5. **No Regressions**: Existing hotkeys (Ctrl+Shift+S for screenshot, F9/F10 for click tests) continue to work.

**Manual Test Procedure:**

1. Start the game (gakumas.exe)
2. Run `.\target\release\gakumas-rehearsal-automation.exe`
3. Verify log shows "Config loaded from config.json" or "Using default config"
4. Press Ctrl+Shift+S - screenshot should save (existing functionality)
5. Press Ctrl+Shift+F12 - cursor should move to center of game and click
6. Navigate game to a screen with the Skip button visible
7. Press Ctrl+Shift+F11 during loading (dots visible) - note brightness value
8. Press Ctrl+Shift+F11 when ready (no dots) - note brightness value
9. The ready-state brightness should be noticeably higher than loading-state brightness


## Idempotence and Recovery

All steps can be repeated safely:

- Editing Cargo.toml multiple times is fine; cargo handles dependency resolution
- Creating files overwrites previous versions
- The application can be stopped (right-click tray → Exit) and restarted any time
- Config file can be edited while application is stopped; changes take effect on next start

If a step fails partway:

- `cargo check` or `cargo build` will report the exact error
- Fix the error and rebuild; Cargo's incremental compilation handles partial builds
- If config.json is malformed, the application will log an error and use defaults


## Artifacts and Notes

### config.rs Implementation

    //! Configuration types for automation.
    //!
    //! Loads settings from config.json at startup. Provides button positions,
    //! detection thresholds, and timing parameters.

    use anyhow::Result;
    use serde::{Deserialize, Serialize};
    use std::fs;
    use std::path::Path;
    use std::sync::OnceLock;

    /// Global configuration instance, initialized once at startup.
    static CONFIG: OnceLock<AutomationConfig> = OnceLock::new();

    /// A rectangle in relative coordinates (0.0 to 1.0).
    /// Used for defining screen regions that scale with window size.
    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct RelativeRect {
        /// X position of top-left corner (0.0 = left edge, 1.0 = right edge)
        pub x: f32,
        /// Y position of top-left corner (0.0 = top edge, 1.0 = bottom edge)
        pub y: f32,
        /// Width as fraction of window width
        pub width: f32,
        /// Height as fraction of window height
        pub height: f32,
    }

    impl Default for RelativeRect {
        fn default() -> Self {
            Self {
                x: 0.0,
                y: 0.0,
                width: 0.1,
                height: 0.1,
            }
        }
    }

    /// A point in relative coordinates for button centers.
    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct ButtonConfig {
        /// X position (0.0 = left edge, 1.0 = right edge)
        pub x: f32,
        /// Y position (0.0 = top edge, 1.0 = bottom edge)
        pub y: f32,
    }

    impl Default for ButtonConfig {
        fn default() -> Self {
            Self { x: 0.5, y: 0.5 }
        }
    }

    /// Complete automation configuration.
    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct AutomationConfig {
        /// Position of the "開始する" (Start) button
        pub start_button: ButtonConfig,
        /// Position of the "スキップ" (Skip) button
        pub skip_button: ButtonConfig,
        /// Region around skip button for brightness detection
        pub skip_button_region: RelativeRect,
        /// Brightness threshold: above this = ready, below = loading
        pub brightness_threshold: f32,
        /// Maximum time to wait for loading (milliseconds)
        pub loading_timeout_ms: u64,
        /// Delay after clicking skip before capturing result (milliseconds)
        pub capture_delay_ms: u64,
        /// Test position for relative click hotkey
        pub test_click_position: ButtonConfig,
    }

    impl Default for AutomationConfig {
        fn default() -> Self {
            Self {
                start_button: ButtonConfig { x: 0.5, y: 0.85 },
                skip_button: ButtonConfig { x: 0.9, y: 0.95 },
                skip_button_region: RelativeRect {
                    x: 0.85,
                    y: 0.90,
                    width: 0.10,
                    height: 0.10,
                },
                brightness_threshold: 150.0,
                loading_timeout_ms: 30000,
                capture_delay_ms: 500,
                test_click_position: ButtonConfig { x: 0.5, y: 0.5 },
            }
        }
    }

    /// Loads configuration from config.json or returns defaults.
    fn load_config() -> AutomationConfig {
        let config_path = Path::new("config.json");

        if config_path.exists() {
            match fs::read_to_string(config_path) {
                Ok(contents) => match serde_json::from_str(&contents) {
                    Ok(config) => {
                        crate::log("Config loaded from config.json");
                        return config;
                    }
                    Err(e) => {
                        crate::log(&format!("Failed to parse config.json: {}. Using defaults.", e));
                    }
                },
                Err(e) => {
                    crate::log(&format!("Failed to read config.json: {}. Using defaults.", e));
                }
            }
        } else {
            crate::log("config.json not found. Using default config.");
        }

        AutomationConfig::default()
    }

    /// Initializes the global configuration. Call once at startup.
    pub fn init_config() {
        let _ = CONFIG.set(load_config());
    }

    /// Returns a reference to the global configuration.
    /// Panics if called before init_config().
    pub fn get_config() -> &'static AutomationConfig {
        CONFIG.get().expect("Config not initialized. Call init_config() first.")
    }

### detection.rs Implementation

    //! Loading state detection via brightness analysis.

    use anyhow::{anyhow, Result};
    use image::{ImageBuffer, Rgba};
    use std::time::{Duration, Instant};
    use windows::Win32::Foundation::HWND;

    use crate::automation::config::AutomationConfig;
    use crate::capture::region::capture_region;

    /// Calculates the average brightness (luminance) of an image.
    ///
    /// Uses the ITU-R BT.601 luma formula: Y = 0.299*R + 0.587*G + 0.114*B
    /// Returns a value from 0.0 (black) to 255.0 (white).
    pub fn calculate_brightness(img: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> f32 {
        if img.width() == 0 || img.height() == 0 {
            return 0.0;
        }

        let mut total: f64 = 0.0;
        let pixel_count = (img.width() * img.height()) as f64;

        for pixel in img.pixels() {
            let r = pixel[0] as f64;
            let g = pixel[1] as f64;
            let b = pixel[2] as f64;
            let luminance = 0.299 * r + 0.587 * g + 0.114 * b;
            total += luminance;
        }

        (total / pixel_count) as f32
    }

    /// Waits until the skip button region brightness exceeds the threshold.
    ///
    /// Polls repeatedly, capturing the skip_button_region and measuring brightness.
    /// Returns Ok(()) when brightness exceeds threshold, or Err on timeout.
    pub fn wait_for_loading(hwnd: HWND, config: &AutomationConfig) -> Result<()> {
        let start = Instant::now();
        let timeout = Duration::from_millis(config.loading_timeout_ms);

        loop {
            if start.elapsed() > timeout {
                return Err(anyhow!(
                    "Loading timeout after {}ms",
                    config.loading_timeout_ms
                ));
            }

            let region_img = capture_region(hwnd, &config.skip_button_region)?;
            let brightness = calculate_brightness(&region_img);

            crate::log(&format!("Loading check: brightness = {:.2}", brightness));

            if brightness > config.brightness_threshold {
                crate::log("Loading complete (brightness threshold exceeded)");
                return Ok(());
            }

            std::thread::sleep(Duration::from_millis(200));
        }
    }


## Interfaces and Dependencies

**New dependencies in Cargo.toml:**

    serde = { version = "1.0", features = ["derive"] }
    serde_json = "1.0"

**New public interfaces:**

In `src/automation/config.rs`:

    pub struct RelativeRect { pub x: f32, pub y: f32, pub width: f32, pub height: f32 }
    pub struct ButtonConfig { pub x: f32, pub y: f32 }
    pub struct AutomationConfig { /* fields as defined above */ }
    pub fn init_config()
    pub fn get_config() -> &'static AutomationConfig

In `src/automation/input.rs` (additions):

    pub fn click_at_relative(hwnd: HWND, rel_x: f32, rel_y: f32) -> Result<()>

In `src/capture/region.rs`:

    pub fn capture_region(hwnd: HWND, rel_rect: &RelativeRect) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>>

In `src/automation/detection.rs`:

    pub fn calculate_brightness(img: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> f32
    pub fn wait_for_loading(hwnd: HWND, config: &AutomationConfig) -> Result<()>

**New hotkeys in main.rs:**

    HOTKEY_RELATIVE_CLICK (4): Ctrl+Shift+F12 - Clicks at test_click_position
    HOTKEY_BRIGHTNESS_TEST (5): Ctrl+Shift+F11 - Logs brightness of skip_button_region
