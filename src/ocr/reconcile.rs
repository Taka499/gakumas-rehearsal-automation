//! Checksum-based reconstruction of per-character scores corrupted by the
//! overlapping-million OCR failure (see docs/EXECPLAN_OVERLAP_SCORE_RECOVERY.md).
//!
//! When two adjacent per-character scores are both >= 1,000,000 the game renders
//! them so close that the right number's leading "1" overlaps the left number's
//! last digit. OCR then drops the right "1" and may misread the left number's
//! units digit. The screen also shows an isolated stage total and a bonus badge,
//! and the game guarantees the exact identity
//!
//!     stage_total = c1 + c2 + c3 + floor(max(c1, c2, c3) / 5)
//!
//! (the bonus is `floor(max/5)`), so `reconcile_stage` reconstructs the true
//! scores from the total alone via a small exhaustive search, using the bonus
//! only as an optional cross-check.

/// Confidence of a stage's reconstructed scores.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Recovery {
    /// Read cleanly (or the checksum confirmed the raw read with no edits).
    Ok,
    /// One or more values were repaired and the result is trusted.
    Repaired,
    /// Ambiguous or unverifiable: stored as best-effort, needs human review.
    Flagged,
}

/// Upper bound on a single per-character score (exclusive), used to prune
/// reconstruction candidates and bound the dropped-leading-digit search.
///
/// The original invariant was "< 2,000,000" (a 7-digit score is always
/// `1,XXX,XXX`). Scores have since approached that ceiling, so this is raised to
/// 3,000,000 to keep collision recovery working for `2,XXX,XXX` values (the
/// dropped leading digit can now be 1 or 2). 3M is chosen deliberately: it stays
/// below the points where the bonus (`floor(max/5)`, here < 600,000, a clean
/// 6-digit number) or the total (here < ~9.6M, still 7 digits) would overflow
/// their digit guards. Note a *clean* read above this bound is unaffected — the
/// raw value is always a candidate and passes the checksum at cost 0 — only
/// in-collision recovery of a score >= 3M would fall back to a flag. Raise this
/// (and revisit the bonus/total guards) if scores ever exceed it.
const MAX_SCORE: u32 = 3_000_000;

/// Reconstructs one stage's three per-character scores from the OCR'd scores
/// plus the optional stage total and bonus.
///
/// See the module docs and the ExecPlan for the full algorithm. In brief:
/// validate the total/bonus (step 0), then if the total is usable run an
/// exhaustive small search over per-slot candidate values (raw, optionally
/// +1,000,000 to restore a dropped leading "1", each with its units digit
/// replaced 0..=9), keep combinations that satisfy the checksum exactly, and
/// pick the one with the lowest *corruption-aware* cost. With no usable total,
/// fall back to a conservative structural-only pass.
pub fn reconcile_stage(
    ocr_scores: [u32; 3],
    total: Option<u32>,
    bonus: Option<u32>,
) -> ([u32; 3], Recovery) {
    let total_provided = total.is_some();

    // --- Step 0: validate inputs before trusting them. ---
    let max_raw = ocr_scores.iter().copied().max().unwrap_or(0);
    let total_ok = total.filter(|&t| t <= 9_999_999 && t >= max_raw && t > 0);
    let bonus_ok = bonus.filter(|&b| {
        b < 1_000_000 && total_ok.map_or(true, |t| b < t)
    });

    let Some(total) = total_ok else {
        return structural_only(ocr_scores, total_provided, bonus_ok);
    };

    // --- Steps 2–3: build candidates and collect checksum-satisfying combos. ---
    let cand = [
        candidates(ocr_scores[0]),
        candidates(ocr_scores[1]),
        candidates(ocr_scores[2]),
    ];

    let mut solutions: Vec<([u32; 3], u32)> = Vec::new(); // (combo, cost)
    for &a in &cand[0] {
        for &b in &cand[1] {
            for &c in &cand[2] {
                let combo = [a, b, c];
                if !physically_valid(combo, ocr_scores) {
                    continue;
                }
                let max = a.max(b).max(c);
                if a + b + c + max / 5 == total {
                    solutions.push((combo, cost(combo, ocr_scores)));
                }
            }
        }
    }

    // --- Step 5 (zero solutions): the total was subtly wrong; flag. ---
    if solutions.is_empty() {
        return (ocr_scores, Recovery::Flagged);
    }

    // --- Step 4: pick the minimum-cost combo, using the bonus to break ties. ---
    let min_cost = solutions.iter().map(|&(_, c)| c).min().unwrap();
    let mut best: Vec<[u32; 3]> = solutions
        .iter()
        .filter(|&&(_, c)| c == min_cost)
        .map(|&(combo, _)| combo)
        .collect();

    if best.len() > 1 {
        if let Some(b) = bonus_ok {
            let corroborated: Vec<[u32; 3]> = best
                .iter()
                .copied()
                .filter(|combo| derived_bonus(*combo) == b)
                .collect();
            if !corroborated.is_empty() {
                best = corroborated;
            }
        }
    }

    // Deterministic order so a residual tie returns a stable best-effort value.
    best.sort_unstable();
    let chosen = best[0];

    let tie = best.len() > 1;
    let bonus_disagrees = bonus_ok.map_or(false, |b| derived_bonus(chosen) != b);

    let recovery = if tie || bonus_disagrees {
        Recovery::Flagged
    } else if min_cost == 0 {
        Recovery::Ok
    } else {
        Recovery::Repaired
    };

    (chosen, recovery)
}

