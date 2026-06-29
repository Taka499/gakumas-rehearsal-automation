# Review-save UX: auto-save on verify, chart regen on save, and a post-run flagged-row prompt

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

This repository contains `docs/PLANS.md` (relative to the repository root). This document must be maintained in accordance with `docs/PLANS.md`.


## Purpose / Big Picture

The tool automates the game *gakumas*' "rehearsal" feature: it clicks through many runs and, on each result screen, reads nine numbers via OCR. Each run's nine scores are written to a per-session `results.csv`, whose last column, `recovery`, records confidence in that row: `ok` (clean), `repaired` (a known corruption was reconstructed and confirmed by the screen's arithmetic checksum), `flagged` (could not be confirmed — needs a human), `manual` (a human edited it), or `verified` (a human confirmed a flagged/repaired row is correct without editing it — added by `EXECPLAN_REVIEW_VERIFIED_STATE.md`).

Three small UX gaps remain around finishing a run and reviewing its results:

1. **Charts go stale after a manual correction.** Charts and `statistics.json` are generated once when the run finishes (`finalize_status`). If the user then opens the review window and corrects OCR mistakes (or generates charts manually), the saved charts no longer reflect the corrected numbers until the user remembers to press "📊 グラフを生成". The charts should track the data automatically.

2. **Verifying a row still needs a second click to persist.** Clicking the new "✓" (verify) button marks the row `verified` in memory and dirties the window, but the user must then click "💾 保存" to write it. For a one-row "yes this is correct" judgement, the extra step is friction. Verifying should persist immediately.

3. **Nothing tells the user a finished run has rows to check.** When a run completes, the panel shows progress and an action list, but the user has to *remember* to open the review window and look for `flagged` rows. A finished run with flagged rows should say so, with the count, and point at the review button.

After this change: saving review edits regenerates the session's charts automatically (when scores changed); clicking "✓" verifies *and* saves in one action; and the finished-run panel shows a coloured "⚠ 要確認の行が N件 あります" prompt above the review button whenever the just-finished (or just-saved) session still has `flagged`/`repaired` rows.


## The key facts this plan relies on

State these to yourself; the implementation depends on them. They are established by reading the current code.

1. `src/gui/mod.rs::handle_save_review` is the single persistence path for the review window. It parses each row's edit buffers, sets `row.recovery = RECOVERY_MANUAL` and increments a local `changed` counter for any row whose parsed scores differ from its stored scores, then calls `save_review_rows` (atomic rewrite of `results.csv` + `rehearsal_data.csv`) and re-seeds the buffers. A row whose scores did **not** change is left untouched, so a `recovery` set elsewhere (e.g. `verified`) survives the save.

2. The verify action is dispatched in `src/gui/mod.rs::render_review_window`: `if let Some(iter) = actions.mark_verified { … row.recovery = RECOVERY_VERIFIED; review.dirty = true; }`. Immediately below it, `if actions.save { self.handle_save_review(); }`. So routing verify through the existing save needs only widening that `save` condition — no second save path.

3. `src/analysis/mod.rs::generate_analysis_for_session(session_dir: &Path) -> Result<(Vec<PathBuf>, PathBuf)>` reads `<session_dir>/results.csv` and (re)writes `charts/` + `statistics.json`. It is exactly what `finalize_status` calls at run end, and it is safe to call again (it overwrites). Calling it after a review save brings the charts in line with the corrected CSV.

4. The finished-run UI is `src/gui/render.rs::render_finished(ui, state: &mut GuiState, status, actions)`. It already renders the status summary, a generated-files list, and the action buttons including "📝 結果を確認・修正". It has `&mut GuiState`, so it can read any `GuiState` field to drive a prompt.

5. `src/automation/results_edit.rs::load_review_rows(session_dir) -> Result<Vec<ReviewRow>>` parses `results.csv` into rows carrying the `recovery` string. Counting `flagged`/`repaired` rows is a load + count; it is already imported into `src/gui/mod.rs`.

6. The terminal `AutomationStatus` is built in `src/gui/mod.rs::update_automation_status` (which has `&mut self`) via `finalize_status`; this is the one place a run transitions Running → Completed/Aborted/Error, and `finalize_status` has already (re)generated the charts and thus a final `results.csv` by the time it returns.


## Progress

- [x] (2026-06-30) M1 — Added `GuiState.attention_counts: Option<(u32,u32)>` + `Default` (`src/gui/state.rs`); `count_attention(&Path) -> (u32,u32)` helper in `src/gui/mod.rs`; set in `update_automation_status` right after `finalize_status` (clones `session_path` since `finalize_status` consumes it).
- [x] (2026-06-30) M2 — Auto-save on verify: dispatch condition widened to `actions.save || actions.mark_verified.is_some()` in `render_review_window`.
- [x] (2026-06-30) M3 — `handle_save_review` now captures `session_path`/`changed`, and after a successful save recomputes `attention_counts` and (only when `changed > 0`) calls `generate_analysis_for_session`.
- [x] (2026-06-30) M4 — Finished-panel prompt added to `render_finished`: a flagged-orange "⚠ 要確認の行が N件 あります…（自動修復 M件）" (or blue repaired-only) notice above the review button, gated on `state.attention_counts`.
- [ ] M5 — Code-complete; `GAKUMAS_NO_MANIFEST=1 cargo test` → 110 passed; release build via `scripts/build.ps1 -Kill` green (1m59s). **Manual GUI click-through pending**: correct a row → charts regenerate; verify a row → persists without 保存; finish a run with a flagged row → prompt shows the count.

Use timestamps when you check items off, e.g. `- [x] (2026-06-29 14:00Z) ...`.


## Surprises & Discoveries

- (none yet)


## Decision Log

- Decision: Regenerate charts only when `changed > 0`, not on every save.
  Rationale: Charts derive solely from the nine scores. A verify-only save (the common case now that "✓" auto-saves) changes only the `recovery` string, so the charts would be byte-identical — regenerating would be pure waste and, worse, would add a multi-hundred-ms plotters render on the GUI thread to every single "✓" click, making rapid verification janky. Gating on `changed` keeps "charts always reflect the data" true (scores only change via an edit, which sets `changed`) while avoiding the jank. The literal request was "regenerate after each save"; this honours its intent (keep charts in sync with corrections) without the cost on no-op saves.
  Date/Author: 2026-06-29.

- Decision: Implement "save on verify" by widening the existing dispatch condition (`actions.save || actions.mark_verified.is_some()`) rather than adding a save call inside the verify block or a new save routine.
  Rationale: There must remain exactly one persistence path (`handle_save_review`) so the manual-vs-verified rule lives in one place. `handle_save_review` already flips only score-changed rows to `manual`; a verified-but-unedited row therefore saves as `verified`, and a row the user both edited and verified saves as `manual` (the edit is a real correction and should win). This is the user's "append a save after the verify status change" idea, realised as a one-line condition change with no duplicated logic.
  Date/Author: 2026-06-29.

- Decision: Cache the attention count in `GuiState` (computed at run-finish and after each save) instead of recounting from the CSV every frame in `render_finished`.
  Rationale: `render_finished` runs on every repaint (many times per second). Reading and parsing `results.csv` each frame to count flags would be needless I/O. The count only changes at two moments — when a run finishes and when the user saves a review edit — so computing it exactly there and caching the `(flagged, repaired)` pair is both correct and cheap. The pair is `Copy`, so reading it in render is free.
  Date/Author: 2026-06-29.

- Decision: Count and surface both `flagged` and `repaired`, but emphasise `flagged`.
  Rationale: The review window's default attention view is `flagged` + `repaired`, so the prompt should mirror it. `flagged` rows genuinely need a human (the request's focus), so the notice leads with them and uses the flagged colour when any exist; `repaired` rows are auto-recovered and shown as a secondary, lower-key mention so a user who wants to audit them knows they exist.
  Date/Author: 2026-06-29.


