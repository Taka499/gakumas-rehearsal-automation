# Extract capture/ and automation/ Modules from main.rs

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

This document must be maintained in accordance with `docs/PLANS.md`.


## Purpose / Big Picture

The single-file `src/main.rs` has grown to 860 lines and will grow significantly more as OCR, statistics, and chart features are added. This refactoring extracts logically separate concerns into modules, making the codebase easier to navigate, test, and extend. After this change, a developer can find window discovery code in `src/capture/window.rs`, screenshot logic in `src/capture/screenshot.rs`, and input simulation in `src/automation/input.rs` without scrolling through hundreds of unrelated lines.

The application behavior remains identical. Users will still press Ctrl+Shift+S to capture screenshots, Ctrl+Shift+F9/F10 for click tests, and right-click the tray icon to exit. The only observable difference is cleaner source organization.


## Progress

- [x] (2026-01-13 10:05) Create `src/capture/mod.rs` with module declarations
- [x] (2026-01-13 10:00) Create `src/capture/window.rs` with `find_gakumas_window()` and `get_client_area_info()`
- [x] (2026-01-13 10:02) Create `src/capture/screenshot.rs` with `capture_gakumas()` and D3D11 helpers
- [x] (2026-01-13 10:06) Create `src/automation/mod.rs` with module declarations
- [x] (2026-01-13 10:04) Create `src/automation/input.rs` with click test functions
- [x] (2026-01-13 10:07) Update `src/main.rs` to import and use the new modules
- [x] (2026-01-13 10:08) Verify build succeeds with `cargo build --release`
- [x] (2026-01-13 10:15) Test screenshot functionality (Ctrl+Shift+S) - User confirmed working
- [x] (2026-01-13 10:15) Test SendInput click (Ctrl+Shift+F10) - User confirmed working
- [x] (2026-01-13 10:15) Test tray icon exit - User confirmed working
- [x] (2026-01-13 10:16) Commit changes


## Surprises & Discoveries

- Observation: Total line count increased from 860 to 968 lines (+12.5%)
  Evidence: `wc -l` output shows main.rs at 284 lines (down from 860), but module files add overhead
  Reason: Added module documentation comments, function doc comments, and module declaration files. This is acceptable trade-off for better organization and documentation.

- Observation: Initial build had 2 unused import warnings
  Evidence: `HWND` unused in input.rs, `get_client_area_info` re-exported but not used by main.rs
  Resolution: Removed unused HWND import; kept get_client_area_info re-export with `#[allow(unused_imports)]` for future use


## Decision Log

- Decision: Keep `log()` function in `main.rs` rather than creating a separate `util.rs` module.
  Rationale: The function is only 12 lines and used throughout. Creating a module for one small function adds complexity. If more utilities emerge, we can extract later.
  Date/Author: 2026-01-13

- Decision: Pass `log` function as a closure/function pointer to modules rather than making it `pub` and importing from main.
  Rationale: This avoids circular dependencies and keeps modules self-contained. Each module function that needs logging will accept an optional logging callback. For simplicity in this first refactor, we will make `log` public in main and have modules import it via `crate::log`.
  Date/Author: 2026-01-13

- Decision: Move the `GAKUMAS_PROCESS_NAME` constant to `capture/window.rs` since it is only used there.
  Rationale: Keeps related constants with related code. The hotkey IDs stay in main.rs since they are message-loop specific.
  Date/Author: 2026-01-13


## Outcomes & Retrospective

**Status: COMPLETED**

The refactoring successfully extracted the monolithic 860-line main.rs into a modular structure:

- `main.rs` now contains only application shell code (284 lines)
- `capture/` module encapsulates window discovery and screenshot functionality
- `automation/` module encapsulates input simulation for future UI automation

**Achievements:**
1. Clear separation of concerns - each module has a single responsibility
2. Better documentation - modules and functions now have doc comments
3. Easier extensibility - new features (OCR, statistics) can be added as separate modules
4. All existing functionality preserved and tested working

**Lessons Learned:**
1. Module overhead (mod.rs files, re-exports) adds some lines but improves organization
2. The `crate::log` pattern works well for cross-module logging without circular dependencies
3. Keeping re-exports with `#[allow(unused_imports)]` for future use is acceptable

