# Auto-update + identity-separated distribution channel

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. It must be maintained in accordance with `docs/PLANS.md`.

The governing cross-plan decision is `docs/adr/0011-identity-separated-distribution-channel.md`; if this plan and that ADR ever disagree, the ADR wins.

## Purpose / Big Picture

Two user-facing gains. First, **auto-update**: today a user who wants a new version must be handed a fresh download link, download a zip, and re-extract it by hand. After this plan, the app checks for a newer version at launch, shows a "新しいバージョン vX.Y.Z" notice with an update button in the GUI header, and one click downloads, verifies, and installs the update, then offers a restart — the user's `config.json` (calibrated button positions, OCR regions) is never touched. Second, **identity-separated distribution**: today the download link is a GitHub release URL under the developer's personal account, whose profile shows a real name. After this plan, the shared link is a neutral project domain (e.g. `https://tia.run/latest`), backed by a releases-only repository under a neutral GitHub organization; a casual downloader never sees the personal account. (A determined person can still trace authorship through the open-source repo and its commit history — that residual traceability is explicitly accepted; see ADR-0011.)

You can see it working when: a locally built binary with a deliberately old version number, launched normally, shows the update notice within a few seconds, and clicking the button replaces the exe and `resources/`, preserves `config.json`, and after restart reports the new version — with every network request going to `tia.run` or `api.github.com/repos/tia-tools/...`, never to the personal account's URLs.

## Progress

- [x] (2026-07-08) M0: Names chosen; org, dist repo, machine account + PAT, domain, Cloudflare zone created (user-performed, agent-guided). Final names: org `tia-tools`, bot `tia-tools-bot`, domain `tia.run`. Logged-out People-tab check green; PAT stored in repo-root `.env` as `GAKUMAS_DIST_TOKEN` (`.env*` is gitignored; verified with `git check-ignore`); exit check `GH_TOKEN=... gh release list -R tia-tools/releases` passed (empty list). Small leftover for M1: `tia-tools/releases` README is still empty — write the neutral-voice install README when publishing the first release there.
- [x] (2026-07-08) M1: `/release` skill retargeted to the dist repo (bot-authored, sha256 sidecar, privacy scrub step); first release published there. `.claude/commands/release.md` rewritten (publishes to `tia-tools/releases` via `GH_TOKEN=$GAKUMAS_DIST_TOKEN` from `.env`, tags stay in this repo, sha256-sidecar + privacy-scrub + author-verification steps); dist repo README pushed via the bot PAT (commit `2744bba`). **v0.9.0 published 2026-07-08** as the dist repo's first release: privacy scrub 0 hits on exe+zip, author verified `tia-tools-bot`, assets = zip + `.sha256`, tag `v0.9.0` pushed to this repo, `Cargo.toml` bumped (`8a021d5` range).
- [x] (2026-07-08) M2: Cloudflare Worker deployed; endpoints verified. Worker source + `wrangler.toml` (custom_domain route) + deploy README under `infra/worker/` (commit `4443718`); user deployed via `wrangler deploy`. Verified live: `/latest.json` → 502 "No release" (correct until the first dist-repo release exists), `/latest` → 302. Manifest-content verification completes with the first release (M1 leftover).
- [x] (2026-07-08) M3: Updater core in the app (`src/update/`): endpoint constants (`endpoints.rs`), manifest fetch with GitHub fallback + `.sha256` sidecar fill-in, strict validation (semver triple, https URL, 64-hex sha256), `is_newer` compare, silent-failure contract. 9 new unit tests (parsers + version compare); full suite 133 passed / 0 failed via `GAKUMAS_NO_MANIFEST=1 cargo test`. `sha2` dep added for M4. Gotcha: a JSON fixture whose value starts with `"## ` terminates an `r#"…"#` raw string (`"#`) — fixtures use `r###`.
- [x] (2026-07-08) M4 code-complete: `src/update/install.rs` (staged download to a temp file in the exe dir → sha256 verify → zip extract routing exe→`.exe.new` / `resources/**`→`resources.new` / existing root files skipped → `swap_resources` with rollback → exe rename-swap keeping `.exe.old`), `cleanup_old_files()` called at startup in `main.rs`; GUI: `UpdateUiState` (Idle/Available/Downloading/ReadyToRestart/Failed) on `GuiState`, startup check thread + install worker thread report via an mpsc channel polled in `update()`, header notice with アップデート button (disabled mid-run) / spinner / 再起動 / dismissible red failure. 5 new unit tests (sha256 vector, zip routing incl. config.json skip, zip-slip rejection, both swaps); suite 138 passed / 0 failed; `cargo check` clean. Two gotchas recorded in Surprises. Live click-through is M5.
- [x] (2026-07-08) M5: End-to-end acceptance PASSED (user click-through): forced-old `0.0.1` build showed the header notice for v0.9.0, one-click アップデート → 再起動 came back as v0.9.0, `config.json` untouched, `.exe.old` cleaned on the following launch. Note: the negative-path checks (no-network silence, corrupted-sha256 abort) were not separately click-tested live; both contracts are unit-covered (`check_for_update` silent-`None` paths, sha mismatch bail in `download_and_install`).
- [x] (2026-07-08) PLAN COMPLETE — closed out; see Outcomes & Retrospective. This document is now immutable history.

