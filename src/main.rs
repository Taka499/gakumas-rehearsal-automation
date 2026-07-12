//! Gakumas Screenshot Tool
//!
//! A Windows system tray application that captures screenshots of the
//! gakumas.exe game window using the Windows Graphics Capture API.

// Hide console window on Windows for GUI mode — but NOT for the test harness:
// a GUI-subsystem test binary never attaches to an interactive console, so
// `cargo test` run from PowerShell shows no test output at all (it only shows
// up when stdout is a pipe, e.g. under CI or an agent shell).
#![cfg_attr(not(test), windows_subsystem = "windows")]

mod analysis;
mod automation;
mod calibration;
mod capture;
mod feedback;
mod gui;
mod ocr;
mod paths;
mod update;

use anyhow::{anyhow, Result};
use chrono::Local;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT,
};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow, DispatchMessageW,
    GetCursorPos, GetMessageW, InsertMenuW, LoadIconW, PostQuitMessage, RegisterClassW,
    SetForegroundWindow, TrackPopupMenu, TranslateMessage, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT,
    IDI_APPLICATION, MF_BYPOSITION, MF_SEPARATOR, MF_STRING, MSG, TPM_BOTTOMALIGN, TPM_LEFTALIGN,
    TPM_RIGHTBUTTON, WM_COMMAND, WM_DESTROY, WM_HOTKEY, WM_LBUTTONDBLCLK, WM_RBUTTONUP, WM_USER,
    WNDCLASSW, WS_OVERLAPPEDWINDOW,
};

const HOTKEY_ID: i32 = 1;
const HOTKEY_CLICK_TEST: i32 = 2;
const HOTKEY_SENDINPUT_TEST: i32 = 3;
const HOTKEY_RELATIVE_CLICK: i32 = 4;
const HOTKEY_BRIGHTNESS_TEST: i32 = 5;
const HOTKEY_AUTOMATION: i32 = 6;
const HOTKEY_ABORT: i32 = 7;
const WM_TRAYICON: u32 = WM_USER + 1;

// Menu item IDs
const MENU_CALIBRATE: usize = 1001;
const MENU_PREVIEW: usize = 1002;
const MENU_TEST_OCR: usize = 1004;
const MENU_CAPTURE_START_REF: usize = 1005;
const MENU_CAPTURE_SKIP_REF: usize = 1006;
const MENU_CAPTURE_END_REF: usize = 1007;
const MENU_GENERATE_CHARTS: usize = 1008;
const MENU_EXIT: usize = 1003;

/// Path to the current session log file (set during automation runs).
static SESSION_LOG_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Sets or clears the session log path. When set, `log()` writes to both
/// the global log file and the session log file.
pub fn set_session_log(path: Option<PathBuf>) {
    if let Ok(mut p) = SESSION_LOG_PATH.lock() {
        *p = path;
    }
}

/// Logs a message to console, global log file, and optionally the session log file.
pub fn log(msg: &str) {
    let timestamp = Local::now().format("%H:%M:%S%.3f");
    let line = format!("[{}] {}\n", timestamp, msg);
    print!("{}", line);

    // Write to global log
    let log_path = paths::get_logs_dir().join("gakumas_screenshot.log");
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        let _ = file.write_all(line.as_bytes());
    }

    // Write to session log if active
    if let Ok(guard) = SESSION_LOG_PATH.lock() {
        if let Some(ref session_path) = *guard {
            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(session_path)
            {
                let _ = file.write_all(line.as_bytes());
            }
        }
    }
}

static mut MAIN_HWND: HWND = HWND(std::ptr::null_mut());

