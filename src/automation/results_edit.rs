//! Load and rewrite a finished session's result CSVs for the GUI review/edit
//! feature.
//!
//! A session folder holds `results.csv` (header + one row per iteration:
//! `iteration,timestamp,screenshot,s1c1..s3c3,recovery`) and `rehearsal_data.csv`
//! (headerless, one line per iteration in order, just the nine scores). The
//! review window loads these into editable [`ReviewRow`]s, lets the user correct
//! OCR mistakes against the screenshot, and writes them back — keeping the two
//! files consistent and marking each hand-edited row `recovery=manual` so the
//! correction is auditable.
//!
//! Rewriting (not appending) is deliberate: editing an existing row is the whole
//! point. Saves go through a temp-file-then-rename so an interrupted write can
//! never truncate the originals.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// One reviewable/editable result row, mirroring a `results.csv` line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewRow {
    pub iteration: u32,
    /// Preserved verbatim on rewrite (we never re-stamp time on a correction).
    pub timestamp: String,
    /// Absolute screenshot path string exactly as stored in the CSV.
    pub screenshot: String,
    /// The nine per-character scores: `[stage][slot]`.
    pub scores: [[u32; 3]; 3],
    /// `ok` / `repaired` / `flagged` / `manual` / `verified`. `manual` is set when
    /// a row is hand-edited; `verified` when the user reviews an auto-recovered
    /// (flagged/repaired) row, confirms it is correct, and clears it without
    /// changing any value.
    pub recovery: String,
}

/// The recovery marker written for a row the user corrected by hand.
pub const RECOVERY_MANUAL: &str = "manual";

/// The recovery marker for a flagged/repaired row the user reviewed and confirmed
/// correct without editing it (resolves the flag while preserving the data).
pub const RECOVERY_VERIFIED: &str = "verified";

const CSV_HEADER: &str =
    "iteration,timestamp,screenshot,s1c1,s1c2,s1c3,s2c1,s2c2,s2c3,s3c1,s3c2,s3c3,recovery";

fn results_path(session_dir: &Path) -> PathBuf {
    session_dir.join("results.csv")
}

fn raw_path(session_dir: &Path) -> PathBuf {
    session_dir.join("rehearsal_data.csv")
}

/// Parses `results.csv` in `session_dir` into rows, in file order.
///
/// Tolerates the legacy 12-column form (no `recovery`) by defaulting the flag to
/// `ok`. A row that cannot be parsed (too few columns, non-numeric scores) is
/// skipped rather than aborting the whole load, so a partially-corrupt file is
/// still reviewable.
pub fn load_review_rows(session_dir: &Path) -> Result<Vec<ReviewRow>> {
    let path = results_path(session_dir);
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    let mut rows = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        if idx == 0 && line.starts_with("iteration,") {
            continue; // header
        }
        if line.trim().is_empty() {
            continue;
        }
        // Screenshot paths and ISO timestamps contain no commas, so a plain split
        // yields exactly 12 (legacy) or 13 (current) fields.
        let f: Vec<&str> = line.split(',').collect();
        if f.len() < 12 {
            continue;
        }
        let iteration: u32 = match f[0].trim().parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let mut scores = [[0u32; 3]; 3];
        let mut ok = true;
        for s in 0..3 {
            for c in 0..3 {
                match f[3 + s * 3 + c].trim().parse::<u32>() {
                    Ok(v) => scores[s][c] = v,
                    Err(_) => {
                        ok = false;
                        break;
                    }
                }
            }
        }
        if !ok {
            continue;
        }
        let recovery = f.get(12).map(|s| s.trim().to_string()).unwrap_or_default();
        let recovery = if recovery.is_empty() { "ok".to_string() } else { recovery };
        rows.push(ReviewRow {
            iteration,
            timestamp: f[1].trim().to_string(),
            screenshot: f[2].trim().to_string(),
            scores,
            recovery,
        });
    }
    Ok(rows)
}

/// Rewrites `results.csv` (header + every row) and patches `rehearsal_data.csv`
/// line-for-line from the same `rows`, in the given order.
///
/// The caller is responsible for having set `recovery = RECOVERY_MANUAL` on rows
/// it changed. Both files are written via a temp file + rename so a crash mid-
/// write cannot leave a truncated CSV.
pub fn save_review_rows(session_dir: &Path, rows: &[ReviewRow]) -> Result<()> {
    // results.csv
    let mut out = String::with_capacity(rows.len() * 96 + CSV_HEADER.len() + 1);
    out.push_str(CSV_HEADER);
    out.push('\n');
    for r in rows {
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
            r.iteration,
            r.timestamp,
            r.screenshot,
            r.scores[0][0], r.scores[0][1], r.scores[0][2],
            r.scores[1][0], r.scores[1][1], r.scores[1][2],
            r.scores[2][0], r.scores[2][1], r.scores[2][2],
            r.recovery,
        ));
    }
    write_atomic(&results_path(session_dir), &out)?;

    // rehearsal_data.csv (headerless, nine scores per line, same row order)
    let mut raw = String::with_capacity(rows.len() * 64);
    for r in rows {
        raw.push_str(&format!(
            "{},{},{},{},{},{},{},{},{}\n",
            r.scores[0][0], r.scores[0][1], r.scores[0][2],
            r.scores[1][0], r.scores[1][1], r.scores[1][2],
            r.scores[2][0], r.scores[2][1], r.scores[2][2],
        ));
    }
    write_atomic(&raw_path(session_dir), &raw)?;
    Ok(())
}

