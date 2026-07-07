//! Auto-update core: check the distribution channel for a newer release.
//!
//! Per `docs/adr/0011` the check goes domain-first (`https://tia.run/latest.json`,
//! a stateless Cloudflare Worker reshaping the dist repo's latest release) with
//! the dist repo's GitHub Releases API as fallback. Every failure path returns
//! `None` — an update check must never surface an error, block startup, or
//! alarm the user. Call `check_for_update` from a spawned thread only; it does
//! blocking network I/O with short timeouts.

mod endpoints;
pub mod install;

use std::time::Duration;

use endpoints::{FALLBACK_API_URL, MANIFEST_URL};

/// A downloadable newer release, as described by the update channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateInfo {
    /// Version without leading `v`, e.g. `"0.9.0"`.
    pub version: String,
    /// First paragraph of the release notes (may be empty).
    pub notes: String,
    /// Download URL of the release zip.
    pub url: String,
    /// Lowercase hex SHA-256 of the zip; installation verifies against this.
    pub sha256: String,
}

/// Returns a strictly newer release than the running binary, or `None`
/// (no newer version, or the channel was unreachable — indistinguishable
/// by design; failures are silent).
pub fn check_for_update() -> Option<UpdateInfo> {
    let info = fetch_update_info()?;
    if is_newer(&info.version, env!("CARGO_PKG_VERSION")) {
        Some(info)
    } else {
        crate::log(&format!(
            "[update] up to date (running {}, channel has {})",
            env!("CARGO_PKG_VERSION"),
            info.version
        ));
        None
    }
}

/// `true` iff `candidate` is a well-formed `X.Y.Z` strictly greater than
/// `current`. Malformed input on either side is never "newer".
pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(c), Some(cur)) => c > cur,
        _ => false,
    }
}

fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let mut parts = s.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

fn fetch_update_info() -> Option<UpdateInfo> {
    let client = client()?;
    if let Some(info) = get_text(&client, MANIFEST_URL).and_then(|t| parse_manifest(&t)) {
        crate::log(&format!("[update] manifest: channel has v{}", info.version));
        return Some(info);
    }
    crate::log("[update] manifest unavailable; trying GitHub fallback");
    let info = fetch_github_fallback(&client)?;
    crate::log(&format!("[update] fallback: channel has v{}", info.version));
    Some(info)
}

fn client() -> Option<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .user_agent(concat!(
            "gakumas-rehearsal-automation/",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
        .ok()
}

fn get_text(client: &reqwest::blocking::Client, url: &str) -> Option<String> {
    let resp = client.get(url).send().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.text().ok()
}

fn fetch_github_fallback(client: &reqwest::blocking::Client) -> Option<UpdateInfo> {
    let text = get_text(client, FALLBACK_API_URL)?;
    let (mut info, sidecar_url) = parse_github_release(&text)?;
    if !is_valid_sha256(&info.sha256) {
        // GitHub didn't report an asset digest; read the .sha256 sidecar
        // asset that /release uploads next to the zip.
        let body = get_text(client, &sidecar_url?)?;
        info.sha256 = body.trim().split_whitespace().next()?.to_lowercase();
    }
    validated(info)
}

/// Parses the tia.run `latest.json` manifest. Returns `None` unless every
/// field needed for a verifiable install is present and well-formed.
fn parse_manifest(text: &str) -> Option<UpdateInfo> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    validated(UpdateInfo {
        version: v["version"].as_str()?.trim_start_matches('v').to_string(),
        notes: v["notes"].as_str().unwrap_or("").to_string(),
        url: v["url"].as_str()?.to_string(),
        sha256: v["sha256"].as_str().unwrap_or("").to_lowercase(),
    })
}

/// Parses a GitHub `releases/latest` API response into an `UpdateInfo`
/// (sha256 may still be empty if GitHub reported no asset digest) plus the
/// download URL of the `.sha256` sidecar asset, if one was uploaded.
fn parse_github_release(text: &str) -> Option<(UpdateInfo, Option<String>)> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let version = v["tag_name"].as_str()?.trim_start_matches('v').to_string();
    let assets = v["assets"].as_array()?;

    let zip = assets
        .iter()
        .find(|a| a["name"].as_str().is_some_and(|n| n.ends_with(".zip")))?;
    let zip_name = zip["name"].as_str()?;
    let url = zip["browser_download_url"].as_str()?.to_string();

    let sha256 = zip["digest"]
        .as_str()
        .and_then(|d| d.strip_prefix("sha256:"))
        .unwrap_or("")
        .to_lowercase();

    let sidecar_name = format!("{zip_name}.sha256");
    let sidecar_url = assets
        .iter()
        .find(|a| a["name"].as_str() == Some(sidecar_name.as_str()))
        .and_then(|a| a["browser_download_url"].as_str())
        .map(String::from);

    let notes = first_paragraph(v["body"].as_str().unwrap_or(""));
    Some((UpdateInfo { version, notes, url, sha256 }, sidecar_url))
}

fn first_paragraph(body: &str) -> String {
    body.replace("\r\n", "\n")
        .split("\n\n")
        .next()
        .unwrap_or("")
        .trim()
        .to_string()
}

