# Inline per-stage screenshot crops (with character icons) under the editable columns in the OCR review window

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

This repository contains `docs/PLANS.md` (relative to the repository root). This document must be maintained in accordance with `docs/PLANS.md`.

This plan builds directly on the checked-in `docs/EXECPLAN_OCR_REVIEW_EDIT_GUI.md`, which delivered the review window this plan refines. Read that file's `Purpose` and `Context and Orientation` first. Facts repeated here are deliberate — `docs/PLANS.md` requires every plan to be self-contained, so a novice with only this file and the working tree can finish the work.


## Purpose / Big Picture

The app automates many *gakumas* "rehearsal" runs. Each run's result screen is a tall phone screenshot showing three "stages", and each stage shows three character portraits with that character's score printed underneath. The tool OCRs those nine per-character scores into `results.csv` and flags rows it cannot verify against the screen's checksum. The companion review window (built in `docs/EXECPLAN_OCR_REVIEW_EDIT_GUI.md`) lists each run's scores in an editable table, defaulting to the rows that need attention (`flagged`/`repaired`), and lets the user correct a misread by hand.

Today, to *see* the true value, the user clicks a row's 📷 button and the **whole** portrait screenshot renders in a narrow pane on the right of the window. Because the meaningful content (one thin score line) is a tiny fraction of a 721×1281 image scaled into a ~300px pane, the digits are hard to read — the exact problem the review window was meant to remove.

This plan replaces that right-hand whole-image pane with **inline, expand-on-demand, per-stage crops placed directly beneath each stage's three editable cells**, modeled on the sister web app `gakumas-tools` (whose rehearsal page expands the selected row and shows the relevant image inline). Clicking a row's 📷 expands that row; beneath each stage's three score cells appears a cropped image of *that stage* — the three character portraits and their printed scores — so the user reads the ground truth right next to the cell they are editing. The crop is sized **dynamically** to the width of that stage's three columns, so widening or maximizing the window enlarges the crops and makes them more legible (no fixed pixel cap). Clicking 📷 again collapses the row.

After this change a user can: open a finished session's review window, click 📷 on a flagged row, and immediately see — inline, under each stage — the character icons and printed scores at a size that scales with the window, type the correct value into the cell directly above the image, and save. You can see it working by building the app, opening a session with flagged rows, expanding one, and observing the three stage crops appear under their columns and grow when the window is widened.


## Key facts this plan relies on

1. **Crop coordinates are window fractions that map directly to texture UV.** `src/automation/config.rs` defines `RelativeRect { x, y, width, height }` in `0.0..1.0` fractions of the captured client area. The OCR `score_regions: [RelativeRect; 3]` (one per stage) are `{ x: 0.0, y: 0.179|0.430|0.685, width: 1.0, height: 0.022 }` — a full-width, very thin band over just the printed digits. Because a screenshot **is** the full client area, a fraction rect is also a normalized UV rect into that screenshot's texture: UV `min = (rect.x, rect.y)`, `max = (rect.x + rect.width, rect.y + rect.height)`. egui's `egui::Image::uv(Rect)` takes exactly such a normalized rect and draws only that sub-region of a texture — so cropping needs **no** pixel math and **no** extra textures beyond the one already loaded.

2. **The full-screenshot texture is already loaded on demand.** `src/gui/mod.rs::load_review_preview(ctx, iteration)` decodes the row's screenshot with `image::open` → RGBA → `ctx.load_texture` and stores `review.preview = Some((iteration, TextureHandle))`, caching one iteration at a time. This plan reuses that exact texture as the source for the inline crops (we sample sub-rects of it), so the loader changes little.

3. **A texture knows its own native pixel size.** `TextureHandle::size_vec2()` returns the screenshot's pixel dimensions (e.g. `vec2(721.0, 1281.0)`). The crop's native aspect ratio is therefore `(crop.height * tex.size().y) / (crop.width * tex.size().x)` — computed from the real loaded image, never hardcoded — and is used to choose a displayed height once the displayed width (the stage-column width) is known.

4. **The score line alone is too thin to read when narrowed.** One stage's `score_regions` band is `721 × ~28px` native — an aspect of ~25:1. Forcing a 25:1 image into a ~224px-wide three-column group renders it ~9px tall (digits unreadable). Extending the crop **upward** to include the character portraits makes the block much chunkier (~6:1 per stage), so at the same width it has real height and is legible. Including the icon is therefore not decoration; it is what makes "under the column" readable. This plan adds a small, configurable vertical/horizontal adjustment on top of `score_regions` to produce the human-review crop.

