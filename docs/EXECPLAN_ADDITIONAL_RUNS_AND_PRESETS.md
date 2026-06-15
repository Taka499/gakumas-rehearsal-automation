# Add "追加実行" (extend a finished run) and preset run-count buttons

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

This repository's ExecPlan conventions live in `docs/PLANS.md` (relative to the repository root). This document must be maintained in accordance with `docs/PLANS.md`.


## Purpose / Big Picture

This tool (`gakumas-screenshot`) is a Windows system-tray + GUI application that automates the game "Gakumas" rehearsal screen. It runs a chosen number of rehearsal cycles ("runs"); each run captures one screenshot into a timestamped session folder under `output/` and extracts nine scores per screenshot with OCR. A "run" is one full rehearsal cycle that produces one screenshot and one row in that session's `results.csv`. The main window is an `egui` GUI (an "immediate-mode" UI library: the interface is rebuilt from scratch every frame by calling functions like `ui.button(...)`, and a button "click" is simply that call returning a value whose `.clicked()` is true on the frame it was pressed). Its third column is a single state-driven control panel that shows only the controls relevant to the current automation state.

Two everyday frictions remain, and this plan removes both:

1. **No way to add more runs to a finished series.** Once a series of, say, 100 runs completes, the only way to gather more data for the same character lineup is to start a brand-new series in a *new* folder, leaving you with two folders and two `results.csv` files to merge by hand. After this change, a finished series shows a **「追加実行」(additional runs)** control: type or tap a count, press it, and the app performs that many *more* runs appended into the **same** session folder — continuing the iteration numbering, appending to the same `results.csv`, and regenerating the charts over the whole, longer series. The same control is also reachable from the idle screen's "前回の結果" (last results) shortcut, so you can add to the last run even after returning to the idle view.

2. **Setting the run count is fiddly.** The 実行回数 (run count) input is a single numeric box you drag or click-to-edit. Choosing a common value like 500 means dragging a long way or typing. After this change, a row of one-tap **preset buttons (100 / 200 / 500 / 1000)** sits under the count input; tapping one sets the count instantly. The same preset row appears under the 追加実行 count, so both "how many runs" inputs work the same way.

You can see it working like this. Open the app: under "実行回数:" you now see four buttons `100 200 500 1000`; tap `500` and the box jumps to 500. Run a short series (set it to a small number, start, let it finish). The completed panel now shows, below the chart/folder actions, a "追加実行" section with its own count input + preset row and a "➕ 追加実行" button. Put the game back on the rehearsal start page, set the additional count to 2, and press it: the app performs 2 more runs into the *same* folder, the progress line counts up past the original total, and when it finishes `results.csv` has the original rows plus 2 more (still one header), `screenshots/` has the extra files in sequence, and `run-meta.json` records the new, larger `total` with `"status": "completed"`. Press "← 戻る" to return to the idle screen: the "前回の結果" shortcut there offers the same 追加実行 control for that session.


## Progress

- [x] (2026-06-15Z) M1: Extend-capable engine. Added `extend_automation(session_dir, additional)` to `src/automation/runner.rs` (recompute completed from screenshots via `count_captured`, new total = completed + additional, reuse folder via the existing `start_automation_inner`) and re-exported it from `src/automation/mod.rs`. `cargo check` clean (`grep "^error"` → no output).
- [x] (2026-06-15Z) M2: Preset run-count buttons. Added `COUNT_PRESETS = [100, 200, 500, 1000]` and a reusable `render_count_input(ui, label, value)` helper to `src/gui/render.rs`; replaced the idle 実行回数 input block with a call to it. `cargo check` clean. Visual confirmation of the preset row in the idle panel pending (requires running the elevated binary).
- [ ] M3: 「追加実行」 GUI wiring. Add `additional_iterations` to `GuiState`, an `extend` flag to `PanelActions`, a `render_extend_section` helper, render it in the finished (non-resumable) state and in the idle "前回の結果" shortcut, add `handle_extend` to `GuiApp`, and dispatch it. Gate: `cargo build --release` clean; manual Scenarios A–D below.