fn main() -> Result<()> {
    // Set up panic hook to log panics
    std::panic::set_hook(Box::new(|panic_info| {
        let msg = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };
        let location = if let Some(loc) = panic_info.location() {
            format!(" at {}:{}:{}", loc.file(), loc.line(), loc.column())
        } else {
            String::new()
        };
        // Try to log even if paths module isn't initialized
        let log_msg = format!("[PANIC]{} {}\n", location, msg);
        eprintln!("{}", log_msg);
        if let Ok(exe_dir) = std::env::current_exe().map(|p| p.parent().unwrap().to_path_buf()) {
            let log_path = exe_dir.join("logs").join("gakumas_screenshot.log");
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
            {
                use std::io::Write;
                let _ = file.write_all(log_msg.as_bytes());
            }
        }
    }));

    unsafe {
        windows::Win32::System::WinRT::RoInitialize(
            windows::Win32::System::WinRT::RO_INIT_MULTITHREADED,
        )?
    };

    // Ensure output directories exist
    paths::ensure_directories()?;

    // Remove leftovers from a previous self-update (<exe>.old, resources.old).
    update::install::cleanup_old_files();

    // Ensure Tesseract is available (extracts from embedded zip if needed)
    if let Err(e) = ocr::ensure_tesseract() {
        log(&format!("Warning: Failed to setup Tesseract: {}", e));
        log("OCR features may not work correctly.");
    }

    // Load configuration
    automation::init_config();

    // Check if developer mode is enabled
    let config = automation::get_config();
    if config.developer_mode {
        // Run as system tray application (developer mode)
        log("Developer mode enabled - running tray application");
        run_tray_app()
    } else {
        // Run GUI application (normal mode)
        log("Starting GUI application...");
        match gui::run_gui() {
            Ok(()) => {
                log("GUI application exited normally");
                Ok(())
            }
            Err(e) => {
                log(&format!("GUI error: {}", e));
                Err(anyhow!("GUI error: {}", e))
            }
        }
    }
}

/// Runs the main system tray application with hotkey handling.
fn run_tray_app() -> Result<()> {
    // Create hidden window for message handling
    let hwnd = create_message_window()?;
    unsafe { MAIN_HWND = hwnd };

    // Add system tray icon
    add_tray_icon(hwnd)?;

    // Register global hotkey: Ctrl+Shift+S for screenshot
    unsafe {
        RegisterHotKey(
            hwnd,
            HOTKEY_ID,
            MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT,
            0x53, // 'S' key
        )?;
    }

    // Register global hotkey: Ctrl+Shift+F9 for PostMessage click test
    unsafe {
        RegisterHotKey(
            hwnd,
            HOTKEY_CLICK_TEST,
            MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT,
            0x78, // VK_F9
        )?;
    }

    // Register global hotkey: Ctrl+Shift+F10 for SendInput click test
    unsafe {
        RegisterHotKey(
            hwnd,
            HOTKEY_SENDINPUT_TEST,
            MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT,
            0x79, // VK_F10
        )?;
    }

    // Register global hotkey: Ctrl+Shift+F12 for relative click test
    unsafe {
        RegisterHotKey(
            hwnd,
            HOTKEY_RELATIVE_CLICK,
            MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT,
            0x7B, // VK_F12
        )?;
    }

    // Register global hotkey: Ctrl+Shift+F11 for brightness test
    unsafe {
        RegisterHotKey(
            hwnd,
            HOTKEY_BRIGHTNESS_TEST,
            MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT,
            0x7A, // VK_F11
        )?;
    }

    // Register global hotkey: Ctrl+Shift+A for automation
    unsafe {
        RegisterHotKey(
            hwnd,
            HOTKEY_AUTOMATION,
            MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT,
            0x41, // 'A' key
        )?;
    }

    // Register global hotkey: Ctrl+Shift+Q for abort
    unsafe {
        RegisterHotKey(
            hwnd,
            HOTKEY_ABORT,
            MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT,
            0x51, // 'Q' key
        )?;
    }

    log("Gakumas Screenshot Tool started");
    log("Hotkey: Ctrl+Shift+S (screenshot)");
    log("Hotkey: Ctrl+Shift+A (start automation)");
    log("Hotkey: Ctrl+Shift+Q (abort automation)");
    log("Hotkey: Ctrl+Shift+F9 (PostMessage click test)");
    log("Hotkey: Ctrl+Shift+F10 (SendInput click test - MOVES CURSOR)");
    log("Hotkey: Ctrl+Shift+F11 (brightness test)");
    log("Hotkey: Ctrl+Shift+F12 (relative click test - MOVES CURSOR)");
    log("Right-click tray icon to exit");

    // Message loop
    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Cleanup
        let _ = UnregisterHotKey(hwnd, HOTKEY_ID);
        let _ = UnregisterHotKey(hwnd, HOTKEY_CLICK_TEST);
        let _ = UnregisterHotKey(hwnd, HOTKEY_SENDINPUT_TEST);
        let _ = UnregisterHotKey(hwnd, HOTKEY_RELATIVE_CLICK);
        let _ = UnregisterHotKey(hwnd, HOTKEY_BRIGHTNESS_TEST);
        let _ = UnregisterHotKey(hwnd, HOTKEY_AUTOMATION);
        let _ = UnregisterHotKey(hwnd, HOTKEY_ABORT);
        remove_tray_icon(hwnd);
        let _ = DestroyWindow(hwnd);
    }

    Ok(())
}

