# Recover per-character scores when two ≥1,000,000 values overlap in the rehearsal UI

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

This repository contains `docs/PLANS.md` (relative to the repository root). This document must be maintained in accordance with `docs/PLANS.md`.


## Purpose / Big Picture

The tool automates the game *gakumas*' "rehearsal" (リハーサル) feature: it clicks through many rehearsal runs and, on each result screen, reads nine numbers via OCR (Optical Character Recognition — turning pixels of text into digits). Those nine numbers are three "stages", each showing up to three per-character breakdown scores. They are written to a CSV file (`results.csv`) for later statistical analysis.

The game's result screen renders each stage's per-character scores as a single horizontal row, for example `339,709  155,256  105,212`. Each number uses a comma every three digits. A per-character score is at most `1,999,999` in practice (no character has ever reached two million), so a seven-digit score always looks like `1,XXX,XXX` and a six-digit score looks like `XXX,XXX`.

The bug: when two adjacent per-character scores are **both** at least 1,000,000, the game draws them so close that the leading "1" of the right-hand number visually collides with (overlaps) the last digit of the left-hand number. Exactly one digit column is shared. This produces two distinct OCR failures that currently corrupt or crash a run:

- Mode A (silent wrong value): the right number's leading "1," is dropped and a stray comma appears, so `1,161,196` is read as `161,196` — a value that is exactly 1,000,000 too low but still looks like a valid score, so it is recorded with no warning.
- Mode B (hard failure): the collision turns the junction into a clean comma, so `1,327,533` followed by `1,151,661` is read as one perfectly comma-grouped token `1,327,533,151,661`. That is thirteen digits, which overflows the 32-bit integer parser (`u32`, maximum 4,294,967,295), and the error aborts OCR for the entire iteration.

After this change, the tool will recover the true nine scores even when one or two such overlaps occur in a stage, by (1) splitting over-long tokens so they never overflow, and (2) using an arithmetic checksum that the screen already provides — the big stage total and the bonus badge — to reconstruct the dropped "1"s and the corrupted digits exactly. When recovery is not unambiguous, the iteration is flagged for human review rather than silently storing a guess.

You can see it working by running OCR on the four real failing screenshots checked in under `temp/failed_overlapped_samples/` and observing that the recovered scores equal the known-true values listed in `Artifacts and Notes`, and that the previously-correct sample is left unchanged.


## The key invariants this plan relies on

State these to yourself; the whole reconstruction depends on them. They are facts about the game's UI established by inspecting many real screenshots.

