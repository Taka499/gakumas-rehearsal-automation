---
status: accepted
---

# Flagged rows never auto-clear; verification is an explicit human act (`verified` recovery value)

A flagged row is flagged precisely because the checksum admits multiple equally-valid solutions — the stored value is the solver's deterministic tie-break, so "satisfies the checksum" does NOT prove it is correct. Therefore flagged rows are resolved only by a human clicking ✓ after looking at the screenshot; auto-clearing checksum-consistent rows was explicitly rejected because it silently reintroduces the error class the flagging exists to prevent (real example: run 20260629_014136 iter 181 satisfies the checksum yet is genuinely ambiguous). Do not "optimize" this away. The resolved state is stored as a new `recovery` column *value* (`verified`), not a new CSV column, so older readers (which index only the first 12 columns) need no migration.

Source: `docs/EXECPLAN_REVIEW_VERIFIED_STATE.md` Decision Log 2026-06-29 — user decision, with the concrete ambiguous-row example; roundtrip unit-tested.

Addendum 2026-07-06: the exclusion now also applies to the final on-disk charts and `statistics.json` — `analysis::csv_reader` drops `flagged` rows at load, and any review save regenerates the charts (a verify-only save re-includes the row). This unifies the final analysis with the live figure, which had deliberately diverged (the divergence was recorded in `docs/EXECPLAN_LIVE_BOX_PLOT.md`; unification was a user decision surfaced by the 2026-07-06 backfill audit).
