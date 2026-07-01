# Live Box-Plot View During an OCR Automation Run

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

This plan must be maintained in accordance with `docs/PLANS.md` (read it in full before implementing). That file defines what a good ExecPlan is; this file is one such plan and must keep all of its required sections current.


## Purpose / Big Picture

Today, when a user runs the rehearsal automation (the tool that repeatedly screenshots `gakumas.exe`, OCRs the nine per-character rehearsal scores, and writes them to a CSV), they see only a progress bar and an iteration counter while the run is in flight. The score distribution charts — including the combined box plot of all nine score columns — are generated **only after the run finishes**, by reading the completed CSV. A user running 500 or 1000 iterations gets no feedback on how the distribution is shaping up until the very end.

After this change, the GUI's third-column control panel will show, **while a run is in progress**, a live "score distribution" figure: nine box plots side by side (one per stage/character-slot column, labelled `S1C1 … S3C3`), each with a small block of live statistics printed directly beneath it — Mean, Median, Max, Min, Q1 (the 25th-percentile, i.e. the bottom of the box), and Q3 (the 75th-percentile, the top of the box). The figure refreshes as each new iteration's OCR result arrives, so the boxes visibly grow and settle as data accumulates.

A "term of art" used throughout: a **box plot** is a compact picture of one column of numbers. The filled box spans from the first quartile (Q1, the value below which 25% of the data falls) to the third quartile (Q3, 75%); a line inside the box marks the median (the middle value); and thin "whiskers" extend from the box down to the minimum and up to the maximum value. We draw nine of these next to each other, one per score column.

Two product decisions are fixed for this plan (see Decision Log for rationale):

1. **Flagged rows are excluded from the live figure until verified.** A "flagged" row is one where the OCR overlap-recovery logic could not confidently reconstruct the score (the worst of its three stages came back `flagged`, written as the 13th CSV column `recovery=flagged`). Such a row is omitted from the live statistics so the live box plot is never skewed by an unconfirmed value. Rows whose recovery is `ok`, `repaired`, `manual`, or `verified` are all included.
2. **One combined figure, nine boxes in a single row, statistics printed below each box.** We do not add per-column pop-out charts to the live view; the value is the at-a-glance combined distribution.

You will know it works when: in the idle panel you tick a "ライブ分布を表示" (Show live distribution) checkbox (next to the run-count, before Start), start an automation run, and watch a nine-box figure appear and update on every completed iteration, with the six statistics under each box changing as new scores land — and a flagged iteration does **not** move the boxes. The checkbox is a pre-run choice precisely because, once the run starts, the automation controls the mouse and the user is told not to move it.


## Progress

- [x] (2026-07-01) M1 — Live score buffer plumbed from the OCR worker to a process-global store in `runner.rs`, reset on a fresh run and seeded from the existing CSV on resume/extend, excluding flagged rows. Unit test `live_score_buffer_records_and_excludes_flagged` passes; `cargo check` clean.
- [x] (2026-07-01) M2 — In-memory renderer `render_live_box_plot_rgba` in `analysis/charts.rs` (nine boxes + six-line stats block per column, RGBA buffer, no file I/O), backed by `DataSetStats::from_score_rows` in `analysis/statistics.rs`. Tests `render_live_box_plot_*`, `group_thousands_*`, `from_score_rows_*` pass; `#[ignore]`d `live_box_plot_preview` produced a clean `temp/live_box_plot_preview.png` (verified visually). `cargo check` clean.
- [x] (2026-07-01) M3 — GUI wiring code-complete: `show_live_chart` preference on `GuiState`; `live_chart_tex`/`live_chart_rendered_count` on `GuiApp`; `GuiApp::update_live_chart` rebuilds the texture only when `live_score_count()` changes; "ライブ分布を表示" **pre-run checkbox in `render_idle`** (above Start), with `render_running` displaying the figure inline (texture threaded through `render_control_panel`). `cargo check` clean, full suite 116 passed, guarded release build OK (28 expected warnings). Remaining: manual click-through against the live game (cannot be automated here — needs `gakumas.exe`).
- [x] (2026-07-01) M3 fix — moved the show/hide control out of the running panel (unreachable once the run owns the mouse) into the idle panel as a pre-run preference; removed the dead `PanelActions::toggle_live_chart` plumbing. See Decision Log.
- [x] (2026-07-01) M3 layout — the figure was unreadably small in the control column. Moved it to a wide resizable right `SidePanel`; the window grows while shown and shrinks back when hidden (`GuiApp::live_chart_expanded`). Removed the first guide image column and renumbered steps (rehearsal-page guide → ①, control step → ②). See Decision Log.
- [x] (2026-07-01) M3 restart-seed — on launch, seed the live buffer from the newest session's CSV (`GuiApp::new` → `reload_live_scores_from_csv(latest_session_path)`), so the plot/table show the last run after a restart instead of zeros (the process-global buffer is otherwise empty). A new run still clears the buffer first, so it does not bleed in.
- [x] (2026-07-01) M3 review-sync — after a review save (manual correction or ✓ verify), reload the live buffer from the saved CSV (`runner::reload_live_scores_from_csv`) and force a re-render (`live_chart_dirty`), so the live figure + table reflect the fixed/verified values and re-include rows whose flag was cleared. A `ctx.request_repaint()` guarantees the follow-up frame.
- [x] (2026-07-01) M3 polish 3 — legibility pass: bumped plot fonts (column labels 28px bold, axis labels/desc 22px), thickened box/median strokes (3px) and whiskers/caps (2px), abbreviated y-axis ticks with "k"; bumped the table font to 16px. Sized for the 1200px→panel downscale.
- [x] (2026-07-01) M3 polish 2 — removed the header panel's horizontal separator line (`show_separator_line(false)`; the columns originally had none) and made the stats table span the full panel width (equal `min`/`max_col_width` from `available_width`, centered cells via `stat_cell`); switched the plot panel to a vertical-only scroll area so the table width is bounded.
- [x] (2026-07-01) M3 polish — (a) toggle defaults on and is persisted to `gui_settings.json` (`load/save_gui_settings`, `saved_show_live_chart`); (b) window expands the moment the box is ticked, not at run start (`show_live_panel = show_live_chart`), with the initial viewport sized to the persisted value to avoid a launch flash; (c) per-column stats moved out of the plot image into a live 6×9 egui table (`render::render_live_stats_table` + `abbrev_k`), plot image simplified to boxes+labels (1200×600); (d) layout reworked to top header + fixed 300px left guide panel + 760px right plot panel + narrow central control. `cargo check` clean, 115 tests pass, guarded release build OK. See Decision Log.