Use timestamps (UTC) when checking off items, e.g. `- [x] (2026-06-15 14:00Z) ...`.


## Surprises & Discoveries

- (Add findings here as you implement, with short evidence snippets.)
- Anticipated borrow constraint (verify during M3): making `render_finished` take `&mut GuiState` (so the 追加実行 count input can mutate `state.additional_iterations`) is safe because `render_control_panel` already clones the status (`let status = state.status.clone();`) and passes `&status` — a borrow of that local clone, *not* of `state`. So a `&mut state` borrow and the `&status` borrow inside the same match arm do not conflict. If you instead pass `&state.status` you will see `error[E0502]`.


## Decision Log

- Decision: "追加実行" reuses the existing append-capable engine (`start_automation_inner`) rather than adding a new code path.
  Rationale: The resume feature already made the engine able to begin at an arbitrary iteration into an existing folder (`start_automation_inner(iterations, start_iteration, existing_session)`), append to `results.csv`, reuse `session.log`, and regenerate charts on finish. "Add N more runs" is exactly "start at completed+1, with total = completed+N, into the same folder." Building on the proven path keeps risk minimal.
  Date/Author: 2026-06-15 / planning.
- Decision: `extend_automation` recomputes `completed` from the screenshots on disk (`count_captured`) rather than trusting an in-memory count.
  Rationale: Screenshots are written synchronously before asynchronous OCR, so the screenshot count is the crash-proof source of truth (the same reason the resume feature uses it). Recomputing also means the caller only has to supply the folder and how many *more* runs — it cannot pass a stale total. It makes 追加実行 correct whether the prior run completed fully or stopped early.
  Date/Author: 2026-06-15 / planning.
- Decision: Show 追加実行 in two places — the finished 完了 panel (and any finished, non-resumable terminal state) and the idle "前回の結果" shortcut — but never alongside the resume control 続行.
  Rationale: This is the placement the user chose. It mirrors the panel's existing "one resume affordance per state" rule: an interrupted run (Aborted/Error with runs left) shows 続行 (finish what remained); a finished run shows 追加実行 (add more). The idle shortcut keeps 追加実行 reachable after pressing ← 戻る, which returns to Idle and would otherwise lose the finished-panel control.
  Date/Author: 2026-06-15 / planning.
- Decision: The finished-state 追加実行 is gated on `status.resumable().is_none()` so it is mutually exclusive with 続行.
  Rationale: `续行` (resume) and `追加実行` (extend) both append to the same folder, but they answer different questions ("finish the remaining runs" vs "add brand-new runs"). Showing both at once on an interrupted run would be confusing. Finish first (続行); once finished, the panel offers 追加実行.
  Date/Author: 2026-06-15 / planning.
- Decision: Preset values are `100 / 200 / 500 / 1000`, held in a single `COUNT_PRESETS: [u32; 4]` constant and rendered by one shared `render_count_input` helper used by both the idle count and the 追加実行 count.
  Rationale: The user chose these values. A single constant + shared helper means the presets are defined once and both inputs behave identically; changing the presets later is a one-line edit.
  Date/Author: 2026-06-15 / planning.
- Decision: Add a separate `additional_iterations: u32` field to `GuiState` (default 100) rather than reusing `iterations`.
  Rationale: `iterations` is the fresh-run count shown when idle; the 追加実行 count is a distinct quantity ("how many *more*"). A separate field avoids the two controls fighting over one value and avoids a 追加実行 count bleeding into the next fresh run's default.
  Date/Author: 2026-06-15 / planning.


## Outcomes & Retrospective

To be completed at the end of each milestone and at full completion. Compare against Purpose: can the user (a) add more runs to a finished series into the same folder with unified output, from both the finished panel and the idle shortcut, and (b) set either run count with one tap on a 100/200/500/1000 preset?


