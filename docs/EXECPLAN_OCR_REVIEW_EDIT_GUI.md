# Trust total-confirmed scores over a noisy bonus, and add a GUI to review/edit OCR results

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

This repository contains `docs/PLANS.md` (relative to the repository root). This document must be maintained in accordance with `docs/PLANS.md`.

This plan builds on two prior, checked-in plans: `docs/EXECPLAN_OVERLAP_SCORE_RECOVERY.md` (the checksum recovery) and `docs/EXECPLAN_OCR_TOTAL_BONUS_ROBUSTNESS.md` (the total/bonus hardening). Read their `Purpose` and `Key facts/invariants` sections first; facts repeated here are deliberate (PLANS.md requires self-containment).


## Purpose / Big Picture

The tool clicks through many *gakumas* "rehearsal" runs and, on each result screen, OCRs nine per-character scores into `results.csv`, using the screen's own checksum (`stage_total = c1 + c2 + c3 + floor(max/5)`, the bonus badge = `floor(max/5)`) to repair OCR corruption and to mark a row `recovery=flagged` when it cannot verify. This plan delivers two improvements found in the field run `target/release/output/20260624_214602/` (a multi-character run: 132 ok, 49 repaired, 19 flagged):

- **A — stop false-flagging correct scores when only the bonus mis-read.** In several flagged stages the per-character scores are correct and the **total confirms them exactly** (the checksum matches at zero edits), yet the row is flagged solely because the *bonus* badge over-detected a digit (e.g. `+74,413` OCR'd as `744135`). The bonus is only a cross-check; when the total already confirms an unedited read, a disagreeing bonus must not override it to `flagged`. After A, such stages read `ok`.

- **D — a GUI to review and correct OCR results, like the sister web app `gakumas-tools`.** Some failures are genuine OCR digit-loss the checksum cannot recover (e.g. iter 126: the score row `1,032,249 1,048,189` lost digits, read as `1032`/`48189`; correctly flagged but unrecoverable). Today such a row is "silent": the run completes, nothing alerts the user, and the flagged row still holds plausible-looking garbage. `gakumas-tools`' rehearsal page solves this with an **editable results table plus a per-row image button** that opens the source screenshot so the user can read the true value and fix it. This plan ports that experience to this app's egui GUI: a review window listing each stage's scores (defaulting to the rows that need attention — `flagged`/`repaired` — with a toggle to show all), with **editable score cells**, an **in-app preview pane** that renders the row's screenshot when its 📷 button is clicked, and a **save** that rewrites `results.csv` and `rehearsal_data.csv` and marks each edited row `recovery=manual` so corrections are auditable.

You can see A working via new unit tests (each fails before, passes after) and by re-OCR'ing the `20260624_214602` screenshots and watching the flag count drop. You can see D by building the GUI, opening a finished session's review window, clicking a flagged row's 📷 to see its screenshot, typing the correct value, and saving — then confirming `results.csv` shows the corrected values with `recovery=manual`.


## Key facts this plan relies on

1. The checksum is `stage_total = c1 + c2 + c3 + floor(max(c1,c2,c3) / 5)`; the bonus equals `floor(max/5)` and is an independent cross-check only (never a required input). A wrong bonus must, at worst, be ignored — it must never *force* a flag on a read the total already confirms.
2. `reconcile_stage` (in `src/ocr/reconcile.rs`) returns the chosen scores and a `Recovery` (`Ok`/`Repaired`/`Flagged`). It computes a cost-0 solution exactly when the raw scores already satisfy the checksum with no edits (`chosen == ocr_scores`). The existing code then flags that cost-0 solution if `bonus_disagrees` — this is the A bug.
3. `results.csv` columns are `iteration,timestamp,screenshot,s1c1,s1c2,s1c3,s2c1,s2c2,s2c3,s3c1,s3c2,s3c3,recovery` (13 data columns; `recovery` is the 13th). `rehearsal_data.csv` is headerless, one line per iteration in order, holding only the nine scores `s1c1..s3c3`. `analysis::csv_reader` indexes the first 12 columns by position, so adding a new `recovery` value like `manual` is non-breaking.
4. The GUI (`src/gui/`) is a single eframe `CentralPanel` with a 3-column layout (two guide images + a state-driven control panel). Textures are created with `ctx.load_texture` from an `image`-decoded RGBA buffer (see `GuiApp::load_images`). `GuiState` (`src/gui/state.rs`) holds all UI state; `render::render_control_panel` returns a `PanelActions` struct that `mod.rs::update` dispatches to `handle_*` methods. New UI follows this emit-action→dispatch pattern.


## Progress

- [x] (2026-06-24) M1 (A) — In `src/ocr/reconcile.rs::reconcile_stage`, a cost-0, unedited, total-confirmed read (`chosen == ocr_scores && min_cost == 0 && !tie`) is now classified `Ok` *before* the `bonus_disagrees` check, so a noisy bonus no longer flags it; the bonus still guards edited (cost > 0) repairs. Tests added (`test_total_confirmed_over_detected_bonus_is_ok`, `test_edited_repair_still_bonus_guarded`). Full suite 98 passed, e2e overlap fixtures unchanged.
- [x] (2026-06-24) M2 (D-data) — A CSV review model in a new `src/automation/results_edit.rs` (or `analysis/`): `load_review_rows(session_dir) -> Vec<ReviewRow>` parsing `results.csv` (iteration, screenshot path, nine score strings, recovery), and `save_review_rows(session_dir, &[ReviewRow])` that rewrites `results.csv` (preserving header + timestamps) and patches `rehearsal_data.csv` line-for-line, setting `recovery=manual` on rows whose scores changed. Pure/file-level, unit-tested with a temp dir.
- [x] (2026-06-24) M3 (D-state) — `ReviewState` added to `src/gui/state.rs` (rows, edit buffers, `show_all`, `dirty`, `preview:(u32,TextureHandle)`, `open`; manual `Debug`) and `GuiState.review: Option<ReviewState>`. `PanelActions.open_review` + a `📝 結果を確認・修正` button in `render_finished` and the idle 前回の結果 block. `handle_open_review` loads rows for `latest_session_path` and seeds edit buffers.
- [x] (2026-06-24) M4 (D-ui+preview) — `render_review_window_contents` in `render.rs` (2-column layout: editable `Grid` of filtered rows — flagged/repaired by default, すべて表示 checkbox — with 9 `TextEdit` cells, a colored recovery badge, and a 📷 per row; right pane previews the screenshot). `mod.rs::render_review_window` shows the `egui::Window`, collects `ReviewActions`, and `load_review_preview` decodes + caches the texture on demand.
- [x] (2026-06-24) M5 (D-save) — `handle_save_review` parses the edit buffers, marks changed rows `manual`, calls `save_review_rows` (rewrites both CSVs), clears `dirty`, and re-seeds buffers. 保存 button enabled only when dirty. **Code-complete; builds clean (0 errors), 102 unit tests pass. Manual GUI acceptance pending** (the tray app can't be `cargo test`-driven). Update Outcomes after the user verifies.

Use timestamps when you check items off, e.g. `- [x] (2026-06-24 14:00Z) ...`.


## Surprises & Discoveries

- Observation (field run 20260624_214602, multi-character): of 19 flagged iterations, several are correct scores the **total confirms at zero edits** but a noisy **bonus** over-detected a digit, forcing the flag. Example: stage `[365181,372069,357515]`, total `1,169,178` matches exactly (`365181+372069+357515+floor(372069/5)=1169178`), but bonus read `744135` (true `74413`), so `bonus_disagrees` flagged a correct read.
  Evidence: per-stage `session.log` lines; the diff-from-total of these rows is 0.
- Observation: iter 126 is a genuine score-row digit loss (`1,032,249`→`1032`, `1,048,189`→`48189`) the checksum cannot recover (5 digits never captured), even though total/bonus are perfect — the motivating case for the editable-GUI (D). Algebraically the missing middle score is `total − c1 − c3 − bonus = 1,032,249`, but recovering a value OCR never saw is exactly where a "silent wrong" could creep in, so the design keeps it flagged and lets the user fix it by hand against the screenshot.
- (Extend as M1–M5 land.)


## Decision Log

- Decision (A): when the total confirms the raw scores at zero cost, return `Ok` regardless of the bonus; only use bonus disagreement to flag *edited* (cost > 0) reconstructions.
  Rationale: invariant 1 — the bonus is a cross-check, not an authority. A million lost from a non-max slot (the case the bonus was meant to catch) makes the total *not* match at cost 0, so it is already handled by the no-solution / structural-only paths; a cost-0 total match means the scores are right and the bonus is the unreliable party (it over-detects digits exactly like the total's comma does).
  Date/Author: 2026-06-24, after diagnosing run 20260624_214602.

- Decision (D): port the `gakumas-tools` review experience — an editable results table with a per-row image button — into this app's egui GUI as a dedicated review **window**, with the screenshot rendered **in-app** (not the OS viewer), defaulting the table to `flagged`/`repaired` rows with a show-all toggle, and persisting edits by rewriting the CSVs with a `recovery=manual` marker.
  Rationale: user direction — they specifically value the gakumas-tools GUI (inline edit + click-to-see-the-image). In-app preview keeps the verify→fix loop in one window. Defaulting to attention-needed rows keeps the table small on a 200-row run. The `manual` marker keeps a hand-edit auditable and distinct from `ok`/`repaired`/`flagged`, and is non-breaking for `analysis::csv_reader` (it indexes the first 12 columns).
  Date/Author: 2026-06-24, user direction.

- Decision (D): rewrite `results.csv` in full on save (preserving header, timestamps, and untouched rows) and patch `rehearsal_data.csv` line-for-line, rather than appending.
  Rationale: editing is the feature; an in-place rewrite is the only way to correct an existing row. The write is to the session folder the user is reviewing; the prior plans' "append-only for crash safety" applies to live capture, not to a deliberate post-run correction. Both files are rewritten together so they never diverge.
  Date/Author: 2026-06-24.


## Outcomes & Retrospective

(To be written as milestones land. Compare against Purpose: A drops the false-flag count on the 20260624_214602 screenshots; D lets a user open a finished session, see a flagged row's screenshot in-app, type the true value, and save it back to both CSVs marked `manual`.)


## Context and Orientation

Rust Windows tray/GUI app. Relevant files, by full repository-relative path:

- `src/ocr/reconcile.rs` — `reconcile_stage(ocr_scores:[u32;3], total:Option<u32>, bonus:Option<u32>) -> ([u32;3], Recovery)`. After building checksum-satisfying solutions it picks the min-cost combo, does a bonus tie-break/corroboration, then sets `recovery = if tie || bonus_disagrees { Flagged } else if chosen == ocr_scores { Ok } else { Repaired }`. M1 edits this final classification. `Recovery` is `{Ok, Repaired, Flagged}`.
- `src/automation/csv_writer.rs` — `CSV_HEADER`, `init_csv`, `append_to_csv(path, work_item, scores, recovery)`, `append_to_raw_csv(path, scores)`. M2 adds read/rewrite functions (here or in a new module).
- `src/analysis/csv_reader.rs` — reads `results.csv` for charts/stats, indexing the first 12 columns by position (so a new `recovery` value is safe). Read it to mirror its parsing.
- `src/gui/state.rs` — `GuiState` (all UI state) and `AutomationStatus` (Idle/Running/Completed/Aborted/Error, each carrying `session_path`). M3 adds review state here.
- `src/gui/render.rs` — `render_control_panel(ui, &mut GuiState) -> PanelActions`; `PanelActions` is the per-frame click-signal struct. `render_finished`/`render_idle` are where the "open review" button goes. M3/M4 add fields + rendering.
- `src/gui/mod.rs` — `GuiApp` (holds textures + state), `eframe::App::update` (the 3-column `CentralPanel`; dispatches `PanelActions` to `handle_*`), `load_images` (the `ctx.load_texture` pattern to copy for the preview). M3–M5 add `handle_open_review`/`handle_save_review` and render the review window.

Terms: "stage" = one rehearsal row (up to three per-character scores); "slot" = one of its three positions; "recovery flag" = the `ok`/`repaired`/`flagged`/(new)`manual` marker in `results.csv` column 13; "preview pane" = an egui area that draws the row's screenshot via a `TextureHandle`.

Build/test: the admin manifest blocks `cargo test` unless built with `GAKUMAS_NO_MANIFEST=1 cargo test` (gate in `build.rs`). `reconcile.rs` and the CSV model are pure/file-level and unit-testable this way; the egui review window is verified by building (`cargo build --release`) and manual interaction (the tray GUI cannot be driven from `cargo test`).


## Plan of Work

Implement in order; each milestone is independently verifiable. M1 (A) is a small pure-function fix shippable on its own. M2 is the file model behind the GUI. M3–M5 build the review window incrementally (state → table+preview → save).


### Milestone M1 (A) — Total-confirmed scores are not flagged by a noisy bonus

Goal: a multi-character stage whose raw scores satisfy the total checksum at zero edits is `Ok` even when the bonus over-detected a digit.

In `src/ocr/reconcile.rs::reconcile_stage`, the final block currently reads (paraphrased): `let recovery = if tie || bonus_disagrees { Flagged } else if chosen == ocr_scores { Ok } else { Repaired };`. Change it so a cost-0, unedited, total-confirmed read is trusted over the bonus: when `chosen == ocr_scores` **and** `min_cost == 0` **and not** `tie`, return `Ok` regardless of `bonus_disagrees`. Keep `bonus_disagrees` (and `tie`) able to flag only when the solution is an *edit* (`chosen != ocr_scores`, i.e. `min_cost > 0`) — there the bonus genuinely guards an uncertain reconstruction. Concretely:

    let recovery = if chosen == ocr_scores && min_cost == 0 && !tie {
        Recovery::Ok                       // total confirms the raw read; ignore a noisy bonus
    } else if tie || bonus_disagrees {
        Recovery::Flagged
    } else if chosen == ocr_scores {
        Recovery::Ok
    } else {
        Recovery::Repaired
    };

(Equivalently: hoist the cost-0 confirmation above the `bonus_disagrees` check.)

Add unit tests in that file's `tests` module:

- Total-confirmed, bonus over-detected → `Ok`: `reconcile_stage([365181,372069,357515], Some(1169178), Some(744135))` → `([365181,372069,357515], Recovery::Ok)` (true bonus is `74413`; the inflated `744135` must not flag a total-confirmed read).
- Edited reconstruction still bonus-guarded: keep the existing overlap tests (e.g. the 003/005/102842 repaired samples and any bonus-disagree-on-edit case) passing unchanged — a wrong bonus on a *cost>0* repair still flags.

Run `GAKUMAS_NO_MANIFEST=1 cargo test reconcile`; the new test passes and all prior reconcile/e2e tests are unchanged.


### Milestone M2 (D-data) — Load and rewrite the results CSVs

Goal: a file-level model the GUI can load, edit in memory, and save back, keeping `results.csv` and `rehearsal_data.csv` consistent.

Add a module (e.g. `src/automation/results_edit.rs`, declared in `src/automation/mod.rs`) with:

    pub struct ReviewRow {
        pub iteration: u32,
        pub timestamp: String,        // preserved verbatim on rewrite
        pub screenshot: String,       // absolute path string from the CSV
        pub scores: [[u32; 3]; 3],    // s1c1..s3c3
        pub recovery: String,         // "ok"|"repaired"|"flagged"|"manual"
    }

    // Parse results.csv into rows (skip the header; tolerate the 12-column legacy
    // form by defaulting recovery to "" / "ok"). Returns rows in file order.
    pub fn load_review_rows(session_dir: &std::path::Path) -> anyhow::Result<Vec<ReviewRow>>;

    // Rewrite results.csv (header + every row, edited or not) and patch
    // rehearsal_data.csv line-for-line from the same rows (line N = iteration N's
    // nine scores). Caller has already set recovery="manual" on changed rows.
    pub fn save_review_rows(session_dir: &std::path::Path, rows: &[ReviewRow]) -> anyhow::Result<()>;

`save_review_rows` writes `results.csv` with the exact `CSV_HEADER` from `csv_writer`, each row formatted identically to `append_to_csv` (iteration, timestamp, screenshot, nine scores, recovery). It then writes `rehearsal_data.csv` as one headerless line per row (nine scores, matching `append_to_raw_csv`'s format), in iteration order. Write to a temp file and rename, so a crash mid-write cannot truncate the originals.

Unit tests (temp dir): round-trip a small `results.csv` (load → save → load equal); editing one row's score and saving updates both files and that row's line in `rehearsal_data.csv`; a 12-column legacy file loads (recovery defaults) and saves in the 13-column form. Run `GAKUMAS_NO_MANIFEST=1 cargo test results_edit`.


### Milestone M3 (D-state) — Review state and the open button

Goal: the GUI can open a review session for the latest finished run.

In `src/gui/state.rs` add:

    pub struct ReviewState {
        pub session_path: std::path::PathBuf,
        pub rows: Vec<crate::automation::results_edit::ReviewRow>,
        pub edits: Vec<[[String; 3]; 3]>,   // per-row editable text buffers, parallel to rows
        pub show_all: bool,                  // false = only flagged/repaired
        pub dirty: bool,
        pub preview: Option<(u32, eframe::egui::TextureHandle)>, // (iteration, texture)
        pub open: bool,
    }

and a field `pub review: Option<ReviewState>` on `GuiState` (default `None`). In `src/gui/render.rs` add `pub open_review: bool` to `PanelActions` and a button — `📝 結果を確認・修正` — in `render_finished` (and in `render_idle`'s "前回の結果" block) that sets it. In `src/gui/mod.rs` add `handle_open_review`: load rows for `self.state.latest_session_path` via `load_review_rows`, seed `edits` from `rows`, set `open = true`; dispatch it in `update`.

Acceptance: clicking the button populates `state.review` (verify by a log line) without yet rendering the window.


### Milestone M4 (D-ui+preview) — The review window and in-app screenshot preview

Goal: an interactive window to inspect/edit scores with the screenshot visible in-app.

In `mod.rs::update`, after the central panel, when `self.state.review.is_some() && open`, show an `egui::Window::new("結果の確認・修正")` (resizable, collapsible, with a close affordance that sets `open = false`). Layout: left = the table, right = the preview pane.

Table: a header row, then one row per `ReviewRow` filtered by `show_all` (default shows only `recovery ∈ {flagged, repaired}`; a checkbox toggles all). Each row shows the iteration, nine `egui::TextEdit::singleline` cells bound to `edits[i]` (mark `dirty` and the row changed when a buffer differs from the stored score), a colored recovery badge, and a `📷` button. Use a `ScrollArea` and a stable `id_source` per cell.

Preview pane: when a row's `📷` is clicked, load its screenshot into a texture (decode with `image::open(screenshot_path)` → RGBA → `ctx.load_texture`, mirroring `GuiApp::load_images`) and store `preview = Some((iteration, tex))`; draw it with `ui.image` scaled to the pane width (aspect-preserved, like `render_guide_image`). Cache until another 📷 is clicked. If the file is missing, show a placeholder/label.

Because the screenshots are portrait phone captures, the value the user needs is a small region; rendering the whole image scaled-to-width is enough to read the scores (the user can resize the window). (Optional, note-only: a future enhancement could crop to the relevant stage row using the same `score_regions` fractions; not required here.)

Acceptance (manual, build + run): open a finished session, the table lists the flagged/repaired rows, clicking 📷 shows that screenshot in the pane, and typing in a cell marks the window dirty.


### Milestone M5 (D-save) — Persist edits

Goal: edits are written back to both CSVs and marked `manual`.

Add a 保存 (Save) button (enabled when `dirty`). On click, in `handle_save_review`: for each row, parse its `edits` buffers to `[[u32;3];3]` (reject/ignore non-numeric, keeping the prior value), and where the parsed scores differ from `row.scores`, update `row.scores` and set `row.recovery = "manual"`. Call `save_review_rows(session_path, &rows)`, clear `dirty`, log a summary, and re-seed `edits` from the saved rows. Offer a cancel/close that discards unsaved edits (with the dirty state cleared on reload).

Acceptance (manual): edit a flagged row to the value read from its screenshot, Save, and confirm `results.csv` shows the new scores with `recovery=manual` and `rehearsal_data.csv`'s corresponding line is updated; re-opening the review shows the corrected row no longer needs attention. Then write Outcomes and add a one-line cross-reference in `docs/EXECPLAN_OCR_TOTAL_BONUS_ROBUSTNESS.md`.


## Concrete Steps

From repo root `C:\Work\GitRepos\gakumas-screenshot` (PowerShell; Bash tool available):

    cargo build --release
    GAKUMAS_NO_MANIFEST=1 cargo test reconcile        # M1
    GAKUMAS_NO_MANIFEST=1 cargo test results_edit      # M2
    .\target\release\gakumas-screenshot.exe            # M3-M5 manual (run a short series or open a prior session)

Build emits ~30 expected warnings; only `^error` lines matter.


## Validation and Acceptance

- M1: `GAKUMAS_NO_MANIFEST=1 cargo test reconcile` passes incl. the new total-confirmed-over-bonus test; prior reconcile/e2e tests unchanged. Re-OCR of the `20260624_214602` screenshots reduces the flagged count (the ~5 bonus-only false flags become `ok`).
- M2: `cargo test results_edit` passes (round-trip, edit-one-row, legacy-12-col).
- M3–M5 (manual): in the running GUI, open a finished session's review window; the table defaults to flagged/repaired rows with a show-all toggle; 📷 renders the screenshot in-app; editing a cell and Save rewrites `results.csv` (edited rows `recovery=manual`) and `rehearsal_data.csv`, and the reloaded table reflects the correction.


## Idempotence and Recovery

`cargo build`/`cargo test` are idempotent. M1 is additive to a pure function guarded by the exact checksum (worst case: a stage flags instead of corrupting). M2's save writes via temp-file-then-rename so an interrupted save cannot truncate the originals; loading a legacy 12-column file is tolerated. The review window is non-destructive until 保存 is pressed; closing without saving discards edits. The feature only ever rewrites the session folder the user is actively reviewing.


## Interfaces and Dependencies

No new crates (reuse `image`, `eframe/egui`, `anyhow`, `csv`-free manual parsing as the existing code does). End-state signatures:

In `src/ocr/reconcile.rs` (unchanged signature; internal classification fixed):

    pub fn reconcile_stage(ocr_scores: [u32;3], total: Option<u32>, bonus: Option<u32>) -> ([u32;3], Recovery);

In `src/automation/results_edit.rs`:

    pub struct ReviewRow { pub iteration: u32, pub timestamp: String, pub screenshot: String, pub scores: [[u32;3];3], pub recovery: String }
    pub fn load_review_rows(session_dir: &std::path::Path) -> anyhow::Result<Vec<ReviewRow>>;
    pub fn save_review_rows(session_dir: &std::path::Path, rows: &[ReviewRow]) -> anyhow::Result<()>;

In `src/gui/state.rs`: `ReviewState` and `GuiState.review: Option<ReviewState>` as above.
In `src/gui/render.rs`: `PanelActions.open_review: bool`.
In `src/gui/mod.rs`: `handle_open_review`, `handle_save_review`, and the review-window rendering in `update`.


## Revision Note

2026-06-24: Initial authoring. Captures two field-run findings on `20260624_214602`: (A) correct, total-confirmed multi-character stages were being flagged solely because the bonus badge over-detected a digit — fixed by trusting a cost-0 total-confirmed read over the bonus; and (D) genuine OCR digit-loss (e.g. iter 126) is unrecoverable by checksum and was "silent", motivating a port of `gakumas-tools`' editable results table with a per-row image button into this app's egui GUI (in-app preview, flagged/repaired default with show-all toggle, save rewrites both CSVs with a `recovery=manual` marker). Five milestones: A as a standalone pure fix, then the CSV edit model, review state, the window+preview, and save.
