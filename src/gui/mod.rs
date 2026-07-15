//! GUI module for the application.
//!
//! Provides a graphical interface using egui/eframe for user interaction.

mod changelog;
pub(crate) mod clipboard;
pub(crate) mod copyable;
mod live_chart;
pub mod render;
mod review;
pub mod state;

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use eframe::egui::{self, TextureHandle, Vec2};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    TrayIcon, TrayIconBuilder,
};

use crate::automation::runner::{
    extend_automation, get_progress, is_automation_running, resume_automation, start_automation,
    AutomationOutcome,
};
use crate::automation::results_edit::load_review_rows;
use crate::automation::state::request_abort;

use copyable::CopyToast;
use live_chart::LiveChartCache;
use review::ReviewController;
use state::{AutomationStatus, GuiState, UpdateUiState};

/// Messages from the update worker threads (check at startup, install on
/// click) back to the GUI thread. Results arrive via `try_recv` polling in
/// `update()`; the workers `request_repaint` so a result shows immediately.
enum UpdateMsg {
    /// The startup check found a strictly newer release.
    UpdateAvailable(crate::update::UpdateInfo),
    /// download_and_install finished (`Err` carries the display message).
    InstallDone(Result<(), String>),
}

/// Menu item IDs for tray menu
const MENU_SHOW_WINDOW: &str = "show_window";
const MENU_EXIT: &str = "exit";

/// Hotkey IDs
const HOTKEY_SCREENSHOT: i32 = 101;
const HOTKEY_ABORT: i32 = 102;

/// Global hotkey event signal (set by hotkey thread, read by GUI thread)
static HOTKEY_TRIGGERED: AtomicI32 = AtomicI32::new(0);

/// egui context shared with the hotkey thread. eframe only runs `update()` when
/// the window is focused/repainting, so a hotkey pressed while the window is in
/// the background would sit queued until the window came to front. The hotkey
/// thread uses this to `request_repaint()` and wake the event loop immediately,
/// giving real-time background screenshots.
static EGUI_CTX: OnceLock<egui::Context> = OnceLock::new();

/// Embedded guide images (also copied to resources/guide/ by build.rs and package-release.ps1).
const GUIDE_IMAGE_1: &[u8] = include_bytes!("../../resources/guide/step1_contest_mode.png");
const GUIDE_IMAGE_2: &[u8] = include_bytes!("../../resources/guide/step2_rehearsal_page.png");

/// Default window size (guide + controls only, live plot hidden).
const WINDOW_SIZE_COLLAPSED: Vec2 = Vec2::new(620.0, 580.0);
/// Window size when the live distribution side panel is shown — wide enough that
/// the nine-box figure and the statistics table are comfortably readable, while the
/// control column stays about as narrow as it originally was.
const WINDOW_SIZE_EXPANDED: Vec2 = Vec2::new(1380.0, 760.0);
/// Default width of the live-plot side panel.
const LIVE_PLOT_PANEL_WIDTH: f32 = 760.0;
/// Width of the left guide-image side panel.
const GUIDE_PANEL_WIDTH: f32 = 300.0;

/// Persisted GUI preferences, stored as `gui_settings.json` next to the executable
/// (consistent with the app's other portable config files). Currently just the
/// live-distribution toggle. Defaults to on.
#[derive(serde::Serialize, serde::Deserialize)]
struct GuiSettings {
    show_live_chart: bool,
}

impl Default for GuiSettings {
    fn default() -> Self {
        Self { show_live_chart: true }
    }
}

/// Path to the persisted GUI settings file (next to the executable).
fn gui_settings_path() -> std::path::PathBuf {
    crate::paths::get_exe_dir().join("gui_settings.json")
}

/// Loads GUI settings, returning defaults if the file is missing or unreadable.
fn load_gui_settings() -> GuiSettings {
    match std::fs::read_to_string(gui_settings_path()) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => GuiSettings::default(),
    }
}

