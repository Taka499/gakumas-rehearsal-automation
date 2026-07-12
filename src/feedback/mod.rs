//! In-app feedback: build and send a feedback payload to the tia.run Worker,
//! which turns it into an issue in the private `tia-tools/feedback` repo
//! (design: `docs/EXECPLAN_FEEDBACK_FORM.md`; credential posture:
//! `docs/adr/0015`). Nothing is ever sent silently: the payload is exactly
//! the user's typed message, their chosen category, the app version, and —
//! only when they explicitly selected one — a session log's tail.
//!
//! `send_feedback` does blocking network I/O; call it from a spawned thread
//! only, like `crate::update::check_for_update`.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// Feedback ingestion endpoint on the per-app subdomain (docs/adr/0014).
/// Unlike the updater URLs in `src/update/endpoints.rs` this is not
/// security-load-bearing: nothing is downloaded or executed in response.
pub const FEEDBACK_URL: &str = "https://rehearsal-automation.tia.run/feedback";

/// Most bytes of `session.log` sent, taken from the file's TAIL (crashes and
/// aborts live there). The Worker re-truncates defensively: GitHub caps an
/// issue body at 65,536 characters.
pub const LOG_TAIL_MAX: usize = 60_000;

/// Longest accepted user message, in characters. Must match the Worker's
/// `FEEDBACK_MESSAGE_MAX` (`infra/worker/worker.js`) or valid form input
/// would be rejected server-side.
pub const MESSAGE_MAX: usize = 4_000;

/// How many recent sessions the bug-report log picker offers.
pub const SESSION_PICKER_MAX: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackCategory {
    Bug,
    Request,
    Other,
}

impl FeedbackCategory {
    pub const ALL: [FeedbackCategory; 3] = [
        FeedbackCategory::Bug,
        FeedbackCategory::Request,
        FeedbackCategory::Other,
    ];

    /// Wire value; the Worker validates against exactly these strings.
    pub fn as_str(self) -> &'static str {
        match self {
            FeedbackCategory::Bug => "bug",
            FeedbackCategory::Request => "request",
            FeedbackCategory::Other => "other",
        }
    }

    /// Label shown in the form's category picker.
    pub fn label_ja(self) -> &'static str {
        match self {
            FeedbackCategory::Bug => "バグ",
            FeedbackCategory::Request => "要望",
            FeedbackCategory::Other => "その他",
        }
    }
}

/// One session offered by the bug-report log picker: the session folder's
/// name (e.g. `20260704_052007`) and the full path of its `session.log`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionLogEntry {
    pub name: String,
    pub path: PathBuf,
}

