# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## ExecPlans

When writing complex features or significant refactors, use an ExecPlan (as described in docs/PLANS.md) from design to implementation.

When working on execution plans (ExecPlans), always read the full plan document completely before beginning any implementation or summarization. Confirm understanding by listing all milestones/phases before proceeding.

Active ExecPlans (keep their `Progress` sections current; each is self-contained):
- `docs/EXECPLAN_RESUME_AUTOMATION.md` - resume an interrupted automation run. Complete (committed d968a4a, acceptance passed).
- `docs/EXECPLAN_GUI_STATE_DRIVEN_PANEL.md` - redesign the GUI third column into a state-driven control panel. Code-complete (merged f8e5230); manual acceptance pending. See its Progress section for status.
- `docs/EXECPLAN_ADDITIONAL_RUNS_AND_PRESETS.md` - add "追加実行" (extend a finished series into the same folder) and 100/200/500/1000 preset run-count buttons. Code-complete (M1–M3); manual acceptance pending. See its Progress section for status.
- `docs/EXECPLAN_OVERLAP_SCORE_RECOVERY.md` - recover per-character scores when two ≥1,000,000 values overlap in the rehearsal UI (one digit collides; right number's leading "1" is always lost). Uses a structural re-split plus the on-screen `total = c1+c2+c3+bonus` checksum to reconstruct/flag. Not started; see its Progress section.
- `docs/EXECPLAN_REVIEW_INLINE_STAGE_CROPS.md` - refine the OCR review window: replace the right-hand whole-screenshot preview with inline, expand-on-demand, per-stage crops (character icons + printed scores) placed under each stage's editable columns, sized dynamically to the column-group width. Crop derived from `score_regions` + a configurable `ReviewCropAdjust` offset (so the dev's future horizontal re-layout tracks from one source). Complete (M1–M5, incl. field-calibrated `ReviewCropAdjust` defaults via `scripts/region_tuner.py`).
- `docs/EXECPLAN_REVIEW_VERIFIED_STATE.md` - add a `verified` recovery state + per-row "✓" action to the GUI review window, so a correct-but-flagged row can be resolved without editing its values. Complete (M1–M4; manual click-through confirmed 2026-06-30).
- `docs/EXECPLAN_REVIEW_SAVE_UX.md` - review-save UX: auto-save when a row is verified (✓), regenerate charts after a score-changing save, and show a post-run "N rows need checking" prompt in the finished panel. Complete (M1–M5; manual click-through confirmed 2026-06-30).
- `docs/EXECPLAN_TOOLS_SOLVER_PARITY_PORT.md` - port the five post-PR#102 Rust OCR-solver capabilities (Prepend candidates, "1"→"2" swap, comma-leaked-total deletion, bonus-driven/single-char no-checksum fallbacks, multi-score-unsatisfiable flag) into the `gakumas-tools` web fork (`C:\Work\GitRepos\gakumas-tools\root-pr`), bringing `rehearsalRecovery.js` to parity with `src/ocr/reconcile.rs`. Keystone is provenance-tagged candidates so `cost()` is faithful. Plan kept here, NOT committed to the fork. Code-complete (M0–M6; branch `feature/rehearsal-solver-parity-root`, commits `c549037b..bd72f65c`): 23/23 unit vectors, e2e 8/8, and the 1200-row replay matches Rust exactly (`temp/tools-port-tests/replay.mjs`). **Contributed upstream as `surisuririsu/gakumas-tools#103`** (parity branch; carries the single-char base commit). Continues `docs/EXECPLAN_TOOLS_OVERLAP_PORT.md`.
- `docs/EXECPLAN_LIVE_BOX_PLOT.md` - live nine-box score-distribution figure shown in a wide right-hand GUI side panel (box plot image + a live 6×9 Avg/Med/Max/Min/Q1/Q3 table) that updates as each iteration's OCR result lands; flagged rows excluded until verified. Data flows via a `LIVE_SCORES` global in `runner.rs` pushed from `ocr_worker.rs`; plotters renders to an in-memory RGBA buffer uploaded as an egui texture. Toggle defaults on and is persisted to `gui_settings.json`; the buffer is seeded from the newest session on launch and reloaded after review corrections/verifications. Complete (M1–M3 + polish; merged to main at `122be10`).
- `docs/EXECPLAN_SUSTAINABILITY_REFACTOR.md` - behaviour-preserving refactoring batch from the 2026-07 four-subsystem design audit: dedup the triplicated D3D11 capture pipeline / Tesseract invocation / nine-box drawing / CSV header, consolidate runner progress globals into one `Progress` mutex, bundle the three OCR region arrays into `OcrRegions`, extract `LiveChartCache` + `ReviewController` from `GuiApp`, and put the state machine behind a `GameOps` trait seam (making it unit-testable) with the abort check injected into `detection.rs`. `reconcile.rs` got named constants only (the plan cited a "JS-parity read-mostly zone" constraint since rescinded — per `docs/adr/0001`). Code-complete (M1–M9 merged, 121 unit tests incl. 6 new state-machine tests, e2e + pixel-diff proofs); final live-game click-through pending. See its Progress section.
- `docs/EXECPLAN_SCORE_ROW_THRESHOLD_RETRY.md` - score-row multi-threshold OCR retry: when a stage reconciles as Flagged, re-OCR the score-row crop at `SCORE_ALT_THRESHOLDS` and adopt the first checksum-exact re-read (mirrors the existing total retry; fixes the ocr_threshold-190 knife-edge trailing-digit misreads from field session 20260704_052007). Orchestration-only in `src/ocr/mod.rs`; `reconcile.rs` untouched (the plan cited a "JS parity" constraint since rescinded — per `docs/adr/0001`). Code-complete (M1–M2; e2e fail-before/pass-after; 499-row field replay: 0 rows flagged, sole mismatch was a wrong auto-repair in the CSV itself, since hand-corrected). See its Progress section.
- `docs/EXECPLAN_IMAGE_COPY_TO_CLIPBOARD.md` - right-click-to-copy for GUI images (live box plot + review stage crops): direct copy at native resolution via `arboard` (box plot re-rendered from cached `DataSetStats`; stage crop re-cropped from the screenshot PNG on disk), with a fade-in/out "📋 コピーしました" toast overlaid on the clicked image (red on failure). Complete (M1–M3 + acceptance 2026-07-07: user click-through on both surfaces; WinForms round-trip test pixel-exact). New image surfaces opt in via `src/gui/copyable.rs::copy_on_right_click` — the image widget MUST be built with `.sense(egui::Sense::click())` (bare egui images are hover-only and never see right-clicks). Clipboard reads go through an independent consumer, never `arboard::get_image` — per `docs/adr/0010`.
- `docs/EXECPLAN_AUTO_UPDATE_DISTRIBUTION.md` - auto-update + identity-separated distribution: neutral GitHub org dist repo (`tia-tools/releases`, authored by the `tia-tools-bot` machine account) behind a custom-domain Cloudflare Worker (`latest.json` manifest + download redirect); in-app updater (new `src/update/`) checks domain-first/GitHub-fallback, notify + one-click install (sha256 verify → exe rename-swap; `config.json` never touched). Governed by `docs/adr/0011`. Complete (M0–M5; v0.9.0 published 2026-07-08 as the dist repo's first release, author verified `tia-tools-bot`; M5 click-through passed 2026-07-08: forced-old build → v0.9.0, `config.json` untouched, `.exe.old` cleaned). Worker source lives in `infra/worker/`; the only updater URLs are in `src/update/endpoints.rs`; the bot PAT (repo-root `.env`, `GAKUMAS_DIST_TOKEN`, gitignored) expires mid-2027 — 401s from `/release` mean renew it.
- `docs/EXECPLAN_RELEASE_SIGNING.md` - sign release zips with a minisign/Ed25519 key held only on the dev's machine (never in the dist repo or Cloudflare); embed the public key in the binary and make the updater REJECT any download whose signature doesn't verify — closing the 2026-07-08 security review's finding #1 (manifest+zip share one origin, so the sha256 alone can't stop a malware push from a leaked bot PAT or CF account; app installs/restarts as admin). Also folds in review finding #2 (updater host allowlist) and #3 (writable-install-dir warning). Not started; drafted from the security review, tool choice (minisign vs Authenticode) pending user confirmation. See its Progress section.
- `docs/EXECPLAN_DIST_PERMALINK_AND_METRICS.md` - permanent version-less download URL (`https://rehearsal-automation.tia.run/download` → 302 to the latest zip; per-app subdomain is canonical and the Worker MOVES off the bare `tia.run` — no alias, since v0.9.0 was never distributed and the sole existing install rides the updater's GitHub fallback; second-level subdomains rejected — free Universal SSL covers one wildcard level only) + anonymous usage metrics on the dist Worker: Analytics Engine events for update checks/downloads (day, type, client version from the updater's UA, country, daily-rotating salted IP hash per `docs/adr/0012` — no persistent identifiers ever), nightly cron rollup into Workers KV for permanent history (AE only retains ~90 days), and a local reader `scripts/dist_stats.py` (tokens in gitignored `.env`). All $0 on the Workers free plan; sole Rust change is the `MANIFEST_URL` flip in `src/update/endpoints.rs`. Hostname scheme governed by `docs/adr/0014`. Complete (2026-07-10; shipped in v0.9.1, cron rollup proven firing unattended, real traffic observed; ADR 0012 + 0014 capture the durable decisions).
- `docs/EXECPLAN_TOOLS_REVIEW_FLAGGING.md` - separate follow-up in the `gakumas-tools` fork (branch `feature/rehearsal-review-flagging-root`): replace silent `[0,0,0]` zeroing of unverifiable rehearsal stages with keep-and-flag + highlight + "N rows need checking" + exclude-from-stats-until-verified, plus a total-confidence guard (portable analog of gakumas-rehearsal-automation `3be5b78`). Plan kept here, NOT committed to the fork. Code-complete (M1–M5; branch `feature/rehearsal-review-flagging-root`); zero-drop field check 0/700. **Contributed upstream as `surisuririsu/gakumas-tools#104`** (depends on #103; diff narrows to flagging-only once #103 merges).


- `docs/EXECPLAN_FEEDBACK_FORM.md` - in-app feedback form: a 「フィードバック」 header button opens a floating egui form (message + bug/request/other category; bug reveals a session-log picker, newest preselected, 添付しない available) that POSTs to `https://rehearsal-automation.tia.run/feedback`; the Worker (size-capped, rate-limited 5/day per ADR-0012 daily IP hash, counted only on success) creates a labeled issue in the private `tia-tools/feedback` repo via an issues-only fine-grained PAT (never the release PAT — per `docs/adr/0015`; `FEEDBACK_TOKEN` wrangler secret, expires 2027-07-13 → 502s on /feedback then mean renew it). Log travels inline, tail-truncated to ~60KB (GitHub: no issue-attachment API, 65,536-char body cap). Complete (2026-07-12: 9/9 Worker rejection probes, exact-truncation proof, real-UI issues #3–#5 incl. offline-retry, 429 on the 6th same-day submission).

## Durable Decisions (ADRs)

Cross-plan decisions live in `docs/adr/` (convention: `docs/adr/README.md`). This list is an index, not a home — one line per active decision; full context, provenance, and lifecycle live in the ADR file. Completed ExecPlans are immutable history; when an ADR corrects something a plan asserted, the ADR wins.

- `docs/adr/0001-reconcile-has-no-js-parity-obligation.md` — **accepted**: `src/ocr/reconcile.rs` has NO JS-parity obligation to the gakumas-tools fork (that constraint was a false agent inference; the #103/#104 ports were one-time contributions). `reconcile.rs` may be freely refactored, subject to its unit tests.
- `docs/adr/0002-region-based-ocr-supersedes-full-image.md` — **accepted**: per-stage `score_regions` cropping superseded the full-image OCR the Phase-2/Calibration plans declared final (full-image picked up too much noise; user-confirmed 2026-07-06).
- `docs/adr/0003-dual-mode-startup-gui-vs-tray.md` — **accepted**: GUI-by-default vs legacy-tray startup split + message-only hotkey window, forced by eframe/Win32 event-loop incompatibility.
- `docs/adr/0004-two-phase-loading-detection.md` — **accepted**: loading detection is histogram-vs-reference then brightness, because brightness alone cannot tell "Skip dimmed" from "Skip ready".
- `docs/adr/0005-bundled-cli-tesseract.md` — **accepted**: OCR shells out to a bundled `tesseract.exe` (30 MB zip embedded, extracted next to the exe); C bindings and %LOCALAPPDATA% rejected.
- `docs/adr/0006-score-recovery-solver-model.md` — **accepted**: reconcile solver = exhaustive search + total-only checksum (bonus demoted to cross-check) + asymmetric corruption-aware cost; `MAX_SCORE` is a soft 3,000,000 (leading digit may be 2).
- `docs/adr/0007-verification-is-an-explicit-human-act.md` — **accepted**: flagged rows never auto-clear on checksum satisfaction — only a human ✓ resolves them; `verified` is a `recovery` *value*, not a new CSV column.
- `docs/adr/0008-screenshots-are-the-source-of-truth-for-progress.md` — **accepted**: `completed` is always recomputed from the screenshot count (crash-proof); only `total` is trusted from `run-meta.json`.
- `docs/adr/0009-review-saves-rewrite-csvs-append-only-is-capture-scoped.md` — **accepted**: review saves rewrite both CSVs in full together; the append-only discipline is scoped to live capture only.
- `docs/adr/0010-arboard-is-write-only-verify-via-independent-consumer.md` — **accepted**: `arboard` is write-only (its `get_image` fails on its own "PNG" format even on healthy desktops); clipboard read-back verification shells out to WinForms `Clipboard.GetImage`, never `arboard::get_image`.
- `docs/adr/0011-identity-separated-distribution-channel.md` — **accepted**: distribution is identity-separated — new releases publish ONLY to a neutral-org dist repo (`tia-tools/releases`, authored by the `tia-tools-bot` machine account) fronted by a project domain; the in-app updater checks the domain first with org-GitHub-API fallback. Never publish new release assets to the personal repo (`/release` must use the bot PAT, not ambient `gh auth`).
- `docs/adr/0013-releases-are-minisign-signed.md` — **accepted**: the updater rejects any download whose minisign signature doesn't verify against the public key baked into the binary (`src/update/endpoints.rs::PUBLIC_KEY`); the secret key lives only on the dev's machine, never in git/dist-repo/Cloudflare. Signature is mandatory and checked before the hash. Never regenerate the key (breaks verification for shipped binaries); Authenticode rejected (cost + identity binding).
- `docs/adr/0014-per-app-subdomain-hostname-scheme.md` — **accepted**: each tia.run tool gets its own single-level subdomain and the Worker serves only that (this app: `rehearsal-automation.tia.run`); the bare root is unbound (reserved for a brand page), no root alias. `MANIFEST_URL` is baked into shipped binaries (hard to change). Second-level subdomains rejected (free Universal SSL covers one wildcard level only). Complements `docs/adr/0011`.
- `docs/adr/0012-metrics-are-anonymous-by-design.md` — **accepted**: distribution metrics carry NO persistent client identifiers — uniqueness comes only from a daily-rotating salted IP hash computed in the tia.run Worker (salt is a `wrangler` secret); dimensions are limited to day/event/client-version/country. Never add an install ID or store raw IPs.
- `docs/adr/0015-worker-holds-only-least-privilege-tokens.md` — **accepted**: the tia.run Worker holds only least-privilege tokens — the release-publishing bot PAT (`GAKUMAS_DIST_TOKEN`) never leaves the dev machine; any Worker feature needing GitHub access gets its own fine-grained, minimum-scope PAT (first instance: the feedback endpoint's issues-only PAT on the private `tia-tools/feedback` repo).

## Project Overview

Windows screenshot tool that captures the client area of `gakumas.exe` using Windows Graphics Capture API. Runs as a system tray application with global hotkey support. Includes rehearsal automation with embedded Tesseract OCR.

## COMMIT DISCIPLINE
- Follow Git-flow workflow to manage the branches
- Use small, frequent commits rather than large, infrequent ones
- Only add and commit affected files. Keep untracked other files as are
- Never add Claude Code attribution in commit

## Build Commands

Build emits ~30 expected warnings (unused `pub use` re-exports, OCR dead code); these are not regressions. Filter with `cargo check 2>&1 | grep "^error"` to find real failures.

```powershell
# Build release (optimized with LTO). PREFER the guarded wrapper: a running app
# instance locks gakumas-rehearsal-automation.exe, so a bare `cargo build --release` only
# fails at the LINK step after a full multi-minute compile ("failed to remove
# file ... gakumas-rehearsal-automation.exe"). build.ps1 checks for a running instance
# FIRST and aborts in ~1s (pass -Kill to stop it automatically). Always run this
# guard (or check `Get-Process gakumas-rehearsal-automation`) before building.
powershell -ExecutionPolicy Bypass -File scripts/build.ps1          # cargo build --release
powershell -ExecutionPolicy Bypass -File scripts/build.ps1 -Kill    # stop a running instance first
cargo build --release                                                # bare form (only safe if the app is closed)

# Create release package with proper folder structure (also guards the running app)
powershell -ExecutionPolicy Bypass -File scripts/package-release.ps1

# Run
.\target\release\gakumas-rehearsal-automation.exe
```

## Architecture

Multi-module Rust application with these key components:

- **src/main.rs**: Entry point, initializes GUI or legacy tray mode
- **src/paths.rs**: Centralized path resolution (logs/, screenshots/, output/, template/, tesseract/)
- **src/gui/**: egui-based GUI window. Layout is a top header panel + fixed-width left guide panel + wide right live-chart side panel + central control area (per `docs/EXECPLAN_LIVE_BOX_PLOT.md`; the earlier three-column framing is obsolete). The control panel is a single state-driven unit: `render.rs::render_control_panel` branches on `AutomationStatus` and returns a `PanelActions` struct that `mod.rs::update()` dispatches to `handle_*` methods. Add controls by emitting a button → setting a `PanelActions` field → dispatching it, not by rendering everything unconditionally.
- **src/capture/**: Window discovery and screenshot capture via Windows Graphics Capture API
- **src/automation/**: Rehearsal automation state machine, button detection, OCR worker, session metadata/resume (`session_meta.rs`). Every "run N iterations" variant — `start_automation` (fresh), `resume_automation` (finish remaining), `extend_automation` (add more to a finished series) — delegates to `runner.rs::start_automation_inner(iterations, start_iteration, existing_session)`; wrap it rather than duplicating the window/CSV/log/meta/thread setup. After starting, the GUI reads the live total/current from runner atomics (`get_total_iterations`/`get_current_iteration`), not by recomputing.
- **src/calibration/**: Interactive calibration wizard for button positions
- **src/ocr/**: Tesseract integration with per-stage crop→threshold→OCR→extract pipeline
- **src/analysis/**: Statistics calculation and chart generation (plotters)
- **src/feedback/**: In-app feedback client (per `docs/EXECPLAN_FEEDBACK_FORM.md`): session-log enumeration for the bug-report picker, UTF-8-safe log-tail truncation, blocking POST to the tia.run Worker's `/feedback` route (`infra/worker/worker.js`), which creates issues in the private `tia-tools/feedback` repo. Sender must run on a worker thread; message length is counted in UTF-16 units to match the Worker's JS check
- **src/update/**: Auto-update (per `docs/EXECPLAN_AUTO_UPDATE_DISTRIBUTION.md` / `docs/adr/0011`): launch-time check on a worker thread (domain manifest first, org GitHub API fallback — both URLs only in `endpoints.rs`), header notice + one-click install in the GUI, staged download→sha256 verify→zip extract→rename-swap in `install.rs`. Updates never write `config.json`/`gui_settings.json` or any existing root file; `.exe.old`/`resources.old` are cleaned at next launch. All check failures are silently `None` by design.

Key technical details:
- **Window Discovery**: `EnumWindows` + `QueryFullProcessImageNameW` to find target process
- **Screen Capture**: Windows Graphics Capture (WGC) API via `IGraphicsCaptureItemInterop::CreateForWindow`
- **GPU Pipeline**: D3D11 device creates staging texture, copies captured frame, maps for CPU read
- **Embedded Tesseract**: `include_bytes!` embeds tesseract.zip, extracted on first run to exe directory
- **OCR Pipeline**: Per-stage cropping (`score_regions` in config) → brightness thresholding → Tesseract `--psm 6` → sanitize leading garbage chars → regex extraction. Each stage processed independently to avoid cross-stage noise. Crop regions are tightened to exclude horizontal UI divider lines that confuse Tesseract layout analysis
- **Session folders**: Each automation series writes to `output/YYYYMMDD_HHMMSS/` holding `screenshots/`, `results.csv`, `session.log`, `charts/`, and `run-meta.json`. `run-meta.json` (written by `session_meta.rs`) records `total`/`completed`/`status`/`dismissed` so an interrupted series can resume into the same folder; `completed` is authoritatively recomputed from the screenshot count (crash-proof), not trusted from the file. `dismissed: true` (set via `dismiss_session`) hides a session from the resume picker without deleting its data

## Key Constants and Hotkeys

- Process matching: exact match `"gakumas.exe"` (case-insensitive)
- `HOTKEY_ID` (1): Ctrl+Shift+S - Screenshot
- `HOTKEY_AUTOMATION` (6): Ctrl+Shift+A - Start automation
- `HOTKEY_ABORT` (7): Ctrl+Shift+Q - Abort automation
- `HOTKEY_CLICK_TEST` (2): Ctrl+Shift+F9 - PostMessage click test
- `HOTKEY_SENDINPUT_TEST` (3): Ctrl+Shift+F10 - SendInput click test
- Output: `screenshots/gakumas_YYYYMMDD_HHMMSS.png`
- Log: `logs/gakumas_screenshot.log`
- Reference images: `resources/template/rehearsal/*.png`

## Windows API Notes

- Uses Rust 2024 edition requiring explicit `unsafe` blocks inside `unsafe fn`
- `EnumWindows` returns FALSE when callback stops early - don't treat as error
- `windows` crate v0.58 feature flags must match APIs used (see Cargo.toml)
- `SendInput` with `SetForegroundWindow` is required for game input (PostMessage is ignored)
- Must run as Administrator if game runs elevated (UIPI restriction)
- egui render fns: matching on `state.status` (or iterating `state.resumable_sessions`) while mutating sibling `GuiState` fields trips `E0502`. Clone the status (`let status = state.status.clone();`) or snapshot the list into an owned `Vec` first, then mutate freely

## Design Constraints

- **Admin privileges required**: The executable has a Windows manifest (`gakumas-rehearsal-automation.exe.manifest`) that requires administrator elevation. This is necessary for `SendInput` to work with elevated game processes.
- **No command-line arguments**: This is a system tray application, not a CLI tool. All functionality should be accessed via tray menu, hotkeys, or config file. Do not add command-line argument handling.
- **Testing limitations**: The admin manifest normally makes the `cargo test` harness require elevation (os error 740). Build tests with `GAKUMAS_NO_MANIFEST=1 cargo test` to skip embedding the manifest so unit tests run unelevated (the gate is in `build.rs`; normal/release builds still embed it). Pure-logic modules (`ocr::extract`, `ocr::reconcile`, `ocr::engine` parsing, `analysis`, `csv_writer`) are covered this way. Tesseract-dependent end-to-end checks are `#[ignore]`d and run explicitly, e.g. `GAKUMAS_NO_MANIFEST=1 cargo test ocr_overlap_recovery_e2e -- --ignored` (uses the embedded Tesseract + sample PNGs under `temp/`). Anything that drives the live tray app/hotkeys still must be tested manually. Two related gotchas (per `docs/EXECPLAN_IMAGE_COPY_TO_CLIPBOARD.md`): `src/main.rs` gates `windows_subsystem = "windows"` behind `cfg_attr(not(test), …)` — don't remove the gate, or all test output becomes invisible in interactive consoles (GUI-subsystem binaries never attach to them); and the `clipboard_roundtrip` ignored test must be run by a human from an interactive desktop terminal — agent/CI shells have no clipboard access (writes even "succeed" silently there).

## Roadmap

See `docs/ROADMAP_AUTOMATION.md` for the full automation feature roadmap. Current status:
- Phase 1: UI automation (clicking buttons) - complete
- Phase 2: OCR integration (Tesseract) - complete with embedded Tesseract
- Phase 3: Automation loop - complete with state machine
- Phase 4: Statistics and visualization - complete (CSV, charts, JSON)
- Phase 5: User interface - in progress (egui GUI implemented; resume of interrupted runs added; third-column UI redesign pending, see Active ExecPlans)
