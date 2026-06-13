# Redesign the GUI third column into a state-driven control panel

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

This repository's ExecPlan conventions live in `docs/PLANS.md` (relative to the repository root). This document must be maintained in accordance with `docs/PLANS.md`.


## Purpose / Big Picture

This tool (`gakumas-screenshot`) is a Windows system-tray + GUI application that automates the game "Gakumas" rehearsal screen. Its main window (an `egui` GUI) is laid out in three columns: two guide images on the left, and a tall control column on the right that holds every control — the run-count input, Start/Stop, a "continue interrupted run" button, the progress bar, a generated-files summary, chart/folder actions, and a picker for resuming past sessions.

Today that third column renders **all of those at once, stacked vertically, with no scroll container**. Three concrete problems result, all visible in normal use:

1. **Overflow.** The column is taller than the window, so the bottom is silently clipped. The "resume a previous session" picker sits last and is unreachable unless the user manually enlarges the window.
2. **Contradictory run count.** The "実行回数" (run count) input is an editable box bound to a standalone value that defaults to 100. While a run is in progress — especially a *resumed* run of, say, 5 — the box still shows 100, contradicting the progress line that correctly reads "5". The user sees "100" and "5" on screen at the same time.
3. **Two competing resume controls.** A "⏵ 続行" (continue) button and a separate session **picker** are both always visible. They mean nearly the same thing ("finish the runs that didn't happen") but live in different places, so the interface feels fragmented and is easy to misread.

After this change, the third column shows **only the controls relevant to the current state**, wrapped in a vertical scroll area so nothing can ever be clipped:

- **待機中 (Idle):** the run-count input and a ▶開始 button. Below them — *only if* the on-disk scan found interrupted sessions — the resume picker. Nothing else.
- **実行中 (Running):** the actual run count shown as **read-only text derived from the live run** ("5回 実行中 — 3回目"), the "don't move the mouse" warning, the progress bar, elapsed time, and a ◼停止 button. No editable input, no Start — so the 100-vs-5 contradiction cannot occur.
- **終了 (Completed / Aborted / Error):** a colored result summary, the progress bar, the generated-files list, and chart/folder actions. If the run was interrupted with runs left, exactly **one** prominent ⏵続行 button appears here.

This makes resume coherent: **続行** means "finish the run I just watched stop" (shown only in a finished state), and the **picker** means "resume something from an earlier app session" (shown only when idle). The two are never on screen together.

You can see it working like this: shrink the window to a small size — the third column gains a scrollbar and the resume picker is always reachable, never clipped. Start a 5-run series; while it runs, the column shows "5回 実行中 — N回目" and a Stop button, with no "100" anywhere. Abort it; the column switches to a "中断 (2/5回 完了)" summary with a single "⏵ 続行 (残り 3回)" button and no picker. Close and reopen the app while idle; the picker (and only the picker) lists the interrupted session.


## Progress

- [x] (2026-06-13Z) M1: Scroll the third column. Wrapped the existing third-column rendering in `egui::ScrollArea::vertical().auto_shrink([false, false])` so content can never be clipped, with no other behavior change. `cargo check` passes with only pre-existing warnings.
- [ ] M2: State-driven control panel. Add `PanelActions` and `render_control_panel` (+ private per-state helpers) to `src/gui/render.rs`; rewire `src/gui/mod.rs` `update()` to call it; remove the four superseded render functions. This delivers the idle/running/finished panels, the read-only running count, and the single-resume-affordance rule. (completed: —; remaining: all)

Use timestamps (UTC) when checking off items, e.g. `- [x] (2026-06-13 14:00Z) ...`.


## Surprises & Discoveries

- (Add findings here as you implement, with short evidence snippets.)
- Anticipated borrow constraint (verify during M2): `render_control_panel` takes `&mut GuiState` and, inside a `match` on the status, must also mutate *other* `GuiState` fields (the run-count `DragValue` mutates `state.iterations`; the resume combo mutates `state.selected_resume`). Matching on `&state.status` while mutating sibling fields can trip the borrow checker. The plan resolves this by cloning the status once at the top (`let status = state.status.clone();` — `AutomationStatus` derives `Clone`) and matching on `&status`, leaving `state` free to mutate. If you skip the clone you will likely see `error[E0502]: cannot borrow ... as mutable because it is also borrowed as immutable`.