**Next Steps:**
- Phase 2 (OCR) can now add `src/ocr/` module cleanly
- Phase 3 (Automation Loop) can extend `src/automation/` with state machine
- Phase 4 (Statistics) can add `src/analysis/` module


## Context and Orientation

The project is a Windows screenshot tool for the game "gakumas.exe". It currently consists of:

- `src/main.rs` (860 lines): Contains all application logic
- `Cargo.toml`: Dependencies including `windows` crate v0.58, `image`, `chrono`, `anyhow`
- `docs/ROADMAP_AUTOMATION.md`: Future feature plans
- `CLAUDE.md`: Development guidance

The main.rs file contains these logical groups:

1. **Window discovery** (lines 491-636): `find_gakumas_window()`, `get_client_area_info()` - Locates the game window by enumerating all windows and matching the process name "gakumas.exe".

2. **Screenshot capture** (lines 304-489, 638-687): `capture_gakumas()`, `create_d3d11_device()`, `create_direct3d_device()`, `create_capture_item()` - Uses Windows Graphics Capture API to capture the game window, crops to client area, converts BGRA to RGBA, saves as PNG.

3. **Input simulation** (lines 689-859): `test_postmessage_click()`, `test_sendinput_click()` - Test functions for simulating mouse clicks using PostMessage (doesn't work with the game) and SendInput (works).

4. **Application shell** (lines 51-302): `main()`, `create_message_window()`, `window_proc()`, `add_tray_icon()`, `remove_tray_icon()`, `show_context_menu()`, `log()` - Message loop, hotkey handling, system tray.


## Plan of Work

The refactoring proceeds in this order to minimize risk and allow incremental testing:

**Step 1: Create capture/window.rs**

Create `src/capture/window.rs` containing:
- The `find_gakumas_window()` function (including the `EnumData` struct and `enum_callback`)
- The `get_client_area_info()` function
- Required imports from `windows` crate

The function signatures remain identical. The only change is adding `pub` visibility and importing `log` from the crate root.

**Step 2: Create capture/screenshot.rs**

Create `src/capture/screenshot.rs` containing:
- The `capture_gakumas()` function
- Helper functions: `create_d3d11_device()`, `create_direct3d_device()`, `create_capture_item()`
- Required imports

This module will import `find_gakumas_window` and `get_client_area_info` from `super::window`.

**Step 3: Create capture/mod.rs**

Create `src/capture/mod.rs` that declares and re-exports the submodules:
- `pub mod window;`
- `pub mod screenshot;`
- Re-export key functions for convenient access

**Step 4: Create automation/input.rs**

Create `src/automation/input.rs` containing:
- `test_postmessage_click()` function
- `test_sendinput_click()` function
- Required imports

This module will import `find_gakumas_window` from `crate::capture::window`.

**Step 5: Create automation/mod.rs**

Create `src/automation/mod.rs` that declares the submodule:
- `pub mod input;`

**Step 6: Update main.rs**

Modify `src/main.rs` to:
- Add `mod capture;` and `mod automation;` declarations
- Make `log()` function `pub` so modules can import it
- Remove the extracted functions
- Update `window_proc` to call `capture::screenshot::capture_gakumas()` and `automation::input::test_*` functions
- Keep all tray icon, message window, and message loop code


## Concrete Steps

All commands run from the repository root: `C:\Work\GitRepos\gakumas-rehearsal-automation`

**1. Create directory structure:**

    mkdir src\capture
    mkdir src\automation

**2. Create src/capture/window.rs**

This file will contain approximately 150 lines including imports, `find_gakumas_window()`, and `get_client_area_info()`.

**3. Create src/capture/screenshot.rs**

This file will contain approximately 220 lines including imports, `capture_gakumas()`, and the three D3D11 helper functions.

**4. Create src/capture/mod.rs**

    pub mod window;
    pub mod screenshot;

    pub use screenshot::capture_gakumas;
    pub use window::{find_gakumas_window, get_client_area_info};

**5. Create src/automation/input.rs**

This file will contain approximately 180 lines including imports and both click test functions.

**6. Create src/automation/mod.rs**

    pub mod input;

    pub use input::{test_postmessage_click, test_sendinput_click};

**7. Update src/main.rs**

Remove the extracted functions and add module imports. The file should shrink from 860 lines to approximately 310 lines.

**8. Build and verify:**

    cargo build --release

Expected output: Compilation succeeds with no errors. Warnings about unused imports may appear and should be fixed.

**9. Run and test:**

    .\target\release\gakumas-rehearsal-automation.exe

With the game running:
- Press Ctrl+Shift+S: Should capture screenshot and save `gakumas_YYYYMMDD_HHMMSS.png`
- Press Ctrl+Shift+F10: Should move cursor to game center and click
- Right-click tray icon, select Exit: Should terminate cleanly


## Validation and Acceptance

The refactoring is complete when:

1. `cargo build --release` compiles without errors
2. The application starts and shows a system tray icon with tooltip "Gakumas Screenshot (Ctrl+Shift+S)"
3. With gakumas.exe running, pressing Ctrl+Shift+S creates a PNG file in the current directory
4. The captured image shows the game's client area (no title bar or window borders)
5. Pressing Ctrl+Shift+F10 moves the cursor to the game window center and clicks
6. Right-clicking the tray icon shows "Exit" menu, clicking it terminates the application
7. The log file `gakumas_screenshot.log` records all operations

Line count verification:
- `src/main.rs`: approximately 310 lines (down from 860)
- `src/capture/window.rs`: approximately 150 lines
- `src/capture/screenshot.rs`: approximately 220 lines
- `src/capture/mod.rs`: approximately 10 lines
- `src/automation/input.rs`: approximately 180 lines
- `src/automation/mod.rs`: approximately 5 lines


## Idempotence and Recovery

The refactoring is purely additive followed by deletions. If something goes wrong:

1. The original `main.rs` can be restored from git: `git checkout src/main.rs`
2. New files can be deleted: `rm -r src/capture src/automation`
3. The build will return to its original state

Each step can be tested independently. If the build fails after creating a module, check imports and visibility modifiers before proceeding.


## Artifacts and Notes

**Target file structure after refactoring:**

    src/
    ├── main.rs              (~310 lines)
    ├── capture/
    │   ├── mod.rs           (~10 lines)
    │   ├── window.rs        (~150 lines)
    │   └── screenshot.rs    (~220 lines)
    └── automation/
        ├── mod.rs           (~5 lines)
        └── input.rs         (~180 lines)

**Key imports each module needs:**

capture/window.rs:
- `windows::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT, TRUE}`
- `windows::Win32::UI::WindowsAndMessaging::*` (EnumWindows, GetClientRect, etc.)
- `windows::Win32::System::Threading::*` (OpenProcess, QueryFullProcessImageName)
- `windows::Win32::Graphics::Gdi::ClientToScreen`
- `anyhow::{anyhow, Result}`
- `crate::log`

capture/screenshot.rs:
- `windows::Graphics::Capture::*`
- `windows::Win32::Graphics::Direct3D11::*`
- `windows::Win32::System::WinRT::*`
- `image::{ImageBuffer, Rgba}`
- `chrono::Local`
- `anyhow::{anyhow, Context, Result}`
- `super::window::{find_gakumas_window, get_client_area_info}`
- `crate::log`

automation/input.rs:
- `windows::Win32::UI::Input::KeyboardAndMouse::*`
- `windows::Win32::UI::WindowsAndMessaging::{GetClientRect, SetForegroundWindow, GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN, PostMessageW, WM_MOUSEMOVE, WM_LBUTTONDOWN, WM_LBUTTONUP}`
- `windows::Win32::Graphics::Gdi::ClientToScreen`
- `windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM}`
- `anyhow::{anyhow, Result}`
- `crate::capture::window::find_gakumas_window`
- `crate::log`


## Interfaces and Dependencies

No new external dependencies. The `Cargo.toml` remains unchanged.

**Public interfaces after refactoring:**

In `src/capture/mod.rs`:

    pub fn capture_gakumas() -> Result<PathBuf>
    pub fn find_gakumas_window() -> Result<HWND>
    pub fn get_client_area_info(hwnd: HWND) -> Result<(RECT, POINT)>

In `src/automation/mod.rs`:

    pub fn test_postmessage_click() -> Result<()>
    pub fn test_sendinput_click() -> Result<()>

In `src/main.rs`:

    pub fn log(msg: &str)  // Used by all modules