Use timestamps (UTC) as steps complete, e.g. `- [x] (2026-06-30 14:00Z) M1 done`.


## Surprises & Discoveries

- Observation: The post-run analysis path (`src/analysis/csv_reader.rs::DataSet::from_csv`) parses only the first 12 CSV columns and **ignores** the 13th `recovery` column, so the *final* charts currently include flagged rows. The live view deliberately differs (it excludes flagged), which is a genuine behavior difference, not an oversight.
  Evidence: `csv_reader.rs::parse_line` checks `parts.len() < 12` and reads indices `0..=11` only; there is no read of `parts[12]`.
- (Add further discoveries here as you implement — e.g. plotters in-memory backend quirks, font sizing for nine columns at 1200px width.)


## Decision Log

- Decision: Exclude `recovery=flagged` rows from the live figure; include `ok`/`repaired`/`manual`/`verified`.
  Rationale: A flagged row's score is explicitly "not confidently recovered." Letting it move the live boxes would mislead the user mid-run. Verification (which turns `flagged` into `verified` or `manual`) happens in the review window, generally after a run; the live view simply waits for confidence.
  Date/Author: 2026-06-30 / Taka499 (decided in design discussion).

- Decision: One combined nine-box figure with statistics under each box, rendered as a single image; no live per-column charts.
  Rationale: The at-a-glance combined distribution is the high-value live signal; nine extra live charts cost screen real estate and render time for marginal benefit.
  Date/Author: 2026-06-30 / Taka499.

- Decision: Render the live figure with the existing `plotters` crate into an in-memory RGBA buffer (via `BitMapBackend::with_buffer`) and upload it as an egui texture each time the data changes, rather than (a) re-running the full on-disk `generate_analysis_for_session` per iteration, or (b) hand-drawing boxes with the egui `Painter`.
  Rationale: (a) is heavy — it reads the CSV, recomputes nine columns, and writes ~10 PNGs to disk per iteration. (b) reimplements box-plot geometry and the stats layout from scratch. Reusing the proven plotters drawing code into a memory buffer is cheap, has zero disk I/O, and keeps the live figure visually consistent with the final combined box plot. Regeneration is gated on the iteration counter changing (new data points arrive every few seconds, not every frame), so cost is negligible.
  Date/Author: 2026-06-30 / Taka499.

- Decision: Store every non-discarded row in the live buffer tagged with a `flagged: bool`, and filter at statistics-compute time, rather than dropping flagged rows at insert time.
  Rationale: Keeping flagged rows (but excluding them from stats) lets the figure caption report "(N flagged, excluded)" so the user understands why the live count may lag the iteration counter. It also leaves room for a future "verify mid-run" feature to un-flag without re-deriving the buffer.
  Date/Author: 2026-06-30 / Taka499.

