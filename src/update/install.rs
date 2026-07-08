//! Update installation: download, verify, and swap in the new version.
//!
//! Everything is staged beside the live files (`<exe>.new`, `resources.new`),
//! verified, and then swapped via renames — Windows allows *renaming* a running
//! exe, just not deleting or overwriting it, so the running binary moves to
//! `<exe>.old` and the staged one takes its name. `config.json`,
//! `gui_settings.json`, and every other root file that already exists locally
//! are NEVER overwritten (user calibration must survive updates); only the exe
//! and `resources/` are replaced. `.old` leftovers are removed on the next
//! launch by `cleanup_old_files`.

use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use minisign_verify::{PublicKey, Signature};
use sha2::{Digest, Sha256};

use super::endpoints::PUBLIC_KEY;
use super::UpdateInfo;

/// Top-level folder inside the release zip, fixed by `scripts/package-release.ps1`
/// (every release archive has `gakumas-rehearsal-automation/` as its root).
const ZIP_ROOT: &str = "gakumas-rehearsal-automation";

/// What `stage_from_zip` laid down next to the live files.
struct Staged {
    exe: bool,
    resources: bool,
}

/// Downloads `info.url`, verifies its SHA-256 against `info.sha256`, stages the
/// contents, and swaps them in. On success the process keeps running as the OLD
/// version — the caller shows a "restart" affordance; the new exe takes over on
/// the next launch. Call from a worker thread only (blocking network I/O).
pub fn download_and_install(info: &UpdateInfo) -> Result<()> {
    let exe_path = std::env::current_exe().context("current exe path")?;
    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| anyhow!("exe has no parent directory"))?
        .to_path_buf();

    crate::log(&format!("[update] downloading {}", info.url));
    let mut zip_file = download_to_temp(&info.url, &exe_dir)?;

    // Authenticity FIRST: verify the release signature with the embedded public
    // key before anything else. This is the trust anchor — a compromised dist
    // repo or Cloudflare account can serve any bytes + matching hash, but cannot
    // forge a signature for this key. Reject before hashing/extracting/swapping.
    let sig_text = download_signature(&info.sig_url)?;
    let zip_bytes = read_all(zip_file.as_file_mut()).context("reading download")?;
    verify_signature(&zip_bytes, &sig_text).context("署名を確認できません")?;
    crate::log("[update] signature verified");

    // Integrity: the hash is now defence-in-depth (corruption / truncation),
    // with a distinct error message from the signature failure above.
    let actual = sha256_hex(zip_file.as_file_mut()).context("hashing download")?;
    if !actual.eq_ignore_ascii_case(&info.sha256) {
        bail!(
            "チェックサム不一致 (expected {}, got {})",
            &info.sha256[..12.min(info.sha256.len())],
            &actual[..12]
        );
    }
    crate::log("[update] sha256 verified");

    let new_exe_path = sibling(&exe_path, "new");
    let staged = stage_from_zip(zip_file.as_file_mut(), &exe_dir, &new_exe_path)
        .context("extracting update")?;
    if !staged.exe {
        bail!("アーカイブに実行ファイルが見つかりません");
    }

    if staged.resources {
        swap_resources(&exe_dir).context("swapping resources/")?;
    }
    swap_exe(&exe_path, &new_exe_path).context("swapping exe")?;

    crate::log(&format!("[update] v{} installed; restart to apply", info.version));
    Ok(())
}

/// Removes leftovers from a previous self-update (`<exe>.old`, `resources.old`).
/// Best-effort: called early at startup, failures are only logged.
pub fn cleanup_old_files() {
    let Ok(exe_path) = std::env::current_exe() else { return };
    let old_exe = sibling(&exe_path, "old");
    if old_exe.exists() && fs::remove_file(&old_exe).is_ok() {
        crate::log("[update] removed leftover exe from previous update");
    }
    if let Some(dir) = exe_path.parent() {
        let old_res = dir.join("resources.old");
        if old_res.exists() && fs::remove_dir_all(&old_res).is_ok() {
            crate::log("[update] removed leftover resources.old");
        }
    }
}

/// `path` with `.suffix` appended to its file name (`a.exe` → `a.exe.old`).
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!("{name}.{suffix}"))
}