/// Rejects physically-impossible reconstructions. A slot counts as "restored"
/// (had a dropped leading digit) when its raw value was < 1,000,000 but the
/// reconstructed value is >= 1,000,000. Per invariants 2–3 a leading digit is
/// only ever dropped at a collision between two adjacent >= 1,000,000 scores, so:
///   - the leftmost slot can never be restored (it has no left neighbour), and
///   - a restored slot's left neighbour must itself be >= 1,000,000.
/// This eliminates spurious million-trades (e.g. reading `…,1200000,1100000` as
/// `…,200000,2100000`, which would require restoring a slot whose left neighbour
/// is sub-million).
fn physically_valid(combo: [u32; 3], raw: [u32; 3]) -> bool {
    for i in 0..3 {
        let restored = raw[i] < 1_000_000 && combo[i] >= 1_000_000;
        if restored && (i == 0 || combo[i - 1] < 1_000_000) {
            return false;
        }
    }
    true
}

/// `floor(max(combo) / 5)` — the bonus the game would render for this combo.
fn derived_bonus(combo: [u32; 3]) -> u32 {
    combo.iter().copied().max().unwrap_or(0) / 5
}

/// Builds the candidate value set for one slot (ExecPlan step 2).
///
/// Always includes the raw value. If the raw is a plausible victim of a dropped
/// leading digit (>= 1000 and < 1,000,000), also includes raw + d*1,000,000 for
/// each leading digit `d` that keeps the result below `MAX_SCORE` (d = 1 for a
/// `1,XXX,XXX` victim, d = 2 for a `2,XXX,XXX` one). For every base >= 100,000,
/// includes the ten units-digit variants (covers the corrupted left-units
/// digit). Generated variants are capped to < `MAX_SCORE`; the raw is always
/// kept (so a clean read above the cap still survives). A dash slot (0)
/// contributes only {0}.
fn candidates(v: u32) -> Vec<u32> {
    if v == 0 {
        return vec![0];
    }

    let mut bases = vec![v];
    if (1_000..1_000_000).contains(&v) {
        for d in 1..=9u32 {
            let restored = v + d * 1_000_000;
            if restored >= MAX_SCORE {
                break;
            }
            bases.push(restored);
        }
    }

    let mut out = vec![v];
    for &b in &bases {
        if b < MAX_SCORE {
            out.push(b);
        }
        if b >= 100_000 {
            let floor10 = (b / 10) * 10;
            for d in 0..=9u32 {
                let variant = floor10 + d;
                if variant < MAX_SCORE {
                    out.push(variant);
                }
            }
        }
    }

    out.sort_unstable();
    out.dedup();
    out
}