/// Persists GUI settings; failures are logged but never fatal.
fn save_gui_settings(settings: &GuiSettings) {
    match serde_json::to_string_pretty(settings) {
        Ok(json) => {
            if let Err(e) = std::fs::write(gui_settings_path(), json) {
                crate::log(&format!("Failed to save GUI settings: {}", e));
            }
        }
        Err(e) => crate::log(&format!("Failed to serialize GUI settings: {}", e)),
    }
}

/// Main GUI application struct.
pub struct GuiApp {
    /// Application state.
    state: GuiState,
    /// Loaded guide image textures.
    guide_images: [Option<TextureHandle>; 2],
    /// Flag to track if images have been loaded.
    images_loaded: bool,
    /// Cached live distribution figure (texture + stats + counts + change
    /// detection), owned together so the pieces can never drift apart.
    live_chart: LiveChartCache,
    /// The OCR review/edit window's controller (open/preview/save lifecycle).
    review: ReviewController,
    /// The single shared slot for the right-click-copy fade notice; painted
    /// over whichever image was copied last (see `copyable`).
    copy_toast: Option<CopyToast>,
    /// Whether the 更新履歴 (release history) window is shown. Not persisted.
    show_changelog: bool,
    /// Whether the window is currently expanded to make room for the live plot
    /// side panel. Used to resize once on show/hide rather than every frame.
    live_chart_expanded: bool,
    /// Last `show_live_chart` value written to disk; lets us persist the preference
    /// only when it actually changes rather than every frame.
    saved_show_live_chart: bool,
    /// Tray icon (kept alive for the duration of the app).
    #[allow(dead_code)]
    tray_icon: Option<TrayIcon>,
    /// Menu event receiver for tray menu (uses crossbeam-channel from tray-icon).
    menu_event_receiver: Option<tray_icon::menu::MenuEventReceiver>,
    /// Flag to request exit from tray menu.
    exit_requested: bool,
    /// Sender cloned into update worker threads (check + install).
    update_tx: std::sync::mpsc::Sender<UpdateMsg>,
    /// Results from the update worker threads, polled each frame.
    update_rx: std::sync::mpsc::Receiver<UpdateMsg>,
    /// Sender cloned into feedback send threads; the payload is the send's
    /// result (`Err` = user-facing Japanese message).
    feedback_tx: std::sync::mpsc::Sender<Result<(), String>>,
    /// Results from feedback send threads, polled each frame.
    feedback_rx: std::sync::mpsc::Receiver<Result<(), String>>,
}

impl GuiApp {
    /// Create a new GUI application instance.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Configure fonts to support Japanese
        Self::setup_fonts(&cc.egui_ctx);

        // Share the egui context with the hotkey thread so a background hotkey
        // press can wake the event loop (real-time screenshots even when this
        // window is not focused).
        let _ = EGUI_CTX.set(cc.egui_ctx.clone());

        // Set up tray icon
        let (tray_icon, menu_event_receiver) = Self::setup_tray_icon();

        // Restore the persisted live-distribution preference (default on).
        let settings = load_gui_settings();
        let mut state = GuiState::default();
        state.show_live_chart = settings.show_live_chart;

        // Fire the update check on a worker thread: check_for_update() does
        // blocking network I/O (up to ~30 s worst case) and must never touch
        // the GUI thread. No news (no update / check failed) sends nothing.
        let (update_tx, update_rx) = std::sync::mpsc::channel();
        {
            let tx = update_tx.clone();
            let ctx = cc.egui_ctx.clone();
            std::thread::spawn(move || {
                if let Some(info) = crate::update::check_for_update() {
                    let _ = tx.send(UpdateMsg::UpdateAvailable(info));
                    ctx.request_repaint();
                }
            });
        }

        let (feedback_tx, feedback_rx) = std::sync::mpsc::channel();

