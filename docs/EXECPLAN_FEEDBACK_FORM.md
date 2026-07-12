# In-app feedback form → tia.run Worker → private GitHub issues

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. It must be maintained in accordance with `docs/PLANS.md` (repository root: `C:\Work\GitRepos\gakumas-rehearsal-automation`).

## Purpose / Big Picture

Users of the distributed app (installed from a zip downloaded via `rehearsal-automation.tia.run`; they have no GitHub account and we know nothing about them) currently have no way to report a bug or request a feature. After this change, a 「フィードバック」 button in the GUI header opens a small form: the user types a message, picks a category (バグ / 要望 / その他), and — for bug reports — may attach one session's log, explicitly selected from a dropdown. Pressing 送信 POSTs the payload to the existing Cloudflare Worker, which creates a labeled GitHub issue in the private repo `tia-tools/feedback`. The developer triages feedback with GitHub's ordinary notification and issue UI; the user sees 「送信しました」 on success or an inline error (with their text preserved) on failure.

To see it working end-to-end: run the app, click 「フィードバック」 in the header, type a message, press 送信, and watch a new issue appear at `https://github.com/tia-tools/feedback/issues` authored by `tia-tools-bot`.

## Progress

- [x] (2026-07-11) Design interview complete; all decisions recorded in the Decision Log.
- [x] (2026-07-11) `docs/adr/0015-worker-holds-only-least-privilege-tokens.md` written and indexed in CLAUDE.md.
- [x] (2026-07-11) Full plan drafted (this document).
- [x] (2026-07-12) M1 code + infra: private repo `tia-tools/feedback` created with `bug`/`request`/`other` labels (bot turned out to be org owner, so the triage grant was redundant); Worker `/feedback` route written, deployed, and probed — all 9 rejection-path probes PASS (405/400×6/413, and the valid pre-token probe correctly 502s), `/latest.json` regression-checked OK. Committed `82c762d`.
- [x] (2026-07-12) M1 acceptance PASSED: user created the issues-only fine-grained PAT and set the `FEEDBACK_TOKEN` wrangler secret; two live probes both returned `{"ok":true}` and produced issues #1/#2 in `tia-tools/feedback` authored by `tia-tools-bot` — correct `[その他]`/`[バグ]` titles (first line only), correct labels, `**Version:**` line present; the 70,000-char-log probe yielded a body of exactly 65,000 chars with the tail marker preserved and the head marker cut, inside the collapsed details block. Probe issues closed after verification. The 429 path was NOT probed to preserve the shared 5/day IP budget for the user's click-through (2 of 5 spent); the 6th submission of the day doubles as the 429 acceptance.
- [x] (2026-07-12) M2: `src/feedback/mod.rs` written and registered in `src/main.rs`; 7 unit tests pass (`GAKUMAS_NO_MANIFEST=1 cargo test feedback`). Note: reqwest's `json` feature is NOT enabled in this repo — the sender sets content-type and serializes via `payload.to_string()` instead. Committed `d7bf958`.
- [x] (2026-07-12) M3 code: `FeedbackUiState` in `src/gui/state.rs`; header フィードバック button (right-aligned on the heading row) + floating window + dedicated `feedback_tx`/`feedback_rx` mpsc pair + `poll_feedback_messages`/`handle_open_feedback`/`handle_send_feedback`/`render_feedback_window` in `src/gui/mod.rs`. Full suite 148 passed; guarded release build clean (28 expected warnings). Committed `0da4054`.
- [ ] M3 acceptance: USER performs the live click-through in Validation and Acceptance. Partially done (2026-07-12): one real submission from the UI (その他, no log) arrived as issue #3 with correct title/label/version/body — the UI→Worker→issue path is proven. Remaining: bug report with the newest session log attached (verify the log block content), offline → inline error with text preserved → reconnect retry succeeds, and mid-run reachability of the header button. Budget note: 3 of today's 5 submissions used (2 probes + 1 user); a 6th today would demonstrate the 429.
- [x] (2026-07-12) CLAUDE.md's Active ExecPlans list updated (done at design time).
- [ ] Run close-out (`/close-out`) when acceptance passes; record the PAT expiry date in Artifacts and Notes.