/// Downloads `url` into a temp file created in `dir` — the exe's own directory,
/// so all later renames stay on one volume (cross-volume renames are copies and
/// lose atomicity). Long total timeout: this is a multi-megabyte zip.
fn download_to_temp(url: &str, dir: &Path) -> Result<tempfile::NamedTempFile> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(600))
        .user_agent(concat!(
            "gakumas-rehearsal-automation/",
            env!("CARGO_PKG_VERSION")
        ))
        .build()?;
    let mut resp = client.get(url).send()?.error_for_status()?;
    let mut file = tempfile::NamedTempFile::new_in(dir).context("temp file for download")?;
    resp.copy_to(&mut file.as_file_mut())?;
    file.as_file_mut().flush()?;
    Ok(file)
}

/// Fetches the `.minisig` signature text. Small file, short timeout. A missing
/// or unreachable signature is a hard failure: no signature, no install.
fn download_signature(sig_url: &str) -> Result<String> {
    if sig_url.is_empty() {
        bail!("署名ファイルがありません (no signature URL in manifest)");
    }
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .user_agent(concat!(
            "gakumas-rehearsal-automation/",
            env!("CARGO_PKG_VERSION")
        ))
        .build()?;
    let text = client.get(sig_url).send()?.error_for_status()?.text()?;
    Ok(text)
}

/// Reads the reader's full contents into memory (rewinds first). The release
/// zip is tens of MB — acceptable to hold in RAM for signature verification.
fn read_all<R: Read + Seek>(reader: &mut R) -> Result<Vec<u8>> {
    reader.seek(SeekFrom::Start(0))?;
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf)?;
    Ok(buf)
}

/// Verifies `zip_bytes` against `sig_text` (the `.minisig` file contents) using
/// the embedded [`PUBLIC_KEY`]. Returns an error if the key or signature won't
/// parse, or if the signature does not match — the caller must treat any error
/// as "do not install". `allow_legacy = true` accepts both prehashed and legacy
/// minisign signature formats, so it works regardless of how the release was
/// signed.
fn verify_signature(zip_bytes: &[u8], sig_text: &str) -> Result<()> {
    let public_key =
        PublicKey::from_base64(PUBLIC_KEY).map_err(|e| anyhow!("bad embedded public key: {e}"))?;
    let signature =
        Signature::decode(sig_text).map_err(|e| anyhow!("bad signature file: {e}"))?;
    public_key
        .verify(zip_bytes, &signature, true)
        .map_err(|e| anyhow!("signature mismatch: {e}"))
}

/// Lowercase hex SHA-256 of the reader's full contents (rewinds first).
fn sha256_hex<R: Read + Seek>(reader: &mut R) -> Result<String> {
    reader.seek(SeekFrom::Start(0))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Extracts the release archive into staging locations beside the live files:
/// the root-level `.exe` goes to `new_exe_path`, `resources/**` goes under
/// `<exe_dir>/resources.new/`, and any other root file is written only if it
/// does not already exist locally (new config-like files ship in; existing
/// ones — `config.json`, `gui_settings.json` — are never touched).
fn stage_from_zip<R: Read + Seek>(
    archive: &mut R,
    exe_dir: &Path,
    new_exe_path: &Path,
) -> Result<Staged> {
    archive.seek(SeekFrom::Start(0))?;
    let mut zip = zip::ZipArchive::new(archive).context("opening zip")?;
    let mut staged = Staged { exe: false, resources: false };
    let resources_new = exe_dir.join("resources.new");

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        if entry.is_dir() {
            continue;
        }
        // Zip-slip guard. enclosed_name() is the zip crate's own safety filter,
        // but its exact semantics have shifted across versions, so additionally
        // require every path component to be a plain name (no `..`, no roots).
        let Some(name) = entry.enclosed_name() else {
            bail!("unsafe path in archive: {}", entry.name());
        };
        if name
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            bail!("unsafe path in archive: {}", entry.name());
        }
        let Ok(rel) = name.strip_prefix(ZIP_ROOT).map(Path::to_path_buf) else {
            continue; // stray entry outside the expected root folder
        };

        let is_root_level = rel.components().count() == 1;
        let dest = if is_root_level && rel.extension().is_some_and(|e| e == "exe") {
            staged.exe = true;
            new_exe_path.to_path_buf()
        } else if rel.starts_with("resources") {
            staged.resources = true;
            let under = rel.strip_prefix("resources").expect("checked starts_with");
            resources_new.join(under)
        } else {
            let dest = exe_dir.join(&rel);
            if dest.exists() {
                continue; // never overwrite an existing local file (config etc.)
            }
            dest
        };

        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = fs::File::create(&dest)
            .with_context(|| format!("creating {}", dest.display()))?;
        std::io::copy(&mut entry, &mut out)?;
    }
    Ok(staged)
}

