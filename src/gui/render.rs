//! GUI rendering functions.
//!
//! Contains UI layout and component rendering logic.

use eframe::egui::{self, Color32, RichText, TextureHandle, Vec2};

use super::state::{AutomationStatus, GuiState};

/// Render a single guide image with label above.
pub fn render_guide_image(
    ui: &mut egui::Ui,
    texture: &Option<TextureHandle>,
    label: &str,
) {
    // Label above the image
    ui.label(RichText::new(label).strong());
    ui.add_space(4.0);

    let available_width = ui.available_width() - 8.0; // Leave some margin

    if let Some(tex) = texture {
        // Preserve original aspect ratio
        let orig_size = tex.size_vec2();
        let aspect_ratio = orig_size.y / orig_size.x;
        let image_height = available_width * aspect_ratio;
        ui.image((tex.id(), Vec2::new(available_width, image_height)));
    } else {
        // Placeholder when image not loaded (use 16:9 as default)
        let image_height = available_width * 1.78; // 9:16 portrait ratio
        let (rect, _response) = ui.allocate_exact_size(
            Vec2::new(available_width, image_height),
            egui::Sense::hover(),
        );
        ui.painter().rect_filled(rect, 4.0, Color32::from_gray(200));
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "画像",
            egui::FontId::proportional(16.0),
            Color32::from_gray(100),
        );
    }
}

/// Render the iteration input and control buttons.
/// Returns (start_clicked, stop_clicked, continue_clicked).
pub fn render_controls(
    ui: &mut egui::Ui,
    state: &mut GuiState,
) -> (bool, bool, bool) {
    let mut start_clicked = false;
    let mut stop_clicked = false;

    ui.heading("設定");
    ui.add_space(8.0);

    // Iteration count input
    ui.horizontal(|ui| {
        ui.label("実行回数:");
        ui.add(
            egui::DragValue::new(&mut state.iterations)
                .range(1..=9999)
                .speed(1.0)
        );
        ui.label("回");
    });

    ui.add_space(12.0);

    // Start/Stop buttons
    ui.horizontal(|ui| {
        let is_running = state.status.is_running();

        // Start button - disabled while running
        ui.add_enabled_ui(!is_running, |ui| {
            if ui.button(RichText::new("▶ 開始").size(16.0)).clicked() {
                start_clicked = true;
            }
        });

        ui.add_space(16.0);

        // Stop button - enabled only while running
        ui.add_enabled_ui(is_running, |ui| {
            if ui.button(RichText::new("◼ 停止").size(16.0)).clicked() {
                stop_clicked = true;
            }
        });
    });

    // Continue button - shown when the last run was interrupted with runs left.
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

    (start_clicked, stop_clicked, continue_clicked)
}

/// Render the progress display section.
pub fn render_progress(
    ui: &mut egui::Ui,
    state: &GuiState,
) {
    ui.add_space(16.0);
    ui.heading("進捗");
    ui.add_space(8.0);

    // Status text
    let status_color = match &state.status {
        AutomationStatus::Idle => Color32::GRAY,
        AutomationStatus::Running { .. } => Color32::from_rgb(0, 120, 200),
        AutomationStatus::Completed { .. } => Color32::from_rgb(0, 150, 0),
        AutomationStatus::Aborted { .. } => Color32::from_rgb(200, 150, 0),
        AutomationStatus::Error { .. } => Color32::from_rgb(200, 0, 0),
    };

    ui.label(RichText::new(state.status.status_text()).color(status_color));

    // Warning notice while running
    if state.status.is_running() {
        ui.add_space(4.0);
        ui.label(
            RichText::new("⚠ 実行中はマウスを動かさないでください")
                .color(Color32::from_rgb(200, 120, 0))
                .small()
        );
    }

    // Progress bar
    ui.add_space(8.0);
    let progress = state.status.progress();

    let progress_bar = egui::ProgressBar::new(progress)
        .show_percentage()
        .animate(state.status.is_running());

    ui.add(progress_bar);

    // Elapsed time (if running)
    if let Some(elapsed) = state.status.elapsed_text() {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("経過時間:");
            ui.label(elapsed);
        });
    }

    // Completion summary (for any terminal state that produced a session folder).
    // A timeout/abort run still captured partial data worth showing.
    let summary_path = match &state.status {
        AutomationStatus::Completed { session_path, .. } => Some(session_path),
        AutomationStatus::Aborted { session_path, .. } => session_path.as_ref(),
        AutomationStatus::Error { session_path, .. } => session_path.as_ref(),
        _ => None,
    };
    if let Some(session_path) = summary_path {
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);

        // Show generated files info
        ui.label(RichText::new("生成ファイル:").strong());
        ui.add_space(4.0);

        // Check what files exist in the session folder
        let results_csv = session_path.join("results.csv");
        let stats_json = session_path.join("statistics.json");
        let charts_dir = session_path.join("charts");

        if results_csv.exists() {
            ui.label("  ✓ results.csv (OCR結果)");
        }

        if stats_json.exists() {
            ui.label("  ✓ statistics.json (統計データ)");
        }

        if charts_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&charts_dir) {
                let chart_count = entries.filter(|e| {
                    e.as_ref().map(|e| e.path().extension().map(|x| x == "png").unwrap_or(false)).unwrap_or(false)
                }).count();
                if chart_count > 0 {
                    ui.label(format!("  ✓ charts/ ({}個のグラフ)", chart_count));
                }
            }
        }

        ui.add_space(4.0);
        ui.label(RichText::new("下の「フォルダを開く」で結果を確認").color(Color32::from_rgb(0, 120, 200)));
    }
}

/// Render the action buttons (Generate Charts, Open Folder).
/// Returns (generate_charts_clicked, open_folder_clicked).
pub fn render_actions(
    ui: &mut egui::Ui,
    state: &GuiState,
) -> (bool, bool) {
    let mut generate_clicked = false;
    let mut open_folder_clicked = false;

    ui.add_space(16.0);
    ui.heading("アクション");
    ui.add_space(8.0);

    // Generate Charts button
    if ui.button("📊 グラフを生成").clicked() {
        generate_clicked = true;
    }

    ui.add_space(8.0);

    // Open Folder button - enabled only if we have a session path
    ui.add_enabled_ui(state.latest_session_path.is_some(), |ui| {
        if ui.button("📁 フォルダを開く").clicked() {
            open_folder_clicked = true;
        }
    });

    (generate_clicked, open_folder_clicked)
}

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