/// Writes `content` to `path` via a sibling temp file and an atomic rename, so an
/// interrupted write never truncates the existing file.
fn write_atomic(path: &Path, content: &str) -> Result<()> {
    let tmp = path.with_extension("csv.tmp");
    std::fs::write(&tmp, content)
        .with_context(|| format!("Failed to write {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("Failed to replace {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample_results() -> &'static str {
        "iteration,timestamp,screenshot,s1c1,s1c2,s1c3,s2c1,s2c2,s2c3,s3c1,s3c2,s3c3,recovery\n\
         1,2026-06-24T22:28:09,C:\\out\\001.png,100,200,300,400,500,600,700,800,900,ok\n\
         2,2026-06-24T22:28:30,C:\\out\\002.png,206174,1032,48189,0,0,0,11,22,33,flagged\n"
    }

    #[test]
    fn test_load_parses_rows() {
        let dir = tempdir().unwrap();
        std::fs::write(results_path(dir.path()), sample_results()).unwrap();
        let rows = load_review_rows(dir.path()).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].iteration, 1);
        assert_eq!(rows[0].scores, [[100, 200, 300], [400, 500, 600], [700, 800, 900]]);
        assert_eq!(rows[0].recovery, "ok");
        assert_eq!(rows[1].iteration, 2);
        assert_eq!(rows[1].scores[0], [206174, 1032, 48189]);
        assert_eq!(rows[1].recovery, "flagged");
        assert_eq!(rows[1].screenshot, "C:\\out\\002.png");
    }

    #[test]
    fn test_roundtrip_preserves_rows() {
        let dir = tempdir().unwrap();
        std::fs::write(results_path(dir.path()), sample_results()).unwrap();
        let rows = load_review_rows(dir.path()).unwrap();
        save_review_rows(dir.path(), &rows).unwrap();
        let again = load_review_rows(dir.path()).unwrap();
        assert_eq!(rows, again);
    }

    #[test]
    fn test_edit_updates_both_files_and_marks_manual() {
        let dir = tempdir().unwrap();
        std::fs::write(results_path(dir.path()), sample_results()).unwrap();
        let mut rows = load_review_rows(dir.path()).unwrap();
        // Correct iter 2's stage 2 to a real value and mark manual.
        rows[1].scores[1] = [206174, 1032249, 1048189];
        rows[1].recovery = RECOVERY_MANUAL.to_string();
        save_review_rows(dir.path(), &rows).unwrap();

        // results.csv reflects the edit + manual marker.
        let reloaded = load_review_rows(dir.path()).unwrap();
        assert_eq!(reloaded[1].scores[1], [206174, 1032249, 1048189]);
        assert_eq!(reloaded[1].recovery, "manual");

        // rehearsal_data.csv line 2 (iteration 2) carries the same nine scores.
        let raw = std::fs::read_to_string(raw_path(dir.path())).unwrap();
        let line2 = raw.lines().nth(1).unwrap();
        assert_eq!(line2, "206174,1032,48189,206174,1032249,1048189,11,22,33");
    }

    #[test]
    fn test_verified_recovery_roundtrips() {
        // A row marked `verified` (user confirmed a flagged row is correct without
        // editing it) must load and re-save with that marker unchanged.
        let dir = tempdir().unwrap();
        let csv = "iteration,timestamp,screenshot,s1c1,s1c2,s1c3,s2c1,s2c2,s2c3,s3c1,s3c2,s3c3,recovery\n\
                   1,2026-06-29T01:00:00,C:\\out\\001.png,848392,1340813,1026578,1,2,3,4,5,6,verified\n";
        std::fs::write(results_path(dir.path()), csv).unwrap();
        let rows = load_review_rows(dir.path()).unwrap();
        assert_eq!(rows[0].recovery, RECOVERY_VERIFIED);
        save_review_rows(dir.path(), &rows).unwrap();
        let again = load_review_rows(dir.path()).unwrap();
        assert_eq!(again[0].recovery, "verified");
        assert_eq!(again[0].scores[0], [848392, 1340813, 1026578]);
    }

    #[test]
    fn test_legacy_12_column_loads_and_saves_13() {
        let dir = tempdir().unwrap();
        let legacy = "iteration,timestamp,screenshot,s1c1,s1c2,s1c3,s2c1,s2c2,s2c3,s3c1,s3c2,s3c3\n\
                      1,2026-06-24T00:00:00,C:\\out\\001.png,1,2,3,4,5,6,7,8,9\n";
        std::fs::write(results_path(dir.path()), legacy).unwrap();
        let rows = load_review_rows(dir.path()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].recovery, "ok"); // defaulted
        save_review_rows(dir.path(), &rows).unwrap();
        let content = std::fs::read_to_string(results_path(dir.path())).unwrap();
        assert!(content.lines().next().unwrap().ends_with(",recovery"));
        assert!(content.lines().nth(1).unwrap().ends_with(",ok"));
    }
}
