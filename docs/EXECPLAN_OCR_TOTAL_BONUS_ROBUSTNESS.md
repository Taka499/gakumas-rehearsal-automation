# Make OCR robust to noisy stage-total/bonus reads and recover single-character corruption

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

This repository contains `docs/PLANS.md` (relative to the repository root). This document must be maintained in accordance with `docs/PLANS.md`.

This plan builds directly on `docs/EXECPLAN_OVERLAP_SCORE_RECOVERY.md` (checked into this repo). That plan introduced the checksum-based recovery this one hardens; read its `Purpose`, `The key invariants this plan relies on`, and `Surprises & Discoveries` sections first. Where this plan repeats facts from it, that is deliberate (per PLANS.md, each plan must be self-contained).


## Purpose / Big Picture

The tool automates the game *gakumas*' "rehearsal" feature: it clicks through many rehearsal runs and, on each result screen, reads nine numbers via OCR (Optical Character Recognition — turning pixels of text into digits). Each of three "stages" shows up to three per-character breakdown scores; they are written to `results.csv`. The screen also shows, per stage, an isolated **stage total** (e.g. `393,454Pt`) and a **bonus badge** (a gold crown then `+65,575` in light blue). The game guarantees the exact identity

    stage_total = c1 + c2 + c3 + floor(max(c1, c2, c3) / 5)

(the bonus is `floor(max/5)`), so the prior plan reconstructs scores corrupted by the "overlapping million" rendering bug from the total alone, using the bonus as a cross-check, and flags a row (`recovery=flagged` in the CSV) when it cannot recover unambiguously.

The problem this plan fixes was found in the field run `target/release/output/20260623_232320/` (200 iterations × 3 = 577 readable stages; almost every stage was a **single character**). 121 iterations were marked `flagged`, but on inspection **~94 % of those flags are false**: the per-character score read correctly and only the two *isolated checksum numbers* mis-OCR'd, so the exact checksum could not confirm an already-correct score. Two concrete recognition failures dominate:

- The stage total's **thousands comma is read as a digit** (almost always `5`): `393,454Pt` → `3935454`. A 6-digit total inflates to 7 digits, which still passes the "≤ 7 digits" guard, so the checksum silently fails.
- The bonus badge's **leading `+` (next to the gold crown) is read as a digit `4`**: `+65,575` → `465575`. The `+`-anchored parser then finds no `+`, falls back to the whole string, and keeps the spurious `4`.

A smaller set (8 of 577 stages, ~1.4 %) were **genuinely wrong scores** that the prior recovery could not fix because it was written only for the *multi-character overlap* case, never for a lone character:

- Five single ≥1,000,000 characters whose leading `1` read as `4`, producing an impossible ≥3,000,000 seven-digit value (e.g. `4,177,174` for true `1,177,174`). This is the same "1,"→"4" glyph confusion the prior plan saw at overlaps, but here on a single isolated score with no neighbour.
- Two characters that **dropped a leading digit** entirely (`92118` for true `892118`; `55172` for true `855172`).
- One character whose score row was **split into two tokens** by OCR (`41110` `707` for true `1110707`).

After this change: a single-character (more precisely, *non-collision-prone*) stage whose score reads cleanly is recorded as `ok` even when its total/bonus mini-numbers are garbled, so the false-flag rate collapses; and the genuine single-character corruptions above are reconstructed (`repaired`) from the checksum exactly, or flagged only when truly unrecoverable. You can see it working by running the unit tests added here (each fails before the change and passes after) and by re-running OCR over the `20260623_232320` screenshots and observing the flag count drop from 121 to a handful while the 8 wrong scores become correct.


## Key facts and invariants this plan relies on

These come from the prior plan (verified across many real screenshots) plus the new field run. Define them to yourself; the recovery depends on them.