/// Corruption-aware cost of a reconstructed combo relative to the raw OCR
/// (ExecPlan step 4). NOT a plain edit count: per invariant 3, an overlap
/// restores a leading million on the RIGHT operand of a junction and corrupts
/// only the units digit of its LEFT neighbour. So:
///   - +1 for each slot given a restored leading million,
///   - a units-digit change costs +1 only when the slot is immediately LEFT of
///     a restored slot (the expected victim), else +3.
fn cost(chosen: [u32; 3], raw: [u32; 3]) -> u32 {
    let restored = [
        raw[0] < 1_000_000 && chosen[0] >= 1_000_000,
        raw[1] < 1_000_000 && chosen[1] >= 1_000_000,
        raw[2] < 1_000_000 && chosen[2] >= 1_000_000,
    ];

    let mut c = 0u32;
    for i in 0..3 {
        if restored[i] {
            c += 1;
        }
        if chosen[i] % 10 != raw[i] % 10 {
            let left_of_restored = i + 1 < 3 && restored[i + 1];
            c += if left_of_restored { 1 } else { 3 };
        }
    }
    c
}

/// Structural-only fallback when no usable total is available (ExecPlan step 6).
///
/// Without the checksum we cannot recover from the numeric values alone — the
/// lost-million markers (a leading-zero group, or provenance of an over-split
/// token) live in the OCR *text*, which is not available here — so the raw
/// scores are returned as best-effort. The flag distinguishes the two reasons we
/// got here: a total that was *provided but rejected* as garbage is suspicious
/// (Flagged), as is a present bonus that disagrees with the raw maximum; an
/// absent total with a corroborating (or absent) bonus is treated as a clean
/// read (Ok).
fn structural_only(
    ocr_scores: [u32; 3],
    total_provided: bool,
    bonus_ok: Option<u32>,
) -> ([u32; 3], Recovery) {
    let bonus_disagrees = bonus_ok.map_or(false, |b| derived_bonus(ocr_scores) != b);

    // An overlap only happens between two adjacent >= 1,000,000 scores, so a
    // stage with a >= 1M slot and at least two non-zero slots is collision-prone.
    // Without a usable total we cannot verify the sum, and the bonus only pins
    // the maximum — a million lost from a NON-max slot is invisible to it (this
    // is exactly how a broken read once slipped through as Ok). Flag such stages
    // rather than trust them.
    let nonzero = ocr_scores.iter().filter(|&&s| s > 0).count();
    let has_million = ocr_scores.iter().any(|&s| s >= 1_000_000);
    let collision_prone = has_million && nonzero >= 2;

    let recovery = if total_provided || bonus_disagrees || collision_prone {
        Recovery::Flagged
    } else {
        Recovery::Ok
    };
    (ocr_scores, recovery)
}

