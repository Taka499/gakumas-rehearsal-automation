Push main and publish a new versioned GitHub release of gakumas-screenshot (build → package → zip → `gh release`). Use this whenever the user asks to "cut a release", "make a release", "publish a new version", "ship it", or "do a release" for this repo.

Usage: /release [optional version or one-line theme, e.g. "v0.4.0 additional runs"]

This procedure publishes a public GitHub release, which is outward-facing and hard to undo. Two points REQUIRE a human decision before anything is published — the version number and the release notes. Stop and confirm both with the user; do not guess past them.

## Background facts about this repo

- **The release version is the git tag / `gh` release.** Find the latest with `gh release list`. Keep `Cargo.toml`'s `version` in sync: bump it to the release version and commit before tagging.
- **Version bump convention:** new user-facing feature(s) → minor bump (`v0.X.0`); bug fix / small change → patch bump (`v0.X.Y`). This mirrors the existing history (v0.2.0, v0.3.0 were feature releases; v0.3.1–v0.3.3 were fixes).
- **`scripts/package-release.ps1`** builds the optimized binary and assembles `release/gakumas-screenshot/` (exe + config.json + resources/). It does not zip and does not take a version.
- **The release asset is a zip** named `gakumas-screenshot-vX.Y.Z.zip` whose top-level folder is `gakumas-screenshot/` (matches every prior release).
- **The binary requires administrator elevation** (Windows manifest) to run, because `SendInput` must drive an elevated game process.
- Git on Windows prints `LF will be replaced by CRLF` warnings — these are noise, not errors.

## Steps

1. **Pre-flight.** Confirm the working state is releasable:
   - On `main` with the intended commits merged: `git status -sb` and `git log --oneline -5`.
   - Build is green: `cargo build --release 2>&1 | grep -E "^error" || echo OK` (only `error:` lines matter; ~30 warnings are expected).
   - **Remind the user** that the GUI binary needs admin elevation and that the manual game-acceptance scenarios for whatever shipped should have passed before releasing. If acceptance has not been done, ask whether to proceed anyway.

2. **Determine and CONFIRM the version.** Find the latest published version, then propose the next one:

       gh release list -L 5

   Decide minor vs patch from what changed (feature → `v0.X.0`, fix → `v0.X.Y`). Tell the user your proposed version and reasoning, and **wait for explicit confirmation** before continuing. If they gave a version in `$ARGUMENTS`, confirm it still makes sense against the latest release.

3. **Bump `Cargo.toml` to match and commit.** Set the `version` field to the confirmed `X.Y.Z` (no leading `v`), then commit just that file:

       git add Cargo.toml && git commit -m "Bump version to X.Y.Z"

4. **Push main** (includes the version bump):

       git push origin main

5. **Gather release-note highlights and draft notes.** Ask the user for the user-facing highlights (or derive a draft from the merged commits / the relevant ExecPlan `Purpose` section and show it for approval). Write the notes to a temp file, e.g.:

       cat > /tmp/release-notes-vX.Y.Z.md <<'EOF'
       ## New Features
       ### <feature> ...
       ## Install
       Download `gakumas-screenshot-vX.Y.Z.zip`, extract, and run `gakumas-screenshot.exe` as administrator. Embedded Tesseract OCR extracts on first run.
       EOF

   Keep the notes focused on what the user can now do. Get the user's OK on the draft.

6. **Build and package** (assembles `release/gakumas-screenshot/`):

       powershell -ExecutionPolicy Bypass -File scripts/package-release.ps1

7. **Zip with the correct top-level folder** (run from repo root so `gakumas-screenshot/` is the archive root):

       powershell -Command "Compress-Archive -Path 'release/gakumas-screenshot' -DestinationPath 'gakumas-screenshot-vX.Y.Z.zip' -Force"

8. **Verify the zip structure** — the first entries must be under `gakumas-screenshot\...`, not loose files:

       unzip -l gakumas-screenshot-vX.Y.Z.zip | head -8

9. **Publish the release** (creates the tag at `main` and uploads the zip):

       gh release create vX.Y.Z gakumas-screenshot-vX.Y.Z.zip \
         --target main \
         --title "vX.Y.Z - <short theme>" \
         --notes-file /tmp/release-notes-vX.Y.Z.md

10. **Verify it published** and is marked Latest:

        gh release view vX.Y.Z --json tagName,name,assets,isDraft,isPrerelease \
          -q '"tag: \(.tagName)  draft: \(.isDraft)  prerelease: \(.isPrerelease)\nassets: \(.assets | map(.name) | join(", "))"'
        gh release list -L 1

    Report the release URL.

11. **Cleanup (offer, don't assume).** The published zip also sits in the working dir; older `gakumas-screenshot-v*.zip` files may linger. Offer to delete superseded zips. Do NOT touch the tracked tree or commit anything here — these zips are untracked artifacts.

## Notes

- If `gh release create` fails because the tag already exists, the version was likely already published — re-check `gh release list` and pick the next number rather than overwriting.
- Do not push or publish without the user's go-ahead on version (step 2) and notes (step 5). The local Cargo.toml commit (step 3) is reversible; from the push (step 4) onward it is visible to others.
