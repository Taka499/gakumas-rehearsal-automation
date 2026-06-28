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

/// How a candidate value was derived from the raw OCR value — its corruption
/// provenance. Centralising this (rather than re-inferring "was a million
/// restored?" from magnitudes at each use site, as the code used to) is what lets
/// the cost model and the physical-validity guard agree on what kind of edit each
/// candidate represents, and is the seam through which new corruption modes are
/// added in one place.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BaseKind {
    /// The raw OCR value, high-order digits unchanged.
    Raw,
    /// A dropped leading "1," (or "2,") restored: `raw + d*1_000_000`. The classic
    /// overlap victim (incl. the leading-zero-group case `062,741` -> `1,062,741`).
    /// Only physically valid at a junction — its left neighbour must be >= 1M.
    Million,
    /// A dropped leading *non-million* digit restored by prepending it at the next
    /// decimal place, e.g. `52,517` -> `852,517` (the "leading 8" failure), or the
    /// impossible-7-digit `1,`->`4` substitution `4,177,174` -> `1,177,174`. A
    /// plain OCR drop/substitution, position-independent (no junction required).
    Prepend,
}

/// A candidate value for one slot, tagged with its provenance and whether its
/// units digit was changed relative to the raw value.
#[derive(Clone, Copy)]
struct Cand {
    value: u32,
    kind: BaseKind,
    units_edited: bool,
}

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
    // Use each slot's *plausible magnitude* as the floor: an impossible raw value
    // (e.g. 4,177,174 from a leading-"1"->"4" misread) is larger than its true,
    // smaller total and would otherwise reject it. Its plausible magnitude
    // (1,177,174) is what the total must clear.
    let max_raw = ocr_scores
        .iter()
        .copied()
        .map(plausible_floor)
        .max()
        .unwrap_or(0);
    let total_ok = total.filter(|&t| t <= 9_999_999 && t >= max_raw && t > 0);
    let bonus_ok = bonus.filter(|&b| {
        b < 1_000_000 && total_ok.map_or(true, |t| b < t)
    });

    let Some(total) = total_ok else {
        // No usable total. Before giving up, try a bonus-driven repair: the bonus
        // pins floor(max/5), so a unique physically-valid leading-"1" restore that
        // makes the restored slot the max with floor(max/5) == bonus is strong
        // evidence (kept best-effort / Flagged since the bonus can't verify the
        // non-max slots). Otherwise fall back to the conservative structural pass.
        if let Some(rep) = bonus_driven_repair(ocr_scores, bonus_ok) {
            return (rep, Recovery::Flagged);
        }
        return structural_only(ocr_scores, total_provided, bonus_ok);
    };

    // --- Steps 2–3: build candidates and collect checksum-satisfying combos. ---
    let cand = [
        slot_candidates(ocr_scores[0]),
        slot_candidates(ocr_scores[1]),
        slot_candidates(ocr_scores[2]),
    ];

    // The OCR'd total may carry one spuriously-inserted digit (the thousands
    // comma read as a digit, e.g. "393,454" -> "3935454"). Try the literal total
    // plus every one-digit deletion of it; a deletion-derived total carries a
    // small penalty so the literal always wins a genuine tie.
    let total_set = total_candidates(total, max_raw);

    let mut solutions: Vec<([u32; 3], u32)> = Vec::new(); // (combo, cost)
    for &(t, penalty) in &total_set {
        for &a in &cand[0] {
            for &b in &cand[1] {
                for &c in &cand[2] {
                    let combo = [a.value, b.value, c.value];
                    let kinds = [a.kind, b.kind, c.kind];
                    if !physically_valid(combo, kinds) {
                        continue;
                    }
                    let max = combo[0].max(combo[1]).max(combo[2]);
                    if combo[0] + combo[1] + combo[2] + max / 5 == t {
                        let units = [a.units_edited, b.units_edited, c.units_edited];
                        solutions.push((combo, cost(kinds, units) + penalty));
                    }
                }
            }
        }
    }

    // --- Step 5 (zero solutions): the total was subtly wrong, or the stage is a
    // single character whose total/bonus mis-OCR'd. Try a bonus-driven repair
    // first (a unique leading-"1" restore the bonus corroborates), then the
    // non-collision-prone fallback, rather than reflexively flagging. ---
    if solutions.is_empty() {
        if let Some(rep) = bonus_driven_repair(ocr_scores, bonus_ok) {
            return (rep, Recovery::Flagged);
        }
        return resolve_without_checksum(ocr_scores, total_set, bonus_ok);
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

    // Classify by whether the scores actually changed, not by raw cost: a clean
    // score confirmed only under a one-digit-deleted total carries a non-zero
    // selection penalty but was not edited, so it is `Ok`, not `Repaired`.
    //
    // A cost-0, unedited read that the total confirms is trusted as `Ok` even if
    // the bonus disagrees: the bonus is only a cross-check (it equals
    // `floor(max/5)`) and over-detects digits exactly like the total's comma
    // does, so a noisy bonus must not flag a read the total already confirms. The
    // bonus still guards *edited* (cost > 0) reconstructions below, where it
    // genuinely disambiguates an uncertain repair. (A million lost from a non-max
    // slot — the case the bonus was meant to catch — makes the total *not* match
    // at cost 0, so it never reaches this branch.)
    let recovery = if chosen == ocr_scores && min_cost == 0 && !tie {
        Recovery::Ok
    } else if tie || bonus_disagrees {
        Recovery::Flagged
    } else if chosen == ocr_scores {
        Recovery::Ok
    } else {
        Recovery::Repaired
    };

    (chosen, recovery)
}