fn is_valid_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn validated(info: UpdateInfo) -> Option<UpdateInfo> {
    (parse_version(&info.version).is_some()
        && info.url.starts_with("https://")
        && is_valid_sha256(&info.sha256))
    .then_some(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

    #[test]
    fn is_newer_basic_bumps() {
        assert!(is_newer("0.9.0", "0.8.0"));
        assert!(is_newer("0.8.1", "0.8.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("0.10.0", "0.9.0")); // numeric, not lexicographic
    }

    #[test]
    fn is_newer_equal_and_older() {
        assert!(!is_newer("0.8.0", "0.8.0"));
        assert!(!is_newer("0.7.9", "0.8.0"));
        assert!(!is_newer("0.8.0", "1.0.0"));
    }

    #[test]
    fn is_newer_rejects_malformed() {
        assert!(!is_newer("v0.9.0", "0.8.0")); // leading v must be stripped upstream
        assert!(!is_newer("0.9", "0.8.0"));
        assert!(!is_newer("0.9.0.1", "0.8.0"));
        assert!(!is_newer("abc", "0.8.0"));
        assert!(!is_newer("0.9.0", "garbage"));
        assert!(!is_newer("", ""));
    }

    #[test]
    fn parse_manifest_happy_path() {
        let json = format!(
            r#"{{
              "version": "0.9.0",
              "notes": "box plot copy + review fixes",
              "url": "https://tia.run/download/gakumas-rehearsal-automation-v0.9.0.zip",
              "sha256": "{SHA}"
            }}"#
        );
        let info = parse_manifest(&json).expect("valid manifest");
        assert_eq!(info.version, "0.9.0");
        assert_eq!(info.notes, "box plot copy + review fixes");
        assert!(info.url.ends_with("v0.9.0.zip"));
        assert_eq!(info.sha256, SHA);
    }

    #[test]
    fn parse_manifest_tolerates_leading_v_and_uppercase_sha() {
        let json = format!(
            r#"{{"version":"v0.9.1","notes":"","url":"https://tia.run/download/a.zip","sha256":"{}"}}"#,
            SHA.to_uppercase()
        );
        let info = parse_manifest(&json).expect("valid manifest");
        assert_eq!(info.version, "0.9.1");
        assert_eq!(info.sha256, SHA);
    }

    #[test]
    fn parse_manifest_rejects_missing_or_bad_fields() {
        assert!(parse_manifest("not json").is_none());
        assert!(parse_manifest(r#"{"version":"0.9.0"}"#).is_none()); // no url/sha
        let bad_sha =
            r#"{"version":"0.9.0","url":"https://tia.run/download/a.zip","sha256":"abc"}"#;
        assert!(parse_manifest(bad_sha).is_none());
        let http_url = format!(
            r#"{{"version":"0.9.0","url":"http://tia.run/download/a.zip","sha256":"{SHA}"}}"#
        );
        assert!(parse_manifest(&http_url).is_none()); // https only
    }

    fn github_fixture(digest: &str, with_sidecar: bool) -> String {
        let digest_field = if digest.is_empty() {
            String::new()
        } else {
            format!(r#""digest": "sha256:{digest}","#)
        };
        let sidecar = if with_sidecar {
            r#",{"name": "gakumas-rehearsal-automation-v0.9.0.zip.sha256",
                "browser_download_url": "https://github.com/tia-tools/releases/releases/download/v0.9.0/gakumas-rehearsal-automation-v0.9.0.zip.sha256"}"#
        } else {
            ""
        };
        // r### delimiter: the body value contains `"#` (a quote followed by a
        // markdown heading), which would terminate an r#-delimited raw string.
        format!(
            r###"{{
              "tag_name": "v0.9.0",
              "body": "## New Features\r\n### one-click updates\r\n\r\n## Install\r\nDownload and extract.",
              "assets": [
                {{"name": "gakumas-rehearsal-automation-v0.9.0.zip",
                  {digest_field}
                  "browser_download_url": "https://github.com/tia-tools/releases/releases/download/v0.9.0/gakumas-rehearsal-automation-v0.9.0.zip"}}
                {sidecar}
              ]
            }}"###
        )
    }

    #[test]
    fn parse_github_release_with_digest() {
        let (info, sidecar) = parse_github_release(&github_fixture(SHA, true)).expect("parses");
        assert_eq!(info.version, "0.9.0");
        assert_eq!(info.sha256, SHA);
        assert!(info.url.ends_with("v0.9.0.zip"));
        assert_eq!(info.notes, "## New Features\n### one-click updates");
        assert!(sidecar.is_some());
        assert!(validated(info).is_some());
    }

    #[test]
    fn parse_github_release_without_digest_exposes_sidecar() {
        let (info, sidecar) = parse_github_release(&github_fixture("", true)).expect("parses");
        assert_eq!(info.sha256, "");
        assert!(validated(info).is_none()); // not installable until sidecar is read
        assert!(sidecar.expect("sidecar url").ends_with(".zip.sha256"));
    }

    #[test]
    fn parse_github_release_requires_zip_asset() {
        let json = r#"{"tag_name": "v0.9.0", "body": "", "assets": []}"#;
        assert!(parse_github_release(json).is_none());
    }
}