## Context and Orientation

You are working in a Rust 2024-edition Windows application. Build from the repository root (`C:\Work\GitRepos\gakumas-screenshot`) with `cargo build` / `cargo build --release`. The executable carries an administrator manifest, so `cargo test` cannot launch the test binary (it fails with an elevation error). Therefore the compile gate for this work is `cargo check`, and behavioral acceptance is manual (running the app and looking at the window, with the game open for the run-related scenarios). Build emits ~30 expected warnings (unused `pub use` re-exports, OCR dead code); these are not regressions — find real failures with `cargo check 2>&1 | grep "^error"`.

Define the terms used below in plain language: a "session folder" is one directory under `output/` named with a timestamp like `20260615_141500` that holds one series' `screenshots/`, `results.csv`, `session.log`, `charts/`, and `run-meta.json`. "Captured" means a screenshot was saved to disk for that run (the unit counted as completed). "Resume / 続行" means run the *remaining* iterations of an interrupted series into its existing folder. "Extend / 追加実行" (the new feature) means run *additional brand-new* iterations onto a series that already reached its target, into the same folder.

Four files matter for this work. Read them before editing so the snippets below land in the right place:

- `src/automation/runner.rs` — Entry point for automation. It already exposes `start_automation(max_iterations: Option<u32>) -> Result<()>` (fresh run) and `resume_automation(session_dir: PathBuf, completed: u32, total: u32) -> Result<()>` (finish an interrupted run), both thin wrappers over the private `start_automation_inner(iterations: u32, start_iteration: u32, existing_session: Option<PathBuf>) -> Result<()>`. `start_automation_inner` finds the game window, reuses or creates the session folder, sets up CSV/log, **seeds the GUI progress atomics** (`CURRENT_ITERATION` to `start_iteration - 1`, `TOTAL_ITERATIONS` to `iterations`), writes `run-meta.json` with `status: "running"`, and spawns the automation thread. On finish, `run_automation_loop` rewrites `run-meta.json` with the final status/`completed` and stores the outcome. You will add `extend_automation` next to `resume_automation`, mirroring its shape. The relevant getters already exist: `get_current_iteration()`, `get_total_iterations()`, `get_current_session_path() -> Option<PathBuf>`.
- `src/automation/session_meta.rs` — Persistence + discovery. `count_captured(session_dir: &Path) -> u32` counts `.png` files in `session_dir/screenshots` and is the crash-proof count of completed runs. `extend_automation` will call it. No changes needed in this file.
- `src/automation/mod.rs` — Module list and re-exports. The runner re-export line currently reads `pub use runner::{is_automation_running, request_abort, resume_automation, start_automation};`. You will add `extend_automation` to it.
- `src/gui/state.rs` — Defines `enum AutomationStatus` (variants `Idle`; `Running { current, total, state_description, start_time }`; `Completed { completed, total, session_path }`; `Aborted { completed, total, session_path: Option<PathBuf> }`; `Error { completed, total, message, session_path: Option<PathBuf> }`; derives `Clone, Debug`) and `struct GuiState`. `AutomationStatus` has `resumable(&self) -> Option<(u32, u32, PathBuf)>` returning `Some` only for an interrupted Aborted/Error with `completed < total` and a known folder. `GuiState` today holds `iterations: u32` (default 100), `status`, `latest_session_path: Option<PathBuf>`, `automation_start_time: Option<Instant>`, `resumable_sessions: Vec<ResumableSession>`, `selected_resume: Option<usize>`. You will add `additional_iterations: u32`.
- `src/gui/render.rs` — Pure rendering helpers. `render_control_panel(ui, &mut GuiState) -> PanelActions` clones the status and dispatches to private helpers `render_idle`, `render_running`, `render_finished`, plus `render_generated_files` and `render_resume_section`. `PanelActions` is a `#[derive(Default)]` struct of `bool` click flags (`start`, `stop`, `continue_run`, `generate_charts`, `open_folder`, `refresh_resumable`, `resume_selected`, `back_to_idle`, `dismiss_selected`). You will add an `extend` flag, a `COUNT_PRESETS` constant, a `render_count_input` helper, and a `render_extend_section` helper, and you will edit `render_idle` and `render_finished`.
- `src/gui/mod.rs` — The egui application (`struct GuiApp`). `update()` calls `render::render_control_panel(ui, &mut self.state)` once inside a `ScrollArea` in column three and dispatches each `PanelActions` flag to a `handle_*` method. The import line is `use crate::automation::runner::{get_last_outcome, is_automation_running, resume_automation, start_automation, AutomationOutcome};`. Existing handlers `handle_start`, `handle_continue`, `handle_resume_selected` show the pattern you will copy for `handle_extend`. You will add `extend_automation` to the import, add `handle_extend`, and dispatch `actions.extend`.

