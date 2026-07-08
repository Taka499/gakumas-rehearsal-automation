# rehearsal-automation.tia.run distribution Worker

Cloudflare Worker fronting the dist repo `tia-tools/releases` (per `docs/adr/0011`). Stateless — it reshapes the GitHub Releases API into an update manifest and download redirects, edge-cached 5 minutes. Publishing a release is the only state change; nothing here ever needs a data update.

Hostname policy: each tia-tools app lives on its own single-level subdomain; the bare `tia.run` is intentionally unbound (reserved for a future brand landing page). Second-level subdomains (e.g. `dl.rehearsal-automation.tia.run`) must not be added — the zone's free Universal SSL certificate covers one wildcard level only, so HTTPS there would fail.

| Route | Serves |
|---|---|
| `https://rehearsal-automation.tia.run/latest.json` | update manifest: `{version, notes, url, sha256}` |
| `https://rehearsal-automation.tia.run/download` | 302 to the latest release's zip (permanent share link) |
| `https://rehearsal-automation.tia.run/download/<asset>` | 302 to that GitHub release asset |
| `https://rehearsal-automation.tia.run/` and `/latest` | 302 to the latest-release page (release notes for humans) |

## Deploy

Requires Node.js. Run from this directory (`infra/worker/`):

    npx wrangler login      # MUST be the PROJECT Cloudflare account (the one holding tia.run), not a personal one
    npx wrangler deploy

The `custom_domain` route in `wrangler.toml` makes Cloudflare attach the Worker to `rehearsal-automation.tia.run` and manage DNS automatically — no manual DNS records.

## Verify

    curl -s https://rehearsal-automation.tia.run/latest.json
    # -> JSON manifest matching the newest release on tia-tools/releases
    curl -sI https://rehearsal-automation.tia.run/download | head -5
    # -> HTTP 302 with location: the newest release's zip on GitHub
    # https://rehearsal-automation.tia.run/latest in a browser -> dist repo's latest release page

Before the first release exists on the dist repo, `/latest.json` returns 502 "No release" — expected.

## Anonymous metrics (per docs/adr/0012)

Every `/latest.json` hit records a `check` event and every resolved `/download...` hit a `download` event into the Analytics Engine dataset `dist_metrics` (binding `METRICS`). Dimensions: event type, client app version (parsed from the updater's User-Agent; empty for browsers), country, a daily-rotating salted IP hash (`HASH_SALT` secret; raw IPs never stored), and the asset name for downloads. A nightly cron (`30 2 * * *` UTC) aggregates each finished UTC day into KV (binding `HISTORY`) under `daily/<YYYY-MM-DD>` — permanent history, since Analytics Engine keeps raw events only ~90 days — backfilling up to 7 missed days. Read the numbers with `python scripts/dist_stats.py` from the repo root (token in the repo-root `.env`).

Secrets on this Worker: `HASH_SALT` (random, regenerable at the cost of one day's unique-count continuity) and `CF_ANALYTICS_TOKEN` (API token, Account Analytics: Read — if rollup keys stop appearing, this token has expired; recreate and `npx wrangler secret put CF_ANALYTICS_TOKEN`).

One-time account prerequisites (already done; recorded because both fail deploys confusingly if absent): Analytics Engine must be enabled in the dashboard (error 10089), and the account needs a workers.dev subdomain for cron schedules to attach (error 10063) — ours is `tia-tools-dev.workers.dev`, unused by this Worker.

## Rate limits

Anonymous GitHub API allows 60 req/hr per egress IP; the 5-minute edge cache keeps usage far below that. If it ever becomes a problem: `npx wrangler secret put GITHUB_TOKEN` with the **bot's** PAT (never a personal one, per ADR-0011).
