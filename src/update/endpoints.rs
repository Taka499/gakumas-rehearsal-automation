//! The only two update-channel URLs in the codebase (per docs/adr/0011).
//!
//! Order matters: the tia.run manifest is authoritative; the dist repo's
//! GitHub API is the fallback so a lapsed domain degrades gracefully
//! instead of bricking update checks in shipped binaries.

pub const MANIFEST_URL: &str = "https://tia.run/latest.json";
pub const FALLBACK_API_URL: &str =
    "https://api.github.com/repos/tia-tools/releases/releases/latest";