/// Rejects physically-impossible reconstructions. A `Million` restore (a dropped
/// leading "1,"/"2,") only ever happens at a collision between two adjacent
/// >= 1,000,000 scores (invariants 2–3), so:
///   - the leftmost slot can never be `Million`-restored (no left neighbour), and
///   - a `Million`-restored slot's left neighbour must itself be >= 1,000,000.
/// This eliminates spurious million-trades (e.g. reading `…,1200000,1100000` as
/// `…,200000,2100000`). A `Prepend` restore is a plain leading-digit drop, not a
/// junction artifact, so it carries no neighbour constraint.
fn physically_valid(combo: [u32; 3], kinds: [BaseKind; 3]) -> bool {
    for i in 0..3 {
        if kinds[i] == BaseKind::Million && (i == 0 || combo[i - 1] < 1_000_000) {
            return false;
        }
    }
    true
}

/// Best-effort repair driven by the bonus when the exact-total checksum found no
/// solution (the total mis-OCR'd). The bonus equals `floor(max/5)`, so if exactly
/// one physically-valid leading-"1"/"2" restore makes the restored slot the max
/// with `floor(max/5) == bonus`, apply it (leaving the other slots raw). Returns
/// the repaired combo, or `None` if zero or several candidates qualify.
///
/// Always best-effort (the caller marks it `Flagged`): the bonus only pins the
/// max, so a units corruption on a *non-max* slot cannot be fixed here — it just
/// gets the obvious collision victim restored, which is far closer to truth than
/// keeping the sub-million raw, and a human still reviews the flagged row.
fn bonus_driven_repair(raw: [u32; 3], bonus: Option<u32>) -> Option<[u32; 3]> {
    let bonus = bonus?;
    let mut hit: Option<[u32; 3]> = None;
    for i in 0..3 {
        // Only a junction million-restore: a sub-million slot (not the leftmost)
        // whose left neighbour is >= 1M (a real collision partner).
        if i == 0 || !(1_000..1_000_000).contains(&raw[i]) || raw[i - 1] < 1_000_000 {
            continue;
        }
        for d in 1..=2u32 {
            let restored = raw[i] + d * 1_000_000;
            if restored >= MAX_SCORE {
                break;
            }
            let mut combo = raw;
            combo[i] = restored;
            let max = combo[0].max(combo[1]).max(combo[2]);
            if max == restored && max / 5 == bonus {
                if hit.is_some() && hit != Some(combo) {
                    return None; // ambiguous — refuse to guess
                }
                hit = Some(combo);
            }
        }
    }
    hit
}

/// `floor(max(combo) / 5)` — the bonus the game would render for this combo.
fn derived_bonus(combo: [u32; 3]) -> u32 {
    combo.iter().copied().max().unwrap_or(0) / 5
}

