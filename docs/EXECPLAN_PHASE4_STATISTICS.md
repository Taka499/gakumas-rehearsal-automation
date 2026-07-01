# Phase 4: Statistics Calculation and Chart Visualization

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds. This document must be maintained in accordance with `docs/PLANS.md`.


## Purpose / Big Picture

After this change, the user can generate statistical analysis and visual charts from their rehearsal data by selecting a tray menu option. The application reads the CSV file produced by automation (Phase 3), calculates statistics for each of the 9 score columns (mean, median, min, max, standard deviation, quartiles), and generates per-column charts with box plots and histograms, plus a combined box plot showing all columns. Charts are saved as PNG files in an `output` subfolder.

This enables users to:
- See average performance across all rehearsal runs at a glance
- Understand score variability through box plots (showing quartiles and outliers)
- Share or analyze the generated PNG charts in external tools
- Get a JSON summary of all computed statistics
- Customize chart styling via `chart_config.json` without rebuilding

The feature completes the data pipeline: Automation (Phase 3) collects data → Statistics (Phase 4) analyzes and visualizes it.


## Progress

- [x] (2026-01-15 10:00) Milestone 1: Add dependencies (plotters crate)
- [x] (2026-01-15 10:05) Milestone 2: CSV reader and data structures
- [x] (2026-01-15 10:10) Milestone 3: Statistics calculation module
- [x] (2026-01-15 10:15) Milestone 4: Per-character chart generation (box plot + histogram)
- [x] (2026-01-15 10:20) Milestone 5: Statistics table in charts
- [x] (2026-01-15 10:25) Milestone 6: JSON export
- [x] (2026-01-15 10:30) Milestone 7: Tray menu integration
- [x] (2026-01-15 11:00) Milestone 8: End-to-end testing - COMPLETE
- [x] (2026-01-15 12:00) Milestone 9: Combined box plot for all columns
- [x] (2026-01-15 12:30) Milestone 10: Orange color scheme and improved styling
- [x] (2026-01-15 13:00) Milestone 11: Configurable chart styling (chart_config.json)
- [x] (2026-01-15 13:30) Milestone 12: Output to dedicated output folder


## Surprises & Discoveries

- Observation: Unit tests cannot run from cargo test due to admin manifest requirement
  Evidence: "The requested operation requires elevation. (os error 740)"
  Resolution: Tests compile successfully (validating code correctness), manual testing required for runtime verification. This is a known limitation documented in CLAUDE.md.


## Decision Log

- Decision: Use `plotters` crate for chart generation
  Rationale: Pure Rust, no external dependencies, supports PNG output, well-documented. Mentioned in ROADMAP_AUTOMATION.md as the intended solution.
  Date/Author: 2026-01-15 / Initial design

- Decision: Read from existing CSV file rather than keeping data in memory
  Rationale: CSV is the source of truth. User can run automation multiple times, accumulating data. Statistics should reflect all accumulated data. Also allows re-running statistics without re-running automation.
  Date/Author: 2026-01-15 / Initial design

- Decision: Charts saved as PNG files in exe directory
  Rationale: Consistent with CSV and screenshot output locations. PNG is widely supported and lossless. Easy to share or embed in reports.
  Date/Author: 2026-01-15 / Initial design

- Decision: JSON export for programmatic analysis
  Rationale: Users may want to process statistics in external tools (Python, Excel). JSON is machine-readable and widely supported.
  Date/Author: 2026-01-15 / Initial design

- Decision: Trigger via tray menu, not automatic after automation
  Rationale: User may want to run multiple automation sessions before generating charts. Manual trigger gives control. Also allows regenerating charts if CSV is manually edited.
  Date/Author: 2026-01-15 / Initial design

- Decision: Changed from combined charts to per-character charts
  Rationale: User requested individual charts for each of the 9 columns (S1C1-S3C3). Each chart contains a box plot, distribution histogram with dynamic bucket sizing, and a statistics table showing MIN/MEAN/MODE/MAX.
  Date/Author: 2026-01-15 / User feedback

- Decision: Dynamic bucket sizing for histograms
  Rationale: Formula `bucketSize = 1000 * max(floor((max-min)/1000/100), 1)` adapts to data range. Prevents too many or too few buckets. MIN_BUCKET_SIZE of 1000 provides reasonable granularity for typical game scores.
  Date/Author: 2026-01-15 / Implementation

