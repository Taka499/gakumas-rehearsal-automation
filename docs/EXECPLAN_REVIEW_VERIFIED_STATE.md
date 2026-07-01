# Add a "verified" review state to resolve correct-but-flagged OCR rows

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

This repository contains `docs/PLANS.md` (relative to the repository root). This document must be maintained in accordance with `docs/PLANS.md`.


## Purpose / Big Picture

The tool automates the game *gakumas*' "rehearsal" feature: it clicks through many runs and, on each result screen, reads nine numbers via OCR (Optical Character Recognition — turning pixels of printed digits into numbers). Each run's nine scores are written to a per-session `results.csv`, whose last column, `recovery`, records how confident the reader is in that row: `ok` (read cleanly), `repaired` (a known corruption was reconstructed and confirmed by the screen's own arithmetic checksum), `flagged` (could not be confirmed — needs a human to look), or `manual` (a human edited the values by hand).

A GUI "review/edit" window (opened with the "📝 結果を確認・修正" button) lets the user filter to the `flagged`/`repaired` rows, compare them against the screenshot, and correct mistakes. Correcting a cell marks that row `manual`.

The gap this plan closes: some rows are `flagged` even though their values are actually **correct**. The reader flags them when the screen's checksum has more than one equally-valid solution (for example, a small units-digit correction that could belong to either of two non-max score columns), so it refuses to guess and asks a human. When the human looks and sees the value is already right, there is today **no way to record "I checked this, it's fine."** The row stays `flagged` forever, cluttering the attention list and making the data look worse than it is. Editing a cell would flip it to `manual`, but a correct row needs no edit, so nothing clears it.

After this change, the review window gains a per-row **"確認済み" (verified)** action: a small "✓" button on any `flagged` or `repaired` row. Clicking it sets that row's `recovery` to a new value, `verified`, without altering any score. On save it is written to `results.csv`, and because the `verified` filter is off by default the row drops out of the attention list — exactly like a resolved item. You can see it working by opening the review window on a session that has a `flagged` row, clicking its "✓", clicking "💾 保存", reopening the window, and observing the row is now `verified` (teal badge) and hidden unless you tick the new "verified" filter.


## The key facts this plan relies on

State these to yourself; the implementation depends on them. They are established by reading the current code.

1. The `recovery` column is a free-form string in the CSV and in memory; the data layer (`src/automation/results_edit.rs`) reads and writes it verbatim and never validates it against a fixed set. So introducing a new value (`verified`) needs no parser or schema change — only the producers (the GUI) and the human-facing consumers (badge colour, filter toggles) must learn about it.

2. The review window's in-memory model is `ReviewState` (`src/gui/state.rs`): it holds the loaded `rows: Vec<ReviewRow>`, the parallel editable text buffers `edits`, the per-status visibility booleans (`show_ok`, `show_repaired`, `show_flagged`, `show_manual`, plus a master `show_all`), a `dirty` flag (enables the 保存 button), and view state. A row is visible when its status's toggle is on (or `show_all`) AND it matches the search box.

3. Rendering and user input for the window live in `src/gui/render.rs::render_review_window_contents`. It draws the status filter checkboxes, the table (one row per visible result, nine editable cells, a status badge, and a "📷" expand button), and writes user intents into a `ReviewActions` struct (`src/gui/render.rs`). The owner (`src/gui/mod.rs`) dispatches those intents after rendering: `actions.save → handle_save_review`, `actions.toggle_expand → expand/preview`, `actions.close → close`.

4. Saving is `src/gui/mod.rs::handle_save_review`. It parses each row's edit buffers; if a row's parsed scores differ from its stored scores it sets `row.recovery = RECOVERY_MANUAL` and counts it changed; then it calls `save_review_rows` (which rewrites `results.csv` and `rehearsal_data.csv` atomically) and re-seeds the buffers. Crucially, a row whose **scores did not change is left untouched** — so a `recovery` value we set elsewhere (to `verified`) survives a save as long as the user did not also edit that row's cells.

5. The badge colour is `src/gui/render.rs::recovery_color(&str) -> Color32`, a match on the recovery string with a catch-all (orange) for `flagged`/unknown.


## Progress

- [x] (2026-06-29) M1 — Data layer: added `RECOVERY_VERIFIED = "verified"` + doc in `src/automation/results_edit.rs`; new test `test_verified_recovery_roundtrips` proves a `verified` row load→save→load unchanged. `cargo test results_edit` → 5 passed.
- [x] (2026-06-29) M2 — GUI state: added `show_verified: bool` to `ReviewState` (`src/gui/state.rs`) + `Debug` field + comment; initialised `false` in `handle_open_review` (`src/gui/mod.rs`).
- [x] (2026-06-29) M3 — GUI render + action: `recovery_color` teal `verified` arm; "verified" filter checkbox + filter match arm; per-row "✓" button on `flagged`/`repaired` rows; `ReviewActions::mark_verified: Option<u32>`; dispatch in `src/gui/mod.rs` (sets `recovery=verified`, `dirty=true`). `cargo test` → 110 passed, no errors.
- [x] (2026-06-30) M4 — Manual GUI click-through confirmed by the user: opening review on a session with a flagged row, clicking ✓ → reopen shows the `verified` badge with scores unchanged and the row written `verified` in `results.csv`. Plan complete.

Use timestamps when you check items off, e.g. `- [x] (2026-06-29 14:00Z) ...`.


## Surprises & Discoveries

- Observation (M4 manual check): adding the per-row "✓" button pushed the "📷" expand button off the right edge of the row. The table's cell width is computed by reserving a fixed trailing width for the `状態` badge + buttons cluster (`render_review_window_contents`), and that reserve was `150.0`, sized for only the badge + 📷. The new button overflowed it, so the row clipped the 📷 (it looked like a fixed-width, non-responsive row). Fix: raised the reserve to `trailing_w = 200.0` so all three (badge + ✓ + 📷) fit; the nine score cells still size dynamically from the remainder (clamped 60–160px).
  Evidence: user screenshot of run `20260629_014136` iter 181 showing "flagged ✓" with the 📷 cut off at the window edge.


## Decision Log

- Decision: Represent the resolved state as a new `recovery` string value `verified`, not a separate boolean column.
  Rationale: The CSV schema and `results_edit.rs` treat `recovery` as one free-form string, and `analysis::csv_reader` indexes only the first twelve columns, so a new *value* in the existing column is non-breaking and needs no schema migration. A new column would change the header and risk older readers. `verified` is mutually exclusive with the other states (a row is exactly one of ok/repaired/flagged/manual/verified), which a single column expresses naturally.
  Date/Author: 2026-06-29.

- Decision: Make "verify" an explicit per-row button rather than auto-clearing flagged rows that happen to satisfy the checksum.
  Rationale: The user owns the truth. A flagged row is flagged precisely because the checksum has multiple equally-valid solutions; the stored value is one arbitrary valid option (the solver's deterministic tie-break), so "satisfies the checksum" does NOT prove the stored value is the *correct* one. Only a human looking at the screenshot can confirm it. An explicit action records that human judgement; auto-clearing would re-introduce the silent-error risk the flagging exists to prevent.
  Date/Author: 2026-06-29 (user chose "add a 'verified' state" over "auto-clear when checksum-consistent").

- Decision: Show the "✓" verify button only on `flagged` and `repaired` rows.
  Rationale: `ok` rows need no confirmation; `manual`/`verified` rows are already resolved. `flagged` is the primary case; `repaired` is included because a user may also want to sign off on an automatic repair. Keeping the button off the resolved/clean states avoids clutter and accidental status churn.
  Date/Author: 2026-06-29.

- Decision: Default the new `verified` filter to OFF.
  Rationale: The window is attention-first (it defaults to showing `flagged` + `repaired`). A verified row is resolved, so like `ok`/`manual` it should be hidden by default; ticking its checkbox (or "すべて表示") reveals it for audit.
  Date/Author: 2026-06-29.


## Outcomes & Retrospective

- (2026-06-29) M1–M3 landed; the `verified` state exists end-to-end in code and is unit-tested (data-layer round-trip; full suite 110 passed, no regression). M4's automated portion (release build) is green; the interactive click-through is the only remaining step and is inherently manual (the tray GUI cannot be driven from `cargo test`). What now works that did not before: a `flagged`/`repaired` row in the review window shows a "✓" button; clicking it marks the row `verified` (teal badge) and dirties the window; saving writes `verified` to `results.csv` with the nine scores untouched; the row leaves the default attention view because the new "verified" filter defaults off. The change is additive — no existing recovery value, field, or signature was removed, so prior behaviour (ok/repaired/flagged/manual, edit→manual) is unaffected.


## Context and Orientation

The application is a Windows system-tray automation tool in Rust with an egui GUI. You need to know nothing else about it. The four files this plan touches, by full repository-relative path:

- `src/automation/results_edit.rs` — the data layer for the review feature. Defines `ReviewRow { iteration, timestamp, screenshot, scores: [[u32;3];3], recovery: String }`, the constant `RECOVERY_MANUAL: &str = "manual"`, and the functions `load_review_rows(session_dir) -> Result<Vec<ReviewRow>>` and `save_review_rows(session_dir, &[ReviewRow]) -> Result<()>`. Loading tolerates a missing `recovery` (defaults to `ok`); saving writes whatever string each row holds. It has a `#[cfg(test)] mod tests` with `tempdir`-based tests.

- `src/gui/state.rs` — defines `ReviewState` (the window's in-memory model; see fact 2 above) and a hand-written `impl Debug for ReviewState` (because the GPU texture handle is not `Debug`). The per-status booleans and a doc comment describing their defaults live here.

- `src/gui/render.rs` — defines `pub struct ReviewActions { save, close, toggle_expand: Option<u32> }`, the helper `recovery_color`, and `pub fn render_review_window_contents(ui, review: &mut ReviewState, actions: &mut ReviewActions)` which draws the filter checkboxes, computes the `visible` index list, and draws the table rows (each with nine `TextEdit` cells, a status `Label`, and the "📷" button).

- `src/gui/mod.rs` — owns the window. `handle_open_review` constructs the `ReviewState` (this is where the `show_*` booleans get their initial values). `render_review_window` shows the egui viewport, calls `render_review_window_contents`, then dispatches `ReviewActions` (the `if actions.save { ... }` etc. block). `handle_save_review` performs the save (fact 4). It imports `RECOVERY_MANUAL`.

Terms: "recovery state" = the `recovery` column's value for a row. "flagged" = the reader could not confirm the row. "verified" (new) = a human confirmed a flagged/repaired row is correct without editing it. "badge" = the small coloured status label at the end of each table row. "dirty" = there are unsaved changes; it enables the 保存 (save) button.


## Plan of Work

Work proceeds in four milestones, each independently verifiable. The change is additive: no existing state value or function signature is removed, so existing behaviour (ok/repaired/flagged/manual, editing → manual) is unaffected.

Milestone M1 (data layer). In `src/automation/results_edit.rs`, immediately after the `RECOVERY_MANUAL` constant, add:

    /// The recovery marker for a flagged/repaired row the user reviewed and
    /// confirmed correct without editing it (resolves the flag, preserves data).
    pub const RECOVERY_VERIFIED: &str = "verified";

Extend the `recovery` field doc comment on `ReviewRow` to mention `verified`. Add a test to the existing `mod tests` that writes a `results.csv` whose row has `recovery=verified`, loads it, asserts `rows[0].recovery == "verified"`, saves, reloads, and asserts it is still `verified` (proving the data layer carries the new value verbatim — it will, because it already round-trips arbitrary strings, but the test pins the behaviour).

Milestone M2 (GUI state). In `src/gui/state.rs`, add `pub show_verified: bool,` to `ReviewState` next to the other `show_*` fields, and add `.field("show_verified", &self.show_verified)` to the hand-written `Debug` (optional but tidy). Update the doc comment so it notes `verified` defaults off. In `src/gui/mod.rs::handle_open_review`, add `show_verified: false,` to the `ReviewState { ... }` initialiser (the struct literal will otherwise fail to compile, which is a useful compiler-enforced reminder).

Milestone M3 (render + action). In `src/gui/render.rs`:

- In `recovery_color`, add an arm `"verified" => Color32::from_rgb(0, 160, 130),` (a teal, distinct from `ok` green, `repaired` blue, `manual` purple, `flagged` orange).
- In the filter checkbox row of `render_review_window_contents`, add `status_chk(ui, &mut review.show_verified, "verified");` after the `manual` checkbox.
- In the `visible` filter closure's match, add the arm `"verified" => review.show_verified,`.
- Add `pub mark_verified: Option<u32>,` to `ReviewActions`.
- In the per-row drawing, after the status badge label and before/after the "📷" button, when the row's recovery is `"flagged"` or `"repaired"`, draw a small button `"✓"` with hover text "確認済みにする（値はそのまま）"; on click set `actions.mark_verified = Some(iteration);`.

In `src/gui/mod.rs::render_review_window`, in the dispatch block after `render_review_window_contents`, add handling for `actions.mark_verified`: find the row with that iteration in `self.state.review`'s `rows`, set its `recovery = RECOVERY_VERIFIED.to_string()`, and set `review.dirty = true` (so 保存 lights up). Import `RECOVERY_VERIFIED` alongside `RECOVERY_MANUAL`.

Note on save interaction (fact 4): `handle_save_review` only overwrites `recovery` to `manual` when a row's *scores* changed. A verified-but-unedited row keeps `verified` through the save. If a user both verifies and edits the same row, the edit wins (it becomes `manual`), which is correct — an edited row is a manual correction.

Milestone M4 (build, run, verify). Build the release binary with the guarded wrapper, run the app, open the review window on a session containing a `flagged` row, verify the new button/badge/filter behave as described, and record the result in Outcomes & Retrospective.


## Concrete Steps

Work from the repository root `C:\Work\GitRepos\gakumas-rehearsal-automation` in PowerShell unless noted. The Bash tool is also available.

Run the data-layer test as you implement M1 (the admin manifest blocks the test harness unless you skip it via the build gate):

    GAKUMAS_NO_MANIFEST=1 cargo test results_edit

Expect the existing `results_edit` tests plus the new `verified` round-trip test to pass.

Compile-check the GUI changes (M2/M3) without a full release build:

    GAKUMAS_NO_MANIFEST=1 cargo test 2>&1 | grep -E "^error" || echo "no errors"

Expect no `^error` lines (≈30 pre-existing warnings are normal).

Build the release binary (guarded; a running instance locks the exe). PREFER:

    powershell -ExecutionPolicy Bypass -File scripts/build.ps1 -Kill

Run it:

    ./target/release/gakumas-rehearsal-automation.exe


## Validation and Acceptance

Acceptance is observable behaviour in the review window, not code shape.

1. Unit (M1): `GAKUMAS_NO_MANIFEST=1 cargo test results_edit` passes, including the new test that a `verified` row round-trips through load → save → load unchanged. This test fails before M1 (the test, and the constant it references, do not exist) and passes after.

2. End-to-end (M4), manual since the GUI cannot be driven from `cargo test`:
   - Use a session whose `results.csv` has at least one `flagged` row. A real one is `target/release/output/20260629_014136` (iteration 181 is `flagged` and correct). If that folder is absent, run a short automation, or hand-author a `results.csv` with a `flagged` row in a scratch session folder.
   - Launch the app, click "📝 結果を確認・修正". The window opens defaulting to the `flagged` + `repaired` rows. The new "verified" checkbox is present and unticked.
   - On a `flagged` row, observe the new "✓" button. Click it: the row's badge immediately changes to `verified` (teal) and "● 未保存の変更" appears (dirty). Because the `verified` filter is off, after the next repaint the row is filtered out of the default view (it remains visible only if you tick "verified" or "すべて表示").
   - Click "💾 保存". Reopen the window (or tick "verified"): the row shows `recovery=verified`. Open `results.csv` in the session folder and confirm that row's last column is `verified` and its nine scores are unchanged.
   - Confirm an `ok` row shows no "✓" button, and that editing a cell still flips a row to `manual` (regression check).


## Idempotence and Recovery

All steps are safe to re-run. `cargo test`/`cargo build` are idempotent. The change is additive: no existing `recovery` value, struct field, or function is removed, so older `results.csv` files (including ones with no `verified` rows) load and save exactly as before. Clicking "✓" twice on the same row is harmless (it is already `verified`). Saving is atomic (temp file + rename in `save_review_rows`), so an interrupted save cannot truncate the CSV. To undo a mistaken verify before saving, simply do not save (close the window) or edit the row (which makes it `manual`); after saving, re-open and the value can be changed by editing.


## Artifacts and Notes

Real flagged-but-correct example that motivated this plan (run `20260629_014136`, iteration 181, stage 2): stored `[848392, 1340813, 1026578]` with the screen's total `3,483,945` and bonus `268,162`. The values satisfy the checksum exactly (`848392+1340813+1026578+floor(1340813/5) = 3483945`), so they are correct — but the reader flagged the row because the required units correction could have landed on either non-max column, an ambiguity the checksum cannot resolve. This is precisely the kind of row the new `verified` action is for: a human confirms it, the flag clears, the data is untouched.


## Interfaces and Dependencies

In `src/automation/results_edit.rs`, define:

    pub const RECOVERY_VERIFIED: &str = "verified";

In `src/gui/render.rs`, `ReviewActions` gains:

    pub mark_verified: Option<u32>,   // iteration to mark verified

and `recovery_color` gains a `"verified"` arm. In `src/gui/state.rs`, `ReviewState` gains:

    pub show_verified: bool,

No new external crates. egui/eframe are already dependencies. The dispatch in `src/gui/mod.rs` uses the existing `RECOVERY_VERIFIED` constant and the existing `review.dirty`/`save_review_rows` machinery.