/// Reconstructs a stage's scores directly from the score row's raw digit stream
/// (all commas/spaces removed), guided by the total and bonus checksums.
///
/// This is the fallback for when the comma-based `reconcile_stage` cannot recover
/// — chiefly the **two-collision** case (three adjacent >= 1,000,000 scores),
/// where the colliding glyphs scramble Tesseract's comma grouping so badly that
/// the per-number tokenization loses interior digits (e.g. `1,206,537` is split
/// as `206,53` + `7`). The digits themselves usually survive in order, so this
/// re-partitions the raw stream into `k` consecutive scores, optionally restoring
/// a dropped leading "1" on any non-first 6-digit part, searches the units digit
/// of each junction's left neighbour, and keeps only partitions that satisfy the
/// exact total checksum (and the bonus, when present). Returns `None` when no
/// partition satisfies the checksum.
///
/// Requires a usable `total`; without it the sum cannot be pinned.
pub fn reconstruct_from_digits(
    digits: &str,
    total: Option<u32>,
    bonus: Option<u32>,
) -> Option<([u32; 3], Recovery)> {
    let total = total?;
    if !(1..=9_999_999).contains(&total) {
        return None;
    }
    let ds: Vec<u8> = digits.bytes().filter(u8::is_ascii_digit).collect();
    let n = ds.len();
    if n == 0 || n > 21 {
        return None;
    }
    let bonus_ok = bonus.filter(|&b| b < 1_000_000 && b < total);

    // (combo, cost) for every partition that satisfies the checksum.
    let mut solutions: Vec<([u32; 3], u32)> = Vec::new();

    for k in 1..=3usize.min(n) {
        for comp in compositions(n, k, 1, 7) {
            // Slice the stream into k parts.
            let mut parts: [&[u8]; 3] = [&[], &[], &[]];
            let mut off = 0;
            for i in 0..k {
                parts[i] = &ds[off..off + comp[i]];
                off += comp[i];
            }

            // Per-part value options: the literal value, plus — for a non-first
            // 6-digit part that lost a leading digit — `d,XXX,XXX` for each
            // leading digit `d` (1..) keeping it below MAX_SCORE. A 7-digit
            // literal may start with any digit now (scores can exceed 2M).
            let mut popts: [Vec<(u32, bool)>; 3] = [Vec::new(), Vec::new(), Vec::new()];
            let mut comp_valid = true;
            for i in 0..k {
                let v: u32 = std::str::from_utf8(parts[i]).unwrap().parse().unwrap_or(u32::MAX);
                if v < MAX_SCORE {
                    popts[i].push((v, false));
                }
                if i > 0 && comp[i] == 6 {
                    for d in 1..=9u32 {
                        let rv = d * 1_000_000 + v;
                        if rv >= MAX_SCORE {
                            break;
                        }
                        popts[i].push((rv, true));
                    }
                }
                if popts[i].is_empty() {
                    comp_valid = false;
                    break;
                }
            }
            if !comp_valid {
                continue;
            }

            // Cartesian product of the per-part options (mixed-radix counter).
            let sizes: [usize; 3] = [
                popts[0].len().max(1),
                popts[1].len().max(1),
                popts[2].len().max(1),
            ];
            let combo_count: usize = (0..k).map(|i| sizes[i]).product();
            for sel in 0..combo_count {
                let mut base = [0u32; 3];
                let mut restored = [false; 3];
                let mut x = sel;
                for i in 0..k {
                    let (val, isr) = popts[i][x % sizes[i]];
                    x /= sizes[i];
                    base[i] = val;
                    restored[i] = isr;
                }

                // A restored part needs a >= 1M left neighbour (a collision
                // partner); reject physically-impossible million-trades.
                if (0..k).any(|i| restored[i] && (i == 0 || base[i - 1] < 1_000_000)) {
                    continue;
                }

                // Units corruption is confined to the left neighbour of a
                // restored (malignant-junction) part.
                let corrupt: Vec<usize> = (0..k).filter(|&i| i + 1 < k && restored[i + 1]).collect();
                let restores = restored.iter().filter(|&&r| r).count() as u32;

                for a in 0..10usize.pow(corrupt.len() as u32) {
                    let mut c = base;
                    let mut x = a;
                    let mut units_changes = 0u32;
                    let mut ok = true;
                    for &slot in &corrupt {
                        let d = (x % 10) as u32;
                        x /= 10;
                        let nv = (base[slot] / 10) * 10 + d;
                        if nv >= MAX_SCORE {
                            ok = false;
                            break;
                        }
                        if d != base[slot] % 10 {
                            units_changes += 1;
                        }
                        c[slot] = nv;
                    }
                    if !ok {
                        continue;
                    }

                    let max = c[0].max(c[1]).max(c[2]);
                    if c[0] + c[1] + c[2] + max / 5 != total {
                        continue;
                    }
                    if let Some(b) = bonus_ok {
                        if max / 5 != b {
                            continue;
                        }
                    }
                    solutions.push((c, restores + units_changes));
                }
            }
        }
    }

    if solutions.is_empty() {
        return None;
    }

    let min_cost = solutions.iter().map(|&(_, c)| c).min().unwrap();
    let mut at_min: Vec<[u32; 3]> = solutions
        .iter()
        .filter(|&&(_, c)| c == min_cost)
        .map(|&(combo, _)| combo)
        .collect();
    at_min.sort_unstable();
    at_min.dedup();

    let chosen = at_min[0];
    let recovery = if at_min.len() > 1 {
        Recovery::Flagged // genuinely ambiguous partition
    } else if min_cost == 0 {
        Recovery::Ok
    } else {
        Recovery::Repaired
    };
    Some((chosen, recovery))
}

