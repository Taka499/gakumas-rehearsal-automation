---
status: accepted
---

# The score-recovery solver: exhaustive search, total-only checksum, corruption-aware cost

`src/ocr/reconcile.rs` reconstructs overlap-corrupted scores with three deliberate, coupled choices. (1) An exhaustive brute-force search over candidate repairs (≤ 21³ ≈ 9,261 combinations) rather than per-junction heuristics, because the two-simultaneous-overlaps case leaves two unknown units digits that a single checksum equation cannot pin. (2) The checksum is total-only — `total = c1+c2+c3+floor(max/5)` — with the on-screen bonus demoted to an optional cross-check, because the bonus is mathematically redundant (`bonus = floor(max/5)`) and its badge is the least reliable OCR read; a wrong bonus can at most flag a stage, never corrupt one. (3) Candidate selection uses an asymmetric corruption-aware cost, not a plain edit count: a units-digit edit on the left neighbour of a restored slot costs +1, elsewhere +3, encoding the physics of the overlap (only that digit is plausibly corrupted); a plain edit count provably ties on real data (sample 003). Related corrected bound: per-character scores have exceeded 1.8M, so "leading digit is always 1" is false — the regex accepts leading `[1-9]` and `MAX_SCORE` is a soft 3,000,000 cap (older statements in the overlap plan's top section are stale).

Source: `docs/EXECPLAN_OVERLAP_SCORE_RECOVERY.md` Decision Log and Surprises & Discoveries (bonus identity supplied by the user; tie example sample 003; MAX_SCORE correction verified by a 100-screenshot replay with 0 diffs); enforced by unit + e2e tests.
