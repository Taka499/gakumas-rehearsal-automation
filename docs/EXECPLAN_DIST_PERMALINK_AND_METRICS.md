# Permanent download link + anonymous usage metrics on the tia.run distribution Worker

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

This document must be maintained in accordance with `docs/PLANS.md` (repository root: `C:\Work\GitRepos\gakumas-rehearsal-automation`).

Governing decisions: `docs/adr/0011-identity-separated-distribution-channel.md` (what the distribution channel is) and `docs/adr/0012-metrics-are-anonymous-by-design.md` (what metrics may and may not record). This plan implements both asks from the 2026-07-08 /grill-me session: a version-less permanent download URL, and privacy-preserving usage metrics with permanent history and a local reader script.

## Purpose / Big Picture

Today, sharing the app means sharing a URL that embeds a version number (`https://tia.run/download/gakumas-rehearsal-automation-v0.9.0.zip`), which goes stale at every release. After this change, `https://rehearsal-automation.tia.run/download` is a permanent URL that always downloads the newest release's zip — shareable once, correct forever. The subdomain is deliberate groundwork: the user plans multiple tools under `tia.run`, so each tool gets its own single-level subdomain namespace, and the bare root is freed entirely for a future brand landing page. The Worker moves OFF the bare `tia.run` — no compatibility alias — because v0.9.0 was never distributed to anyone: the only install with the old root URL baked in is the developer's own, and the in-app updater degrades gracefully to its GitHub API fallback when the manifest URL is unreachable, so even that copy keeps updating.

Also today, the developer has zero visibility into usage: no idea how many people download the tool, how many actively use it, or what versions they run. After this change, the Worker records anonymous events for every update check and download, a nightly job preserves daily aggregates forever, and running one local script prints: update checks per day, approximate unique users per day, downloads per day (split app-vs-browser), version spread, and country split. No persistent identifiers are ever collected (per ADR 0012).

## Progress

