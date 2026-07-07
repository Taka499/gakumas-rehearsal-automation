# tia.run distribution Worker

Cloudflare Worker fronting the dist repo `tia-tools/releases` (per `docs/adr/0011`). Stateless — it reshapes the GitHub Releases API into an update manifest and download redirects, edge-cached 5 minutes. Publishing a release is the only state change; nothing here ever needs a data update.

| Route | Serves |
|---|---|
| `https://tia.run/latest.json` | update manifest: `{version, notes, url, sha256}` |
| `https://tia.run/download/<asset>` | 302 to the GitHub release asset |
| `https://tia.run/` and `/latest` | 302 to the latest-release page (human-shareable link) |

## Deploy

Requires Node.js. Run from this directory (`infra/worker/`):

    npx wrangler login      # MUST be the PROJECT Cloudflare account (the one holding tia.run), not a personal one
    npx wrangler deploy

The `custom_domain` route in `wrangler.toml` makes Cloudflare attach the Worker to `tia.run` and manage DNS automatically — no manual DNS records.

## Verify

    curl -s https://tia.run/latest.json
    # -> JSON manifest matching the newest release on tia-tools/releases
    curl -sIL "https://tia.run/download/<zip name from the manifest>" | tail -5
    # -> ends in HTTP 200 with the zip's content-length
    # https://tia.run/latest in a browser -> dist repo's latest release page

Before the first release exists on the dist repo, `/latest.json` returns 502 "No release" — expected.

## Rate limits

Anonymous GitHub API allows 60 req/hr per egress IP; the 5-minute edge cache keeps usage far below that. If it ever becomes a problem: `npx wrangler secret put GITHUB_TOKEN` with the **bot's** PAT (never a personal one, per ADR-0011).
