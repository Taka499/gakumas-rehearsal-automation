//! The only two update-channel URLs in the codebase (per docs/adr/0011).
//!
//! Order matters: the domain manifest is authoritative; the dist repo's
//! GitHub API is the fallback so a lapsed domain degrades gracefully
//! instead of bricking update checks in shipped binaries. The host is the
//! per-app subdomain (the bare tia.run is reserved for a future brand
//! landing page and serves nothing).

pub const MANIFEST_URL: &str = "https://rehearsal-automation.tia.run/latest.json";
pub const FALLBACK_API_URL: &str =
    "https://api.github.com/repos/tia-tools/releases/releases/latest";

/// Minisign (Ed25519) public key for release-signature verification, per
/// docs/EXECPLAN_RELEASE_SIGNING.md. This is the updater's ROOT OF TRUST: the
/// installer refuses any download whose `.minisig` this key does not verify,
/// so a compromised dist repo or Cloudflare account cannot push code the
/// developer did not sign. Baked into every shipped binary — changing it is a
/// breaking trust event (old binaries can only verify signatures from the key
/// they carry). The matching SECRET key lives only on the developer's machine
/// (~/.minisign/gakumas.key), never in git, the dist repo, or Cloudflare.
pub const PUBLIC_KEY: &str = "RWSsK+YIsZpesxIA/bU6J4wwjBJajq9Ky8UGWyBcbOsb+VBkb2aUlw4Q";

/// Hosts the updater will download an update zip from. A rogue manifest cannot
/// redirect the download to an attacker-controlled origin (security review
/// finding #2). The Worker serves the zip from the first; the GitHub API
/// fallback's `browser_download_url` uses the latter two.
pub const ALLOWED_DOWNLOAD_HOSTS: &[&str] = &[
    "rehearsal-automation.tia.run",
    "github.com",
    "objects.githubusercontent.com",
];