The exact current idle count-input block in `src/gui/render.rs` `render_idle` (the lines you will replace in M2) is:

        ui.horizontal(|ui| {
            ui.label("実行回数:");
            ui.add(
                egui::DragValue::new(&mut state.iterations)
                    .range(1..=9999)
                    .speed(1.0),
            );
            ui.label("回");
        });

The exact current "前回の結果" block in `render_idle` (which you will augment in M3) is:

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
        }


## Plan of Work

Three milestones, each compiling cleanly with `cargo check`. M1 adds the engine entry point with no UI change (provable by a one-line programmatic reasoning + compile). M2 adds the preset buttons (immediately visible in the idle panel, no game needed). M3 wires 追加実行 into the GUI in both placements (needs the game to fully demonstrate, but compiles and renders without it). M2 and M3 both touch `src/gui/render.rs`; do M2 first because M3's 追加実行 section reuses M2's `render_count_input`.


### Milestone M1 — Extend-capable engine

Goal: a public `extend_automation(session_dir, additional)` that appends `additional` brand-new runs onto an existing folder. Nothing in the GUI calls it yet, so behavior is unchanged; success is `cargo check` plus reading the code to confirm it reuses `start_automation_inner` correctly.

In `src/automation/runner.rs`, add this function immediately after `resume_automation` (it mirrors `resume_automation`'s validation-then-delegate shape):

    /// Extends a finished run with `additional` brand-new iterations, appending
    /// into its existing folder.
    ///
    /// The number of already-captured runs is recomputed from the screenshots on
    /// disk (the crash-proof source of truth), so the caller need only say how
    /// many *more* runs to perform. New iterations are numbered `completed + 1`
    /// through `completed + additional`, reusing the same screenshots/,
    /// results.csv, session.log, and run-meta.json (whose `total` becomes the new,
    /// larger value).
    pub fn extend_automation(session_dir: PathBuf, additional: u32) -> Result<()> {
        if additional == 0 {
            return Err(anyhow!("Nothing to add: requested 0 additional runs"));
        }
        if !session_dir.exists() {
            return Err(anyhow!(
                "Cannot extend: session folder no longer exists: {}",
                session_dir.display()
            ));
        }
        let completed = crate::automation::session_meta::count_captured(&session_dir);
        let new_total = completed + additional;
        start_automation_inner(new_total, completed + 1, Some(session_dir))
    }

Why this is enough: `start_automation_inner(new_total, completed + 1, Some(session_dir))` reuses the folder, seeds `CURRENT_ITERATION = completed` and `TOTAL_ITERATIONS = new_total` (so the GUI progress bar continues past the original total), calls `init_csv` (a no-op when the CSV already has rows, so no duplicate header), reactivates the existing `session.log` in append mode, and the state machine begins at `start_iteration = completed + 1`. Because the first iteration's `current_iteration` equals `start_iteration`, the End-button retry is skipped on that first added run (the game is sitting on the rehearsal start page, exactly as for a resume). On finish, `run_automation_loop` rewrites `run-meta.json` with `total = new_total` and the final `completed`, and regenerates charts over the whole folder via the GUI's `finalize_status` path.

Then, in `src/automation/mod.rs`, add `extend_automation` to the runner re-export so the GUI can import it. Change:

    pub use runner::{is_automation_running, request_abort, resume_automation, start_automation};

to:

    pub use runner::{
        extend_automation, is_automation_running, request_abort, resume_automation, start_automation,
    };

Compile-check (`cargo check`). Expect no errors. If you see `unused ... extend_automation`, that is the expected re-export warning until M3 calls it (the same warning the resume feature saw for `resume_automation` between its milestones); it is not a failure.


### Milestone M2 — Preset run-count buttons

Goal: under the idle 実行回数 input, a row of `100 200 500 1000` buttons; tapping one sets the count. Achieved with a reusable helper so M3 can reuse it for the 追加実行 count.

In `src/gui/render.rs`, add near the top of the file (after the `use` lines, before `render_guide_image`) the presets constant and the shared input helper:

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

Then, in `render_idle`, replace the current idle count-input block (the `ui.horizontal(|ui| { ui.label("実行回数:"); ... ui.label("回"); });` shown in Context) with a single call:

    render_count_input(ui, "実行回数:", &mut state.iterations);

Leave the rest of `render_idle` (the ▶ 開始 button, the "前回の結果" shortcut, the resume section) unchanged for now.

Compile-check (`cargo check`). Expect no errors. Then run the app idle (no game needed) and confirm the `100 200 500 1000` row appears under "実行回数:" and that tapping `500` sets the box to 500 and `1000` sets it to 1000.


### Milestone M3 — 「追加実行」 in the finished panel and the idle shortcut

Goal: a finished, non-resumable series shows a 追加実行 section (count input + presets + button); the idle "前回の結果" shortcut shows the same; pressing it runs that many more runs into the same folder.

First, in `src/gui/state.rs`, add the field to `struct GuiState` (place it after `iterations`):

    /// Number of *additional* runs for the 追加実行 (extend) control, kept
    /// separate from `iterations` so the fresh-run count and the extend count do
    /// not overwrite each other.
    pub additional_iterations: u32,

and initialize it in `impl Default for GuiState` (alongside `iterations: 100,`):

    additional_iterations: 100,

Next, in `src/gui/render.rs`, add the `extend` flag to `PanelActions` (after `dismiss_selected`):

    /// Run additional iterations into the most recent session's folder.
    pub extend: bool,

Add a private `render_extend_section` helper (place it after `render_count_input`, or anywhere among the private helpers). It reuses `render_count_input` from M2:

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

Now render it in the **finished** state. In `render_finished`, two edits are needed. First, change its signature so it can mutate the extend count — change the second parameter from `state: &GuiState` to `state: &mut GuiState`:

    fn render_finished(
        ui: &mut egui::Ui,
        state: &mut GuiState,
        status: &AutomationStatus,
        actions: &mut PanelActions,
    ) {

(This compiles because `render_control_panel` passes `&status`, a borrow of the local status *clone*, not of `state`; see Surprises & Discoveries. The existing `state.latest_session_path.is_some()` read still works through a `&mut` borrow.)

Second, in `render_finished` the local `session_path` is currently *consumed* by the generated-files block:

        if let Some(path) = session_path {
            render_generated_files(ui, &path);
        }

Change that to borrow instead of move, so `session_path` stays usable for the 追加実行 gate:

        if let Some(path) = session_path.as_ref() {
            render_generated_files(ui, path);
        }

Then, at the **end** of `render_finished` (after the アクション `📁 フォルダを開く` block), add the 追加実行 section, gated so it never appears next to 続行 and only when the finished session has a folder:

        // 追加実行 (extend): only for a finished series that is NOT resumable
        // (Completed, or a non-resumable terminal state) and that has a folder.
        // Mutually exclusive with the 続行 button rendered above.
        if status.resumable().is_none() && session_path.is_some() {
            render_extend_section(ui, &mut state.additional_iterations, actions);
        }

Now render it in the **idle** shortcut. In `render_idle`, inside the existing `if state.latest_session_path.is_some() { ... }` "前回の結果" block, after the `📁 フォルダを開く` button and before the block's closing brace, add:

            render_extend_section(ui, &mut state.additional_iterations, actions);

This gives the idle shortcut the same 追加実行 control for the most recent session.

Finally, wire the handler. In `src/gui/mod.rs`, add `extend_automation` to the runner import:

    use crate::automation::runner::{
        extend_automation, get_last_outcome, is_automation_running, resume_automation,
        start_automation, AutomationOutcome,
    };

Add the handler method on `GuiApp` (place it near `handle_continue`). It reads the most recent session folder from `latest_session_path` (which is set both when a run finishes and when one starts, and points to the finished folder in both the finished state and the idle shortcut), starts the extend, then derives the new `total`/`current` for the Running status straight from the runner's seeded atomics so the progress display is correct without the GUI recomputing the completed count:

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
                self.state.latest_session_path =
                    crate::automation::runner::get_current_session_path();
                // start_automation_inner has already seeded these atomics:
                // TOTAL_ITERATIONS = completed + additional, CURRENT_ITERATION = completed.
                let total = crate::automation::runner::get_total_iterations();
                let current = crate::automation::runner::get_current_iteration();
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

Dispatch it in `update()` where the other `PanelActions` flags are handled (inside the `ScrollArea` closure in column three), adding one line alongside the existing dispatches:

    if actions.extend { self.handle_extend(); }

Build a runnable binary (`cargo build --release`). Expect `Finished release` with no errors; the binary is `target\release\gakumas-screenshot.exe`.


## Concrete Steps

Run all commands from the repository root `C:\Work\GitRepos\gakumas-screenshot` in PowerShell.

1. Implement M1, then compile-check:

        cargo check

   Expected: `Finished` with no `error:` lines (filter with `cargo check 2>&1 | grep "^error"` — expect no output). A lone `unused ... extend_automation` warning is expected until M3.

2. Implement M2, then:

        cargo check

   Expected: no errors. Common mistakes: leaving the old `ui.horizontal(|ui| { ui.label("実行回数:"); ... })` block in place *and* adding `render_count_input` (you would get two count inputs) — replace, do not duplicate. If `COUNT_PRESETS` is reported unused, you have not yet called `render_count_input` from `render_idle`; wire that call.

3. Implement M3, then build:

        cargo build --release

   Expected: `Finished release` with no errors. If you see `error[E0502]` mentioning `state`, confirm `render_finished` takes `state: &mut GuiState` and that `render_control_panel` still passes `&status` (the clone), not `&state.status`. If you see "expected 1 argument, found 2" or a missing-field error around `GuiState`, confirm `additional_iterations` was added to both the struct and its `Default`. Grep to confirm the new symbols are wired:

        git grep -n "extend_automation\|render_extend_section\|additional_iterations\|render_count_input" src

   Expected: `extend_automation` in `runner.rs` (def), `mod.rs` (re-export), and `gui/mod.rs` (import + call); `render_count_input` defined once and called from `render_idle` and `render_extend_section`; `render_extend_section` defined once and called from `render_idle` and `render_finished`; `additional_iterations` in `state.rs` (field + default) and `gui/mod.rs`/`gui/render.rs`.


## Validation and Acceptance

Because the executable requires administrator elevation, automated `cargo test` cannot run; acceptance is the following manual checks. Scenario A needs no game; B–D need the game on the rehearsal start page. Launch the built app (elevated if the game runs elevated):

    .\target\release\gakumas-screenshot.exe

Scenario A — Preset buttons set the count (M2, no game needed). With the app idle, confirm a row `100 200 500 1000` appears directly under "実行回数:". Tap `500`; the numeric box shows 500. Tap `1000`; it shows 1000. Drag/type still works. This proves the presets and the shared input helper.

Scenario B — Extend a completed series (M1+M3). With the game on the rehearsal start page, set 実行回数 to a small number (e.g. tap a preset then drag down to 2, or type 2) and press ▶ 開始. Let both runs finish; the panel shows green "完了 (2/2回) → <folder>", the 生成ファイル list, the アクション buttons, **and** below them a "追加実行" section with a "⚠ ②のリハーサル開始画面に戻してから追加実行してください" warning, a "追加回数:" input with its own `100 200 500 1000` preset row, and a "➕ 追加実行" button — and **no** "⏵ 続行" button (a completed run is not resumable). Set 追加回数 to 2 and, with the game back on the rehearsal start page, press ➕ 追加実行. The panel switches to the 実行中 layout; the line reads "4回 実行中 — N回目" with N advancing 3→4 (the total is now 4, not 2), and the progress bar continues past 50%. When it finishes, open the folder (📁 フォルダを開く) and confirm: `screenshots/` holds files numbered `001`–`004` in sequence; `results.csv` has exactly one header line plus four data rows (no duplicate header); `run-meta.json` contains `"total": 4`, `"completed": 4`, `"status": "completed"`. This proves 追加実行 appends into the same folder with continuous numbering and unified output.

    Expected results.csv (abbreviated) after the extend:
    iteration,timestamp,screenshot,s1c1,...,s3c3
    1,...
    2,...
    3,...
    4,...

Scenario C — Extend from the idle shortcut (M3). After Scenario B finishes, press "← 戻る" to return to Idle. In the "前回の結果" section confirm the same "追加実行" control (warning + 追加回数 input + presets + ➕ 追加実行) appears under the 📊/📁 buttons. With the game on the rehearsal start page, set 追加回数 to 1 and press ➕ 追加実行. The app performs one more run into the same folder; when done, the folder has `005` and `results.csv` has five data rows. This proves 追加実行 is reachable after returning to idle.

Scenario D — 追加実行 and 続行 never coexist (M3). Start a fresh series of 3, abort after 1 with Ctrl+Shift+Q. The panel shows the amber "中断 (1/3回 完了)" summary with exactly one "⏵ 続行 (残り 2回)" button and **no** "追加実行" section (an interrupted run is resumable, so it is finished with 続行 first). Put the game back on the rehearsal start page and press ⏵ 続行; let it complete. Now the panel reads "完了 (3/3回)" and the "追加実行" section appears while 続行 is gone. This proves the mutual exclusivity rule.

If every scenario matches, both features meet their purpose: a finished series can be extended into the same folder from two places, the count never collides with 続行, and either run-count can be set with one preset tap.


## Idempotence and Recovery

All steps are safe to repeat. `cargo check` / `cargo build` are idempotent. `extend_automation` is built on the same append-only foundations as resume: `init_csv` never clobbers existing rows, screenshot/log writes are append-only, and `count_captured` re-reads the screenshots directory each time, so extending the same folder repeatedly only ever adds data and recomputes the start point correctly. `run-meta.json` is overwritten wholesale at start and finish, so each extend simply refreshes it with the new, larger total. If the chosen session folder was deleted between finishing and pressing 追加実行, `extend_automation` returns an error that the GUI logs (and the panel stays put); nothing is corrupted.

The change is confined to five files (`src/automation/runner.rs`, `src/automation/mod.rs`, `src/gui/state.rs`, `src/gui/render.rs`, `src/gui/mod.rs`). To revert to pre-feature behavior, `git checkout -- src/automation/runner.rs src/automation/mod.rs src/gui/state.rs src/gui/render.rs src/gui/mod.rs`. To clean up test data, delete the extra session folders under `output/` (each is self-contained; there is no undo).


## Artifacts and Notes

The third column's behavior by state, after this change (additions over the prior design in **bold** intent):

    Idle      : "③ 回数を設定して開始"  [実行回数: N 回]  [100][200][500][1000]  [▶ 開始]
                前回の結果: [📊][📁]  追加実行: [追加回数: M 回][100][200][500][1000][➕ 追加実行]
                (+ resume picker only if interrupted sessions exist on disk)
    Running   : "実行中"  "4回 実行中 — 3回目"  ⚠  [████░░] 75%  経過 mm:ss  [◼ 停止]
    Aborted   : "中断"   "中断 (1/3回 完了)"   [██░░░░]  [⏵ 続行 (残り 2回)]  生成ファイル … アクション …   (no 追加実行)
    Completed : "完了"   "完了 (4/4回) → folder"  [██████] 100%  生成ファイル … アクション …  追加実行: [追加回数][presets][➕ 追加実行]

Representative log lines distinguishing a fresh start, a resume, and an extend (in `session.log` and the global log):

    [14:15:00.123] Starting automation: 2 iterations (Ctrl+Shift+Q to abort)
    [14:16:10.456] Resuming automation from iteration 2/3 (Ctrl+Shift+Q to abort)
    [14:17:20.789] GUI: 追加実行 2回 → 4 (folder ...\output\20260615_141500)

(`extend_automation` reuses `start_automation_inner`'s "Resuming automation from iteration k/total" log line, since an extend is mechanically a resume from `completed+1` to the new total; the GUI-side `追加実行 N回 → total` line above distinguishes it in the log.)


## Interfaces and Dependencies

Use only crates already in `Cargo.toml`: `anyhow` for errors, `eframe`/`egui` for the UI. No new dependencies.

In `src/automation/runner.rs`, at the end of M1:

    pub fn extend_automation(session_dir: std::path::PathBuf, additional: u32) -> anyhow::Result<()>;

re-exported from `src/automation/mod.rs` alongside `start_automation`/`resume_automation`.

In `src/gui/state.rs`, `GuiState` must gain `pub additional_iterations: u32` (default 100).

In `src/gui/render.rs`, at the end of M3 these must exist:

    const COUNT_PRESETS: [u32; 4];
    fn render_count_input(ui: &mut eframe::egui::Ui, label: &str, value: &mut u32);
    fn render_extend_section(ui: &mut eframe::egui::Ui, additional: &mut u32, actions: &mut PanelActions);
    // PanelActions gains: pub extend: bool
    // render_finished's signature changes to take state: &mut GuiState
    // render_idle calls render_count_input (idle count) and render_extend_section (前回の結果 shortcut)

In `src/gui/mod.rs`, `GuiApp` must gain `fn handle_extend(&mut self)`, must import `extend_automation`, and `update()` must dispatch `if actions.extend { self.handle_extend(); }`.


## Revision Notes

- 2026-06-15: Initial ExecPlan authored from the request to add a "追加実行" (additional runs) feature and preset run-count buttons. Scope: an `extend_automation` engine entry point that appends N brand-new runs into a finished series' existing folder by reusing the resume feature's append-capable `start_automation_inner`, surfaced in the GUI in both the finished 完了 panel and the idle "前回の結果" shortcut (the user's chosen placement); plus a shared `render_count_input` helper with `100/200/500/1000` preset buttons (the user's chosen values) used by both the idle 実行回数 input and the 追加実行 count. Built atop the already-committed resume feature (`start_automation_inner`, `count_captured`, the state-driven `render_control_panel`), all of which remain unchanged except for the additive entry point, one widened render signature, and the new GuiState field. Reason for the design: "add more runs" is mechanically "resume from completed+1 with a larger total," so the proven append path is reused rather than duplicated, and `count_captured` keeps the start point correct without trusting in-memory state.