## Outcomes & Retrospective

- (2026-06-30) M1–M4 landed; the three UX gaps are closed in code (full suite 110 passed, release build green). What now works that did not before: (1) saving a review edit that changes a score automatically regenerates the session's `charts/` + `statistics.json` (no manual "📊 グラフを生成" needed), while a verify-only save deliberately skips regen; (2) clicking "✓" on a flagged/repaired row both marks it `verified` and persists immediately through the single `handle_save_review` path (an unedited row → `verified`, a same-frame-edited row → `manual`); (3) a finished run with `flagged`/`repaired` rows shows a coloured count prompt above the review button, recomputed after each save so it shrinks as rows are resolved. The change is additive — no existing field/signature removed; a clean run shows no prompt and behaves as before. Only M5's interactive click-through remains (inherently manual per the tray-GUI policy).


## Context and Orientation

Files this plan touches, by full repository-relative path:

- `src/gui/state.rs` — defines `GuiState` and its `Default`. Gains one field, `attention_counts: Option<(u32, u32)>`.
- `src/gui/mod.rs` — owns the GUI. `update_automation_status`/`finalize_status` handle the Running→terminal transition and chart generation; `handle_save_review` is the save path; `render_review_window` dispatches `ReviewActions`. Gains the `count_attention` helper, the finish-time count, the auto-save-on-verify condition, and the post-save chart regen + recount.
- `src/gui/render.rs` — `render_finished` draws the finished-run panel. Gains the attention prompt.

No data-layer (`results_edit.rs`) or analysis changes are needed: both `load_review_rows` and `generate_analysis_for_session` already exist and are called as-is.