        let mut app = Self {
            state,
            guide_images: [None, None],
            images_loaded: false,
            live_chart: LiveChartCache::new(),
            review: ReviewController::new(),
            copy_toast: None,
            show_changelog: false,
            // Seed to match the persisted preference so the initial viewport size
            // (chosen in run_gui) is not resized on the first frame.
            live_chart_expanded: settings.show_live_chart,
            saved_show_live_chart: settings.show_live_chart,
            tray_icon,
            menu_event_receiver,
            exit_requested: false,
            update_tx,
            update_rx,
            feedback_tx,
            feedback_rx,
        };
        // Populate the resume picker with interrupted sessions found on disk.
        app.scan_resumable_sessions();
        // Seed "latest session" to the newest folder on disk so the previous-run
        // actions (charts/folder/review/extend) are reachable right after launch,
        // before any run starts this session. This is what lets a user review a
        // past session's OCR results without first kicking off a new run.
        if app.state.latest_session_path.is_none() {
            app.state.latest_session_path = newest_session_dir();
        }
        // Seed the live distribution from the most recent session so its figure shows
        // the last run's data right after launch (the LIVE_SCORES buffer is otherwise
        // empty in a fresh process). A new run clears the buffer before it starts, so
        // this never bleeds into a subsequent run.
        if let Some(path) = &app.state.latest_session_path {
            crate::automation::runner::reload_live_scores_from_csv(path);
        }
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

    /// Rebuild the live score-distribution figure (box-plot texture + statistics for
    /// the table) when the user has it enabled and new iteration data has arrived
    /// since the last render. Runs whether or not a run is in progress, so the empty
    /// figure is already visible the moment the user enables it (or on launch when the
    /// preference is on). Flagged rows are excluded from the statistics (kept in the
    /// buffer but not plotted) until verified. Cheap on idle frames thanks to the
    /// row-count guard; only re-renders on a new data point.
    fn update_live_chart(&mut self, ctx: &egui::Context) {
        self.live_chart.update(ctx, self.state.show_live_chart);
    }

