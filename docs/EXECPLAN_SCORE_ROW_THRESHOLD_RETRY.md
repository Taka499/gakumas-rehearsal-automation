# Score-row multi-threshold OCR retry: recover knife-edge trailing-digit misreads

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. It must be maintained in accordance with `docs/PLANS.md` (from the repository root).

## Purpose / Big Picture

In field session `target/release/output/20260704_052007/` (500 runs, a two-character stage-1 composition, every score far below 1,000,000 so digit overlap is impossible), 10 stage-1 rows still came back needing human attention: 2 auto-repaired and 8 flagged, all with the same signature — the second score's **trailing digit misread** (`276,981` read as `276,982`, `601,170` as `601,172`, and so on).

The root cause is reproduced and deterministic: at the configured score-row luminance threshold **190**, the embedded Tesseract misclassifies this font's trailing "1"/"0" glyph at the game window size used (721×1281). Re-running Tesseract on the *same* crop binarized at 170, 180, 200, 210, or 220 reads the digits correctly. The threshold sits on a knife-edge for that one glyph; the crop, the game rendering, and the reconciliation solver are all fine. (The solver behaved correctly given its input: with two scores, a ±1/±2 units error can often be absorbed by a units-digit edit on *either* score with an identical checksum, so it rightly flagged those rows as ambiguous.)

After this change, when a stage's reconciliation comes back `Flagged`, the pipeline re-OCRs the **score row** at a few alternate thresholds and adopts the first re-read whose reconciliation passes the on-screen checksum without a flag — exactly the pattern the pipeline already uses for the isolated stage *total* (`TOTAL_ALT_THRESHOLDS`). No guessing is introduced: an adopted re-read must satisfy `total = c1+c2+c3+bonus` exactly. In the field session, this would have resolved all 8 flagged stage-1 rows automatically (the threshold-200 re-read passes the checksum with zero edits, so they log as `ok`).

Observable outcome: a new end-to-end test feeds the real iteration-107 screenshot through the real pipeline; before this change stage 1 comes back `Flagged` with a wrong best-effort value, after it comes back `Ok` with the true scores `[568601, 276981, 0]`.

## Progress

- [x] (2026-07-04) M1: Retry added (`SCORE_ALT_THRESHOLDS` + flag-gated block before the total retry in `ocr_screenshot`); fixtures under `tests/fixtures/score_retry_samples/` (LFS rule added to `.gitattributes`); `ocr_score_row_retry_e2e` FAILED before the change (107 → `Flagged [568600, 276982, 0]`; 45 → `Repaired`) and PASSES after (107 → `Ok [568601, 276981, 0]`; 45 → `Ok [510531, 601171, 0]`). Unit suite unchanged at 121; `ocr_overlap_recovery_e2e` still passes.
- [x] (2026-07-05) M2: Full-session replay done (499 rows, 641s). Result: **0 rows still flagged** (stage-1 flagged count 8 → 0; even iteration 222, expected to stay flagged, resolved to the hand-corrected values) and **exactly 1 non-flagged mismatch: iteration 45**, where the CSV itself holds the wrong auto-repair (`601170` vs pixel-verified `601171`) — the pipeline is right and the stored row is wrong. The replay's strict assert fails against this session BY DESIGN until that CSV row is hand-corrected in the review window; after correction it passes.
- [x] (2026-07-05) Fix field data: iteration 45's s1c2 corrected 601170 → 601171 via the review window (user-confirmed; row re-marked `manual`, charts regenerated). The replay assert is now expected to pass green against this session.

Use timestamps (UTC) when checking items off.

## Surprises & Discoveries

