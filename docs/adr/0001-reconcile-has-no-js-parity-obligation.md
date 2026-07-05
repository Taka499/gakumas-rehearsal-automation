---
status: accepted
---

# reconcile.rs has no JS-parity obligation (rescinds an implicit constraint)

`CLAUDE.md` and several 2026-06/07 ExecPlans (`EXECPLAN_SUSTAINABILITY_REFACTOR.md`, `EXECPLAN_SCORE_ROW_THRESHOLD_RETRY.md`) treated `src/ocr/reconcile.rs` as a "JS-parity read-mostly zone" that had to stay in lockstep with the gakumas-tools fork's `rehearsalRecovery.js`. That constraint was never true: it was a mistaken inference by a coding agent that went unnoticed. The heuristics were ported *to* gakumas-tools as a one-time upstream contribution (`surisuririsu/gakumas-tools#103`/`#104`), which created no ongoing obligation in either direction. `reconcile.rs` may be freely refactored, subject to its unit tests. The old plans still contain the parity phrasing; per the ADR sync contract they are immutable history and this ADR is the correction.

Source: user correction, session 2026-07-05 ("it is the mistake Claude Code made that I failed to notice"; the ports were one-time contributions). No evidence ever supported the original constraint — it was asserted by an agent, unconfirmed.
