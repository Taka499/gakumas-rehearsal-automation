# Sustainability refactoring batch: dedup, seams, and state consolidation

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. It must be maintained in accordance with `docs/PLANS.md` (from the repository root).

## Purpose / Big Picture

This plan makes the codebase cheaper to change, without changing what the user sees. It was produced by a four-subsystem design audit (GUI, OCR, automation, capture/analysis/calibration) in July 2026. The audit found that the app's behaviour is correct but that several implementations are copy-pasted in two or three places (so every fix must be applied multiple times), that the automation runner exposes seven independent process-global variables to the GUI (so every new live feature widens an already implicit interface), and that the automation state machine is welded to Windows API calls (so its eleven-state sequencing logic can only be tested by hand against the live game).

After this plan is complete:

- A bug fix in screen capture, Tesseract invocation, or box-plot drawing lands in exactly one place instead of two or three.
- The GUI reads run progress through one snapshot call per frame instead of six separate global accessors, and adding a live-progress field is a one-struct edit.
- The automation state machine (start → wait → click → capture → loop) is unit-testable on any machine with `GAKUMAS_NO_MANIFEST=1 cargo test`, with no game window and no Windows input APIs. New tests prove sequences like "abort during a wait ends in Aborted" that previously required a manual run.
- The OCR entry point takes one region-set value instead of three parallel arrays.
- The review-window lifecycle (open, preview, save) lives in one module instead of being spread across `src/gui/mod.rs` and `src/gui/render.rs`.

Every milestone is behaviour-preserving. The proof for each is: the full unit-test suite still passes, the guarded release build still succeeds, and (for milestones touching capture, GUI, or the runner) a short manual click-through against the live game behaves exactly as before.

## Progress

- [x] (2026-07-03) M1: Deduplicate the D3D11 capture pipeline — `capture_and_crop(hwnd, CropBox, verbose)` in `screenshot.rs`; the three public fns are thin wrappers; region.rs keeps only coordinate math (net −283 lines). 115 tests pass; merged to main (`f4cf726`). Live-game capture check deferred to the final click-through.
- [x] (2026-07-03) M2: Deduplicate Tesseract invocation — private `run_tesseract_tsv(img, tag, psm, whitelist)`; three public recognizers are now wrappers (net −78 lines). 115 tests + `ocr_overlap_recovery_e2e` pass; merged to main (`c545980`).
- [x] (2026-07-03) M3: Deduplicate nine-box drawing — generic `draw_nine_boxes` + shared colour/geometry consts; callers pass `BoxStrokes { 2,2,1 }` (file) / `{ 3,3,2 }` (live). Proof: regenerated `temp/live_box_plot_preview.png` is pixel-identical to the baseline (0 differing pixels). Merged to main (`5e962f0`).
- [x] (2026-07-03) M4: `CSV_HEADER` shared (`pub(crate)` in csv_writer, imported by results_edit); `MAX_DIGIT_STREAM_LEN`/`MAX_PART_DIGITS` named in reconcile.rs with a file-top JS-parity note. Names only, no expression changed; 40 reconcile tests pass. Merged to main (`df0b906`).
- [x] (2026-07-03) M5: `LiveChartCache` extracted to `src/gui/live_chart.rs` (update/invalidate/texture/stats/counts); `GuiApp` keeps only the viewport fields. 115 tests pass, guarded release build OK (1m57s, 28 expected warnings). Merged to main (`4a6eaf3`).
- [x] (2026-07-03) M6: Five progress statics → one `Mutex<Progress>` + `get_progress()` snapshot + private `progress_update()`; GUI/analysis callers migrated (incl. `analysis::generate_analysis`). 115 tests pass, guarded build OK. Merged to main (`84feb10`).
- [x] (2026-07-03) M7: `OcrRegions` view struct + `AutomationConfig::ocr_regions()`; `ocr_screenshot`/`run_ocr_worker` take one value; runner/main.rs/e2e/worker-test migrated. Serde format untouched. 115 tests + e2e pass. Merged to main (`934b956`).
- [x] (2026-07-03) M8: `GameOps` trait + `LiveGame` production impl (kept in `state.rs` rather than a new file — the moved code all originated there); `AutomationContext<G: GameOps>`; detection waits take an injected `abort: &dyn Fn() -> bool`. Six new MockGame tests (happy-path ordering, resume numbering, abort-vs-error, pre-step abort, dead window). 121 tests pass (baseline 115 + 6); guarded build OK. Merged to main (`26f32ee`).
- [x] (2026-07-03) M9: `ReviewController` in `src/gui/review.rs` owns `Option<ReviewState>` + open/load_preview/save/show; `show()` returns `SaveEffects { session_path, scores_changed }` and `GuiApp` applies the cross-module reactions (attention counts, live reload, chart regen). `GuiState.review` field removed. 121 tests pass, guarded build OK. Merged to main (`b5e21fc`).
- [ ] Final: full manual click-through against the live game (see Validation and Acceptance) — needs the user + gakumas.exe; everything automated is green (121 unit + 2 ignored e2e, guarded build, dedup greps).

Use timestamps (UTC) when checking items off, e.g. `- [x] (2026-07-03 14:00Z) M1 done`.

## Surprises & Discoveries

(To be filled in during implementation.)

## Decision Log

