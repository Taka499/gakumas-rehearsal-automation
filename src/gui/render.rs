//! GUI rendering functions.
//!
//! Contains UI layout and component rendering logic.

use eframe::egui::{self, Color32, RichText, TextureHandle, Vec2};

use super::state::{AutomationStatus, GuiState};

/// One-tap run-count presets shown beneath every run-count input. Edit this
/// single array to change the buttons everywhere they appear.
const COUNT_PRESETS: [u32; 4] = [100, 200, 500, 1000];

/// Renders a run-count input: a numeric DragValue (drag or click-to-type,
/// clamped 1..=9999) followed by a row of one-tap preset buttons that set the
/// value directly. Shared by the idle 実行回数 input and the 追加実行 count so
/// both behave identically.
fn render_count_input(ui: &mut egui::Ui, label: &str, value: &mut u32) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(
            egui::DragValue::new(value)
                .range(1..=9999)
                .speed(1.0),
        );
        ui.label("回");
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        for preset in COUNT_PRESETS {
            if ui.button(preset.to_string()).clicked() {
                *value = preset;
            }
        }
    });
}

/// The 追加実行 (extend) control: a warning to return to the ② screen, a
/// count input with presets, and a button. Sets `actions.extend` when pressed.
/// `additional` is the GuiState field holding the extend count.
fn render_extend_section(ui: &mut egui::Ui, additional: &mut u32, actions: &mut PanelActions) {
    ui.add_space(16.0);
    ui.separator();
    ui.add_space(4.0);
    ui.label(RichText::new("追加実行").strong());
    ui.add_space(4.0);
    ui.label(
        RichText::new("⚠ ②のリハーサル開始画面に戻してから追加実行してください")
            .color(Color32::from_rgb(200, 120, 0))
            .small(),
    );
    ui.add_space(6.0);
    render_count_input(ui, "追加回数:", additional);
    ui.add_space(8.0);
    if ui.button(RichText::new("➕ 追加実行").size(16.0)).clicked() {
        actions.extend = true;
    }
}

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

/// Click signals collected from the state-driven control panel in one frame.
/// Each field is true if the corresponding button was clicked this frame.
#[derive(Default)]
pub struct PanelActions {
    pub start: bool,
    pub stop: bool,
    pub continue_run: bool,
    pub generate_charts: bool,
    pub open_folder: bool,
    pub refresh_resumable: bool,
    pub resume_selected: bool,
    /// Return from a terminal state to Idle (start a fresh run / reach the picker).
    pub back_to_idle: bool,
    /// Dismiss the session at `state.selected_resume` from the resume picker.
    pub dismiss_selected: bool,
    /// Run additional iterations into the most recent session's folder.
    pub extend: bool,
}

/// Renders the entire third column as a single state-driven panel: only the
/// controls relevant to the current automation state are shown. The caller
/// must wrap this in a vertical ScrollArea so content can never clip.
pub fn render_control_panel(ui: &mut egui::Ui, state: &mut GuiState) -> PanelActions {
    let mut actions = PanelActions::default();
    // Clone the status so we can read it while mutating other GuiState fields
    // (the run-count DragValue and the resume combo both borrow state mutably).
    let status = state.status.clone();

    match &status {
        AutomationStatus::Idle => render_idle(ui, state, &mut actions),
        AutomationStatus::Running { current, total, .. } => {
            render_running(ui, state, *current, *total, &mut actions)
        }
        AutomationStatus::Completed { .. }
        | AutomationStatus::Aborted { .. }
        | AutomationStatus::Error { .. } => {
            render_finished(ui, state, &status, &mut actions)
        }
    }

    actions
}

