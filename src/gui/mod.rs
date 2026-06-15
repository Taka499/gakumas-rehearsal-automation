//! GUI module for the application.
//!
//! Provides a graphical interface using egui/eframe for user interaction.

pub mod render;
pub mod state;

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::time::Instant;

use eframe::egui::{self, TextureHandle, Vec2};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    TrayIcon, TrayIconBuilder,
};

use crate::automation::runner::{
    get_last_outcome, is_automation_running, resume_automation, start_automation, AutomationOutcome,
};
use crate::automation::state::request_abort;

use state::{AutomationStatus, GuiState};

/// Menu item IDs for tray menu
const MENU_SHOW_WINDOW: &str = "show_window";
const MENU_EXIT: &str = "exit";

/// Hotkey IDs
const HOTKEY_SCREENSHOT: i32 = 101;
const HOTKEY_ABORT: i32 = 102;

/// Global hotkey event signal (set by hotkey thread, read by GUI thread)
static HOTKEY_TRIGGERED: AtomicI32 = AtomicI32::new(0);

/// Embedded guide images (also copied to resources/guide/ by build.rs and package-release.ps1).
const GUIDE_IMAGE_1: &[u8] = include_bytes!("../../resources/guide/step1_contest_mode.png");
const GUIDE_IMAGE_2: &[u8] = include_bytes!("../../resources/guide/step2_rehearsal_page.png");

/// Main GUI application struct.
pub struct GuiApp {
    /// Application state.
    state: GuiState,
    /// Loaded guide image textures.
    guide_images: [Option<TextureHandle>; 2],
    /// Flag to track if images have been loaded.
    images_loaded: bool,
    /// Tray icon (kept alive for the duration of the app).
    #[allow(dead_code)]
    tray_icon: Option<TrayIcon>,
    /// Menu event receiver for tray menu (uses crossbeam-channel from tray-icon).
    menu_event_receiver: Option<tray_icon::menu::MenuEventReceiver>,
    /// Flag to request exit from tray menu.
    exit_requested: bool,
}

impl GuiApp {
    /// Create a new GUI application instance.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Configure fonts to support Japanese
        Self::setup_fonts(&cc.egui_ctx);

        // Set up tray icon
        let (tray_icon, menu_event_receiver) = Self::setup_tray_icon();

