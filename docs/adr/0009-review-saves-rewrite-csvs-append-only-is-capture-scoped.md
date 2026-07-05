---
status: accepted
---

# Post-run review saves rewrite both CSVs in full; "append-only" applies to live capture only

The review window's save path rewrites `results.csv` in full and patches `rehearsal_data.csv` line-for-line (temp-file-then-rename), always together so the two files never diverge. This is a deliberate, bounded exception to the append-only discipline: the crash-safety rationale for append-only applies to *live capture* (a crash mid-run must not lose rows), not to a deliberate post-run correction performed while nothing else is writing. Readers who encounter the full rewrite should not "fix" it back to appends, and readers of the append-only invariant should not extend it to the review path.

Source: `docs/EXECPLAN_OCR_REVIEW_EDIT_GUI.md` Decision Log (entry D); round-trip unit-tested.