- Decision: `reconcile.rs` receives ONLY additive changes (named constants, comments). No restructuring, no function extraction, no reordering.
  Rationale: `src/ocr/reconcile.rs` was ported line-for-line to JavaScript (`rehearsalRecovery.js` in the `gakumas-tools` fork, upstream PR surisuririsu/gakumas-tools#103) and a 1200-row replay harness verifies the two produce identical output. Structural refactoring would break the line-by-line correspondence that makes future paired fixes cheap, for zero behavioural gain.
  Date/Author: 2026-07-03 / Claude + Taka499.

- Decision: The region consolidation (M7) is a parameter object (`OcrRegions`, built from the existing three config fields), NOT a restructuring of the serialized config format into per-stage structs.
  Rationale: `AutomationConfig` is serialized to the user's config file with per-field serde defaults; changing `score_regions`/`total_regions`/`bonus_regions` into a nested structure would break every existing config file or require a migration. The audit's pain point (three parallel arrays threaded through four call sites) is fully solved by a view struct with no serialization change.
  Date/Author: 2026-07-03 / Claude + Taka499.

- Decision: No speculative geometry helpers (`compose_relative_rect`, `inflate_region`) are added.
  Rationale: The design-audit report proposed them for the review-window inline-crops feature, but that feature is already complete and its geometry lives in `config.rs::review_crop_rect` with unit tests. A helper with no second caller is a hypothetical seam ("one adapter means a hypothetical seam") — add them when a real caller appears.
  Date/Author: 2026-07-03 / Claude + Taka499.

- Decision: `AUTOMATION_RUNNING` stays a separate `AtomicBool` and `ABORT_REQUESTED` stays a separate atomic in `state.rs`; neither moves into the consolidated `Progress` struct (M6).
  Rationale: `AUTOMATION_RUNNING` is used with an atomic `swap` as the mutual-exclusion guard for starting a run — folding it into a mutex-guarded struct would change the concurrency semantics for no benefit. `ABORT_REQUESTED` is a signal with a different lifetime than progress state (set by a hotkey thread, reset at run start).
  Date/Author: 2026-07-03 / Claude + Taka499.

- Decision: `LIVE_SCORES` also stays separate from the `Progress` struct.
  Rationale: It is a growing `Vec` with its own cheap change-detection accessor (`live_score_count`), already covered by a unit test; bundling it into a per-frame snapshot clone would copy the whole vector every frame.
  Date/Author: 2026-07-03 / Claude + Taka499.

- Decision: Per-milestone live-game manual checks are deferred to the single final click-through; each milestone merges to main once the automated gates pass (cargo check clean, full unit suite, relevant `--ignored` e2e, guarded build for GUI/runner milestones).
  Rationale: The implementation session cannot drive the live game; batching the manual acceptance into one end-to-end pass covers the same behaviours without blocking every merge.
  Date/Author: 2026-07-03 / Claude.

- Decision: The M1 acceptance grep is "one *definition* of the pipeline" (`fn create_d3d11_device` etc. each appear once), not "one textual occurrence of D3D11CreateDevice" — the import line plus the single call site are two hits in the one remaining copy.
  Rationale: The original grep metric was imprecise about imports.
  Date/Author: 2026-07-03 / Claude.

## Outcomes & Retrospective

(2026-07-03, all nine milestones code-complete and merged; only the live-game click-through remains.)

All nine milestones landed in one session as nine feature branches merged to main (`54a9039`..`b167575`), each gated on a clean `cargo check`, the full unit suite, the relevant `--ignored` e2e, and (for GUI/runner milestones) the guarded release build. Net effect: ~440 lines of duplicated implementation deleted (capture −283, Tesseract −78, nine-box −11 net with the helper, plus the CSV header); the runner's GUI-facing interface shrank from six global accessors to `is_automation_running()` + one `get_progress()` snapshot; the state machine gained its first-ever unit tests (6, covering ordering, resume numbering, and abort-vs-error semantics) via the `GameOps` seam; and the GUI's two most fragmented features (live chart, review window) each moved into a module that owns their whole lifecycle.

Proof highlights: the M3 dedup was verified pixel-identical (0 differing pixels in the regenerated live-plot preview vs the pre-refactor baseline); the M2/M7 OCR changes were verified by the real-Tesseract e2e on the LFS sample screenshots; M4's reconcile edits changed names only and all 40 reconcile tests (including the 1200-row field replay) pass unchanged.

Lessons: (a) the `replace_all` edit of the abort checks missed the differently-indented occurrence — a grep after every bulk replace caught it; (b) textual grep counts in the acceptance criteria needed refinement to "definition sites" since imports/comments also match; (c) keeping `LiveGame` in `state.rs` (rather than the plan's optional `game_ops.rs`) kept the M8 diff reviewable as a move-not-rewrite.

Remaining: the single end-to-end manual click-through (fresh 3-run series with live figure, abort, resume, extend, review edit + verify + save, chart regen). Until it passes, treat the refactor as code-complete rather than accepted.

## Context and Orientation

This repository is a Windows-only Rust application (`gakumas-rehearsal-automation.exe`) that automates "rehearsal" runs in the game gakumas.exe: it clicks the game's buttons, screenshots each result screen, OCRs the nine per-character scores with an embedded Tesseract, recovers OCR damage using an on-screen checksum, and shows live statistics in an egui GUI. Key module map:

- `src/capture/` — screen capture via the Windows Graphics Capture (WGC) API: a D3D11 GPU pipeline copies the captured frame to a staging texture, maps it for CPU read, and converts BGRA→RGBA.
- `src/ocr/` — per-stage crop → threshold → Tesseract CLI → parse → reconcile pipeline. `reconcile.rs` is the checksum-based damage-recovery solver (READ-MOSTLY, see Decision Log).
- `src/automation/` — the run loop: `runner.rs` spawns an automation thread (state machine in `state.rs`) and an OCR worker thread (`ocr_worker.rs`); `detection.rs` decides "is the screen ready" by brightness/histogram; `input.rs` clicks via `SendInput`; `csv_writer.rs`/`results_edit.rs` persist results; `session_meta.rs` writes `run-meta.json`.
- `src/gui/` — egui window. `mod.rs` holds `GuiApp` (the application struct) and `run_gui`; `render.rs` holds pure render functions that return action structs which `GuiApp::update` dispatches to `handle_*` methods; `state.rs` holds the `GuiState`/`ReviewState` data bags.
- `src/analysis/` — statistics and plotters-based charts, both file-output and an in-memory RGBA render for the live GUI panel.

Terms used below:

- "Deep module": a module hiding a lot of behaviour behind a small interface. The refactorings below either deepen a module (hide duplicated implementation behind one function) or narrow an interface (replace many globals/params with one value).
- "Seam": a place where behaviour can be substituted without editing code at that place — concretely here, a Rust trait with a production implementation and a test implementation.
- "Guarded build": `powershell -ExecutionPolicy Bypass -File scripts/build.ps1` — it aborts in ~1s if the app is running (a running instance locks the exe and a bare `cargo build --release` would waste minutes before failing at link). Always use it, or check `Get-Process gakumas-rehearsal-automation` first.
- "Unit tests": the exe embeds an administrator manifest, which makes the normal `cargo test` harness require elevation (os error 740). `GAKUMAS_NO_MANIFEST=1 cargo test` skips embedding the manifest (the gate is in `build.rs`) so tests run unelevated. Tesseract-dependent end-to-end tests are `#[ignore]`d and run explicitly, e.g. `GAKUMAS_NO_MANIFEST=1 cargo test ocr_overlap_recovery_e2e -- --ignored`.
- Release builds emit ~30 expected warnings (unused `pub use` re-exports, OCR dead code). Filter with `cargo check 2>&1 | grep "^error"` to find real failures.

Branching: follow git-flow. Create one feature branch per milestone (suggested names below), commit small and often, only add the affected files, never add Claude Code attribution, and merge to `main` when that milestone's acceptance passes. Milestones M1–M4 are independent of each other and of M5–M9; M5 should land before M9 (both touch `GuiApp`); M6 and M8 both touch the runner/state area, do them in order.

Before starting, record the baseline: run `GAKUMAS_NO_MANIFEST=1 cargo test 2>&1 | tail -5` from the repo root and note the total passed count (expected: ~116, all passing). Every milestone below must end with at least that many tests passing (more where a milestone adds tests).

## Plan of Work

### M1 — Deduplicate the D3D11 capture pipeline (branch `feature/sustain-capture-dedup`)

The identical WGC capture sequence (create D3D11 device → create capture item → frame pool → session → wait for frame → staging texture → map → crop loop with BGRA→RGBA conversion → unmap → close) exists three times:

- `src/capture/screenshot.rs` lines 37–225: `capture_gakumas()` (finds the window itself, logs verbosely, saves a PNG, returns the path).
- `src/capture/screenshot.rs` lines 274–425: `capture_gakumas_to_buffer(hwnd)` (quiet, returns the `ImageBuffer`).
- `src/capture/region.rs` lines 35–196: `capture_region(hwnd, rel_rect)` (converts a `RelativeRect` — relative 0.0–1.0 coordinates — to absolute pixels, clamps, returns the cropped `ImageBuffer`).

The three private helpers are also duplicated verbatim across the two files: `create_d3d11_device` (screenshot.rs:231–253 = region.rs:199–221), `create_direct3d_device` (258–266 = 224–232), `create_capture_item` (430–445 = 235–247; the screenshot.rs copy logs two lines, the region.rs copy does not).

Edits, all in `src/capture/screenshot.rs`:

1. Add a private struct and function:

        struct CropBox { x: u32, y: u32, width: u32, height: u32 }

        /// Runs the full WGC capture of `hwnd` and returns the sub-image
        /// described by `crop` (in full-window coordinates, i.e. including
        /// the client-area offset). `verbose` gates all logging so the
        /// polling callers (brightness detection runs this several times a
        /// second) do not flood the log.
        fn capture_and_crop(hwnd: HWND, crop: CropBox, verbose: bool)
            -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>>

    Its body is the existing pipeline from `capture_gakumas_to_buffer`, with the crop parameters taken from `crop` instead of computed inline, and every `crate::log(...)` call wrapped in `if verbose`.

2. Rewrite the three public functions as thin wrappers. `capture_gakumas()` finds the window, computes the client-area `CropBox`, calls `capture_and_crop(hwnd, crop, true)`, and keeps its save-to-PNG + return-path tail (lines 215–224) unchanged. `capture_gakumas_to_buffer(hwnd)` computes the same `CropBox` and calls with `verbose = false`. `capture_region` keeps its relative→absolute conversion and clamping (region.rs lines 40–50), builds a `CropBox` with `x = client_offset.x + region_x` (matching region.rs lines 151–153), and calls with `verbose = false`.

3. Make the three helper functions (`create_d3d11_device`, `create_direct3d_device`, `create_capture_item`) exist only once, as private functions in `screenshot.rs`, with logging inside `create_capture_item` moved behind the `verbose` flag (pass it in, or inline the two log lines into `capture_and_crop`). Delete the copies in `region.rs`; `region.rs` shrinks to the coordinate math plus a call into `pub(crate) fn capture_and_crop` (export it `pub(crate)` from `screenshot.rs` via `super::screenshot::capture_and_crop`, or move `capture_region` itself into `screenshot.rs` and keep `region.rs` as a re-export — prefer the first, smaller diff).

Behaviour notes to preserve exactly: the 5-second frame timeout; `SetIsCursorCaptureEnabled(false)`; the bounds `break`s in the crop loop; the BGRA→RGBA channel order. One accepted log-only change: `capture_gakumas_to_buffer` currently emits the two "activation factory" lines from `create_capture_item` on every call and will no longer do so; this is noise reduction, record it in Surprises & Discoveries if anything downstream cared.

### M2 — Deduplicate Tesseract invocation (branch `feature/sustain-tesseract-dedup`)

`src/ocr/engine.rs` contains three functions that each perform: save the image to a temp PNG → build a `tesseract.exe` command (with `CREATE_NO_WINDOW` on Windows) → run it → log stderr → check exit status → read the `.tsv` output file → delete it → `parse_tsv_output`:

- `recognize_image` (lines 32–96): psm 6, no whitelist, output tag `tesseract_out_<pid>`; on TSV-read failure additionally logs Tesseract's stdout.
- `recognize_image_line` (lines 195–247): psm 6, no whitelist, tag `tesseract_line_<pid>`.
- `recognize_single_number` (lines 268–332): psm 7, `tessedit_char_whitelist=<whitelist>`, tag `tesseract_num_<pid>`, then joins the parsed text and delegates to the pure helper `parse_single_number`.

Edit: extract one private function in `engine.rs`:

        /// Saves `img` to a temp PNG, runs the embedded tesseract.exe with
        /// TSV output, and returns the parsed lines. `whitelist` adds
        /// `-c tessedit_char_whitelist=...` when Some. `tag` keeps the three
        /// callers' temp-file names distinct (tesseract_out / tesseract_line
        /// / tesseract_num), preserving today's on-disk naming.
        fn run_tesseract_tsv(
            img: &ImageBuffer<Luma<u8>, Vec<u8>>,
            tag: &str,
            psm: &str,
            whitelist: Option<&str>,
        ) -> Result<Vec<OcrLine>>

Move the shared scaffolding into it (including the stderr logging, the exit-status error, the TSV-read failure logging with output base + expected path + stdout, and the cleanup). The three public functions become two-to-five-line wrappers; `recognize_single_number` keeps its text-joining and `parse_single_number(raw, anchor_plus)` tail. Do NOT introduce a `trait OcrEngine`: Tesseract is embedded and non-swappable; a trait with one implementation is indirection without a seam. Leave `recognize_image_simple` (lines 404–435, stdout-mode debug helper) untouched. Leave the pure helpers `parse_tsv_output`, `parse_single_number`, `longest_digit_run` and their unit tests untouched.

### M3 — Deduplicate nine-box drawing (branch `feature/sustain-boxplot-dedup`)

`src/analysis/charts.rs` draws the nine box plots twice with only stroke widths differing:

- `generate_combined_box_plot` (lines 465–605; the per-column loop is 536–601): file output via `BitMapBackend::new`, strokes outline 2 / median 2 / whisker+caps 1.
- `render_live_box_plot_rgba` (lines 622–765; loop 688–743): in-memory via `BitMapBackend::with_buffer`, strokes outline 3 / median 3 / whisker+caps 2 (sized for the GUI downscale).

Edit: extract a private generic helper:

        struct BoxStrokes { outline: u32, median: u32, whisker: u32 }

        fn draw_nine_boxes<DB: plotters::prelude::DrawingBackend>(
            chart: &mut ChartContext<DB, Cartesian2d<RangedCoordf64, RangedCoordf64>>,
            stats: &super::statistics::DataSetStats,
            strokes: BoxStrokes,
        ) -> Result<()>

containing the shared loop: per column `idx`, stage colour from `[RGBColor(220,80,80), RGBColor(80,180,80), RGBColor(80,120,200)]` by `idx / 3`, box half-width 0.35, cap half-width 0.2, whisker colour `RGBColor(80,80,80)`, median colour `RGBColor(200,50,50)`, drawing fill, outline, median, both whiskers, both caps. (plotters' error type is generic over the backend; the simplest signature maps drawing errors with `.map_err(|e| anyhow::anyhow!("{}", e))?` inside the helper, matching the file's existing anyhow usage.) The two callers keep their own canvas setup, y-range computation (note the live one clamps `range` with `.max(1.0)` — keep that difference where it is), axis/mesh config, and label strips; each calls `draw_nine_boxes(&mut chart, stats, BoxStrokes { ... })` with its stroke set. The stage-colour array and the two half-width constants move to module-level `const`s so they exist once.

### M4 — Shared CSV header + named reconcile constants (branch `feature/sustain-constants`)

Two trivially duplicated/unnamed facts:

1. The results-CSV header string `iteration,timestamp,screenshot,s1c1,s1c2,s1c3,s2c1,s2c2,s2c3,s3c1,s3c2,s3c3,recovery` is defined twice: `src/automation/csv_writer.rs:21` and `src/automation/results_edit.rs:43–44` (both `const CSV_HEADER`). Change the `csv_writer.rs` one to `pub(crate) const CSV_HEADER`, delete the `results_edit.rs` copy, and add `use crate::automation::csv_writer::CSV_HEADER;` there. A future schema change (e.g. a new column) then requires one edit. Do not touch the string literals inside `#[cfg(test)]` blocks — tests asserting on literal file content are more honest that way.

2. `src/ocr/reconcile.rs` (READ-MOSTLY — see Decision Log; these edits are additive-only and change no behaviour):
   - Line 558: `if n == 0 || n > 21` — introduce `const MAX_DIGIT_STREAM_LEN: usize = 21;` near `MAX_SCORE` (line 40) with a comment: three scores of at most 7 digits each is 21 digits; anything longer is garbage, and the duplicated-leading-digit case is handled per-part, not by lengthening the stream.
   - Line 572: `compositions(n, k, 1, 8)` — introduce `const MAX_PART_DIGITS: usize = 8;` with a comment: a part is normally ≤ 7 digits (score < 3,000,000), but a colliding leading "1" is sometimes duplicated by OCR, inflating one part to 8; the 8-digit case is collapsed back to 7 by the candidate generator.
   - Add one file-top comment line noting that `rehearsalRecovery.js` in the gakumas-tools fork mirrors this file line-for-line and behavioural changes must be ported in pairs.
   The 23 reconcile unit vectors and the replay-verified behaviour must be bit-identical: this milestone must not change any expression, only name numbers and add comments.

### M5 — Extract `LiveChartCache` (branch `feature/sustain-live-chart-cache`)

`GuiApp` in `src/gui/mod.rs` manages the live distribution figure through six loosely coordinated fields (lines 112–124): `live_chart_tex` (the uploaded egui texture), `live_chart_rendered_count` (the live-buffer row count the texture was built from — the change detector), `live_chart_stats` (the stats behind the 6×9 table), `live_chart_total` / `live_chart_excluded` (included/flagged row counts for the header), and `live_chart_dirty` (forced-refresh flag set after review saves, line 835). The update logic is `GuiApp::update_live_chart` (lines 321–359); render reads the fields around lines 1032–1046. Forgetting to update one field with the others breaks the display silently — the invariant "these six describe one rendered figure" lives only in the maintainer's head.

Edits:

1. Create `src/gui/live_chart.rs` with:

        pub struct LiveChartCache {
            tex: Option<egui::TextureHandle>,
            rendered_count: usize,
            stats: Option<crate::analysis::statistics::DataSetStats>,
            total: usize,
            excluded: usize,
            dirty: bool,
        }

        impl LiveChartCache {
            pub fn new() -> Self { ... }
            /// Re-renders the texture iff the live buffer changed or
            /// `invalidate` was called. No-op when `enabled` is false.
            pub fn update(&mut self, ctx: &egui::Context, enabled: bool) { ... }
            pub fn invalidate(&mut self) { self.dirty = true; }
            pub fn texture(&self) -> Option<&egui::TextureHandle> { ... }
            pub fn stats(&self) -> Option<&DataSetStats> { ... }
            pub fn counts(&self) -> (usize, usize) { (self.total, self.excluded) }
        }

   `update` is the moved body of `update_live_chart` (it reads `runner::get_live_scores`/`live_score_count`, filters flagged rows, calls `analysis::charts::render_live_box_plot_rgba`, uploads via `ctx.load_texture`, and sets all six fields together).

2. In `src/gui/mod.rs`: declare `mod live_chart;`, replace the six fields with `live_chart: LiveChartCache`, replace the `update_live_chart` method with a call to `self.live_chart.update(ctx, self.state.show_live_chart)`, replace `self.live_chart_dirty = true` (line 835 and the launch-seed path) with `self.live_chart.invalidate()`, and update the render reads to use the accessors. The window-size fields `live_chart_expanded` and `saved_show_live_chart` (lines 127–130) are viewport/persistence concerns, not figure state — they stay on `GuiApp`.

### M6 — Consolidate runner progress globals (branch `feature/sustain-progress-struct`)

`src/automation/runner.rs` exposes run progress to the GUI through five separate statics plus accessor pairs: `CURRENT_ITERATION` (line 25), `TOTAL_ITERATIONS` (line 28), `CURRENT_STATE_DESC` (line 112), `CURRENT_SESSION_PATH` (line 115), `LAST_OUTCOME` (line 135), with getters/setters at lines 141–207. (`AUTOMATION_RUNNING` at line 22 and `LIVE_SCORES` at line 47 stay as they are — see Decision Log.) Each new live feature has added another static; the GUI polls up to six functions per frame; nothing enforces that the fields describe the same moment.

Edits:

1. In `runner.rs`, define:

        /// Everything the GUI needs to display about the current/most recent
        /// run, updated together under one lock so a reader never sees a
        /// mixed state (e.g. the new run's iteration with the old run's
        /// session path).
        #[derive(Clone, Default)]
        pub struct Progress {
            pub current_iteration: u32,
            pub total_iterations: u32,
            pub state_desc: String,
            pub session_path: Option<PathBuf>,
            pub last_outcome: Option<AutomationOutcome>,
        }

        static PROGRESS: Mutex<Progress> = Mutex::new(/* Default is not const */);

   Since `Mutex::new` needs a const initializer and `String`/`Vec` consts are fine but `Default::default()` is not const, initialize with explicit empty values: `Mutex::new(Progress { current_iteration: 0, total_iterations: 0, state_desc: String::new(), session_path: None, last_outcome: None })`. If `Progress` fields make that awkward, use `OnceLock<Mutex<Progress>>` — but the explicit literal works because `String::new()` is const.

2. Replace the five statics and their accessor functions with: `pub fn get_progress() -> Progress` (lock, clone, return) and private setters used by the runner thread: `fn progress_update(f: impl FnOnce(&mut Progress))` as the single mutation path. All existing writers map onto it: `set_current_session_path` → `progress_update(|p| p.session_path = Some(...))`, the loop's `CURRENT_ITERATION.store(...)` + `update_state_description(...)` → one `progress_update` closure per loop step, `set_last_outcome`/`clear_last_outcome` likewise, and the run-start seeding at lines 334–336 becomes one closure that also resets `last_outcome` (replacing `clear_last_outcome`).

3. Keep `pub fn is_automation_running()` (atomic) unchanged. Delete `get_current_iteration`, `get_total_iterations`, `get_current_state_description`, `get_current_session_path`, `get_last_outcome`, and in `src/gui/mod.rs` change `update_automation_status` (around lines 362–380) and any handler reads (`handle_start` line 480, `handle_continue` line 514, `handle_extend` line 551, `handle_resume_selected` line 603) to call `get_progress()` once and read fields from the snapshot. Check `src/main.rs` for legacy-tray-mode callers of the deleted accessors and migrate them the same way (grep for each deleted name before deleting).

### M7 — `OcrRegions` parameter object (branch `feature/sustain-ocr-regions`)

The three parallel region arrays travel together everywhere: `AutomationConfig.score_regions` / `total_regions` / `bonus_regions` (`src/automation/config.rs` lines 158–172) are cloned individually in `runner.rs` (lines 395–397), passed as three parameters to `run_ocr_worker` (`src/automation/ocr_worker.rs:32`), passed again to `ocr_screenshot` (`src/ocr/mod.rs:51`, also called from `src/main.rs:596` and the `#[ignore]`d e2e test at `src/ocr/mod.rs:212`).

Edits (no serde change — see Decision Log):

1. In `src/automation/config.rs`, add:

        /// The three per-stage OCR crop-region arrays, bundled so callers
        /// pass one value instead of three parallel arrays. Built from (not
        /// replacing) the individually-serialized config fields.
        #[derive(Clone, Copy, Debug)]
        pub struct OcrRegions {
            pub score: [RelativeRect; 3],
            pub total: [RelativeRect; 3],
            pub bonus: [RelativeRect; 3],
        }

        impl AutomationConfig {
            pub fn ocr_regions(&self) -> OcrRegions { ... }
        }

   Re-export `OcrRegions` from `src/automation/mod.rs` alongside `RelativeRect`.

2. Change `ocr_screenshot(img, regions: &OcrRegions)` in `src/ocr/mod.rs` (internal indexing becomes `regions.score[stage]` etc.), `run_ocr_worker(receiver, csv_path, regions: OcrRegions)` in `ocr_worker.rs`, the spawn in `runner.rs::run_automation_loop` (one `let regions = config.ocr_regions();` replaces three clones), the call in `src/main.rs:596`, and the e2e test at `src/ocr/mod.rs:212`.

### M8 — `GameOps` seam + injected abort check; state-machine tests (branch `feature/sustain-game-seam`)

`src/automation/state.rs` interleaves the eleven-state sequencing logic with direct Windows calls: `IsWindow`/`SetForegroundWindow` (lines 14, 473–492), `click_at_relative` (SendInput, via `input.rs`), `capture_gakumas_to_buffer`, and the blocking waits `wait_for_start_page` / `wait_for_loading` / `wait_for_result` from `detection.rs` (which themselves poll `capture_region` and read the global `ABORT_REQUESTED` at detection.rs lines 275, 329, 430, 540). Consequence: none of the transition logic — the retry-on-first-iteration rule at state.rs:198, the abort-vs-error disambiguation repeated at lines 219, 273, 327, the completed-count semantics — can run under `cargo test`.

The seam: define, in `src/automation/state.rs` (or a new `src/automation/game_ops.rs` if `state.rs` grows unwieldy — implementer's choice, record it):

        /// Everything the state machine asks of the outside world. The
        /// production implementation talks to the real game window via
        /// Windows APIs; tests use a scripted fake. Wait methods block until
        /// the screen condition holds, returning Err on timeout or abort
        /// (exactly the current detection.rs contract).
        pub trait GameOps {
            fn is_window_valid(&self) -> bool;
            fn click_start(&mut self) -> Result<()>;
            fn click_skip(&mut self) -> Result<()>;
            fn click_end(&mut self) -> Result<()>;
            /// `retry_end_click`: on iterations after this run's first, the
            /// wait may re-click End if the page hasn't changed (the current
            /// ClickRetryInfo behaviour).
            fn wait_for_start_page(&mut self, retry_end_click: bool) -> Result<()>;
            fn wait_for_loading(&mut self) -> Result<()>;
            fn wait_for_result(&mut self) -> Result<()>;
            fn capture_result(&mut self) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>>;
            fn abort_requested(&self) -> bool;
        }

Plan of edits:

1. Create the production implementation `LiveGame` holding `hwnd`, the `AutomationConfig`, and the three pre-loaded button `ReferenceImage`s (the `load_ref_image` logic moves here from state.rs:445–470). Its methods contain today's code: `click_*` = `click_with_focus` with the right config coordinates; `wait_*` = the current `ClickRetryInfo` construction (state.rs lines 198–210, 256–264, 310–318) plus the `detection.rs` call; `capture_result` = `capture_gakumas_to_buffer(self.hwnd)`; `abort_requested` = `ABORT_REQUESTED.load(...)`.
2. Change `AutomationContext` to `AutomationContext<G: GameOps>` holding `ops: G` instead of `hwnd` + button refs (it keeps `config` only if still needed for non-ops values; aim to drop it). `step()` loses every direct Windows/API touch: the top-of-step abort check becomes `self.ops.abort_requested()`, window validity `self.ops.is_window_valid()`, and each state delegates to the corresponding ops method. The screenshot-save + OCR-queue logic in `Capturing` (filename with timestamp, `img.save`, `work_sender.send`) stays in `step()` — file I/O works in tests via a temp dir.
3. In `detection.rs`, replace the four direct `ABORT_REQUESTED.load(...)` polls with a passed-in check: add a parameter `abort: &dyn Fn() -> bool` to `wait_for_loading`, `wait_for_result`, `wait_for_start_page` and drop `use crate::automation::state::ABORT_REQUESTED` (line 20). `LiveGame` passes `&|| ABORT_REQUESTED.load(Ordering::SeqCst)`. This removes the state↔detection global coupling and makes the waits' abort behaviour testable in principle.
4. `runner.rs::run_automation_loop` builds `LiveGame` and `AutomationContext::new(ops, max_iterations, start_iteration, sender, screenshot_dir)`. Generic monomorphization keeps this zero-cost; no `Box<dyn>` needed since the runner names the concrete type.
5. Add `#[cfg(test)]` state-machine tests in `state.rs` with a `MockGame` whose method results are scripted (e.g. `VecDeque<Result<()>>` per method, plus a settable abort flag and a counter of clicks). Required test cases, all runnable via `GAKUMAS_NO_MANIFEST=1 cargo test`:
   - Happy path, 2 iterations from iteration 1: assert the observed ops-call sequence is wait_start_page(false), click_start, wait_loading, click_skip, wait_result, capture, click_end, then wait_start_page(true) for iteration 2, ...; final state `Complete`, `completed_iterations == 2`.
   - Resume path (start_iteration 3 of total 4): first `wait_for_start_page` gets `retry_end_click == false` (state.rs:198 rule uses `current_iteration > start_iteration`), and `completed_iterations` starts at 2 and ends at 4.
   - Abort during a wait: the wait returns Err while `abort_requested()` is true → final state `Aborted`, not `Error`.
   - Wait failure without abort → `Error(...)` containing the wait's message.
   - `is_window_valid` false at any step → `Error("Game window closed")`.
   These tests document today's behaviour; they must pass against the moved (not rewritten) logic.

### M9 — Extract `ReviewController` (branch `feature/sustain-review-controller`)

The review window (the editable table of a session's OCR rows with inline crops, verification ✓, and save) is spread across `GuiApp`: `handle_open_review` (mod.rs:709), `load_review_preview` (746), `handle_save_review` (779, which also regenerates charts and invalidates the live chart at 835), `render_review_window` (859), plus the rendering in `render.rs::render_review_window_contents`. Tracing one operation touches five locations in two files.

Edits:

1. Create `src/gui/review.rs` with `pub struct ReviewController { state: Option<ReviewState> }` (the `ReviewState` data bag stays in `src/gui/state.rs`). Methods, each the moved body of the corresponding `GuiApp` code:
   - `open(&mut self, session_dir: &Path) -> Result<()>` (from `handle_open_review`),
   - `load_preview(&mut self, ctx: &egui::Context, session_dir: &Path, iteration: u32)` (from `load_review_preview`),
   - `save(&mut self, session_dir: &Path) -> Result<SaveEffects>` (from `handle_save_review`) where `pub struct SaveEffects { pub scores_changed: bool }`,
   - `is_open(&self) -> bool`, `close(&mut self)`,
   - `show(&mut self, ctx: &egui::Context) -> ReviewActions` (from `render_review_window`; it renders the egui window by calling the existing `render.rs::render_review_window_contents` and returns the actions).
2. `GuiApp` keeps a `review: ReviewController` field and stays responsible for the cross-module *effects*: on `SaveEffects { scores_changed: true }` it regenerates charts, calls `runner::reload_live_scores_from_csv`, and `self.live_chart.invalidate()` — those touch subsystems the controller should not own. Action dispatch moves with the methods: `GuiApp::update` asks `self.review.show(ctx)` for actions and routes them into controller methods (mirroring the existing `PanelActions` pattern; this keeps render functions pure and avoids the E0502 borrow trap documented in CLAUDE.md — snapshot/clone before mutating).
3. `render.rs` is unchanged except imports. No behaviour change: same auto-save-on-verify, same filters/search, same crop expansion.

## Concrete Steps

All commands run from the repository root `C:\Work\GitRepos\gakumas-rehearsal-automation` in PowerShell unless noted.

1. Baseline (once, before M1):

        GAKUMAS_NO_MANIFEST=1 cargo test 2>&1 | tail -5

   (In PowerShell: `$env:GAKUMAS_NO_MANIFEST="1"; cargo test`.) Record the pass count in Progress. Expect ~116 passed, 0 failed, some `ignored`.

2. Per milestone: `git checkout -b <branch>` → edit → `cargo check 2>&1 | grep "^error"` (expect no output; ~30 warnings are normal) → run the milestone's tests (below) → commit in small steps (only affected files, no attribution) → after acceptance, merge to `main`.

3. Milestone test commands:
   - M1: full suite; then guarded build and a manual capture check (press Ctrl+Shift+S with the game open → a PNG appears under `screenshots/`; run one automation iteration → the result screenshot lands in the session folder and OCR produces scores).
   - M2: full suite (covers `parse_single_number` etc.), then the Tesseract e2e: `GAKUMAS_NO_MANIFEST=1 cargo test ocr_overlap_recovery_e2e -- --ignored` → expect `test result: ok`.
   - M3: full suite (`render_live_box_plot_*` tests); then `GAKUMAS_NO_MANIFEST=1 cargo test live_box_plot_preview -- --ignored` and visually compare `temp/live_box_plot_preview.png` with a pre-refactor copy (make the copy before starting M3).
   - M4: full suite; reconcile vectors must be untouched (`cargo test reconcile` — same count as baseline).
   - M5: `cargo check`, guarded build, manual: launch the app → the live panel shows the newest session's figure; untick/tick "ライブ分布を表示" → panel hides/shows and the window resizes; open review, verify a flagged row, save → figure refreshes.
   - M6: full suite; guarded build; manual: run a 3-iteration automation → progress bar counts 1/3→3/3 with Japanese state text updating; finished panel shows the completion message; "フォルダを開く" opens the right session.
   - M7: full suite; then `GAKUMAS_NO_MANIFEST=1 cargo test ocr_overlap_recovery_e2e -- --ignored` (its call site changed).
   - M8: full suite — the new state-machine tests must appear and pass (baseline count + ≥5); guarded build; manual: full 3-iteration run, then Ctrl+Shift+Q mid-run on another → finished panel says aborted with the correct completed count.
   - M9: `cargo check`, guarded build, manual: open review from the finished panel; expand a 📷 crop; edit a score and save → charts regenerate; ✓ a flagged row → auto-saves and the live figure re-includes it.

## Validation and Acceptance

The plan is complete when, on `main` with all milestones merged:

- `GAKUMAS_NO_MANIFEST=1 cargo test` passes with at least the baseline count plus the ≥5 new state-machine tests, and `GAKUMAS_NO_MANIFEST=1 cargo test -- --ignored` (Tesseract e2e + preview) passes.
- `powershell -ExecutionPolicy Bypass -File scripts/build.ps1` succeeds with no `^error` lines.
- Duplication is demonstrably gone: `grep -c "D3D11CreateDevice" src/capture/*.rs` returns 1 total; `grep -c "tessedit_create_tsv" src/ocr/engine.rs` returns 1; the nine-box drawing loop exists once in `charts.rs`; `grep -rn "iteration,timestamp,screenshot" src/automation/*.rs` outside `#[cfg(test)]` returns 1.
- A full manual click-through against the live game reproduces pre-refactor behaviour end-to-end: fresh 3-run series (progress, live figure, completion panel), abort mid-run, resume it, extend it by 2, open review, edit + verify + save, charts regenerate. Nothing in the on-disk session layout (`screenshots/`, `results.csv`, `rehearsal_data.csv`, `session.log`, `charts/`, `run-meta.json`) changes format.

## Idempotence and Recovery

Every milestone is an ordinary git branch; if a milestone goes wrong, `git checkout main` and delete the branch — no data, config, or on-disk format changes anywhere in this plan (M7 explicitly avoids touching serialization). Milestones are independently mergeable except: M5 before M9, M6 before M8 (both pairs touch the same files; the order minimizes rebasing). If a milestone is interrupted mid-way, the branch compiles or it doesn't — `cargo check` is the resume point; the Progress section must record partial completions as split entries ("done: X; remaining: Y").

## Artifacts and Notes

Baseline (2026-07-03): HEAD `28a22b6b99585b79ee8e678889b3e38fea28f800`; `GAKUMAS_NO_MANIFEST=1 cargo test` → `test result: ok. 115 passed; 0 failed; 2 ignored`. M3 visual reference saved as `temp/live_box_plot_preview_baseline_28a22b6.png`.

## Interfaces and Dependencies

No new crates. The signatures that must exist at the end (all named above in their milestones):

- `src/capture/screenshot.rs`: private `fn capture_and_crop(hwnd: HWND, crop: CropBox, verbose: bool) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>>`; the three public capture functions keep their exact current signatures.
- `src/ocr/engine.rs`: private `fn run_tesseract_tsv(img: &ImageBuffer<Luma<u8>, Vec<u8>>, tag: &str, psm: &str, whitelist: Option<&str>) -> Result<Vec<OcrLine>>`; public signatures unchanged.
- `src/analysis/charts.rs`: private `fn draw_nine_boxes<DB: DrawingBackend>(...)` + `struct BoxStrokes`; public signatures unchanged.
- `src/automation/csv_writer.rs`: `pub(crate) const CSV_HEADER: &str`.
- `src/ocr/reconcile.rs`: `const MAX_DIGIT_STREAM_LEN: usize = 21;`, `const MAX_PART_DIGITS: usize = 8;` (names only; expressions unchanged).
- `src/gui/live_chart.rs`: `pub struct LiveChartCache` with `new/update/invalidate/texture/stats/counts`.
- `src/automation/runner.rs`: `pub struct Progress`, `pub fn get_progress() -> Progress`; `is_automation_running`, `record_live_score`, `get_live_scores`, `live_score_count`, `reload_live_scores_from_csv`, `start_automation`, `resume_automation`, `extend_automation`, `request_abort` unchanged.
- `src/automation/config.rs`: `pub struct OcrRegions`, `impl AutomationConfig { pub fn ocr_regions(&self) -> OcrRegions }`; serialized fields unchanged.
- `src/ocr/mod.rs`: `pub fn ocr_screenshot(img: &ImageBuffer<Rgba<u8>, Vec<u8>>, regions: &OcrRegions) -> Result<StageReadout>`.
- `src/automation/state.rs` (or `game_ops.rs`): `pub trait GameOps` (nine methods as specified in M8), `pub struct LiveGame`, `pub struct AutomationContext<G: GameOps>` with `step(&mut self) -> Result<bool>` semantics unchanged.
- `src/automation/detection.rs`: `wait_for_loading` / `wait_for_result` / `wait_for_start_page` each gain a trailing `abort: &dyn Fn() -> bool` parameter; no other signature changes.
- `src/gui/review.rs`: `pub struct ReviewController` with `open/load_preview/save/show/is_open/close` and `pub struct SaveEffects { pub scores_changed: bool }`.
