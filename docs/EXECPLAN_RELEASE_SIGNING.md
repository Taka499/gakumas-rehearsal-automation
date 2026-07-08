# Sign releases so a leaked distribution credential can't push malware

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

This document must be maintained in accordance with `docs/PLANS.md` (repository root: `C:\Work\GitRepos\gakumas-rehearsal-automation`).

## Purpose / Big Picture

Right now the auto-updater's only integrity check is that the downloaded zip's SHA-256 matches the hash in the manifest. But the manifest and the zip come from the *same* origin (the `tia-tools/releases` GitHub repo, reshaped by the `rehearsal-automation.tia.run` Cloudflare Worker). Whoever controls either the release bot's GitHub token (`GAKUMAS_DIST_TOKEN`) or the Cloudflare account can publish a malicious zip *and* the matching hash; the updater will "verify" it, extract it, swap in the exe, and — because the app runs with administrator elevation and the restart reuses that elevation with no fresh Windows UAC prompt — run it as administrator on every user's machine. The hash defends against accidental corruption in transit, not against a compromised release. (This is finding #1 of the 2026-07-08 distribution security review; findings #2 and #3 are folded in here because they touch the same code.)

After this change, every release zip is accompanied by a **cryptographic signature** made with an Ed25519 secret key that lives only on the developer's own machine — never in the dist repo, never in Cloudflare, never in git. The updater carries the matching **public key baked into the binary** and refuses to install any download whose signature the public key does not verify. The security property a user can rely on afterward: *even if an attacker fully controls the GitHub dist repo and the Cloudflare account, they cannot make the updater install code the developer did not sign.* The signing key becomes the single root of trust, and it is the one thing that never touches the network-facing infrastructure.

You can see it working two ways. Positive: cut a signed release, run an older signed build, click アップデート, and it installs. Negative (the proof that matters): take a validly-signed release, tamper with one byte of the zip (or swap in a different zip and regenerate its hash and manifest, exactly as a repo-compromising attacker would), and the updater rejects it with a signature-verification error before anything is written to disk.

## Progress

- [x] (2026-07-08) M1: keypair generated (`~/.minisign/gakumas.key`, password-protected, offline-backed; public key `RWSsK+YIsZpesxIA/...` recorded); `.claude/commands/release.md` gained step 9a (sign) + `.minisig` in the upload list + background facts. `.gitignore` hardened against committing key files. (Signing stays in the skill, not `package-release.ps1`, as planned.)
- [x] (2026-07-08) M2: `PUBLIC_KEY` + `ALLOWED_DOWNLOAD_HOSTS` in `endpoints.rs`; `UpdateInfo.sig_url`; `parse_manifest`/`parse_github_release` surface the signature on both paths; `validated()` now requires a signature and an allowlisted download host (finding #2); `install.rs` verifies the signature with the embedded key BEFORE the hash, before extract/swap. 17 updater unit tests pass (incl. new missing-signature, off-host, and exact-host-match rejections). `#[ignore]`d `verify_signature_accepts_genuine_and_rejects_tampered` written against a committed fixture — needs the fixture signed (see M4 remaining).
- [x] (2026-07-08) M3: Worker manifest gains a `sig` field pointing at the `.minisig` sidecar via the tia.run download route (empty when absent, which the updater treats as non-installable by design). Code-complete; deploy alongside M4's first signed release.
- [~] M4: (completed: fixture signed + ignored verification test passes — genuine verifies, tamper + garbage rejected; key-custody runbook written; finding #3 mitigated via dist-README guidance to install under an admin-only dir + a signed-update-integrity note, pushed as `tia-tools-bot` commit `d419ccf`. Remaining: the live signed-release acceptance — cut the first signed release via `/release`, deploy the Worker `sig`-field change with it, and confirm on a real older→newer update that a genuine build installs and a hand-tampered artifact is rejected. This happens at the next release; then close out + no new ADR needed (0013 already written).)

## Surprises & Discoveries

- Observation: the signing test fixture must be marked binary in `.gitattributes` (`tests/fixtures/signing/* -text`), or git's autocrlf rewrites its line endings on checkout and the committed signature no longer matches the on-disk bytes the test reads.
  Evidence: `git add` warned `LF will be replaced by CRLF` on `sample.bin`; after the `.gitattributes` rule, `git check-attr text -- tests/fixtures/signing/sample.bin` reports `text: unset`.
- Observation: minisign key generation must not be run via a backgrounded/non-interactive shell — `rsign generate` blocks on an interactive password prompt and, if it later receives input, would generate a DIFFERENT keypair and overwrite the real one (a trust-breaking event). The keypair was generated interactively by the developer; the agent-launched attempt was stopped.
  Evidence: the background task sat at "Please enter a password to protect the secret key." with a 0-byte `.pub`; the real key is at `~/.minisign/gakumas.key` (259 bytes).

## Decision Log

- Decision: use **minisign/Ed25519 signatures** (dev signs with the `rsign2` CLI; the client verifies with the pure-Rust `minisign-verify` crate), NOT Windows Authenticode code-signing.
  Rationale: three constraints decide it. (1) Cost — Authenticode needs a certificate from a commercial CA (roughly $100–400/year, OV; EV more); minisign is free. (2) Anonymity — an Authenticode cert binds to a legal identity (individual or org) printed in the exe's signature and shown by Windows, which directly conflicts with the identity-separation posture of `docs/adr/0011`; a minisign key is anonymous. (3) Control — the minisign secret key is a local file the developer alone holds, exactly the "never touches the dist infra" property this plan needs. Authenticode's one advantage — Windows SmartScreen / "Unknown publisher" reputation — does not help this app, which is already an unsigned admin-elevated portable exe users must trust out-of-band. Authenticode can be layered on later if SmartScreen ever matters; it is not mutually exclusive with minisign. Recorded as a candidate ADR (see close-out).
  Date/Author: 2026-07-08, security-review follow-up (agent recommendation; **user confirmed minisign, 2026-07-08**, with Authenticode left as an optional future layer if SmartScreen reputation ever matters).
- Decision: signature verification is **mandatory** in the new updater — a missing or invalid signature aborts the install — rather than "verify only if a signature is present."
  Rationale: a soft/optional check buys nothing, because an attacker who can rewrite the manifest would simply omit the signature. The security only exists if absence is a hard failure. Safe to make mandatory because it is forward-looking: only builds that ship with the verification code enforce it, and every release from this plan onward is signed. The already-shipped v0.9.0 (never distributed to anyone) has no verification code and is unaffected; the developer's own copy updates into the first signed build via the normal path.
  Date/Author: 2026-07-08, security-review follow-up.
- Decision: the signing secret key lives as a password-protected file on the developer's machine only, gitignored, with an offline backup; it is never a GitHub Actions secret or a Cloudflare secret.
  Rationale: releases are cut locally by the `/release` skill (there is no CI publishing pipeline), so the key never needs to exist in any shared/automated system — which is the entire point (a shared secret store is exactly the blast-radius this plan removes). The password protects against laptop theft; the offline backup protects against laptop loss (losing the key would force baking a new public key into a future build and re-establishing trust).
  Date/Author: 2026-07-08, security-review follow-up.
- Decision: fold review findings #2 (updater accepts any `https://` download host) and #3 (writable-install-dir privilege risk) into this plan rather than spinning separate ones; #2 into M2 (same file, `src/update/mod.rs`), #3 into M4 as a doc mitigation.
  Rationale: #2 is a few lines in the exact function M2 already rewrites; batching avoids touching the trust-critical updater twice. #3 is a local-attacker issue, lower severity, and naturally a documentation addition.
  Date/Author: 2026-07-08, security-review follow-up.
- Decision: finding #3 is mitigated by DOCUMENTATION ONLY (dist README tells users to extract to an admin-only location such as `%ProgramFiles%`), NOT by a programmatic startup ACL check.
  Rationale: a *correct* "is my directory writable by non-administrators" test requires reading the directory's security descriptor and checking the DACL for write grants to non-admin SIDs (e.g. `Users`, S-1-5-32-545) — substantial `unsafe` Windows-API surface. A naive heuristic (e.g. "warn unless under Program Files") produces false positives/negatives that erode trust in the warning. For a Low-severity local-attacker finding on a portable tool, the disproportion isn't worth it; the extract-location guidance removes the exposure at its source. A programmatic check remains available as future polish if the threat model changes.
  Date/Author: 2026-07-09, M4 implementation judgement.

## Outcomes & Retrospective

- (to be written at completion)

## Context and Orientation

The reader needs to understand three files/areas; all paths are from the repo root.

The **updater** is `src/update/`. `mod.rs` fetches release metadata: `check_for_update()` → `fetch_update_info()` tries the Worker manifest (`MANIFEST_URL` in `endpoints.rs`, currently `https://rehearsal-automation.tia.run/latest.json`) and falls back to the GitHub Releases API (`FALLBACK_API_URL`). Both paths produce an `UpdateInfo { version, notes, url, sha256 }` and pass it through `validated()`, which today checks only that the version parses, `url` starts with `https://`, and `sha256` is 64 hex chars. `install.rs::download_and_install(info)` then downloads `info.url` to a temp file, checks its SHA-256 against `info.sha256` (`install.rs:46-54`), extracts the zip beside the live files, and rename-swaps the exe. The restart that activates the new exe is `src/gui/mod.rs::handle_restart` (spawns the swapped exe, which inherits admin elevation — no new UAC prompt).

Term: **minisign** is a small signature scheme (Ed25519 under the hood) by the author of libsodium. A *secret key* signs a file, producing a small `.minisig` text file; a *public key* verifies that `.minisig` against the file. There is no certificate authority and no expiry — trust comes entirely from the client holding the right public key. **`rsign2`** is the Rust command-line implementation (`cargo install rsign2` gives the `rsign` command) used here for the developer-side signing. **`minisign-verify`** is a dependency-free Rust crate that does verification only; the updater uses it.

The **release pipeline** is the `/release` skill (`.claude/commands/release.md`) which orchestrates `scripts/package-release.ps1` (builds the optimized exe, assembles `release/gakumas-rehearsal-automation/`) then, in the skill's own steps, zips it to `gakumas-rehearsal-automation-vX.Y.Z.zip`, writes a `.sha256` sidecar, tags the source repo, and publishes to `tia-tools/releases` as the bot account. The signature step slots in right where the `.sha256` sidecar is created (skill step 9), and the `.minisig` asset uploads alongside the zip and `.sha256` in the `gh release create` (step 12).

The **Worker** is `infra/worker/worker.js`. `latestJson()` builds the manifest object `{version, notes, url, sha256}` from the latest GitHub release, taking `sha256` from GitHub's asset digest or the `.sha256` sidecar. This plan adds a `sig` field the same way, sourced from the `.minisig` sidecar asset.

## Plan of Work

**M1 — signing keypair + release pipeline.** Generate the keypair once, on the developer's machine, with `rsign generate` (password-protected). This writes a secret key (default `~/.minisign/minisign.key` or a path you choose) and prints/writes the public key. Store the secret key OUTSIDE the repo tree entirely (e.g. `%USERPROFILE%\.minisign\gakumas.key`), and keep an offline backup; add its path to `.gitignore` defensively if it must live near the repo. Record the public key string (single line, starts with `RW`) — it gets embedded in M2. Then teach the pipeline to sign: after the `.sha256` sidecar is produced (`release.md` step 9), add a step that runs `rsign sign -s <secret-key-path> -x gakumas-rehearsal-automation-vX.Y.Z.zip.minisig gakumas-rehearsal-automation-vX.Y.Z.zip` (prompts for the key password), producing the `.minisig` sidecar; and add that file to the `gh release create` upload list in step 12. Update `scripts/package-release.ps1` only if you choose to move signing there — recommended to keep signing in the skill (step-level, where the zip already lives), since `package-release.ps1` neither zips nor knows the version. Document the whole flow in `release.md`'s background facts and steps.

**M2 — enforce verification in the updater.** Add `minisign-verify` to `Cargo.toml`. In `src/update/endpoints.rs`, add a `pub const PUBLIC_KEY: &str = "RW..."` constant (the M1 public key) with a comment that it is a root of trust baked into shipped binaries and changing it is a breaking trust event. Extend `UpdateInfo` with a `sig_url: String` (the `.minisig` download URL). In `mod.rs`: `parse_manifest` reads a new `sig` field; `parse_github_release` locates the `<zip>.minisig` asset's `browser_download_url` the same way it already locates the `.sha256` sidecar; `validated()` additionally requires `sig_url` non-empty and (fold in finding #2) requires the `url` host to be in an allowlist (`rehearsal-automation.tia.run`, plus `github.com`/`objects.githubusercontent.com` for the fallback asset host). In `install.rs::download_and_install`, after the temp download and BEFORE the SHA-256 check, download the `.minisig` text and call `minisign-verify` with the embedded `PUBLIC_KEY` over the downloaded zip bytes; on failure, `bail!` with a Japanese error (署名を確認できません) and write nothing further. Keep the SHA-256 check too (cheap defense-in-depth against corruption and a clear separate error message). Order matters: signature first (authenticity), then hash (integrity), then extract, then swap.

**M3 — Worker manifest `sig` field.** In `infra/worker/worker.js::latestJson`, after computing `sha256`, find the `.minisig` sidecar asset (`assets.find(a => a.name === `${zip.name}.minisig`)`) and add `sig: \`https://${DOMAIN}/download/${encodeURIComponent(zip.name)}.minisig\`` to the manifest object (a Worker-served URL, so it inherits the same host as the zip and passes the M2 allowlist). If no `.minisig` asset exists (e.g. a pre-signing release), omit the field or set it empty — the updater treats absence as a hard verification failure by design, which is correct: an unsigned release must not auto-install on a signing-aware client.

**M4 — ship, prove, document, finish.** Cut the first signed release through the updated `/release` flow. Prove the negative with a tampered artifact (see Validation). Add the finding-#3 mitigation: at startup, check whether the exe's own directory is writable by non-administrators and, if so, log a warning (and optionally surface a one-line GUI notice); document in the dist-repo README that the app should be extracted to a location normal users can't write (e.g. under `Program Files`) for the admin-elevation threat model. Write the key-custody runbook (where the secret key lives, how it's backed up, what to do if it's lost — bake a new public key into the next build — or suspected compromised — rotate key, re-sign, ship). Close out per `docs/PLANS.md`, promoting the minisign decision to an ADR.

## Concrete Steps

Signing tool install (once, developer machine):

    cargo install rsign2
    rsign generate -p gakumas-minisign.pub -s "%USERPROFILE%\.minisign\gakumas.key"
    # choose a strong password; back up the .key file offline; record the RW... public key

Adding the client verify dependency (repo root):

    # Cargo.toml [dependencies]: minisign-verify = "0.2"
    cargo check 2>&1 | grep "^error"   # expect no output

Signing during a release (fits into release.md between steps 9 and 12):

    rsign sign -s "%USERPROFILE%\.minisign\gakumas.key" \
      -x gakumas-rehearsal-automation-vX.Y.Z.zip.minisig \
      gakumas-rehearsal-automation-vX.Y.Z.zip
    # then include the .minisig in the gh release create upload list

Local negative-proof harness (M4, before trusting the live flow) — this is the load-bearing test and should be an `#[ignore]`d integration test so it is repeatable:

    # pseudocode of the test:
    #   sign a fixture zip with a throwaway key whose PUBLIC key is passed in
    #   assert verification PASSES for the untampered zip
    #   flip one byte of the zip
    #   assert verification FAILS (and download_and_install bails before swapping)

## Validation and Acceptance

Behavioral acceptance, in order of importance:

1. **Tamper rejection (the security property).** With a signed release published, construct the attacker's artifact: take the real zip, modify a byte, recompute its SHA-256, and hand-write a manifest pointing at it with the new hash (simulating full manifest+repo control) but leave the original `.minisig`. Point a debug build's updater at it. Expected: the install aborts at the signature step with 署名を確認できません, and no `.new`/`.old` files are created. Then also test the case where the attacker re-signs with the WRONG key (a key whose public half is not the embedded one): same rejection. This proves the embedded public key — not the origin — is the trust anchor.
2. **Genuine update still installs.** Running a signed build older than the latest, clicking アップデート, downloads, verifies signature + hash, swaps, and 再起動 launches the new version. Prove by version string in the header.
3. **Unit/integration tests.** `GAKUMAS_NO_MANIFEST=1 cargo test` passes; the new `#[ignore]`d signing integration test passes when run explicitly. The existing `parse_manifest`/`parse_github_release` tests are updated for the new `sig`/`sig_url` field and still pass.
4. **Host allowlist (finding #2).** A manifest whose `url` host is not on the allowlist is rejected by `validated()` (add a unit test mirroring the existing `http_url` rejection test).
5. **Writable-dir warning (finding #3).** Extracting to a user-writable dir and launching logs the warning; extracting under an admin-only path does not.

## Idempotence and Recovery

Key generation is one-time; re-running `rsign generate` would create a DIFFERENT key and silently break verification for every shipped binary carrying the old public key — so guard against accidental regeneration (the runbook must say "the key already exists; do not regenerate"). Signing a zip is idempotent (re-running overwrites the same `.minisig`). The updater changes are additive and covered by tests; a bad public key constant is caught immediately by the negative-proof test failing to verify a genuinely-signed zip. If the secret key is ever lost, recovery is: generate a new key, bake the new public key into the next release, and ship it the normal way — users on the last good signed build update into it (that build still verifies the OLD signatures, which is fine because the new release is signed with the new key... — NOTE: this transition needs care; see the runbook to be written in M4, which will specify shipping one release signed by BOTH keys or accepting that pre-transition builds can't auto-update to post-transition ones).

## Key-custody runbook

The signing key is the whole security model; treat it accordingly.

- **Where it lives.** Secret key: `~/.minisign/gakumas.key` (i.e. `%USERPROFILE%\.minisign\gakumas.key`), password-protected, on the developer's machine only. It is NOT in git, the dist repo, Cloudflare, or any CI/secret store — by design, so no network-facing system compromise can reach it. Public key: `RWSsK+YIsZpesxIA/bU6J4wwjBJajq9Ky8UGWyBcbOsb+VBkb2aUlw4Q`, embedded in `src/update/endpoints.rs::PUBLIC_KEY` (safe to publish).
- **Backup.** Keep at least one offline copy of the secret key file AND its password, stored separately (e.g. an encrypted USB drive + a password manager). Losing the key is a recoverable-but-painful event (see below); losing it with no backup means the current signing identity is gone forever.
- **Never regenerate over it.** Running `rsign generate` again writes a DIFFERENT key. Do not run it on a machine holding the real key. There is no reason to regenerate unless rotating (below).
- **Signing a release.** Handled by `/release` step 9a: `rsign sign -s ~/.minisign/gakumas.key -x <zip>.minisig <zip>`, then verify with the embedded public key before publishing. Requires the key password.
- **If the key is LOST (backup gone):** generate a new keypair, replace `PUBLIC_KEY` in `endpoints.rs`, ship a normal signed release with the new key. Users on any prior signed build update into it via the usual path (their build verifies the OLD key, and the transition release must therefore be signed with the OLD key — see rotation). If no old key exists to sign the transition, users on old builds cannot auto-update and must re-download manually once; new downloads are fine.
- **If the key is SUSPECTED COMPROMISED (rotation):** (1) generate a new keypair; (2) cut ONE transition release whose zip is signed with the OLD key (so existing builds accept it) but whose embedded `PUBLIC_KEY` is the NEW key; (3) all releases after that are signed with the NEW key only. After the transition ships, revoke/destroy the old secret key. This is the only clean way to move the trust anchor without stranding users — record the transition version here when it happens.
- **Rotation caveat baked into the code:** a shipped binary can only verify signatures made by the key it carries. Plan any key change as the two-step transition above, never a hard cutover.

## Artifacts and Notes

- 2026-07-08/09 unit proof: `verify_signature_accepts_genuine_and_rejects_tampered` (ignored) passes — the committed fixture `tests/fixtures/signing/sample.bin`, signed by the real secret key (prehashed minisign format), verifies against the embedded `PUBLIC_KEY`; a one-byte tamper and a garbage signature both fail. This confirms the embedded key matches the signing key with no typo.

      running 1 test
      test update::install::tests::verify_signature_accepts_genuine_and_rejects_tampered ... ok
      test result: ok. 1 passed; 0 failed; ...

- 17 updater unit tests pass under `GAKUMAS_NO_MANIFEST=1 cargo test update::`, including missing-signature rejection, off-host-download rejection, and exact-host-match (blocks `github.com.evil.test`).

## Interfaces and Dependencies

New client dependency: `minisign-verify = "0.2"` (pure Rust, no libsodium). Developer-side tool: `rsign2` (installed via `cargo install`, not a project dependency). In `src/update/endpoints.rs`:

    pub const PUBLIC_KEY: &str = "RW..."; // minisign public key; root of trust, baked into binaries

`UpdateInfo` gains `sig_url: String`. `install.rs` gains a verification step; signature is checked before hash, before extraction, before swap. The Worker manifest JSON gains an optional `sig` string field (a `rehearsal-automation.tia.run/download/<zip>.minisig` URL).
