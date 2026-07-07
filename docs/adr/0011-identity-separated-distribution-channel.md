---
status: accepted
---

# Distribution is identity-separated: org dist repo behind a domain, bot-authored, dist-only

Public distribution moves off the personal GitHub account. A neutral GitHub organization hosts a releases-only repository (`tia-tools/releases` — no code, no commit trail), fronted by a project domain (`tia.run`) whose Cloudflare Worker serves a stateless `latest.json` manifest and download redirects. New releases publish ONLY to the dist repo (this repo keeps its git tags but gains no new release assets), and are created by a machine account (`tia-tools-bot`) so the release page never shows the personal handle (GitHub displays the release creator; org membership privacy does not hide it). The in-app auto-updater checks the domain first and falls back to the org's GitHub Releases API — both endpoints are baked into shipped binaries, which is what makes this hard to reverse. Rejected alternatives: publishing to both repos (drift, and every release mints a fresh personal-account link), transferring the whole repo to the org (commit avatars put the personal profile one click from the download page), personal-account release authorship (defeats the org boundary at exactly the page downloaders land on). Scope of the goal: separation from *casual* downloaders only — the open-source repo and its commit authorship remain traceable by design, per the user's explicit acceptance.

Source: user decisions, /grill-me session 2026-07-07; implementation plan `docs/EXECPLAN_AUTO_UPDATE_DISTRIBUTION.md`. Confirmed end-to-end 2026-07-08: v0.9.0 published to the dist repo (author verified `tia-tools-bot`, privacy scrub 0 hits) and the in-app updater's M5 click-through passed against the live channel.