fn create_message_window() -> Result<HWND> {
    unsafe {
        let hinstance = GetModuleHandleW(None)?;
        let class_name = w!("GakumasScreenshotClass");

        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            hInstance: hinstance.into(),
            lpszClassName: class_name,
            ..Default::default()
        };

        let atom = RegisterClassW(&wc);
        if atom == 0 {
            return Err(anyhow!("Failed to register window class"));
        }

        let hwnd = CreateWindowExW(
            Default::default(),
            class_name,
            w!("Gakumas Screenshot"),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            None,
            None,
            hinstance,
            None,
        )?;

        Ok(hwnd)
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_HOTKEY => {
                let hotkey_id = wparam.0 as i32;

                // Check if this is a calibration hotkey
                if hotkey_id >= calibration::HOTKEY_CAL_F1
                    && hotkey_id <= calibration::HOTKEY_CAL_ENTER
                {
                    if let Err(e) = calibration::handle_calibration_hotkey(hotkey_id) {
                        log(&format!("Calibration error: {}", e));
                    }
                    return LRESULT(0);
                }

                // Normal hotkeys
                if hotkey_id == HOTKEY_ID {
                    log("Hotkey pressed! Capturing...");
                    match capture::capture_gakumas() {
                        Ok(path) => log(&format!("Screenshot saved: {}", paths::relative_display(&path))),
                        Err(e) => log(&format!("Capture failed: {}", e)),
                    }
                } else if hotkey_id == HOTKEY_CLICK_TEST {
                    log("PostMessage click test hotkey pressed!");
                    match automation::test_postmessage_click() {
                        Ok(()) => log("PostMessage click test completed"),
                        Err(e) => log(&format!("PostMessage click test failed: {}", e)),
                    }
                } else if hotkey_id == HOTKEY_SENDINPUT_TEST {
                    log("SendInput click test hotkey pressed!");
                    match automation::test_sendinput_click() {
                        Ok(()) => log("SendInput click test completed"),
                        Err(e) => log(&format!("SendInput click test failed: {}", e)),
                    }
                } else if hotkey_id == HOTKEY_RELATIVE_CLICK {
                    log("Relative click test hotkey pressed!");
                    let config = automation::get_config();
                    match capture::find_gakumas_window() {
                        Ok(game_hwnd) => {
                            let pos = &config.test_click_position;
                            match automation::click_at_relative(game_hwnd, pos.x, pos.y) {
                                Ok(()) => log("Relative click test completed"),
                                Err(e) => log(&format!("Relative click test failed: {}", e)),
                            }
                        }
                        Err(e) => log(&format!("Could not find game window: {}", e)),
                    }
                } else if hotkey_id == HOTKEY_BRIGHTNESS_TEST {
                    log("Brightness test hotkey pressed!");
                    let config = automation::get_config();
                    match capture::find_gakumas_window() {
                        Ok(game_hwnd) => {
                            log("Capturing region for brightness test...");
                            match automation::measure_region_brightness(game_hwnd, config) {
                                Ok(brightness) => {
                                    log(&format!("Region brightness: {:.2}", brightness));
                                    log(&format!(
                                        "Threshold is {:.2} (current {} threshold)",
                                        config.brightness_threshold,
                                        if brightness > config.brightness_threshold {
                                            "EXCEEDS"
                                        } else {
                                            "BELOW"
                                        }
                                    ));
                                }
                                Err(e) => log(&format!("Brightness test failed: {}", e)),
                            }
                        }
                        Err(e) => log(&format!("Could not find game window: {}", e)),
                    }
                } else if hotkey_id == HOTKEY_AUTOMATION {
                    log("Automation hotkey pressed!");
                    if automation::is_automation_running() {
                        log("Automation is already running");
                    } else {
                        match automation::start_automation(None) {
                            Ok(()) => {} // Logging handled by start_automation
                            Err(e) => log(&format!("Failed to start automation: {}", e)),
                        }
                    }
                } else if hotkey_id == HOTKEY_ABORT {
                    if automation::is_automation_running() {
                        log("Abort hotkey pressed - stopping automation");
                        automation::request_abort();
                    } else {
                        log("Abort hotkey pressed but no automation running");
                    }
                }
                LRESULT(0)
            }
            WM_TRAYICON => {
                let event = (lparam.0 & 0xFFFF) as u32;
                match event {
                    WM_RBUTTONUP => {
                        show_context_menu(hwnd);
                    }
                    WM_LBUTTONDBLCLK => {
                        // Double-click could trigger capture or show status
                        log("Tray icon double-clicked");
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_COMMAND => {
                let cmd = wparam.0 & 0xFFFF;
                if cmd == MENU_CALIBRATE {
                    log("Calibration requested");
                    if let Err(e) = calibration::start_calibration(hwnd) {
                        log(&format!("Failed to start calibration: {}", e));
                    }
                } else if cmd == MENU_PREVIEW {
                    log("Preview requested");
                    if let Err(e) = calibration::show_preview_once() {
                        log(&format!("Failed to show preview: {}", e));
                    }
                } else if cmd == MENU_TEST_OCR {
                    log("Test OCR requested");
                    test_ocr();
                } else if cmd == MENU_CAPTURE_START_REF {
                    log("Capture Start Reference requested");
                    capture_start_reference();
                } else if cmd == MENU_CAPTURE_SKIP_REF {
                    log("Capture Skip Reference requested");
                    capture_skip_reference();
                } else if cmd == MENU_CAPTURE_END_REF {
                    log("Capture End Reference requested");
                    capture_end_reference();
                } else if cmd == MENU_GENERATE_CHARTS {
                    log("Generate Charts requested");
                    generate_charts();
                } else if cmd == MENU_EXIT {
                    log("Exit requested");
                    PostQuitMessage(0);
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

fn add_tray_icon(hwnd: HWND) -> Result<()> {
    unsafe {
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: WM_TRAYICON,
            hIcon: LoadIconW(None, IDI_APPLICATION)?,
            ..Default::default()
        };

        // Set tooltip
        let tip = "Gakumas Screenshot (Ctrl+Shift+S)";
        let tip_wide: Vec<u16> = tip.encode_utf16().chain(std::iter::once(0)).collect();
        let len = tip_wide.len().min(nid.szTip.len());
        nid.szTip[..len].copy_from_slice(&tip_wide[..len]);

        if !Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
            return Err(anyhow!("Failed to add tray icon"));
        }

        Ok(())
    }
}

fn remove_tray_icon(hwnd: HWND) {
    unsafe {
        let nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            ..Default::default()
        };
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

fn show_context_menu(hwnd: HWND) {
    unsafe {
        let menu = CreatePopupMenu().unwrap();

        // Add menu items (inserted in reverse order since position 0)
        let exit_text = w!("Exit");
        let _ = InsertMenuW(menu, 0, MF_BYPOSITION | MF_STRING, MENU_EXIT, exit_text);

        // Separator
        let _ = InsertMenuW(menu, 0, MF_BYPOSITION | MF_SEPARATOR, 0, None);

        let preview_text = w!("Preview Regions");
        let _ = InsertMenuW(menu, 0, MF_BYPOSITION | MF_STRING, MENU_PREVIEW, preview_text);

        let test_ocr_text = w!("Test OCR");
        let _ = InsertMenuW(menu, 0, MF_BYPOSITION | MF_STRING, MENU_TEST_OCR, test_ocr_text);

        let start_ref_text = w!("Capture Start Reference");
        let _ = InsertMenuW(menu, 0, MF_BYPOSITION | MF_STRING, MENU_CAPTURE_START_REF, start_ref_text);

        let skip_ref_text = w!("Capture Skip Reference");
        let _ = InsertMenuW(menu, 0, MF_BYPOSITION | MF_STRING, MENU_CAPTURE_SKIP_REF, skip_ref_text);

        let end_ref_text = w!("Capture End Reference");
        let _ = InsertMenuW(menu, 0, MF_BYPOSITION | MF_STRING, MENU_CAPTURE_END_REF, end_ref_text);

        let calibrate_text = w!("Calibrate Regions...");
        let _ = InsertMenuW(menu, 0, MF_BYPOSITION | MF_STRING, MENU_CALIBRATE, calibrate_text);

        // Separator before analysis
        let _ = InsertMenuW(menu, 0, MF_BYPOSITION | MF_SEPARATOR, 0, None);

        let generate_charts_text = w!("Generate Charts");
        let _ = InsertMenuW(menu, 0, MF_BYPOSITION | MF_STRING, MENU_GENERATE_CHARTS, generate_charts_text);

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);

        // Required for the menu to work properly
        let _ = SetForegroundWindow(hwnd);

        let _ = TrackPopupMenu(
            menu,
            TPM_BOTTOMALIGN | TPM_LEFTALIGN | TPM_RIGHTBUTTON,
            pt.x,
            pt.y,
            0,
            hwnd,
            None,
        );

        let _ = DestroyMenu(menu);
    }
}

/// Tests OCR on the current game window screenshot
fn test_ocr() {
    // Find game window
    let game_hwnd = match capture::find_gakumas_window() {
        Ok(hwnd) => hwnd,
        Err(e) => {
            log(&format!("Could not find game window: {}", e));
            return;
        }
    };

    // Capture screenshot
    log("Capturing screenshot for OCR...");
    let img = match capture::capture_window_to_image(game_hwnd) {
        Ok(img) => img,
        Err(e) => {
            log(&format!("Failed to capture screenshot: {}", e));
            return;
        }
    };

    // Run OCR pipeline
    log("Running OCR...");
    let config = automation::get_config();
    let threshold = config.ocr_threshold;

    match ocr::ocr_screenshot(&img, &config.ocr_regions()) {
        Ok(readout) => {
            let scores = readout.scores;
            log("OCR succeeded!");
            log(&format!("Stage 1: {:?} (total={:?}, bonus={:?})", scores[0], readout.totals[0], readout.bonuses[0]));
            log(&format!("Stage 2: {:?} (total={:?}, bonus={:?})", scores[1], readout.totals[1], readout.bonuses[1]));
            log(&format!("Stage 3: {:?} (total={:?}, bonus={:?})", scores[2], readout.totals[2], readout.bonuses[2]));

            // Calculate totals
            let total: u32 = scores.iter().flat_map(|s| s.iter()).sum();
            log(&format!("Total: {}", total));
        }
        Err(e) => {
            log(&format!("OCR failed: {}", e));

            // Try to save preprocessed image for debugging
            let preprocessed = ocr::threshold_bright_pixels(&img, threshold);
            if let Err(save_err) = preprocessed.save("debug_preprocessed.png") {
                log(&format!("Could not save debug image: {}", save_err));
            } else {
                log("Saved debug_preprocessed.png for inspection");
            }
        }
    }
}

/// Captures the current Start button region as a reference image for histogram comparison.
/// The game should be showing the rehearsal start page with the "開始する" button when this is called.
fn capture_start_reference() {
    // Find game window
    let game_hwnd = match capture::find_gakumas_window() {
        Ok(hwnd) => hwnd,
        Err(e) => {
            log(&format!("Could not find game window: {}", e));
            return;
        }
    };

    let config = automation::get_config();

    // Determine save path (use assets directory)
    let ref_path = paths::get_rehearsal_template_dir().join("start_button_ref.png");

    // Capture and save
    match automation::save_start_button_reference(game_hwnd, config, &ref_path) {
        Ok(()) => {
            log(&format!(
                "Start button reference saved to {}",
                paths::relative_display(&ref_path)
            ));
            log("The automation will now use this image to detect when the Start button (rehearsal page) appears.");
        }
        Err(e) => {
            log(&format!("Failed to capture Start reference: {}", e));
        }
    }
}

/// Captures the current Skip button region as a reference image for histogram comparison.
/// The game should be showing the Skip button (during rehearsal) when this is called.
fn capture_skip_reference() {
    // Find game window
    let game_hwnd = match capture::find_gakumas_window() {
        Ok(hwnd) => hwnd,
        Err(e) => {
            log(&format!("Could not find game window: {}", e));
            return;
        }
    };

    let config = automation::get_config();

    // Determine save path (use assets directory)
    let ref_path = paths::get_rehearsal_template_dir().join("skip_button_ref.png");

    // Capture and save
    match automation::save_skip_button_reference(game_hwnd, config, &ref_path) {
        Ok(()) => {
            log(&format!(
                "Skip button reference saved to {}",
                paths::relative_display(&ref_path)
            ));
            log("The automation will now use this image to detect when the Skip button appears.");
        }
        Err(e) => {
            log(&format!("Failed to capture Skip reference: {}", e));
        }
    }
}

/// Captures the current End button region as a reference image for histogram comparison.
/// The game should be showing the result page with the "終了" button when this is called.
fn capture_end_reference() {
    // Find game window
    let game_hwnd = match capture::find_gakumas_window() {
        Ok(hwnd) => hwnd,
        Err(e) => {
            log(&format!("Could not find game window: {}", e));
            return;
        }
    };

    let config = automation::get_config();

    // Determine save path (use assets directory)
    let ref_path = paths::get_rehearsal_template_dir().join("end_button_ref.png");

    // Capture and save
    match automation::save_end_button_reference(game_hwnd, config, &ref_path) {
        Ok(()) => {
            log(&format!(
                "End button reference saved to {}",
                paths::relative_display(&ref_path)
            ));
            log("The automation will now use this image to detect when the result page appears.");
        }
        Err(e) => {
            log(&format!("Failed to capture End reference: {}", e));
        }
    }
}

/// Generates statistics charts from the results CSV file.
fn generate_charts() {
    match analysis::generate_analysis() {
        Ok((chart_paths, json_path)) => {
            log("Charts generated successfully!");
            for path in &chart_paths {
                log(&format!("  Chart: {}", paths::relative_display(path)));
            }
            log(&format!("  Statistics: {}", paths::relative_display(&json_path)));
        }
        Err(e) => {
            log(&format!("Failed to generate charts: {}", e));
        }
    }
}