        let mut app = Self {
            state: GuiState::default(),
            guide_images: [None, None],
            images_loaded: false,
            tray_icon,
            menu_event_receiver,
            exit_requested: false,
        };
        // Populate the resume picker with interrupted sessions found on disk.
        app.scan_resumable_sessions();
        app
    }

    /// Set up the system tray icon with menu.
    fn setup_tray_icon() -> (Option<TrayIcon>, Option<tray_icon::menu::MenuEventReceiver>) {
        // Create menu
        let menu = Menu::new();
        let show_item = MenuItem::with_id(MENU_SHOW_WINDOW, "ウィンドウを表示", true, None);
        let exit_item = MenuItem::with_id(MENU_EXIT, "終了", true, None);

        if let Err(e) = menu.append(&show_item) {
            crate::log(&format!("Failed to add show menu item: {}", e));
        }
        if let Err(e) = menu.append(&exit_item) {
            crate::log(&format!("Failed to add exit menu item: {}", e));
        }

        // Create tray icon with default Windows icon
        let tray_icon = match TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Gakumas Rehearsal Automation")
            .build()
        {
            Ok(icon) => {
                crate::log("Tray icon created successfully");
                Some(icon)
            }
            Err(e) => {
                crate::log(&format!("Failed to create tray icon: {}", e));
                None
            }
        };

        // Get the menu event receiver
        let menu_event_receiver = Some(MenuEvent::receiver().clone());

        (tray_icon, menu_event_receiver)
    }

    /// Setup fonts with Japanese support.
    fn setup_fonts(ctx: &egui::Context) {
        let mut fonts = egui::FontDefinitions::default();

        // Try to load a Japanese system font
        // Common Japanese fonts on Windows: Yu Gothic, Meiryo, MS Gothic
        let font_paths = [
            "C:\\Windows\\Fonts\\YuGothM.ttc",  // Yu Gothic Medium
            "C:\\Windows\\Fonts\\meiryo.ttc",   // Meiryo
            "C:\\Windows\\Fonts\\msgothic.ttc", // MS Gothic
        ];

        let mut font_loaded = false;
        for font_path in &font_paths {
            if let Ok(font_data) = std::fs::read(font_path) {
                fonts.font_data.insert(
                    "japanese_font".to_owned(),
                    egui::FontData::from_owned(font_data).into(),
                );

                // Add Japanese font as first priority for proportional text
                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .insert(0, "japanese_font".to_owned());

                // Also add for monospace
                fonts
                    .families
                    .entry(egui::FontFamily::Monospace)
                    .or_default()
                    .insert(0, "japanese_font".to_owned());

                crate::log(&format!("Loaded Japanese font from: {}", font_path));
                font_loaded = true;
                break;
            }
        }

        if !font_loaded {
            crate::log("Warning: Could not load Japanese font. Text may not display correctly.");
        }

        ctx.set_fonts(fonts);
    }

    /// Load guide images as textures.
    fn load_images(&mut self, ctx: &egui::Context) {
        if self.images_loaded {
            return;
        }

        // Load image 1
        if let Ok(image) = image::load_from_memory(GUIDE_IMAGE_1) {
            let rgba = image.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let pixels = rgba.into_raw();
            let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
            self.guide_images[0] = Some(ctx.load_texture(
                "guide_image_1",
                color_image,
                egui::TextureOptions::LINEAR,
            ));
        }

        // Load image 2
        if let Ok(image) = image::load_from_memory(GUIDE_IMAGE_2) {
            let rgba = image.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let pixels = rgba.into_raw();
            let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
            self.guide_images[1] = Some(ctx.load_texture(
                "guide_image_2",
                color_image,
                egui::TextureOptions::LINEAR,
            ));
        }

        self.images_loaded = true;
    }

    /// Update automation status by polling the automation runner.
    fn update_automation_status(&mut self) {
        let is_running = is_automation_running();

        match &self.state.status {
            AutomationStatus::Running { total, start_time, .. } => {
                if !is_running {
                    // Automation finished - resolve the real outcome (success vs
                    // timeout/error vs abort) reported by the runner.
                    let session_path = crate::automation::runner::get_current_session_path()
                        .unwrap_or_else(|| crate::paths::get_output_dir());
                    self.state.latest_session_path = Some(session_path.clone());

                    self.state.status =
                        self.finalize_status(get_last_outcome(), *total, session_path);
                    // The just-finished session should immediately appear in (or
                    // drop out of) the resume picker.
                    self.scan_resumable_sessions();
                } else {
                    // Still running - update progress
                    let current = crate::automation::runner::get_current_iteration();
                    let state_desc = crate::automation::runner::get_current_state_description();
                    self.state.status = AutomationStatus::Running {
                        current,
                        total: *total,
                        state_description: state_desc,
                        start_time: *start_time,
                    };
                }
            }
            AutomationStatus::Idle | AutomationStatus::Completed { .. } |
            AutomationStatus::Aborted { .. } | AutomationStatus::Error { .. } => {
                // Check if automation started externally (shouldn't happen with GUI)
                if is_running && !matches!(self.state.status, AutomationStatus::Running { .. }) {
                    self.state.status = AutomationStatus::Running {
                        current: 0,
                        total: self.state.iterations,
                        state_description: "開始中...".to_string(),
                        start_time: Instant::now(),
                    };
                }
            }
        }
    }

    /// Builds the terminal `AutomationStatus` from the runner's outcome and
    /// generates analysis charts when at least one run produced data.
    fn finalize_status(
        &self,
        outcome: Option<AutomationOutcome>,
        running_total: u32,
        session_path: std::path::PathBuf,
    ) -> AutomationStatus {
        // Map the outcome to (completed, total, optional error message). If the
        // outcome is missing (shouldn't happen), fall back to treating it as an
        // error with no completed runs so we never falsely claim success.
        let (completed, total, error_msg, aborted) = match outcome {
            Some(AutomationOutcome::Completed { completed, total }) => {
                (completed, total, None, false)
            }
            Some(AutomationOutcome::Aborted { completed, total }) => {
                (completed, total, None, true)
            }
            Some(AutomationOutcome::Error { completed, total, message }) => {
                (completed, total, Some(message), false)
            }
            None => (0, running_total, Some("不明な理由で停止しました".to_string()), false),
        };

        // Generate charts whenever there is captured data to analyze, even on a
        // partial (timeout/abort) run, so the user still gets stats for what ran.
        if completed > 0 {
            crate::log("GUI: Auto-generating charts...");
            match crate::analysis::generate_analysis_for_session(&session_path) {
                Ok((chart_paths, json_path)) => {
                    crate::log(&format!(
                        "GUI: Charts generated: {} files, stats: {}",
                        chart_paths.len(),
                        json_path.display()
                    ));
                }
                Err(e) => {
                    crate::log(&format!("GUI: Failed to generate charts: {}", e));
                }
            }
        }

        match (error_msg, aborted) {
            (Some(message), _) => AutomationStatus::Error {
                completed,
                total,
                message,
                session_path: Some(session_path),
            },
            (None, true) => AutomationStatus::Aborted {
                completed,
                total,
                session_path: Some(session_path),
            },
            (None, false) => AutomationStatus::Completed {
                completed,
                total,
                session_path,
            },
        }
    }

    /// Handle start button click.
    fn handle_start(&mut self) {
        let iterations = self.state.iterations;

        // Start automation (runner creates session folder internally)
        match start_automation(Some(iterations)) {
            Ok(()) => {
                // Get session path from runner
                self.state.latest_session_path = crate::automation::runner::get_current_session_path();

                self.state.status = AutomationStatus::Running {
                    current: 0,
                    total: iterations,
                    state_description: "開始中...".to_string(),
                    start_time: Instant::now(),
                };
                self.state.automation_start_time = Some(Instant::now());
                crate::log(&format!("GUI: Started automation with {} iterations", iterations));
            }
            Err(e) => {
                self.state.status = AutomationStatus::Error {
                    completed: 0,
                    total: iterations,
                    message: e.to_string(),
                    session_path: None,
                };
                crate::log(&format!("GUI: Failed to start automation: {}", e));
            }
        }
    }

    /// Handle stop button click.
    fn handle_stop(&mut self) {
        request_abort();
        crate::log("GUI: Requested automation abort");
    }

    /// Handle "続行" (continue) button click — resumes the in-memory interrupted run.
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

    /// Rescan the output directory for interrupted sessions that can be resumed.
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

    /// Resume the session currently selected in the picker (restart survival path).
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

    /// Return from any terminal state to Idle so the user can start a fresh run
    /// (or reach the resume picker). Without this, a terminal state with no
    /// resume affordance — e.g. a "game not running" error — would be a dead end.
    fn handle_back_to_idle(&mut self) {
        self.state.status = AutomationStatus::Idle;
        // Re-scan so the picker reflects the current on-disk state: the
        // just-finished run may now be resumable, or a dismissed one gone.
        self.scan_resumable_sessions();
        crate::log("GUI: Returned to idle");
    }

    /// Dismiss the selected interrupted session from the picker (marks it done
    /// on disk via run-meta.json; the folder and its data are kept).
    fn handle_dismiss_selected(&mut self) {
        let chosen = self
            .state
            .selected_resume
            .and_then(|i| self.state.resumable_sessions.get(i).cloned());
        if let Some(s) = chosen {
            if crate::automation::session_meta::dismiss_session(&s.path) {
                crate::log(&format!("GUI: Dismissed session {}", s.path.display()));
            }
            self.state.selected_resume = None;
            self.scan_resumable_sessions();
        }
    }

    /// Handle generate charts button click.
    fn handle_generate_charts(&self) {
        crate::log("GUI: Generating charts...");
        match crate::analysis::generate_analysis() {
            Ok((chart_paths, json_path)) => {
                crate::log(&format!(
                    "GUI: Charts generated: {} files, stats: {}",
                    chart_paths.len(),
                    json_path.display()
                ));
            }
            Err(e) => {
                crate::log(&format!("GUI: Failed to generate charts: {}", e));
            }
        }
    }

    /// Handle open folder button click.
    fn handle_open_folder(&self) {
        if let Some(path) = &self.state.latest_session_path {
            // Open folder in Windows Explorer
            if let Err(e) = std::process::Command::new("explorer")
                .arg(path)
                .spawn()
            {
                crate::log(&format!("GUI: Failed to open folder: {}", e));
            }
        }
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Handle tray menu events
        self.handle_tray_events(ctx);

        // Handle global hotkey events
        self.handle_hotkey_events();

        // Check if exit was requested from tray menu
        if self.exit_requested {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        // Load images on first frame
        self.load_images(ctx);

        // Poll automation status
        self.update_automation_status();

        // Request repaint while automation is running (for progress updates)
        if self.state.status.is_running() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        // Main panel
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("学マス リハーサル統計自動化ツール");
            ui.add_space(8.0);

            // Three-column layout: image1, image2, controls
            ui.columns(3, |columns| {
                // Column 1: First guide image
                columns[0].vertical(|ui| {
                    render::render_guide_image(ui, &self.guide_images[0], "① コンテストで「リハーサル」を選択");
                });

                // Column 2: Second guide image
                columns[1].vertical(|ui| {
                    render::render_guide_image(ui, &self.guide_images[1], "② この画面で待機");
                });

                // Column 3: a single state-driven control panel, scrollable so nothing clips.
                columns[2].vertical(|ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let actions = render::render_control_panel(ui, &mut self.state);
                            if actions.start { self.handle_start(); }
                            if actions.stop { self.handle_stop(); }
                            if actions.continue_run { self.handle_continue(); }
                            if actions.generate_charts { self.handle_generate_charts(); }
                            if actions.open_folder { self.handle_open_folder(); }
                            if actions.refresh_resumable { self.scan_resumable_sessions(); }
                            if actions.resume_selected { self.handle_resume_selected(); }
                            if actions.back_to_idle { self.handle_back_to_idle(); }
                            if actions.dismiss_selected { self.handle_dismiss_selected(); }
                        });
                });
            });
        });
    }
}