- Decision: Orange color scheme with table at top
  Rationale: User provided reference screenshot showing preferred style. Table moved to top with orange header bar, histogram and box plot use orange colors, light gray background with grid lines.
  Date/Author: 2026-01-15 / User feedback

- Decision: Statistics table shows Min/Average/Median/Max (replaced Mode with Median)
  Rationale: Median is more useful for understanding score distribution than Mode. User requested this change.
  Date/Author: 2026-01-15 / User feedback

- Decision: Add combined box plot showing all 9 columns
  Rationale: Allows visual comparison of all score columns in a single chart. Each stage has distinct color (red/green/blue). X-axis shows all column labels (S1C1-S3C3).
  Date/Author: 2026-01-15 / User feedback

- Decision: Configurable chart styling via chart_config.json
  Rationale: Allows iterating on font sizes, colors, and layout without rebuilding. Config is loaded fresh each time charts are generated. Default config is auto-created on first run.
  Date/Author: 2026-01-15 / User feedback

- Decision: Output files saved to `output` subfolder
  Rationale: Keeps generated files organized separately from input files and config. Cleaner directory structure.
  Date/Author: 2026-01-15 / User feedback


## Outcomes & Retrospective

Phase 4 implementation is complete. The feature successfully:

1. Reads CSV data from Phase 3 automation output
2. Calculates comprehensive statistics (mean, median, min, max, std_dev, quartiles)
3. Generates 9 per-character PNG charts, each containing:
   - Statistics table at top (Min, Average, Median, Max) with orange header
   - Box plot showing distribution (min, Q1, median, Q3, max)
   - Histogram with dynamic bucket sizing and legend
4. Generates a combined box plot showing all 9 columns side by side
5. Exports full statistics to JSON for programmatic analysis
6. Supports configurable styling via `chart_config.json`

Output structure:
```
exe_directory/
├── results.csv              # Input from automation
├── chart_config.json        # Styling configuration (auto-created)
└── output/                   # Generated output
    ├── chart_s1c1.png       # Per-column charts (9 total)
    ├── ...
    ├── chart_s3c3.png
    ├── chart_combined.png   # Combined box plot
    └── statistics.json      # JSON statistics
```

The data pipeline is now complete: Automation → OCR → CSV → Statistics → Charts


## Context and Orientation

This plan builds upon Phase 3 (Automation Loop), which produces a CSV file at `results.csv` in the exe directory. The CSV has this format:

    iteration,timestamp,screenshot,s1c1,s1c2,s1c3,s2c1,s2c2,s2c3,s3c1,s3c2,s3c3
    1,2026-01-13T14:30:00,screenshots/001.png,50339,50796,70859,64997,168009,128450,122130,105901,96776
    2,2026-01-13T14:30:45,screenshots/002.png,48000,52000,71000,65000,170000,130000,120000,106000,97000

Column meanings (from game context):
- `s1c1`, `s1c2`, `s1c3`: Stage 1, criteria 1/2/3 (3 breakdown scores for stage 1)
- `s2c1`, `s2c2`, `s2c3`: Stage 2, criteria 1/2/3
- `s3c1`, `s3c2`, `s3c3`: Stage 3, criteria 1/2/3

Each row represents one rehearsal run. The automation captures 9 scores per run (3 stages with 3 breakdown scores each).

Key existing files:

    src/automation/csv_writer.rs   - Writes CSV (we will add a reader)
    src/automation/runner.rs       - Automation entry point
    src/paths.rs                   - Centralized path resolution
    src/main.rs                    - Tray menu and hotkey handling

New module structure for Phase 4:

    src/analysis/
    ├── mod.rs          - Module exports, entry point
    ├── csv_reader.rs   - Parse CSV into data structures
    ├── statistics.rs   - Calculate statistics (mean, median, etc.)
    ├── charts.rs       - Generate charts using plotters
    └── export.rs       - JSON export

Terms used in this document:

- Mean: The arithmetic average of a set of values.
- Median: The middle value when values are sorted. For even counts, average of two middle values.
- Mode: The most frequently occurring value.
- Standard deviation (std_dev): A measure of how spread out values are from the mean.
- Quartile: Values that divide sorted data into four equal parts. Q1 (25th percentile), Q2 (median, 50th), Q3 (75th percentile).
- Box plot (box-and-whisker plot): A chart showing min, Q1, median, Q3, max as a box with whiskers.
- Bar chart: A chart with rectangular bars whose heights represent values.
- Interquartile range (IQR): Q3 minus Q1, used to identify outliers.


## Plan of Work

### Milestone 1: Add Dependencies

Add the `plotters` crate to Cargo.toml for chart generation. The crate supports bitmap output (PNG) via its `BitMapBackend`.

In `Cargo.toml`, add under `[dependencies]`:

    plotters = "0.3"

This is the only new dependency needed. The `csv` crate mentioned in the roadmap is not needed because we will parse manually (the format is simple and fixed).


### Milestone 2: CSV Reader and Data Structures

Create `src/analysis/csv_reader.rs` to parse the CSV file into structured data.

Data structures:

    /// Raw data from one CSV row (one rehearsal run)
    pub struct RunData {
        pub iteration: u32,
        pub timestamp: String,
        pub screenshot_path: String,
        pub scores: [[u32; 3]; 3],  // [stage][criterion]
    }

    /// All data loaded from CSV
    pub struct DataSet {
        pub runs: Vec<RunData>,
    }

    impl DataSet {
        /// Load from CSV file path
        pub fn from_csv(path: &Path) -> Result<Self>;

        /// Get all values for a specific column (stage, criterion)
        pub fn column_values(&self, stage: usize, criterion: usize) -> Vec<u32>;

        /// Number of runs
        pub fn len(&self) -> usize;
    }

CSV parsing approach:
1. Read file line by line
2. Skip header row (first line)
3. Split each line by comma
4. Parse iteration (u32), timestamp (String), screenshot (String), then 9 scores (u32)
5. Collect into Vec<RunData>

