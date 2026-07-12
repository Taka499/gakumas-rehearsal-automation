---
status: accepted
---

# The tia.run Worker holds only least-privilege tokens; release credentials never leave the dev machine

Any credential stored on the Cloudflare Worker (as a wrangler secret or otherwise) must be scoped so that its leak cannot compromise the distribution channel: the 2026-07-08 security review identified the Cloudflare account as a compromise vector, and the app installs updates with administrator elevation, so a Worker-side credential that can publish releases is an admin-RCE supply-chain path. Concretely: the release-publishing bot PAT (`GAKUMAS_DIST_TOKEN`) lives ONLY on the developer's machine (repo-root `.env`, gitignored) and must NEVER be placed in the Worker; any Worker feature that needs GitHub access gets its own fine-grained PAT scoped to the minimum permission on the minimum repo (first instance: the feedback endpoint's issues-only PAT for the private `tia-tools/feedback` repo — worst case of a leak is issue spam, not a malware push). Rejected alternative: reusing the existing bot PAT in the Worker for convenience — one secret to manage, but it would collapse the separation that `docs/adr/0011` (identity-separated distribution) and `docs/adr/0013` (minisign signing as root of trust) exist to defend.

Source: user decision, /grill-me session 2026-07-11 (feedback-form design); threat model from the 2026-07-08 distribution security review. First applied in `docs/EXECPLAN_FEEDBACK_FORM.md`.