/// Atomically-ish replaces `resources/` with the staged `resources.new/`:
/// old moves to `resources.old`, new takes its place; on failure the old
/// directory is rolled back. `resources.old` is deleted best-effort here and
/// again at next launch (files may still be memory-mapped).
fn swap_resources(exe_dir: &Path) -> Result<()> {
    let res = exe_dir.join("resources");
    let res_new = exe_dir.join("resources.new");
    let res_old = exe_dir.join("resources.old");

    if res_old.exists() {
        let _ = fs::remove_dir_all(&res_old);
    }
    let had_old = res.exists();
    if had_old {
        fs::rename(&res, &res_old).context("resources -> resources.old")?;
    }
    if let Err(e) = fs::rename(&res_new, &res) {
        if had_old {
            let _ = fs::rename(&res_old, &res); // roll back
        }
        return Err(anyhow!(e).context("resources.new -> resources"));
    }
    let _ = fs::remove_dir_all(&res_old);
    Ok(())
}

/// The rename-swap: running exe → `<exe>.old`, staged `<exe>.new` → exe name.
/// Renaming the running exe is legal on Windows; deleting it is not, so the
/// `.old` file stays until the next launch cleans it up.
fn swap_exe(exe_path: &Path, new_exe_path: &Path) -> Result<()> {
    let old = sibling(exe_path, "old");
    if old.exists() {
        let _ = fs::remove_file(&old);
    }
    fs::rename(exe_path, &old).context("exe -> exe.old")?;
    if let Err(e) = fs::rename(new_exe_path, exe_path) {
        let _ = fs::rename(&old, exe_path); // roll back to the running version
        return Err(anyhow!(e).context("exe.new -> exe"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use zip::write::SimpleFileOptions;

    #[test]
    fn sha256_hex_known_vector() {
        let mut data = Cursor::new(b"abc".to_vec());
        assert_eq!(
            sha256_hex(&mut data).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    /// Builds an in-memory release zip with the standard top-level folder.
    fn make_zip(entries: &[(&str, &str)]) -> Cursor<Vec<u8>> {
        let mut cursor = Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(&mut cursor);
        for (name, content) in entries {
            writer
                .start_file(format!("{ZIP_ROOT}/{name}"), SimpleFileOptions::default())
                .unwrap();
            writer.write_all(content.as_bytes()).unwrap();
        }
        writer.finish().unwrap();
        cursor
    }

    #[test]
    fn stage_routes_exe_resources_and_skips_existing_root_files() {
        let dir = tempfile::tempdir().unwrap();
        let exe_dir = dir.path();
        std::fs::write(exe_dir.join("config.json"), "USER CALIBRATION").unwrap();

        let mut zip = make_zip(&[
            ("gakumas-rehearsal-automation.exe", "NEW EXE"),
            ("config.json", "SHIPPED DEFAULTS"),
            ("NEWFILE.txt", "brand new root file"),
            ("resources/template/rehearsal/start.png", "PNG"),
        ]);
        let new_exe = exe_dir.join("gakumas-rehearsal-automation.exe.new");
        let staged = stage_from_zip(&mut zip, exe_dir, &new_exe).unwrap();

        assert!(staged.exe && staged.resources);
        assert_eq!(std::fs::read_to_string(&new_exe).unwrap(), "NEW EXE");
        // Existing root file untouched; genuinely new root file shipped in.
        assert_eq!(
            std::fs::read_to_string(exe_dir.join("config.json")).unwrap(),
            "USER CALIBRATION"
        );
        assert_eq!(
            std::fs::read_to_string(exe_dir.join("NEWFILE.txt")).unwrap(),
            "brand new root file"
        );
        assert_eq!(
            std::fs::read_to_string(
                exe_dir.join("resources.new/template/rehearsal/start.png")
            )
            .unwrap(),
            "PNG"
        );
        // Nothing extracted over the live resources dir.
        assert!(!exe_dir.join("resources").exists());
    }

    #[test]
    fn stage_rejects_zip_slip() {
        let mut cursor = Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(&mut cursor);
        writer
            .start_file(
                format!("{ZIP_ROOT}/../evil.txt"),
                SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(b"evil").unwrap();
        writer.finish().unwrap();

        // Nest the exe dir one level down so the escape check inspects a
        // directory owned by this test, not the shared system temp dir.
        let dir = tempfile::tempdir().unwrap();
        let exe_dir = dir.path().join("app");
        std::fs::create_dir(&exe_dir).unwrap();
        let new_exe = exe_dir.join("x.exe.new");
        assert!(stage_from_zip(&mut cursor, &exe_dir, &new_exe).is_err());
        assert!(!dir.path().join("evil.txt").exists());
    }

    #[test]
    fn swap_resources_replaces_and_cleans() {
        let dir = tempfile::tempdir().unwrap();
        let exe_dir = dir.path();
        std::fs::create_dir_all(exe_dir.join("resources/template")).unwrap();
        std::fs::write(exe_dir.join("resources/template/a.png"), "OLD").unwrap();
        std::fs::create_dir_all(exe_dir.join("resources.new/template")).unwrap();
        std::fs::write(exe_dir.join("resources.new/template/a.png"), "NEW").unwrap();

        swap_resources(exe_dir).unwrap();
        assert_eq!(
            std::fs::read_to_string(exe_dir.join("resources/template/a.png")).unwrap(),
            "NEW"
        );
        assert!(!exe_dir.join("resources.new").exists());
        assert!(!exe_dir.join("resources.old").exists());
    }

    #[test]
    fn swap_exe_renames_and_keeps_old() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("app.exe");
        let new = dir.path().join("app.exe.new");
        std::fs::write(&exe, "RUNNING OLD").unwrap();
        std::fs::write(&new, "STAGED NEW").unwrap();

        swap_exe(&exe, &new).unwrap();
        assert_eq!(std::fs::read_to_string(&exe).unwrap(), "STAGED NEW");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("app.exe.old")).unwrap(),
            "RUNNING OLD"
        );
        assert!(!new.exists());
    }

    /// The core security property: a real signature over the fixture, made with
    /// the developer's secret key, verifies against the PUBLIC_KEY baked into
    /// the binary — and a single flipped byte makes verification fail. This
    /// ties the embedded key to the actual signing key (catches a wrong/typo'd
    /// PUBLIC_KEY) and proves tamper rejection.
    ///
    /// Ignored because it needs the committed signature fixture
    /// `tests/fixtures/signing/sample.bin.minisig`, produced once by:
    ///   rsign sign -s ~/.minisign/gakumas.key \
    ///     -x tests/fixtures/signing/sample.bin.minisig \
    ///        tests/fixtures/signing/sample.bin
    /// Run with: GAKUMAS_NO_MANIFEST=1 cargo test verify_signature_ -- --ignored
    #[test]
    #[ignore]
    fn verify_signature_accepts_genuine_and_rejects_tampered() {
        let base = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/signing");
        let data = std::fs::read(format!("{base}/sample.bin")).expect("fixture bin");
        let sig = std::fs::read_to_string(format!("{base}/sample.bin.minisig"))
            .expect("fixture minisig — sign it first (see doc comment)");

        // Genuine content verifies.
        verify_signature(&data, &sig).expect("genuine signature must verify");

        // One flipped byte must fail.
        let mut tampered = data.clone();
        tampered[0] ^= 0x01;
        assert!(
            verify_signature(&tampered, &sig).is_err(),
            "tampered content must be rejected"
        );

        // A signature the embedded key didn't make must fail (garbage sig).
        assert!(verify_signature(&data, "untrusted comment: x\nRWQnonsense\n").is_err());
    }
}
