---
status: proposed
---

# Per-stage region-based OCR supersedes full-image OCR (undocumented reversal)

`docs/EXECPLAN_PHASE2_OCR.md` and `docs/EXECPLAN_CALIBRATION_TOOL.md` record full-image OCR with pattern matching as the final architecture and declare `score_regions` "NO LONGER NEEDED" — but the shipped pipeline is the opposite: per-stage cropping via `score_regions` → brightness threshold → Tesseract per stage, and `score_regions` is load-bearing in `config.json`, `config.rs`, and `ocr_worker.rs`. The reversal itself was never documented in any plan. The inferred rationale (from CLAUDE.md's pipeline description): processing each stage independently avoids cross-stage noise, and tightened crops exclude the horizontal UI divider lines that confuse Tesseract layout analysis. Anyone reading the Phase-2/Calibration plans should treat their "full-image OCR is superior" conclusions as superseded.

Source: backfill audit 2026-07-06 over all 23 ExecPlans. The superseding decision has no recorded provenance; rationale inferred from CLAUDE.md and code, unconfirmed by the user — hence status `proposed` until the actual reason for the reversal is confirmed.