impl GuiApp {
    /// Handle tray icon menu events.
    fn handle_tray_events(&mut self, ctx: &egui::Context) {
        if let Some(receiver) = &self.menu_event_receiver {
            // Non-blocking check for menu events
            while let Ok(event) = receiver.try_recv() {
                match event.id.0.as_str() {
                    MENU_SHOW_WINDOW => {
                        // Bring window to front
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                        crate::log("Tray: Show window requested");
                    }
                    MENU_EXIT => {
                        crate::log("Tray: Exit requested");
                        self.exit_requested = true;
                    }
                    _ => {}
                }
            }
        }
    }

    /// Handle global hotkey events.
    fn handle_hotkey_events(&mut self) {
        let hotkey_id = HOTKEY_TRIGGERED.swap(0, Ordering::SeqCst);
        if hotkey_id == 0 {
            return;
        }

        match hotkey_id {
            HOTKEY_SCREENSHOT => {
                crate::log("Hotkey: Screenshot (Ctrl+Shift+S)");
                match crate::capture::capture_gakumas() {
                    Ok(path) => crate::log(&format!("Screenshot saved: {}", path.display())),
                    Err(e) => crate::log(&format!("Screenshot failed: {}", e)),
                }
            }
            HOTKEY_ABORT => {
                if is_automation_running() {
                    crate::log("Hotkey: Abort (Ctrl+Shift+Q)");
                    request_abort();
                } else {
                    crate::log("Hotkey: Abort pressed but no automation running");
                }
            }
            _ => {}
        }
    }
}