/// All ways to write `n` as `k` ordered parts, each in `[min, max]`.
fn compositions(n: usize, k: usize, min: usize, max: usize) -> Vec<Vec<usize>> {
    let mut res = Vec::new();
    let mut cur = Vec::with_capacity(k);
    fn rec(n: usize, k: usize, min: usize, max: usize, cur: &mut Vec<usize>, res: &mut Vec<Vec<usize>>) {
        if k == 0 {
            if n == 0 {
                res.push(cur.clone());
            }
            return;
        }
        for len in min..=max.min(n) {
            let rem = n - len;
            if rem < (k - 1) * min || rem > (k - 1) * max {
                continue;
            }
            cur.push(len);
            rec(rem, k - 1, min, max, cur, res);
            cur.pop();
        }
    }
    rec(n, k, min, max, &mut cur, &mut res);
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- The four real samples (ground truth from the ExecPlan). ---

    #[test]
    fn test_reconcile_102842_one_junction_all_three_million() {
        let (scores, rec) =
            reconcile_stage([1172669, 161196, 1093518], Some(3661912), Some(234533));
        assert_eq!(scores, [1172665, 1161196, 1093518]);
        assert_eq!(rec, Recovery::Repaired);
    }

    #[test]
    fn test_reconcile_003_mode_b_overflow_dash() {
        let (scores, rec) = reconcile_stage([1327534, 151661, 0], Some(2744700), Some(265506));
        assert_eq!(scores, [1327533, 1151661, 0]);
        assert_eq!(rec, Recovery::Repaired);
    }

    #[test]
    fn test_reconcile_005_leading_zero_victim() {
        let (scores, rec) = reconcile_stage([1083344, 62741, 0], Some(2362759), Some(216669));
        assert_eq!(scores, [1083349, 1062741, 0]);
        assert_eq!(rec, Recovery::Repaired);
    }

    #[test]
    fn test_reconcile_102623_regression_guard() {
        // Already-correct read; the checksum confirms it with zero edits.
        let (scores, rec) =
            reconcile_stage([912127, 1171024, 1004816], Some(3322171), Some(234204));
        assert_eq!(scores, [912127, 1171024, 1004816]);
        assert_eq!(rec, Recovery::Ok);
    }

    // --- Total-only tier: recovery must work without the bonus. ---

    #[test]
    fn test_reconcile_total_only_recovers_exactly() {
        let (scores, rec) = reconcile_stage([1327534, 151661, 0], Some(2744700), None);
        assert_eq!(scores, [1327533, 1151661, 0]);
        assert_eq!(rec, Recovery::Repaired);
    }

    // --- Cost model: the asymmetric cost breaks a plain-edit tie. ---

    #[test]
    fn test_asymmetric_cost_breaks_unit_trade_tie() {
        // Both combos satisfy the checksum for total 2,744,700 at plain edit
        // count 2 (restore + one units edit). The asymmetric cost charges the
        // units edit on the left neighbour (slot 0) only 1, but on the restored
        // slot itself (slot 1) 3, so the correct combo wins.
        let raw = [1327534, 151661, 0];
        let correct = [1327533, 1151661, 0]; // edits slot 0 units
        let wrong = [1327534, 1151660, 0]; // edits slot 1 units

        // Both genuinely satisfy the checksum (the tie is real).
        for combo in [correct, wrong] {
            let max = combo.iter().copied().max().unwrap();
            assert_eq!(combo[0] + combo[1] + combo[2] + max / 5, 2744700);
        }
        // Plain edit count ties them; the asymmetric cost does not.
        assert!(cost(correct, raw) < cost(wrong, raw));

        let (scores, rec) = reconcile_stage(raw, Some(2744700), None);
        assert_eq!(scores, correct);
        assert_eq!(rec, Recovery::Repaired);
    }

    // --- Over-detection guards: bad inputs flag, never silently corrupt. ---

    #[test]
    fn test_over_detected_bonus_ignored() {
        // 8-digit bonus → demoted to None in step 0; the total alone still
        // recovers the correct scores.
        let (scores, rec) =
            reconcile_stage([1327534, 151661, 0], Some(2744700), Some(23545335));
        assert_eq!(scores, [1327533, 1151661, 0]);
        assert_eq!(rec, Recovery::Repaired);
    }

    #[test]
    fn test_over_detected_total_rejected() {
        // 8-digit total → demoted to None; drops to structural-only. Must not
        // emit any checksum-derived (units-edited or million-restored) value.
        let (scores, rec) =
            reconcile_stage([1327534, 151661, 0], Some(27447007), Some(265506));
        assert_eq!(scores, [1327534, 151661, 0]);
        assert_eq!(rec, Recovery::Flagged);
    }

    #[test]
    fn test_unreachable_total_flags() {
        // A wrong total that no candidate combination can satisfy degrades to a
        // flagged best-effort read (the exact-checksum requirement is the final
        // guard). Note: an off-by-one total (2,744,701) is NOT usable here — it
        // is exactly satisfiable by the restore-only combo with slot 0's units
        // left uncorrected, so a compensating units error hides it. We use a
        // clearly-unreachable total instead.
        let (scores, rec) = reconcile_stage([1327534, 151661, 0], Some(2744600), None);
        assert_eq!(scores, [1327534, 151661, 0]);
        assert_eq!(rec, Recovery::Flagged);
    }

    #[test]
    fn test_off_by_one_total_is_satisfiable() {
        // Documents the compensating-error limitation: total off by +1 plus the
        // uncorrected +1 units error in slot 0 cancel, so this is "recovered"
        // (to the OCR'd, units-uncorrected value) rather than flagged.
        let (scores, _rec) = reconcile_stage([1327534, 151661, 0], Some(2744701), None);
        assert_eq!(scores, [1327534, 1151661, 0]);
    }

    #[test]
    fn test_no_total_single_score_is_ok() {
        // No total, a single non-zero sub-million score (a normal one-character
        // stage) cannot have an overlap → Ok even without verification.
        let (scores, rec) = reconcile_stage([450190, 0, 0], None, Some(90038));
        assert_eq!(scores, [450190, 0, 0]);
        assert_eq!(rec, Recovery::Ok);
    }

    #[test]
    fn test_no_total_collision_prone_flags() {
        // No total and a >= 1M slot with other non-zero slots: the sum is
        // unverifiable and the bonus only pins the max, so a million lost from a
        // non-max slot would be invisible. Flag rather than trust.
        let (scores, rec) =
            reconcile_stage([1240514, 178565, 455013], None, Some(248102));
        assert_eq!(scores, [1240514, 178565, 455013]);
        assert_eq!(rec, Recovery::Flagged);
    }

    // --- Digit-stream reconstruction (two-collision and friends). ---

    #[test]
    fn test_reconstruct_two_collisions_iter9() {
        // Three overlapping >= 1M scores; OCR commas scrambled. Raw digit stream
        // from "1,314,249,,206,53 71,103,897". True: 1,314,245 / 1,206,537 / 1,103,897.
        let (scores, rec) =
            reconstruct_from_digits("13142492065371103897", Some(3887528), Some(262849)).unwrap();
        assert_eq!(scores, [1314245, 1206537, 1103897]);
        assert_eq!(rec, Recovery::Repaired);
    }

    #[test]
    fn test_reconstruct_single_collision_iter18() {
        // One collision (c1|c2), c3 sub-million. Stream from
        // "1,240,514,,178,565 455,013". True: 1,240,513 / 1,178,565 / 455,013.
        let (scores, rec) =
            reconstruct_from_digits("1240514178565455013", Some(3122193), Some(248102)).unwrap();
        assert_eq!(scores, [1240513, 1178565, 455013]);
        assert_eq!(rec, Recovery::Repaired);
    }

    #[test]
    fn test_reconstruct_clean_three_million_iter102623() {
        // Already-correct three-score read confirms via the checksum at cost 0.
        let (scores, rec) =
            reconstruct_from_digits("91212711710241004816", Some(3322171), Some(234204)).unwrap();
        assert_eq!(scores, [912127, 1171024, 1004816]);
        assert_eq!(rec, Recovery::Ok);
    }

    #[test]
    fn test_reconstruct_requires_total() {
        assert!(reconstruct_from_digits("13142492065371103897", None, Some(262849)).is_none());
    }

    #[test]
    fn test_reconstruct_wrong_total_no_solution() {
        // A total no partition can satisfy yields None (caller keeps the flag).
        assert!(reconstruct_from_digits("13142492065371103897", Some(9999999), Some(262849)).is_none());
    }

    // --- Scores at/above 2,000,000 (relaxed invariant 1). ---

    #[test]
    fn test_clean_two_million_no_collision_ok() {
        // A clean >= 2M read (no overlap) is confirmed by the checksum at cost 0.
        let (scores, rec) =
            reconcile_stage([2134567, 500000, 300000], Some(3361480), Some(426913));
        assert_eq!(scores, [2134567, 500000, 300000]);
        assert_eq!(rec, Recovery::Ok);
    }

    #[test]
    fn test_collision_with_two_million_neighbour() {
        // c1 is 2,134,567 (its units misread 7->9); c2 lost its leading "1".
        let (scores, rec) =
            reconcile_stage([2134569, 200000, 0], Some(3761480), Some(426913));
        assert_eq!(scores, [2134567, 1200000, 0]);
        assert_eq!(rec, Recovery::Repaired);
    }

    #[test]
    fn test_collision_restored_victim_is_two_million() {
        // The restored (right-operand) victim is 2,134,567 — it lost a leading
        // "2", not "1" (d=2 restore). c1's units were corrupted 0->3.
        let (scores, rec) =
            reconcile_stage([1500003, 134567, 0], Some(4061480), Some(426913));
        assert_eq!(scores, [1500000, 2134567, 0]);
        assert_eq!(rec, Recovery::Repaired);
    }

    #[test]
    fn test_reconstruct_two_collisions_with_two_million() {
        // Three >= 1M scores, c1 >= 2M, both junctions dropped a leading "1".
        let (scores, rec) =
            reconstruct_from_digits("2134567200000100000", Some(4861480), Some(426913)).unwrap();
        assert_eq!(scores, [2134567, 1200000, 1100000]);
        assert_eq!(rec, Recovery::Repaired);
    }

    #[test]
    fn test_candidates_include_restore_and_units() {
        // Leading-zero victim: 62741 must yield 1,062,741 as a candidate.
        let c = candidates(62741);
        assert!(c.contains(&62741));
        assert!(c.contains(&1062741));
        // Units variants exist around the restored base.
        assert!(c.contains(&1062740) && c.contains(&1062749));
        // Capped below 2,000,000.
        assert!(c.iter().all(|&x| x < MAX_SCORE));
    }
}