/// The newest (at most `max`) sessions under `output_dir` that have a
/// `session.log`. Folder names are `YYYYMMDD_HHMMSS`, so descending name
/// order is descending chronological order. Unreadable dirs yield an empty
/// list — the picker then only offers 「添付しない」.
pub fn list_session_logs(output_dir: &Path, max: usize) -> Vec<SessionLogEntry> {
    let mut entries: Vec<SessionLogEntry> = match std::fs::read_dir(output_dir) {
        Ok(rd) => rd
            .flatten()
            .filter_map(|e| {
                let log_path = e.path().join("session.log");
                log_path.is_file().then(|| SessionLogEntry {
                    name: e.file_name().to_string_lossy().into_owned(),
                    path: log_path,
                })
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    entries.sort_by(|a, b| b.name.cmp(&a.name));
    entries.truncate(max);
    entries
}

/// The largest suffix of `text` that fits in `max_bytes` and starts on a
/// character boundary (never cuts a multibyte character in half).
pub fn log_tail(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut start = text.len() - max_bytes;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    &text[start..]
}

/// Sends one feedback submission. Blocking; spawn a thread. `log` is
/// `(session folder name, already-truncated log text)` — pass the result of
/// `log_tail(..., LOG_TAIL_MAX)`, not a raw file. `Err` is the user-facing
/// Japanese message shown verbatim in the form.
pub fn send_feedback(
    category: FeedbackCategory,
    message: &str,
    log: Option<(&str, &str)>,
) -> Result<(), String> {
    let mut payload = serde_json::json!({
        "category": category.as_str(),
        "message": message,
        "version": env!("CARGO_PKG_VERSION"),
    });
    if let Some((name, text)) = log {
        payload["log_name"] = name.into();
        payload["log"] = text.into();
    }

    const NET_ERR: &str = "送信に失敗しました。ネットワーク接続を確認してください";
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(30))
        .user_agent(concat!(
            "gakumas-rehearsal-automation/",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
        .map_err(|_| NET_ERR.to_string())?;
    let resp = client
        .post(FEEDBACK_URL)
        .header("content-type", "application/json")
        .body(payload.to_string())
        .send()
        .map_err(|e| {
            crate::log(&format!("[feedback] send failed: {e}"));
            NET_ERR.to_string()
        })?;

    let code = resp.status().as_u16();
    crate::log(&format!("[feedback] sent ({}): HTTP {code}", category.as_str()));
    match code {
        200 => Ok(()),
        429 => Err("送信回数の上限に達しました。明日もう一度お試しください".to_string()),
        _ => Err(format!("送信に失敗しました (HTTP {code})")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_tail_returns_whole_string_when_it_fits() {
        assert_eq!(log_tail("abc", 3), "abc");
        assert_eq!(log_tail("abc", 100), "abc");
        assert_eq!(log_tail("", 10), "");
    }

    #[test]
    fn log_tail_takes_the_tail_not_the_head() {
        assert_eq!(log_tail("0123456789", 4), "6789");
    }

    #[test]
    fn log_tail_never_cuts_a_multibyte_char() {
        // "あ" is 3 bytes; a 7-byte budget over "あああ" (9 bytes) must yield
        // 2 whole chars (6 bytes), not a torn one.
        let s = "あああ";
        let tail = log_tail(s, 7);
        assert_eq!(tail, "ああ");
        assert!(s.ends_with(tail));
        // Budget 0 on multibyte input is an empty (still valid) suffix.
        assert_eq!(log_tail(s, 0), "");
    }

    #[test]
    fn log_tail_result_is_always_a_suffix_within_budget() {
        let s = "x".repeat(10) + &"あ".repeat(10);
        for max in 0..=s.len() {
            let tail = log_tail(&s, max);
            assert!(tail.len() <= max);
            assert!(s.ends_with(tail));
        }
    }

    #[test]
    fn category_wire_values_match_the_worker() {
        assert_eq!(FeedbackCategory::Bug.as_str(), "bug");
        assert_eq!(FeedbackCategory::Request.as_str(), "request");
        assert_eq!(FeedbackCategory::Other.as_str(), "other");
        assert_eq!(FeedbackCategory::Bug.label_ja(), "バグ");
        assert_eq!(FeedbackCategory::Request.label_ja(), "要望");
        assert_eq!(FeedbackCategory::Other.label_ja(), "その他");
    }

    #[test]
    fn list_session_logs_filters_sorts_and_truncates() {
        let dir = tempfile::tempdir().unwrap();
        let mk = |name: &str, with_log: bool| {
            let d = dir.path().join(name);
            std::fs::create_dir(&d).unwrap();
            if with_log {
                std::fs::write(d.join("session.log"), "x").unwrap();
            }
        };
        mk("20260701_120000", true);
        mk("20260703_090000", true);
        mk("20260702_100000", false); // no session.log -> excluded
        std::fs::write(dir.path().join("stray.txt"), "x").unwrap(); // file -> excluded

        let all = list_session_logs(dir.path(), 10);
        assert_eq!(
            all.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            vec!["20260703_090000", "20260701_120000"] // newest first
        );
        assert!(all[0].path.ends_with("session.log"));

        let capped = list_session_logs(dir.path(), 1);
        assert_eq!(capped.len(), 1);
        assert_eq!(capped[0].name, "20260703_090000");
    }

    #[test]
    fn list_session_logs_missing_dir_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let gone = dir.path().join("nope");
        assert!(list_session_logs(&gone, 10).is_empty());
    }
}
