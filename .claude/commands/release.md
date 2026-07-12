Push main and publish a new versioned release of gakumas-rehearsal-automation to the distribution repo `tia-tools/releases` (build → package → zip → sha256 → `gh release` as the bot account). Use this whenever the user asks to "cut a release", "make a release", "publish a new version", "ship it", or "do a release" for this repo.

Usage: /release [optional version or one-line theme, e.g. "v0.4.0 additional runs"]

This procedure publishes a public GitHub release, which is outward-facing and hard to undo. Two points REQUIRE a human decision before anything is published — the version number and the release notes. Stop and confirm both with the user; do not guess past them.

## Background facts about this repo

- **Distribution is identity-separated (per `docs/adr/0011`).** New release assets publish ONLY to `tia-tools/releases`, authored by the machine account `tia-tools-bot` via its fine-grained PAT, which is stored in the repo-root `.env` (gitignored) as `GAKUMAS_DIST_TOKEN`. NEVER run `gh release create` against the dist repo with ambient `gh auth` — that would stamp the personal account as the release author on the public page, defeating the whole channel. NEVER publish new assets to the personal repo (its pre-v0.9 releases are frozen history).
- **The release version is the git tag.** The tag `vX.Y.Z` is pushed to THIS repo (source of truth for version history) and the release of the same tag is created on `tia-tools/releases`. Find the latest with `git tag --sort=-v:refname | head -5` and `gh release list -R tia-tools/releases -L 5` (the dist repo only has releases from v0.9-era onward; older ones live on the personal repo). Keep `Cargo.toml`'s `version` in sync: bump it to the release version and commit before tagging.
- **Version bump convention:** new user-facing feature(s) → minor bump (`v0.X.0`); bug fix / small change → patch bump (`v0.X.Y`). This mirrors the existing history (v0.2.0, v0.3.0 were feature releases; v0.3.1–v0.3.3 were fixes).
- **`scripts/package-release.ps1`** builds the optimized binary and assembles `release/gakumas-rehearsal-automation/` (exe + config.json + resources/). It does not zip and does not take a version.
- **The release asset is a zip** named `gakumas-rehearsal-automation-vX.Y.Z.zip` whose top-level folder is `gakumas-rehearsal-automation/` (matches every prior release), accompanied by a `.sha256` sidecar AND a `.minisig` signature sidecar (both consumed by the in-app updater; the signature is mandatory — an unsigned release will NOT auto-install on current clients).
- **Releases are cryptographically signed (per `docs/EXECPLAN_RELEASE_SIGNING.md` / `docs/adr/0013`).** The zip is signed with the developer's minisign secret key at `~/.minisign/gakumas.key` (password-protected, never in git/dist-repo/Cloudflare); the matching public key is baked into the binary (`src/update/endpoints.rs::PUBLIC_KEY`). This is the trust anchor: a compromised dist repo or Cloudflare account still cannot push malware the updater will accept. Signing needs the `rsign` tool (`cargo install rsign2`) and the key password. NEVER regenerate the key (it would break verification for every shipped binary).
- **The binary requires administrator elevation** (Windows manifest) to run, because `SendInput` must drive an elevated game process.
- Git on Windows prints `LF will be replaced by CRLF` warnings — these are noise, not errors.

## Steps

1. **Pre-flight.** Confirm the working state is releasable:
   - On `main` with the intended commits merged: `git status -sb` and `git log --oneline -5`.
   - Build is green: `cargo build --release 2>&1 | grep -E "^error" || echo OK` (only `error:` lines matter; ~30 warnings are expected).
   - The bot PAT is available: `grep -q GAKUMAS_DIST_TOKEN .env && echo TOKEN-PRESENT`.
   - **Remind the user** that the GUI binary needs admin elevation and that the manual game-acceptance scenarios for whatever shipped should have passed before releasing. If acceptance has not been done, ask whether to proceed anyway.

2. **Determine and CONFIRM the version.** Find the latest published version, then propose the next one:

       git tag --sort=-v:refname | head -5
       set -a; source .env; set +a
       GH_TOKEN="$GAKUMAS_DIST_TOKEN" gh release list -R tia-tools/releases -L 5

   Decide minor vs patch from what changed (feature → `v0.X.0`, fix → `v0.X.Y`). Tell the user your proposed version and reasoning, and **wait for explicit confirmation** before continuing. If they gave a version in `$ARGUMENTS`, confirm it still makes sense against the latest release.

3. **Bump `Cargo.toml` to match and commit.** Set the `version` field to the confirmed `X.Y.Z` (no leading `v`), then commit just that file:

       git add Cargo.toml && git commit -m "Bump version to X.Y.Z"

4. **Push main** (includes the version bump):

       git push origin main

