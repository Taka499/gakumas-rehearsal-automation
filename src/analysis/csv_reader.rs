//! CSV reader for automation results.
//!
//! Parses the CSV file produced by automation (Phase 3) into structured data.

use anyhow::{anyhow, Context, Result};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Raw data from one CSV row (one rehearsal run).
#[derive(Debug, Clone)]
pub struct RunData {
    /// Iteration number (1-based)
    pub iteration: u32,
    /// Timestamp string (ISO format)
    pub timestamp: String,
    /// Path to screenshot file
    pub screenshot_path: String,
    /// Scores: [stage][criterion], 3 stages with 3 criteria each
    pub scores: [[u32; 3]; 3],
    /// Recovery state from the optional 13th column ("ok", "repaired",
    /// "flagged", "manual", "verified"); empty for legacy 12-column CSVs.
    pub recovery: String,
}

/// All data loaded from CSV.
#[derive(Debug, Clone)]
pub struct DataSet {
    /// All runs loaded from CSV
    pub runs: Vec<RunData>,
}

impl DataSet {
    /// Load data from a CSV file.
    ///
    /// CSV format expected:
    /// iteration,timestamp,screenshot,s1c1,s1c2,s1c3,s2c1,s2c2,s2c3,s3c1,s3c2,s3c3[,recovery]
    ///
    /// Skips the header row and any malformed rows (with warning log).
    /// Rows whose `recovery` column reads `flagged` are excluded: their stored
    /// values are one arbitrary checksum-valid candidate, not confirmed data,
    /// so they stay out of stats and charts until a human verifies them
    /// (docs/adr/0007) — matching the live distribution figure.
    pub fn from_csv(path: &Path) -> Result<Self> {
        let file = File::open(path).context(format!("Failed to open CSV file: {}", path.display()))?;
        let reader = BufReader::new(file);
        let mut runs = Vec::new();
        let mut flagged_excluded = 0usize;

        for (line_num, line_result) in reader.lines().enumerate() {
            let line = line_result.context("Failed to read line from CSV")?;

            // Skip header row
            if line_num == 0 {
                continue;
            }

            // Skip empty lines
            if line.trim().is_empty() {
                continue;
            }

            // Parse the line
            match Self::parse_line(&line) {
                Ok(run_data) => {
                    if run_data.recovery == "flagged" {
                        flagged_excluded += 1;
                    } else {
                        runs.push(run_data);
                    }
                }
                Err(e) => {
                    crate::log(&format!(
                        "Warning: Skipping malformed CSV row {}: {}",
                        line_num + 1,
                        e
                    ));
                }
            }
        }

        if flagged_excluded > 0 {
            crate::log(&format!(
                "Excluded {} flagged row(s) from analysis (unverified; see review window)",
                flagged_excluded
            ));
        }

        Ok(DataSet { runs })
    }

    /// Parse a single CSV line into RunData.
    fn parse_line(line: &str) -> Result<RunData> {
        let parts: Vec<&str> = line.split(',').collect();

        if parts.len() < 12 {
            return Err(anyhow!(
                "Expected 12 columns, got {}",
                parts.len()
            ));
        }

        let iteration = parts[0]
            .parse::<u32>()
            .context("Invalid iteration number")?;
        let timestamp = parts[1].to_string();
        let screenshot_path = parts[2].to_string();

        // Parse 9 score values
        let mut scores = [[0u32; 3]; 3];
        for stage in 0..3 {
            for criterion in 0..3 {
                let idx = 3 + stage * 3 + criterion;
                scores[stage][criterion] = parts[idx]
                    .parse::<u32>()
                    .context(format!("Invalid score at column {}", idx + 1))?;
            }
        }

        let recovery = parts.get(12).map(|s| s.trim().to_string()).unwrap_or_default();

        Ok(RunData {
            iteration,
            timestamp,
            screenshot_path,
            scores,
            recovery,
        })
    }

    /// Get all values for a specific column (stage, criterion).
    ///
    /// Stage and criterion are 0-indexed.
    pub fn column_values(&self, stage: usize, criterion: usize) -> Vec<u32> {
        self.runs
            .iter()
            .map(|run| run.scores[stage][criterion])
            .collect()
    }