/// Builds the provenance-tagged candidate set for one slot (ExecPlan step 2,
/// generalised). Each base value is paired with its [`BaseKind`]; for every base
/// >= 100,000 the ten units-digit variants are added (covers the corrupted
/// left-units digit). All values are capped to < `MAX_SCORE`; the raw is always
/// kept (so a clean read above the cap still survives). A dash slot (0)
/// contributes only `{0}`. The bases are:
///   - `Raw`: the OCR value itself.
///   - `Million`: `raw + d*1,000,000` (d = 1,2) when raw is in [1000, 1,000,000)
///     — a dropped leading "1,"/"2," at a collision (incl. leading-zero-group
///     victims like 62,741 -> 1,062,741).
///   - `Prepend`: a dropped *non-million* leading digit restored at the next
///     decimal place, `d*10^len(raw) + raw` (d = 1..9), when raw is in
///     [1000, 100,000) — e.g. the "leading 8" drop 52,517 -> 852,517. Distinct in
///     value from `Million` (which always adds >= 1,000,000), so the two never
///     collide. Also covers the impossible-7-digit "1,"->"4" substitution
///     (4,177,174 -> 1,177,174 / 2,177,174), which needs no junction neighbour.
fn slot_candidates(v: u32) -> Vec<Cand> {
    if v == 0 {
        return vec![Cand { value: 0, kind: BaseKind::Raw, units_edited: false }];
    }
    let raw_units = v % 10;

    let mut bases: Vec<(u32, BaseKind)> = vec![(v, BaseKind::Raw)];
    if (1_000..1_000_000).contains(&v) {
        for d in 1..=2u32 {
            let r = v + d * 1_000_000;
            if r >= MAX_SCORE {
                break;
            }
            bases.push((r, BaseKind::Million));
        }
    }
    if (1_000..100_000).contains(&v) {
        let place = 10u32.pow(v.to_string().len() as u32);
        for d in 1..=9u32 {
            let r = d * place + v;
            if r >= MAX_SCORE {
                break;
            }
            bases.push((r, BaseKind::Prepend));
        }
    }
    if (MAX_SCORE..10_000_000).contains(&v) {
        let tail = v % 1_000_000;
        bases.push((1_000_000 + tail, BaseKind::Prepend));
        bases.push((2_000_000 + tail, BaseKind::Prepend));
    }
    // A valid-looking 2,XXX,XXX whose leading "1" was misread as "2" (e.g.
    // 2,396,184 OCR'd for a true 1,396,184). Offer the 1,XXX,XXX reading; the
    // exact total disambiguates — a genuine 2M score keeps its raw value at cost
    // 0, so the swap only wins when the checksum actually demands it.
    if (2_000_000..MAX_SCORE).contains(&v) {
        bases.push((1_000_000 + v % 1_000_000, BaseKind::Prepend));
    }

    let mut out: Vec<Cand> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut push = |out: &mut Vec<Cand>, value: u32, kind: BaseKind| {
        if value < MAX_SCORE && seen.insert((value, kind as u8)) {
            out.push(Cand { value, kind, units_edited: value % 10 != raw_units });
        }
    };
    for (b, kind) in bases {
        push(&mut out, b, kind);
        if b >= 100_000 {
            let floor10 = (b / 10) * 10;
            for d in 0..=9u32 {
                push(&mut out, floor10 + d, kind);
            }
        }
    }
    out
}

/// Corruption-aware cost of a reconstructed combo (ExecPlan step 4). NOT a plain
/// edit count: per invariant 3, an overlap restores a leading million on the
/// RIGHT operand of a junction and corrupts only the units digit of its LEFT
/// neighbour. So:
///   - +1 for each restored slot (`Million` or `Prepend`),
///   - a units-digit change costs +1 only when the slot is immediately LEFT of a
///     `Million`-restored slot (the expected junction victim), else +3.
fn cost(kinds: [BaseKind; 3], units_edited: [bool; 3]) -> u32 {
    let mut c = 0u32;
    for i in 0..3 {
        if kinds[i] != BaseKind::Raw {
            c += 1;
        }
        if units_edited[i] {
            let left_of_junction = i + 1 < 3 && kinds[i + 1] == BaseKind::Million;
            c += if left_of_junction { 1 } else { 3 };
        }
    }
    c
}

/// A raw slot's plausible magnitude. An impossible value (>= MAX_SCORE — a
/// leading-digit glyph misread like 4,177,174) is treated as its `1,XXX,XXX`
/// repair so it does not over-bound the total; everything else is itself.
fn plausible_floor(v: u32) -> u32 {
    if v < MAX_SCORE {
        v
    } else {
        1_000_000 + v % 1_000_000
    }
}