/// Idle: run-count input + Start, then the resume picker only if the on-disk
/// scan found interrupted sessions.
fn render_idle(ui: &mut egui::Ui, state: &mut GuiState, actions: &mut PanelActions) {
    ui.label(RichText::new("③ 回数を設定して開始").strong());
    ui.add_space(8.0);

    render_count_input(ui, "実行回数:", &mut state.iterations);

    ui.add_space(12.0);
    if ui.button(RichText::new("▶ 開始").size(18.0)).clicked() {
        actions.start = true;
    }

    // Shortcut to the most recent session's results, so charts/folder stay
    // reachable after returning to Idle (e.g. via the terminal-state 戻る button)
    // without having to re-enter a finished state.
    if state.latest_session_path.is_some() {
        ui.add_space(16.0);
        ui.separator();
        ui.add_space(4.0);
        ui.label(RichText::new("前回の結果").strong());
        ui.add_space(6.0);
        if ui.button("📊 グラフを生成").clicked() {
            actions.generate_charts = true;
        }
        ui.add_space(6.0);
        if ui.button("📁 フォルダを開く").clicked() {
            actions.open_folder = true;
        }
        render_extend_section(ui, &mut state.additional_iterations, actions);
    }

    if !state.resumable_sessions.is_empty() {
        ui.add_space(20.0);
        ui.separator();
        render_resume_section(ui, state, actions);
    }
}

/// Running: read-only count derived from the live run, warning, progress,
/// elapsed, and Stop. No editable input and no Start, so the count shown here
/// can never contradict the run in progress.
fn render_running(
    ui: &mut egui::Ui,
    state: &GuiState,
    current: u32,
    total: u32,
    actions: &mut PanelActions,
) {
    ui.heading(RichText::new("実行中").color(Color32::from_rgb(0, 120, 200)));
    ui.add_space(8.0);

    let line = if current >= 1 {
        format!("{}回 実行中 — {}回目", total, current)
    } else {
        format!("{}回 実行中 — 準備中", total)
    };
    ui.label(RichText::new(line).size(15.0));

    ui.add_space(4.0);
    ui.label(
        RichText::new("⚠ 実行中はマウスを動かさないでください")
            .color(Color32::from_rgb(200, 120, 0))
            .small(),
    );

    ui.add_space(8.0);
    ui.add(
        egui::ProgressBar::new(state.status.progress())
            .show_percentage()
            .animate(true),
    );

    if let Some(elapsed) = state.status.elapsed_text() {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("経過時間:");
            ui.label(elapsed);
        });
    }

    ui.add_space(12.0);
    if ui.button(RichText::new("◼ 停止").size(18.0)).clicked() {
        actions.stop = true;
    }
}

/// Finished (Completed/Aborted/Error): colored summary + progress, one
/// Continue button when interrupted with runs left, the generated-files list,
/// and chart/folder actions.
fn render_finished(
    ui: &mut egui::Ui,
    state: &mut GuiState,
    status: &AutomationStatus,
    actions: &mut PanelActions,
) {
    // Always offer a way back to Idle. Without this, a terminal state (notably a
    // "game not running" error with no session) would be a dead end: no Start
    // (idle-only) and possibly no 続行, leaving the panel unmanipulable.
    if ui
        .button("← 戻る")
        .on_hover_text("待機中に戻り、新しい実行を開始できます")
        .clicked()
    {
        actions.back_to_idle = true;
    }
    ui.add_space(8.0);

    let (heading, color) = match status {
        AutomationStatus::Completed { .. } => ("完了", Color32::from_rgb(0, 150, 0)),
        AutomationStatus::Aborted { .. } => ("中断", Color32::from_rgb(200, 150, 0)),
        AutomationStatus::Error { .. } => ("エラー", Color32::from_rgb(200, 0, 0)),
        _ => ("", Color32::GRAY),
    };
    ui.heading(RichText::new(heading).color(color));
    ui.add_space(8.0);
    ui.label(RichText::new(status.status_text()).color(color));

    ui.add_space(8.0);
    ui.add(egui::ProgressBar::new(status.progress()).show_percentage());

    if let Some((completed, total, _)) = status.resumable() {
        let remaining = total.saturating_sub(completed);
        ui.add_space(12.0);
        // Prominent instruction: the user must return to the ② screen first.
        ui.label(
            RichText::new("⚠ ②のリハーサル開始画面に戻してから「続行」を押してください")
                .color(Color32::from_rgb(200, 120, 0))
                .strong(),
        );
        ui.add_space(6.0);
        if ui
            .button(RichText::new(format!("⏵ 続行 (残り {}回)", remaining)).size(18.0))
            .clicked()
        {
            actions.continue_run = true;
        }
    }

    let session_path = match status {
        AutomationStatus::Completed { session_path, .. } => Some(session_path.clone()),
        AutomationStatus::Aborted { session_path, .. } => session_path.clone(),
        AutomationStatus::Error { session_path, .. } => session_path.clone(),
        _ => None,
    };
    if let Some(path) = session_path.as_ref() {
        render_generated_files(ui, path);
    }

    ui.add_space(16.0);
    ui.heading("アクション");
    ui.add_space(8.0);
    if ui.button("📊 グラフを生成").clicked() {
        actions.generate_charts = true;
    }
    ui.add_space(8.0);
    ui.add_enabled_ui(state.latest_session_path.is_some(), |ui| {
        if ui.button("📁 フォルダを開く").clicked() {
            actions.open_folder = true;
        }
    });

    // 追加実行 (extend): only for a finished series that is NOT resumable
    // (Completed, or a non-resumable terminal state) and that has a folder.
    // Mutually exclusive with the 続行 button rendered above.
    if status.resumable().is_none() && session_path.is_some() {
        render_extend_section(ui, &mut state.additional_iterations, actions);
    }
}