    /// Number of runs in the dataset.
    pub fn len(&self) -> usize {
        self.runs.len()
    }

    /// Check if the dataset is empty.
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_csv(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", content).unwrap();
        file
    }

    #[test]
    fn test_parse_valid_csv() {
        let csv_content = "iteration,timestamp,screenshot,s1c1,s1c2,s1c3,s2c1,s2c2,s2c3,s3c1,s3c2,s3c3
1,2026-01-15T10:00:00,test1.png,100,200,300,400,500,600,700,800,900
2,2026-01-15T10:01:00,test2.png,110,210,310,410,510,610,710,810,910";

        let file = create_test_csv(csv_content);
        let dataset = DataSet::from_csv(file.path()).unwrap();

        assert_eq!(dataset.len(), 2);
        assert_eq!(dataset.runs[0].iteration, 1);
        assert_eq!(dataset.runs[0].scores[0][0], 100); // s1c1
        assert_eq!(dataset.runs[0].scores[2][2], 900); // s3c3
        assert_eq!(dataset.runs[1].iteration, 2);
        assert_eq!(dataset.runs[1].scores[0][0], 110);
    }

    #[test]
    fn test_column_values() {
        let csv_content = "iteration,timestamp,screenshot,s1c1,s1c2,s1c3,s2c1,s2c2,s2c3,s3c1,s3c2,s3c3
1,2026-01-15T10:00:00,test1.png,100,200,300,400,500,600,700,800,900
2,2026-01-15T10:01:00,test2.png,150,250,350,450,550,650,750,850,950";

        let file = create_test_csv(csv_content);
        let dataset = DataSet::from_csv(file.path()).unwrap();

        let s1c1_values = dataset.column_values(0, 0);
        assert_eq!(s1c1_values, vec![100, 150]);

        let s3c3_values = dataset.column_values(2, 2);
        assert_eq!(s3c3_values, vec![900, 950]);
    }

    #[test]
    fn test_empty_csv_header_only() {
        let csv_content =
            "iteration,timestamp,screenshot,s1c1,s1c2,s1c3,s2c1,s2c2,s2c3,s3c1,s3c2,s3c3\n";

        let file = create_test_csv(csv_content);
        let dataset = DataSet::from_csv(file.path()).unwrap();

        assert!(dataset.is_empty());
    }

    #[test]
    fn test_flagged_rows_excluded_others_included() {
        // 13-column CSV: flagged is excluded (unverified data, docs/adr/0007);
        // ok/repaired/manual/verified are included; legacy 12-column rows
        // (no recovery value) are included.
        let csv_content = "iteration,timestamp,screenshot,s1c1,s1c2,s1c3,s2c1,s2c2,s2c3,s3c1,s3c2,s3c3,recovery
1,2026-01-15T10:00:00,t1.png,100,200,300,400,500,600,700,800,900,ok
2,2026-01-15T10:01:00,t2.png,110,210,310,410,510,610,710,810,910,flagged
3,2026-01-15T10:02:00,t3.png,120,220,320,420,520,620,720,820,920,repaired
4,2026-01-15T10:03:00,t4.png,130,230,330,430,530,630,730,830,930,verified
5,2026-01-15T10:04:00,t5.png,140,240,340,440,540,640,740,840,940";

        let file = create_test_csv(csv_content);
        let dataset = DataSet::from_csv(file.path()).unwrap();

        assert_eq!(dataset.len(), 4);
        let iterations: Vec<u32> = dataset.runs.iter().map(|r| r.iteration).collect();
        assert_eq!(iterations, vec![1, 3, 4, 5]);
        assert_eq!(dataset.runs[0].recovery, "ok");
        assert_eq!(dataset.runs[3].recovery, "");
    }

    #[test]
    fn test_skip_empty_lines() {
        let csv_content = "iteration,timestamp,screenshot,s1c1,s1c2,s1c3,s2c1,s2c2,s2c3,s3c1,s3c2,s3c3
1,2026-01-15T10:00:00,test1.png,100,200,300,400,500,600,700,800,900

2,2026-01-15T10:01:00,test2.png,110,210,310,410,510,610,710,810,910";

        let file = create_test_csv(csv_content);
        let dataset = DataSet::from_csv(file.path()).unwrap();

        assert_eq!(dataset.len(), 2);
    }
}