## Surprises & Discoveries

- Observation: `zip::read::ZipFile::enclosed_name()` alone did not stop a `<root>/../evil.txt` entry in the version pinned here — the traversal survived to `Path::strip_prefix`, which happily returned `../evil.txt`, and the staged write escaped the exe dir.
  Evidence: `stage_rejects_zip_slip` wrote `evil.txt` into the tempdir's parent before the explicit guard was added. Fix: additionally require every component of the entry path to be `Component::Normal` and bail otherwise.
- Observation: a test that asserts on the *parent* of a `tempfile::tempdir()` is asserting on the shared system `%TEMP%`, which persists pollution across runs — the zip-slip test kept failing on the leftover `evil.txt` from its own earlier buggy run.
  Evidence: second `stage_rejects_zip_slip` failure fired on the escape-check assert even after the guard fix; deleting `%TEMP%\evil.txt` and nesting the test's exe dir one level down fixed it.
- Observation: a JSON test fixture whose string value starts with `"## ` (markdown heading in release notes) contains the byte sequence `"#`, which terminates an `r#"…"#` raw string mid-fixture.
  Evidence: 22 cascading parse errors in `update::tests`; fixed by using `r###"…"###` delimiters.

## Decision Log

- Decision: Layer the channel as GitHub org (identity boundary) + custom domain (presentation layer), rather than either alone.
  Rationale: The org gives a free, durable, non-identifying fallback URL if the domain ever lapses; the domain gives a clean shareable link and endpoint indirection. User chose the mixture explicitly.
  Date/Author: 2026-07-07, user via /grill-me.
- Decision: The org hosts a releases-only dist repo (`tia-tools/releases`); development stays in the personal repo.
  Rationale: A dist repo has no commit trail, so the download location shows no author at all; transferring the full repo would put the personal avatar one click away via commit history. Also avoids a risky repo migration.
  Date/Author: 2026-07-07, user via /grill-me.
- Decision: Updater endpoint order is domain-manifest first, org GitHub API fallback.
  Rationale: Domain gives future re-hosting freedom without stranding shipped binaries; GitHub fallback means a lapsed domain degrades gracefully instead of bricking update checks. Cost: two endpoints to keep consistent — mitigated by making the Worker stateless (it derives the manifest from the GitHub API, so there is one source of truth).
  Date/Author: 2026-07-07, user via /grill-me.
- Decision: Update UX is notify + one-click install; never silent.
  Rationale: Matches the project's explicit-human-act philosophy (ADR-0007's spirit); an admin-elevated binary silently replacing itself is also the shape AV heuristics dislike.
  Date/Author: 2026-07-07, user via /grill-me.