5. **Draft the notes in `CHANGELOG.md` FIRST, then derive the release body.** Release notes are authored bilingually in the repo-root `CHANGELOG.md` (its header comment states the format; per `docs/EXECPLAN_CHANGELOG_AND_JP_NOTES.md`): add a new `## vX.Y.Z — YYYY-MM-DD` section at the top with a one-line Japanese summary, Japanese bullets for users, and a `### English` subsection for maintainers. The file is embedded into the binary (`src/gui/changelog.rs::CHANGELOG_MD` via `include_str!`) and shown in the in-app 更新履歴 window, so it MUST be written and committed BEFORE the build/package step (an entry added after building would not be in the shipped exe). Commit it together with (or right after) the version bump; push again if main was already pushed. Get the user's OK on the draft — this is the second REQUIRED human decision.

   The GitHub release body is that section's content with the `## vX.Y.Z` heading dropped: the Japanese one-liner first, blank line, then the Japanese bullets and the `### English` subsection (GitHub shows both languages; that is intended). Write it to a temp file, e.g.:

       cat > /tmp/release-notes-vX.Y.Z.md <<'EOF'
       <one-line Japanese summary — same line as in CHANGELOG.md>

       - <Japanese bullets...>

       ### English
       - <English bullets...>

       ## Install
       Download `gakumas-rehearsal-automation-vX.Y.Z.zip`, extract, and run `gakumas-rehearsal-automation.exe` as administrator. Embedded Tesseract OCR extracts on first run.
       EOF

   Keep the notes focused on what the user can now do, in a neutral voice (no personal links or signatures — the dist repo is the identity-separated channel).

   IMPORTANT — first-paragraph convention: the Worker's manifest `notes` field (shown as the in-app アップデート button's hover hint) is only the FIRST paragraph of the body, split on the first blank line. The body MUST open with the one-line JAPANESE summary (the app's users read Japanese; this is why CHANGELOG.md leads every section with it), then a blank line, then the detail. Never open with a `## Heading` or an English sentence.

6. **Build and package** (assembles `release/gakumas-rehearsal-automation/`):

       powershell -ExecutionPolicy Bypass -File scripts/package-release.ps1

7. **Zip with the correct top-level folder** (run from repo root so `gakumas-rehearsal-automation/` is the archive root):

       powershell -Command "Compress-Archive -Path 'release/gakumas-rehearsal-automation' -DestinationPath 'gakumas-rehearsal-automation-vX.Y.Z.zip' -Force"

8. **Verify the zip structure** — the first entries must be under `gakumas-rehearsal-automation\...`, not loose files:

       unzip -l gakumas-rehearsal-automation-vX.Y.Z.zip | head -8

9. **Create the sha256 sidecar** (the in-app updater verifies downloads against this):

       sha256sum gakumas-rehearsal-automation-vX.Y.Z.zip | awk '{print $1}' > gakumas-rehearsal-automation-vX.Y.Z.zip.sha256
       cat gakumas-rehearsal-automation-vX.Y.Z.zip.sha256   # 64 hex chars

9a. **Sign the zip** (mandatory — the updater rejects unsigned or mismatched downloads). Prompts for the key password; the developer must run this, not an agent shell:

       rsign sign -s "$HOME/.minisign/gakumas.key" -x gakumas-rehearsal-automation-vX.Y.Z.zip.minisig gakumas-rehearsal-automation-vX.Y.Z.zip
       # verify against the PUBLIC key embedded in the binary before publishing.
       # Extract ONLY the base64 between the quotes (a bare `RW\S+` grabs the
       # trailing `";` and fails base64 decode):
       rsign verify -P "$(grep -oP '"\KRW[^"]+' src/update/endpoints.rs | head -1)" -x gakumas-rehearsal-automation-vX.Y.Z.zip.minisig gakumas-rehearsal-automation-vX.Y.Z.zip
       # -> "Signature and comment signature verified"

10. **Privacy scrub.** The artifacts must contain no identifying strings. Expect `0` from both (any hit → STOP and investigate before publishing):

        grep -ac "takatomo\|Taka499" release/gakumas-rehearsal-automation/gakumas-rehearsal-automation.exe || echo 0
        grep -ac "takatomo\|Taka499" gakumas-rehearsal-automation-vX.Y.Z.zip || echo 0

11. **Tag this repo** (version history stays with the source):

        git tag vX.Y.Z && git push origin vX.Y.Z

12. **Publish the release to the dist repo AS THE BOT** (uploads zip + sidecar; no `--target` — the dist repo's default branch is fine):

        set -a; source .env; set +a
        GH_TOKEN="$GAKUMAS_DIST_TOKEN" gh release create vX.Y.Z \
          gakumas-rehearsal-automation-vX.Y.Z.zip \
          gakumas-rehearsal-automation-vX.Y.Z.zip.sha256 \
          gakumas-rehearsal-automation-vX.Y.Z.zip.minisig \
          -R tia-tools/releases \
          --title "vX.Y.Z - <short theme>" \
          --notes-file /tmp/release-notes-vX.Y.Z.md

13. **Verify it published, is marked Latest, and — critically — is authored by the bot:**

        GH_TOKEN="$GAKUMAS_DIST_TOKEN" gh release view vX.Y.Z -R tia-tools/releases \
          --json tagName,name,assets,isDraft,isPrerelease,author \
          -q '"tag: \(.tagName)  author: \(.author.login)  draft: \(.isDraft)\nassets: \(.assets | map(.name) | join(", "))"'

    `author` MUST be `tia-tools-bot`. If it shows the personal account, the wrong token was used: delete the release (`gh release delete` with the same `-R`), fix the token, and republish. Report the release URL (`https://github.com/tia-tools/releases/releases/tag/vX.Y.Z`).

14. **Cleanup (offer, don't assume).** The published zip and `.sha256` also sit in the working dir; older `gakumas-rehearsal-automation-v*.zip*` files may linger. Offer to delete superseded ones. Do NOT touch the tracked tree or commit anything here — these are untracked artifacts.

## Notes

- If `gh release create` fails because the tag already exists on the dist repo, the version was likely already published — re-check `gh release list -R tia-tools/releases` and pick the next number rather than overwriting.
- If the dist repo's README is still a stub, offer to write it while publishing: neutral-voice description of the tool + install steps (download zip → extract → run as administrator), linking nowhere personal.
- Do not push or publish without the user's go-ahead on version (step 2) and notes (step 5). The local Cargo.toml commit (step 3) is reversible; from the push (step 4) onward it is visible to others.