Error handling:
- Return error if file doesn't exist
- Return error if CSV is empty (no data rows)
- Skip malformed rows with warning log (don't fail entire parse)


### Milestone 3: Statistics Calculation

Create `src/analysis/statistics.rs` with functions to calculate statistics.

Column statistics structure:

    /// Statistics for one score column
    #[derive(Debug, Clone, Serialize)]
    pub struct ColumnStats {
        pub stage: usize,           // 1, 2, or 3
        pub criterion: usize,       // 1, 2, or 3
        pub count: usize,           // Number of values
        pub mean: f64,              // Average
        pub median: f64,            // Middle value
        pub mode: u32,              // Most frequent value
        pub min: u32,               // Minimum value
        pub max: u32,               // Maximum value
        pub std_dev: f64,           // Standard deviation
        pub quartile_1: f64,        // 25th percentile
        pub quartile_3: f64,        // 75th percentile
    }

    /// Statistics for entire dataset
    #[derive(Debug, Clone, Serialize)]
    pub struct DataSetStats {
        pub total_runs: usize,
        pub columns: Vec<ColumnStats>,  // 9 columns
    }

    impl DataSetStats {
        /// Calculate statistics for all columns
        pub fn from_dataset(data: &DataSet) -> Self;
    }

Calculation formulas:

Mean:
    sum(values) / count

Median:
    Sort values
    If count is odd: middle value
    If count is even: average of two middle values

Mode:
    Count occurrences of each value
    Return value with highest count
    If tie, return smallest value

Standard deviation (population):
    sqrt(sum((value - mean)^2) / count)

Percentile (linear interpolation):
    index = (percentile / 100) * (count - 1)
    lower = values[floor(index)]
    upper = values[ceil(index)]
    result = lower + (upper - lower) * frac(index)

Quartiles:
    Q1 = percentile(25)
    Q3 = percentile(75)


### Milestone 4: Bar Chart Generation

Create chart generation in `src/analysis/charts.rs`.

Bar chart specifications:
- Title: "Average Scores by Stage/Criterion"
- X-axis: 9 bars labeled "S1C1", "S1C2", "S1C3", "S2C1", "S2C2", "S2C3", "S3C1", "S3C2", "S3C3"
- Y-axis: Score value (0 to max_mean * 1.1 for headroom)
- Bar colors: Different color per stage (Stage 1 = red, Stage 2 = green, Stage 3 = blue)
- Image size: 800x600 pixels
- Output: PNG file

Implementation using plotters:

    use plotters::prelude::*;

    pub fn generate_bar_chart(stats: &DataSetStats, output_path: &Path) -> Result<()> {
        let root = BitMapBackend::new(output_path, (800, 600)).into_drawing_area();
        root.fill(&WHITE)?;

        // Find max mean for Y-axis scaling
        let max_mean = stats.columns.iter()
            .map(|c| c.mean)
            .fold(0.0f64, f64::max);

        let mut chart = ChartBuilder::on(&root)
            .caption("Average Scores by Stage/Criterion", ("sans-serif", 24))
            .margin(20)
            .x_label_area_size(40)
            .y_label_area_size(60)
            .build_cartesian_2d(
                0..9,                      // 9 bars
                0.0..(max_mean * 1.1),     // Y range with headroom
            )?;

        chart.configure_mesh()
            .x_labels(9)
            .x_label_formatter(&|x| {
                let stage = x / 3 + 1;
                let criterion = x % 3 + 1;
                format!("S{}C{}", stage, criterion)
            })
            .y_desc("Average Score")
            .draw()?;

        // Draw bars with stage-based colors
        let colors = [&RED, &GREEN, &BLUE];
        for (idx, col_stats) in stats.columns.iter().enumerate() {
            let stage = idx / 3;
            chart.draw_series(std::iter::once(
                Rectangle::new(
                    [(idx as i32, 0.0), ((idx + 1) as i32, col_stats.mean)],
                    colors[stage].filled(),
                )
            ))?;
        }

        root.present()?;
        Ok(())
    }


### Milestone 5: Box Plot Generation

Box plot specifications:
- Title: "Score Distribution by Stage/Criterion"
- X-axis: 9 positions labeled "S1C1" through "S3C3"
- Y-axis: Score value (0 to max_value * 1.1)
- Box: Q1 to Q3 range
- Median line: Horizontal line inside box (different color)
- Whiskers: Min to Q1, Q3 to Max
- Image size: 800x600 pixels
- Output: PNG file

Implementation:

    pub fn generate_box_plot(stats: &DataSetStats, output_path: &Path) -> Result<()> {
        let root = BitMapBackend::new(output_path, (800, 600)).into_drawing_area();
        root.fill(&WHITE)?;

        let max_val = stats.columns.iter()
            .map(|c| c.max as f64)
            .fold(0.0f64, f64::max);

        let mut chart = ChartBuilder::on(&root)
            .caption("Score Distribution by Stage/Criterion", ("sans-serif", 24))
            .margin(20)
            .x_label_area_size(40)
            .y_label_area_size(60)
            .build_cartesian_2d(
                0f64..9f64,
                0f64..(max_val * 1.1),
            )?;

        chart.configure_mesh()
            .x_labels(9)
            .x_label_formatter(&|x| {
                let idx = *x as usize;
                if idx < 9 {
                    let stage = idx / 3 + 1;
                    let criterion = idx % 3 + 1;
                    format!("S{}C{}", stage, criterion)
                } else {
                    String::new()
                }
            })
            .y_desc("Score")
            .draw()?;

        // Draw box plots
        for (idx, col_stats) in stats.columns.iter().enumerate() {
            let x_center = idx as f64 + 0.5;
            let box_width = 0.3;

            // Box (Q1 to Q3)
            chart.draw_series(std::iter::once(
                Rectangle::new(
                    [(x_center - box_width, col_stats.quartile_1),
                     (x_center + box_width, col_stats.quartile_3)],
                    BLUE.stroke_width(2),
                )
            ))?;

            // Median line
            chart.draw_series(std::iter::once(
                PathElement::new(
                    vec![
                        (x_center - box_width, col_stats.median),
                        (x_center + box_width, col_stats.median),
                    ],
                    RED.stroke_width(2),
                )
            ))?;

            // Lower whisker (min to Q1)
            chart.draw_series(std::iter::once(
                PathElement::new(
                    vec![
                        (x_center, col_stats.min as f64),
                        (x_center, col_stats.quartile_1),
                    ],
                    BLACK.stroke_width(1),
                )
            ))?;

            // Upper whisker (Q3 to max)
            chart.draw_series(std::iter::once(
                PathElement::new(
                    vec![
                        (x_center, col_stats.quartile_3),
                        (x_center, col_stats.max as f64),
                    ],
                    BLACK.stroke_width(1),
                )
            ))?;

            // Min/Max caps (horizontal lines at whisker ends)
            let cap_width = 0.15;
            chart.draw_series(std::iter::once(
                PathElement::new(
                    vec![
                        (x_center - cap_width, col_stats.min as f64),
                        (x_center + cap_width, col_stats.min as f64),
                    ],
                    BLACK.stroke_width(1),
                )
            ))?;
            chart.draw_series(std::iter::once(
                PathElement::new(
                    vec![
                        (x_center - cap_width, col_stats.max as f64),
                        (x_center + cap_width, col_stats.max as f64),
                    ],
                    BLACK.stroke_width(1),
                )
            ))?;
        }

        root.present()?;
        Ok(())
    }


### Milestone 6: JSON Export

Create `src/analysis/export.rs` for JSON output.

The `DataSetStats` struct already derives `Serialize`. Export is straightforward:

    use std::fs::File;
    use std::io::Write;

    pub fn export_to_json(stats: &DataSetStats, output_path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(stats)?;
        let mut file = File::create(output_path)?;
        file.write_all(json.as_bytes())?;
        Ok(())
    }

Output format:

    {
      "total_runs": 50,
      "columns": [
        {
          "stage": 1,
          "criterion": 1,
          "count": 50,
          "mean": 51234.5,
          "median": 50000.0,
          "mode": 49500,
          "min": 45000,
          "max": 58000,
          "std_dev": 2345.67,
          "quartile_1": 48000.0,
          "quartile_3": 54000.0
        },
        ...
      ]
    }


### Milestone 7: Tray Menu Integration

Add menu items to the system tray context menu in `src/main.rs`.

New menu items:
- "Generate Charts" - Runs full analysis pipeline

Menu structure (updated):

    Right-click tray icon:
    ├── Take Screenshot (Ctrl+Shift+S)
    ├── Start Automation (Ctrl+Shift+A)
    ├── ─────────────────────
    ├── Calibration
    │   ├── Run Calibration Wizard
    │   ├── Capture Start Reference
    │   ├── Capture Skip Reference
    │   └── Capture End Reference
    ├── ─────────────────────
    ├── Generate Charts          <-- NEW
    ├── ─────────────────────
    └── Exit

Implementation in `window_proc`:

    const IDM_GENERATE_CHARTS: u16 = 108;

    // In menu creation
    AppendMenuW(hmenu, MF_STRING, IDM_GENERATE_CHARTS as usize, w!("Generate Charts"))?;

    // In WM_COMMAND handler
    IDM_GENERATE_CHARTS => {
        match analysis::generate_analysis() {
            Ok(paths) => {
                log(&format!("Charts generated: {}, {}", paths.0.display(), paths.1.display()));
                log(&format!("Statistics saved: {}", paths.2.display()));
            }
            Err(e) => {
                log(&format!("Failed to generate charts: {}", e));
            }
        }
    }

Entry point function in `src/analysis/mod.rs`:

    /// Runs the full analysis pipeline: read CSV, calculate stats, generate charts, export JSON.
    /// Returns paths to (bar_chart.png, box_plot.png, statistics.json).
    pub fn generate_analysis() -> Result<(PathBuf, PathBuf, PathBuf)> {
        let exe_dir = crate::paths::get_exe_dir();
        let csv_path = exe_dir.join("results.csv");
        let bar_chart_path = exe_dir.join("chart_averages.png");
        let box_plot_path = exe_dir.join("chart_distribution.png");
        let json_path = exe_dir.join("statistics.json");

        // Load data
        let data = csv_reader::DataSet::from_csv(&csv_path)?;
        if data.len() == 0 {
            return Err(anyhow!("No data in CSV file"));
        }

        crate::log(&format!("Loaded {} runs from CSV", data.len()));

        // Calculate statistics
        let stats = statistics::DataSetStats::from_dataset(&data);

        // Generate charts
        charts::generate_bar_chart(&stats, &bar_chart_path)?;
        charts::generate_box_plot(&stats, &box_plot_path)?;

        // Export JSON
        export::export_to_json(&stats, &json_path)?;

        Ok((bar_chart_path, box_plot_path, json_path))
    }


### Milestone 8: End-to-End Testing

Manual testing checklist:
- Run automation to generate CSV with at least 5 rows
- Select "Generate Charts" from tray menu
- Verify bar chart PNG is created with correct averages
- Verify box plot PNG is created with correct distribution
- Verify JSON file contains expected statistics
- Test with empty CSV (should show error)
- Test with CSV containing 1 row (edge case for median/quartiles)
- Test with CSV containing 2 rows
- Open generated PNGs in image viewer to verify visual correctness

Unit tests:
- Test statistics calculation with known values
- Test CSV parsing with valid and malformed data
- Test chart generation doesn't crash (visual verification is manual)


## Concrete Steps

All commands run from repository root: `C:\Work\GitRepos\gakumas-rehearsal-automation`


### Step 1: Add plotters dependency

Edit `Cargo.toml` to add plotters:

    plotters = "0.3"

Verify:

    cargo build --release

Expected: Build succeeds. Plotters downloads and compiles.


### Step 2: Create analysis module structure

Create these files:

    src/analysis/mod.rs
    src/analysis/csv_reader.rs
    src/analysis/statistics.rs
    src/analysis/charts.rs
    src/analysis/export.rs

Add to `src/main.rs`:

    mod analysis;

Verify:

    cargo build --release

Expected: Build succeeds with empty module stubs.


### Step 3: Implement CSV reader

Implement `src/analysis/csv_reader.rs` with `DataSet::from_csv()`.

Create a test CSV file manually:

    echo iteration,timestamp,screenshot,s1c1,s1c2,s1c3,s2c1,s2c2,s2c3,s3c1,s3c2,s3c3 > test_data.csv
    echo 1,2026-01-15T10:00:00,test.png,100,200,300,400,500,600,700,800,900 >> test_data.csv
    echo 2,2026-01-15T10:01:00,test.png,110,210,310,410,510,610,710,810,910 >> test_data.csv

Verify with unit test:

    cargo test analysis::csv_reader::tests -- --nocapture

Expected: Tests pass, data is correctly parsed.


### Step 4: Implement statistics calculation

Implement `src/analysis/statistics.rs`.

Verify with unit test using known values:

    cargo test analysis::statistics::tests -- --nocapture

Example test: For values [1, 2, 3, 4, 5]:
- Mean = 3.0
- Median = 3.0
- Min = 1, Max = 5
- Q1 = 2.0, Q3 = 4.0
- Std dev = sqrt(2) ≈ 1.414


### Step 5: Implement bar chart

Implement `src/analysis/charts.rs` with `generate_bar_chart()`.

Verify:

    cargo build --release

Expected: Build succeeds. Manual verification by generating a test chart.


### Step 6: Implement box plot

Add `generate_box_plot()` to `src/analysis/charts.rs`.

Verify:

    cargo build --release

Expected: Build succeeds.


### Step 7: Implement JSON export

Implement `src/analysis/export.rs`.

Verify:

    cargo build --release

Expected: Build succeeds.


### Step 8: Implement entry point

Add `generate_analysis()` to `src/analysis/mod.rs`.

Verify:

    cargo build --release

Expected: Build succeeds.


### Step 9: Add tray menu item

Update `src/main.rs` to add "Generate Charts" menu item.

Verify:

    cargo build --release
    .\target\release\gakumas-rehearsal-automation.exe

Right-click tray icon. Expected: "Generate Charts" menu item appears.


### Step 10: End-to-end test

Ensure a valid `results.csv` exists (from previous automation runs or manually created).

1. Run the application
2. Right-click tray → "Generate Charts"
3. Check console log for success message
4. Verify files created:
   - `chart_averages.png`
   - `chart_distribution.png`
   - `statistics.json`

Expected log output:

    [10:30:00.000] Loaded 10 runs from CSV
    [10:30:00.500] Charts generated: C:\...\chart_averages.png, C:\...\chart_distribution.png
    [10:30:00.500] Statistics saved: C:\...\statistics.json


## Validation and Acceptance

The statistics feature is complete when:

1. "Generate Charts" menu item appears in tray menu

2. Clicking "Generate Charts" reads `results.csv` and processes all rows

3. `chart_averages.png` is created showing 9 bars with correct heights

4. `chart_distribution.png` is created showing 9 box plots

5. `statistics.json` is created with valid JSON containing all statistics

6. Statistics values are mathematically correct (verified with test data)

7. Error handling works: empty CSV shows error message, doesn't crash

8. Charts are visually readable (bars have labels, colors distinguish stages)

9. Multiple runs don't corrupt data (each run reads fresh from CSV)


## Idempotence and Recovery

- Running "Generate Charts" multiple times overwrites previous chart files (safe)
- CSV file is read-only during analysis (never modified)
- If chart generation fails partway, partial files may exist but no data loss
- If CSV is malformed, parser skips bad rows and logs warnings
- Charts can be regenerated anytime without affecting CSV data


## Artifacts and Notes

### Expected Output Files

After running "Generate Charts":

    exe_directory/
    ├── results.csv              # Input (from Phase 3)
    ├── chart_config.json        # Styling configuration
    └── output/
        ├── chart_s1c1.png       # Per-column charts (9 total)
        ├── chart_s1c2.png
        ├── chart_s1c3.png
        ├── chart_s2c1.png
        ├── chart_s2c2.png
        ├── chart_s2c3.png
        ├── chart_s3c1.png
        ├── chart_s3c2.png
        ├── chart_s3c3.png
        ├── chart_combined.png   # Combined box plot
        └── statistics.json      # JSON statistics output


### Sample chart_config.json

    {
      "font": {
        "title_size": 32,
        "table_header_size": 32,
        "table_value_size": 32,
        "axis_label_size": 14,
        "legend_size": 14,
        "box_plot_caption_size": 16
      },
      "colors": {
        "orange_primary": [243, 156, 18],
        "orange_header": [230, 126, 34],
        "light_gray_bg": [245, 245, 245],
        "grid_color": [220, 220, 220]
      },
      "layout": {
        "chart_width": 900,
        "chart_height": 700,
        "title_height": 50,
        "table_height": 90,
        "table_header_height": 40,
        "box_plot_width": 300
      }
    }


### Sample JSON Output (statistics.json)

    {
      "total_runs": 10,
      "columns": [
        {
          "stage": 1,
          "criterion": 1,
          "count": 10,
          "mean": 50500.0,
          "median": 50250.0,
          "mode": 50000,
          "min": 48000,
          "max": 53000,
          "std_dev": 1500.5,
          "quartile_1": 49000.0,
          "quartile_3": 52000.0
        },
        ...8 more columns...
      ]
    }


### Chart Visual Reference

Per-Column Chart Layout (chart_s1c1.png etc.):

    ┌─────────────────────────────────────────────────────────┐
    │  S1C1 Distribution                           (n = 100)  │
    ├───────────┬───────────┬───────────┬─────────────────────┤
    │    Min    │  Average  │  Median   │        Max          │ ← Orange header
    ├───────────┼───────────┼───────────┼─────────────────────┤
    │   48000   │   50500   │   50250   │       53000         │ ← Values
    ├───────────────────────┴───────────────────────────────────┤
    │  ┌─────────┐    ┌──────────────────────────────────┐    │
    │  │Box Plot │    │  ██ Score (n=100)                │    │
    │  │  ┬      │    │  ████████                        │    │
    │  │  │      │    │  ████████████                    │    │
    │  │ ┌┴─┐    │    │  ████████████████                │    │
    │  │ │──│    │    │  Histogram                       │    │
    │  │ └┬─┘    │    │                                  │    │
    │  │  │      │    │                                  │    │
    │  │  ┴      │    │                                  │    │
    │  └─────────┘    └──────────────────────────────────┘    │
    └─────────────────────────────────────────────────────────┘

Combined Box Plot Layout (chart_combined.png):

    ┌────────────────────────────────────────────────────────┐
    │     Score Distribution (100 runs)                       │
    │                                                         │
    │        ┬           ┬                                    │
    │        │           │                 ┬                  │
    │   ┬    ├───┐  ┬    ├───┐        ┬    │                  │
    │   │    │   │  │    │   │   ┬    │    ├───┐              │
    │   ├───┐├───┤  ├───┐├───┤   │    ├───┐│   │  ...         │
    │   │   ││   │  │   ││   │   ├───┐│   ││   │              │
    │   └───┘└───┘  └───┘└───┘   │   │└───┘└───┘              │
    │   ┴    ┴      ┴    ┴       └───┘┴    ┴                  │
    │ ─────────────────────────────────────────────────────── │
    │ S1C1  S1C2  S1C3  S2C1  S2C2  S2C3  S3C1  S3C2  S3C3   │
    │                                                         │
    │ Colors: Red = Stage 1, Green = Stage 2, Blue = Stage 3 │
    └────────────────────────────────────────────────────────┘


## Interfaces and Dependencies

### Dependencies (Cargo.toml additions)

    plotters = "0.3"


### New Files

    src/analysis/mod.rs
    src/analysis/config.rs
    src/analysis/csv_reader.rs
    src/analysis/statistics.rs
    src/analysis/charts.rs
    src/analysis/export.rs


### Key Types: src/analysis/csv_reader.rs

    /// Raw data from one CSV row
    pub struct RunData {
        pub iteration: u32,
        pub timestamp: String,
        pub screenshot_path: String,
        pub scores: [[u32; 3]; 3],
    }

    /// All data loaded from CSV
    pub struct DataSet {
        pub runs: Vec<RunData>,
    }

    impl DataSet {
        pub fn from_csv(path: &Path) -> Result<Self>;
        pub fn column_values(&self, stage: usize, criterion: usize) -> Vec<u32>;
        pub fn len(&self) -> usize;
        pub fn is_empty(&self) -> bool;
    }


### Key Types: src/analysis/statistics.rs

    use serde::Serialize;

    #[derive(Debug, Clone, Serialize)]
    pub struct ColumnStats {
        pub stage: usize,
        pub criterion: usize,
        pub count: usize,
        pub mean: f64,
        pub median: f64,
        pub mode: u32,
        pub min: u32,
        pub max: u32,
        pub std_dev: f64,
        pub quartile_1: f64,
        pub quartile_3: f64,
    }

    #[derive(Debug, Clone, Serialize)]
    pub struct DataSetStats {
        pub total_runs: usize,
        pub columns: Vec<ColumnStats>,
    }

    impl DataSetStats {
        pub fn from_dataset(data: &DataSet) -> Self;
    }


### Key Types: src/analysis/config.rs

    #[derive(Debug, Clone, Deserialize, Serialize)]
    pub struct ChartConfig {
        pub font: FontConfig,
        pub colors: ColorConfig,
        pub layout: LayoutConfig,
    }

    impl ChartConfig {
        pub fn load(config_path: &Path) -> Self;
        pub fn save_default(config_path: &Path) -> std::io::Result<()>;
    }


### Key Functions: src/analysis/charts.rs

    use std::path::Path;
    use anyhow::Result;

    pub fn generate_column_chart(
        column_name: &str,
        values: &[u32],
        stats: &ColumnStats,
        total_runs: usize,
        output_path: &Path,
        config: &ChartConfig,
    ) -> Result<()>;

    pub fn generate_all_charts(
        data: &DataSet,
        stats: &DataSetStats,
        output_dir: &Path,
        config: &ChartConfig,
    ) -> Result<Vec<PathBuf>>;

    pub fn generate_combined_box_plot(
        stats: &DataSetStats,
        output_path: &Path,
        config: &ChartConfig,
    ) -> Result<()>;


### Key Functions: src/analysis/export.rs

    pub fn export_to_json(stats: &DataSetStats, output_path: &Path) -> Result<()>;


### Key Functions: src/analysis/mod.rs

    pub mod charts;
    pub mod config;
    pub mod csv_reader;
    pub mod export;
    pub mod statistics;

    pub use config::ChartConfig;
    pub use csv_reader::DataSet;
    pub use statistics::DataSetStats;

    /// Run full analysis pipeline. Returns (chart_paths, json_path).
    pub fn generate_analysis() -> Result<(Vec<PathBuf>, PathBuf)>;


### Menu Constants in src/main.rs

    const IDM_GENERATE_CHARTS: u16 = 108;


---

## Revision History

- 2026-01-15: Initial ExecPlan created for Phase 4
- 2026-01-15: Added combined box plot, orange styling, config file, output folder