- Decision: Releases publish ONLY to the dist repo going forward; this repo keeps tags (Cargo.toml version history) but gains no new release assets. Existing v0.2.0–v0.8.0 releases stay.
  Rationale: One canonical source, no drift, and new share-links never point at the personal account. Old releases are already-accepted history.
  Date/Author: 2026-07-07, user via /grill-me.
- Decision: Dist-repo releases are created by a project-branded machine account (`tia-tools-bot`), not the personal account.
  Rationale: GitHub release pages display the creating account ("X released this") — personal authorship would leak identity on exactly the page downloaders land on. GitHub ToS permits one free machine account.
  Date/Author: 2026-07-07, user via /grill-me.
- Decision: Names (org, domain, bot) were initially placeholders (`<ORG>`, `<DOMAIN>`, `<BOT>`) until the user registers them (M0).
  Rationale: Nothing downstream is blocked; endpoints are constants in one file. Note: a domain containing "gakumas" references the game's trademark — prefer a neutral tool brand.
  Date/Author: 2026-07-07, user via /grill-me.
- Decision: Org is `tia-tools`, bot is `tia-tools-bot` (both created 2026-07-08); the root brand is the user's game ID "tia", not the circle name ドリンク警察/"drink*".
  Rationale: The user personally pays the yearly renewals and doesn't own the circle's brand, so the root identity should be their own persona (which the anonymity goal never needed to hide — it hides the legal name only). Pure `tia`/`tialab`/`tiatools` GitHub names were taken; `tia-tools` was free.
  Date/Author: 2026-07-08, user decision.