    /// Update automation status by polling the automation runner.
    fn update_automation_status(&mut self) {
        let is_running = is_automation_running();

        match &self.state.status {
            AutomationStatus::Running { total, start_time, .. } => {
                // One snapshot so path/outcome/iteration all describe the same moment.
                let progress = get_progress();
                if !is_running {
                    // Automation finished - resolve the real outcome (success vs
                    // timeout/error vs abort) reported by the runner.
                    let session_path = progress
                        .session_path
                        .unwrap_or_else(|| crate::paths::get_output_dir());
                    self.state.latest_session_path = Some(session_path.clone());

                    self.state.status =
                        self.finalize_status(progress.last_outcome, *total, session_path.clone());
                    // Cache how many rows still need a human look so the finished
                    // panel can prompt the user (charts/CSV are final by now).
                    self.state.attention_counts =
                        Some(Self::count_attention(&session_path));
                    // The just-finished session should immediately appear in (or
                    // drop out of) the resume picker.
                    self.scan_resumable_sessions();
                } else {
                    // Still running - update progress
                    self.state.status = AutomationStatus::Running {
                        current: progress.current_iteration,
                        total: *total,
                        state_description: progress.state_desc,
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
                self.state.latest_session_path = get_progress().session_path;

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
                    self.state.latest_session_path = get_progress().session_path;
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

    /// Handle "➕ 追加実行" — runs `additional_iterations` more runs into the most
    /// recent session's folder, continuing its numbering.
    fn handle_extend(&mut self) {
        let additional = self.state.additional_iterations;
        let session_path = match &self.state.latest_session_path {
            Some(p) => p.clone(),
            None => {
                crate::log("GUI: 追加実行 requested but no recent session is known");
                return;
            }
        };
        match extend_automation(session_path.clone(), additional) {
            Ok(()) => {
                // start_automation_inner has already seeded the progress state:
                // total = completed + additional, current = completed.
                let progress = get_progress();
                self.state.latest_session_path = progress.session_path;
                let total = progress.total_iterations;
                let current = progress.current_iteration;
                self.state.status = AutomationStatus::Running {
                    current,
                    total,
                    state_description: "追加実行中...".to_string(),
                    start_time: std::time::Instant::now(),
                };
                self.state.automation_start_time = Some(std::time::Instant::now());
                crate::log(&format!(
                    "GUI: 追加実行 {}回 → {} (folder {})",
                    additional,
                    total,
                    session_path.display()
                ));
            }
            Err(e) => {
                crate::log(&format!("GUI: Failed to extend automation: {}", e));
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
                    self.state.latest_session_path = get_progress().session_path;
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

    /// Count rows in a finished session that still need attention: `flagged`
    /// (the reader could not confirm them — a human must look) and `repaired`
    /// (auto-recovered, worth a glance). Returns `(flagged, repaired)`; `(0, 0)`
    /// if the CSV is missing or unreadable. Drives the finished-panel prompt.
    fn count_attention(session_path: &std::path::Path) -> (u32, u32) {
        match load_review_rows(session_path) {
            Ok(rows) => {
                let mut flagged = 0u32;
                let mut repaired = 0u32;
                for r in &rows {
                    match r.recovery.as_str() {
                        "flagged" => flagged += 1,
                        "repaired" => repaired += 1,
                        _ => {}
                    }
                }
                (flagged, repaired)
            }
            Err(_) => (0, 0),
        }
    }

    /// Handle "📝 結果を確認・修正" — load the latest session's results into the
    /// review/edit window.
    fn handle_open_review(&mut self) {
        match &self.state.latest_session_path {
            Some(p) => self.review.open(p.clone()),
            None => crate::log("GUI: 結果を確認 requested but no recent session is known"),
        }
    }

    /// Render the review/edit window (when open) and apply the cross-module
    /// reactions to a save performed this frame. The window's own lifecycle
    /// (open/preview/edit/save) lives in `ReviewController`; the reactions
    /// below touch subsystems the review window should not own.
    fn render_review_window(&mut self, ctx: &egui::Context) {
        if let Some(effects) = self.review.show(ctx, &mut self.copy_toast) {
            // Keep the finished-panel prompt's count in step with the saved
            // recovery flags (a verified/manual row leaves the attention set).
            self.state.attention_counts =
                Some(Self::count_attention(&effects.session_path));
            // Refresh the live distribution from the saved CSV so the figure and table
            // reflect the corrected/verified values (verification can re-include a row
            // whose flag was cleared, which changes the stats without changing scores).
            crate::automation::runner::reload_live_scores_from_csv(&effects.session_path);
            self.live_chart.invalidate();
            // Charts exclude flagged rows until verified (docs/adr/0007), so any
            // save changes chart inputs: a score edit changes values, a verify-only
            // save re-includes the row. Regenerate unconditionally.
            crate::log("GUI: Regenerating charts after review save...");
            match crate::analysis::generate_analysis_for_session(&effects.session_path) {
                Ok((chart_paths, json_path)) => crate::log(&format!(
                    "GUI: Charts regenerated: {} files, stats: {}",
                    chart_paths.len(),
                    json_path.display()
                )),
                Err(e) => {
                    crate::log(&format!("GUI: Failed to regenerate charts: {}", e))
                }
            }
            // The save marked the live figure dirty; ensure the main viewport
            // runs another frame to pick up the refreshed distribution.
            ctx.request_repaint();
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

    /// Drain results from the update worker threads into the UI state.
    fn poll_update_messages(&mut self) {
        while let Ok(msg) = self.update_rx.try_recv() {
            match msg {
                UpdateMsg::UpdateAvailable(info) => {
                    // Only surface the startup check's result from Idle: don't
                    // stomp an install already in progress or finished.
                    if matches!(self.state.update, UpdateUiState::Idle) {
                        crate::log(&format!("GUI: update available: v{}", info.version));
                        self.state.update = UpdateUiState::Available(info);
                    }
                }
                UpdateMsg::InstallDone(Ok(())) => {
                    self.state.update = UpdateUiState::ReadyToRestart;
                }
                UpdateMsg::InstallDone(Err(e)) => {
                    crate::log(&format!("GUI: update install failed: {}", e));
                    self.state.update = UpdateUiState::Failed(e);
                }
            }
        }
    }

    /// Drain feedback-send results from worker threads into the UI state.
    fn poll_feedback_messages(&mut self) {
        while let Ok(result) = self.feedback_rx.try_recv() {
            let fb = &mut self.state.feedback;
            fb.sending = false;
            match result {
                Ok(()) => {
                    fb.open = false;
                    fb.message.clear();
                    fb.error = None;
                    fb.sent_toast = Some(std::time::Instant::now());
                }
                Err(msg) => {
                    // Keep the window open and the text intact for a retry.
                    fb.error = Some(msg);
                }
            }
        }
    }

    /// Handle the header's フィードバック button: refresh the session-log
    /// picker from disk and show the form window.
    fn handle_open_feedback(&mut self) {
        let fb = &mut self.state.feedback;
        fb.sessions = crate::feedback::list_session_logs(
            &crate::paths::get_output_dir(),
            crate::feedback::SESSION_PICKER_MAX,
        );
        // Newest session preselected (a bug report almost always concerns the
        // run just performed); 「添付しない」 remains available in the picker.
        fb.selected_log = if fb.sessions.is_empty() { None } else { Some(0) };
        fb.error = None;
        fb.open = true;
    }

    /// Handle the form's 送信 button: read the selected session log (if any)
    /// and send on a worker thread — blocking file + network I/O must stay
    /// off the GUI thread. The result comes back via `feedback_rx`.
    fn handle_send_feedback(&mut self, ctx: &egui::Context) {
        let fb = &mut self.state.feedback;
        if fb.sending {
            return;
        }
        let category = fb.category;
        let message = fb.message.trim().to_string();
        if message.is_empty() {
            return;
        }
        // The log is only attached for bug reports; the picker is hidden for
        // other categories, so any lingering selection must not leak through.
        let log_entry = if category == crate::feedback::FeedbackCategory::Bug {
            fb.selected_log.and_then(|i| fb.sessions.get(i).cloned())
        } else {
            None
        };
        fb.sending = true;
        fb.error = None;
        let tx = self.feedback_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let log_data = match &log_entry {
                    Some(entry) => {
                        // Lossy read: a stray non-UTF-8 byte in a log must not
                        // block the whole report.
                        let bytes = std::fs::read(&entry.path).map_err(|_| {
                            "ログの読み込みに失敗しました。「添付しない」を選ぶと送信できます"
                                .to_string()
                        })?;
                        Some((
                            entry.name.clone(),
                            String::from_utf8_lossy(&bytes).into_owned(),
                        ))
                    }
                    None => None,
                };
                let log_ref = log_data.as_ref().map(|(name, text)| {
                    (
                        name.as_str(),
                        crate::feedback::log_tail(text, crate::feedback::LOG_TAIL_MAX),
                    )
                });
                crate::feedback::send_feedback(category, &message, log_ref)
            })();
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }

    /// Render the feedback form as a floating window (opened from the header;
    /// see docs/EXECPLAN_FEEDBACK_FORM.md). Actions that need `&mut self`
    /// beyond the feedback state are collected as flags and dispatched after
    /// the closure, per this file's usual pattern.
    fn render_feedback_window(&mut self, ctx: &egui::Context) {
        if !self.state.feedback.open {
            return;
        }
        let fb = &mut self.state.feedback;
        let mut send_clicked = false;
        let mut cancel_clicked = false;
        let mut open = fb.open;
        egui::Window::new("フィードバック")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(380.0)
            .show(ctx, |ui| {
                egui::ComboBox::from_label("カテゴリ")
                    .selected_text(fb.category.label_ja())
                    .show_ui(ui, |ui| {
                        for c in crate::feedback::FeedbackCategory::ALL {
                            ui.selectable_value(&mut fb.category, c, c.label_ja());
                        }
                    });
                if fb.category == crate::feedback::FeedbackCategory::Bug {
                    let selected_text = fb
                        .selected_log
                        .and_then(|i| fb.sessions.get(i))
                        .map(|s| s.name.as_str())
                        .unwrap_or("添付しない");
                    egui::ComboBox::from_label("セッションログ")
                        .selected_text(selected_text)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut fb.selected_log, None, "添付しない");
                            for (i, s) in fb.sessions.iter().enumerate() {
                                ui.selectable_value(&mut fb.selected_log, Some(i), &s.name);
                            }
                        });
                }
                ui.add_space(4.0);
                ui.add(
                    egui::TextEdit::multiline(&mut fb.message)
                        .desired_rows(5)
                        .desired_width(f32::INFINITY)
                        .hint_text("ご意見や不具合の内容をご記入ください"),
                );
                // UTF-16 units to match the Worker's JS `message.length` check,
                // so nothing the form accepts is rejected server-side.
                let len = fb.message.encode_utf16().count();
                if len > crate::feedback::MESSAGE_MAX {
                    ui.label(
                        egui::RichText::new(format!(
                            "文字数が上限を超えています ({len}/{})",
                            crate::feedback::MESSAGE_MAX
                        ))
                        .color(egui::Color32::RED)
                        .small(),
                    );
                }
                let mut disclosure =
                    String::from("送信内容: メッセージ、カテゴリ、アプリのバージョン");
                if fb.category == crate::feedback::FeedbackCategory::Bug
                    && fb.selected_log.is_some()
                {
                    disclosure.push_str("、選択したセッションログ (末尾60KBまで)");
                }
                ui.label(egui::RichText::new(disclosure).small().weak());
                if let Some(err) = &fb.error {
                    ui.label(egui::RichText::new(err).color(egui::Color32::RED).small());
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if fb.sending {
                        ui.spinner();
                        ui.label("送信中...");
                    } else {
                        let can_send = !fb.message.trim().is_empty()
                            && len <= crate::feedback::MESSAGE_MAX;
                        if ui.add_enabled(can_send, egui::Button::new("送信")).clicked() {
                            send_clicked = true;
                        }
                        if ui.button("キャンセル").clicked() {
                            cancel_clicked = true;
                        }
                    }
                });
            });
        // The window's × and キャンセル both hide the form; the message is
        // kept either way so nothing typed is ever lost.
        self.state.feedback.open = open && !cancel_clicked;
        if send_clicked {
            self.handle_send_feedback(ctx);
        }
    }

    /// Handle the header's アップデート button: run download → verify → swap on
    /// a worker thread (blocking network + file I/O must stay off the GUI thread).
    fn handle_install_update(&mut self, ctx: &egui::Context) {
        let info = match &self.state.update {
            UpdateUiState::Available(info) => info.clone(),
            _ => return,
        };
        crate::log(&format!("GUI: installing update v{}", info.version));
        self.state.update = UpdateUiState::Downloading;
        let tx = self.update_tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = crate::update::install::download_and_install(&info)
                .map_err(|e| e.to_string());
            let _ = tx.send(UpdateMsg::InstallDone(result));
            ctx.request_repaint();
        });
    }

    /// Handle the header's 再起動 button after a successful install: spawn the
    /// (now updated) exe and exit this process. The child inherits this
    /// process's admin elevation, so no extra UAC prompt appears.
    fn handle_restart(&mut self) {
        let spawn = std::env::current_exe()
            .and_then(|exe| std::process::Command::new(exe).spawn());
        match spawn {
            Ok(_) => {
                crate::log("GUI: restarting into the updated exe");
                self.exit_requested = true;
            }
            Err(e) => {
                self.state.update =
                    UpdateUiState::Failed(format!("再起動に失敗しました: {}", e));
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

        // Drain update-check/install results from the worker threads.
        self.poll_update_messages();

        // Drain feedback-send results from the worker threads.
        self.poll_feedback_messages();

        // Keep the download spinner animating while an install runs.
        if matches!(self.state.update, UpdateUiState::Downloading) {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        // Keep the send spinner animating, and expire the sent notice.
        if self.state.feedback.sending {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if let Some(t) = self.state.feedback.sent_toast {
            if t.elapsed() > std::time::Duration::from_secs(4) {
                self.state.feedback.sent_toast = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
        }

        // Rebuild the live distribution figure when new iteration data has arrived.
        self.update_live_chart(ctx);

        // Persist the live-distribution preference whenever the user changes it, so it
        // is remembered across restarts.
        if self.state.show_live_chart != self.saved_show_live_chart {
            save_gui_settings(&GuiSettings {
                show_live_chart: self.state.show_live_chart,
            });
            self.saved_show_live_chart = self.state.show_live_chart;
        }

        // Expand the window the moment the live plot is enabled (not only once a run
        // starts), and shrink it back when disabled. Resized once per transition so it
        // never fights a manual resize.
        let show_live_panel = self.state.show_live_chart;
        if show_live_panel != self.live_chart_expanded {
            let size = if show_live_panel {
                WINDOW_SIZE_EXPANDED
            } else {
                WINDOW_SIZE_COLLAPSED
            };
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
            self.live_chart_expanded = show_live_panel;
        }

        // Request repaint while automation is running (for progress updates)
        if self.state.status.is_running() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        // Header spanning the full width. No separator line (it looked awkward above
        // the columns, which originally had none). The update notice renders here so
        // it is visible in every panel state; its actions are collected as flags and
        // dispatched after the closure (the closure immutably matches on state).
        let mut install_clicked = false;
        let mut restart_clicked = false;
        let mut dismiss_update_error = false;
        let mut feedback_clicked = false;
        let automation_running = self.state.status.is_running();
        egui::TopBottomPanel::top("header_panel")
            .show_separator_line(false)
            .show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading("学マス リハーサル統計自動化ツール");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("フィードバック").clicked() {
                        feedback_clicked = true;
                    }
                    // Right-to-left layout: added after フィードバック so it
                    // appears to its left.
                    if ui
                        .button("更新履歴")
                        .on_hover_text("これまでのバージョンの変更内容を表示します")
                        .clicked()
                    {
                        self.show_changelog = !self.show_changelog;
                    }
                    if self.state.feedback.sent_toast.is_some() {
                        ui.label(
                            egui::RichText::new("✅ 送信しました")
                                .color(egui::Color32::from_rgb(0x2e, 0x7d, 0x32)),
                        );
                    }
                });
            });
            ui.label(
                egui::RichText::new(
                    "💡 ショートカット: Ctrl+Shift+S でスクリーンショット／ Ctrl+Shift+Q で自動実行を中止",
                )
                .small()
                .weak(),
            );
            match &self.state.update {
                UpdateUiState::Idle => {}
                UpdateUiState::Available(info) => {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "🔄 新しいバージョン v{} が利用可能です",
                                info.version
                            ))
                            .color(egui::Color32::from_rgb(0x2e, 0x7d, 0x32)),
                        );
                        let button =
                            ui.add_enabled(!automation_running, egui::Button::new("アップデート"));
                        let button = if info.notes.is_empty() {
                            button
                        } else {
                            button.on_hover_text(&info.notes)
                        };
                        if button.clicked() {
                            install_clicked = true;
                        }
                        if automation_running {
                            ui.label(
                                egui::RichText::new("(自動実行中は更新できません)")
                                    .small()
                                    .weak(),
                            );
                        }
                    });
                }
                UpdateUiState::Downloading => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("アップデートをダウンロード中...");
                    });
                }
                UpdateUiState::ReadyToRestart => {
                    ui.horizontal(|ui| {
                        ui.label("✅ 更新の準備ができました。再起動すると新しいバージョンになります");
                        if ui.button("再起動").clicked() {
                            restart_clicked = true;
                        }
                    });
                }
                UpdateUiState::Failed(message) => {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("⚠ 更新に失敗しました: {}", message))
                                .color(egui::Color32::RED)
                                .small(),
                        );
                        if ui.small_button("×").clicked() {
                            dismiss_update_error = true;
                        }
                    });
                }
            }
            ui.add_space(4.0);
        });
        if install_clicked {
            self.handle_install_update(ctx);
        }
        if restart_clicked {
            self.handle_restart();
        }
        if dismiss_update_error {
            self.state.update = UpdateUiState::Idle;
        }
        if feedback_clicked {
            self.handle_open_feedback();
        }

        // Feedback form (floating window; renders only while open).
        self.render_feedback_window(ctx);
        changelog::show_window(ctx, &mut self.show_changelog);

        // Left: the rehearsal-page guide image (fixed width).
        egui::SidePanel::left("guide_panel")
            .resizable(false)
            .exact_width(GUIDE_PANEL_WIDTH)
            .show(ctx, |ui| {
                render::render_guide_image(ui, &self.guide_images[1], "① この画面で待機");
            });

        // Right: the live distribution figure + statistics table (wide, resizable).
        if show_live_panel {
            egui::SidePanel::right("live_plot_panel")
                .resizable(true)
                .default_width(LIVE_PLOT_PANEL_WIDTH)
                .min_width(380.0)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.add_space(4.0);
                            ui.heading("スコア分布（ライブ）");
                            let (included, excluded) = self.live_chart.counts();
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} 件（除外フラグ {} 件）",
                                    included, excluded
                                ))
                                .small()
                                .weak(),
                            );
                            ui.add_space(6.0);
                            // Snapshot the Copy handles first so the texture
                            // borrow does not overlap the `&mut` borrow of the
                            // copy-toast slot below (E0502 otherwise).
                            let plot_tex = self.live_chart.texture().map(|t| (t.id(), t.size()));
                            if let Some((tex_id, size)) = plot_tex {
                                // Scale to the panel width, preserving the figure's aspect.
                                let w = ui.available_width();
                                let aspect = size[1] as f32 / size[0] as f32;
                                // Images are Sense::hover() by default — without an
                                // explicit click sense, secondary_clicked() never fires.
                                let resp = ui.add(
                                    egui::Image::new((tex_id, Vec2::new(w, w * aspect)))
                                        .sense(egui::Sense::click()),
                                );
                                // Right-click copies the figure at native resolution:
                                // re-rendered from the exact stats behind the displayed
                                // texture (the RGBA buffer is not retained after upload).
                                let stats = self.live_chart.stats().cloned();
                                copyable::copy_on_right_click(
                                    ui,
                                    &resp,
                                    egui::Id::new("copy_live_plot"),
                                    &mut self.copy_toast,
                                    move || {
                                        let stats = stats.ok_or_else(|| {
                                            anyhow::anyhow!("live stats not available")
                                        })?;
                                        crate::analysis::charts::render_live_box_plot_rgba(&stats)
                                    },
                                );
                            }
                            ui.add_space(10.0);
                            if let Some(stats) = self.live_chart.stats() {
                                render::render_live_stats_table(ui, stats);
                            }
                        });
                });
        }

        // Center: the state-driven control panel (narrow), scrollable so nothing clips.
        egui::CentralPanel::default().show(ctx, |ui| {
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
                    if actions.extend { self.handle_extend(); }
                    if actions.open_review { self.handle_open_review(); }
                });
        });

        // Review/edit window (floats over the main panel when open).
        self.render_review_window(ctx);
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