## Decision Log

- Decision: Wrap the third column in a vertical `ScrollArea` rather than shrinking content or enlarging the default window.
  Rationale: A scroll area is robust to any future content growth and to any window size, where hand-tuned spacing is not. It fixes overflow structurally.
  Date/Author: 2026-06-13 / planning.
- Decision: Render the third column as a single state-driven panel that shows only the controls relevant to the current `AutomationStatus`, instead of always rendering every section.
  Rationale: The fragmentation, the dual resume controls, and the contradictory count all stem from showing everything at once. Branching on state removes all three at the source.
  Date/Author: 2026-06-13 / planning.
- Decision: While running, show the run count as read-only text derived from the live status (`total`/`current`), and only show the editable input when idle.
  Rationale: The "100 vs 5" contradiction exists because the editable input (defaulting to 100) is shown alongside a running session whose true total lives elsewhere. Showing the derived value during a run, and the input only when idle, makes the two impossible to contradict.
  Date/Author: 2026-06-13 / planning.
- Decision: The resume **picker** appears only in the Idle state; the **続行** button appears only in a finished (Aborted/Error with runs left) state. They are never shown together.
  Rationale: 続行 resumes the just-finished in-memory session; the picker resumes a session discovered on disk (e.g. after an app restart). Separating them by state removes the redundancy without losing either capability.
  Date/Author: 2026-06-13 / planning.
- Decision: When the Idle state has no resumable sessions, hide the resume section entirely (no always-present "更新" button).
  Rationale: A session becomes resumable only when this app interrupts a run (the existing finalize path rescans) or when a prior app session left one (the existing startup scan in `GuiApp::new` finds it). Neither real path needs a manual refresh to *discover* a session, so an always-on refresh button when the list is empty would be clutter with no workflow behind it. A "🔄 更新" button is still offered inside the section once at least one session is listed, to re-scan after resuming one.
  Date/Author: 2026-06-13 / planning.
- Decision: Clone `state.status` at the top of `render_control_panel` and match on the clone.
  Rationale: Avoids a borrow-checker conflict between the immutable borrow of `state.status` (the match) and the mutable borrows of `state.iterations` / `state.selected_resume` inside the arms. See `Surprises & Discoveries`.
  Date/Author: 2026-06-13 / planning.


## Outcomes & Retrospective

To be completed at the end of each milestone and at full completion. Compare against Purpose: is the third column free of clipping at small window sizes, free of the 100-vs-5 contradiction during runs, and showing exactly one resume affordance appropriate to the current state?


## Context and Orientation

You are working in a Rust 2024-edition Windows application. Build from the repository root (`C:\Work\GitRepos\gakumas-screenshot`) with `cargo build` / `cargo build --release`. The executable carries an administrator manifest, so `cargo test` cannot launch the test binary (it fails with an elevation error). Therefore the compile gate for this work is `cargo check`, and behavioral acceptance is manual (running the app and looking at the window). "egui" is the immediate-mode GUI library this app uses; "immediate-mode" means the UI is rebuilt from scratch every frame by calling functions like `ui.button(...)`, and a button "click" is simply that call returning a `Response` whose `.clicked()` is true on the frame it was pressed. You never store widgets; you re-emit them each frame from current state.

Two files matter for this work, and nothing else needs to change:

- `src/gui/state.rs` — Defines `enum AutomationStatus` and `struct GuiState`. You will not modify this file, but you must understand it. `AutomationStatus` (which derives `Clone, Debug`) has these variants:

      pub enum AutomationStatus {
          Idle,
          Running { current: u32, total: u32, state_description: String, start_time: Instant },
          Completed { completed: u32, total: u32, session_path: PathBuf },
          Aborted { completed: u32, total: u32, session_path: Option<PathBuf> },
          Error { completed: u32, total: u32, message: String, session_path: Option<PathBuf> },
      }

  It provides these methods you will call: `status_text(&self) -> String` (a localized one-line description), `progress(&self) -> f32` (0.0–1.0, reflecting real completion even for terminal states), `elapsed_text(&self) -> Option<String>` (mm:ss while running, else None), `is_running(&self) -> bool`, and `resumable(&self) -> Option<(u32, u32, std::path::PathBuf)>` (returns `(completed, total, session_path)` when the state is an interrupted Aborted/Error with `completed < total` and a known folder, else None). `struct GuiState` has the fields you will read/write: `iterations: u32` (the run-count input value), `status: AutomationStatus`, `latest_session_path: Option<PathBuf>`, `automation_start_time: Option<Instant>`, `resumable_sessions: Vec<ResumableSession>` (interrupted sessions found on disk), and `selected_resume: Option<usize>` (index into `resumable_sessions`). A `ResumableSession` has public fields `path: PathBuf`, `total: u32`, `completed: u32`.

- `src/gui/render.rs` — Pure rendering helpers. Today it contains `render_guide_image` (keep it unchanged) plus four functions you will replace: `render_controls(ui, &mut GuiState) -> (bool, bool, bool)` (run-count input + Start/Stop + Continue), `render_progress(ui, &GuiState)` (status text + warning + progress bar + elapsed + generated-files summary), `render_actions(ui, &GuiState) -> (bool, bool)` (グラフ/フォルダ buttons), and `render_resume_picker(ui, &mut GuiState) -> (bool, bool)` (the session combo). Its top imports are `use eframe::egui::{self, Color32, RichText, TextureHandle, Vec2};` and `use super::state::{AutomationStatus, GuiState};` — both already provide everything the new code needs.

- `src/gui/mod.rs` — The egui application (`struct GuiApp`). Its `eframe::App::update` method runs each frame; near the end it builds the three-column layout with `ui.columns(3, |columns| { ... })`. Column three (`columns[2].vertical(|ui| { ... })`) currently calls the four render functions above and dispatches their click booleans to handler methods. Those handler methods already exist and **do not change**: `handle_start(&mut self)`, `handle_stop(&mut self)`, `handle_continue(&mut self)`, `handle_generate_charts(&self)`, `handle_open_folder(&self)`, `handle_resume_selected(&mut self)`, and `scan_resumable_sessions(&mut self)`. You are only changing *what* gets rendered and *how* clicks are collected, not the handlers' bodies.

The exact current third-column block in `src/gui/mod.rs` `update()` is this (you will replace the inside of `columns[2].vertical(...)`):

    // Column 3: Controls, progress, actions
    columns[2].vertical(|ui| {
        // Guide text at top of column
        ui.label(egui::RichText::new("③ 回数を設定して開始").strong());
        ui.add_space(8.0);

        // Controls section (iteration input, start/stop/continue buttons)
        let (start_clicked, stop_clicked, continue_clicked) =
            render::render_controls(ui, &mut self.state);

        if start_clicked {
            self.handle_start();
        }
        if stop_clicked {
            self.handle_stop();
        }
        if continue_clicked {
            self.handle_continue();
        }

        // Progress section
        render::render_progress(ui, &self.state);

        // Action buttons section
        let (generate_clicked, open_folder_clicked) = render::render_actions(ui, &self.state);

        if generate_clicked {
            self.handle_generate_charts();
        }
        if open_folder_clicked {
            self.handle_open_folder();
        }

        // Resume-a-previous-session picker (restart survival)
        let (refresh_clicked, resume_selected_clicked) =
            render::render_resume_picker(ui, &mut self.state);
        if refresh_clicked {
            self.scan_resumable_sessions();
        }
        if resume_selected_clicked {
            self.handle_resume_selected();
        }
    });


## Plan of Work

The work is two milestones. M1 makes the column scroll (a small, safe, immediately-observable fix for overflow) without changing any other behavior. M2 replaces the four always-on render functions with one state-driven panel, delivering the idle/running/finished layouts, the read-only running count, and the single-resume-affordance rule. Each milestone compiles cleanly with `cargo check`.


