---
status: accepted
---

# Distribution metrics are anonymous-by-design: no persistent identifiers, uniqueness only via a daily-rotating salted IP hash

The tia.run Worker records usage events (update checks, downloads) with dimensions limited to: day, event type, client version (parsed from the updater's existing User-Agent), country code, and an anonymous daily bucket computed at the edge as truncated SHA-256 of (client IP + UTC date + secret salt). The salt is a Cloudflare Worker secret, never in git; because the UTC date is part of the input, the same user hashes to a different bucket every day, so buckets cannot be chained into a cross-day profile and are irreversible without the salt. No persistent client identifier may ever be introduced (rejected: an anonymous install ID in the updater — accurate retention metrics, but it is a durable per-user tracker; also rejected: event-counts-only — strongest privacy but no unique-user signal at all). Any future telemetry must fit inside this posture; raw IPs are never stored anywhere.

Source: user decisions, /grill-me session 2026-07-08; implementation plan `docs/EXECPLAN_DIST_PERMALINK_AND_METRICS.md`.