Terms: "attention rows" = rows with `recovery` of `flagged` or `repaired` (the review window's default view). "finished panel" = the third-column UI shown for the Completed/Aborted/Error states (`render_finished`). "dirty" = unsaved changes; enables 保存.


## Plan of Work

The change is additive; no existing field or signature is removed.

Milestone M1 (state + helper). In `src/gui/state.rs`, add to `GuiState`:

    /// (flagged, repaired) row counts for `latest_session_path`. Set when a run
    /// reaches a terminal state and re-computed after each review save, so the
    /// finished panel can prompt the user to check remaining attention rows.
    pub attention_counts: Option<(u32, u32)>,

and `attention_counts: None,` in `Default`. In `src/gui/mod.rs`, add an associated helper:

    fn count_attention(session_path: &std::path::Path) -> (u32, u32) { … load_review_rows, count "flagged" and "repaired" … }

In `update_automation_status`, after `self.state.status = self.finalize_status(...)`, add `self.state.attention_counts = Some(Self::count_attention(&session_path));` (clone `session_path` as needed; `finalize_status` consumes it).

Milestone M2 (auto-save on verify). In `src/gui/mod.rs::render_review_window`, change `if actions.save {` to `if actions.save || actions.mark_verified.is_some() {`. The `mark_verified` block above it already set `recovery=verified` + `dirty=true` before this runs.

Milestone M3 (chart regen + recount on save). In `handle_save_review`, capture `let session_path = review.session_path.clone();` and the `changed` count; after the save `match` succeeds, end the `review` borrow, then: `self.state.attention_counts = Some(Self::count_attention(&session_path));` and, `if changed > 0`, call `crate::analysis::generate_analysis_for_session(&session_path)` (log success/failure like `finalize_status` does).

Milestone M4 (prompt). In `src/gui/render.rs::render_finished`, just before the "📝 結果を確認・修正" button, read `state.attention_counts`; if `flagged > 0 || repaired > 0`, draw a `RichText` notice: lead with "⚠ 要確認の行が {flagged}件 あります" when `flagged > 0` (flagged colour), append "（自動修復 {repaired}件）" when `repaired > 0`; if only `repaired > 0`, a blue "自動修復された行が {repaired}件 あります". End with "「結果を確認・修正」で確認してください。"

Milestone M5 (build, run, verify). Build with the guarded wrapper, run, and confirm the three behaviours; record in Outcomes.


## Concrete Steps

Work from the repository root `C:\Work\GitRepos\gakumas-screenshot`.

Compile-check:

    GAKUMAS_NO_MANIFEST=1 cargo test 2>&1 | grep -E "^error" || echo "no errors"

Build release (guarded; a running instance locks the exe):

    powershell -ExecutionPolicy Bypass -File scripts/build.ps1 -Kill

Run:

    ./target/release/gakumas-screenshot.exe


## Validation and Acceptance

Acceptance is observable behaviour, not code shape. All three are GUI-driven, hence manual per the project's tray-GUI testing policy:

1. **Chart regen on correction (M3):** Open the review window on a finished session, edit a score cell, press 保存. Confirm the session's `charts/*.png` and `statistics.json` file modification times update and reflect the new value. Confirm a verify-only save (no cell edit) does **not** rewrite the charts (mtime unchanged).
2. **Auto-save on verify (M2):** On a `flagged` row, click "✓" only (do not press 保存). Reopen the window (or tick the "verified" filter): the row is `verified` in `results.csv`, scores unchanged. No "● 未保存の変更" lingers after the click.
3. **Finished-run prompt (M1/M4):** Finish a run (or open a session) whose `results.csv` has ≥1 `flagged` row; the finished panel shows "⚠ 要確認の行が N件 あります …" with the correct N above the review button. After verifying/correcting all flagged rows and saving, the count shown (on the still-open finished panel) drops accordingly.
4. **Regression:** Editing a cell still flips a row to `manual`; `ok` rows show no "✓"; a clean run (no flagged/repaired) shows no prompt.


## Idempotence and Recovery

All steps are re-runnable. `generate_analysis_for_session` overwrites its outputs, so repeated saves simply rewrite the same charts. Saving is atomic (temp + rename). The `attention_counts` cache is derived state recomputed from the CSV at each finish/save, so a stale value cannot persist past the next save or run. The change is additive: a session with no flagged/repaired rows shows no prompt and behaves exactly as before; charts for a verify-only save are left untouched.


## Interfaces and Dependencies

- `src/gui/state.rs`: `GuiState` gains `pub attention_counts: Option<(u32, u32)>`.
- `src/gui/mod.rs`: new private `fn count_attention(&Path) -> (u32, u32)`; reuses `load_review_rows` (already imported) and `crate::analysis::generate_analysis_for_session` (already used by `finalize_status`).
- `src/gui/render.rs`: `render_finished` reads `state.attention_counts`.

No new external crates.
