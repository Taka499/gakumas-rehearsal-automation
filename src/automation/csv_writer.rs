//! CSV writer for automation results.
//!
//! Writes OCR results to a CSV file in append-only mode for crash safety.
//! Each row contains: iteration, timestamp, screenshot path, and 9 score values.

use crate::automation::queue::OcrWorkItem;
use anyhow::{Context, Result};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

/// CSV header row.
/// Columns: iteration, timestamp, screenshot, then 9 scores (3 stages × 3
/// criteria each), then a `recovery` flag (`ok`/`repaired`/`flagged`, the worst
/// of the three stages' overlap-reconstruction outcomes).
///
/// `recovery` is the 13th column, appended after the original 12. Readers index
/// the first 12 by position (see `analysis::csv_reader`), so older `results.csv`
/// files that lack it remain readable; when resuming such a file the header is
/// preserved (it simply won't name the trailing column).
pub(crate) const CSV_HEADER: &str = "iteration,timestamp,screenshot,s1c1,s1c2,s1c3,s2c1,s2c2,s2c3,s3c1,s3c2,s3c3,recovery";

/// Initializes CSV file with header if it doesn't exist or is empty.
///
/// If the file exists and has content, this does nothing (preserves existing data).
pub fn init_csv(path: &Path) -> Result<()> {
    if path.exists() {
        // Check if file has content
        let file = File::open(path).context("Failed to open existing CSV")?;
        let reader = BufReader::new(file);
        if reader.lines().next().is_some() {
            // File has content, don't overwrite
            return Ok(());
        }
    }

    // Create new file with header
    let mut file = File::create(path).context("Failed to create CSV file")?;
    writeln!(file, "{}", CSV_HEADER).context("Failed to write CSV header")?;
    Ok(())
}

/// Appends just the 9 scores (comma-separated, no header) to rehearsal_data.csv.
///
/// This file contains only raw score data for easy external processing.
pub fn append_to_raw_csv(path: &Path, scores: &[[u32; 3]; 3]) -> Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .context("Failed to open raw CSV for append")?;

    // Format: s1c1,s1c2,s1c3,s2c1,s2c2,s2c3,s3c1,s3c2,s3c3 (no header, just scores)
    let line = format!(
        "{},{},{},{},{},{},{},{},{}",
        scores[0][0],
        scores[0][1],
        scores[0][2],
        scores[1][0],
        scores[1][1],
        scores[1][2],
        scores[2][0],
        scores[2][1],
        scores[2][2],
    );

    writeln!(file, "{}", line).context("Failed to write raw CSV row")?;
    Ok(())
}

/// Appends one result row to the CSV file.
///
/// Opens the file in append mode for each write, ensuring crash safety.
/// If OCR fails partway through automation, completed results are already saved.
///
/// `recovery` is the overlap-reconstruction outcome for this row
/// (`ok`/`repaired`/`flagged`), written as the trailing column.
pub fn append_to_csv(
    path: &Path,
    work_item: &OcrWorkItem,
    scores: &[[u32; 3]; 3],
    recovery: &str,
) -> Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .context("Failed to open CSV for append")?;

    // Format: iteration,timestamp,screenshot,s1c1..s3c3,recovery
    let line = format!(
        "{},{},{},{},{},{},{},{},{},{},{},{},{}",
        work_item.iteration,
        work_item.captured_at.format("%Y-%m-%dT%H:%M:%S"),
        work_item.screenshot_path.display(),
        scores[0][0],
        scores[0][1],
        scores[0][2],
        scores[1][0],
        scores[1][1],
        scores[1][2],
        scores[2][0],
        scores[2][1],
        scores[2][2],
        recovery,
    );

    writeln!(file, "{}", line).context("Failed to write CSV row")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn test_init_csv_creates_header() {
        let dir = tempdir().unwrap();
        let csv_path = dir.path().join("test.csv");

        init_csv(&csv_path).unwrap();

        let content = std::fs::read_to_string(&csv_path).unwrap();
        assert!(content.starts_with(CSV_HEADER));
    }

    #[test]
    fn test_init_csv_preserves_existing() {
        let dir = tempdir().unwrap();
        let csv_path = dir.path().join("test.csv");

        // Write some existing content
        std::fs::write(&csv_path, "existing,data\n1,2,3\n").unwrap();

        init_csv(&csv_path).unwrap();

        let content = std::fs::read_to_string(&csv_path).unwrap();
        assert!(content.starts_with("existing,data"));
    }

    #[test]
    fn test_append_to_csv() {
        let dir = tempdir().unwrap();
        let csv_path = dir.path().join("test.csv");

        init_csv(&csv_path).unwrap();

        let work_item = OcrWorkItem::new(PathBuf::from("screenshots/001.png"), 1);
        let scores = [[100, 200, 300], [400, 500, 600], [700, 800, 900]];

        append_to_csv(&csv_path, &work_item, &scores, "ok").unwrap();

        let content = std::fs::read_to_string(&csv_path).unwrap();
        let lines: Vec<&str> = content.lines().collect();

        assert_eq!(lines.len(), 2); // header + 1 data row
        assert!(lines[1].contains("screenshots/001.png"));
        assert!(lines[1].contains("100,200,300,400,500,600,700,800,900"));
        assert!(lines[1].ends_with(",ok"));
    }

    #[test]
    fn test_append_multiple_rows() {
        let dir = tempdir().unwrap();
        let csv_path = dir.path().join("test.csv");

        init_csv(&csv_path).unwrap();

        for i in 1..=3 {
            let work_item = OcrWorkItem::new(PathBuf::from(format!("screenshots/{:03}.png", i)), i);
            let scores = [[i * 100, i * 100, i * 100]; 3];
            append_to_csv(&csv_path, &work_item, &scores, "ok").unwrap();
        }

        let content = std::fs::read_to_string(&csv_path).unwrap();
        let lines: Vec<&str> = content.lines().collect();

        assert_eq!(lines.len(), 4); // header + 3 data rows
    }
}