/// Newest session folder under the output directory, or `None` if there are no
/// sessions. Folder names are `YYYYMMDD_HHMMSS`, so the lexicographically-largest
/// name is the most recent. Only directories containing a `results.csv` qualify,
/// so an empty/aborted-before-OCR folder is skipped.
fn newest_session_dir() -> Option<std::path::PathBuf> {
    let dir = crate::paths::get_output_dir();
    let mut best: Option<(String, std::path::PathBuf)> = None;
    for entry in std::fs::read_dir(&dir).ok()?.flatten() {
        let path = entry.path();
        if !path.is_dir() || !path.join("results.csv").exists() {
            continue;
        }
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if best.as_ref().map_or(true, |(b, _)| name > *b) {
            best = Some((name, path));
        }
    }
    best.map(|(_, p)| p)
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

    // Start at the size that matches the persisted live-plot preference, so the
    // window opens correctly sized instead of resizing on the first frame.
    let initial_size = if load_gui_settings().show_live_chart {
        WINDOW_SIZE_EXPANDED
    } else {
        WINDOW_SIZE_COLLAPSED
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(initial_size)
            .with_min_inner_size(Vec2::new(560.0, 450.0))
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
                    // Wake the GUI event loop so the hotkey is handled now, even
                    // when the window is in the background (otherwise update()
                    // would not run until the window regained focus).
                    if let Some(ctx) = EGUI_CTX.get() {
                        ctx.request_repaint();
                    }
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