- Decision: (a) The live toggle **defaults on and is persisted** across restarts in `gui_settings.json` next to the exe; (b) the window **expands the instant the checkbox is ticked** (in the idle panel), not when a run starts; (c) the per-column statistics are shown as a **live 6×9 egui table** (rows Avg/Med/Max/Min/Q1/Q3 × columns S1C1..S3C3) instead of being drawn into the plot image, using a `k` abbreviation (`284k`, `1341k`) so nine columns fit; (d) the **control column is kept narrow** (~320px) by laying the UI out as a top header panel + fixed-width left guide panel + wide right plot panel + central control, rather than equal `columns(2)`.
  Rationale: (a) The user wants the live view by default and remembered — a tiny portable JSON file matches the app's other exe-adjacent config and avoids depending on eframe's appdata storage. Persist only on change (tracked by `saved_show_live_chart`). (b) Tying the window size and panel visibility to `show_live_chart` alone (not to the existence of a texture) makes ticking the box immediately expand and show the empty figure, which is what the user expects. `live_chart_expanded` is seeded from the persisted value and the initial viewport is sized to match, so launch doesn't flash-resize. (c) The in-image stats text was redundant and cramped; a real table updates crisply and reads better, and `k` keeps the six-figure scores narrow. The plot image is now boxes + column labels only, and is shorter (1200×600). (d) Fixed-width side panels give a predictably narrow control column close to the original three-column width.
  Date/Author: 2026-07-01 / Taka499 (raised: checkbox not remembered + should default on, expand on check not on start; stats redundant → table with k abbreviation; control panel too wide).

- Decision: Show the live figure in a **wide, resizable right-hand `SidePanel`** and **grow the window** (800×580 → 1640×760) while it is visible, instead of squeezing it into the narrow third control column. Also **removed the first guide image column** ("コンテストで「リハーサル」を選択") and renumbered the remaining steps (rehearsal-page guide ② → ①, control-panel step ③ → ②).
  Rationale: Inside the ~260px control column the nine-box figure was unreadably small. A dedicated right panel (default 820px, user-resizable) makes it legible, and expanding the window only while the plot is shown keeps the default footprint small. The first guide image is the least essential of the two (the second, rehearsal-page, is the screen the user must actually be on), so dropping it frees horizontal space. The window resizes once on show/hide (tracked by `GuiApp::live_chart_expanded`) rather than every frame, so it never fights a user's manual resize. The panel reads `live_chart_tex` directly, so the texture no longer needs threading through `render_control_panel`/`render_running`.
  Date/Author: 2026-07-01 / Taka499 (raised: plot too small, expand window, drop first column, put plot right of the control panel).

- Decision: The show/hide control is a **pre-run preference checkbox in the idle panel** (next to the run-count, above Start), not an interactive toggle in the running panel. The running panel only *displays* the figure when the preference is on.
  Rationale: The first M3 implementation put the checkbox in the running panel, but that panel only appears after Start — at which point the automation has taken over the mouse and the user is explicitly warned not to move it, so the checkbox was unreachable. Deciding before the run (the same place the run count is set) and then auto-displaying needs no mouse interaction mid-run. The preference persists in `GuiState`, so a resumed/continued run honors the choice made before the original run. Because the checkbox sits in `render_idle` (which already holds `&mut GuiState`), it mutates `state.show_live_chart` directly and the `PanelActions::toggle_live_chart` plumbing was removed as dead code.
  Date/Author: 2026-07-01 / Taka499 (raised after manual review of the M3 UX).


## Outcomes & Retrospective

M1–M3 are code-complete on branch `feature/live-box-plot`. The data path (OCR worker → `LIVE_SCORES` → GUI), the in-memory plotters renderer, and the GUI toggle/display are all in place and unit-tested; the full suite passes (116) and the optimized release build links. The `#[ignore]`d `live_box_plot_preview` test produced a figure that matches the intended design exactly: nine per-stage-colored boxes in one row with an aligned six-line stats block (Avg/Med/Max/Min/Q1/Q3) under each, and a title reporting the run count and flagged-excluded count.

Decisions held up cleanly in implementation:

- Excluding flagged rows is a single `.filter(|r| !r.flagged)` at stats-compute time in `update_live_chart`, with the excluded count surfaced in the figure title — so a flagged iteration visibly does not move the boxes and the user can see why the plotted count trails the iteration counter.
- Reusing the existing plotters box geometry into an in-memory RGB buffer (then RGB→RGBA for egui) avoided any disk I/O and kept the live figure visually consistent with the on-disk `chart_combined.png`.
- The row-count guard (`count == live_chart_rendered_count`) makes idle frames free; the figure only re-renders on a genuinely new data point.

