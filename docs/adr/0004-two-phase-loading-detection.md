---
status: accepted
---

# Loading detection is two-phase: histogram-vs-reference match, then brightness threshold

`detection.rs` decides "loading finished" by first matching a captured region's grayscale histogram (Bhattacharyya coefficient) against a user-captured reference image of the Skip button, then applying a brightness threshold — with brightness-only fallback if no reference exists. This looks elaborate, but single-phase brightness detection demonstrably failed: the previous screen is also bright, and the Skip button appears immediately but dimmed during loading, so brightness alone cannot distinguish "before Skip appears" from "Skip ready" (field-measured state brightnesses 98.06 / 92.58 / 97.50 are too close). The cost is a user-facing calibration step (tray "Capture Skip Reference").

Source: `docs/EXECPLAN_PHASE1_REMAINING.md` (Decision Log, field brightness measurements) and `docs/EXECPLAN_PHASE3_AUTOMATION.md` (Surprises & Discoveries, Decision Log); the naive approach was built first and observed failing.