/// Lists which result files exist in a finished session's folder.
fn render_generated_files(ui: &mut egui::Ui, session_path: &std::path::Path) {
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(4.0);
    ui.label(RichText::new("生成ファイル:").strong());
    ui.add_space(4.0);

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
            let chart_count = entries
                .filter(|e| {
                    e.as_ref()
                        .map(|e| {
                            e.path().extension().map(|x| x == "png").unwrap_or(false)
                        })
                        .unwrap_or(false)
                })
                .count();
            if chart_count > 0 {
                ui.label(format!("  ✓ charts/ ({}個のグラフ)", chart_count));
            }
        }
    }

    ui.add_space(4.0);
    ui.label(
        RichText::new("「フォルダを開く」で結果を確認")
            .color(Color32::from_rgb(0, 120, 200)),
    );
}

/// Idle-only resume picker, collapsed by default so it never feels always-on.
/// The caller guarantees the list is non-empty. Each interrupted session gets
/// its own row with a ▶再開 (resume) and a ✕ (dismiss) button; dismissing marks
/// the session done on disk so it never lists again.
fn render_resume_section(ui: &mut egui::Ui, state: &mut GuiState, actions: &mut PanelActions) {
    let count = state.resumable_sessions.len();

    // Snapshot the row labels first so the picker can mutate `state` (the
    // selected-index channel) inside the loop without a borrow conflict.
    let rows: Vec<(usize, String)> = state
        .resumable_sessions
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let name = s.path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            (i, format!("{} — {}/{}回", name, s.completed, s.total))
        })
        .collect();

    egui::CollapsingHeader::new(format!("中断したセッションを再開 ({}件)", count))
        .id_source("resume_sessions_collapsing")
        .show(ui, |ui| {
            ui.label(
                RichText::new("⚠ ②のリハーサル開始画面に戻してから再開してください")
                    .color(Color32::from_rgb(200, 120, 0))
                    .small(),
            );
            ui.add_space(6.0);

            for (i, label) in rows {
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("▶ 再開").size(14.0)).clicked() {
                        state.selected_resume = Some(i);
                        actions.resume_selected = true;
                    }
                    if ui
                        .button("非表示")
                        .on_hover_text("このセッションをリストに表示しません（フォルダとデータは残ります）")
                        .clicked()
                    {
                        state.selected_resume = Some(i);
                        actions.dismiss_selected = true;
                    }
                    ui.label(label);
                });
            }

            ui.add_space(6.0);
            if ui.button("🔄 更新").clicked() {
                actions.refresh_resumable = true;
            }
        });
}
