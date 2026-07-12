# Bilingual CHANGELOG, in-app 更新履歴 window, Japanese update hint, and flagged-only review default

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. It must be maintained in accordance with `docs/PLANS.md` (repository root: `C:\Work\GitRepos\gakumas-rehearsal-automation`).

## Purpose / Big Picture

Three user-visible improvements, decided in a design interview on 2026-07-12:

1. The OCR result review window opens showing only the rows that actually need attention: the status filter defaults to `flagged` only (today `repaired` is also pre-checked, burying the urgent rows among already-auto-repaired ones).
2. The update notice's hover hint (the text shown when hovering the アップデート button in the header) reads in Japanese. Today it shows the first paragraph of the GitHub release body, which has always been written in English.
3. The app gains a 更新履歴 (release history) window: a header button opens a scrollable, Japanese-only list of what changed in every version, always exactly matching the running binary — with no network access.

Behind these sits one durable convention: release notes are henceforth written bilingually in a repo-root `CHANGELOG.md` — Japanese for users (it feeds the in-app window and the release body's first paragraph), English for maintainers and coding agents (it stays in the file and on GitHub but is hidden in-app). The release procedure (`.claude/commands/release.md`) drafts notes in `CHANGELOG.md` first and copies that version's section into the GitHub release body.

After this plan: run the app, click 更新履歴 in the header → a window lists every version back to v0.1.0 in Japanese. Open the review window on a session with flagged rows → only `flagged` is checked. Fetch `https://rehearsal-automation.tia.run/latest.json` → its `notes` field is a Japanese sentence.

## Progress

- [x] (2026-07-12) Design interview complete; all decisions recorded in the Decision Log. Plan drafted.
- [x] (2026-07-12) M1: review window default filter → flagged only; defaults extracted into `ReviewState::with_default_filters` (`src/gui/state.rs`) + unit test. Commit `c4f68dc`.
- [x] (2026-07-12) M2: `CHANGELOG.md` authored — format header + full bilingual backfill, 15 sections v0.1.0 → v0.9.1 from the old release bodies (all 15 tags had one; none needed the git-log fallback). The v0.9.0 entry restores the live box-plot feature the published body omitted. Commit `b442dbb`.
- [x] (2026-07-12) M3: in-app 更新履歴 window — `src/gui/changelog.rs` (include_str! embed, `japanese_only` filter with 5 unit tests incl. one over the real embedded file, plain-egui renderer), header button left of フィードバック, floating scrollable window. Full suite 154 passed / 0 failed. Commit `ba1e074`.
- [x] (2026-07-12) M4: `.claude/commands/release.md` step 5 rewritten (CHANGELOG-first drafting, commit-before-build warning, first paragraph MUST be the one-line Japanese summary); v0.9.1 body retro-edited via bot PAT (JP one-liner prepended, rest verbatim; author verified `tia-tools-bot`); live manifest verified: `latest.json` `notes` = 「固定ダウンロードリンク、署名付き自動アップデート、匿名利用統計を追加」.
- [ ] M5: manual acceptance — user click-through: review window opens flagged-only; 更新履歴 window shows all 15 versions in Japanese, scrolls, no English visible; (optional) hover hint on a forced-old build.

## Surprises & Discoveries

- Observation: a `CHANGELOG.md` shipped inside the release zip would go permanently stale for auto-updating users.
  Evidence: `src/update/install.rs` `stage_from_zip` (the `dest.exists()` → `continue` branch, ~line 233) deliberately never overwrites an existing root file — that is how `config.json` calibration survives updates. Only the exe and `resources/` are replaced. This forced the embed-in-binary decision (see Decision Log).
- Observation: the "first paragraph = in-app hint" mechanism has three replicas that must agree: the Cloudflare Worker (`infra/worker/worker.js` ~line 338, `split(/\r?\n\r?\n/)[0]`), the GitHub-fallback parser (`src/update/mod.rs::first_paragraph`), and the release-notes convention in `.claude/commands/release.md` step 5. Changing the convention (language) needs no code change precisely because all three only split, never interpret.

## Decision Log

- Decision: review window default filter = `flagged` only (`show_repaired` defaults to `false`; all other defaults unchanged).
  Rationale: `repaired` rows were auto-fixed and checksum-satisfying; `flagged` rows are the ones a human must act on. The user asked for exactly this. Interview confirmed "default selected row" meant the filter checkboxes, not a row-focus behavior.
  Date/Author: 2026-07-12, design interview.
- Decision: the Japanese update hint is achieved purely by convention — every release body MUST open with a one-line Japanese summary, then a blank line, then detail. No Rust or Worker change.
  Rationale: the hint is mechanically the body's first paragraph (Worker + fallback both just split on the first blank line). A structured `notes_ja` manifest field was rejected as machinery that still needs a source convention anyway.
  Date/Author: 2026-07-12, design interview.
- Decision: retro-edit the published v0.9.1 release body on `tia-tools/releases` so its first paragraph is Japanese, using the bot PAT.
  Rationale: current clients get the Japanese hint immediately; a body edit touches no assets or signatures, and `gh release edit` with the bot token preserves `tia-tools-bot` authorship. The "releases are immutable" concern was judged less important than fixing the only shipped install's UX.
  Date/Author: 2026-07-12, design interview.
- Decision: release notes live in a repo-root `CHANGELOG.md`, embedded into the binary at compile time via `include_str!`, displayed in-app by a new 更新履歴 window.
  Rationale: the user wanted both a repo CHANGELOG and an in-app viewer. Shipping the file in the zip fails silently (updater never overwrites existing root files — see Surprises). Embedding makes the in-app history always match the running exe, works offline, and needs no Worker/packaging/updater change. A browser link to the GitHub release page was rejected (context switch, English chrome, raw GitHub page for non-technical Japanese users).
  Date/Author: 2026-07-12, design interview.
- Decision: per-version CHANGELOG format = JP one-liner + JP bullets + a `### English` subsection; the in-app window hides `### English` subsections; the JP one-liner doubles as the release body's first paragraph.
  Rationale: user's principle — Japanese for users, English for maintainers/agents. The `### English` marker makes the split structural so the app can filter it; GitHub renders both.
  Date/Author: 2026-07-12, design interview.
- Decision: backfill the full history (every tag v0.1.0 → v0.9.1), bilingual — Japanese written retroactively.
  Rationale: one-time effort; the in-app window then shows a complete Japanese history and the file becomes the single English record of all releases. (Interview offered v0.2.0+; v0.1.0 is included for completeness since "full history" was the intent.)
  Date/Author: 2026-07-12, design interview.
- Decision: no "Unreleased" section in `CHANGELOG.md`; a version's entry is written (or finalized) during the release procedure, dated with the release date.
  Rationale: the release skill already forces a human stop for notes; an Unreleased section would drift and the in-app window should only show shipped versions (the embedded file in a released binary can by construction only contain entries up to its own version).
  Date/Author: 2026-07-12, planning.
- Decision: the 更新履歴 window is a floating `egui::Window` inside the main viewport (like the feedback form), not a separate OS window (immediate viewport) like the review window.
  Rationale: it is a read-only scroll of text; it does not need independent OS-level resizing, and the feedback form already establishes the floating-window pattern. Placement: header button labeled 更新履歴, in the same right-aligned cluster as フィードバック.
  Date/Author: 2026-07-12, planning (minor call, announced to user without objection).
- Decision: no ADR from this plan.
  Rationale: the conventions' durable homes are `.claude/commands/release.md` (release-time rules, read by every release) and `CHANGELOG.md`'s own header comment (format rules, read by every editor); the updater's root-file behavior is already documented in `install.rs` and CLAUDE.md. Applying docs/adr/README.md's three-gate test: reversible, not surprising once those docs are read, no cross-plan trade-off left undocumented.
  Date/Author: 2026-07-12, planning.

## Outcomes & Retrospective

(To be written at completion.)

## Context and Orientation

This is a Windows Rust GUI app (egui via eframe) that automates a game's rehearsal mode and OCRs score screenshots. Full build/run instructions are in `CLAUDE.md`. Key facts for this plan:

- The GUI main window (`src/gui/mod.rs`, method `update` on `GuiApp`) starts with a full-width header (`egui::TopBottomPanel::top("header_panel")`, ~line 1075) containing the app title, a right-aligned フィードバック button, a shortcut hint line, and — when an update is available — a green notice with an アップデート button whose hover text is `UpdateInfo.notes` (~line 1116).
- `UpdateInfo.notes` (defined in `src/update/mod.rs`) is filled from either the tia.run manifest's `notes` field (which the Cloudflare Worker in `infra/worker/worker.js` computes as the first blank-line-separated paragraph of the latest GitHub release body) or, on fallback, from `first_paragraph()` of the GitHub API's `body`. Nothing anywhere interprets the language — it is whatever the release author wrote first.
- The OCR review window is controlled by `src/gui/review.rs` (`ReviewController::open` constructs a `ReviewState`, defined in `src/gui/state.rs`, with filter booleans `show_flagged`, `show_repaired`, `show_ok`, `show_manual`, `show_verified`, `show_all`). The checkboxes render in `src/gui/render.rs` (~line 521) and the row-visibility filter applies them (~line 558).
- The self-updater (`src/update/install.rs`) replaces only the exe and `resources/` during an update; any other root file that already exists locally is never overwritten (that protects `config.json`). Consequence: files shipped in the zip root reach a user once, at first manual install, and never again.
- Releases: git tags live in this repo (`v0.1.0` … `v0.9.1`; `Cargo.toml` version currently `0.9.1`). Release bodies for v0.9.0+ are on the dist repo `tia-tools/releases` (access with the bot PAT: `set -a; source .env; set +a` then `GH_TOKEN="$GAKUMAS_DIST_TOKEN" gh …`); older release bodies are on the personal repo `Taka499/gakumas-rehearsal-automation` (ambient `gh auth` is fine for reading those). Some old tags may have no GitHub release; derive their notes from `git log vA..vB --oneline`.
- Unit tests run unelevated with `GAKUMAS_NO_MANIFEST=1 cargo test` (the env var skips embedding the admin manifest; see CLAUDE.md "Testing limitations").
- Term: "immediate viewport" = a separate OS window egui renders synchronously (used by the review window). The 更新履歴 window deliberately does NOT use one (see Decision Log).

## Plan of Work

Milestone 1 changes one default in `src/gui/review.rs`: `ReviewState` construction in `ReviewController::open` sets `show_repaired: false` (was `true`). To make the default testable as pure logic, move the literal into a constructor `ReviewState::with_default_filters(session_path, rows, edits)` in `src/gui/state.rs` (or an equivalent associated fn) and assert the defaults in a unit test: `flagged` on; `repaired`, `ok`, `manual`, `verified`, `all` off.

Milestone 2 creates `CHANGELOG.md` at the repository root. It opens with an HTML comment documenting the format rules (so the file is self-governing):

    <!--
    Format (per docs/EXECPLAN_CHANGELOG_AND_JP_NOTES.md):
    - One "## vX.Y.Z — YYYY-MM-DD" section per release, newest first.
    - First line under the heading: one-line Japanese summary. It doubles as the
      first paragraph of the GitHub release body (= the in-app update hover hint),
      so it must stand alone and read naturally.
    - Then Japanese bullets (user-facing).
    - Then a "### English" subsection (for maintainers/agents). The in-app
      更新履歴 window hides everything from "### English" to the next "## ".
    - No Unreleased section: entries are written during the release procedure.
    -->

Then one section per tag, newest first, v0.9.1 down to v0.1.0. Source material: `GH_TOKEN="$GAKUMAS_DIST_TOKEN" gh release view vX.Y.Z -R tia-tools/releases --json body -q .body` for v0.9.x; `gh release view vX.Y.Z -R Taka499/gakumas-rehearsal-automation --json body -q .body` for older; `git log vA..vB --oneline` where no release exists. Japanese entries are written fresh (user-facing tone, what the user can now do); English entries may condense the old bodies.

Milestone 3 adds the in-app window. New module `src/gui/changelog.rs` containing: the embedded text (`pub const CHANGELOG_MD: &str = include_str!("../../CHANGELOG.md");` — path relative to the source file), a pure function that strips English subsections, and the render function. The filter's exact semantics (unit-tested): scan line by line; drop everything before the first line starting with `## ` (the format-rules comment and any intro); when a line whose trimmed text equals `### English` is seen, skip lines until the next line starting with `## ` (exclusive). Rendering (inside `egui::Window::new("更新履歴").open(&mut open_flag)` with a `ScrollArea::vertical`): lines starting with `## ` render as a strong/larger heading (strip the `## `), lines starting with `- ` as labels with a small indent, other non-empty lines as plain labels, blank lines as small vertical space. No markdown crate. State: a `show_changelog: bool` on `GuiApp` (not persisted). Header: in the right-aligned cluster in `src/gui/mod.rs` (~line 1081), add a 更新履歴 button after the フィードバック button (right-to-left layout ⇒ it appears to フィードバック's left) toggling the flag; render the window near the other floating windows in `update()`.