- Decision: Domain remained `<DOMAIN>` pending one event: `tia.run` (quoted $20 on Cloudflare's public search, RDAP-confirmed unregistered) could not be registered because the entire `.run` TLD was paused at Cloudflare Registrar until Jul 9, 2026, 00:39. If it registers at ~$20 with ~$20 renewal after the pause, take it; if it proves premium/reserved, fall back to `tiatools.dev` (or `tialab.dev`, price-verified $12.20). All other `tia.<tld>` were taken or premium (`tia.page` $54/yr, `tia.link` $750/yr).
  Rationale: 3-letter roots are premium almost everywhere; `tia.run` is the only candidate at standard-ish price, and the fallback keeps the org-matching brand.
  Date/Author: 2026-07-08, user + agent availability/price sweep.
- Decision: Domain is `tia.run` — registered in the user's Cloudflare account 2026-07-08; all `<DOMAIN>` placeholders in this plan resolved to it. The Worker serves the apex (`https://tia.run/latest.json`, `/download/...`, `/latest`); subdomains (e.g. `drinklab.tia.run`) remain free for future use.
  Rationale: The pause resolved in our favor at the expected price; the root brand is the user's persona per the naming decision above.
  Date/Author: 2026-07-08, user.
- Decision: The Worker is stateless — it proxies the dist repo's `releases/latest` and reshapes it into `latest.json` on the fly (with edge caching).
  Rationale: `/release` then never has to update the domain side; no KV state to drift from GitHub.
  Date/Author: 2026-07-07, agent, user-reviewed in the same session.
- Decision: An update replaces the exe and `resources/` wholesale, and never writes `config.json` or `gui_settings.json`.
  Rationale: `resources/` is app assets (reference templates); `config.json` holds per-machine calibration the user cannot cheaply recreate. New config keys already arrive via serde defaults.
  Date/Author: 2026-07-07, agent, user-reviewed in the same session.

## Outcomes & Retrospective

**Outcome (2026-07-08): everything the Purpose promised, delivered in one day.** The distribution channel is fully identity-separated: `tia-tools/releases` (bot-authored, README and all commits by `tia-tools-bot`), fronted by the `tia.run` Worker, with v0.9.0 published as its first release (privacy scrub 0 hits; author verified on the live page). The app self-updates: the M5 click-through took a forced-old 0.0.1 build to v0.9.0 via the header one-click flow, preserving `config.json` and cleaning `.exe.old` on the next launch. 14 unit tests added (suite 133→138 plus the 9 M3 tests landed mid-count; final 138 passed / 0 failed).

**What remains / accepted gaps:** the negative-path M5 checks (unplugged network, corrupted sha256) were unit-verified but not click-tested live. The `.sha256` sidecar is belt-and-suspenders alongside GitHub's asset `digest` field. WHOIS privacy and org People-tab privacy were verified once at setup; they are standing configuration, not code, so future audits are manual. The bot PAT expires in one year — the user holds a calendar reminder; when `/release` starts failing with 401s, that is why.

**Lessons learned:** (1) The interview surfaced two leaks a naive design would have shipped: GitHub release pages display the *creating account* (hence the machine account), and `gh` ambient auth would have silently used the personal identity (hence `GH_TOKEN` from `.env` spelled out in the skill). (2) Registry pricing is the real constraint on short domains — every `tia.<tld>` was premium or taken except `tia.run`, and only a TLD-wide registration pause (not name reservation) hid its availability; RDAP also returns false negatives for some registries (.io/.me). (3) `enclosed_name()` is not a sufficient zip-slip guard in the pinned zip version — an explicit all-components-`Normal` check is; the unit test that caught this also demonstrated that asserting on a tempdir's *parent* means asserting on shared `%TEMP%` pollution. (4) Building the Worker stateless (manifest derived from the releases API) meant the entire release pipeline stayed one command with zero infra state to update — that choice paid for itself on the very first release.

## Context and Orientation

This repository builds `gakumas-rehearsal-automation.exe`, a Windows GUI (egui/eframe) tray-capable tool that automates rehearsal runs in the game client `gakumas.exe`. Its Windows manifest requires administrator elevation (needed for `SendInput` into an elevated game). The binary embeds a Tesseract OCR zip and extracts it next to the exe on first run. The app currently has **no networking code at runtime** even though `reqwest = { version = "0.12", features = ["blocking"] }` is already in `Cargo.toml` (a Phase-2 leftover) — this plan is where it first gets used. `zip = "2.2"` is also already a dependency (used for the embedded Tesseract extraction).

Distribution today: the maintainer runs the `/release` skill (defined in `.claude/commands/release.md`), which builds via `scripts/package-release.ps1`, assembles `release/gakumas-rehearsal-automation/` (exe + `config.json` + `resources/`), zips it as `gakumas-rehearsal-automation-vX.Y.Z.zip` with `gakumas-rehearsal-automation/` as the archive's top-level folder, and publishes with `gh release create` **to the personal repo**. The app's version is `Cargo.toml`'s `version` field, kept in sync with the git tag; at compile time it is available as `env!("CARGO_PKG_VERSION")` (e.g. `"0.8.0"`, no leading `v`).

Terms used below:

- *dist repo*: a GitHub repository under the new neutral org, named `releases` (full path `tia-tools/releases`), containing only a README and release assets — no source code, hence no commit/author trail worth speaking of.
- *manifest*: a small JSON document describing the newest release, served at `https://tia.run/latest.json`:

      {
        "version": "0.9.0",
        "notes": "one-paragraph summary of the release",
        "url": "https://tia.run/download/gakumas-rehearsal-automation-v0.9.0.zip",
        "sha256": "hex digest of the zip"
      }

- *rename-swap*: the standard Windows self-update trick. You cannot delete or overwrite a running exe, but you CAN rename it. So: rename the running `gakumas-rehearsal-automation.exe` to `gakumas-rehearsal-automation.exe.old`, write the new exe under the original name, and delete the `.old` file on the next launch.
- *MOTW (Mark of the Web)*: the NTFS zone marker browsers attach to downloads, which triggers SmartScreen warnings. Files written by the app itself (our updater) get no MOTW, so in-app updates avoid the SmartScreen friction that first-time browser downloads have.

GUI orientation: the window layout is a top header panel + left guide panel + right live-chart panel + central state-driven control panel. The control panel is rendered by `src/gui/render.rs::render_control_panel`, which branches on `AutomationStatus` and returns a `PanelActions` struct that `src/gui/mod.rs::update()` dispatches to `handle_*` methods. New controls are added by emitting a button → setting a `PanelActions` field → dispatching, never by rendering unconditionally. The update notice lives in the header panel; its button follows the same action-struct pattern.

## Plan of Work

### M0 — Infrastructure prerequisites (user-performed, agent-guided)

Everything here happens on GitHub/Cloudflare/registrar websites, not in this repo. The user: (1) picks a neutral tool brand (avoid the game's trademark in the domain), an org name, and a bot name; (2) creates the machine account `tia-tools-bot` (separate email), then from it creates the organization `tia-tools` so the bot is the org's visible owner; adds the personal account as a second owner **with membership visibility set to Private** (Settings → Members → toggle each member's visibility); (3) creates repo `tia-tools/releases` with a README describing the tool and install steps (download zip → extract → run as administrator) — written in a neutral voice, linking nowhere personal; (4) generates a fine-grained PAT on `tia-tools-bot` scoped to `tia-tools/releases` with Contents: Read and write (that permission covers releases), stored locally (e.g. in the user's credential manager or an env var `GAKUMAS_DIST_TOKEN` — never committed); (5) registers `tia.run` with WHOIS privacy and adds it to a Cloudflare account (free plan suffices for one Worker route). Acceptance: `gh release list -R tia-tools/releases` succeeds (empty list) when authenticated with the bot PAT; the org's People tab shows only `tia-tools-bot` in a logged-out browser.

All names are final: org `tia-tools`, bot `tia-tools-bot` (created 2026-07-08), domain `tia.run` (registered 2026-07-08 at Cloudflare). No placeholders remain in this plan.

### M1 — Retarget the release pipeline

Edit `.claude/commands/release.md`: step 9 becomes `gh release create` with `-R tia-tools/releases` and `GH_TOKEN` taken from the bot PAT (explicitly NOT the personal `gh` login — spell out `GH_TOKEN=$GAKUMAS_DIST_TOKEN gh release create ...`, because ambient `gh auth` would author the release as the personal account and put "Taka499 released this" on the dist page, defeating ADR-0011). The tag still gets created in THIS repo (`git tag vX.Y.Z && git push origin vX.Y.Z`) so version history stays with the source; the dist-repo release is created with `--target` omitted (its default branch) since the dist repo has no meaningful commits. Add two steps: compute and upload a sidecar checksum asset (`Get-FileHash -Algorithm SHA256` → `gakumas-rehearsal-automation-vX.Y.Z.zip.sha256` containing just the lowercase hex digest) alongside the zip; and a privacy scrub check before upload — search the packaged tree and zip for identifying strings (the personal email and real name; expect zero hits; `Cargo.toml` has no `authors` field and release builds use `strip = true`, but panic-location paths like `C:\Work\GitRepos\...` remain — those contain no personal name and are acceptable). Publish the next release (whatever version is current then) to the dist repo as its first release. Acceptance: the dist repo's release page shows `tia-tools-bot` as the author, the zip + `.sha256` assets download, and this repo has the new tag but no new release.

### M2 — Cloudflare Worker (stateless manifest + download redirect)

A single Worker bound to `tia.run` with three routes. `GET /latest.json`: fetch `https://api.github.com/repos/tia-tools/releases/releases/latest` (mandatory `User-Agent` header — GitHub rejects UA-less requests) with edge caching (`cf: { cacheTtl: 300, cacheEverything: true }`) so the unauthenticated 60 req/hr/IP GitHub limit is never approached; reshape into the manifest: `version` = tag with leading `v` stripped, `notes` = release body's first paragraph, `url` = `https://tia.run/download/<zip asset name>`, `sha256` = the zip asset's `digest` field (GitHub now reports `"digest": "sha256:..."` per asset) with the `sha256:` prefix stripped, else the content of the `.sha256` sidecar asset from M1. `GET /download/<name>`: look up the asset by name in the same cached API response and 302 to its `browser_download_url` (the updater and browsers both follow redirects; the GitHub CDN URL flashes by network-level only, which is outside the casual-visitor threat model). `GET /` or `/latest`: 302 to the dist repo's releases page — this is the human-shareable link. If a bot PAT is ever needed for rate limits, store it as a Worker secret (bot's token, never personal). Keep the Worker source in this repo under `infra/worker/` (it contains nothing identifying) with a short README on how to deploy via `wrangler deploy`. Acceptance: `curl https://tia.run/latest.json` returns the manifest matching the M1 release; `curl -IL https://tia.run/download/<zip>` ends in HTTP 200 with the right content-length; `https://tia.run/latest` opens the dist repo releases page in a browser.

### M3 — Updater core in the app

New module `src/update/` (declare in `src/main.rs`). `src/update/endpoints.rs` holds the only two URLs in the codebase: `pub const MANIFEST_URL: &str = "https://tia.run/latest.json";` and `pub const FALLBACK_API_URL: &str = "https://api.github.com/repos/tia-tools/releases/releases/latest";`. `src/update/mod.rs` defines:

    pub struct UpdateInfo {
        pub version: String,       // "0.9.0", no leading v
        pub notes: String,
        pub url: String,           // zip download URL
        pub sha256: String,        // lowercase hex
    }

    pub fn check_for_update() -> Option<UpdateInfo>   // blocking; call from a spawned thread only
    pub fn is_newer(candidate: &str, current: &str) -> bool  // semver triple compare

`check_for_update` GETs `MANIFEST_URL` with a short timeout (5 s connect / 10 s total); on any failure (network down, non-200, parse error) it silently falls back to `FALLBACK_API_URL` (with a `User-Agent` header), parsing the GitHub release JSON into the same `UpdateInfo` (tag → version, first zip asset → url, its `digest` or the `.sha256` sidecar asset → sha256). If both fail, return `None` — an update check must never surface an error to the user or block anything. `is_newer` parses `X.Y.Z` into a `(u32,u32,u32)` triple (reject anything that doesn't parse as exactly three numeric parts) and compares lexicographically; compare against `env!("CARGO_PKG_VERSION")`. Everything here is pure-logic-testable: unit tests for `is_newer` (equal, patch/minor/major bumps, malformed strings) and for parsing both manifest JSON and a captured GitHub API JSON fixture, run with `GAKUMAS_NO_MANIFEST=1 cargo test` per the repo's testing convention. Add `sha2 = "0.10"` to `Cargo.toml` (needed by M4; add it here so the module compiles once).

### M4 — Install path and GUI surface

`src/update/install.rs`: `pub fn download_and_install(info: &UpdateInfo) -> anyhow::Result<()>`. Steps: download the zip to a `tempfile` in the exe's directory (same volume — required for atomic renames); compute its SHA-256 with `sha2` and compare to `info.sha256` case-insensitively, bailing with a clear error on mismatch; open with the `zip` crate and extract from under the archive's `gakumas-rehearsal-automation/` top-level folder: the exe is written to `gakumas-rehearsal-automation.exe.new` beside the current exe, `resources/` is replaced wholesale (extract to `resources.new`, rename old to `resources.old`, rename new into place, delete `resources.old`; on any rename failure, roll `resources.old` back), and `config.json` in the archive is SKIPPED entirely (also skip any other root file that already exists locally except the exe — conservative default so user data can never be clobbered). Then the rename-swap: rename the running `gakumas-rehearsal-automation.exe` → `.exe.old` (allowed for a running exe on Windows), rename `.exe.new` → `gakumas-rehearsal-automation.exe`. In app startup (early in `src/main.rs`), best-effort delete `gakumas-rehearsal-automation.exe.old` and `resources.old` if present.

GUI: add to `GuiState` an `update: UpdateUiState` (enum: `Idle`, `Available(UpdateInfo)`, `Downloading`, `ReadyToRestart`, `Failed(String)`), fed by a background thread spawned once at GUI startup that runs `check_for_update` and posts the result through the same channel/mutex pattern the OCR worker uses (plus `egui::Context::request_repaint` so the notice appears without user input). The header panel shows, when `Available`: "新しいバージョン v{X.Y.Z}" + an「アップデート」button — but the button only enabled while no automation run is active (mid-run self-replacement is pointless risk); clicking it runs `download_and_install` on a worker thread (`Downloading` shows a spinner). On success (`ReadyToRestart`), show「再起動」which spawns the new exe with `std::process::Command` (the child inherits the current process's admin elevation, so no extra UAC prompt) and exits the current process cleanly. On `Failed`, show the message in red; the app keeps working — failure must be inert. Follow the `PanelActions` dispatch pattern for the button (emit action → field → `handle_*`), and remember the repo's egui gotcha: clone status out of `GuiState` before mutating sibling fields (E0502).

Respect the repo design constraint of **no command-line arguments**: the restart is a plain re-spawn; `.old` cleanup keys off file existence, not flags.

### M5 — End-to-end acceptance

Temporarily set `Cargo.toml` `version = "0.0.1"` (uncommitted), build with `scripts/build.ps1`, launch, and observe: the header shows the notice for the real latest dist-repo release within seconds; clicking アップデート downloads and verifies (watch `logs/gakumas_screenshot.log` for the updater's log lines — add logging throughout M3/M4); 再起動 brings the app back reporting the real version; `config.json`'s modification time is unchanged; `gakumas-rehearsal-automation.exe.old` exists before restart and is gone after the next launch. Negative checks: with the network cable pulled (or Worker route disabled), launch shows no notice and no error; corrupt the manifest's sha256 via a temporary Worker tweak (or a local edit of `endpoints.rs` pointing at a doctored manifest served from a local file server) and confirm the install aborts with the red failure message and the running install stays intact. Revert the version change. Manual click-throughs here are unavoidable (live GUI + elevation), per the repo's testing limitations.

## Concrete Steps

Commands are indicative; each milestone's acceptance paragraph is authoritative.

    # M0 verification (from any shell, with the bot PAT)
    GH_TOKEN=$GAKUMAS_DIST_TOKEN gh release list -R tia-tools/releases

    # M1 publish (replaces the old step-9 of /release)
    GH_TOKEN=$GAKUMAS_DIST_TOKEN gh release create vX.Y.Z \
      gakumas-rehearsal-automation-vX.Y.Z.zip gakumas-rehearsal-automation-vX.Y.Z.zip.sha256 \
      -R tia-tools/releases --title "vX.Y.Z - <theme>" --notes-file <notes>

    # M2 verification
    curl -s https://tia.run/latest.json
    curl -sIL https://tia.run/download/gakumas-rehearsal-automation-vX.Y.Z.zip | tail -5

    # M3 tests
    GAKUMAS_NO_MANIFEST=1 cargo test update::

    # M4/M5 build
    powershell -ExecutionPolicy Bypass -File scripts/build.ps1

## Validation and Acceptance

The plan is accepted when the M5 scenario passes end-to-end on the live channel: an old-versioned local build self-updates to the current dist-repo release with one click, preserving `config.json`, restarting into the new version, and cleaning up `.old` files on the next launch; update-check failure is silent; checksum failure is loud but harmless. Additionally, a logged-out browser visiting `https://tia.run/latest`, the dist repo, and the release page encounters no reference to the personal account.

## Idempotence and Recovery

All updater file operations are staged (`*.new` / `*.old`) so a crash mid-install leaves either the old or the new set runnable; re-running the updater from either state converges (stale `.new`/`.old` files are overwritten/cleaned). Re-publishing a release re-runs `gh release create` with a bumped version rather than overwriting. The Worker is stateless, so redeploying it is always safe. If the domain is ever lost, shipped binaries degrade to the GitHub fallback automatically — that is by design.

## Interfaces and Dependencies

New dependency: `sha2 = "0.10"`. Reused: `reqwest` 0.12 (blocking), `zip` 2.2, `tempfile`, `serde_json`, `anyhow`. New module `src/update/` exposing `UpdateInfo`, `check_for_update`, `is_newer`, `download_and_install`, with URLs confined to `src/update/endpoints.rs`. GUI touch points: `GuiState` (new `update` field), header panel rendering, `PanelActions` (new action fields), `src/gui/mod.rs::update()` dispatch. Infra artifacts in `infra/worker/` (Worker script + deploy README). Release-process artifact: revised `.claude/commands/release.md`.
