---
status: accepted
---

# Screenshots on disk are the source of truth for `completed`; only `total` is trusted from run-meta.json

Resume, extend (追加実行), the session picker, and the live-chart re-seed all recompute a session's `completed` count from the PNG count in its `screenshots/` folder, deliberately ignoring the `completed` field that `run-meta.json` also stores. Reason: screenshots are written synchronously in the `Capturing` state *before* asynchronous OCR, so the on-disk count is crash-proof and never lags, while any stored counter can. The asymmetry is intentional: `total` is *not* recoverable from disk (it lives only in a runtime atomic), so it alone is trusted from the file. Do not "simplify" by trusting stored `completed` or by deriving `total` from artifacts.

Source: `docs/EXECPLAN_RESUME_AUTOMATION.md` Decision Log and Surprises & Discoveries; manual crash/resume scenarios A–D passed (committed d968a4a); invariant re-applied by `docs/EXECPLAN_ADDITIONAL_RUNS_AND_PRESETS.md` and `docs/EXECPLAN_LIVE_BOX_PLOT.md`.