- Observation (2026-07-04, M1): the field CSV itself contains a silently WRONG auto-repair. Iteration 45's screenshot pixels read `510,531 / 601,171` with on-screen total `1,231,936` (verified by eye at 3–4× zoom AND by re-OCR of the saved PNG at threshold 210, which returns `1,231,936`). The field run that night logged `REPAIRED [510531, 601172] -> [510531, 601170] (total=Some(1231935))` — i.e. its total read was ALSO off by one, and the solver bent the score to match the wrong total, checksum-consistently. The stored `601170` (marked `repaired`) is wrong by 1. The new pipeline reads this row cleanly as `[510531, 601171]`/`Ok`.
  Evidence: `session.log` line 05:27:34.647 vs the e2e output `stage1 scores=[510531, 601171, 0] total=Some(1231936) ... flag=Ok`.
  Implication 1: the e2e expectation for case 45 was corrected to the pixel truth mid-M1.
  Implication 2: this is the checksum's fundamental blind spot — coordinated same-direction errors on both sides of the identity cannot be detected arithmetically. Rare (1 of 500 here), and the retry reduces exposure by getting the score row right in the first place.
  Implication 3: the field run's `1231935` total is NOT reproducible from the saved PNG with today's binary and config (both configs specify `total_threshold` 210); the exact conditions of that misread that night are unknown. The pixel evidence and reproducible re-OCR anchor the truth regardless.
- Observation (2026-07-04, M1): iteration 53 (the other stage-1 auto-repair) was pixel-verified CORRECT (`574,750 / 680,281`, total `1,391,087`) — the field CSV needs correcting only for iteration 45.

- Pre-plan finding, recorded for posterity: iteration 222 of the field session ALSO had a corrupted stage-1 *total* — the true `947,392` OCR'd as `9,473,920` (a leaked trailing digit forming a plausible 7-digit number, which the existing 8-digit "Pt-leak" guard cannot catch). That row needs both a score re-read AND a total re-read to reconcile; the independent retries in this plan will not fix it and it will correctly stay flagged for human review. A combined score×total retry is out of scope (see Decision Log).

## Decision Log

- Decision: The score-row retry runs BEFORE the existing total retry (both only when `flag == Flagged`, both short-circuit on success).
  Rationale: The score row carries up to three multi-digit numbers (many more glyphs than the total), and in the field data it was the misread input while the total was correct. Trying the noisier input first against the trusted checksum resolves the common case without spending seven total re-OCRs. Order between the two retries cannot change a *result* (both only adopt exact-checksum reconciliations), only which one wins when both could.
  Date/Author: 2026-07-04 / Claude + Taka499.

- Decision: Independent retries only — no combined score×total re-read matrix.
  Rationale: The observed failure mode is one input wrong at a time; a cross-product multiplies Tesseract invocations (~35 extra per flagged stage) for a case seen once in 500 runs (iteration 222), which correctly remains flagged for human review. Revisit if field data shows both-wrong rows are common.
  Date/Author: 2026-07-04 / Claude + Taka499.

- Decision: Fix via retry, not by changing the default `ocr_threshold` away from 190.
  Rationale: The sweep proved 190 is a knife-edge *for this glyph at this window size*; another composition, score magnitude, or window size could put a different threshold on a different knife-edge. The retry is threshold-agnostic and checksum-gated. (A user can still change `ocr_threshold` in config; the retry simply skips the alt equal to the configured value.)
  Date/Author: 2026-07-04 / Claude + Taka499.