/// Plausible totals to test against the checksum: the literal total (penalty 0)
/// plus every one-digit deletion of its decimal string (penalty 1, so the literal
/// wins genuine ties). A deletion models the stage total's thousands comma being
/// OCR'd as a spurious digit (e.g. "393,454" -> "3935454"). Deletions below
/// `floor` or outside the valid total range are dropped.
fn total_candidates(total: u32, floor: u32) -> Vec<(u32, u32)> {
    let mut out = vec![(total, 0u32)];
    let s = total.to_string();
    if s.len() > 1 {
        let bytes = s.as_bytes();
        let mut seen = std::collections::BTreeSet::new();
        for skip in 0..bytes.len() {
            let mut t = String::with_capacity(bytes.len() - 1);
            for (j, &b) in bytes.iter().enumerate() {
                if j != skip {
                    t.push(b as char);
                }
            }
            if let Ok(v) = t.parse::<u32>() {
                if v > 0 && v <= 9_999_999 && v >= floor && v != total && seen.insert(v) {
                    out.push((v, 1));
                }
            }
        }
    }
    out
}

/// The unique per-character score `c` with `c + floor(c/5) == total`, if any.
/// `c + c/5` is monotonic in `c`, so a tiny window around `floor(total*5/6)`
/// suffices to find or rule it out.
fn solve_single(total: u32) -> Option<u32> {
    let approx = (total as u64 * 5 / 6) as u32;
    for c in approx.saturating_sub(3)..=approx.saturating_add(3) {
        if c + c / 5 == total {
            return Some(c);
        }
    }
    None
}