1. A per-character score's leading digit, when the score is ≥ 1,000,000, is `1` or `2` (scores have approached but not reached 3,000,000). `MAX_SCORE = 3_000_000` is the exclusive upper bound used to prune candidates. A clean read above this would be unaffected (the raw value is always a candidate), but a *recovered* value is bounded below it.
2. The checksum `stage_total = c1 + c2 + c3 + floor(max(c1,c2,c3) / 5)` is exact. It needs only the total; the bonus (`floor(max/5)`) is an independent cross-check.
3. "Overlap corruption" (the prior plan's subject) happens only between two side-by-side scores that are **both** ≥ 1,000,000. A stage with at most one non-zero score therefore **cannot** have overlap corruption — its single score, if it reads as a well-formed in-range number, is trustworthy on its own. We call a stage **collision-prone** when it has at least one ≥ 1,000,000 raw slot **and** at least two non-zero raw slots (this matches the existing `structural_only` test in `src/ocr/reconcile.rs`; note a victim that lost its leading million reads as < 1,000,000, so collision-proneness must be judged from "has a million somewhere and ≥ 2 numbers", not from "two visible millions").
4. The "1,"→"4" glyph confusion is **not** exclusive to overlaps. On any score whose leading `1` is immediately followed by a thousands comma (i.e. any ≥ 1,000,000 score, rendered `1,XXX,XXX`), OCR can read the `1,` pair as a single `4`, giving an impossible `4,XXX,XXX` (≥ 3,000,000). The fix: an impossible seven-digit value's leading digit is replaced by `1` or `2`, guarded by the checksum.
5. The stage total's thousands comma can OCR as a spurious digit, inserting **one** extra digit at a thousands-separator position (observed: `393,454` → `3935454`). So an OCR total may be the true total with one extra interior digit. The fix: when the literal total yields no checksum solution, also try the total with any one digit deleted.
6. The bonus badge **always** renders a literal `+` immediately before its number. If the parser does not find a `+`, the leading glyph (crown/`+`) was misread as a digit and the read cannot be trusted: return no bonus rather than a value with a prepended garbage digit. The bonus is only a cross-check, so discarding it never corrupts a value — it at most disables one corroboration path.


## Progress

- [x] (2026-06-23) M1 — Bonus `+`-anchor hardening: in `src/ocr/engine.rs::parse_single_number`, when `anchor_plus` is true and no `+` is present, return `None` instead of falling back to the whole string. Tests added (`+65,575`→`65575`, no-`+` `465575`→`None`, total path `465575`→`465575` unchanged); the prior no-`+` fallback assertion flipped to `None`. `cargo test bonus_parsing`/`test_total_parsing` → pass.
- [x] (2026-06-23) M2 — Solver robustness in `src/ocr/reconcile.rs`: (a) `candidates()` adds leading-digit-replacement (`1`/`2`) for an impossible ≥ `MAX_SCORE` seven-digit raw value; (b) `reconcile_stage` step 0 uses `plausible_floor` so an impossible raw slot does not reject a smaller true total; (c) the checksum search iterates `total_candidates` = literal + one-digit deletions, with a `+1` selection penalty on deletions; (d) `cost` charges `+1` for a changed leading digit, and recovery is now classified by `chosen == ocr_scores` (so a clean score under a deleted total is `Ok`, not `Repaired`). Tests added for the five real ≥3M cases and the comma-inserted-total false-flag case. `cargo test reconcile` → 33 passed.
- [x] (2026-06-23) M3 — Non-collision-prone fallback `resolve_without_checksum` in `reconcile.rs`: replaces the unconditional `solutions.is_empty() -> Flagged`. Collision-prone stays Flagged; otherwise a single-slot total-solve (`solve_single`) recovers dropped-leading-digit cases (`55172`→`855172`, `92118`→`892118`) corroborated by the bonus, else the raw read is `Ok` when the bonus is absent or corroborates, else `Flagged`. Tests added (recovery, transposed-total-bonus-corroborates `Ok`, bonus-contradicts `Flagged`, clean-stays-`Ok`). Full suite → 95 passed, 0 failed, 1 ignored.
- [ ] M4 — Single-character digit-stream recovery in `reconstruct_from_digits` for the split-token case (`41110707`→`1110707`), plus an end-to-end pass: re-run OCR over the `20260623_232320` screenshots (or a representative subset) and confirm the flag count drops and the 8 known-wrong scores become correct. Update `docs/EXECPLAN_OVERLAP_SCORE_RECOVERY.md`'s cross-reference and this plan's Outcomes.

Use timestamps when you check items off, e.g. `- [x] (2026-06-23 14:00Z) ...`.


## Surprises & Discoveries

- Observation (field run 20260623_232320): the dominant failure is **false flags**, not wrong scores. 121/577 stages flagged; ~115 had a correct score and only a mis-OCR'd total/bonus. Stage 3 totals were worst (the comma→`5` insertion clustered there).
  Evidence: cropping the actual regions (e.g. iter 3 stage 3) shows score `327879` is correct; the total region image reads `393,454Pt` but OCR'd `3935454`, and the bonus region reads `👑+65,575` but OCR'd `465575`. Classification of all 129 flagged stage-events: 121 score-correct (total/bonus mis-OCR), 8 genuinely wrong.

- (Add further observations here as M1–M4 land, with short evidence snippets — test output is ideal.)


## Decision Log

- Decision: Harden recognition at the source (bonus `+`-required; total comma-insertion tolerance) rather than only loosening the flag policy.
  Rationale: The checksum is only as trustworthy as its inputs. Making the total/bonus reliable lets the existing exact-checksum keep catching *genuine* errors (the 8 wrong scores) while no longer punishing correct scores for noisy mini-numbers. Loosening the flag policy alone would either keep false flags or start trusting genuinely-wrong reads.
  Date/Author: 2026-06-23, design phase (after diagnosing run 20260623_232320).

- Decision: For a **non-collision-prone** stage, treat the per-character score as trustworthy unless there is positive evidence it is wrong; do not let a noisy total/bonus alone force a flag.
  Rationale: Invariant 3 — a stage with at most one non-zero score cannot have overlap corruption. The only single-character corruptions are an impossible ≥3M value (detectable from the score alone), a dropped leading digit (detectable because the total/bonus then disagree), or a split token (detectable structurally). Absent any of those signals, a clean in-range score is correct, and the total/bonus are the unreliable party.
  Date/Author: 2026-06-23, design phase.

- Decision: Recover an impossible ≥3M seven-digit value by replacing its leading digit with `1`/`2`, and recover a dropped-leading-digit single character by solving the score directly from the (comma-tolerant) total; both gated by the exact checksum and, when present, the bonus.
  Rationale: These are the two genuine single-character corruption modes observed. The checksum pins the answer exactly (a single character's score is the unique `c` with `c + floor(c/5) == total`), so recovery is verified, not guessed; the bonus corroborates.
  Date/Author: 2026-06-23, design phase.

- Decision: Leave the existing `20260623_232320` CSVs unpatched; drop a `DATA_QUALITY_NOTE.md` in that folder instead.
  Rationale: User instruction. The note records which 8 rows are genuinely wrong (with corrected values) and that the bulk of flags are false, so the data is usable without rewriting history.
  Date/Author: 2026-06-23, user direction.


## Outcomes & Retrospective

(To be written as milestones land. Compare against Purpose: flag rate on the 20260623_232320 screenshots should fall from 121 toward single digits, and the 8 known-wrong scores should become correct, with no regression in the prior plan's 6 end-to-end overlap fixtures.)


## Context and Orientation

The application is a Windows system-tray screenshot/automation tool in Rust. It captures the game window, clicks through rehearsals, and OCRs the result screen with an embedded Tesseract (`target/release/tesseract/tesseract.exe`, English data under `target/release/tesseract/tessdata/`). You need no prior context beyond the files below.

Key files, by full repository-relative path:

- `src/ocr/mod.rs` — `ocr_screenshot(img, score_regions, total_regions, bonus_regions) -> Result<StageReadout>`. For each of three stages it crops and OCRs the score row, the isolated stage total (white text, luminance threshold), and the bonus badge (light-blue blue-mask), then calls `reconcile_stage` (and, when that flags, `reconstruct_from_digits`) and stores the corrected scores plus a `Recovery` flag. `StageReadout { scores:[[u32;3];3], totals:[Option<u32>;3], bonuses:[Option<u32>;3], flags:[Recovery;3] }`.
- `src/ocr/engine.rs` — runs Tesseract. `recognize_single_number(img, whitelist, anchor_plus) -> Result<Option<u32>>` OCRs a pre-binarized crop as one integer (page-segmentation mode 7, character whitelist). Its pure helper `parse_single_number(raw: &str, anchor_plus: bool) -> Option<u32>` extracts the integer: when `anchor_plus` it keeps digits after the **last** `+`; it uses `longest_digit_run` (treats `,`/`.` as in-number separators that are skipped) and a digit-count guard (> 7 for a total, > 6 for a bonus → `None`; an 8-digit total is truncated to its first 7 to drop a faint "Pt" leak). This is where M1 changes.
- `src/ocr/reconcile.rs` — the pure recovery logic. `reconcile_stage(ocr_scores:[u32;3], total:Option<u32>, bonus:Option<u32>) -> ([u32;3], Recovery)` validates the total/bonus (step 0), builds per-slot candidate values via `candidates(v)`, exhaustively searches combinations satisfying the exact checksum, scores them with a corruption-aware `cost`, and returns the best with `Recovery::{Ok,Repaired,Flagged}`; `structural_only` is the no-usable-total fallback. `reconstruct_from_digits(digits, total, bonus)` re-partitions the raw score-row digit stream when the comma tokenizer failed (the two-collision case, and — added by the prior plan — substituted/duplicated leading digits on **non-first** parts). `MAX_SCORE = 3_000_000`. This is where M2/M3/M4 change. **Read this whole file before editing it.**
- `src/automation/ocr_worker.rs` — background worker; calls `ocr_screenshot`, derives the worst-of-three recovery, logs flagged/repaired iterations, and appends the 13th `recovery` CSV column (`ok`/`repaired`/`flagged`).
- `src/automation/config.rs` + repo-root/`target/release` `config.json` — `total_regions`/`bonus_regions` and the thresholds `total_threshold`/`bonus_blue_min`/`bonus_br_margin`. Not changed by this plan (the fix is in recognition parsing and reconciliation, not region geometry), but useful for re-running OCR in M4.

Term definitions: "stage" = one rehearsal row of up to three per-character scores. "slot"/"character column" = one of the three positions. "collision-prone" = a stage with ≥ 1 raw slot ≥ 1,000,000 and ≥ 2 non-zero raw slots (only such stages can have overlap corruption). "units digit" = the rightmost digit. "impossible value" = a per-character score ≥ `MAX_SCORE` (3,000,000), which a real score never is. "total-solve" = computing the unique `c` with `c + floor(c/5) == total` for a single-character stage.

Build/test environment: the release binary embeds a Windows admin manifest, which makes `cargo test` require elevation (os error 740). Build tests with `GAKUMAS_NO_MANIFEST=1 cargo test` to skip the manifest (the gate is in `build.rs`; normal/release builds still embed it). The pure logic in `reconcile.rs` and `engine.rs::parse_single_number` is fully unit-testable this way. Tesseract-dependent end-to-end checks are `#[ignore]`d; run them explicitly, e.g. `GAKUMAS_NO_MANIFEST=1 cargo test ocr_overlap_recovery_e2e -- --ignored`.


## Plan of Work

The work is four milestones, each independently testable. M1 and M2/M3 are pure-function changes covered by `cargo test`; M4 adds one more pure case and an end-to-end confirmation. Implement and test in order; each milestone's tests must fail before its change and pass after.


### Milestone M1 — Bonus badge must contain a literal "+"

Goal: a bonus read where the leading `+` was misread as a digit (so no `+` survives) returns `None` instead of a value with a prepended garbage digit, removing the largest source of bonus over-reads. The total is unaffected.

Edit `src/ocr/engine.rs`, function `parse_single_number`. Today, when `anchor_plus` is true and `raw.rfind('+')` is `None`, it falls back to using the whole `raw` string. Change that branch so a missing `+` yields `None` (the badge always renders a `+`; its absence means the crown/`+` glyph was read as a digit and the value cannot be trusted). When a `+` **is** present, behaviour is unchanged (take the digits after the last `+`). The `anchor_plus == false` (total) path is unchanged.

Add unit tests in that file's `#[cfg(test)] mod tests`:

- `parse_single_number("+65,575", true) == Some(65575)` (normal bonus).
- `parse_single_number("465575", true) == None` (crown+`+` read as `4`, no literal `+`).
- `parse_single_number("465575", false) == Some(465575)` (the change is anchor-only; a total path keeps the value).
- Keep/adjust any existing test that asserted the no-`+` fallback returned a number.

Run `GAKUMAS_NO_MANIFEST=1 cargo test parse_single_number` (or `cargo test engine`) and expect the new tests to pass and existing ones to still pass.

Acceptance: re-OCR'ing a stage whose bonus badge renders `👑+65,575` no longer yields `465575`; it yields `65575` (when the `+` reads) or `None` (when it does not), so a spurious bonus never flags an otherwise-correct stage.


### Milestone M2 — Solver tolerates comma-inserted totals and recovers impossible ≥3M values

Goal: `reconcile_stage` (a) recovers a single isolated `4,XXX,XXX` (leading `1`→`4`) to `1,XXX,XXX`, and (b) stops flagging a correct score whose total gained one spurious comma-digit — both guarded by the exact checksum. All in `src/ocr/reconcile.rs`.

Four edits:

(a) In `candidates(v)`: after the existing base/units-variant generation, if `v` is a seven-digit impossible value (`v >= MAX_SCORE` and `v < 10_000_000`), add the leading-digit-replacement candidates `1_000_000 + (v % 1_000_000)` and `2_000_000 + (v % 1_000_000)` (both < `MAX_SCORE`), each also expanded with its ten units-digit variants like the other bases. This makes `1,177,174` a candidate for raw `4,177,174`. (Do not gate this on slot position — a lone first slot is exactly the case we must fix.)

(b) In `reconcile_stage` step 0, the guard "total must be ≥ the largest raw score" currently uses `ocr_scores.iter().max()`. An impossible raw slot (e.g. `4,177,174`) is larger than the true, smaller total and would wrongly reject it. Compute the floor from each slot's **plausible magnitude** instead: define a small helper `plausible_floor(v) = if v < MAX_SCORE { v } else { 1_000_000 + v % 1_000_000 }` and take the max of that over the slots. So `4,177,174` contributes `1,177,174` to the floor, letting total `1,412,608` pass.

(c) In `reconcile_stage`, generalize the checksum search to try, in addition to the literal `total`, every total formed by deleting any one digit of the literal total's decimal string (the comma-insertion tolerance, invariant 5). Only keep a deleted-total candidate that is still a plausible total (`> 0`, `<= 9_999_999`, and `>= plausible_floor` max). Add a per-solution cost penalty of `+1` for solutions found under a deleted total so the literal total wins ties — i.e. a correct score whose literal total works is never overridden by a coincidental deletion. Implementation note: keep the existing per-slot candidate search; wrap the "for combo … if a+b+c+max/5 == total" check in a loop over the (literal + single-deletion) total set, recording which total produced each solution so the penalty can be applied.

(d) In `cost(chosen, raw)`: add a term charging `+1` when `chosen[i]` and `raw[i]` are both ≥ 1,000,000 but their **leading digits differ** (a leading-`1`→`4` repair). Without this the leading-digit replacement from (a) scores cost 0 and would be reported `Recovery::Ok`; it must be `Recovery::Repaired`.

Add unit tests (pure) using the five real ≥3M field cases and one comma-inserted-total false-flag case. Each `reconcile_stage(...)` call asserts the corrected scores and flag:

- `[4172520,0,0]`, `Some(1407024)`, `Some(234504)` → `([1172520,0,0], Repaired)` (iter 267).
- `[4177174,0,0]`, `Some(1412608)`, `None` → `([1177174,0,0], Repaired)` (iter 304; bonus was unread).
- `[4117975,0,0]`, `Some(1341570)`, `Some(223595)` → `([1117975,0,0], Repaired)` (iter 349).
- `[4115501,0,0]`, `Some(1338601)`, `Some(223100)` → `([1115501,0,0], Repaired)` (iter 363).
- `[4122517,0,0]`, `Some(1347020)`, `Some(224503)` → `([1122517,0,0], Repaired)` (iter 521).
- Comma-inserted total false flag: `[327879,0,0]`, `Some(3935454)` (true total `393,454` with the comma read as `5`), `None` → `([327879,0,0], Ok)` (the one-digit-deletion `393454` satisfies the checksum at cost 0 for the literal score; report `Ok`, score unchanged).

Run `GAKUMAS_NO_MANIFEST=1 cargo test reconcile` and expect all prior `reconcile` tests plus the new ones to pass. The prior plan's tests (`test_unreachable_total_flags`, `test_off_by_one_total_is_satisfiable`, the four overlap samples) must be unchanged — verify they still pass (the deletion candidates are 6-digit and get filtered out whenever a real ≥1M score sets the floor, so multi-character overlap cases are unaffected).


### Milestone M3 — Trust clean single-character scores; recover dropped leading digits

Goal: when the checksum search finds **no** solution and the stage is **not** collision-prone, `reconcile_stage` resolves the stage instead of flagging a correct score — recovering a dropped-leading-digit single character from the total, otherwise accepting the clean raw read as `ok` unless the bonus positively contradicts it.

Edit `reconcile_stage` at the point where the search yields zero solutions (today: `if solutions.is_empty() { return (ocr_scores, Recovery::Flagged); }`). Replace that unconditional flag with:

1. Compute `collision_prone` = `(any raw slot >= 1_000_000) && (count of non-zero raw slots >= 2)` (invariant 3). If `collision_prone`, keep the existing behaviour: return `(ocr_scores, Recovery::Flagged)` — the caller's `reconstruct_from_digits` fallback (and M4) handles these.

2. Otherwise the stage has at most one non-zero score (or only sub-million scores; either way no overlap is possible). Let `idx` be the single non-zero slot if exactly one exists. **Total-solve:** for each total candidate `T` in (literal total + single-digit deletions, validated as in M2c), compute the unique `c` with `c + floor(c/5) == T` (search `c` in a tiny window around `floor(T*5/6)`); accept `c` only if `c < MAX_SCORE` and (`bonus` is `None` or `floor(c/5) == bonus`). Prefer a `c` from the literal total over a deletion. If such a `c` exists for a single-non-zero stage: place it in slot `idx`; return `Recovery::Ok` when `c == ocr_scores[idx]`, else `Recovery::Repaired`. This recovers `55172`→`855172` (total `1,026,206`, bonus `171,034`) and `92118`→`892118` (total `1,070,541`, bonus `178,423`), and confirms correct reads as `Ok`.

3. If no total-solve `c` was found (total too garbled, e.g. a transposition that one deletion cannot fix): fall back to the bonus. If `bonus` is present and `floor(max_raw_plausible / 5) == bonus`, the bonus corroborates the raw read → return `(ocr_scores, Recovery::Ok)`. If `bonus` is present and disagrees → `Recovery::Flagged` (positive evidence of an error we cannot pin without a good total). If `bonus` is absent → return `(ocr_scores, Recovery::Ok)` (a non-collision-prone, in-range read with nothing contradicting it; invariant 3 says it cannot be overlap-corrupted).

Add unit tests (pure):

- Dropped leading digit recovered: `[55172,0,0]`, `Some(1026206)`, `Some(171034)` → `([855172,0,0], Repaired)`; and `[92118,0,0]`, `Some(1070541)`, `Some(178423)` → `([892118,0,0], Repaired)`.
- Comma-garbled total, clean score, no usable bonus → not flagged: `[327879,0,0]`, `Some(3935454)`, `None` already returns `Ok` via M2; add one where even deletion fails but the score is clean, e.g. `[1119377,0,0]`, `Some(1343525)` (a transposed total `1,343,252`→`1,343,525` that no single deletion fixes), `Some(223875)` → `([1119377,0,0], Ok)` (bonus `floor(1119377/5)=223875` corroborates).
- Bonus contradicts a clean-looking single score, total unusable → flag: `[500000,0,0]`, `Some(9999999)` (garbage total, demoted by step-0 range/`> max` checks or unsatisfiable), `Some(123456)` (≠ `floor(500000/5)=100000`) → `([500000,0,0], Flagged)`.
- Regression: a normal single sub-million score with a matching total stays `Ok` (covered by existing `reconcile_stage` happy-path tests; add one if none exists, e.g. `[994573,0,0]`, `Some(1193487)`, `None` → `([994573,0,0], Ok)`).

Run `GAKUMAS_NO_MANIFEST=1 cargo test reconcile`; new tests pass, all prior tests unchanged. Acceptance: re-OCR of the `20260623_232320` single-character stages no longer flags correct scores (the ~115 false flags become `ok`), and the dropped-leading-digit stages become `repaired`.


### Milestone M4 — Recover a split single-character score, and confirm end-to-end

Goal: the one remaining field case (iter 400: the score row OCR'd as two tokens `41110` `707` for true `1,110,707`) recovers, and the whole change is confirmed against the real screenshots.

Edit `reconstruct_from_digits` in `src/ocr/reconcile.rs`. It re-partitions the raw score-row digit stream (all non-digits stripped) into `k` consecutive scores and keeps partitions satisfying the exact total/bonus. Today its leading-digit repairs (impossible-7-digit → `1`/`2`; duplicated-8-digit → drop one) are gated to **non-first** parts (`i > 0`) because overlap corruption only affects the right operand of a junction. The single-character split is a different mode: there is no junction, the whole stream is one number that gained a spurious leading digit (`41110707` = `4` + `1110707`). Extend the partition logic so that for the **first part of a single-part partition** (`k == 1`, `i == 0`) an eight-digit part may drop its leading digit to form a seven-digit value `< MAX_SCORE` (no equal-first-two-digits requirement), and a seven-digit impossible part may have its leading digit replaced by `1`/`2` — both marked as non-restored glyph fixes (they need no ≥1M left neighbour) and still gated by the exact total checksum and the bonus cross-check. Because the checksum is exact and the bonus corroborates, only the true partition survives.

Add a unit test: `reconstruct_from_digits("41110707", Some(1332848), Some(222141)) == Some(([1110707,0,0], Repaired))` (iter 400).

Confirm `reconstruct_from_digits` is still invoked from `src/ocr/mod.rs` when `reconcile_stage` flags (it is, in `ocr_screenshot`); a single-character flagged stage with a split row will now route through this and recover.

End-to-end confirmation (Tesseract required, so not a pure `cargo test`): re-run the OCR pipeline over the screenshots under `target/release/output/20260623_232320/screenshots/` — either via a small throwaway `#[ignore]`d test that loops the PNGs through `ocr_screenshot` (mirroring the existing `ocr_overlap_recovery_e2e` test in `src/ocr/mod.rs`) and prints per-iteration flags, or by replaying them through the worker. Observe that the count of flagged stages drops from 129 toward single digits, that iterations 207/267/304/349/363/400/472/521 now produce the corrected stage-2 scores listed in this run's `DATA_QUALITY_NOTE.md`, and that the prior plan's six overlap fixtures still pass (`GAKUMAS_NO_MANIFEST=1 cargo test ocr_overlap_recovery_e2e -- --ignored`). Record the before/after flag counts in Outcomes.

Finally, add a one-line cross-reference in `docs/EXECPLAN_OVERLAP_SCORE_RECOVERY.md` (e.g. in its Progress or a Surprises entry) pointing to this plan as the follow-on robustness work, and write this plan's `Outcomes & Retrospective`.


## Concrete Steps

Work from the repository root `C:\Work\GitRepos\gakumas-screenshot`. PowerShell is primary; the Bash tool is available for POSIX one-liners.

Build (expect ~30 pre-existing warnings; only `^error` lines matter):

    cargo build --release

Focused unit tests as you implement each milestone:

    GAKUMAS_NO_MANIFEST=1 cargo test engine        # M1
    GAKUMAS_NO_MANIFEST=1 cargo test reconcile      # M2, M3, M4

End-to-end overlap regression (M4):

    GAKUMAS_NO_MANIFEST=1 cargo test ocr_overlap_recovery_e2e -- --ignored --nocapture

To re-OCR the field run for the M4 confirmation, either add a temporary `#[ignore]`d test alongside `ocr_overlap_recovery_e2e` that iterates the PNGs in `target/release/output/20260623_232320/screenshots/` and prints `iteration, flags`, or feed individual PNGs through a small debug entry point. The expected corrected stage-2 values are in `target/release/output/20260623_232320/DATA_QUALITY_NOTE.md`.


## Validation and Acceptance

Acceptance is observable behaviour, not code shape.

- M1: `GAKUMAS_NO_MANIFEST=1 cargo test engine` passes, including the three new `parse_single_number` cases. A bonus string with no `+` returns `None`.
- M2: `GAKUMAS_NO_MANIFEST=1 cargo test reconcile` passes; the five real ≥3M cases recover to `1,XXX,XXX` (`Repaired`), the comma-inserted-total case returns the unchanged score as `Ok`, and every prior `reconcile` test is unchanged.
- M3: a non-collision-prone stage with a garbled total/bonus but a clean score is `Ok`, dropped-leading-digit single characters recover (`Repaired`), and a bonus that contradicts a clean score with no usable total is `Flagged`.
- M4: `reconstruct_from_digits("41110707", Some(1332848), Some(222141))` returns `([1110707,0,0], Repaired)`; re-OCR of the `20260623_232320` screenshots drops the flagged-stage count from 129 toward single digits and yields the eight corrected stage-2 scores; the six overlap fixtures still pass.

In every recovered case the result satisfies `c1 + c2 + c3 + floor(max/5) == total` for the (comma-corrected) total, and `floor(max/5) == bonus` whenever the bonus was read.


## Idempotence and Recovery

All steps are safe to re-run; `cargo build`/`cargo test` are idempotent. Every change is additive to pure functions guarded by the exact checksum, so the worst case of a mis-tuned candidate is that a stage flags (degrades to human review) rather than stores a wrong value — the existing layered guards (digit-count, plausible-range, exact-checksum, physical-validity) remain in force. The `20260623_232320` CSVs are intentionally left unmodified; only `DATA_QUALITY_NOTE.md` documents them. Keep `temp/` and `debug/` scratch out of commits; the field-run `output/` tree is git-ignored.


## Artifacts and Notes

The eight genuinely-wrong stage-2 readings from run `20260623_232320` and their checksum-verified corrections (also in that folder's `DATA_QUALITY_NOTE.md`). All have `c2 = c3 = 0` (single character); `total`/`bonus` are the on-screen checksum numbers used to verify:

    iter 207: raw 92118      -> 892118   (total 1070541, bonus 178423)  dropped leading 8
    iter 267: raw 4172520    -> 1172520  (total 1407024, bonus 234504)  leading 1->4
    iter 304: raw 4177174    -> 1177174  (total 1412608, bonus —)       leading 1->4
    iter 349: raw 4117975    -> 1117975  (total 1341570, bonus 223595)  leading 1->4
    iter 363: raw 4115501    -> 1115501  (total 1338601, bonus 223100)  leading 1->4
    iter 400: raw 41110,707  -> 1110707  (total 1332848, bonus 222141)  split into two tokens
    iter 472: raw 55172      -> 855172   (total 1026206, bonus 171034)  dropped leading 8
    iter 521: raw 4122517    -> 1122517  (total 1347020, bonus 224503)  leading 1->4

Worked single-character total-solve (iter 472): the unique `c` with `c + floor(c/5) == 1,026,206` is `855,172` (`855172 + 171034 = 1026206`), and `floor(855172/5) = 171034` equals the bonus — so the dropped-leading-`8` read `55172` is recovered exactly from the total, corroborated by the bonus.

Worked comma-inserted-total false flag (iter 3 stage 3): score `327,879` is correct; the total region renders `393,454Pt` but OCR'd `3935454` (comma read as `5`). Deleting the digit at index 3 gives `393454`, and `327879 + floor(327879/5) = 327879 + 65575 = 393454` — so the literal score satisfies the checksum under the one-digit-deletion total and the stage is `Ok`, not flagged.


## Interfaces and Dependencies

No new crates. Reuse `regex`, `image`, `anyhow`, and the embedded Tesseract already in the tree. The function signatures are unchanged; only their internals gain candidates/guards:

In `src/ocr/engine.rs`:

    fn parse_single_number(raw: &str, anchor_plus: bool) -> Option<u32>;   // anchor_plus && no '+' => None

In `src/ocr/reconcile.rs`:

    pub fn reconcile_stage(ocr_scores: [u32;3], total: Option<u32>, bonus: Option<u32>) -> ([u32;3], Recovery);
    pub fn reconstruct_from_digits(digits: &str, total: Option<u32>, bonus: Option<u32>) -> Option<([u32;3], Recovery)>;
    fn candidates(v: u32) -> Vec<u32>;   // adds leading-digit-replacement for impossible 7-digit v
    const MAX_SCORE: u32 = 3_000_000;

The public `ocr_screenshot`, `StageReadout`, `Recovery`, and the CSV schema are unchanged.


## Revision Note

2026-06-23: Initial authoring. Captures the field-run diagnosis of `target/release/output/20260623_232320/` — that the overwhelming majority of `recovery=flagged` rows are false flags caused by the stage total's thousands comma OCR'ing as a digit and the bonus badge's leading `+` OCR'ing as `4`, plus eight genuinely-wrong single-character scores (impossible leading-`1`→`4`, dropped leading digit, split token) that the prior overlap-only recovery could not reach. Defines the four-milestone fix: require a literal `+` for a bonus (M1); make the checksum tolerant of one comma-inserted total digit and recover impossible ≥3M values (M2); trust/corroborate/recover non-collision-prone single-character stages instead of flagging correct scores (M3); recover the split-token single character and confirm end-to-end (M4). Reason for the design: harden the checksum *inputs* so the exact checksum keeps catching genuine errors while no longer punishing correct scores for noisy mini-numbers, and extend recovery to the lone-character cases the overlap plan never covered — every change guarded by the exact checksum so a mis-tuned candidate degrades to a flag, never a silent error.