### Milestone M1 — Scroll the third column

Goal: the third column can never clip its content. At the end of M1 the layout and controls are exactly as before, but wrapped in a vertical scroll area, so shrinking the window reveals a scrollbar instead of hiding the bottom controls.

In `src/gui/mod.rs`, inside `update()`, wrap the **entire body** of `columns[2].vertical(|ui| { ... })` in an `egui::ScrollArea::vertical()`. Concretely, change the column-three block so its closure body is a single scroll area whose inner closure contains the existing code unchanged:

    // Column 3: Controls, progress, actions (scrollable so nothing clips)
    columns[2].vertical(|ui| {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // Guide text at top of column
                ui.label(egui::RichText::new("③ 回数を設定して開始").strong());
                ui.add_space(8.0);

                let (start_clicked, stop_clicked, continue_clicked) =
                    render::render_controls(ui, &mut self.state);
                if start_clicked { self.handle_start(); }
                if stop_clicked { self.handle_stop(); }
                if continue_clicked { self.handle_continue(); }

                render::render_progress(ui, &self.state);

                let (generate_clicked, open_folder_clicked) =
                    render::render_actions(ui, &self.state);
                if generate_clicked { self.handle_generate_charts(); }
                if open_folder_clicked { self.handle_open_folder(); }

                let (refresh_clicked, resume_selected_clicked) =
                    render::render_resume_picker(ui, &mut self.state);
                if refresh_clicked { self.scan_resumable_sessions(); }
                if resume_selected_clicked { self.handle_resume_selected(); }
            });
    });

`auto_shrink([false, false])` tells the scroll area to fill the available width and height rather than collapsing to its content size, so the column keeps its normal width and only scrolls when content exceeds the window height. No other file changes in M1.


### Milestone M2 — State-driven control panel

Goal: the third column shows only what is relevant to the current state. At the end of M2, the four render functions are replaced by one `render_control_panel` that branches on `AutomationStatus`; the running state shows a read-only count (no "100"); the picker shows only when idle; and 続行 shows only in a finished, interrupted state.

First, in `src/gui/render.rs`, **delete** these four functions in their entirety: `render_controls`, `render_progress`, `render_actions`, and `render_resume_picker`. Keep `render_guide_image` and the file's existing `use` lines. Then **add** the following — a small action-collecting struct, the public panel entry point, and four private helpers. Place them after `render_guide_image`.

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

        ui.horizontal(|ui| {
            ui.label("実行回数:");
            ui.add(
                egui::DragValue::new(&mut state.iterations)
                    .range(1..=9999)
                    .speed(1.0),
            );
            ui.label("回");
        });

        ui.add_space(12.0);
        if ui.button(RichText::new("▶ 開始").size(18.0)).clicked() {
            actions.start = true;
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
        state: &GuiState,
        status: &AutomationStatus,
        actions: &mut PanelActions,
    ) {
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
            if ui
                .button(RichText::new(format!("⏵ 続行 (残り {}回)", remaining)).size(18.0))
                .clicked()
            {
                actions.continue_run = true;
            }
            ui.add_space(4.0);
            ui.label(
                RichText::new("ゲームをリハーサル開始画面に戻してから続行してください").small(),
            );
        }

        let session_path = match status {
            AutomationStatus::Completed { session_path, .. } => Some(session_path.clone()),
            AutomationStatus::Aborted { session_path, .. } => session_path.clone(),
            AutomationStatus::Error { session_path, .. } => session_path.clone(),
            _ => None,
        };
        if let Some(path) = session_path {
            render_generated_files(ui, &path);
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

    /// Idle-only resume picker. The caller guarantees the list is non-empty.
    fn render_resume_section(ui: &mut egui::Ui, state: &mut GuiState, actions: &mut PanelActions) {
        ui.add_space(8.0);
        ui.heading("中断したセッションを再開");
        ui.add_space(4.0);
        ui.label(
            RichText::new("ゲームをリハーサル開始画面に戻してから再開してください").small(),
        );
        ui.add_space(6.0);

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

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_enabled_ui(state.selected_resume.is_some(), |ui| {
                if ui.button(RichText::new("▶ 選択を再開").size(16.0)).clicked() {
                    actions.resume_selected = true;
                }
            });
            ui.add_space(8.0);
            if ui.button("🔄 更新").clicked() {
                actions.refresh_resumable = true;
            }
        });
    }