/// Fallback when the checksum search found no satisfying combination.
///
/// For a NON-collision-prone stage (invariant 3 — at most one non-zero score, or
/// only sub-million scores, so no overlap is possible) a clean score is
/// trustworthy on its own, so resolve rather than reflexively flag a correct read
/// whose total/bonus mini-numbers merely mis-OCR'd:
///   - single-character total-solve recovers a dropped leading digit
///     (e.g. 55,172 -> 855,172) directly from the (comma-tolerant) total,
///     corroborated by the bonus when present;
///   - else accept the raw read as `Ok` when nothing contradicts it (bonus absent,
///     or bonus corroborates the raw maximum);
///   - else `Flagged` (the bonus positively disagrees, so there is real evidence
///     of an error we cannot pin without a good total).
/// A collision-prone stage keeps the old behaviour (Flagged; the caller's
/// `reconstruct_from_digits` fallback handles those).
fn resolve_without_checksum(
    ocr_scores: [u32; 3],
    mut total_set: Vec<(u32, u32)>,
    bonus_ok: Option<u32>,
) -> ([u32; 3], Recovery) {
    let nonzero: Vec<usize> = (0..3).filter(|&i| ocr_scores[i] > 0).collect();
    let has_million = ocr_scores.iter().any(|&s| plausible_floor(s) >= 1_000_000);
    let collision_prone = has_million && nonzero.len() >= 2;
    if collision_prone {
        return (ocr_scores, Recovery::Flagged);
    }

    // Single-character total-solve: exactly one non-zero slot. Prefer the literal
    // total (penalty 0) over comma-deletion variants.
    if nonzero.len() == 1 {
        let idx = nonzero[0];
        total_set.sort_by_key(|&(_, p)| p);
        for (t, _) in &total_set {
            if let Some(c) = solve_single(*t) {
                if c < MAX_SCORE && bonus_ok.map_or(true, |b| c / 5 == b) {
                    let mut out = ocr_scores;
                    out[idx] = c;
                    let rec = if c == ocr_scores[idx] {
                        Recovery::Ok
                    } else {
                        Recovery::Repaired
                    };
                    return (out, rec);
                }
            }
        }
    }

    // Reaching here means a usable total was provided but NO candidate combo
    // satisfied it. For a multi-score stage that is definitive: the scores do not
    // sum to the total, so digits were lost — flag, never accept. (Collision-prone
    // multis already returned above; this catches all-sub-million garble where
    // every score lost digits, e.g. [1383, 64377, 364] for the true
    // [1383068, 1377364, 1278247] with a correct total of 4,315,292 — which would
    // otherwise have slipped through as a silent `Ok`.) The bonus cannot rescue it:
    // it only pins the max, so a non-max garbled slot stays invisible.
    if nonzero.len() >= 2 {
        return (ocr_scores, Recovery::Flagged);
    }

    // A single (or zero) non-zero slot: a noisy total need not condemn a clean
    // lone score (its solve attempt above just failed). Lean on the bonus, which
    // for one score fully corroborates it (the score IS the max).
    let max_plausible = ocr_scores
        .iter()
        .copied()
        .map(plausible_floor)
        .max()
        .unwrap_or(0);
    match bonus_ok {
        Some(b) if max_plausible / 5 == b => (ocr_scores, Recovery::Ok),
        Some(_) => (ocr_scores, Recovery::Flagged),
        None => (ocr_scores, Recovery::Ok),
    }
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

    // Parts may be up to 8 digits: a colliding leading "1" is sometimes
    // *duplicated* by OCR rather than dropped (the right number's "1" overlaps
    // the left number's units glyph and reads as "11"), inflating one part to 8
    // digits. Such a part is collapsed back to 7 by dropping the doubled leading
    // digit. See the digit-insertion case in the ExecPlan's field-run notes.
    for k in 1..=3usize.min(n) {
        for comp in compositions(n, k, 1, 8) {
            // Slice the stream into k parts.
            let mut parts: [&[u8]; 3] = [&[], &[], &[]];
            let mut off = 0;
            for i in 0..k {
                parts[i] = &ds[off..off + comp[i]];
                off += comp[i];
            }

            // Per-part value options: the literal value, plus collision-victim
            // repairs for non-first parts (a leading digit is only ever corrupted
            // at a junction, whose left operand is >= 1M — enforced below):
            //   - a 6-digit part that *lost* its leading "1": `d,XXX,XXX`;
            //   - a 7-digit part that is impossible (>= MAX_SCORE, i.e. leading
            //     digit 3..9) because the leading "1" was *substituted* (e.g.
            //     "0"+"1" overlap misread as "4"): replace the leading digit with
            //     1 or 2 (the only valid leading digits of a >= 1M score);
            //   - an 8-digit part whose first two digits are equal: the leading
            //     "1" was *duplicated*, so drop one (collapse to 7 digits).
            // A clean 7-digit literal may start with any digit (scores can
            // exceed 2M), so the literal is always kept when in range.
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
                if i > 0 && comp[i] == 7 && v >= MAX_SCORE {
                    // Impossible 7-digit part: its leading digit was substituted.
                    let tail = v % 1_000_000;
                    for d in 1..=2u32 {
                        let rv = d * 1_000_000 + tail;
                        if rv < MAX_SCORE {
                            popts[i].push((rv, true));
                        }
                    }
                }
                if i > 0 && comp[i] == 8 && parts[i][0] == parts[i][1] {
                    // Duplicated leading digit: drop one, collapse to 7 digits.
                    let rv: u32 = std::str::from_utf8(&parts[i][1..]).unwrap().parse().unwrap_or(u32::MAX);
                    if rv < MAX_SCORE {
                        popts[i].push((rv, true));
                    }
                }
                // Single-character stage (k == 1): the lone score gained a
                // spurious leading digit with no overlap neighbour. An 8-digit
                // part drops its leading digit (e.g. "41110707" -> "1110707");
                // a 7-digit impossible part replaces its leading "1,"->digit
                // misread with 1/2. These need no >= 1M left neighbour, so the
                // physical-validity million-trade guard is skipped when k == 1.
                if k == 1 && comp[0] == 8 {
                    let rv: u32 = std::str::from_utf8(&parts[0][1..]).unwrap().parse().unwrap_or(u32::MAX);
                    if rv >= 1_000_000 && rv < MAX_SCORE {
                        popts[0].push((rv, true));
                    }
                }
                if k == 1 && comp[0] == 7 && v >= MAX_SCORE {
                    let tail = v % 1_000_000;
                    for d in 1..=2u32 {
                        let rv = d * 1_000_000 + tail;
                        if rv < MAX_SCORE {
                            popts[0].push((rv, true));
                        }
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
                // partner); reject physically-impossible million-trades. This
                // overlap rule does not apply to a single-part stage (k == 1):
                // its repair is a lone-character glyph fix, not a junction, so
                // skip the guard there.
                if k > 1 && (0..k).any(|i| restored[i] && (i == 0 || base[i - 1] < 1_000_000)) {
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
        // Plain edit count ties them; the asymmetric cost does not. Both edit
        // slot 0's units; the correct combo charges that as the junction victim
        // (+1, left of the Million-restored slot 1), the wrong combo edits the
        // restored slot's own units instead (+3).
        let correct_cost = cost([BaseKind::Raw, BaseKind::Million, BaseKind::Raw], [true, false, false]);
        let wrong_cost = cost([BaseKind::Raw, BaseKind::Million, BaseKind::Raw], [false, true, false]);
        assert!(correct_cost < wrong_cost);

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

    // --- Bonus over-detection must not flag a total-confirmed read
    //     (run 20260624_214602, multi-character stages). ---

    #[test]
    fn test_total_confirmed_over_detected_bonus_is_ok() {
        // Scores correct and the total confirms them exactly (cost 0), but the
        // bonus over-detected a digit (true floor(372069/5)=74413, OCR'd 744135).
        // The noisy bonus must NOT flag a total-confirmed read.
        let (scores, rec) = reconcile_stage([365181, 372069, 357515], Some(1169178), Some(744135));
        assert_eq!(scores, [365181, 372069, 357515]);
        assert_eq!(rec, Recovery::Ok);
    }

    #[test]
    fn test_edited_repair_still_bonus_guarded() {
        // An *edited* (cost > 0) reconstruction whose derived bonus contradicts a
        // valid OCR'd bonus is still flagged — the bonus guards uncertain repairs.
        // [1327534,151661,0] repairs to [1327533,1151661,0] (cost > 0); a bonus of
        // 200000 (!= floor(1327533/5)=265506) must downgrade it to Flagged.
        let (_scores, rec) = reconcile_stage([1327534, 151661, 0], Some(2744700), Some(200000));
        assert_eq!(rec, Recovery::Flagged);
    }

    // --- Single-character corruption (run 20260623_232320). The score row's lone
    //     character mis-read; total/bonus drive the recovery. ---

    #[test]
    fn test_single_char_leading_one_to_four_iter304() {
        // 1,177,174 read as 4,177,174 (leading "1," glyph misread as "4"); the
        // total confirms, bonus was unread. Impossible >= 3M raw recovered.
        let (scores, rec) = reconcile_stage([4177174, 0, 0], Some(1412608), None);
        assert_eq!(scores, [1177174, 0, 0]);
        assert_eq!(rec, Recovery::Repaired);
    }

    #[test]
    fn test_single_char_leading_one_to_four_with_bonus() {
        // iters 267/349/363/521 — same mode, bonus present and corroborating.
        for (raw, total, bonus, want) in [
            (4172520u32, 1407024u32, 234504u32, 1172520u32),
            (4117975, 1341570, 223595, 1117975),
            (4115501, 1338601, 223100, 1115501),
            (4122517, 1347020, 224503, 1122517),
        ] {
            let (scores, rec) = reconcile_stage([raw, 0, 0], Some(total), Some(bonus));
            assert_eq!(scores, [want, 0, 0], "raw={raw}");
            assert_eq!(rec, Recovery::Repaired, "raw={raw}");
        }
    }

    #[test]
    fn test_single_char_comma_inserted_total_is_ok_not_flagged() {
        // iter 3 stage 3: score 327,879 is correct; total "393,454" OCR'd as
        // "3935454" (comma read as 5). The one-digit-deletion 393454 satisfies the
        // checksum, so the unchanged score is Ok, not flagged.
        let (scores, rec) = reconcile_stage([327879, 0, 0], Some(3935454), None);
        assert_eq!(scores, [327879, 0, 0]);
        assert_eq!(rec, Recovery::Ok);
    }

    #[test]
    fn test_single_char_dropped_leading_digit_recovered() {
        // iter 472: 55,172 -> 855,172; iter 207: 92,118 -> 892,118. The dropped
        // leading digit cannot be reconstructed from the score text, but the
        // single-character total-solve pins it exactly (bonus corroborates).
        let (s1, r1) = reconcile_stage([55172, 0, 0], Some(1026206), Some(171034));
        assert_eq!((s1, r1), ([855172, 0, 0], Recovery::Repaired));
        let (s2, r2) = reconcile_stage([92118, 0, 0], Some(1070541), Some(178423));
        assert_eq!((s2, r2), ([892118, 0, 0], Recovery::Repaired));
    }

    #[test]
    fn test_single_char_transposed_total_bonus_corroborates_ok() {
        // Score 1,119,377 correct; total transposed to 1,343,525 (true 1,343,252),
        // which no single-digit deletion fixes. The bonus (floor(1119377/5)=223875)
        // corroborates the raw read → Ok, not flagged.
        let (scores, rec) = reconcile_stage([1119377, 0, 0], Some(1343525), Some(223875));
        assert_eq!(scores, [1119377, 0, 0]);
        assert_eq!(rec, Recovery::Ok);
    }

    #[test]
    fn test_single_char_bonus_contradicts_flags() {
        // Clean-looking single score but the bonus disagrees and the total is
        // unusable → genuine evidence of an error we cannot pin → Flagged.
        let (scores, rec) = reconcile_stage([500000, 0, 0], Some(9999999), Some(123456));
        assert_eq!(scores, [500000, 0, 0]);
        assert_eq!(rec, Recovery::Flagged);
    }

    #[test]
    fn test_single_char_clean_total_stays_ok() {
        // Regression: a normal single sub-million score with a matching total is Ok.
        let (scores, rec) = reconcile_stage([994573, 0, 0], Some(1193487), None);
        assert_eq!(scores, [994573, 0, 0]);
        assert_eq!(rec, Recovery::Ok);
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

    // --- Field-run failures (run 20260620_030517): the colliding leading "1"
    //     was *substituted* or *duplicated* rather than dropped. ---

    #[test]
    fn test_reconstruct_leading_digit_substituted_iter337() {
        // "0"+"1" overlap misread the leading "1" of 1,023,847 as "4", giving an
        // impossible 7-digit part 4,023,847. Stream "115624040238471089584".
        let (scores, rec) =
            reconstruct_from_digits("115624040238471089584", Some(3500919), Some(231248)).unwrap();
        assert_eq!(scores, [1156240, 1023847, 1089584]);
        assert_eq!(rec, Recovery::Repaired);
    }

    #[test]
    fn test_reconstruct_leading_digit_substituted_iter372() {
        // 1,057,372 read as 4,057,372 (single collision; c3 sub-million).
        let (scores, rec) =
            reconstruct_from_digits("13499404057372861381", Some(3538681), Some(269988)).unwrap();
        assert_eq!(scores, [1349940, 1057372, 861381]);
        assert_eq!(rec, Recovery::Repaired);
    }

    #[test]
    fn test_reconstruct_leading_digit_substituted_iter174() {
        // 1,238,281 read as 4,238,281; c1 also mis-tokenized but digits survive.
        let (scores, rec) =
            reconstruct_from_digits("106173042382811170156", Some(3717823), Some(247656)).unwrap();
        assert_eq!(scores, [1061730, 1238281, 1170156]);
        assert_eq!(rec, Recovery::Repaired);
    }

    #[test]
    fn test_reconstruct_leading_digit_duplicated_iter71() {
        // The colliding "1" of 1,023,254 was duplicated ("...997"+"11023254"),
        // inflating the stream to 21 digits. Collapse the doubled leading digit.
        let (scores, rec) =
            reconstruct_from_digits("118499711023254644786", Some(3090036), Some(236999)).unwrap();
        assert_eq!(scores, [1184997, 1023254, 644786]);
        assert_eq!(rec, Recovery::Repaired);
    }

    #[test]
    fn test_reconstruct_single_char_split_iter400() {
        // iter 400: a single character 1,110,707 whose score row OCR'd as two
        // tokens "41110" "707" — digit stream "41110707" (8 digits, "4" prepended
        // to "1110707"). Drop the spurious leading digit; checksum + bonus confirm.
        let (scores, rec) =
            reconstruct_from_digits("41110707", Some(1332848), Some(222141)).unwrap();
        assert_eq!(scores, [1110707, 0, 0]);
        assert_eq!(rec, Recovery::Repaired);
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

    // --- Field-run replay regression harness (run 20260628_071057). ---
    // Real per-stage OCR inputs (raw scores, total, bonus) extracted from the
    // session log, paired with the manually-verified ground truth. Proves no
    // regression on the 1,186 correct stages and measures recovery on the rest.
    #[test]
    fn replay_field_run_20260628() {
        let data = include_str!("testdata/overlap_replay_20260628.csv");
        // Recovered only via the digit-stream fallback (cross-slot redistribution),
        // which reconcile_stage alone cannot do; not part of this isolation harness.
        const DIGIT_STREAM: &[u32] = &[
            21, 45, 62, 98, 134, 143, 168, 175, 244, 259, 280, 310, 315, 330,
        ];
        // Best-effort bucket B: total mis-OCR'd, bonus restores the (max) c2, but a
        // non-max units corruption can't be fixed — c2 must match truth, c1 may not.
        const BEST_EFFORT: &[u32] = &[60, 67, 109, 206];
        // Unrecoverable from these inputs (>= 2 digits absent, or total wrong with a
        // non-max prepend the bonus can't pin) — must stay at raw, never guess.
        const STAY_RAW: &[u32] = &[24, 114, 180, 289, 291];
        let mut mism: Vec<(u32, u32, [u32; 3], [u32; 3], [u32; 3])> = Vec::new();
        let (mut rows, mut recovered, mut best_effort, mut stayed) = (0, 0, 0, 0);
        for line in data.lines() {
            let f: Vec<&str> = line.split(',').collect();
            if f.len() < 10 {
                continue;
            }
            let p = |i: usize| f[i].parse::<u32>().unwrap();
            let iter = p(0);
            let stage = p(1);
            let raw = [p(2), p(3), p(4)];
            let opt = |i: usize| match f[i].parse::<i64>().unwrap() {
                -1 => None,
                v => Some(v as u32),
            };
            let truth = [p(7), p(8), p(9)];
            rows += 1;
            let (got, _flag) = reconcile_stage(raw, opt(5), opt(6));
            if got == truth {
                if got != raw {
                    recovered += 1;
                }
                continue;
            }
            // got != truth: only the documented exceptions are allowed.
            let max_idx = (0..3).max_by_key(|&i| truth[i]).unwrap();
            if stage == 2 && DIGIT_STREAM.contains(&iter) {
                continue; // handled by reconstruct_from_digits in the live pipeline
            } else if stage == 2 && STAY_RAW.contains(&iter) {
                if got == raw {
                    stayed += 1;
                    continue;
                }
            } else if stage == 2 && BEST_EFFORT.contains(&iter) {
                // The max (collision-victim) slot must be recovered; an unfixable
                // non-max units digit is tolerated.
                if got[max_idx] == truth[max_idx] && got != raw {
                    best_effort += 1;
                    continue;
                }
            }
            mism.push((iter, stage, raw, got, truth));
        }
        for m in &mism {
            eprintln!(
                "UNEXPECTED it{} s{}: raw={:?} got={:?} truth={:?}",
                m.0, m.1, m.2, m.3, m.4
            );
        }
        eprintln!(
            "replay: {} rows | recovered exactly: {} | best-effort: {} | stay-raw: {} | unexpected: {}",
            rows, recovered, best_effort, stayed, mism.len()
        );
        assert!(mism.is_empty(), "{} unexpected mismatches", mism.len());
    }

    #[test]
    fn test_multi_score_unsatisfiable_total_flags_not_ok() {
        // Run 20260629_014136 it200 (false positive): every score lost 3-4 digits,
        // so no candidate reaches the truth and the (correct) total 4,315,292 is
        // unsatisfiable. A multi-score row that contradicts a usable total must
        // FLAG, not silently accept the garbage as `Ok`.
        let (scores, rec) = reconcile_stage([1383, 64377, 364], Some(4315292), None);
        assert_eq!(scores, [1383, 64377, 364]); // unchanged (best-effort)
        assert_eq!(rec, Recovery::Flagged);
    }

    #[test]
    fn test_leading_two_to_one_substitution() {
        // Run 20260628_223009: a valid-looking 2,XXX,XXX whose leading "1" OCR'd
        // as "2", on a non-max slot; the (correct) total disambiguates exactly.
        // it131: c2 2,396,184 -> 1,396,184; c1 units 1->7.
        let (s1, r1) = reconcile_stage([1201271, 2396184, 1541984], Some(4447841), Some(308396));
        assert_eq!(s1, [1201277, 1396184, 1541984]);
        assert_eq!(r1, Recovery::Repaired);
        // it194: c2 2,093,004 -> 1,093,004; c1 units 1->7.
        let (s2, r2) = reconcile_stage([1415951, 2093004, 964825], Some(3756977), Some(283191));
        assert_eq!(s2, [1415957, 1093004, 964825]);
        assert_eq!(r2, Recovery::Repaired);
    }

    #[test]
    fn test_genuine_two_million_not_swapped() {
        // Regression: a real 2,134,567 with a matching total must keep its value
        // (the 1,134,567 swap candidate exists but loses — raw wins at cost 0).
        let (scores, rec) =
            reconcile_stage([2134567, 500000, 300000], Some(3361480), Some(426913));
        assert_eq!(scores, [2134567, 500000, 300000]);
        assert_eq!(rec, Recovery::Ok);
    }

    #[test]
    fn test_candidates_include_restore_and_units() {
        let vals = |v: u32| -> Vec<u32> { slot_candidates(v).iter().map(|c| c.value).collect() };
        // Leading-zero victim: 62741 must yield 1,062,741 (Million) as a candidate.
        let c = vals(62741);
        assert!(c.contains(&62741));
        assert!(c.contains(&1062741));
        // Units variants exist around the restored base.
        assert!(c.contains(&1062740) && c.contains(&1062749));
        // Leading-digit-drop (the "leading 8" failure): 52,517 must yield 852,517
        // as a Prepend candidate, tagged accordingly.
        let p = slot_candidates(52517);
        let eight = p.iter().find(|c| c.value == 852517).expect("852517 candidate");
        assert_eq!(eight.kind, BaseKind::Prepend);
        // All candidates capped below MAX_SCORE.
        assert!(p.iter().all(|c| c.value < MAX_SCORE));
    }
}