## Surprises & Discoveries

- Observation: GitHub has no public API for attaching files to issues (the web UI's drag-and-drop upload to `user-attachments` is browser-only). A bot can only put text in the body/comments or commit files to the repo.
  Evidence: the GitHub REST API has no attachment-upload endpoint for issues. This constraint forced the "inline, truncated" log-transport decision below.
- Observation: GitHub issue bodies cap at 65,536 characters, and this app's `session.log` files (every timestamped `log()` line of a run; see `src/main.rs::log`) exceed that for long series, so the log must be truncated client-side and defensively re-truncated Worker-side.
- Observation: `tia-tools-bot` is an owner of the `tia-tools` org, so it already has admin on the new private repo — the planned collaborator grant was a no-op. The least-privilege boundary therefore rests entirely on the fine-grained PAT's scoping (effective access = intersection of token permissions and user permissions), which is exactly what docs/adr/0015 prescribes.
  Evidence: `gh api repos/tia-tools/feedback/collaborators/tia-tools-bot/permission` → `{"permission":"admin"}` (2026-07-12).
- Observation: this repo's `reqwest` has only the `blocking` feature — `RequestBuilder::json()` doesn't compile (E0599). The sender sets the content-type header and serializes with `serde_json::Value::to_string()` instead of enabling the extra feature.
- Observation: the Worker's message-length check is JS `String.length` (UTF-16 code units), so the form counts `message.encode_utf16().count()` rather than Rust `chars().count()` — astral characters (emoji) count as 2 in both places, and nothing the form accepts can be rejected server-side.
- Observation: the rate-limit counter is incremented only AFTER a successful issue creation (a mid-review refinement of the plan's original wording): transient GitHub failures must not eat a legitimate user's 5-per-day quota. The pre-check still happens before the GitHub call, so the limit holds.

## Decision Log

- Decision: Feedback channel is an in-app form in the GUI that POSTs to a new endpoint on the existing tia.run Cloudflare Worker (`infra/worker/worker.js`), which creates a GitHub issue.
  Rationale: Users installed a zip and cannot be assumed to have GitHub accounts; the identity-separated distribution channel (docs/adr/0011) already provides a neutral, $0 ingestion point; GitHub issues give the developer notifications and a triage UI for free. Alternatives rejected: external hosted form (ties feedback to a personal Google identity, loses in-app version context), direct GitHub-issues link (requires user accounts, public exposure), email (spam, reveals an address).
  Date/Author: 2026-07-11 / grilling interview with user.

- Decision: Issues are created in a NEW PRIVATE repo `tia-tools/feedback`, using a NEW fine-grained PAT scoped to issues-write on only that repo, stored as a wrangler secret on the Worker. The release-publishing bot PAT (`GAKUMAS_DIST_TOKEN`) must NEVER be placed in the Worker.
  Rationale: Feedback is user-generated text (may contain rants, personal info) and must not be public in `tia-tools/releases`. The 2026-07-08 security review identified the Cloudflare account as a compromise vector; a leaked issues-only token can spam issues but cannot push malware releases (complements docs/adr/0011 and docs/adr/0013). A personal-account repo was rejected as re-coupling the distribution channel to the developer's identity.
  Date/Author: 2026-07-11 / grilling interview with user.

- Decision: The form sends exactly: user-typed message (required), a category picker (bug / request / other), and the app's own version string auto-attached. When category = bug, an additional field appears offering selection of a session log to attach; nothing is ever collected silently.
  Rationale: Consistent with the project's anonymous-by-design public posture (docs/adr/0012 — no persistent identifiers, no silent machine data). Version is always wanted for triage and is already known to the app. Log attachment is an explicit user act, mirroring docs/adr/0007's "explicit human act" philosophy.
  Date/Author: 2026-07-11 / grilling interview with user.

- Decision: The selected session log travels inline: the app truncates to the most recent ~60KB and the Worker embeds it in a collapsed HTML details block in the issue body. No repo-file commit, no attachment.
  Rationale: GitHub offers no API for issue file attachments, and issue bodies cap at 65,536 characters while session logs regularly exceed that. Inline-truncated needs only the issues-scoped PAT and one API call; committing full logs as repo files would require contents-write permission and multi-MB payloads. The log tail is where crashes/aborts live; a full log can be requested in follow-up if ever needed.
  Date/Author: 2026-07-11 / grilling interview with user (user challenged "why not embed the file?" — answered: embedding text IS the plan; a literal file attachment is impossible via API).

- Decision: The endpoint is public and unauthenticated (any secret shipped in the binary is extractable), guarded by a request size cap (~100KB) and a light rate limit of ~5 submissions per day per daily-rotating salted IP hash — the same hash the Worker already computes for metrics (docs/adr/0012, `worker.js::dailyBucket`), so no new identifiers are introduced. No CAPTCHA.
  Rationale: Worst case is issue spam in a private repo (annoying, bulk-closable, cannot touch releases thanks to the issues-only PAT). Turnstile would force a browser/webview challenge into a native egui app — heavy machinery for a zero-value target.
  Date/Author: 2026-07-11 / grilling interview with user.

- Decision: The form opens from a small 「フィードバック」 button in the GUI's top header panel (`src/gui/mod.rs`, the `header_panel` TopBottomPanel) and is presented as a floating `egui::Window`. It does not touch the state-driven control panel.
  Rationale: The header renders in every automation state (it already hosts the update notice — the app's other "talks to tia.run" surface), so feedback is reachable at idle, mid-run, and finished alike. Alternatives (guide-panel footer, finished-state-only placement) were rejected as less reachable.
  Date/Author: 2026-07-11 / grilling interview with user.

- Decision: When category = bug, the log picker is a dropdown of the ~10 most recent `output/YYYYMMDD_HHMMSS/` sessions (newest first), with the newest session preselected and an explicit 「添付しない」 option.
  Rationale: A bug report almost always concerns the run just performed; the selection is plainly visible in the form before sending, so preselection is convenience, not silent collection. Defaulting to "none" would make most bug reports arrive logless.
  Date/Author: 2026-07-11 / grilling interview with user.

- Decision: Conventional details resolved without grilling (user offered no objection when they were listed): the send happens on a worker thread with a spinner (same pattern as the update check); success shows 「送信しました」 and closes the form window; failure shows an inline red error with the typed text preserved for manual retry — no background queue or auto-retry. The endpoint is `POST https://rehearsal-automation.tia.run/feedback` (per docs/adr/0014's per-app-subdomain scheme). The created issue carries a `bug`/`request`/`other` label, a title derived from the category plus the message's first line, and a body of app version + message + collapsed log. Form labels are Japanese, consistent with the rest of the GUI. No metrics event is recorded for feedback submissions (the issues themselves are the record; one can be added later if volume ever matters).
  Rationale: All follow established patterns in this repo; none passed the "genuine judgment call" bar.
  Date/Author: 2026-07-11 / resolved by agent during grilling interview, listed to user.

- Decision: The credential-scoping rule ("the Worker holds only least-privilege tokens; release credentials never leave the dev machine") was promoted to `docs/adr/0015-worker-holds-only-least-privilege-tokens.md` and indexed in CLAUDE.md, because it constrains every future Worker feature, not just this plan.
  Rationale: Passed all three ADR gates (hard to reverse once a PAT is exposed to CF; surprising without the 2026-07-08 security-review context; a real trade-off against single-secret convenience).
  Date/Author: 2026-07-11 / user confirmed during grilling interview.

- Decision: The feedback URL constant lives in a new `src/feedback/mod.rs` (with a doc comment citing docs/adr/0014), not in `src/update/endpoints.rs`.
  Rationale: `endpoints.rs` documents itself as "the only two update-channel URLs" and is security-load-bearing for the updater (docs/adr/0011/0013); feedback is a different channel with different trust properties (nothing is downloaded or executed). Keeping it separate avoids diluting that file's contract.
  Date/Author: 2026-07-11 / agent decision while drafting the plan.

## Outcomes & Retrospective

(To be written at completion.)

## Context and Orientation

This repository builds `gakumas-rehearsal-automation.exe`, a Windows GUI tool (egui via eframe) that automates rehearsal runs in the game `gakumas.exe` and OCRs score screenshots. Pieces this plan touches:

- `src/gui/mod.rs` — the eframe `GuiApp`. Its `update()` renders a top header (`egui::TopBottomPanel::top("header_panel")`, around line 865) showing the app title, a hotkey hint, and the auto-update notice. `GuiApp` owns an `mpsc` channel pair (`update_tx`/`update_rx`) whose messages are produced by spawned worker threads and drained each frame by `poll_update_messages()`. The feedback send will follow this exact pattern with its own channel.
- `src/gui/state.rs` — `GuiState`, the plain-data UI state struct. New feedback-window state goes here.
- `src/update/mod.rs` — the update check: a `reqwest::blocking::Client` with 5 s connect / 10 s total timeouts and User-Agent `gakumas-rehearsal-automation/<version>` (from `env!("CARGO_PKG_VERSION")`), called only from a spawned thread. The feedback sender reuses this client shape (`reqwest` with the `blocking` feature is already a dependency).
- `src/automation/session_meta.rs` — session-folder helpers. Each automation series writes to `output/YYYYMMDD_HHMMSS/` containing `screenshots/`, `results.csv`, `session.log`, `charts/`, `run-meta.json`. `list_resumable()` shows the enumeration idiom (read `output/`, filter dirs, sort by name descending — folder names sort chronologically). The feedback log picker enumerates ALL sessions that have a `session.log`, not just resumable ones.
- `src/paths.rs` — `get_output_dir()` resolves the `output/` root next to the exe.
- `infra/worker/worker.js` — the stateless Cloudflare Worker serving `rehearsal-automation.tia.run` (routes `/latest.json`, `/download`, `/download/<asset>`, `/`). It already computes a daily-rotating salted IP hash (`dailyBucket(ip, day, env.HASH_SALT)`) for anonymous metrics (docs/adr/0012) and has a KV namespace binding `HISTORY` (used by the nightly metrics rollup). The new `/feedback` route reuses both.
- `infra/worker/wrangler.toml` — Worker config (bindings for Analytics Engine `METRICS`, KV `HISTORY`, cron). Deploy from `infra/worker/` with `npx wrangler deploy`; secrets with `npx wrangler secret put <NAME>`.

Terms: a **fine-grained PAT** is a GitHub personal access token restricted to named repositories and named permissions (here: only `tia-tools/feedback`, only Issues read/write). A **wrangler secret** is an encrypted environment variable available to the Worker as `env.<NAME>`, set via the wrangler CLI and never present in git. `tia-tools` is the neutral GitHub org from docs/adr/0011; `tia-tools-bot` is its machine account.

Sizing constants (used throughout): `LOG_TAIL_MAX = 60_000` bytes — the client sends at most this much log text, from the file's tail, cut on a UTF-8 character boundary. `MESSAGE_MAX = 4_000` characters for the typed message. The Worker rejects request bodies over `100_000` bytes and, when composing the issue, re-truncates the log from the head so the final body stays under 65,000 characters (GitHub's limit is 65,536).

## Plan of Work

Milestone 1 is infrastructure: create the private repo and token, and teach the Worker the `/feedback` route. In `infra/worker/worker.js`, add a `POST /feedback` branch to `fetch()` (before the 404) that: rejects non-POST with 405; rejects `Content-Length` over 100,000 with 413; parses JSON `{category, message, version, log_name, log}` and validates (category must be one of `"bug" | "request" | "other"`; message non-empty after trim and ≤ 4,000 chars; version matches `\d+\.\d+\.\d+` or is empty; log ≤ 65,000 chars — else truncate from the head, keeping the tail); rate-limits via KV — compute `bucket = dailyBucket(ip, day, env.HASH_SALT)` exactly as `recordEvent` does, read counter key `fb/<day>/<bucket>` from `env.HISTORY`, return 429 if ≥ 5, else increment with `expirationTtl: 172800` (two days; KV is eventually consistent, so this is a soft limit — acceptable by decision above); then creates the issue with `POST https://api.github.com/repos/tia-tools/feedback/issues`, header `Authorization: Bearer ${env.FEEDBACK_TOKEN}` plus the existing `ghHeaders`-style User-Agent, JSON `{title, body, labels: [category]}`. Title is `[バグ] / [要望] / [その他]` + the message's first line clipped to 50 characters. Body is: a `**Version:** vX.Y.Z` line, the message, then — if a log was sent — a `<details><summary>session.log (<log_name>, tail)</summary>` block containing the log inside a four-backtick `text` fence (four so that any stray triple-backtick inside a log line cannot close it). Return 200 `{"ok":true}` on GitHub 201; 502 on other GitHub responses; 400 on validation failure. Do not call `recordEvent` for feedback (decided). Deploy and verify with curl.

Milestone 2 is the Rust client module, fully unit-testable without network: create `src/feedback/mod.rs` (registered in `src/main.rs` as `pub mod feedback;` alongside the existing modules). It contains: `pub const FEEDBACK_URL: &str = "https://rehearsal-automation.tia.run/feedback";` (doc comment citing docs/adr/0014 and this plan); `pub enum FeedbackCategory { Bug, Request, Other }` with `as_str()` → `"bug"/"request"/"other"` and `label_ja()` → `"バグ"/"要望"/"その他"`; `pub struct SessionLogEntry { pub name: String, pub path: PathBuf }` and `pub fn list_session_logs(output_dir: &Path, max: usize) -> Vec<SessionLogEntry>` (directories containing `session.log`, sorted by folder name descending, truncated to `max` = 10); `pub fn log_tail(text: &str, max_bytes: usize) -> &str` (whole string if it fits, else the largest tail ≤ `max_bytes` starting on a `char` boundary); and `pub fn send_feedback(category: FeedbackCategory, message: &str, log: Option<(&str, &str)>) -> Result<(), String>` which builds the JSON body with `serde_json::json!` (`category`, `message`, `version: env!("CARGO_PKG_VERSION")`, and `log_name`/`log` when present), POSTs it with a `reqwest::blocking::Client` configured like `src/update/mod.rs::client()` (5 s connect, 10 s total — bump total to 30 s here, log payloads are bigger), and maps outcomes to user-facing Japanese error strings: non-200 status 429 → 「送信回数の上限に達しました。明日もう一度お試しください」, other non-200 → 「送信に失敗しました (HTTP <code>)」, transport error → 「送信に失敗しました。ネットワーク接続を確認してください」. Unit tests (run with `GAKUMAS_NO_MANIFEST=1 cargo test feedback`) cover `log_tail` boundary behavior (exact fit, mid-multibyte-char cut, empty), `list_session_logs` ordering/filtering against a tempdir fixture, and category string mappings.

Milestone 3 is the GUI: in `src/gui/state.rs` add a `FeedbackUiState` struct (fields: `open: bool`, `category: FeedbackCategory`, `message: String`, `sessions: Vec<SessionLogEntry>`, `selected_log: Option<usize>` — `None` meaning 添付しない, `sending: bool`, `error: Option<String>`, `sent_toast: Option<Instant>`) and a `feedback: FeedbackUiState` field on `GuiState`. In `src/gui/mod.rs`: add a right-aligned small button 「フィードバック」 to the header panel closure (collect a `feedback_clicked` flag like the existing `install_clicked` flags; on click, populate `sessions` via `feedback::list_session_logs(&paths::get_output_dir(), 10)`, preselect index 0 when non-empty and category is Bug, set `open = true`). Render the form as an `egui::Window::new("フィードバック")` when `open`: a category `ComboBox` (バグ/要望/その他); when バグ, a second `ComboBox` listing 「添付しない」 plus each session name; a multiline `TextEdit` for the message with a hint text; a small weak label stating exactly what will be sent (「送信内容: メッセージ、カテゴリ、アプリのバージョン」 plus 「、選択したセッションログ(末尾60KBまで)」 when a log is selected); and 送信/キャンセル buttons. 送信 is disabled while `sending` or when the trimmed message is empty or over 4,000 chars (show a live counter). On click, set `sending = true` and spawn a thread that reads the selected `session.log` (`std::fs::read_to_string`; on read failure send without the log but append a note to the message? No — fail the send with 「ログの読み込みに失敗しました」 so the user can pick 添付しない consciously), applies `log_tail(…, 60_000)`, calls `send_feedback`, and reports back over a dedicated `mpsc::channel` (`feedback_tx`/`feedback_rx` fields on `GuiApp`, drained in a `poll_feedback_messages()` called next to `poll_update_messages()`; call `ctx.request_repaint()` from the thread like the update check does). On `Ok`: clear `message`, set `open = false`, set `sent_toast = Some(Instant::now())` — the header shows a fading 「✅ 送信しました」 label for ~3 s (mirror the toast style in `src/gui/copyable.rs`). On `Err(msg)`: set `error = Some(msg)`, `sending = false`, keep the window open and the text intact. Note the repo-documented egui borrow gotcha: collect actions as flags inside the closure and dispatch after it, exactly like the header's update buttons.

Finally, add this plan to CLAUDE.md's Active ExecPlans list when implementation starts, and follow the close-out ritual (`/close-out`) after acceptance.

## Concrete Steps

Manual setup (developer, once, before M1 completes):

1. As the `tia-tools` org owner, create a PRIVATE repo `tia-tools/feedback` (no code needed; add a one-line README saying what it is and pointing at this plan's path in the private automation repo). Create issue labels `bug`, `request`, `other`.
2. Ensure `tia-tools-bot` has write access to `tia-tools/feedback` (org member with repo write, or direct collaborator).
3. Logged in as `tia-tools-bot`: Settings → Developer settings → Fine-grained personal access tokens → new token. Resource owner: `tia-tools` (if the org blocks fine-grained PATs, enable them in org settings → Third-party access → Personal access tokens). Repository access: only `tia-tools/feedback`. Permissions: Issues = Read and write (Metadata read is added automatically). Expiry: 1 year — record the expiry date below in Artifacts and Notes; a Worker-side 401 on /feedback means renew it.
4. From `infra/worker/`: `npx wrangler secret put FEEDBACK_TOKEN` and paste the PAT.

M1 (Worker):

    cd infra/worker
    # edit worker.js as described in Plan of Work
    npx wrangler deploy

    # acceptance probe (PowerShell; use --% or a bash shell for the quoting):
    curl -s -X POST https://rehearsal-automation.tia.run/feedback \
      -H "content-type: application/json" \
      -d '{"category":"other","message":"test from curl (M1 acceptance)","version":"0.0.0"}'
    # expect: {"ok":true}  and a new issue in tia-tools/feedback labeled "other"

    # validation failures:
    curl -s -X POST .../feedback -d '{"category":"other","message":""}'      # expect HTTP 400
    curl -s -X POST .../feedback -d 'not json'                               # expect HTTP 400
    # rate limit: repeat the valid POST 6 times; the 6th returns HTTP 429

M2 (Rust module):

    # from repo root
    GAKUMAS_NO_MANIFEST=1 cargo test feedback
    # expect the new unit tests to pass (log_tail boundaries, session listing, category mapping)

M3 (GUI, guarded build then live click-through):

    powershell -ExecutionPolicy Bypass -File scripts/build.ps1
    .\target\release\gakumas-rehearsal-automation.exe

Then perform the acceptance in Validation and Acceptance. Commit per repo discipline: small commits per milestone (`feat(worker): /feedback endpoint`, `feat(feedback): client module`, `feat(gui): feedback form window`), never adding unrelated untracked files, no Claude attribution.

## Validation and Acceptance

M1 acceptance is the curl transcript above: a valid POST returns `{"ok":true}` and a correctly titled, labeled, bodied issue appears in `tia-tools/feedback`; an empty message returns 400; the sixth submission in a day from one IP returns 429; a POST with a 70,000-character `log` produces an issue whose body is under 65,536 characters with the log's TAIL preserved.

M2 acceptance: `GAKUMAS_NO_MANIFEST=1 cargo test feedback` passes; `log_tail` tests prove a cut never lands mid-multibyte character (feed it a string of `あ` and assert the result is valid UTF-8 of length ≤ max and a suffix of the input).

M3 (overall) acceptance, performed live by the user: launch the app → header shows 「フィードバック」 in every state (idle, and during a run) → open the form, pick バグ → the log dropdown appears listing recent sessions with the newest preselected and 「添付しない」 available → type a message, press 送信 → spinner, then the window closes and 「✅ 送信しました」 fades in the header → the issue is in `tia-tools/feedback` with label `bug`, `**Version:**` line matching the running build, the message, and the collapsed log block whose content matches the selected session's `session.log` tail. Then: disconnect the network, submit again → inline red error, the typed text still present; reconnect and press 送信 again → succeeds. Finally submit with 「添付しない」 → issue has no details block.

## Idempotence and Recovery

Everything here is additive and re-runnable. `wrangler deploy` is idempotent; re-running `wrangler secret put FEEDBACK_TOKEN` overwrites the secret harmlessly. If the PAT leaks or expires, revoke/renew it in the bot account and re-put the secret — nothing in git changes (that isolation is the point; docs/adr/0015). If the Worker change misbehaves in production, redeploy the previous `worker.js` from git (`git checkout HEAD~1 -- infra/worker/worker.js && npx wrangler deploy`) — the update/download routes are untouched by rollback. The Rust changes are ordinary code changes guarded by unit tests; no data or config migrations. The rate-limit KV keys expire on their own TTL.

## Artifacts and Notes

- `FEEDBACK_TOKEN` PAT: created 2026-07-12 by the user as `tia-tools-bot`, token name `2026-07-13_366days_gakumas_feedback`, issues-only on repo `tia-tools/feedback`, EXPIRES 2027-07-13. A Worker-side 502 on previously-working /feedback around then means the PAT lapsed → renew it in the bot account and `npx wrangler secret put FEEDBACK_TOKEN` in `infra/worker/`.
- M1 acceptance transcript (2026-07-12, via node probe scripts — curl was denied by session permissions):

      plain:   HTTP 200 {"ok":true}   -> issue #1 [その他], label other, author tia-tools-bot
      big-log: HTTP 200 {"ok":true}   -> issue #2 [バグ], label bug, body_len 65000,
               TAIL-MARKER-MUST-SURVIVE present, HEAD-MARKER-SHOULD-BE-CUT absent
      (rejection paths, earlier same setup: 405 GET / 400 ×6 / 413 oversized all PASS)
- Request JSON shape (client → Worker), all strings, `log_name`/`log` optional and only present together:

      {"category":"bug","message":"…","version":"0.9.1","log_name":"20260704_052007","log":"[05:20:07.123] …"}

- Issue body shape (Worker → GitHub):

      **Version:** v0.9.1

      <user message verbatim>

      <details><summary>session.log (20260704_052007, tail)</summary>

      ````text
      [05:20:07.123] …
      ````

      </details>

## Interfaces and Dependencies

No new crate dependencies: `reqwest` (blocking + rustls) and `serde_json` are already in `Cargo.toml` for the updater. In `src/feedback/mod.rs` define:

    pub const FEEDBACK_URL: &str = "https://rehearsal-automation.tia.run/feedback";
    pub const LOG_TAIL_MAX: usize = 60_000;   // bytes of session.log tail sent
    pub const MESSAGE_MAX: usize = 4_000;     // chars of user message accepted

    pub enum FeedbackCategory { Bug, Request, Other }        // as_str() -> "bug"/"request"/"other"; label_ja()
    pub struct SessionLogEntry { pub name: String, pub path: std::path::PathBuf }

    pub fn list_session_logs(output_dir: &std::path::Path, max: usize) -> Vec<SessionLogEntry>;
    pub fn log_tail(text: &str, max_bytes: usize) -> &str;
    pub fn send_feedback(
        category: FeedbackCategory,
        message: &str,
        log: Option<(&str, &str)>,            // (session folder name, already-truncated log text)
    ) -> Result<(), String>;                  // Err is the user-facing Japanese message

In `src/gui/state.rs` define `FeedbackUiState` as described in Plan of Work, owned by `GuiState` as `pub feedback: FeedbackUiState`. In `infra/worker/worker.js` the new env bindings are `env.FEEDBACK_TOKEN` (wrangler secret) plus the existing `env.HISTORY` (KV) and `env.HASH_SALT` (secret); no `wrangler.toml` change is needed unless the KV binding is missing in a fresh environment.

---

Revision note (2026-07-11): initial full draft, written at the end of the /grill-me design interview that produced the Decision Log above; no implementation has begun.