- Decision: `reconcile.rs` is not touched.
  Rationale: This is pipeline orchestration in `src/ocr/mod.rs`. `reconcile.rs` mirrors `rehearsalRecovery.js` in the gakumas-tools fork line-for-line (upstream PR #103) and must not drift. The retry calls the existing `reconcile_stage` as-is.
  Date/Author: 2026-07-04 / Claude + Taka499.

## Outcomes & Retrospective

(2026-07-05, code-complete; one field-data correction pending user action.)

The retry landed as a single flag-gated block plus one constant in `src/ocr/mod.rs` — no signature, config, or `reconcile.rs` changes. Acceptance exceeded the plan: the fail-before/pass-after e2e behaved exactly as predicted, and the full-session replay resolved **all** previously-flagged rows (8 → 0, including the corrupted-total iteration 222 that was expected to remain flagged) with the only mismatch being a row where the *stored CSV* is provably wrong, not the pipeline.

Lessons: (a) checksum-consistent is not the same as correct — iteration 45 showed a coordinated ±1 on score and total sailing through the identity; when a "repaired" value matters, the pixels are the only ground truth; (b) field misreads are not always reproducible from the saved PNG (the 1231935 total could not be re-provoked), so fixtures must be validated against pixels, not against what the field log said; (c) the env-gated replay test is a keeper — it turned 500 screenshots of real gameplay into a regression suite for free.

## Context and Orientation

This repository is a Windows Rust app that automates in-game "rehearsal" runs: it screenshots each result screen and OCRs nine per-character scores with an embedded Tesseract, then repairs OCR damage using an on-screen checksum. The relevant pieces:

- `src/ocr/mod.rs` — the orchestration function `ocr_screenshot(img, regions: &OcrRegions)`. Per stage it: crops the score row (`regions.score[stage]`), binarizes it with `threshold_bright_pixels(crop, config.ocr_threshold)` (a luminance cutoff, default and field value **190**), OCRs it with `recognize_image_line`, extracts up to three numbers with `extract_single_stage`; separately OCRs the isolated stage total and bonus badge; then calls `reconcile_stage(raw_scores, total, bonus)` which returns the reconciled scores plus a `Recovery` flag (`Ok` / `Repaired` / `Flagged`). Two rescue tiers already exist, both gated on `Flagged`: a **total** re-OCR at `TOTAL_ALT_THRESHOLDS` (`const` at the top of the file), and a digit-stream re-partition fallback (`reconstruct_from_digits`).
- `src/ocr/reconcile.rs` — the checksum solver. READ-MOSTLY (JS parity, see Decision Log); this plan only calls it.
- The game guarantees `stage_total = c1 + c2 + c3 + floor(max(c1,c2,c3)/5)`, which is what makes "adopt a re-read only if it reconciles without a flag" safe: an adopted value is arithmetic-verified, never guessed.
- Tests: `GAKUMAS_NO_MANIFEST=1 cargo test` runs the unit suite unelevated (121 pass today). Tesseract-dependent e2e tests are `#[ignore]`d and live in `src/ocr/mod.rs::e2e_tests`, keyed to sample PNGs under `tests/fixtures/` (stored via Git LFS — check `.gitattributes`; run `git lfs pull` first). Run them with `GAKUMAS_NO_MANIFEST=1 cargo test <name> -- --ignored`.
- Field evidence for this plan lives in `target/release/output/20260704_052007/` (untracked): `session.log` holds the FLAGGED/REPAIRED lines, `results.csv` holds the hand-corrected ground truth (rows the user fixed are marked `manual`), and `screenshots/` holds all 500 PNGs.

Reproduction (already performed, 2026-07-04): running the embedded `tesseract.exe --psm 6` on iteration 107's stage-1 crop binarized at 190 yields `568,601 276,982 -`; at 170/180/200/210/220 (or 2–3× upscaled) it yields the correct `568,601 276,981`.

## Plan of Work

### M1 — the retry, fixtures, and e2e (branch `feature/score-row-threshold-retry`)

1. In `src/ocr/mod.rs`, next to `TOTAL_ALT_THRESHOLDS`, add:

        /// Alternate luminance thresholds for the score-row retry, tried in order
        /// when the primary `ocr_threshold` read can't be reconciled. Ordered by
        /// distance from the 190 default. Field evidence (session 20260704_052007):
        /// at exactly 190 Tesseract misreads a trailing "1"/"0" as "2" at the
        /// 721x1281 window size; every neighbouring cutoff reads it correctly.
        const SCORE_ALT_THRESHOLDS: &[u8] = &[200, 180, 210, 170, 220];

2. In `ocr_screenshot`'s per-stage body, insert a new rescue tier immediately AFTER the `reconcile_stage` call and BEFORE the existing total retry (both remain gated on `flag == Recovery::Flagged`):

        // Score-row multi-threshold retry. The score-row binarization is
        // threshold-sensitive the same way the total's is: at one knife-edge
        // cutoff a trailing "1"/"0" reads as "2" (session 20260704_052007).
        // Re-OCR the score crop at alternate thresholds and adopt the first
        // re-read that reconciles without a flag against the already-read
        // total/bonus (still no guessing - only an exact checksum is
        // accepted). Runs only on flagged stages, stops at first success.
        if flag == Recovery::Flagged {
            for &alt in SCORE_ALT_THRESHOLDS {
                if alt == threshold {
                    continue;
                }
                let alt_bin = threshold_bright_pixels(&score_crop, alt);
                let alt_lines = recognize_image_line(&alt_bin)?;
                let alt_raw = match extract_single_stage(&alt_lines) {
                    Ok(s) => s,
                    Err(_) => continue, // unparseable at this cutoff - try next
                };
                if alt_raw == raw {
                    continue; // same read would reconcile (and flag) identically
                }
                let (r2, f2) = reconcile_stage(alt_raw, readout.totals[stage_idx], bonus);
                if f2 != Recovery::Flagged {
                    crate::log(&format!(
                        "OCR stage {}: score retry t{} {:?} -> {:?}",
                        stage_idx + 1, alt, raw, r2
                    ));
                    reconciled = r2;
                    flag = f2;
                    break;
                }
            }
        }

    Notes: `threshold`, `score_crop`, `raw`, `bonus`, `reconciled`, and `flag` are all existing bindings in the loop body. The digit-stream fallback further down keeps using the PRIMARY read's `lines` (if the score retry succeeded, `flag` is no longer `Flagged` and the fallback is skipped). The final `readout.scores[stage_idx] = reconciled` assignment and the Ok/REPAIRED/FLAGGED logging need no changes.

3. Fixtures: copy two field screenshots into `tests/fixtures/score_retry_samples/` (mind Git LFS — `.gitattributes` must cover them the same way as `tests/fixtures/overlap_samples/`):
   - `107_20260704_053916.png` — was FLAGGED (ambiguous units-edit tie), kept the wrong value. Truth: stage 1 = `[568601, 276981, 0]`, total 959302, bonus 113720.
   - `045_20260704_052733.png` — was REPAIRED to the right value but only by luck of a unique candidate. Truth: stage 1 = `[510531, 601170, 0]`, total 1231935, bonus 120234.

4. New `#[ignore]`d test in `src/ocr/mod.rs::e2e_tests`, `ocr_score_row_retry_e2e`, mirroring the existing `ocr_overlap_recovery_e2e` shape: for each fixture, run `ocr_screenshot(&img, &config.ocr_regions())` and assert stage 1's scores equal the truth AND `flags[0] == Recovery::Ok` (the adopted re-read passes the checksum with zero edits, so it is `Ok`, not `Repaired`). Write the test FIRST and run it before adding the retry: it must FAIL (iteration 107 comes back `Flagged` with `[568600, 276982, 0]`); after step 2 it must PASS. Capture both runs in this plan.

### M2 — field replay (validation only, same branch)

Add a second `#[ignore]`d test `ocr_session_replay` (also in `e2e_tests`), gated on an environment variable so it is a no-op in normal runs:

    let Ok(session) = std::env::var("GAKUMAS_REPLAY_SESSION") else { return };

It reads `<session>/results.csv` (the header is `iteration,timestamp,screenshot,s1c1..s3c3,recovery`; the user's hand-corrected rows are `manual`, so the CSV is ground truth), runs `ocr_screenshot` on every screenshot, and prints per-row mismatches plus a summary. Assertion: every row whose re-run flag is NOT `Flagged` must match the CSV scores exactly (a row the pipeline still flags is allowed to mismatch — flagged means "human must look", and its CSV value came from that human). Expected on session `20260704_052007`: the 10 previously-affected stage-1 rows now reconcile to the hand-corrected values with flag `ok`; iteration 222 (corrupted total, see Surprises) remains flagged; zero non-flagged mismatches across all 500 rows and 3 stages. Runtime is Tesseract-bound (~10–15 minutes); run it once and record the summary here.

## Concrete Steps

All commands from the repository root `C:\Work\GitRepos\gakumas-rehearsal-automation`.

    git checkout -b feature/score-row-threshold-retry
    # fixtures
    mkdir tests/fixtures/score_retry_samples
    cp target/release/output/20260704_052007/screenshots/107_20260704_053916.png tests/fixtures/score_retry_samples/
    cp target/release/output/20260704_052007/screenshots/045_20260704_052733.png tests/fixtures/score_retry_samples/
    # (verify .gitattributes LFS coverage for tests/fixtures/**/*.png before committing)

    # write the e2e test, then prove it fails before the change:
    GAKUMAS_NO_MANIFEST=1 cargo test ocr_score_row_retry_e2e -- --ignored
    #   expected BEFORE: FAILED (stage 1 flagged, scores [568600, 276982, 0])

    # add SCORE_ALT_THRESHOLDS + the retry block, then:
    cargo check 2>&1 | grep "^error"          # expect nothing (~28 warnings are normal)
    GAKUMAS_NO_MANIFEST=1 cargo test          # expect 121 passed (no unit-test change)
    GAKUMAS_NO_MANIFEST=1 cargo test ocr_score_row_retry_e2e -- --ignored     # expect ok
    GAKUMAS_NO_MANIFEST=1 cargo test ocr_overlap_recovery_e2e -- --ignored    # expect ok (no regression)

    # M2 replay (one-off, ~10-15 min):
    GAKUMAS_REPLAY_SESSION=target/release/output/20260704_052007 GAKUMAS_NO_MANIFEST=1 cargo test ocr_session_replay -- --ignored --nocapture

Commit in small steps (fixtures+test, then the retry, then docs), merge to `main` after acceptance, following the repo's git-flow discipline. No Claude attribution in commits.

## Validation and Acceptance

- `ocr_score_row_retry_e2e` fails before the retry exists and passes after, with stage 1 of iteration 107 going from `Flagged [568600, 276982, 0]` to `Ok [568601, 276981, 0]`.
- The pre-existing `ocr_overlap_recovery_e2e` (6 overlap cases) still passes — the retry must not disturb genuine collision recovery.
- The unit suite still passes at 121 (this change adds no pure logic; the retry is Tesseract-bound and covered by e2e).
- The M2 replay over the full field session reports zero non-flagged mismatches, the 10 affected rows resolve to the hand-corrected values, and the flagged-row count for stage 1 drops from 8 to at most 1 (iteration 222's corrupted-total row).
- Live behaviour (next real run with a similar composition): `session.log` shows `score retry tNNN` lines instead of stage-1 FLAGGED lines, and the review window's "N rows need checking" prompt shrinks accordingly.

## Idempotence and Recovery

Everything is an ordinary git branch; abandon it with `git checkout main`. The retry is additive and gated on `Flagged`, so any row the old pipeline handled cleanly is untouched by construction. The replay test writes nothing — it only reads screenshots and prints. Fixture copies are plain file adds.

## Artifacts and Notes

Reproduction evidence (2026-07-04, embedded Tesseract, iteration 107 stage-1 crop, `--psm 6`):

    threshold 190 -> "568,601 276,982 -"     (misread; 190 is the configured ocr_threshold)
    thresholds 170/180/200/210/220 -> "568,601 276,981 -"   (correct)

Field session tallies (`session.log`, 20260704_052007, 500 runs): stage 1 — 2 REPAIRED + 8 FLAGGED (all trailing-digit misreads; the 8 flagged were units-edit ties the solver correctly refused to guess); stage 2 — 7 REPAIRED (genuine ≥1M collision-victim repairs, working as designed); stage 3 — 0.

## Interfaces and Dependencies

No new crates, no signature changes, no config-format changes. At the end of M1 the following exist in `src/ocr/mod.rs`:

    const SCORE_ALT_THRESHOLDS: &[u8] = &[200, 180, 210, 170, 220];
    // inside ocr_screenshot's stage loop, after reconcile_stage and before the
    // total retry: the flag-gated score-row retry block described above

and in `src/ocr/mod.rs::e2e_tests`:

    #[test] #[ignore] fn ocr_score_row_retry_e2e()   // two fixture cases
    #[test] #[ignore] fn ocr_session_replay()        // env-gated full-session replay

`reconcile.rs`, `extract.rs`, `engine.rs`, `preprocess.rs` are unchanged.
