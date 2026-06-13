//! Persistent per-session run metadata and resumable-session discovery.
//!
//! Each automation run writes `run-meta.json` into its session folder
//! (e.g. `output/20260606_141500/`). It records the originally requested run
//! count (`total`), which is otherwise only held in GUI memory, so an
//! interrupted run can be resumed even after the app restarts.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// File name written inside each session folder.
const META_FILENAME: &str = "run-meta.json";

/// Persisted metadata describing one automation run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMeta {
    /// Total number of runs originally requested.
    pub total: u32,
    /// Number of runs whose result was captured (best-effort snapshot).
    pub completed: u32,
    /// One of: "running", "completed", "aborted", "error".
    pub status: String,
    /// Optional human-readable error/abort detail.
    #[serde(default)]
    pub message: Option<String>,
}

/// A session folder that was interrupted before all runs finished.
#[derive(Debug, Clone)]
pub struct ResumableSession {
    pub path: PathBuf,
    pub total: u32,
    pub completed: u32,
}

/// Writes `run-meta.json` into `session_dir` (overwrites any existing file).
/// Failures are logged but never panic — metadata is best-effort.
pub fn write_meta(session_dir: &Path, meta: &RunMeta) {
    let path = session_dir.join(META_FILENAME);
    match serde_json::to_string_pretty(meta) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                crate::log(&format!("Failed to write run-meta.json: {}", e));
            }
        }
        Err(e) => crate::log(&format!("Failed to serialize run-meta: {}", e)),
    }
}

/// Reads `run-meta.json` from `session_dir`; None if missing or invalid.
pub fn read_meta(session_dir: &Path) -> Option<RunMeta> {
    let path = session_dir.join(META_FILENAME);
    let json = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&json).ok()
}

/// Counts captured screenshots in `session_dir/screenshots` (files ending
/// `.png`). This is the crash-proof source of truth for completed runs:
/// screenshots are saved synchronously in the `Capturing` state before any
/// asynchronous OCR, so they never lag behind actual progress.
pub fn count_captured(session_dir: &Path) -> u32 {
    let dir = session_dir.join("screenshots");
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return 0,
    };
    entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|x| x.eq_ignore_ascii_case("png"))
                .unwrap_or(false)
        })
        .count() as u32
}

/// Scans `output_dir` for interrupted runs that can be resumed.
///
/// A folder qualifies if it has a readable `run-meta.json` and its captured
/// count (recomputed from screenshots) is below `total`. Folders predating
/// this feature have no metadata and are skipped. Returned newest-first.
pub fn list_resumable(output_dir: &Path) -> Vec<ResumableSession> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(output_dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    // Folder names are timestamps (YYYYMMDD_HHMMSS) and sort chronologically.
    dirs.sort();
    dirs.reverse();
    for dir in dirs {
        if let Some(meta) = read_meta(&dir) {
            let completed = count_captured(&dir);
            if completed < meta.total {
                out.push(ResumableSession {
                    path: dir,
                    total: meta.total,
                    completed,
                });
            }
        }
    }
    out
}