5. **The review window is its own OS window.** `src/gui/mod.rs::render_review_window` opens an egui *immediate viewport* (an independent top-level OS window) via `ctx.show_viewport_immediate(ViewportId::from_hash_of("ocr_review_viewport"), …)`, default inner size `1200×720`, and calls `render::render_review_window_contents(ui, review, &mut actions)` inside its `CentralPanel`. All table/preview rendering lives in that one function in `src/gui/render.rs`. `ReviewActions` (in `render.rs`) is the per-frame click-signal struct the window fills and `render_review_window` dispatches.

6. **The GUI can read the OCR config.** `crate::automation::get_config() -> &'static AutomationConfig` (a process-wide `OnceLock`, set at startup) exposes `score_regions` and, after this plan, the new adjustment. No plumbing is needed to reach it from the render code.


## Progress

- [x] (2026-06-26) M1 (config + crop derivation) — added `ReviewCropAdjust` struct (+ `Default` 0.05/0.0/0.0/0.22), the serde-defaulted `review_crop_adjust` field on `AutomationConfig`, and the pure clamped `review_crop_rect(config, stage)` in `src/automation/config.rs`; re-exported both from `src/automation/mod.rs`. Three unit tests pass (`GAKUMAS_NO_MANIFEST=1 cargo test review_crop` → 3 passed; default extends over portraits & trims right to width≈0.78; over-extension clamps y→0; inset>width → width 0 not negative). `review_crop_rect`/`ReviewCropAdjust` are re-exported but unused until M3 (expected warning).
- [x] (2026-06-27) M2 (state + expand toggle) — added `expanded: Option<u32>` to `ReviewState` (+ `Debug` field + `None` init in `handle_open_review`); added `ReviewActions.toggle_expand: Option<u32>`; `render_review_window` dispatches it (second click on the same row collapses; on expand it calls the existing `load_review_preview` to cache that row's texture for the crops). Compiles clean (`cargo check`, 0 errors); `toggle_expand` is set by M3's 📷 button (dormant until then).
- [x] (2026-06-27) M5 (filters + search) — replaced the lone `すべて表示` toggle with per-status checkboxes (`flagged`/`repaired`/`ok`/`manual`, colour-matched; default flagged+repaired on) plus a kept `すべて表示` master override, and added a live Ctrl+F-style `🔍 スコア検索` box that substring-matches the score cells + iteration. Status and search combine (AND); search is independent state so it persists across status toggling. `ReviewState` gained `show_ok/repaired/flagged/manual`, `search`, and kept `show_all`; the header shows `表示 N / 全 M 件`. Builds clean (3m03s). Manual GUI acceptance pending.
- [x] (2026-06-27) M4 (tuning tool + field calibration) — extended `scripts/region_tuner.py` with the review-crop layer (3 derived purple rects + 4 shared-adjust sliders + per-stage canvas thumbnails + `review_crop_adjust` JSON), pointed its sample dropdown at `tests/fixtures`, and managed its Pillow dep via a uv project (`pyproject.toml`/`uv.lock`). User calibrated against a real screen → `review_crop_adjust = {top 0.05, bottom 0.10, left 0.21, right 0.23}` (the portraits+scores sit in the centred ~0.56-wide column extending below the score band, not above). Baked into `ReviewCropAdjust::default()` in `config.rs`; the `review_crop` unit test updated to the new geometry (3 pass). Raised the tool's bottom-extend slider max to 0.25 for headroom.
- [x] (2026-06-27) M3 (inline dynamic layout) — code-complete. Replaced the fixed 12-column `Grid` + right preview pane in `render_review_window_contents` with a single `ScrollArea::both` of per-row `ui.horizontal`s: dynamic `cell_w = clamp((avail - 44 - 150)/9, 60, 160)`, a header aligned to `iter_w`/`group_w`, and per-stage `ui.vertical` (three cells, then the crop when expanded). Added `draw_stage_crop` (UV sub-image of the cached texture, height from native aspect, "読み込み中…" placeholder while the texture loads). Removed `ReviewActions.preview_iter` and its dispatch; the 📷 button now sets `toggle_expand` and shows 📷✓ when open. Builds clean via `scripts/build.ps1` (3m50s, 0 errors, 28 expected warnings). **Manual GUI acceptance pending** (tray app can't be `cargo test`-driven).

Use timestamps when checking items off, e.g. `- [x] (2026-06-26 14:00Z) ...`.


## Surprises & Discoveries

- Observation: `score_regions` are already normalized `0..1`, so they double as UV rects with zero conversion — `egui::Image::uv` consumes them directly. The "cropping" is a UV change on the already-loaded full-screenshot texture, not a new image.
  Evidence: `src/ocr/mod.rs::crop_region` multiplies the same fractions by `img.width()/height()` to crop for OCR, confirming the fractions are relative to the full captured image.
- Observation: native screenshot is 721×1281; one `score_regions` band is ~721×28px (≈25:1). Scaled into a ~224px three-column group that is ~9px tall — the readability problem motivating the icon-inclusive crop.
  Evidence: measured a sample, `target/release/output/20260116_012805/screenshots/001_20260116_012814.png` → 721×1281; `score_regions[0].height = 0.022` → `0.022 * 1281 ≈ 28px`.
- Observation: the *default* `ReviewCropAdjust` (top_extend 0.05, right_inset 0.22) framed the dark TOTAL banner + the score line, not the character portraits, and looked left-leaning. The portrait band's exact offset from the score line is not what the default guessed, so the default must be calibrated against a real screenshot rather than estimated.
  Evidence: user screenshot of the running review window (2026-06-27): the expanded crop showed "1,272,040 Pt" with the three scores beneath, no icons.
  Resolution: M4 extends `scripts/region_tuner.py` to tune the review crop visually so the right `review_crop_adjust` is found empirically, not guessed.
- (Extend as M4 lands / default is recalibrated.)


## Decision Log

- Decision: derive the human-review crop from `score_regions[stage]` plus a small shared `ReviewCropAdjust { top_extend, bottom_extend, left_inset, right_inset }` (window fractions), rather than storing a full independent `{x,y,width,height}` per stage.
  Rationale: the game developer has confirmed the score-overlap issue and intends to change the *horizontal* score layout in a future update. Reusing `score_regions`' `x`/`width` single-sources exactly the part that will change, so the review crop tracks that re-layout automatically (the scores are calibrated once). The icon-above-score relationship and the right-margin trim are layout-stable, so they live as four small deltas that survive the re-layout. One shared adjust (not per-stage) suffices because the icon-to-score relationship is identical across stages, and it is far fewer numbers to tune (4 vs 12) while still allowing minor readability adjustments. The reader should clamp the derived rect to `[0,1]` so an over-extension never samples outside the image.
  Date/Author: 2026-06-26, user direction ("reuse x/y/width of score_regions because the layout may be adjusted horizontally in the future; still want minor adjustment available").

- Decision: crops are **expand-on-demand** (per row), one row expanded at a time, toggled by the row's existing 📷 button; collapsing on a second click.
  Rationale: user preference, matching `gakumas-tools`' rehearsal page (the selected row expands and shows its image inline). One-at-a-time keeps the table compact on a 200-row run and means a single cached texture is enough.
  Date/Author: 2026-06-26, user direction ("The crop shows only on expand is preferred").

- Decision: each stage's crop is one image spanning that stage's three score columns, sized **dynamically** to the measured column-group width, with displayed height following the crop's native aspect ratio.
  Rationale: user preference ("crop each stage and place under each stage; the cropped stage image has width the same as the three-column-stage width") and the user's explicit push against fixed-pixel sizing. Dynamic width means widening/maximizing the window enlarges the crops, giving the user direct control over legibility. The fixed 72px cells were a workaround for an egui `Grid` quirk (empty-header columns collapsed); since the expand feature moves the rows off `Grid` to per-row layout anyway, cells become proportional to available width too.
  Date/Author: 2026-06-26, user direction.

- Decision: remove the right-hand whole-image preview pane; the inline per-stage crops replace its purpose and reclaim its width for the table.
  Rationale: the inline crops show the relevant region larger and in context; keeping a second whole-image pane would only re-introduce the unreadable-because-tiny view this plan removes. `load_review_preview` is retained (repurposed to load the expanded row's texture for the crops).
  Date/Author: 2026-06-26.

- Decision: tune `review_crop_adjust` empirically by extending the existing `scripts/region_tuner.py` browser tool, rather than estimating defaults from a single inspected screenshot.
  Rationale: the estimated default framed the total banner, not the portraits (see Surprises). The tuner already overlays draggable rectangles on a chosen screenshot and writes a config.json snippet; adding the review crop there lets the user see exactly what the GUI will render and read off the correct adjust. The three crops are derived live from `score_regions` + one shared adjust and move together (dragging any one edits the shared adjust), which both matches the runtime model and visually validates the "same offset works for all three stages" assumption.
  Date/Author: 2026-06-27, user direction ("Extend the existing HTML bounding box adjustment tool for easier parameter adjustment").


## Outcomes & Retrospective

All four milestones landed (M1 config derivation, M2 expand state, M3 inline dynamic layout, M4 tuner + calibration). Against Purpose: opening a finished session and clicking a row's 📷 now expands it and renders each stage's character portraits + printed scores inline, directly under that stage's three editable cells, sized to the cell-group width so widening the window enlarges them — replacing the unreadable narrow whole-screenshot pane. The crop region is a single shared `ReviewCropAdjust` on top of `score_regions`, calibrated to `{top 0.05, bottom 0.10, left 0.21, right 0.23}` via the extended `scripts/region_tuner.py`. Lessons: (1) the score band's extreme aspect ratio, not fixed-vs-dynamic sizing, was the real readability constraint — including the icon (a chunkier crop) was the fix; (2) estimating the crop offset from one inspected screenshot was wrong (it framed the total banner); a visual tuner with live thumbnails got the right value quickly and also validated that one shared offset frames all three stages. Remaining: manual GUI re-verification of the calibrated crop by the user, then merge to main.


## Context and Orientation

Rust Windows tray/GUI app (egui via eframe 0.29). Relevant files, by full repository-relative path:

- `src/automation/config.rs` — `RelativeRect { x, y, width, height }` (fractions `0..1`); `AutomationConfig` with `score_regions: [RelativeRect; 3]` (and `total_regions`, `bonus_regions`); `default_score_regions()`; the process-wide `static CONFIG: OnceLock<AutomationConfig>` exposed by `get_config() -> &'static AutomationConfig`. M1 adds the `ReviewCropAdjust` struct, the `review_crop_adjust` field (with a serde `default`), its `Default` impl, and the `review_crop_rect` helper here.
- `src/automation/mod.rs` — re-exports config types: `pub use config::{get_config, init_config, AutomationConfig, ButtonConfig, RelativeRect};`. M1 adds `ReviewCropAdjust` (and `review_crop_rect`) to this re-export if it is used from `gui`.
- `src/gui/state.rs` — `ReviewState { session_path, rows, edits, show_all, dirty, preview: Option<(u32, TextureHandle)>, open }` (with a manual `Debug` impl) and `GuiState.review: Option<ReviewState>`. M2 adds `expanded: Option<u32>` to `ReviewState` (and to its `Debug`).
- `src/gui/render.rs` — `render_review_window_contents(ui, &mut ReviewState, &mut ReviewActions)`; `ReviewActions { save, close, preview_iter }`; `recovery_color(&str) -> Color32`. The current layout is a fixed-width split: a left `ScrollArea::both` holding a 12-column `Grid` (one column per score, `add_sized([72.0,20.0], TextEdit…)` cells) and a right preview pane drawing `review.preview`. M2 adds `toggle_expand: Option<u32>` to `ReviewActions`; M3 rewrites the body.
- `src/gui/mod.rs` — `GuiApp`; `load_review_preview(ctx, iteration)` (decode→`load_texture`→`review.preview`); `handle_save_review`; `render_review_window(ctx)` (the immediate-viewport window that fills/dispatches `ReviewActions`). M2 dispatches `toggle_expand` here; the texture load is reused for the expanded row.

Terms used in this plan, in plain language:
- "stage" — one of the three rehearsal result rows on the screen; each has up to three character portraits with a printed score under each.
- "per-stage crop" / "icon crop" — the cropped image of one stage (its portraits + printed scores) shown inline; produced as a UV sub-rect of the full screenshot texture.
- "UV rect" — a rectangle in `0..1` texture coordinates telling egui which sub-region of a texture to draw; here it equals the window-fraction crop rect.
- "expand" — clicking a row's 📷 to reveal that row's per-stage crops beneath its cells; clicking again collapses.
- "immediate viewport" — egui's term for an independent top-level OS window driven each frame by a closure; the review window is one.
- "window fraction" — a coordinate in `0.0..1.0` relative to the captured client area, as used by `RelativeRect`.

Build/test: the admin manifest blocks `cargo test` unless built with `GAKUMAS_NO_MANIFEST=1 cargo test` (gate in `build.rs`). The config/crop derivation is pure and unit-testable this way; the egui window is verified by building (`scripts/build.ps1`) and manual interaction (the tray GUI cannot be driven from `cargo test`). A running instance of `gakumas-rehearsal-automation.exe` locks the output binary, so always build via `powershell -ExecutionPolicy Bypass -File scripts/build.ps1` (it aborts in ~1s if the app is running; pass `-Kill` to stop it first) rather than a bare `cargo build --release`.


## Plan of Work

Implement in order. M1 is a pure, unit-tested config helper shippable on its own. M2 adds the expand state and toggle wiring (no visual change yet beyond the texture loading on expand). M3 is the visible payoff: the dynamic inline layout with crops.


### Milestone M1 — Configurable crop derivation from `score_regions`

Goal: a pure function that turns `score_regions[stage]` plus a small, configurable adjustment into the human-review crop rectangle, clamped to the image. After this milestone, `GAKUMAS_NO_MANIFEST=1 cargo test review_crop` passes and proves the derivation and clamping.

In `src/automation/config.rs`, add the adjustment struct and a default that bakes the values derived from a real screenshot (`target/release/output/20260116_012805/screenshots/001_20260116_012814.png`, 721×1281, where stage 1's portraits sit at roughly `y = 0.135..0.175` and its printed scores at `y = 0.179..0.201`, with the 詳細 button on the right beyond `x ≈ 0.78`):

    /// Adjustment applied on top of each `score_regions[stage]` to produce the
    /// human-review crop shown inline in the review window. All values are window
    /// fractions (0..1). One shared instance covers all three stages because the
    /// icon-above-score relationship is identical per stage. Kept separate from
    /// the OCR crop because OCR wants a tight digits-only band, while review wants
    /// the character portraits above the digits so the user can see who/what they
    /// are correcting.
    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    pub struct ReviewCropAdjust {
        /// Extend the crop upward (decreasing y) to include the character
        /// portraits that sit above the printed scores.
        pub top_extend: f32,
        /// Extend the crop downward for breathing room below the digits.
        pub bottom_extend: f32,
        /// Trim from the left edge (default 0 so x tracks score_regions).
        pub left_inset: f32,
        /// Trim from the right edge (drops the 詳細 button / right margin).
        pub right_inset: f32,
    }

    impl Default for ReviewCropAdjust {
        fn default() -> Self {
            Self { top_extend: 0.05, bottom_extend: 0.0, left_inset: 0.0, right_inset: 0.22 }
        }
    }

    fn default_review_crop_adjust() -> ReviewCropAdjust { ReviewCropAdjust::default() }

Add the field to `AutomationConfig` (near `score_regions`), defaulted so existing `config.json` files load unchanged:

    /// Adjustment producing the inline review crop from `score_regions`.
    #[serde(default = "default_review_crop_adjust")]
    pub review_crop_adjust: ReviewCropAdjust,

Set it in the `Default for AutomationConfig` impl too (`review_crop_adjust: ReviewCropAdjust::default()`).

Add the pure derivation helper (free function in `config.rs`), clamping every edge into `[0,1]` and guarding against an inverted/empty rect:

    /// Derive the inline review crop for `stage` from `score_regions[stage]` and
    /// `review_crop_adjust`. Result is clamped to the image so it is always a
    /// valid UV rect. `stage` is 0..=2.
    pub fn review_crop_rect(config: &AutomationConfig, stage: usize) -> RelativeRect {
        let s = config.score_regions[stage];
        let a = config.review_crop_adjust;
        let x0 = (s.x + a.left_inset).clamp(0.0, 1.0);
        let y0 = (s.y - a.top_extend).clamp(0.0, 1.0);
        let x1 = (s.x + s.width - a.right_inset).clamp(0.0, 1.0);
        let y1 = (s.y + s.height + a.bottom_extend).clamp(0.0, 1.0);
        RelativeRect {
            x: x0,
            y: y0,
            width: (x1 - x0).max(0.0),
            height: (y1 - y0).max(0.0),
        }
    }

Re-export from `src/automation/mod.rs` so `gui` can use them:

    pub use config::{get_config, init_config, review_crop_rect, AutomationConfig, ButtonConfig, RelativeRect, ReviewCropAdjust};

Unit tests in `config.rs`'s `tests` module (run with `GAKUMAS_NO_MANIFEST=1 cargo test review_crop`):

- With the default adjust and default `score_regions`, `review_crop_rect(&cfg, 0)` has `y < 0.179` (extended up over the portraits), `y + height ≈ 0.201..0.205`, `x = 0.0`, and `width ≈ 0.78` (right margin trimmed). Assert the rect is inside `[0,1]` on all edges and `width > 0 && height > 0`.
- An exaggerated `top_extend = 0.5` on stage 0 (whose `y = 0.179`) clamps `y` to `0.0` (never negative) and keeps `height > 0`.
- A `right_inset` larger than `width` yields `width == 0.0` (not negative), proving the `.max(0.0)` guard.


### Milestone M2 — Expand state and the 📷 toggle

Goal: clicking a row's 📷 marks that row expanded (or collapses it if already expanded) and ensures that row's full screenshot texture is loaded for the crops. After this milestone the state flips and the texture loads on expand, even though M3 supplies the visible crops.

In `src/gui/state.rs`, add to `ReviewState`:

    /// Iteration whose row is expanded to show inline per-stage crops, or None.
    pub expanded: Option<u32>,

Initialize it `None` wherever `ReviewState` is constructed (`handle_open_review` in `src/gui/mod.rs`), and include it in the manual `Debug` impl.

In `src/gui/render.rs`, add to `ReviewActions`:

    /// A row's 📷 was clicked: toggle that iteration's expanded state.
    pub toggle_expand: Option<u32>,

(Keep `preview_iter` for now; M3 removes its only use. Leaving it avoids a churned diff between milestones — remove it in M3 when the right pane goes.)

In `src/gui/mod.rs::render_review_window`, after collecting actions, dispatch the toggle **before** rendering depends on it next frame:

    if let Some(iter) = actions.toggle_expand {
        if let Some(r) = self.state.review.as_mut() {
            if r.expanded == Some(iter) {
                r.expanded = None;          // second click collapses
            } else {
                r.expanded = Some(iter);
                self.load_review_preview(ctx, iter); // load this row's texture for the crops
            }
        }
    }

`load_review_preview` already caches one `(iteration, TextureHandle)` in `review.preview` and is a no-op if that iteration is already loaded, so it is exactly the loader the crops need; no change to it is required. (Borrow note: `self.load_review_preview` needs `&mut self`, so end the `self.state.review.as_mut()` borrow before calling it — match on the toggle, set `expanded` in one `if let`, then call the loader after, as the snippet does, by re-entering through `self`.)

Acceptance (interim, via a log line or temporary `dbg!`): clicking 📷 sets `review.expanded` and logs a load for that iteration; clicking the same 📷 again clears `expanded`. No crash. The right pane still shows whatever `preview` holds (unchanged until M3).


### Milestone M3 — Dynamic inline layout with per-stage crops

Goal: the review table renders each row with cells that scale to the window width, and an expanded row shows, beneath each stage's three cells, that stage's icon+score crop sized to the column-group width. This is the user-visible deliverable.

Rewrite the body of `render::render_review_window_contents` (`src/gui/render.rs`). Keep the existing top bar (すべて表示 checkbox, 要確認 count, 💾 保存 button, dirty marker, the 📷 hint label) and the `visible: Vec<usize>` filter exactly as they are. Replace the fixed-width left/right split (the `avail`/`preview_w`/`table_w` block, the `horizontal_top` with the `Grid` and the right preview pane) with a single full-width vertical `ScrollArea` containing one `ui.horizontal` per visible row. Delete the right-hand preview pane and the now-unused `preview_w`/`table_w` math; also remove `ReviewActions.preview_iter` and the `📷`→`preview_iter` assignment (replaced by `toggle_expand`).

Compute a dynamic cell width once, from the window width, so columns grow when the user widens the window:

    let avail_w = ui.available_width();
    // 9 score cells + a leading "#" col + a trailing status/📷 cluster.
    // Reserve ~140px for the non-score columns and spacing, divide the rest by 9,
    // and clamp so cells never shrink below readable or grow absurdly wide.
    let cell_w = ((avail_w - 140.0) / 9.0).clamp(60.0, 160.0);

For each visible row index `i`, with `iteration = review.rows[i].iteration` and `expanded = review.expanded == Some(iteration)`:

    ui.horizontal(|ui| {
        ui.label(format!("{}", iteration));               // # column
        for s in 0..3 {
            ui.vertical(|ui| {
                // Row of three editable cells for this stage.
                let cells = ui.horizontal(|ui| {
                    for c in 0..3 {
                        let resp = ui.add_sized(
                            [cell_w, 20.0],
                            egui::TextEdit::singleline(&mut review.edits[i][s][c])
                                .id_source(("cell", i, s, c)),
                        );
                        if resp.changed() { review.dirty = true; }
                    }
                });
                // When expanded, the stage crop sits directly under its cells,
                // exactly as wide as the three-cell group (self-aligned by the
                // enclosing vertical). Width is dynamic; height follows the crop's
                // native aspect so the image is never distorted.
                if expanded {
                    let group_w = cells.response.rect.width();
                    draw_stage_crop(ui, review, iteration, s, group_w);
                }
            });
        }
        let rec = review.rows[i].recovery.clone();
        ui.label(RichText::new(&rec).color(recovery_color(&rec)).small());
        let label = if expanded { "📷✓" } else { "📷" };
        if ui.button(label).on_hover_text("画像で確認（クリックで開閉）").clicked() {
            actions.toggle_expand = Some(iteration);
        }
    });
    ui.separator();

Add the crop drawing helper in `render.rs`. It pulls the config crop rect, converts it to a UV rect, computes the displayed height from the **texture's** native pixel size (so the aspect is correct for whatever resolution the screenshot is), and draws the sub-image. It only draws when `review.preview` holds *this* iteration's texture (loaded by M2's dispatch); otherwise it shows a short placeholder so a missing or still-loading image is obvious rather than blank:

    fn draw_stage_crop(
        ui: &mut egui::Ui,
        review: &ReviewState,
        iteration: u32,
        stage: usize,
        width: f32,
    ) {
        let tex = match &review.preview {
            Some((iter, tex)) if *iter == iteration => tex,
            _ => {
                ui.add_space(2.0);
                ui.label(RichText::new("画像を読み込み中…").small().weak());
                return;
            }
        };
        let cfg = crate::automation::get_config();
        let crop = crate::automation::review_crop_rect(cfg, stage);
        if crop.width <= 0.0 || crop.height <= 0.0 {
            return;
        }
        let uv = egui::Rect::from_min_max(
            egui::pos2(crop.x, crop.y),
            egui::pos2(crop.x + crop.width, crop.y + crop.height),
        );
        // Native aspect from the actual loaded texture (e.g. 721x1281), not a
        // hardcoded constant, so any capture resolution renders undistorted.
        let tex_sz = tex.size_vec2();
        let crop_w_px = (crop.width * tex_sz.x).max(1.0);
        let crop_h_px = (crop.height * tex_sz.y).max(1.0);
        let height = width * (crop_h_px / crop_w_px);
        ui.add_space(2.0);
        ui.add(egui::Image::new((tex.id(), egui::Vec2::new(width, height))).uv(uv));
    }

Note the helper takes `&ReviewState` (immutable) — call it after the cell loop has finished mutating `review.edits`, which the snippet does (the `if expanded` runs after the `ui.horizontal` returns its response). If the borrow checker objects to borrowing `review` immutably while `review.edits` was borrowed mutably in the same closure, snapshot the texture handle and crop before the cell loop, or split the per-stage closure so the mutable edit borrow ends before `draw_stage_crop`.

Because only one row is expanded at a time and `review.preview` caches exactly that row's texture, no extra memory is used versus the old right-pane preview.

Acceptance (manual, build + run): open a finished session with flagged rows; the table fills the window width and the cells widen when the window is widened. Click a row's 📷 — that row expands and, under each of its three stages, the character icons and printed scores appear, each image as wide as that stage's three cells. Widen/maximize the window: the crops (and cells) grow. Click 📷 again — the row collapses. Edit a cell, 💾 保存, reopen — the correction persists (save path is unchanged from the prior plan).


### Milestone M4 — Tune `review_crop_adjust` with the region tuner

Goal: find the correct shared `ReviewCropAdjust` for the real game layout (so the crop frames the character portraits + printed scores, not the total banner) using a visual tool, and record it in `config.json`.

`scripts/region_tuner.py` is an existing local browser tool (run `uv run scripts/region_tuner.py`, open `http://127.0.0.1:8777`) that overlays draggable rectangles on a chosen screenshot to calibrate the OCR `total_regions`/`bonus_regions`, reading current values from and emitting a JSON snippet for `config.json`. This milestone extends it with a **review crop** layer:

- Server (`region_tuner.py`): `load_regions()` also returns `score` (from `config.json`'s `score_regions`, falling back to `SCORE_FALLBACK` that mirrors `default_score_regions()` in `src/automation/config.rs`); a new `load_review_adjust()` reads `review_crop_adjust` (fallback `REVIEW_ADJUST_FALLBACK` mirroring `ReviewCropAdjust::default()`); `/samples` returns both.
- Client: three purple dashed rectangles, one per stage, derived live by a JS `reviewRect(i)` that mirrors `review_crop_rect()` (`score_regions[i]` ± the shared adjust, clamped). Four sliders (top/bottom extend, left/right inset) drive the shared adjust; dragging any rectangle (move = translate → `left+=dx,right-=dx,top-=dy,bottom+=dy`; corner = resize far edges → `right-=dx,bottom+=dy`) edits the same shared adjust so all three move together. A `<canvas>` thumbnail under the sliders renders each derived crop from the loaded image at native aspect — exactly what the GUI's review window draws. The `config.json snippet` textarea now also includes a `review_crop_adjust` object to copy.

To use: run the tuner, type the path to one of this run's screenshots (e.g. `target/release/output/<session>/screenshots/001_*.png`) into the custom-path box, then drag/slide until the three purple boxes frame each stage's portraits + scores and the thumbnails look right. Copy the `review_crop_adjust` block into `config.json`, relaunch the app, and re-open the review window to confirm. Once a good value is found, update `ReviewCropAdjust::default()` in `src/automation/config.rs` so fresh installs ship the calibrated crop.

Acceptance: with the tuned `review_crop_adjust` in `config.json`, the GUI review window's expanded crops show the character portraits and printed scores (not the total banner), framed without excess margin.


## Concrete Steps

From repo root `C:\Work\GitRepos\gakumas-rehearsal-automation` (PowerShell; the Bash tool is also available):

    # M1 — config + derivation (pure, unit tested)
    GAKUMAS_NO_MANIFEST=1 cargo test review_crop

    # M2/M3 — build the GUI (guarded; aborts in ~1s if the app is running)
    powershell -ExecutionPolicy Bypass -File scripts/build.ps1
    # if it reports the app is running and you want it stopped automatically:
    powershell -ExecutionPolicy Bypass -File scripts/build.ps1 -Kill

    # Run and verify manually
    .\target\release\gakumas-rehearsal-automation.exe

Build emits ~30 expected warnings; only `^error` lines matter (`cargo check 2>&1 | grep "^error"`).


## Validation and Acceptance

- M1: `GAKUMAS_NO_MANIFEST=1 cargo test review_crop` passes; the new tests prove `review_crop_rect` extends upward over the portraits, trims the right margin, stays within `[0,1]`, and never produces a negative width/height. They fail before the function exists and pass after.
- M2: with a temporary log line, clicking a row's 📷 sets `review.expanded` and triggers a texture load for that iteration; a second click clears it. No panic.
- M3 (manual): in the running GUI, open a finished session's review window. Confirm: (a) cells fill the window width and grow when the window is widened (dynamic, not a fixed 72px); (b) clicking 📷 expands the row and shows three per-stage crops, each containing the character portraits and printed scores, each as wide as its stage's three cells and sitting directly beneath them; (c) maximizing the window enlarges the crops; (d) a second 📷 click collapses the row; (e) editing a cell and pressing 💾 保存 still rewrites the CSVs (`recovery=manual` on edited rows) as before. Then write `Outcomes & Retrospective`.


## Idempotence and Recovery

`cargo test`/`cargo build` are idempotent. M1 is additive: a new struct, a defaulted field (so existing `config.json` files load unchanged via serde defaults), and a pure clamped function — worst case a mis-tuned adjustment shows a slightly off crop, never a crash, and the OCR/score data is untouched. M2/M3 only change how the **review window** draws; they do not touch capture, OCR, or the save path. The crop is a read-only UV view of an already-loaded texture, so it cannot corrupt anything. Closing the window or collapsing a row is free; nothing is persisted by expanding. Rebuild via `scripts/build.ps1` to avoid the running-app link lock.


## Artifacts and Notes

Reference geometry (sample `target/release/output/20260116_012805/screenshots/001_20260116_012814.png`, 721×1281):

    score_regions[0]      = { x: 0.0, y: 0.179, width: 1.0,  height: 0.022 }   # OCR digits-only band
    review_crop_adjust    = { top_extend: 0.05, bottom_extend: 0.10,
                              left_inset: 0.21, right_inset: 0.23 }            # calibrated default
    review_crop_rect(.,0) = { x: 0.21, y: 0.129, width: 0.56, height: 0.172 }  # centred portrait+score column

The crop is one `egui::Image` per stage with `.uv(Rect)`; no second texture and no CPU cropping — the inline view is essentially free on top of the texture the old right pane already loaded.


## Interfaces and Dependencies

No new crates (reuse `eframe/egui`, `image`, `serde`). End-state signatures:

In `src/automation/config.rs`:

    pub struct ReviewCropAdjust { pub top_extend: f32, pub bottom_extend: f32, pub left_inset: f32, pub right_inset: f32 }
    impl Default for ReviewCropAdjust { /* 0.05 / 0.0 / 0.0 / 0.22 */ }
    // AutomationConfig gains: pub review_crop_adjust: ReviewCropAdjust  (serde default)
    pub fn review_crop_rect(config: &AutomationConfig, stage: usize) -> RelativeRect;  // clamped to [0,1]

In `src/automation/mod.rs`: extend the existing `pub use config::{…}` with `review_crop_rect` and `ReviewCropAdjust`.

In `src/gui/state.rs`: `ReviewState` gains `pub expanded: Option<u32>` (and its `Debug` impl).

In `src/gui/render.rs`: `ReviewActions` gains `pub toggle_expand: Option<u32>` and loses `preview_iter`; add `fn draw_stage_crop(ui, &ReviewState, iteration: u32, stage: usize, width: f32)`; `render_review_window_contents` body rewritten to the per-row dynamic layout.

In `src/gui/mod.rs`: `render_review_window` dispatches `toggle_expand` (set/clear `expanded`, call the existing `load_review_preview` on expand); the `preview_iter` dispatch is removed.


## Revision Note

2026-06-26: Initial authoring. Refines the review window from `docs/EXECPLAN_OCR_REVIEW_EDIT_GUI.md`: replaces the narrow right-hand whole-screenshot preview with inline, expand-on-demand, per-stage crops placed under each stage's editable columns, sized dynamically to the column-group width (so widening the window enlarges them). The crop region is derived from `score_regions` plus a small configurable `ReviewCropAdjust` (top/bottom extend, left/right inset) rather than an independent region, so the game developer's forthcoming horizontal score re-layout propagates from a single calibration source while leaving readability adjustments available. Three milestones: the pure configurable derivation (unit tested), the expand state + 📷 toggle wiring, and the dynamic inline layout with UV-cropped per-stage images.

2026-06-27: Added milestone M4 after field testing M3. The estimated default `ReviewCropAdjust` framed the dark total banner instead of the character portraits (user screenshot), so rather than guessing new numbers, extended the existing `scripts/region_tuner.py` browser tool with a review-crop layer (three live-derived purple rectangles + four shared-adjust sliders + per-stage canvas thumbnails + a `review_crop_adjust` JSON snippet) so the correct adjust is found visually against a real screenshot and pasted into `config.json`. The runtime code (M1–M3) is unchanged; only the tuner and the plan grew.