Remaining gap: the live click-through against a running `gakumas.exe` cannot be exercised in this environment (no game window, and the live GUI/hotkeys require manual testing per the repo's testing constraints). The manual acceptance steps in "Validation and Acceptance" are the proof to run on the dev machine. The one deliberate, documented behavior difference from the final charts (live excludes flagged; the post-run on-disk charts currently include them) is captured in Surprises & Discoveries.


## Context and Orientation

This is a Windows-only Rust application (Rust 2024 edition) that screenshots the game `gakumas.exe` and OCRs rehearsal scores. You build it with `powershell -ExecutionPolicy Bypass -File scripts/build.ps1` (this guard aborts in ~1s if a copy of the app is already running and holding a lock on the exe; pass `-Kill` to stop it first). Tests run with `GAKUMAS_NO_MANIFEST=1 cargo test` (the admin manifest otherwise makes the test harness demand elevation; see `CLAUDE.md` "Testing limitations").

The pieces you will touch and how they fit together:

- `src/automation/ocr_worker.rs` — A dedicated worker thread. Its `run_ocr_worker(receiver, csv_path, …)` loops receiving `OcrWorkItem`s (one per captured screenshot), runs OCR, computes the nine scores as a `[[u32; 3]; 3]` array (indexed `[stage][slot]`), computes a recovery outcome, then appends a row to `results.csv` via `append_to_csv`. The recovery outcome is the worst of the three stages and becomes the string `"ok"`, `"repaired"`, or `"flagged"` (variable `recovery_str` around `ocr_worker.rs:84`). **This is where new score data first becomes available in memory** — the natural place to also push it to a live store.

- `src/automation/runner.rs` — Owns the run lifecycle and a set of **process-global** state holders used to communicate from the automation/worker threads to the GUI thread: `static CURRENT_ITERATION: AtomicU32`, `static TOTAL_ITERATIONS: AtomicU32`, `static CURRENT_STATE_DESC: Mutex<String>`, etc. (around `runner.rs:22–54`), each with a `pub fn get_…()` accessor. Every run variant funnels through `start_automation_inner(iterations, start_iteration, existing_session)` (around `runner.rs:189`), which resets these globals before spawning the automation thread. **We add a new global live-score buffer here, following this exact pattern.**

- `src/automation/results_edit.rs` — Reads/writes the results CSV with recovery awareness. `pub fn load_review_rows(session_dir: &Path) -> Result<Vec<ReviewRow>>` (around `results_edit.rs:60`) returns one `ReviewRow` per data row, where `ReviewRow` has `scores: [[u32; 3]; 3]` and `recovery: String`. **We use this to seed the live buffer when resuming/extending an existing session**, so the live figure reflects the whole series, not just newly-added points.

- `src/analysis/statistics.rs` — `pub struct ColumnStats` (fields include `count`, `mean: f64`, `median: f64`, `min: u32`, `max: u32`, `quartile_1: f64`, `quartile_3: f64`, plus `mode`, `std_dev`, `stage`, `criterion`) and `pub struct DataSetStats { pub total_runs: usize, pub columns: Vec<ColumnStats> }`. Today `DataSetStats::from_dataset(&DataSet)` builds the nine columns by calling private `calculate_column_stats(values, stage, criterion)`. **We add a sibling constructor that builds the same stats from raw score rows**, so the live buffer can be turned into stats without faking a CSV.

- `src/analysis/charts.rs` — `pub fn generate_combined_box_plot(stats: &DataSetStats, output_path: &Path, _config: &ChartConfig)` (around `charts.rs:465`) draws the nine-box figure to a PNG **file** using `plotters`' `BitMapBackend::new(output_path, (1200, 700))`. It computes a global Y range across all columns, draws an axis, manually draws the nine `S1C1…S3C3` x-labels in a lower strip, then for each column draws the Q1–Q3 box, median line, whiskers, and min/max caps with per-stage colors. **We add an in-memory sibling that draws the same boxes plus a six-line statistics block under each, into a pixel buffer instead of a file.**

- `src/gui/mod.rs` — The eframe app. `fn update(&mut self, ctx, _frame)` (around `mod.rs:794`) runs every frame; it calls `self.update_automation_status()` (which reads the runner globals) and, while running, `ctx.request_repaint_after(100ms)`. Texture creation uses the pattern at `mod.rs:618–626`: `egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba_bytes)` → `ctx.load_texture(name, color, TextureOptions::LINEAR)` → store the returned `TextureHandle`. **We create the live figure texture here, in `update`, because `load_texture` needs the `egui::Context`.**

- `src/gui/render.rs` — Pure rendering of the state-driven third column. `render_control_panel(ui, state) -> PanelActions` matches on `state.status`; the `AutomationStatus::Running { current, total, .. }` arm calls `render_running(ui, state, current, total, actions)` (around `render.rs:204`). Controls follow the convention "emit a button/checkbox → set a field on the returned `PanelActions` struct → `mod.rs::update` dispatches it." **We add the show/hide toggle as a `PanelActions` field and display the live texture (held on `GuiState`) inside `render_running`.**

- `src/gui/state.rs` — `GuiState`. Holds UI state including textures (it already stores `TextureHandle`s). `Debug` is hand-implemented because `TextureHandle` is not `Debug`. **We add the live-chart toggle bool, the cached texture, and a "last rendered iteration" marker here.**

Key invariant to respect (from `CLAUDE.md`): egui render functions must not mutate sibling `GuiState` fields while matching on `state.status` or iterating a borrowed list — clone the status first. The render code already does this; keep it.


## Plan of Work

The work is three milestones, each independently verifiable.

### Milestone 1 — Live score buffer (data plumbing)

Goal: every completed, non-discarded iteration's nine scores (tagged flagged-or-not) land in a process-global buffer that the GUI thread can read, the buffer is emptied at the start of a fresh run, and on resume/extend it is pre-filled from the existing CSV. At the end of M1 there is no UI yet — you prove it with a unit test and a temporary log line.

Edits:

1. In `src/automation/runner.rs`, near the other globals (after `static TOTAL_ITERATIONS`), add a live-score store and its API. Define a small public row type and a `Mutex<Vec<…>>`:

       /// One row of live OCR scores for the in-progress run's distribution view.
       /// `flagged` is true when overlap-recovery could not confidently reconstruct
       /// the row (recovery == "flagged"); such rows are kept here but excluded from
       /// the live statistics until verified.
       #[derive(Clone, Copy, Debug)]
       pub struct LiveScoreRow {
           pub scores: [[u32; 3]; 3],
           pub flagged: bool,
       }

       static LIVE_SCORES: Mutex<Vec<LiveScoreRow>> = Mutex::new(Vec::new());

   Add functions:

   - `pub fn record_live_score(scores: [[u32; 3]; 3], flagged: bool)` — locks `LIVE_SCORES` and pushes a `LiveScoreRow`.
   - `pub fn get_live_scores() -> Vec<LiveScoreRow>` — returns a clone of the current buffer (cheap; at most a few thousand 76-byte rows).
   - `pub fn live_score_count() -> usize` — convenience for the GUI's change-detection (length of the buffer); used together with the iteration counter.
   - `fn clear_live_scores()` — empties the buffer.
   - `fn seed_live_scores_from_csv(session_dir: &Path)` — calls `crate::automation::results_edit::load_review_rows(session_dir)`; on `Ok(rows)`, pushes one `LiveScoreRow { scores: r.scores, flagged: r.recovery == "flagged" }` per row; on `Err`, logs and leaves the buffer empty (a missing/short CSV must not abort the run).

2. In `src/automation/runner.rs::start_automation_inner`, after `clear_last_outcome();` and after the session directory is known, reset/seed the buffer: call `clear_live_scores();` always, then — only when `is_resume` is true (i.e. `existing_session.is_some()`, which covers both resume and extend, since both pass `Some(session_dir)`) — call `seed_live_scores_from_csv(&session_dir);`. Place the seed call after `init_csv(&csv_path)` so the file is guaranteed to exist.

3. In `src/automation/ocr_worker.rs::run_ocr_worker`, immediately after `recovery_str` is computed (around line 88, before/after the `append_to_csv` call is fine — do it after a successful score readout regardless of CSV write success, so the live view does not depend on disk), push to the buffer:

       crate::automation::runner::record_live_score(scores, matches!(recovery, Recovery::Flagged));

   `scores` is already `[[u32; 3]; 3]` and `recovery` is the `Recovery` enum value computed just above. Excluding flagged at *stats* time (M2), not here, so we still push flagged rows with `flagged: true`.

4. Add a unit test (gated to run unelevated). In `src/automation/runner.rs` under a `#[cfg(test)] mod tests`, add a test that clears the buffer, records three rows (two non-flagged, one flagged), and asserts `get_live_scores()` returns three rows with the expected `flagged` pattern. Because `LIVE_SCORES` is process-global, isolate the test by calling `clear_live_scores()` at the top and keeping it the only test that touches the buffer, or guard with a mutex; note this in the test comment.

M1 acceptance: `GAKUMAS_NO_MANIFEST=1 cargo test live_score` passes (the new test), and a temporary `crate::log` you optionally add in `record_live_score` shows rows accumulating during a real run.

### Milestone 2 — In-memory nine-box figure with statistics block

Goal: a function that takes a `DataSetStats` and returns the pixels of a finished figure (nine boxes in a row, six statistics under each), with no file I/O, plus a way to build `DataSetStats` from the live buffer.

Edits:

1. In `src/analysis/statistics.rs`, add a constructor that builds stats from raw rows, reusing the existing private `calculate_column_stats`:

       impl DataSetStats {
           /// Build the nine-column statistics from raw score rows (`[stage][slot]`),
           /// e.g. the live in-run buffer. Mirrors `from_dataset` but takes owned rows
           /// instead of a `DataSet`/CSV.
           pub fn from_score_rows(rows: &[[[u32; 3]; 3]]) -> Self {
               let mut columns = Vec::with_capacity(9);
               for stage in 0..3 {
                   for criterion in 0..3 {
                       let values: Vec<u32> =
                           rows.iter().map(|r| r[stage][criterion]).collect();
                       columns.push(calculate_column_stats(&values, stage + 1, criterion + 1));
                   }
               }
               DataSetStats { total_runs: rows.len(), columns }
           }
       }

   Note `calculate_column_stats` already returns a zeroed `ColumnStats` for an empty column, so an early-run figure with zero usable rows renders flat boxes at 0 rather than panicking.

2. In `src/analysis/charts.rs`, add an in-memory renderer. Factor the existing per-column box geometry out of `generate_combined_box_plot` into a private helper if convenient, but at minimum add:

       /// Render the combined nine-box distribution plus a six-line statistics block
       /// (Mean, Median, Max, Min, Q1, Q3) under each box, into an RGBA8 pixel buffer.
       /// Returns (width, height, rgba_bytes) with `rgba_bytes.len() == width*height*4`,
       /// ready for `egui::ColorImage::from_rgba_unmultiplied`. No file is written.
       pub fn render_live_box_plot_rgba(
           stats: &DataSetStats,
           excluded_flagged: usize,
       ) -> Result<(u32, u32, Vec<u8>)>;

   Implementation notes:
   - Choose a canvas size of `(1200, 980)`: reuse the existing `(1200, 700)` box-area proportions for the upper region (split vertically near y=650 for x-labels as the current code does), then reserve the bottom ~300px for the six-line statistics blocks.
   - plotters writes RGB (3 bytes/pixel) into a caller-supplied buffer via `let mut buf = vec![0u8; (W*H*3) as usize]; { let root = BitMapBackend::with_buffer(&mut buf, (W, H)).into_drawing_area(); … draw … root.present()?; }`. After dropping the backend, convert RGB→RGBA by expanding each pixel to 4 bytes with alpha 255. (egui wants RGBA.)
   - Reuse the exact box/whisker/median/cap drawing loop and per-stage colors from `generate_combined_box_plot` (red/green/blue per stage). Keep the same `S1C1…S3C3` x-labels.
   - Under each of the nine column centers, draw six left-aligned text lines using `draw_text` on the lower drawing area (the current code already draws manual labels there with `("sans-serif", 16)`), using a smaller font (~11–12px) so six numbers fit in a ~130px-wide column. Format the six values, rounding the floats to whole scores, with thousands separators, as: `Avg`, `Med`, `Max`, `Min`, `Q1`, `Q3` (these are Mean, Median, Max, Min, quartile_1, quartile_3 from `ColumnStats`). Provide a tiny local `fn group(n: u64) -> String` that inserts commas (no external crate).
   - Put `excluded_flagged` into the title, e.g. `format!("Score Distribution ({} runs, {} flagged excluded)", stats.total_runs, excluded_flagged)`.

3. Add a `#[test]` (or `#[ignore]`d, your choice) in `charts.rs` that builds a small `DataSetStats::from_score_rows(&[…])`, calls `render_live_box_plot_rgba`, and asserts the returned buffer length equals `w*h*4` and `w,h` match the chosen canvas. Optionally, behind `#[ignore]`, also write the buffer to `temp/live_box_plot_preview.png` (re-encode via the `image` crate) for a human to eyeball.

M2 acceptance: `GAKUMAS_NO_MANIFEST=1 cargo test render_live_box_plot` passes; running the optional `#[ignore]`d test produces a viewable PNG with nine boxes and readable six-line stats blocks.

### Milestone 3 — GUI wiring (toggle, regenerate, display)

Goal: a checkbox in the running panel turns the live figure on/off; while on, the figure regenerates whenever a new iteration's data has arrived and displays inline.

Edits:

1. In `src/gui/state.rs::GuiState`, add fields:
   - `pub show_live_chart: bool` (default `false`).
   - `pub live_chart_tex: Option<egui::TextureHandle>` (the cached figure; not `Debug` — extend the hand-written `Debug` impl to print e.g. `live_chart_tex.is_some()`).
   - `pub live_chart_rendered_count: usize` (the live-row count the cached texture was built from; used to detect "new data").

2. In `src/gui/render.rs` (revised — see Decision Log 2026-07-01): the show/hide control is a **pre-run preference checkbox in `render_idle`**, NOT a toggle in `render_running`. The running panel only displays the figure.
   - In `render_idle` (which takes `state: &mut GuiState`), after `render_count_input` and before the Start button, add `ui.checkbox(&mut state.show_live_chart, "ライブ分布を表示")` (with a hover hint). It mutates the preference directly — no `PanelActions` field is needed.
   - Thread the cached texture into the panel: `render_control_panel(ui, state, live_chart: Option<&TextureHandle>)` passes it to `render_running(ui, state, current, total, live_chart, actions)`.
   - In `render_running`, when `state.show_live_chart` is true, display the figure scaled to the panel width preserving aspect: `let w = ui.available_width(); let aspect = tex.size()[1] as f32 / tex.size()[0] as f32; ui.image((tex.id(), egui::Vec2::new(w, w * aspect)));` (mirrors the existing `ui.image((tex.id(), Vec2::new(width, height)))` usage at `render.rs:76`). If the texture is not yet built, show a small "分布を準備中…" label.
   - Rationale for not putting the checkbox in `render_running`: that panel only appears after Start, when the automation owns the mouse and the user is warned not to move it, so the toggle would be unreachable.

3. In `src/gui/mod.rs::update`, after `self.update_automation_status();` and before building the central panel, add live-chart maintenance:
   - Dispatch the toggle: if the last computed `PanelActions.toggle_live_chart` is true, flip `self.state.show_live_chart`. (Follow however `PanelActions` is currently dispatched — the control panel returns `PanelActions` and `update` calls `handle_*`; add the flip alongside the other dispatch handling. When turning the chart *off*, you may drop `self.state.live_chart_tex = None;` to free the texture.)
   - When `self.state.status.is_running()` and `self.state.show_live_chart`, check whether new data has arrived: `let count = crate::automation::runner::live_score_count();` and proceed only if `count != self.state.live_chart_rendered_count`. If so: read `let rows = crate::automation::runner::get_live_scores();`, split into included vs flagged: `let included: Vec<[[u32;3];3]> = rows.iter().filter(|r| !r.flagged).map(|r| r.scores).collect(); let excluded = rows.len() - included.len();`, build `let stats = crate::analysis::statistics::DataSetStats::from_score_rows(&included);`, render `let (w, h, rgba) = crate::analysis::charts::render_live_box_plot_rgba(&stats, excluded)?;` (handle the `Result` — on error, log and skip, do not crash the GUI), then upload the texture exactly like the review-preview pattern at `mod.rs:618–626`:

         let color = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
         let tex = ctx.load_texture("live_box_plot", color, egui::TextureOptions::LINEAR);
         self.state.live_chart_tex = Some(tex);
         self.state.live_chart_rendered_count = count;

     Because `update` already requests a repaint every 100ms while running, the figure refreshes promptly after each new iteration without extra timers. Regeneration only happens when `count` changes, so idle frames cost nothing.
   - Optional cleanup: when a run finishes (status transitions to a finished state), you may leave the last live texture visible or clear it; simplest is to clear `live_chart_tex`/reset `live_chart_rendered_count` when `show_live_chart` is toggled off or a new run starts.

M3 acceptance: see Validation and Acceptance.


## Concrete Steps

All commands run from the repository root `C:\Work\GitRepos\gakumas-rehearsal-automation` in PowerShell.

Before building, ensure no app instance is running (it locks the exe):

    Get-Process gakumas-rehearsal-automation -ErrorAction SilentlyContinue

Build (guarded; aborts fast if the app is running, `-Kill` stops it):

    powershell -ExecutionPolicy Bypass -File scripts/build.ps1 -Kill

Run the unit tests added in M1/M2 (unelevated, manifest skipped):

    $env:GAKUMAS_NO_MANIFEST=1; cargo test live_score
    $env:GAKUMAS_NO_MANIFEST=1; cargo test from_score_rows
    $env:GAKUMAS_NO_MANIFEST=1; cargo test render_live_box_plot

Expected (illustrative): each prints `test result: ok. 1 passed; 0 failed`.

Optionally produce the eyeball preview PNG (M2 `#[ignore]`d test), then open it:

    $env:GAKUMAS_NO_MANIFEST=1; cargo test live_box_plot_preview -- --ignored
    start temp/live_box_plot_preview.png

Find real compile errors amid the ~30 expected warnings:

    cargo check 2>&1 | Select-String "^error"

Run the app for manual acceptance:

    .\target\release\gakumas-rehearsal-automation.exe


## Validation and Acceptance

Automated (must pass):

- `GAKUMAS_NO_MANIFEST=1 cargo test live_score` — the M1 buffer test: records two non-flagged + one flagged row, asserts `get_live_scores()` returns three rows with `flagged` = `[false, false, true]` (or your chosen order). This test fails before M1 (function doesn't exist) and passes after.
- `GAKUMAS_NO_MANIFEST=1 cargo test from_score_rows` — asserts `DataSetStats::from_score_rows(&rows)` yields nine columns and `total_runs == rows.len()`, with a hand-checked median/Q1/Q3 on a tiny fixed input.
- `GAKUMAS_NO_MANIFEST=1 cargo test render_live_box_plot` — asserts the returned buffer length is `w*h*4` and dimensions match the chosen canvas.

Manual (the real proof, behavior a human verifies):

1. Launch the game `gakumas.exe` to the rehearsal screen and start the app elevated.
2. In the GUI third column (idle panel), tick "ライブ分布を表示", then start a run of, say, 20 iterations.
3. While it runs, observe: within ~one iteration, a figure of nine box plots appears under the progress bar, each with six numbers (Avg/Med/Max/Min/Q1/Q3) printed beneath. As iterations complete, the boxes and numbers visibly update; the title shows the running count. No mouse interaction is needed during the run.
4. Confirm flagged exclusion: if an iteration is flagged (watch `session.log` for `iteration N FLAGGED for review`), the figure's run-count in the title increments by less than the iteration counter, and the boxes do **not** lurch on that iteration. The title shows `… N flagged excluded`.
5. Run a second series with the checkbox left unticked: no figure appears in the running panel.
6. Let the run finish, then open the session's `charts/chart_combined.png`. The live figure's boxes should broadly match the final combined box plot (allowing for the deliberate difference that the live one excluded flagged rows).
7. Resume/extend acceptance: interrupt a run (Ctrl+Shift+Q), then resume or "追加実行" into the same folder; with the toggle on, the live figure should immediately reflect the **already-completed** rows (seeded from CSV), not start from an empty buffer.

If the figure never appears, check `cargo check 2>&1 | Select-String "^error"` first, then verify `record_live_score` is actually called (temporary log) and that `update` is regenerating (the `count != live_chart_rendered_count` guard).


## Idempotence and Recovery

All edits are additive and safe to apply incrementally; each milestone leaves a buildable tree. The live buffer is in-memory only and is reset at the start of every fresh run and re-seeded from CSV on resume, so repeated runs never accumulate stale data. Re-running the tests is non-destructive. The optional preview PNG writes under `temp/` (already git-ignored per the repo's `temp/` convention) and can be deleted freely. No CSV/session formats change, so the feature is fully backward-compatible with existing sessions and with the post-run analysis path.

If M3 misbehaves in the GUI, you can disable the feature entirely by leaving `show_live_chart` defaulting to `false` and not exposing the checkbox — the data plumbing (M1/M2) is inert until the GUI reads it.


## Artifacts and Notes

Expected shape of the live data type and key signatures introduced (recap):

    // src/automation/runner.rs
    pub struct LiveScoreRow { pub scores: [[u32; 3]; 3], pub flagged: bool }
    pub fn record_live_score(scores: [[u32; 3]; 3], flagged: bool);
    pub fn get_live_scores() -> Vec<LiveScoreRow>;
    pub fn live_score_count() -> usize;

    // src/analysis/statistics.rs
    impl DataSetStats { pub fn from_score_rows(rows: &[[[u32; 3]; 3]]) -> Self }

    // src/analysis/charts.rs
    pub fn render_live_box_plot_rgba(stats: &DataSetStats, excluded_flagged: usize)
        -> Result<(u32, u32, Vec<u8>)>;

The OCR worker push site (around `ocr_worker.rs:88`), once `recovery_str`/`recovery` are computed:

    let recovery = worst_recovery(&readout.flags);
    // … existing recovery_str / logging …
    crate::automation::runner::record_live_score(scores, matches!(recovery, Recovery::Flagged));


## Interfaces and Dependencies

No new crates. Uses existing `plotters = "0.3"` (the `BitMapBackend::with_buffer` constructor renders to a `&mut [u8]` RGB buffer in memory) and `eframe = "0.29"` / `egui` (`ColorImage::from_rgba_unmultiplied`, `Context::load_texture`, `ui.image`). Reuses `image = "0.25"` only for the optional preview-PNG test.

Module dependencies to add: `src/automation/ocr_worker.rs` and `src/automation/runner.rs` call into each other within the `crate::automation` namespace (already siblings; `ocr_worker` already references `crate::automation::csv_writer`). `src/gui/mod.rs` calls `crate::automation::runner::{get_live_scores, live_score_count}`, `crate::analysis::statistics::DataSetStats::from_score_rows`, and `crate::analysis::charts::render_live_box_plot_rgba`. `runner.rs::seed_live_scores_from_csv` calls `crate::automation::results_edit::load_review_rows`.

Concurrency: `LIVE_SCORES` is a `std::sync::Mutex<Vec<LiveScoreRow>>` written by the OCR worker thread and read by the GUI thread, mirroring the existing `CURRENT_STATE_DESC: Mutex<String>` pattern. Hold the lock only to push or to clone-out; never across a render. The GUI clones the buffer (`get_live_scores`) before computing stats so it never holds the lock while rendering.