Then, in `src/gui/mod.rs` `update()`, replace the **inside** of the `egui::ScrollArea::vertical()...show(ui, |ui| { ... })` closure from M1 with a single call to the new panel plus dispatch. The column-three block becomes:

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
            });
    });

The guide text `"③ 回数を設定して開始"` that previously sat above the controls now lives **inside** `render_idle` (it is only meaningful when idle), so it is intentionally removed from `update()`; the running and finished states present their own headings instead. `PanelActions` owns its data (no borrow of `state`), so calling `self.handle_*` after `render_control_panel` returns does not conflict with the `&mut self.state` borrow that ended when the call returned.


## Concrete Steps

Run all commands from the repository root `C:\Work\GitRepos\gakumas-screenshot` in PowerShell.

1. Implement M1, then compile-check:

       cargo check

   Expected: `Finished` with no errors (pre-existing warnings about unused OCR items and unused `pub use` re-exports are fine). If you see an error about `ScrollArea` not found, confirm you wrote `egui::ScrollArea::vertical()` (the `egui` path is already in scope via `render`/`mod.rs` usage of `egui::`).

2. Implement M2, then:

       cargo check

   Expected: no errors. Common mistakes and what they look like:
   - Leaving a call to a deleted function (`render::render_controls`, `render_progress`, `render_actions`, or `render_resume_picker`) anywhere — `error[E0425]: cannot find function ...`. Grep to be sure none remain:

         git grep -n "render_controls\|render_progress\|render_actions\|render_resume_picker" src/gui

     Expected: no matches after M2.
   - Forgetting the `let status = state.status.clone();` line and matching on `&state.status` directly — `error[E0502]` about borrowing `state` mutably while it is borrowed immutably.

3. Build a runnable binary:

       cargo build --release

   Expected: `Finished release` with no errors. The binary is `target\release\gakumas-screenshot.exe`.


## Validation and Acceptance

Because the executable requires administrator elevation, automated `cargo test` cannot run; acceptance is the following manual checks. Launch the built app (run it elevated if the game runs elevated):

    .\target\release\gakumas-screenshot.exe

Scenario A — No clipping (M1, persists through M2). With the app idle, drag the window to make it short (vertically small). Confirm the third column shows a vertical scrollbar and that scrolling reaches the bottom-most control; nothing is cut off. Before this change the bottom controls were unreachable without enlarging the window.

Scenario B — Idle shows only setup and (conditionally) the picker (M2). With no interrupted sessions on disk, the third column shows the heading "③ 回数を設定して開始", the 実行回数 input, and a ▶開始 button — and **no** progress bar, **no** Stop, **no** 続行, and **no** resume picker. With at least one interrupted session present (produce one via Scenario D, then return to idle by relaunching), the picker appears below ▶開始 with a combo and a ▶選択を再開 / 🔄更新 row.

Scenario C — Running shows a read-only, correct count (M2). Set 実行回数 to 5 and press 開始. While running, confirm the third column shows the heading "実行中", a line reading "5回 実行中 — N回目" (N advancing 1→5), the ⚠ warning, an animated progress bar, 経過時間, and a ◼停止 button. Confirm there is **no** editable input and the number "100" appears nowhere. This is the fix for the prior 100-vs-5 contradiction.

Scenario D — Finished shows a summary and exactly one resume control (M2). During Scenario C, press the abort hotkey Ctrl+Shift+Q after 2 runs. Confirm the column switches to a "中断" heading, an amber "中断 (2/5回 完了)" line, a partial progress bar (~40%, not 100%), exactly one "⏵ 続行 (残り 3回)" button, the 生成ファイル list, and the アクション buttons — and **no** session picker (the picker is idle-only). Click 続行 (game on the rehearsal start page): the run resumes and the column returns to the 実行中 layout from Scenario C.