Milestone 4 amends `.claude/commands/release.md` and performs the retro-edit. In step 5, replace the notes-drafting instruction with: notes are authored in `CHANGELOG.md` first (add the new `## vX.Y.Z — date` section following the file's format comment; commit it with the version bump), and the release body is that version's section content (heading dropped, JP one-liner first — preserving the existing first-paragraph rule, which now explicitly says the first paragraph MUST be the one-line JAPANESE summary). Note in the skill that the English subsection stays in the body (GitHub shows both). Then edit the published release:

    set -a; source .env; set +a
    GH_TOKEN="$GAKUMAS_DIST_TOKEN" gh release edit v0.9.1 -R tia-tools/releases --notes-file <tempfile>

where the tempfile is the old body with a Japanese one-liner + blank line prepended (keep the rest). Verify author unchanged (`gh release view v0.9.1 … --json author`) and the manifest: `curl -s https://rehearsal-automation.tia.run/latest.json` → `notes` is the Japanese line (the Worker edge-caches; allow its CACHE_TTL to pass or expect brief staleness).

## Concrete Steps

Working directory: repo root. Build with the guarded wrapper (`powershell -ExecutionPolicy Bypass -File scripts/build.ps1`) if the app may be running; tests with `GAKUMAS_NO_MANIFEST=1 cargo test` (expect all green; ~121 existing tests plus this plan's new ones). Commit per milestone (small commits, no Claude attribution — see CLAUDE.md COMMIT DISCIPLINE).

## Validation and Acceptance

- M1: `GAKUMAS_NO_MANIFEST=1 cargo test` — the new default-filter test passes (and fails if `show_repaired` is flipped back). Manual: open 結果を確認・修正 on any session → only `flagged` is checked; only flagged rows listed.
- M2: `CHANGELOG.md` has 15 `## v` sections (one per tag), each with a JP one-liner, JP bullets, and a `### English` subsection.
- M3: unit tests for the filter function (English subsection dropped; two consecutive versions both survive; text before the first `## ` dropped; `### English` as the last section drops through end-of-file). Manual: run the app → 更新履歴 button in header → window opens, scrolls, shows Japanese only, all versions present.
- M4: `gh release view v0.9.1 -R tia-tools/releases --json author -q .author.login` → `tia-tools-bot`; `curl -s https://rehearsal-automation.tia.run/latest.json` → Japanese `notes`. The in-app hover hint itself can only be observed from a build whose version is older than the channel's (optional: temporarily set `Cargo.toml` version to 0.9.0, build, run, hover — never release such a build).

## Idempotence and Recovery

All code edits are ordinary commits (revertable). The only outward-facing step is the v0.9.1 body edit; it is itself idempotent (re-running `gh release edit --notes-file` with the same file is a no-op) and reversible (the pre-edit body is preserved in this plan's Artifacts section before editing). Assets, tags, and signatures are never touched.

## Artifacts and Notes

Pre-edit v0.9.1 release body (captured 2026-07-12 before the M4 edit; the edit only PREPENDS a Japanese one-liner + blank line, everything below is kept verbatim):

    Permanent download link, cryptographically signed auto-updates, and anonymous usage statistics.

    ## Changes

    ### Permanent download link
    The latest version is always available at https://rehearsal-automation.tia.run/download — one link that never goes stale.

    ### Signed, tamper-proof updates
    In-app updates are now cryptographically signed and verified before installation, so a compromised distribution server cannot push a modified build.

    ### Anonymous usage statistics
    Downloads and update checks are now counted anonymously (date, version, country, and a daily-rotating bucket) to gauge active usage. No IP addresses or persistent identifiers are stored — see the README's Privacy section.

    ## Install
    Download `gakumas-rehearsal-automation-v0.9.1.zip`, extract to an administrator-only location (e.g. under `C:\Program Files\`), and run `gakumas-rehearsal-automation.exe` as administrator. Embedded Tesseract OCR extracts on first run.

Prepended Japanese one-liner (same as CHANGELOG.md's v0.9.1 summary): 固定ダウンロードリンク、署名付き自動アップデート、匿名利用統計を追加

## Interfaces and Dependencies

No new crates. In `src/gui/changelog.rs`:

    pub const CHANGELOG_MD: &str = include_str!("../../CHANGELOG.md");
    /// Returns the changelog with every "### English" subsection removed
    /// (from that heading up to, excluding, the next "## " line) and any
    /// preamble before the first "## " dropped.
    pub fn japanese_only(md: &str) -> String;
    /// Renders the (already filtered) text into the given Ui.
    pub fn render_changelog(ui: &mut egui::Ui, text: &str);

`GuiApp` gains `show_changelog: bool`. `ReviewState` gains an associated constructor carrying the default filter values so they are unit-testable.