- [x] (2026-07-08) M1: version-less `/download` route + move the Worker from `tia.run` to `rehearsal-automation.tia.run` (root freed, no alias) + updater endpoint flip in `src/update/endpoints.rs`. Deployed (Worker version dda02ada) and verified live: subdomain `/download` 302s to the v0.9.0 zip, `/latest.json` serves the manifest with subdomain `url`, named-asset 404 unchanged, `cargo check` clean, and bare `tia.run` no longer resolves (DNS record removed on detach). See Artifacts.
- [x] (2026-07-08) M2: Analytics Engine event recording, deployed and live-verified end-to-end: check + download events landed in `dist_metrics` with correct schema — event type, asset name (blob5), country (JP), daily bucket, and client version (`0.9.0` from a simulated app User-Agent). User enabled Analytics Engine in the dashboard (dataset `dist_metrics`, binding `METRICS`). (NOTE: the `CF_ANALYTICS_TOKEN` secret, recorded here as set, turned out NOT to be — see the 2026-07-09 Surprises entry; it was set for real on 2026-07-09.)
- [x] (2026-07-09) M3: nightly cron rollup into Workers KV — PROVEN working with production bindings. KV namespace `HISTORY` (id `6bd8c0bf0edc4c22b55624d7eb31c0b0`); cron `30 2 * * *` attached after registering the account's workers.dev subdomain (see Surprises). Root-caused and fixed a silent failure (missing `CF_ANALYTICS_TOKEN` secret → cron query 401'd → wrote nothing); after the fix, a production run wrote the 7-day backfill (`daily/2026-07-02`..`daily/2026-07-08`) and `dist_stats.py` reads the `2026-07-08` row (9 checks / 3 uniq / 13 web dl / versions 0.9.1×2, 0.9.0×1). The scheduler itself is confirmed live (it fired at 02:30 UTC 2026-07-09 — silently, pre-fix); tonight's automatic run is now expected to populate `daily/2026-07-09` unattended. AE returns numeric aggregates as JSON strings; both consumers coerce.
- [x] (2026-07-08) M4: local stats script `scripts/dist_stats.py` — runs clean; AE + GitHub sections verified with live data (KV section correctly reports "no rollup keys yet" until the first cron). See Artifacts.
- [ ] M5: disclosure note + docs sync + close-out. (completed: dist repo README updated via bot PAT — permanent link front-and-center + JA/EN anonymous-metrics privacy section, commit `f6fa456` author verified `tia-tools-bot`; `infra/worker/README.md` metrics/secrets/prereq docs. Remaining: after M3's overnight confirmation — CLAUDE.md snapshot update, retrospective, close-out ritual.)

## Surprises & Discoveries

- Observation: Analytics Engine must be enabled once, manually, in the Cloudflare dashboard before any Worker with an `analytics_engine_datasets` binding can deploy; wrangler cannot do it.
  Evidence: `npx wrangler deploy` failed with `[ERROR] ... You need to enable Analytics Engine. Head to the Cloudflare Dashboard to enable: https://dash.cloudflare.com/<account>/workers/analytics-engine [code: 10089]` (2026-07-08, first M2 deploy attempt).
- Observation: the sandboxed agent shell cannot run `curl` (permission-denied), so live HTTP verification goes through the harness WebFetch tool instead; `npx wrangler` commands work fine.
  Evidence: M1 verification transcript in Artifacts.
- Observation: attaching a cron trigger requires the account to have a workers.dev subdomain, even though this Worker never uses one (custom-domain-only). Error code 10063 on the `/schedules` API. Fixed by the user opening the Workers dashboard landing page, which auto-registered `tia-tools-dev.workers.dev` — a neutral name, so no identity concern.
  Evidence: `wrangler triggers deploy` 403 with `{"code":10063,"message":"You need a workers.dev subdomain..."}`; succeeded immediately after registration (`schedule: 30 2 * * *`).
- Observation: `wrangler dev --test-scheduled --remote` CANNOT verify the rollup: dev preview sessions do not receive production secrets, so `env.CF_ANALYTICS_TOKEN` is undefined and `aggregateDay` returns null by design (its `!res.ok` guard). The scheduled handler must be proven by the real production cron.
  Evidence: forced `__scheduled` run returned "Ran scheduled event" but `wrangler kv key list` stayed `[]`.
- Observation: the AE SQL API returns numeric aggregates as JSON *strings* (`"n": "1"`), not numbers.
  Evidence: ad-hoc query transcript in Artifacts; both `aggregateDay` (`Number(r.n)`) and `dist_stats.py` (`float(r["n"])`) coerce.
- Observation (2026-07-09, root cause of the empty rollup): the Worker's `CF_ANALYTICS_TOKEN` secret was never actually set on the deployed Worker (this plan wrongly recorded it as set in M2). The cron fired on schedule at 02:30 UTC but `aggregateDay`'s fetch went out as `Bearer undefined`, got 401, hit the `if (!res.ok) return null` guard, and wrote nothing — a completely silent failure (the whole metrics path is best-effort/swallow-errors by design, which hid it). Fixed by `printf '%s' "$TOKEN" | wrangler secret put CF_ANALYTICS_TOKEN` with the known-good `.env` token (wrangler said "Creating", confirming it had been absent).
  Evidence: a temporary authenticated `/__rollup` diagnostic route (added, deployed, then removed) returned 404 while the header carried the working `.env` token — proving `env.CF_ANALYTICS_TOKEN !== ` that token; after re-setting the secret the same route returned `rollup ok; keys: ["daily/2026-07-02"..."daily/2026-07-08"]` (7-day backfill), and `dist_stats.py` then showed the `2026-07-08` row (9 checks / 3 uniq / 13 web dl / 0.9.1×2, 0.9.0×1).
  Lesson: a best-effort telemetry path that swallows all errors needs an explicit "did the last rollup run?" signal, or a missing secret looks identical to "no traffic". The silent-swallow design (right for the request path) hid a real misconfig on the cron path for a day.

## Decision Log

- Decision: the permanent link is a direct file download (`GET /download` with no asset name → 302 to the latest zip), not a landing page.
  Rationale: one click for the receiver; `/latest` already exists for humans who want the release-notes page. Bytes still come from GitHub, so GitHub's per-asset `download_count` keeps accumulating for free.
  Date/Author: 2026-07-08, /grill-me session (user decision).
- Decision: the canonical hostname is the per-app subdomain `rehearsal-automation.tia.run`; the bare `tia.run` serves identical routes forever as a compatibility alias.
  Rationale: the user has a concrete plan for multiple tools under `tia.run`, so each tool namespaces under its own single-level subdomain and the root stays free for a future brand landing page. The root cannot ever be dropped: shipped binaries (v0.9.0+) have `https://tia.run/latest.json` baked in (the hard-to-reverse property ADR 0011 already documents). Rejected: `dl.rehearsal-automation.tia.run` — a second-level subdomain is NOT covered by Cloudflare's free Universal SSL certificate (`*.tia.run` covers one level only); HTTPS there fails without Advanced Certificate Manager (~$10/month). Rejected: a shorter slug (e.g. `ra.tia.run`) — user chose the full app name.
  Date/Author: 2026-07-08, /grill-me session (user decision).
- Decision: the in-app updater's primary manifest URL flips to `https://rehearsal-automation.tia.run/latest.json` (one-line change in `src/update/endpoints.rs`, effective from the next release).
  Rationale: future binaries should check the app's own namespace so the root can eventually be repurposed at brand level. This is the plan's only Rust change.
  Date/Author: 2026-07-08, /grill-me session (agent decision following from the subdomain choice).
- Decision: NO root alias — the Worker moves off the bare `tia.run` entirely; only `rehearsal-automation.tia.run` is served. This supersedes the "root serves identical routes forever" clause of the subdomain decision above.
  Rationale: the permanence argument rested on shipped binaries, but v0.9.0 was never shared with anyone — the sole install carrying the baked-in `https://tia.run/latest.json` is the developer's own, and `src/update/`'s designed degradation (manifest unreachable → GitHub API fallback at `FALLBACK_API_URL`) keeps even that copy updating. Dropping the alias now, before any URL is public, costs nothing and frees the root cleanly; keeping it would be paying compatibility cost for zero users. Corollary: the `DOMAIN` constant in `infra/worker/worker.js` (used to build the manifest's download URL) must change to the subdomain, not just the route config.
  Date/Author: 2026-07-08, /grill-me session (user decision: "discard tia.run download path for v0.9.0 — not yet shared to anyone").
- Decision: unique-user estimation uses a daily-rotating salted IP hash computed in the Worker; no persistent identifiers (no install ID), per `docs/adr/0012`.
  Rationale: middle ground between "event counts only" (no uniques at all) and "anonymous install UUID" (a de-facto tracking ID). Hash = truncated SHA-256(IP + UTC date + secret salt); the date input makes it useless across days, the secret salt makes it irreversible. Raw IP is never stored.
  Date/Author: 2026-07-08, /grill-me session (user decision, recorded as ADR 0012).
- Decision: record country (`request.cf.country`, e.g. "JP") as a dimension.
  Rationale: coarse (~200 buckets), standard practice, and genuinely useful for a JP-game tool (is the audience JP-only?). Not personally identifying on its own.
  Date/Author: 2026-07-08, /grill-me session (user decision).
- Decision: all infra (Worker code including metrics) stays in this public repo under `infra/worker/`; no separate private repo.
  Rationale: ADR 0011 explicitly accepted that this repo is traceable ("separation from casual downloaders only"). Public collection code is a privacy asset — anyone can audit exactly what is collected. Secrets (salt, API tokens) live in `wrangler secret` / gitignored `.env`, never in git; metrics *data* lives only in Cloudflare (Analytics Engine + KV), never in a repo.
  Date/Author: 2026-07-08, /grill-me session (user decision).
- Decision: metrics are read via a local script (`scripts/dist_stats.py`), not a `/stats` endpoint on the Worker.
  Rationale: zero new public surface, no URL-borne secret, fastest iteration on queries while discovering which numbers matter. A token-gated `/stats` can be added later without rework because the event schema and SQL are identical.
  Date/Author: 2026-07-08, /grill-me session (user decision).
- Decision: long-term history is preserved by a nightly cron on the Worker that rolls up per-day aggregates into Workers KV.
  Rationale: Analytics Engine deletes raw events after ~90 days (hard platform limit). The cron+KV rollup is $0 on the Workers free plan (1 small KV write/day vs a 1,000/day limit; cron triggers included free) and survives months of the developer not touching the project, unlike script-side snapshotting. Idempotent: each day's aggregate is written under a date key, so re-runs overwrite harmlessly.
  Date/Author: 2026-07-08, /grill-me session (user decision, after cost check).
- Decision: the dist repo's privacy disclosure describes the collection but does NOT link `infra/worker/worker.js` as the auditable source (correcting this plan's own M5 wording).
  Rationale: linking the personal repo from the download page would put the personal identity one click away — exactly what ADR 0011's bot-authored, no-code dist repo exists to prevent. Anyone who independently finds the public repo can still audit the code; the dist repo just doesn't hand out the pointer.
  Date/Author: 2026-07-08, caught during M5 implementation.
- Decision: update checks that fall back to the GitHub API (when tia.run is unreachable) go uncounted.
  Rationale: accepted limitation; the fallback exists for resilience (per `src/update/endpoints.rs`) and instrumenting GitHub's API is impossible. Expected to be a negligible fraction.
  Date/Author: 2026-07-08, /grill-me session.

## Outcomes & Retrospective

- (to be written at completion)

## Context and Orientation

This repository is a Windows Rust application, but this plan touches almost none of the Rust code. The work lives in `infra/worker/`, which contains the source of a **Cloudflare Worker** — a small JavaScript program that Cloudflare runs at its edge servers whenever someone requests `https://tia.run`. The Worker is the public face of the app's distribution channel (per `docs/adr/0011`): release zips are hosted as GitHub Releases on the neutral-org repo `tia-tools/releases`, and the Worker translates friendly URLs into redirects to those assets.

Files that exist today:

- `infra/worker/worker.js` — the Worker. Routes: `GET /latest.json` (update manifest consumed by the in-app updater), `GET /download/<asset-name>` (302 to that GitHub release asset), `GET /` and `/latest` (302 to the dist repo's latest-release page). It is stateless: everything derives from the GitHub Releases API, edge-cached for `CACHE_TTL = 300` seconds.
- `infra/worker/wrangler.toml` — deployment config for `wrangler`, Cloudflare's CLI. Worker name `tia-run-dist`, custom domain `tia.run` today. This plan REPLACES that custom domain with `rehearsal-automation.tia.run` (Cloudflare auto-creates the DNS record on deploy and detaches the old one; single-level subdomains are covered by the zone's free Universal SSL certificate — second-level ones like `dl.x.tia.run` are NOT, which is why that variant was rejected). The bare root then serves nothing, by design, freed for a future brand landing page.
- `infra/worker/README.md` — route documentation and deploy instructions.
- `src/update/mod.rs` — the in-app updater. Relevant fact: its HTTP client sends `User-Agent: gakumas-rehearsal-automation/<version>` (built from `CARGO_PKG_VERSION` at `src/update/mod.rs:83`), and it fetches the manifest once per app launch from `MANIFEST_URL` in `src/update/endpoints.rs`. The metrics side needs NO Rust changes (the version dimension comes from parsing that existing User-Agent in the Worker); the plan's ONLY Rust change is flipping `MANIFEST_URL` to the new subdomain. The developer's own v0.9.0 install still checks the bare root; once the root is detached that check fails and the updater silently uses `FALLBACK_API_URL` (the dist repo's GitHub API) — this degradation is existing, tested behavior, and v0.9.0 was never distributed to anyone else.
- Repo-root `.env` (gitignored) — already holds `GAKUMAS_DIST_TOKEN` (the release bot's PAT). New Cloudflare credentials for the stats script go here too.

Terms used below, in plain language:

- **Workers Analytics Engine (AE)**: a free Cloudflare feature; a write-only event log bound to a Worker. The Worker calls `env.METRICS.writeDataPoint({blobs, doubles, indexes})` to append one row; each row has up to 20 text fields ("blobs"), numeric fields ("doubles"), and one "index" used for sampling. Rows are kept ~90 days. The ONLY way to read them is an SQL query POSTed over HTTPS to `https://api.cloudflare.com/client/v4/accounts/<ACCOUNT_ID>/analytics_engine/sql` with a bearer API token. There is no dashboard UI. One critical read-side rule: because AE may sample under load, accurate counts use `sum(_sample_interval)` instead of `count()`.
- **Workers KV**: a free key-value store bound to a Worker (and also readable/writable via REST API). We use it as the permanent archive: one small JSON value per day.
- **Cron Trigger**: a schedule in `wrangler.toml` that makes Cloudflare invoke the Worker's exported `scheduled()` handler at fixed times, independent of any HTTP request. Free plan includes them.
- **wrangler secret**: an encrypted value stored by Cloudflare and exposed to the Worker as `env.<NAME>`. Set with `wrangler secret put <NAME>` from `infra/worker/`; never appears in git.

The event schema (fixed by this plan; the rollup and the script both depend on the positions):

    blobs[0] ("blob1" in SQL): event type — "check" or "download"
    blobs[1] ("blob2"): client app version parsed from User-Agent
                        ("0.9.0"), or "" when the UA is not the app
                        (i.e. a browser download)
    blobs[2] ("blob3"): country code from request.cf.country, or ""
    blobs[3] ("blob4"): daily anonymous bucket — first 16 hex chars of
                        SHA-256("<ip>|<YYYY-MM-DD UTC>|<salt>")
    blobs[4] ("blob5"): for "download" events, the asset file name
                        (which encodes the downloaded version); "" for checks
    doubles[0]: always 1
    indexes[0]: the daily bucket (blob4), so any sampling groups by user

Privacy invariants (from `docs/adr/0012`, restated so this plan is self-contained): no persistent client identifier may be introduced; raw IPs are never stored anywhere (the IP exists only transiently inside the hash computation); the salt (`HASH_SALT`) exists only as a wrangler secret.

## Plan of Work

Milestone 1 adds the permanent link and moves the Worker to the subdomain. In `infra/worker/worker.js`, change the route dispatch so `GET /download` and `GET /download/` (no asset name) call `download(env, null)`, and change `download()` so a null/empty name selects the first asset whose name ends in `.zip` from the latest release (the same selection rule `latestJson()` already uses) and 302-redirects to its `browser_download_url`. Also change the `DOMAIN` constant from `"tia.run"` to `"rehearsal-automation.tia.run"` — it feeds both the manifest's `url` field and the Worker's outbound User-Agent. In `infra/worker/wrangler.toml`, REPLACE the `routes` entry: `{ pattern = "rehearsal-automation.tia.run", custom_domain = true }` (deleting the `tia.run` pattern detaches the root; deploy handles both changes atomically). In `src/update/endpoints.rs`, change `MANIFEST_URL` to `https://rehearsal-automation.tia.run/latest.json` (the GitHub fallback URL is unchanged) and update the file's doc comment; run a build check (`cargo check`). Update `infra/worker/README.md`'s route list and hostname. Deploy and verify with curl that the subdomain serves everything and the root no longer does.

Milestone 2 adds event recording. In `infra/worker/wrangler.toml`, add an Analytics Engine binding (`[[analytics_engine_datasets]]`, binding `METRICS`, dataset `dist_metrics`). In `worker.js`, add a `recordEvent(env, request, type, extra)` helper that: reads the client IP from the `CF-Connecting-IP` request header; computes the daily bucket with the Web Crypto API (`crypto.subtle.digest("SHA-256", ...)` over `ip + "|" + utcDate + "|" + env.HASH_SALT`, hex-encode, take 16 chars); parses the app version from the `User-Agent` header when it matches `gakumas-rehearsal-automation/<semver>`; reads `request.cf?.country`; and calls `env.METRICS.writeDataPoint(...)` with the schema above. `writeDataPoint` is synchronous fire-and-forget (returns void, adds no latency), and the whole helper must be wrapped so a failure (e.g. missing binding in local dev) can never break the user-facing redirect — try/catch and drop. Call it from the `/latest.json` handler (type "check") and both `/download` variants (type "download", extra = asset name). Generate a salt (`openssl rand -hex 32` in Git Bash, or PowerShell equivalent) and store it with `wrangler secret put HASH_SALT`. The `fetch` handler signature gains the `ctx` parameter (`async fetch(request, env, ctx)`) for consistency, though `writeDataPoint` does not need `ctx.waitUntil`.

Milestone 3 adds the permanent-history rollup. Create a KV namespace (`wrangler kv namespace create HISTORY` from `infra/worker/`; paste the returned id into `wrangler.toml` under `[[kv_namespaces]]`, binding `HISTORY`). Add `[triggers] crons = ["30 2 * * *"]` (02:30 UTC ≈ 11:30 JST, well after any Japanese-evening usage day rolls over in UTC). Export a `scheduled(event, env, ctx)` handler in `worker.js` that computes yesterday's UTC date, POSTs 3–4 SQL queries to the AE SQL API (endpoint above; auth `Bearer env.CF_ANALYTICS_TOKEN`; account id from a plain `ACCOUNT_ID` var in `wrangler.toml` — it is not a secret), aggregates into one JSON object, and writes it to KV key `daily/<YYYY-MM-DD>`:

    {
      "date": "2026-07-08",
      "checks": 41,
      "unique_users": 12,
      "downloads_app": 1,
      "downloads_browser": 3,
      "versions": {"0.9.0": 10, "0.8.2": 2},   // unique users per client version
      "countries": {"JP": 11, "US": 1}          // unique users per country
    }

The queries (dataset `dist_metrics`; remember `sum(_sample_interval)` for counts). Uniques: prefer `count(DISTINCT blob4)` if the AE SQL dialect accepts it; if it does not (verify at implementation time — see Surprises section if so), fall back to `SELECT blob4 FROM dist_metrics WHERE ... GROUP BY blob4` and count rows in JS. Representative queries:

    -- checks + uniques for one day
    SELECT sum(_sample_interval) AS checks, count(DISTINCT blob4) AS uniq
    FROM dist_metrics
    WHERE blob1 = 'check' AND timestamp >= toDateTime('<day> 00:00:00')
      AND timestamp < toDateTime('<day+1> 00:00:00')

    -- version spread (unique users per version)
    SELECT blob2 AS version, count(DISTINCT blob4) AS uniq FROM dist_metrics
    WHERE blob1 = 'check' AND blob2 != '' AND <same day window>
    GROUP BY blob2

    -- downloads split
    SELECT if(blob2 = '', 'browser', 'app') AS kind,
           sum(_sample_interval) AS n FROM dist_metrics
    WHERE blob1 = 'download' AND <same day window> GROUP BY kind

The rollup is idempotent (same key per day; re-running overwrites the same value) and self-healing: after writing yesterday, check whether the KV keys for the 6 days before it exist, and backfill any missing ones that still fall inside AE's ~90-day retention (this covers cron outages up to a week without any manual action). Create a Cloudflare API token (dashboard → My Profile → API Tokens → Create) with permissions **Account Analytics: Read** and **Workers KV Storage: Read** (the KV read scope is for the M4 script; the Worker itself reads/writes KV through its binding, not the REST API), scoped to the account; store it BOTH as `wrangler secret put CF_ANALYTICS_TOKEN` (for the Worker's rollup) and in the repo-root `.env` as `CF_ANALYTICS_TOKEN=` (for the script), alongside `CF_ACCOUNT_ID=` and `CF_KV_NAMESPACE_ID=`.

Milestone 4 adds the reader: `scripts/dist_stats.py` (Python, matching the `scripts/region_tuner.py` precedent; stdlib `urllib` only, no pip installs). It loads `.env` from the repo root (simple KEY=VALUE parser, same file the release script uses), then prints three sections: (1) permanent history — list KV keys with prefix `daily/` via `GET https://api.cloudflare.com/client/v4/accounts/<id>/storage/kv/namespaces/<ns>/keys?prefix=daily/`, fetch each value (or the last N days via `--days`, default 30), and render a per-day table of checks / uniques / downloads; (2) recent detail — query the AE SQL API directly for the last 7 days' version spread and country split (live data, finer than the rollup); (3) all-time downloads per release — `GET https://api.github.com/repos/tia-tools/releases/releases` (unauthenticated is fine at this rate) summing `assets[].download_count` per tag. Failures of any one section print a warning and continue (e.g. GitHub rate-limited must not hide the KV history).

Milestone 5 is disclosure and sync: add a short 定期メンテナンス-style note to the dist repo's release page template or `tia-tools/releases` README (one paragraph, Japanese + English: anonymous usage statistics are collected — daily counts, version, country; no persistent identifiers, no raw IPs; link to `infra/worker/worker.js` as the auditable source). Update `infra/worker/README.md` (new routes, bindings, secrets inventory, cron). Update `CLAUDE.md`'s Active ExecPlans entry and `src/update/` note if wording changed. Then run the close-out ritual per `docs/PLANS.md`.

## Concrete Steps

All Worker commands run from `infra/worker/` (wrangler is invoked via `npx wrangler` if not globally installed; it authenticates via `wrangler login` browser flow, one-time). All script commands run from the repo root.

M1 deploy and verify:

    cd infra/worker
    npx wrangler deploy    # provisions the subdomain custom domain + DNS record, detaches tia.run
    curl -sI https://rehearsal-automation.tia.run/download | head -5
    # expect: HTTP/2 302, location: https://github.com/tia-tools/releases/releases/download/v0.9.0/<zip name>
    curl -sI https://rehearsal-automation.tia.run/latest.json | head -3
    # expect: HTTP/2 200; body's "url" field points at rehearsal-automation.tia.run/download/...
    curl -sI "https://rehearsal-automation.tia.run/download/does-not-exist.zip"
    # expect: HTTP/2 404  (named-asset behavior unchanged)
    curl -sI --max-time 10 https://tia.run/latest.json
    # expect: NOT a 200 manifest — connection error or Cloudflare zone fallback (root intentionally detached)
    cargo check 2>&1 | grep "^error"    # from repo root, after the endpoints.rs flip; expect no output

M2 salt + deploy + verify write:

    cd infra/worker
    openssl rand -hex 32          # copy output
    npx wrangler secret put HASH_SALT   # paste
    npx wrangler deploy
    curl -s https://tia.run/latest.json > /dev/null   # generate one event
    # verification of the write happens via the M3/M4 read path; for an
    # immediate check, `npx wrangler tail` while curling shows the request
    # without errors (writeDataPoint failures would log).

M3 KV + cron:

    cd infra/worker
    npx wrangler kv namespace create HISTORY   # paste id into wrangler.toml
    npx wrangler secret put CF_ANALYTICS_TOKEN
    npx wrangler deploy
    # force one scheduled run locally against production bindings:
    npx wrangler dev --test-scheduled --remote
    curl "http://localhost:8787/__scheduled?cron=30+2+*+*+*"
    # then confirm the key exists:
    npx wrangler kv key list --binding HISTORY --remote --prefix daily/

M4 script:

    python scripts/dist_stats.py --days 30
    # expected shape (values will differ):
    #   == Daily (KV history) ==
    #   date        checks  uniq  dl(app)  dl(web)
    #   2026-07-08      41    12        1        3
    #   == Version spread, last 7d (AE) ==
    #   0.9.0  10
    #   == All-time downloads per release (GitHub) ==
    #   v0.9.0  27

## Validation and Acceptance

Acceptance is behavioral, end-to-end:

1. Opening `https://rehearsal-automation.tia.run/download` in a browser starts downloading the newest release zip, with no version number anywhere in the typed URL and a valid HTTPS certificate. The bare root (`https://tia.run/...`) intentionally no longer serves the app routes. The developer's own v0.9.0 install (the only binary carrying the old root URL) still receives update notices via the updater's GitHub API fallback — verifiable by launching the app and seeing the update notice for the next release when one exists. After a future release is published via the existing `/release` skill, the SAME subdomain URLs serve the new zip (within `CACHE_TTL` = 5 minutes) with no Worker change.
2. Launching the app (which checks `/latest.json` once at startup) and then running `python scripts/dist_stats.py` the NEXT day shows that day's row with `checks >= 1`, `uniq >= 1`, and the app's version in the version spread. (Same-day data is visible via the script's AE section immediately, within ~1 minute of the event.)
3. Downloading via the permanent link from a browser increments `dl(web)`; the in-app updater performing an install increments `dl(app)`.
4. The KV key `daily/<yesterday>` exists every morning without human action (cron), and deleting a middle day's key then waiting for the next cron restores it (backfill), provided the day is within AE retention.
5. Privacy check: `grep -ri "CF-Connecting-IP" infra/worker/worker.js` shows the header read ONLY inside the hash helper; no `writeDataPoint` argument contains the raw IP; `git grep HASH_SALT` finds only `env.HASH_SALT` references and docs, never a value.

## Idempotence and Recovery

Worker deploys are idempotent (`wrangler deploy` replaces the whole script). The rollup writes fixed per-day keys, so repeated or forced runs cannot duplicate history, and its 7-day backfill self-heals cron gaps. If the AE SQL dialect rejects `count(DISTINCT ...)`, switch to the GROUP BY fallback described in M3 — record it in Surprises & Discoveries. If `CF_ANALYTICS_TOKEN` expires or is revoked, the cron silently stops writing keys; the script's history section will show missing days — recreate the token, `wrangler secret put` it again, and the next cron backfills up to 7 days (older gaps need a one-off manual run of the scheduled handler per missing day, adapting the M3 `__scheduled` invocation). Losing `HASH_SALT` (e.g. accidental overwrite) is harmless to history — aggregates in KV are already identifier-free — but breaks intra-day unique continuity for that one day only.

## Artifacts and Notes

M1 live verification, 2026-07-08 (fetched via harness WebFetch because sandbox curl was permission-blocked; `wrangler deploy` output confirmed `rehearsal-automation.tia.run (custom domain)` attached, version dda02ada):

    GET https://rehearsal-automation.tia.run/latest.json
    -> 200, {"version":"0.9.0",
             "url":"https://rehearsal-automation.tia.run/download/gakumas-rehearsal-automation-v0.9.0.zip",
             "sha256":"a82169974c953796f3c23929518665176363f01ef60003023b997f3f1efc95e5", ...}

    GET https://rehearsal-automation.tia.run/download
    -> 302, location: https://github.com/tia-tools/releases/releases/download/v0.9.0/gakumas-rehearsal-automation-v0.9.0.zip

    GET https://rehearsal-automation.tia.run/download/does-not-exist.zip
    -> 404 (named-asset behavior unchanged)

    GET https://tia.run/latest.json
    -> getaddrinfo ENOTFOUND tia.run (root custom domain detached; Cloudflare removed the DNS record)

    cargo check 2>&1 | grep "^error"   -> no output (endpoints.rs flip compiles)

M2/M4 live verification, 2026-07-08 (after user enabled AE + created the API token):

    GET /latest.json (browser UA) + GET /download (browser UA)
    GET /latest.json with User-Agent: gakumas-rehearsal-automation/0.9.0
    -> AE SQL (grouped by event, ver, asset) returns, ~1 min later:
       {"event":"download","ver":"","asset":"gakumas-rehearsal-automation-v0.9.0.zip","n":"1"}
       {"event":"check","ver":"","asset":"","n":"1"}
       {"event":"check","ver":"0.9.0",...,"n":"1"}     <- UA version parse works

    python scripts/dist_stats.py --days 7
    -> == Daily history, last 7 days (KV rollup) ==
         (no rollup keys yet - the nightly cron hasn't produced data)
       == Recent detail, last 7 days (Analytics Engine, live) ==
         update checks: 1   unique users: 1
         country JP         1 user(s)
       == All-time downloads per release (GitHub tia-tools/releases) ==
         v0.9.0           2

## Interfaces and Dependencies

No new crates; the only Rust edit is the `MANIFEST_URL` constant in `src/update/endpoints.rs` (new value `https://rehearsal-automation.tia.run/latest.json`; `FALLBACK_API_URL` unchanged). No new npm dependencies (the Worker stays dependency-free; hashing uses the built-in Web Crypto API). `scripts/dist_stats.py` uses only the Python standard library. In `infra/worker/worker.js`, the following must exist at the end:

    export default {
      async fetch(request, env, ctx)      // existing routes + versionless /download + recordEvent calls
      async scheduled(event, env, ctx)    // nightly rollup: AE SQL -> KV daily/<date>
    }

    async function dailyBucket(ip, utcDate, salt)   // -> 16-hex-char string
    function parseAppVersion(userAgent)             // -> "0.9.0" | ""
    async function recordEvent(env, request, type, extra)  // never throws

Bindings in `infra/worker/wrangler.toml`: `METRICS` (Analytics Engine dataset `dist_metrics`), `HISTORY` (KV), `ACCOUNT_ID` (plain var), plus secrets `HASH_SALT` and `CF_ANALYTICS_TOKEN`; `routes` carries exactly ONE custom domain, `rehearsal-automation.tia.run` (the `tia.run` pattern is deleted — root intentionally detached). Repo-root `.env` gains `CF_ANALYTICS_TOKEN`, `CF_ACCOUNT_ID`, `CF_KV_NAMESPACE_ID`.

Note appended 2026-07-08 (same /grill-me session, before implementation started): reworked M1 to add the `rehearsal-automation.tia.run` subdomain as the canonical hostname with the bare root as a permanent alias, and to flip `src/update/endpoints.rs::MANIFEST_URL` to it — the user confirmed multiple tools under `tia.run` are a concrete plan, so per-app subdomains are the namespace scheme. See the Decision Log entries dated 2026-07-08 for the rejected `dl.` second-level variant (free Universal SSL covers one wildcard level only) and the rationale.

Note appended 2026-07-08 (later the same session): the root alias was then discarded entirely — the user pointed out v0.9.0 was never shared with anyone, so no third-party binary carries the baked-in root URLs and the permanence argument was moot. The Worker now MOVES from `tia.run` to the subdomain instead of serving both; the developer's own install rides the updater's GitHub API fallback. All sections (Purpose, Progress, Context, Plan of Work M1, Concrete Steps, Validation, Interfaces) were updated accordingly; the superseding Decision Log entry records the rationale.
