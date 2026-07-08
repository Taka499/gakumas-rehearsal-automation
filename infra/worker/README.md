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

## Rate limits

Anonymous GitHub API allows 60 req/hr per egress IP; the 5-minute edge cache keeps usage far below that. If it ever becomes a problem: `npx wrangler secret put GITHUB_TOKEN` with the **bot's** PAT (never a personal one, per ADR-0011).