1. A per-character score is never 2,000,000 or larger. Therefore any seven-digit score is `1,XXX,XXX`; its leading digit is always exactly "1".
2. An overlap (and therefore any digit corruption) happens only between two side-by-side scores that are both ≥ 1,000,000.
3. When an overlap corrupts a junction, the damage is bounded to two things: the right number's leading "1" is lost, and the left number's **units digit** (its rightmost digit) may be misread. Every other digit of both numbers is intact. (The "1" glyph is one column wide and sits exactly over the left number's last column; the left number's other digits and the right number's second-digit-onward are untouched.)
4. The stage total and the bonus badge are each rendered as a single isolated number on their own line; they never overlap anything. For every stage, `stage_total = c1 + c2 + c3 + bonus`, where `c1,c2,c3` are the per-character breakdown scores (a missing/blank character contributes 0, shown on screen as a dash `ー`). This is an exact arithmetic identity, verified on all four sample screenshots.
5. The bonus badge appears underneath the character column that has the **largest** of the three scores, so its horizontal position is one of three possibilities. There is only ever one badge per stage. Its value is exactly twenty percent of that largest score, floored to an integer: `bonus = floor(max(c1,c2,c3) / 5)`. Verified on all four samples, including the case where the maximum is the middle column.
   The consequence is powerful: substitute the bonus identity into the total identity to get a checksum that needs **only the total** (the most reliably-OCR'd of the isolated numbers, since the bonus is the smaller, color-tinted, crown-prefixed one):

       stage_total = c1 + c2 + c3 + floor(max(c1, c2, c3) / 5)

   So the reconstruction does not depend on bonus OCR at all. The bonus, when it OCRs cleanly, is kept only as an independent cross-check (`floor(max(reconstructed) / 5)` must equal the OCR'd bonus); a wrong/over-detected bonus can therefore at most raise a review flag, never corrupt a stored value.
6. A dropped leading "1," can make the surviving token begin with a zero group, e.g. `1,062,741` becomes the token `062,741` (numeric value 62,741). A real score never displays a leading zero group, so a leading-zero group is a definitive "a leading 1 was lost here" marker.


## Progress

- [ ] M1 — Structural re-split so overlapping tokens never overflow `u32` and the previously-correct sample still parses. (Pure-function change in `src/ocr/extract.rs` + unit tests using the four real OCR strings.)
- [ ] M2 — Add OCR of the stage total and bonus badge: new config regions, a single-number OCR helper, and have `ocr_screenshot` return totals and bonuses alongside the nine scores. Validate on the four sample PNGs.
- [ ] M3 — Checksum reconstruction solver (pure function `reconcile_stage`). The checksum `total == c1+c2+c3+floor(max/5)` needs only the total; the bonus is an optional cross-check. Uses a corruption-aware cost to pick the right reconstruction. Unit-tested with all four real samples (fails before, passes after).
- [ ] M4 — Wire reconciliation into the live pipeline (`src/ocr/mod.rs`, `src/automation/ocr_worker.rs`), record a confidence flag, and log flagged iterations. End-to-end check on the samples.
- [ ] M5 (optional) — Bonus-column argmax cross-check and calibration-wizard support for the new regions.

Use timestamps when you check items off, e.g. `- [x] (2026-06-18 14:00Z) ...`.


## Surprises & Discoveries

- Observation: The "currently failing" sample `gakumas_20260618_102623.png` actually OCRs and tokenizes **correctly** today; its checksum matches with no change. It must be used as a regression guard so reconstruction does not "fix" a correct reading.
  Evidence: OCR line `912,1271,171,0241,004,816` tokenizes to `[912127, 1171024, 1004816]`; `912127+1171024+1004816+234204(bonus) = 3322171 = displayed total`.

- Observation: A dropped leading "1," can yield a five-digit numeric token with a leading-zero group, not a six-digit one. Victim detection must not assume "six digits".
  Evidence: Sample 005 OCR `1,083,344,062,741`; the second number `1,062,741` survives as `062,741` → numeric 62,741; `62741 + 1000000 = 1062741` (true value).

- Observation: The overlap corrupts the left number's units digit too, not only the right number's leading "1".
  Evidence: Sample 003 true left value is `1,327,533` but OCR read `…,534`; sample 005 true `…,349` but OCR read `…,344`; sample 102842 true `…,665` but OCR read `…,669`.

- Observation: The bonus badge is light blue and is preceded by a gold crown icon and a "+"; a plain luminance threshold cannot separate the crown from the digits, but a blue-selective mask can. The "+" is a reliable anchor for discarding crown noise.
  Evidence: Sampled pixels in a bonus crop — digits avg RGB (115,201,253), crown avg RGB (201,139,97). With the mask "blue ≥ 190 AND blue−red ≥ 30" plus whitelist `0123456789+` and taking digits after the last `+`, all bonuses read cleanly across the four samples: 003 → +81571/+265506/+41251; 005 stage 2 → +216669; 102623 stage 2 → +234204; 102842 stage 2 → +234533. The pre-fix grayscale crops read leading crown garbage like `5481571` and `4265506`.

- Observation: blue-min must be 190, not 150. The character portrait icons contain a dimmer blue; at blue-min 150 it leaked extra digits into the bonus read.
  Evidence: Sample 102842 stage 2 read `23545335` at blue-min 150 but the correct `234533` at blue-min 190. This is why `bonus_blue_min` defaults to 190 in config.json and the tools.

- Observation: the bonus is not an independent number — it is exactly `floor(max(c1,c2,c3) / 5)` (0.2× the largest score, floored). This collapses the two-number checksum (`total = sum + bonus`) into a one-number checksum (`total = sum + floor(max/5)`) that needs only the total, so reconstruction no longer depends on bonus OCR at all.
  Evidence: 003 floor(1327533/5)=265506; 005 floor(1083349/5)=216669; 102842 floor(1172665/5)=234533; 102623 floor(1171024/5)=234204 (max is the middle column here, and the bonus matches it — confirming it tracks the max, not a fixed column). All equal the displayed bonuses.

- Observation: a plain edit-count cost ties on real data, so the solver needs a corruption-aware (asymmetric) cost. A unit can be traded between two slots without changing the sum or `floor(max/5)`.
  Evidence: For sample 003 (OCR `[1327534,151661,0]`, total 2,744,700), both `[1327533,1151661,0]` (correct) and `[1327534,1151660,0]` (wrong) satisfy the checksum at plain cost 2. Charging more to edit the units of a +1M-restored slot than its left neighbour (per invariant 3) breaks the tie toward the correct answer.


## Decision Log

- Decision: Reconstruct using an exhaustive small-search "checksum solver" rather than ad-hoc heuristics.
  Rationale: Per stage there are at most three slots; per slot at most ~21 candidate values (the OCR value, optionally plus 1,000,000 to restore a lost leading "1", each with its units digit replaced by 0–9). Brute-forcing all ≤ 21³ ≈ 9,261 combinations and keeping those that satisfy the checksum `c1+c2+c3+floor(max/5) == total`, then choosing the one with the lowest corruption-aware cost, handles zero, one, and two simultaneous overlaps uniformly and is trivial to reason about and test. Ad-hoc per-junction heuristics were rejected because the two-overlap case (all three scores ≥ 1,000,000) leaves two unknown units digits that a single checksum equation cannot pin without also weighing the OCR prior — which the cost function does naturally. (This entry predates the discovery that the checksum needs only the total; see the later decisions on the `floor(max/5)` identity and the corruption-aware cost.)
  Date/Author: 2026-06-18, design phase.

- Decision: Degrade gracefully across three tiers (structural-only, total-only, total+bonus) instead of hard-requiring the new OCR regions.
  Rationale: The structural re-split alone must always run because it removes a current crash. If only the total OCRs (bonus is smaller/noisier), the ±1,000,000 errors are still unambiguously fixable because a real bonus is far smaller than 1,000,000. Full total+bonus additionally fixes the small units-digit corruption exactly.
  Date/Author: 2026-06-18, design phase.

- Decision: Preprocess the bonus badge with a blue-selective color mask (not a luminance threshold) and parse it by taking the digits after the last "+".
  Rationale: The bonus value is light blue, sits to the right of a gold crown icon, and appears under whichever of the three columns has the largest score, so its horizontal position and width vary. A luminance threshold captures the crown (and its leading garbage digit) and depends on precise left-padding; a blue mask ("blue ≥ 150 AND blue−red ≥ 30") drops the gold crown and white while keeping the blue glyphs, making a full-width three-column crop safe. The "+" the badge always renders is a reliable anchor: whatever the crown reads as lands before it, so the value is the digits after the last "+". Verified to read all four samples cleanly.
  Date/Author: 2026-06-18, calibration phase (validated via scripts/region_tuner.py).

- Decision: Make the checksum `total = c1 + c2 + c3 + floor(max(c1,c2,c3) / 5)` (total only); demote the bonus to an optional cross-check.
  Rationale: The bonus is exactly `floor(max/5)` (invariant 5), so it is redundant with the scores given the total. Driving reconstruction from the total alone removes the dependency on the smaller, color-tinted, crown-prefixed bonus badge — the region most prone to over-detection. The bonus, when it OCRs cleanly, still corroborates (`floor(max/5)` must match it) and helps break ties in the rare two-overlap case, but a wrong bonus can now at most flag a stage, never corrupt one.
  Date/Author: 2026-06-18, design phase (user supplied the 0.2×max relationship).

- Decision: Use a corruption-aware (asymmetric) cost in the solver rather than a plain edit count.
  Rationale: The checksum plus candidate set admits unit-trade ties (e.g. sample 003 ties the correct `[1327533,1151661,0]` with the wrong `[1327534,1151660,0]` at equal plain cost). Invariant 3 says only the LEFT neighbour of a restored (right-operand) slot has a corrupted units digit, so charging `+1` for a units edit on that left neighbour but `+3` elsewhere encodes the physics of the overlap and resolves the tie to the correct answer — recovering exact units from the total alone.
  Date/Author: 2026-06-18, design phase (found while validating the total-only checksum).

- Decision: Validate total/bonus for plausibility before trusting them, and treat the exact-sum requirement as the final guard against bad checksum inputs.
  Rationale: OCR over-detects, not just fails — the bonus blue mask once read `23545335` and a total read `27447007` (both 8 digits of garbage). If the total were trusted, the checksum `total = c1+c2+c3+floor(max/5)` would be wrong and could drive a wrong reconstruction. So `recognize_single_number` drops values with an impossible digit count, `reconcile_stage` step 0 applies contextual checks (total ≤ 7 digits and ≥ the largest raw score; bonus ≤ 6 digits and `< total`), and — crucially — the solver only accepts a combination that satisfies the checksum exactly. A subtly-wrong total that slips past the range checks simply yields no satisfying combination, so the result degrades to a flagged best-effort read rather than a silently-corrupted value. Because the bonus is only a cross-check (it equals `floor(max/5)`), a bad bonus can at most raise a flag. Layered guards: cheap digit-count → contextual ranges → exact-checksum necessity.
  Date/Author: 2026-06-18, design phase (prompted by observed bonus/total over-detection).

- Decision: When the solver finds zero exact-sum solutions, or more than one equally-cheap solution, record the scores as best-effort and set a `recovery` flag column to a value other than `ok`, and log a warning. Do not silently store a guess.
  Rationale: For statistical data, a handful of clearly-flagged rows is honest and reviewable; a silently wrong row poisons the analysis.
  Date/Author: 2026-06-18, design phase.


## Outcomes & Retrospective

To be completed as milestones land. At each milestone, summarize what now works that did not before, measured against `Purpose / Big Picture`.


## Context and Orientation

You need to know nothing about prior work. Here is the relevant code.

The application is a Windows system-tray screenshot/automation tool written in Rust. It captures the game window, clicks through rehearsals, and OCRs the result screen. OCR uses an embedded copy of Tesseract (an open-source OCR engine) shipped as `target/release/tesseract/tesseract.exe` with English data under `target/release/tesseract/tessdata/`.

Key files, by full path:

- `src/ocr/mod.rs` — `ocr_screenshot(image, score_regions) -> Result<[[u32;3];3]>` crops each stage's score row out of the screenshot, thresholds it (converts to black/white), OCRs that one row, and calls into `extract.rs` to turn OCR text into three numbers. It loops over the three stages.
- `src/ocr/extract.rs` — the text-to-numbers logic. `extract_single_stage(lines) -> Result<[u32;3]>` re-tokenizes each OCR line with the regex constant `SCORE_TOKEN_PATTERN` (currently `\d{1,3}(?:[,.]\d{3})*|<dashes>`), parses each token with `parse_score`, drops values below 100 as noise, and maps the survivors left-to-right into three slots (missing slots padded with 0). This file also still contains an older multi-pass `extract_scores` kept for reference; do not remove it.
- `src/ocr/engine.rs` — runs the Tesseract executable. `recognize_image_line(img) -> Result<Vec<OcrLine>>` uses page-segmentation mode 6 ("a block of text") with no character whitelist, writes a TSV (tab-separated values) result, and parses it via `parse_tsv_output` into `OcrLine { text, words: Vec<OcrWord>, confidence }` where `OcrWord { text, confidence }`. The TSV that Tesseract emits also contains per-word bounding-box columns (`left`, `top`, `width`, `height`) and a per-word `conf` that `parse_tsv_output` currently discards.
- `src/automation/config.rs` — `AutomationConfig`, including `score_regions: [RelativeRect; 3]` (each `RelativeRect { x, y, width, height }` is in fractions of the image width/height, 0.0–1.0) with a `#[serde(default)]` default function. The on-disk `config.json` at the repository root mirrors these.
- `src/automation/csv_writer.rs` — `CSV_HEADER = "iteration,timestamp,screenshot,s1c1,s1c2,s1c3,s2c1,s2c2,s2c3,s3c1,s3c2,s3c3"`, and functions `append_to_csv` / `append_to_raw_csv` that write the nine scores.
- `src/automation/ocr_worker.rs` — background worker that calls `ocr_screenshot` for each captured screenshot and appends to CSV.
- `scripts/debug_ocr_regions.py` — a standalone Python tool (run with `uv run scripts/debug_ocr_regions.py <png> --config config.json`) that crops the configured regions, thresholds them, and OCRs them, writing images and printing OCR text to the `debug/` folder. Use it to eyeball crops and OCR output while calibrating regions.

Term definitions used below: "stage" = one of the three rehearsal stages (a row of up to three per-character scores). "slot" / "character column" = one of the three positions in a stage. "junction" = the boundary between two adjacent slots. "malignant junction" = a junction where two ≥ 1,000,000 scores overlapped and corrupted the reading. "victim" = a slot whose value was corrupted by a malignant junction. "threshold" = converting the grayscale crop to pure black/white by a brightness cutoff. "units digit" = the rightmost (ones) digit of a number.


## Plan of Work

The work proceeds in milestones M1–M4 (M5 optional). Each is independently verifiable.


### Milestone M1 — Structural re-split (stop the crash; keep the good case)

Goal: no OCR line can ever produce a token that overflows `u32`, and over-long comma runs are split into individual valid scores, while the already-correct sample is unaffected. After M1 the Mode B crash becomes, at worst, a wrong-but-bounded value (still to be corrected in M3).

Edit `src/ocr/extract.rs`. Replace the body of `SCORE_TOKEN_PATTERN` so each match is capped to a single legal score shape instead of greedily swallowing an unbounded run of comma groups. A legal score token is one of: a millions score `1,XXX,XXX`; or a sub-million score of one optional comma group `XXX,XXX` (also matches plain `\d{1,3}`); or a run of dash characters. Concretely set:

    const SCORE_TOKEN_PATTERN: &str =
        r"1[,.]\d{3}[,.]\d{3}|\d{1,3}(?:[,.]\d{3})?|[\-\u{2014}\u{2013}\u{2015}\u{2500}\u{30FC}\u{4E00}]+";

Why this works: `Regex::find_iter` scans left to right and, at each position, tries the alternatives in order and takes the leftmost match. On `1,327,534,151,661` the first alternative matches `1,327,534` (eight characters), then scanning resumes after it, skips the comma, and the second alternative matches `151,661`. On `912,1271,171,0241,004,816` the first alternative cannot match at `912` (it does not start `1,`), so the second alternative matches `912,127`; then `1,171,024` and `1,004,816` match the first alternative. Thus over-long runs split and the good sample is unchanged. Because every token now has at most seven digits, `parse_score` can never overflow `u32`.

Keep `parse_score`, `is_dash_char`, `is_dash_like`, and the old `extract_scores` exactly as they are.

Add unit tests in the same file's `#[cfg(test)] mod tests` that feed the **real OCR line strings** (from `Artifacts and Notes`) through `extract_single_stage` and assert the post-split token values. Note these assertions are about *splitting*, not yet about reconstruction, so the expected values here are the raw split values (e.g. sample 003 → `[1327534, 151661, 0]`), which M3 will later correct. Crucially add a test that `extract_single_stage` returns `Ok` (does not error) for the sample-003 and sample-005 strings, proving the overflow crash is gone.

Run `cargo test --lib ocr::extract` (or `cargo test extract`) and expect the new tests to pass and all existing `extract.rs` tests to still pass.


### Milestone M2 — OCR the stage total and the bonus badge

Goal: `ocr_screenshot` additionally returns, for each stage, the OCR'd stage total and bonus. The **total** is M3's checksum input (`total = c1+c2+c3+floor(max/5)`, invariant 5); the **bonus** is an optional cross-check only (it equals `floor(max/5)`, so it is redundant with the scores given the total). Both numbers are isolated and never overlap, so they OCR cleanly with a digit whitelist; we still read the bonus because, when clean, it corroborates the reconstruction and helps disambiguate the rare two-overlap case, but recovery never depends on it.

In `src/automation/config.rs` add two new fields to `AutomationConfig`, each `#[serde(default = "...")]`:

    pub total_regions: [RelativeRect; 3],
    pub bonus_regions: [RelativeRect; 3],

Provide default functions. Starting estimates, measured on the 721×1280-class sample screenshots (these are normalized fractions; refine by calibration — see Concrete Steps). The existing `score_regions` defaults are stage tops at y = 0.179, 0.430, 0.685 with height 0.022, full width. The total sits ~0.041 above each score row; the bonus sits ~0.018 below it:

    total_regions (full width, height ~0.026): y = 0.138, 0.389, 0.644
    bonus_regions (spans the three character columns, height ~0.022): y = 0.197, 0.448, 0.703

For `total_regions`, full width is fine: the total is centered and a digit whitelist drops the trailing "Pt". For `bonus_regions`, the badge can be under any of the three columns, so make the crop span all three columns horizontally (for example `x = 0.27, width = 0.51`, covering roughly the icon row) so the single badge is captured wherever it is; the gold crown icon is removed by the blue-selective mask (see the preprocessing note below) and any leading crown noise is discarded by the `+`-anchored parsing. Mirror both new arrays into the repository-root `config.json` so users can recalibrate.

In `src/ocr/engine.rs` add a helper that OCRs a pre-binarized crop as a single isolated integer, distinct from `recognize_image_line`:

    pub fn recognize_single_number(img: &ImageBuffer<Luma<u8>, Vec<u8>>, whitelist: &str, anchor_plus: bool) -> Result<Option<u32>>

It must invoke Tesseract with page-segmentation mode 7 ("treat the image as a single text line") and `-c tessedit_char_whitelist=<whitelist>`. For the total pass `whitelist = "0123456789,"` and `anchor_plus = false`. For the bonus pass `whitelist = "0123456789+"` and `anchor_plus = true`. When `anchor_plus` is true, take only the digits after the **last** `+` in the OCR text (the bonus badge always renders a "+" immediately before the number; whatever the crown icon reads as lands before that "+"), otherwise take the longest run of digits/commas. Strip non-digits, parse to `u32`, and return `Ok(None)` if nothing parseable is found (so a failed total/bonus simply disables the checksum tier rather than crashing). As a first-line over-detection guard, also return `Ok(None)` when the parsed value has an obviously-wrong digit count for its kind — more than 7 digits for a total, more than 6 for a bonus — so the worst garbage never reaches the checksum (this is the cheap guard; `reconcile_stage` step 0 does the contextual range/cross-checks). Reuse the existing helpers `find_tesseract_executable` and `find_tessdata_dir` and the `CREATE_NO_WINDOW` flag pattern already in that file.

Preprocessing differs by region and must happen in the caller (`ocr_screenshot`), which has the full-color RGBA image. The **total** is white text, so binarize it like score rows: grayscale luminance ≥ threshold. The **bonus** value is rendered in **light blue** (measured ≈ RGB (115,201,253)) and is preceded by a **gold crown** icon (≈ RGB (201,139,97)) and a "+". A plain luminance threshold cannot tell the gold crown from the blue digits and is sensitive to which of the three columns the badge sits under. Instead binarize the bonus crop with a **blue-selective mask**: a pixel is "on" when its blue channel ≥ `bonus_blue_min` (default 190) AND (blue − red) ≥ `bonus_br_margin` (default 30). This keeps the light-blue glyphs and drops the gold crown and any white, so the crop can safely span all three character columns and need not be precisely left-padded. Note the blue-min default is 190, not 150: the character portrait icons contain a dimmer blue that at 150 leaks extra digits into the bonus read (it corrupted sample 102842's stage-2 bonus to `23545335`; at 190 it reads the correct `234533`). Add a small helper for this (in `src/ocr/preprocess.rs`), e.g. `fn blue_mask(rgba_crop, bmin: u8, margin: u8) -> ImageBuffer<Luma<u8>, Vec<u8>>`.

The three preprocessing knobs live in `config.json` (the master) as scalar fields with `#[serde(default = ...)]`: `total_threshold` (default 190), `bonus_blue_min` (default 190), `bonus_br_margin` (default 30). Add them to `AutomationConfig` alongside the region arrays. These defaults and the blue/gold pixel measurements were validated on all four sample screenshots by the calibration tools `scripts/region_tuner.py` (the interactive browser GUI) and `scripts/debug_total_bonus.py`, both of which read the regions and these knobs from `config.json`.

In `src/ocr/mod.rs` widen `ocr_screenshot` to also crop `total_regions[i]` and `bonus_regions[i]`, threshold them with the same threshold used for score rows, OCR them with `recognize_single_number`, and return the extra data. Change its signature to:

    pub fn ocr_screenshot(
        image: &ImageBuffer<Rgba<u8>, Vec<u8>>,
        score_regions: &[RelativeRect; 3],
        total_regions: &[RelativeRect; 3],
        bonus_regions: &[RelativeRect; 3],
    ) -> Result<StageReadout>

where `StageReadout` is a new struct (define it in `src/ocr/mod.rs` or `extract.rs`) holding the per-stage OCR scores and optional totals/bonuses, e.g.:

    pub struct StageReadout {
        pub scores: [[u32; 3]; 3],
        pub totals: [Option<u32>; 3],
        pub bonuses: [Option<u32>; 3],
        pub flags: [Recovery; 3], // filled in M3; default Recovery::Ok
    }

Update the one or two existing callers (in `src/automation/ocr_worker.rs`, and any test/debug caller) to pass the new region arrays from config and to read `.scores` where they previously used the `[[u32;3];3]` return. Keeping `scores` as a field means the rest of the pipeline keeps working before M3/M4 wire in reconciliation.

Validation for M2 is by running the actual OCR on the sample PNGs (Tesseract requires the executable, so this is not a pure `cargo test`). Use the debug Python tool to confirm the new regions land on the right pixels and that totals/bonuses read as the expected values in `Artifacts and Notes`. Record the exact normalized region values you settled on in this plan's `Concrete Steps` once calibrated.


### Milestone M3 — The checksum reconstruction solver (the heart; fully unit-testable)

Goal: a pure function that takes one stage's OCR scores plus its optional total and bonus and returns the corrected three scores and a confidence flag. This is offline-testable with `cargo test` using the four real samples; no Tesseract needed.

In `src/ocr/extract.rs` (or a new `src/ocr/reconcile.rs` re-exported from `mod.rs`) define:

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum Recovery { Ok, Repaired, Flagged }

    pub fn reconcile_stage(
        ocr_scores: [u32; 3],
        total: Option<u32>,
        bonus: Option<u32>,
    ) -> ([u32; 3], Recovery)

Algorithm (exhaustive small search, justified in the Decision Log). The checksum is the single identity from invariant 5 — `total == c1 + c2 + c3 + (max(c1,c2,c3) / 5)` using integer (flooring) division — so it needs only the `total`; the `bonus`, when present and valid, is an independent cross-check, never a required input.

0. Validate inputs before trusting them, because OCR over-detects plausible-looking garbage, not just outright failures (the bonus blue mask once read `23545335`, a total read `27447007` — both 8 digits). Demote `total` to `None` when it has more than 7 digits (> 9,999,999) or is below the largest single raw OCR score. Demote `bonus` to `None` when it has more than 6 digits (>= 1,000,000) or is `>= total`. A demoted value is simply not used; never reconstruct from a number you do not believe.

1. If `total` is `Some`, run the checksum solver (steps 2–5). Otherwise skip to the structural-only fallback (step 6).

2. Build, for each slot `i`, a candidate set `cand[i]`:
   - Always include the raw `ocr_scores[i]`.
   - If `ocr_scores[i] >= 1000` and `< 1_000_000`, also include `ocr_scores[i] + 1_000_000` (restores a dropped leading "1"; covers leading-zero-group victims like 62,741 → 1,062,741).
   - For each base value `>= 100_000`, add the ten variants formed by replacing its units digit with 0..=9 (covers the corrupted left-units digit). A dash slot (0) contributes only `{0}`.
   - De-duplicate.

3. Iterate over every combination `(a, b, c)` in `cand[0] × cand[1] × cand[2]` (≤ ~9,261) and keep those satisfying the checksum `a + b + c + (max(a, b, c) / 5) == total` (integer division floors, matching invariant 5).

4. Score each kept combination with a **corruption-aware** cost and pick the minimum. The cost is deliberately NOT a plain edit count — a plain count ties on real data. Per invariant 3, an overlap restores a leading million on the RIGHT operand of a junction and corrupts only the units digit of its LEFT neighbour, so:
   - `+1` for each slot given a leading +1,000,000 (the "restored" slots; each marks a junction whose right operand it is).
   - a units-digit change costs `+1` only when that slot is immediately to the LEFT of a restored slot (the expected victim); any other units-digit change costs `+3`.
   Then minimum cost 0 → return with `Recovery::Ok`; a unique minimum cost > 0 → `Recovery::Repaired`. If the `bonus` survived step 0, use it to break ties and to corroborate: prefer combinations whose `max(a,b,c) / 5 == bonus`, and if the chosen combination's derived bonus disagrees with the OCR'd bonus, downgrade to `Recovery::Flagged`.
   Worked tie example (sample 003): OCR `[1327534, 151661, 0]`, total 2,744,700. Both `[1327533, 1151661, 0]` (correct) and `[1327534, 1151660, 0]` (wrong) satisfy the checksum at plain-edit cost 2, because one unit can be traded between the two numbers without changing the sum or `max/5`. Slot 1 is the restored slot, so the asymmetric cost charges 1 to edit slot 0's units (its left neighbour) but 3 to edit slot 1's own units: the correct combination wins, 2 vs 4. This recovers the exact units digit from the total alone — the bonus is only confirmatory.

5. If the solver found zero checksum-satisfying combinations (the total was subtly wrong despite passing step 0) or the minimum cost remains tied after the bonus tie-break, do not emit an arbitrary guess: return the closest best-effort combination with `Recovery::Flagged` so a human reviews it. The exact-checksum requirement is the final guard — a wrong total degrades to a flagged read, never a silently-corrupted value.

6. Structural-only fallback (`total` is `None`): promote any slot whose token began with a leading-zero group or came from splitting an over-long comma run (definitive lost-million markers, invariant 6); leave units digits as OCR'd (this tier cannot fix units corruption). Return `Recovery::Repaired` if any promotion happened, else `Recovery::Ok`, and `Recovery::Flagged` if the structural signals are ambiguous.

Unit tests (pure, run under `cargo test`). The four real samples assert exact recovery; pass the bonus too so the cross-check path is exercised. Use the numbers from `Artifacts and Notes`:

- 102842: `reconcile_stage([1172669,161196,1093518], Some(3661912), Some(234533))` → `([1172665,1161196,1093518], Repaired)`.
- 003: `reconcile_stage([1327534,151661,0], Some(2744700), Some(265506))` → `([1327533,1151661,0], Repaired)`.
- 005: `reconcile_stage([1083344,62741,0], Some(2362759), Some(216669))` → `([1083349,1062741,0], Repaired)`.
- 102623 (regression guard): `reconcile_stage([912127,1171024,1004816], Some(3322171), Some(234204))` → `([912127,1171024,1004816], Ok)`.

Total-only must still recover exactly, since the checksum needs only the total: assert `reconcile_stage([1327534,151661,0], Some(2744700), None)` → `([1327533,1151661,0], Repaired)`. This proves the asymmetric cost from step 4 breaks the unit-trade tie without the bonus.

Cost-model tie test: assert a plain equal-weight edit count would tie `[1327534,1151660,0]` with the correct answer, and that the implemented asymmetric cost returns `[1327533,1151661,0]`.

Over-detection / guard tests (a wrong-but-plausible total/bonus must never produce a silently-wrong stored value):

- Over-detected bonus is ignored, recovery still correct: `reconcile_stage([1327534,151661,0], Some(2744700), Some(23545335))` — the bonus has 8 digits so step 0 demotes it to `None`; the total checksum alone still yields `[1327533,1151661,0]`. Assert the values are correct (the bad bonus did not corrupt them).
- Over-detected total is rejected: `reconcile_stage([1327534,151661,0], Some(27447007), Some(265506))` — the total has 8 digits so step 0 demotes it to `None`; with no usable total the function drops to structural-only, restores the million from the leading-zero/over-split signal, and returns `Recovery::Repaired` or `Flagged`. Assert it does not crash and returns no checksum-derived (units-edited) value.
- Subtly-wrong total yields no solution: `reconcile_stage([1327534,151661,0], Some(2744701), None)` (total off by 1) — no combination satisfies the checksum, so the result is a flagged best-effort read. Assert `Recovery::Flagged`.

These tests must fail before M3 (the function does not exist / is unimplemented) and pass after.


### Milestone M4 — Wire reconciliation into the live pipeline and CSV

Goal: real runs use the recovered values and flag ambiguous ones.

In `src/ocr/mod.rs`, after collecting `scores`, `totals`, `bonuses` in `ocr_screenshot`, call `reconcile_stage` per stage, write the corrected scores back into `StageReadout::scores`, and store each stage's `Recovery` into `StageReadout::flags`.

In `src/automation/ocr_worker.rs`, use the reconciled `.scores` when appending to CSV. When any stage's flag is `Flagged`, log a warning naming the iteration and screenshot so a human can review; when `Repaired`, log an info line recording the before/after so corrections are auditable.

In `src/automation/csv_writer.rs`, optionally extend the schema so corrections are visible in the data. The least-disruptive approach is to append three columns: `recovery` (one of `ok`/`repaired`/`flagged`, the worst of the three stages), and the two checksum aids `s1_total,s2_total,s3_total` are not required — but at minimum add a single `recovery` column. If you change `CSV_HEADER`, ensure `init_csv`'s "preserve existing file" behavior is not broken for older files (older files simply lack the column; document that). If you prefer zero schema churn, instead write flagged iterations to a sidecar `recovery.log` in the session folder and keep the CSV identical; record whichever you choose in the Decision Log.

Validation: run the application against the four sample screenshots. Because this is an admin-manifest tray app that cannot be driven from `cargo test`, exercise it the way the rest of the project does — either via a small debug entry point that feeds a PNG through `ocr_screenshot` and prints the result, or by replaying the samples through the worker — and confirm the printed nine scores for each sample equal the true values in `Artifacts and Notes`, with sample 102623 unchanged and `recovery=ok`, and samples 003/005/102842 showing `recovery=repaired` with corrected values.


### Milestone M5 — Optional hardening

Bonus-column cross-check: since the bonus badge sits under the largest-scoring slot (invariant 5), after reconciliation assert that the slot index of the maximum reconstructed score matches the column the badge was detected in (requires per-column bonus detection rather than the single wide-row crop). A mismatch is a strong signal to flag. Implement only if M2's wide-row bonus crop proves unreliable.

Calibration wizard: extend `src/calibration/` so the interactive wizard can capture `total_regions` and `bonus_regions` like it already does for `score_regions`, so users on differently-sized windows can recalibrate without editing JSON by hand.


## Concrete Steps

Work from the repository root `C:\Work\GitRepos\gakumas-screenshot` in PowerShell unless noted. The Bash tool is also available for POSIX one-liners.

Build (expect ~30 pre-existing warnings; only `^error` lines matter):

    cargo build --release

Run the focused unit tests as you implement M1 and M3:

    cargo test extract
    cargo test reconcile

Eyeball OCR crops and the new total/bonus regions while calibrating M2 (writes images + prints OCR text under `debug/`):

    uv run scripts/debug_ocr_regions.py temp/failed_overlapped_samples/003_20260618_101738.png --config config.json

To measure/confirm the total and bonus crops independently of the Rust code, you can OCR a band directly with the embedded Tesseract (PowerShell example; adjust the crop in a scratch Python/Pillow snippet as in the design investigation). Expected reads for sample 003 stage 2 are total `2,744,700` and bonus `265506`.

As you finalize the normalized region rectangles, record the exact values you committed here so the plan stays self-contained:

    score_regions:  (unchanged) y = 0.179 / 0.430 / 0.685, h = 0.022, full width
    total_regions:  y = <fill in> , h = <fill in> , x/width = <fill in>
    bonus_regions:  y = <fill in> , h = <fill in> , x = <fill in>, width = <fill in>


## Validation and Acceptance

Acceptance is observable behavior, not code shape.

- M1: `cargo test extract` passes, including a new test proving `extract_single_stage` returns `Ok` for the sample-003 and sample-005 OCR strings (no `u32` overflow), and that the sample-102623 string still tokenizes to `[912127, 1171024, 1004816]`.
- M3: `cargo test reconcile` passes the four sample cases listed in M3, the total-only tier test, and the conflicting/`Flagged` test. Each new test demonstrably fails before the function is implemented and passes after.
- M2/M4 (end-to-end, manual since Tesseract + admin manifest preclude `cargo test`): feeding each of the four PNGs in `temp/failed_overlapped_samples/` through `ocr_screenshot` yields these nine stage-2 values and flags:
  - `003_20260618_101738.png` → stage 2 = `1,327,533 / 1,151,661 / 0`, recovery = repaired.
  - `005_20260618_101804.png` → stage 2 = `1,083,349 / 1,062,741 / 0`, recovery = repaired.
  - `gakumas_20260618_102842.png` → stage 2 = `1,172,665 / 1,161,196 / 1,093,518`, recovery = repaired.
  - `gakumas_20260618_102623.png` → stage 2 = `912,127 / 1,171,024 / 1,004,816`, recovery = ok (unchanged).
  In every case the reconstructed scores satisfy `c1+c2+c3+floor(max/5) == OCR'd total` for that stage, and `floor(max/5)` equals the OCR'd bonus (the cross-check).


## Idempotence and Recovery

All steps are safe to re-run. `cargo build` / `cargo test` are idempotent. Editing `config.json` and the config defaults is additive; if a user's older `config.json` lacks `total_regions`/`bonus_regions`, the `#[serde(default = ...)]` attributes supply them, so old configs keep loading. If you change `CSV_HEADER`, older `results.csv` files remain readable (they simply lack the new column); never rewrite existing rows. If a calibration value is wrong, the worst outcome is that the checksum tier disables itself (total/bonus read as `None`) and the pipeline falls back to structural-only behavior — still no crash, because M1 guarantees no overflow. Keep the `debug/` and `temp/` scratch outputs out of commits.


## Artifacts and Notes

The four real samples and their ground truth (all values are for **stage 2**, the row that overlaps; stages 1 and 3 in these screenshots are single-character and read cleanly). "OCR tokens (post-M1 split)" are what `extract_single_stage` produces after the M1 regex change and are the inputs to `reconcile_stage`.

Sample `gakumas_20260618_102623.png` (regression guard — already correct):

    OCR line:                912,1271,171,0241,004,816
    OCR tokens (post-M1):    [912127, 1171024, 1004816]
    total / bonus:           3,322,171 / 234,204
    true scores:             912,127 / 1,171,024 / 1,004,816
    bonus = floor(max/5): floor(1171024/5) = 234204 (max is c2, the middle column)
    check (total-only): 912127+1171024+1004816+floor(1171024/5) = 3322171  ✓ (no overlap; reconcile = Ok)

Sample `gakumas_20260618_102842.png` (one malignant junction; all three ≥ 1M):

    OCR line:                1,172,669,,161,1961,093,518
    OCR tokens (post-M1):    [1172669, 161196, 1093518]
    total / bonus:           3,661,912 / 234,533
    true scores:             1,172,665 / 1,161,196 / 1,093,518
    notes: c1 units 5 misread as 9; c2 lost leading 1; c2|c3 junction was benign.
    bonus = floor(max/5): floor(1172665/5) = 234533 (max is c1)
    check (total-only): 1172665+1161196+1093518+floor(1172665/5) = 3661912  ✓

Sample `003_20260618_101738.png` (Mode B overflow; third slot is a dash):

    OCR line:                1,327,534,151,661
    OCR tokens (post-M1):    [1327534, 151661, 0]
    total / bonus:           2,744,700 / 265,506
    true scores:             1,327,533 / 1,151,661 / 0
    notes: read as one 13-digit token pre-M1 (overflow). c1 units 3 misread as 4; c2 lost leading 1.
    bonus = floor(max/5): floor(1327533/5) = 265506 (max is c1)
    check (total-only): 1327533+1151661+0+floor(1327533/5) = 2744700  ✓

Sample `005_20260618_101804.png` (Mode B overflow; leading-zero-group victim; third slot dash):

    OCR line:                1,083,344,062,741
    OCR tokens (post-M1):    [1083344, 62741, 0]
    total / bonus:           2,362,759 / 216,669
    true scores:             1,083,349 / 1,062,741 / 0
    notes: c1 units 9 misread as 4; c2 1,062,741 survived as 062,741 → numeric 62741 (leading-zero marker).
    bonus = floor(max/5): floor(1083349/5) = 216669 (max is c1)
    check (total-only): 1083349+1062741+0+floor(1083349/5) = 2362759  ✓

Worked example of the solver on sample 003 — total-only checksum `a + b + c + floor(max/5) == 2,744,700`, no bonus needed:

    cand[0] from 1,327,534 (≥1M): units variants 1,327,530..1,327,539
    cand[1] from 151,661 (<1M):   {151,661, 1,151,661} each with units variants ...,660..669
    cand[2] from 0 (dash):        {0}
    two combinations satisfy the checksum, both at plain-edit cost 2:
        A: 1,327,533 + 1,151,661 + 0 ; max 1,327,533, floor/5 = 265,506 ; total 2,744,700  (correct)
        B: 1,327,534 + 1,151,660 + 0 ; max 1,327,534, floor/5 = 265,506 ; total 2,744,700  (wrong)
    corruption-aware cost breaks the tie: slot 1 is the +1M-restored slot.
        A edits slot 0's units (left neighbour of the restored slot) → cost 1 (restore) + 1 (units) = 2
        B edits slot 1's own units (the restored slot) → cost 1 (restore) + 3 (units) = 4
    → ([1327533, 1151661, 0], Repaired)


## Interfaces and Dependencies

Use the embedded Tesseract already in the tree (`target/release/tesseract/`). No new crates are required; reuse `regex`, `image`, and `anyhow` already in `Cargo.toml`.

End-state signatures that must exist:

In `src/ocr/engine.rs`:

    pub fn recognize_single_number(img: &ImageBuffer<Luma<u8>, Vec<u8>>, whitelist: &str, anchor_plus: bool) -> Result<Option<u32>>;

In `src/ocr/preprocess.rs` (blue-selective binarization for the bonus badge):

    pub fn blue_mask(crop: &ImageBuffer<Rgba<u8>, Vec<u8>>, bmin: u8, margin: u8) -> ImageBuffer<Luma<u8>, Vec<u8>>;

In `src/ocr/extract.rs` (or `src/ocr/reconcile.rs`, re-exported from `src/ocr/mod.rs`):

    pub enum Recovery { Ok, Repaired, Flagged }
    pub fn reconcile_stage(ocr_scores: [u32; 3], total: Option<u32>, bonus: Option<u32>) -> ([u32; 3], Recovery);

In `src/ocr/mod.rs`:

    pub struct StageReadout {
        pub scores: [[u32; 3]; 3],
        pub totals: [Option<u32>; 3],
        pub bonuses: [Option<u32>; 3],
        pub flags: [Recovery; 3],
    }
    pub fn ocr_screenshot(
        image: &ImageBuffer<Rgba<u8>, Vec<u8>>,
        score_regions: &[RelativeRect; 3],
        total_regions: &[RelativeRect; 3],
        bonus_regions: &[RelativeRect; 3],
    ) -> Result<StageReadout>;

In `src/automation/config.rs`:

    pub total_regions: [RelativeRect; 3],   // #[serde(default = "default_total_regions")]
    pub bonus_regions: [RelativeRect; 3],   // #[serde(default = "default_bonus_regions")]
    pub total_threshold: u8,                // #[serde(default = ...)] default 190
    pub bonus_blue_min: u8,                 // #[serde(default = ...)] default 190
    pub bonus_br_margin: u8,                // #[serde(default = ...)] default 30

The unchanged `SCORE_TOKEN_PATTERN` becomes the capped alternation given in M1. `parse_score`, `extract_scores`, `is_dash_char`, and `is_dash_like` remain as-is.


## Revision Note

2026-06-18: Initial authoring. Captures the overlap failure analysis (Modes A/B), the five-plus invariants the recovery relies on (notably "leading digit of a ≥1M score is always 1", "overlap corrupts only the right's leading 1 and the left's units digit", and the exact `total = c1+c2+c3+bonus` checksum), and a four-milestone plan: structural re-split to end the `u32` overflow crash, OCR of the isolated total and bonus regions, an exhaustive checksum solver that uniformly handles zero/one/two overlaps and flags genuine ambiguity, and pipeline/CSV wiring. The four real screenshots under `temp/failed_overlapped_samples/` are embedded as ground-truth fixtures. Region coordinates are starting estimates to be finalized during M2 calibration. Reason for this design: it leans entirely on data the screen already shows (the checksum), so reconstruction is verified rather than guessed, and it degrades safely when the extra OCR regions are unavailable.

2026-06-18 (revision): Bonus preprocessing and the checksum identity. (a) The bonus badge is light blue, preceded by a gold crown and a "+", and its column varies; recovery now binarizes it with a blue-selective mask (`bonus_blue_min`/`bonus_br_margin`) and parses the digits after the last "+", with the three preprocessing knobs (`total_threshold`, `bonus_blue_min=190`, `bonus_br_margin=30`) added to `config.json` and `AutomationConfig`. blue-min is 190 because the character icons' dimmer blue corrupted reads at 150. (b) Over-detection guards added (digit-count + range checks; exact-checksum as final guard) so a wrong-but-plausible total/bonus flags rather than corrupts. (c) Most important: the user supplied that `bonus = floor(max(c1,c2,c3) / 5)`, so the checksum collapses to `total = c1+c2+c3+floor(max/5)` needing only the total — the bonus becomes an optional cross-check, removing the dependency on the over-detection-prone bonus region. (d) The solver requires a corruption-aware asymmetric cost (units edit cheap only on the left neighbour of a +1M-restored slot) because a plain edit count ties on real data (sample 003). Reason: the discoveries make reconstruction depend only on the most reliable OCR region (the total) while still recovering exact units digits, and make every weaker/garbled input degrade to a flag rather than a silent error.