Scenario E — Completed has no resume control (M2). Let a series finish all its runs. Confirm the column shows a green "完了" heading and summary, a full (100%) progress bar, the 生成ファイル list, and アクション buttons, with **no** 続行 button and **no** picker.

If every scenario matches, the redesign meets its purpose: no clipping, no contradictory count, and one resume affordance appropriate to each state.


## Idempotence and Recovery

All steps are safe to repeat. `cargo check` and `cargo build` are idempotent. The change is confined to two files (`src/gui/render.rs` and `src/gui/mod.rs`) and touches only rendering and click-dispatch — no persisted data, no automation logic, no file formats. If the UI misbehaves, revert those two files (`git checkout -- src/gui/render.rs src/gui/mod.rs`) to return to the pre-redesign behavior; nothing else depends on the removed functions. To verify nothing else referenced them, the `git grep` in Concrete Steps step 2 must return no matches.


## Artifacts and Notes

The third column's behavior by state, after this change:

    Idle      : "③ 回数を設定して開始"  [実行回数: N 回]  [▶ 開始]
                (+ resume picker only if interrupted sessions exist on disk)
    Running   : "実行中"  "5回 実行中 — 3回目"  ⚠ warning  [████░░] 60%  経過 mm:ss  [◼ 停止]
    Aborted   : "中断"   "中断 (2/5回 完了)"   [████░░] 40%  [⏵ 続行 (残り 3回)]
                生成ファイル: ✓ results.csv …   アクション: [📊][📁]
    Error     : "エラー" "エラー (k/N回 完了): …"  partial bar  [⏵ 続行 …]  生成ファイル …  アクション …
    Completed : "完了"   "完了 (5/5回) → folder"  [██████] 100%  生成ファイル …  アクション … (no 続行)


## Interfaces and Dependencies

Use the crates already in `Cargo.toml`: `eframe`/`egui` for the UI. No new dependencies, and no changes to `src/gui/state.rs` or any automation module.

In `src/gui/render.rs`, at the end of M2 these must exist and the four old functions must be gone:

    pub struct PanelActions {
        pub start: bool,
        pub stop: bool,
        pub continue_run: bool,
        pub generate_charts: bool,
        pub open_folder: bool,
        pub refresh_resumable: bool,
        pub resume_selected: bool,
    }
    pub fn render_control_panel(ui: &mut eframe::egui::Ui, state: &mut crate::gui::state::GuiState) -> PanelActions;
    // plus private helpers: render_idle, render_running, render_finished,
    // render_generated_files, render_resume_section
    pub fn render_guide_image(ui: &mut eframe::egui::Ui, texture: &Option<eframe::egui::TextureHandle>, label: &str); // unchanged

In `src/gui/mod.rs`, `update()` must call `render::render_control_panel(ui, &mut self.state)` exactly once inside an `egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| { ... })` in column three, and dispatch the returned `PanelActions` fields to the existing, unchanged handler methods (`handle_start`, `handle_stop`, `handle_continue`, `handle_generate_charts`, `handle_open_folder`, `scan_resumable_sessions`, `handle_resume_selected`).


## Revision Notes

- 2026-06-13: Initial ExecPlan authored from the discussion that followed the resume-automation feature. Scope: redesign the GUI third column into a single state-driven control panel wrapped in a vertical scroll area, fixing three observed problems — content clipping at small window sizes, a contradictory run count (an editable "100" shown beside a running series of a different size), and two competing resume controls shown at once. Built atop the already-committed resume feature (`AutomationStatus` variants, `GuiState.resumable_sessions`/`selected_resume`, and the `handle_*`/`scan_resumable_sessions` handlers), all of which remain unchanged; only rendering and click-dispatch change. Reason for the design: every one of the three problems stems from rendering all controls unconditionally, so branching the render on `AutomationStatus` and scrolling the column removes them at the source rather than papering over symptoms.