/// Run the GUI application.
/// This function blocks until the window is closed.
pub fn run_gui() -> eframe::Result<()> {
    crate::log("GUI: Creating native options...");

    // Start hotkey handler thread
    let hotkey_running = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let hotkey_running_clone = hotkey_running.clone();
    let hotkey_thread = std::thread::spawn(move || {
        run_hotkey_thread(hotkey_running_clone);
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(Vec2::new(800.0, 580.0))
            .with_min_inner_size(Vec2::new(600.0, 450.0))
            .with_title("Gakumas Rehearsal Automation")
            // Disable drag-and-drop to avoid COM conflict with RoInitialize (multithreaded)
            .with_drag_and_drop(false),
        ..Default::default()
    };

    crate::log("GUI: Calling eframe::run_native...");

    let result = eframe::run_native(
        "Gakumas Rehearsal Automation",
        options,
        Box::new(|cc| {
            crate::log("GUI: Creating GuiApp instance...");
            Ok(Box::new(GuiApp::new(cc)))
        }),
    );

    // Stop hotkey thread
    hotkey_running.store(false, Ordering::SeqCst);
    let _ = hotkey_thread.join();

    result
}

/// Run the hotkey handler in a background thread.
fn run_hotkey_thread(running: Arc<std::sync::atomic::AtomicBool>) {
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        RegisterHotKey, UnregisterHotKey, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, PeekMessageW,
        RegisterClassW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, HWND_MESSAGE, MSG, PM_REMOVE,
        WM_HOTKEY, WNDCLASSW, WS_OVERLAPPEDWINDOW,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::core::w;

    unsafe {
        let hinstance = match GetModuleHandleW(None) {
            Ok(h) => h,
            Err(e) => {
                crate::log(&format!("Hotkey thread: Failed to get module handle: {}", e));
                return;
            }
        };

        let class_name = w!("GakumasHotkeyClass");
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(hotkey_window_proc),
            hInstance: hinstance.into(),
            lpszClassName: class_name,
            ..Default::default()
        };

        if RegisterClassW(&wc) == 0 {
            crate::log("Hotkey thread: Failed to register window class");
            return;
        }

        // Create message-only window
        let hwnd = match CreateWindowExW(
            Default::default(),
            class_name,
            w!("Hotkey Window"),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT, CW_USEDEFAULT, CW_USEDEFAULT, CW_USEDEFAULT,
            HWND_MESSAGE, // Message-only window
            None,
            hinstance,
            None,
        ) {
            Ok(h) => h,
            Err(e) => {
                crate::log(&format!("Hotkey thread: Failed to create window: {}", e));
                return;
            }
        };

        // Register hotkeys
        // Ctrl+Shift+S for screenshot
        if let Err(e) = RegisterHotKey(hwnd, HOTKEY_SCREENSHOT, MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT, 0x53) {
            crate::log(&format!("Hotkey thread: Failed to register screenshot hotkey: {}", e));
        } else {
            crate::log("Hotkey: Ctrl+Shift+S registered (screenshot)");
        }

        // Ctrl+Shift+Q for abort
        if let Err(e) = RegisterHotKey(hwnd, HOTKEY_ABORT, MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT, 0x51) {
            crate::log(&format!("Hotkey thread: Failed to register abort hotkey: {}", e));
        } else {
            crate::log("Hotkey: Ctrl+Shift+Q registered (abort)");
        }

        // Message loop
        let mut msg = MSG::default();
        while running.load(Ordering::SeqCst) {
            // Use PeekMessage with timeout to allow checking running flag
            if PeekMessageW(&mut msg, HWND::default(), 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_HOTKEY {
                    let hotkey_id = msg.wParam.0 as i32;
                    HOTKEY_TRIGGERED.store(hotkey_id, Ordering::SeqCst);
                }
                let _ = DispatchMessageW(&msg);
            } else {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }

        // Cleanup
        let _ = UnregisterHotKey(hwnd, HOTKEY_SCREENSHOT);
        let _ = UnregisterHotKey(hwnd, HOTKEY_ABORT);
        crate::log("Hotkey thread: Cleaned up");
    }
}

/// Window procedure for hotkey message-only window.
unsafe extern "system" fn hotkey_window_proc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::DefWindowProcW;
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}
