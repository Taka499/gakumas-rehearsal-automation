//! Chart generation using plotters.
//!
//! Generates per-character charts with box plot, distribution histogram, and statistics table.
//! Styling is configurable via chart_config.json.

use super::config::ChartConfig;
use super::csv_reader::DataSet;
use super::statistics::ColumnStats;
use anyhow::{Context, Result};
use plotters::prelude::*;
use std::path::Path;

/// Minimum bucket size for histogram distribution.
const MIN_BUCKET_SIZE: u32 = 1000;

/// Calculate bucket size based on data range.
/// Formula: MIN_BUCKET_SIZE * max(floor((max - min) / MIN_BUCKET_SIZE / 100), 1)
fn calculate_bucket_size(min_score: u32, max_score: u32) -> u32 {
    let range = max_score.saturating_sub(min_score);
    let factor = (range / MIN_BUCKET_SIZE / 100).max(1);
    MIN_BUCKET_SIZE * factor
}

/// Build histogram buckets from raw values.
/// Returns (bucket_starts, counts) where bucket_starts[i] is the start of bucket i.
fn build_histogram(values: &[u32], bucket_size: u32) -> (Vec<u32>, Vec<u32>) {
    if values.is_empty() || bucket_size == 0 {
        return (vec![], vec![]);
    }

    let min_val = *values.iter().min().unwrap();
    let max_val = *values.iter().max().unwrap();

    // Align bucket start to bucket_size boundary
    let bucket_start = (min_val / bucket_size) * bucket_size;
    let bucket_end = ((max_val / bucket_size) + 1) * bucket_size;
    let num_buckets = ((bucket_end - bucket_start) / bucket_size) as usize;

    let mut counts = vec![0u32; num_buckets];

    for &val in values {
        let bucket_idx = ((val - bucket_start) / bucket_size) as usize;
        if bucket_idx < counts.len() {
            counts[bucket_idx] += 1;
        }
    }

    let bucket_starts: Vec<u32> = (0..num_buckets)
        .map(|i| bucket_start + (i as u32) * bucket_size)
        .collect();

    (bucket_starts, counts)
}

/// Generate a combined chart for a single column (stage/criterion).
/// Contains: statistics table (top), box plot (left), histogram (right).
pub fn generate_column_chart(
    column_name: &str,
    values: &[u32],
    stats: &ColumnStats,
    total_runs: usize,
    output_path: &Path,
    config: &ChartConfig,
) -> Result<()> {
    let root = BitMapBackend::new(
        output_path,
        (config.layout.chart_width, config.layout.chart_height),
    )
    .into_drawing_area();
    root.fill(&WHITE)
        .context("Failed to fill chart background")?;

    // Split into: title area, table area, chart area
    let (title_area, rest) = root.split_vertically(config.layout.title_height);
    let (table_area, chart_area) = rest.split_vertically(config.layout.table_height);

    // Draw title with sample count
    let title = format!("{} Distribution", column_name);
    let sample_count = format!("(n = {})", total_runs);
    let title_font = ("sans-serif", config.font.title_size)
        .into_font()
        .style(FontStyle::Bold);
    title_area.draw_text(&title, &title_font.color(&BLACK), (20, 10))?;
    // Draw sample count on the right side (same style as title)
    let sample_x = config.layout.chart_width as i32 - 180;
    title_area.draw_text(&sample_count, &title_font.color(&BLACK), (sample_x, 10))?;

    // Draw statistics table at top
    draw_stats_table_top(&table_area, stats, config)?;

    // Split chart area into box plot (left) and histogram (right)
    let (box_area, hist_area) = chart_area.split_horizontally(config.layout.box_plot_width);

    // Draw box plot
    draw_box_plot(&box_area, values, stats, config)?;

    // Draw histogram with legend
    draw_histogram(&hist_area, values, stats, total_runs, config)?;

    root.present().context("Failed to save chart")?;
    Ok(())
}

/// Draw box plot in the given area with configurable colors.
fn draw_box_plot(
    area: &DrawingArea<BitMapBackend, plotters::coord::Shift>,
    values: &[u32],
    stats: &ColumnStats,
    config: &ChartConfig,
) -> Result<()> {
    if values.is_empty() {
        return Ok(());
    }

    // Get colors from config
    let light_gray_bg = RGBColor(
        config.colors.light_gray_bg[0],
        config.colors.light_gray_bg[1],
        config.colors.light_gray_bg[2],
    );
    let grid_color = RGBColor(
        config.colors.grid_color[0],
        config.colors.grid_color[1],
        config.colors.grid_color[2],
    );
    let orange_primary = RGBColor(
        config.colors.orange_primary[0],
        config.colors.orange_primary[1],
        config.colors.orange_primary[2],
    );
    let orange_header = RGBColor(
        config.colors.orange_header[0],
        config.colors.orange_header[1],
        config.colors.orange_header[2],
    );

    // Fill background with light gray
    area.fill(&light_gray_bg)?;

    let min_val = stats.min as f64;
    let max_val = stats.max as f64;
    let range = max_val - min_val;
    let y_min = (min_val - range * 0.1).max(0.0);
    let y_max = max_val + range * 0.1;

    let mut chart = ChartBuilder::on(area)
        .caption(
            "Box Plot",
            ("sans-serif", config.font.box_plot_caption_size),
        )
        .margin(15)
        .x_label_area_size(30)
        .y_label_area_size(70)
        .build_cartesian_2d(0.0f64..2.0f64, y_min..y_max)
        .context("Failed to build box plot")?;

    chart
        .configure_mesh()
        .disable_x_mesh()
        .disable_x_axis()
        .y_desc("Score")
        .y_label_formatter(&|y| format!("{:.0}", y))
        .light_line_style(grid_color)
        .bold_line_style(grid_color.mix(0.8))
        .draw()
        .context("Failed to draw mesh")?;

    let x_center = 1.0;
    let box_width = 0.4;

    // Gray for whiskers
    let whisker_color = RGBColor(100, 100, 100);

    // Box fill (Q1 to Q3)
    chart.draw_series(std::iter::once(Rectangle::new(
        [
            (x_center - box_width, stats.quartile_1),
            (x_center + box_width, stats.quartile_3),
        ],
        orange_primary.mix(0.5).filled(),
    )))?;

    // Box outline
    chart.draw_series(std::iter::once(Rectangle::new(
        [
            (x_center - box_width, stats.quartile_1),
            (x_center + box_width, stats.quartile_3),
        ],
        orange_header.stroke_width(2),
    )))?;

    // Median line (darker orange)
    chart.draw_series(std::iter::once(PathElement::new(
        vec![
            (x_center - box_width, stats.median),
            (x_center + box_width, stats.median),
        ],
        orange_header.stroke_width(3),
    )))?;

    // Lower whisker
    chart.draw_series(std::iter::once(PathElement::new(
        vec![(x_center, min_val), (x_center, stats.quartile_1)],
        whisker_color.stroke_width(2),
    )))?;

    // Upper whisker
    chart.draw_series(std::iter::once(PathElement::new(
        vec![(x_center, stats.quartile_3), (x_center, max_val)],
        whisker_color.stroke_width(2),
    )))?;

    // Min cap
    let cap_width = 0.25;
    chart.draw_series(std::iter::once(PathElement::new(
        vec![(x_center - cap_width, min_val), (x_center + cap_width, min_val)],
        whisker_color.stroke_width(2),
    )))?;

    // Max cap
    chart.draw_series(std::iter::once(PathElement::new(
        vec![(x_center - cap_width, max_val), (x_center + cap_width, max_val)],
        whisker_color.stroke_width(2),
    )))?;

    Ok(())
}

/// Draw histogram in the given area with configurable colors and legend.
fn draw_histogram(
    area: &DrawingArea<BitMapBackend, plotters::coord::Shift>,
    values: &[u32],
    stats: &ColumnStats,
    total_runs: usize,
    config: &ChartConfig,
) -> Result<()> {
    if values.is_empty() {
        return Ok(());
    }

    // Get colors from config
    let light_gray_bg = RGBColor(
        config.colors.light_gray_bg[0],
        config.colors.light_gray_bg[1],
        config.colors.light_gray_bg[2],
    );
    let grid_color = RGBColor(
        config.colors.grid_color[0],
        config.colors.grid_color[1],
        config.colors.grid_color[2],
    );
    let orange_primary = RGBColor(
        config.colors.orange_primary[0],
        config.colors.orange_primary[1],
        config.colors.orange_primary[2],
    );
    let orange_header = RGBColor(
        config.colors.orange_header[0],
        config.colors.orange_header[1],
        config.colors.orange_header[2],
    );

    // Fill background with light gray
    area.fill(&light_gray_bg)?;

    let bucket_size = calculate_bucket_size(stats.min, stats.max);
    let (bucket_starts, counts) = build_histogram(values, bucket_size);

    if bucket_starts.is_empty() {
        return Ok(());
    }

    let max_count = *counts.iter().max().unwrap_or(&1);
    let x_min = bucket_starts[0] as f64;
    let x_max = (bucket_starts.last().unwrap() + bucket_size) as f64;

    let mut chart = ChartBuilder::on(area)
        .margin(15)
        .margin_top(40) // Extra space for legend
        .x_label_area_size(40)
        .y_label_area_size(50)
        .build_cartesian_2d(x_min..x_max, 0u32..(max_count + 1))
        .context("Failed to build histogram")?;

    chart
        .configure_mesh()
        .x_desc("Score")
        .y_desc("Count")
        .x_label_formatter(&|x| format!("{:.0}", x))
        .light_line_style(grid_color)
        .bold_line_style(grid_color.mix(0.8))
        .draw()
        .context("Failed to draw mesh")?;

    // Draw bars with orange color
    for (i, &start) in bucket_starts.iter().enumerate() {
        let count = counts[i];
        if count > 0 {
            chart.draw_series(std::iter::once(Rectangle::new(
                [
                    (start as f64, 0u32),
                    ((start + bucket_size) as f64, count),
                ],
                orange_primary.filled(),
            )))?;
            // Bar outline (slightly darker)
            chart.draw_series(std::iter::once(Rectangle::new(
                [
                    (start as f64, 0u32),
                    ((start + bucket_size) as f64, count),
                ],
                orange_header.stroke_width(1),
            )))?;
        }
    }

    // Draw legend at top
    let legend_text = format!("Score (n={})", total_runs);
    let legend_x = 200;
    let legend_y = 5;

    // Legend color box
    area.draw(&Rectangle::new(
        [(legend_x, legend_y), (legend_x + 20, legend_y + 14)],
        orange_primary.filled(),
    ))?;
    area.draw(&Rectangle::new(
        [(legend_x, legend_y), (legend_x + 20, legend_y + 14)],
        orange_header.stroke_width(1),
    ))?;

    // Legend text
    area.draw_text(
        &legend_text,
        &("sans-serif", config.font.legend_size).into_font().color(&BLACK),
        (legend_x + 25, legend_y),
    )?;

    Ok(())
}

/// Draw statistics table at top with configurable orange header style.
/// Shows: Min, Average, Median, Max
fn draw_stats_table_top(
    area: &DrawingArea<BitMapBackend, plotters::coord::Shift>,
    stats: &ColumnStats,
    config: &ChartConfig,
) -> Result<()> {
    // Get colors from config
    let orange_header = RGBColor(
        config.colors.orange_header[0],
        config.colors.orange_header[1],
        config.colors.orange_header[2],
    );
    let grid_color = RGBColor(
        config.colors.grid_color[0],
        config.colors.grid_color[1],
        config.colors.grid_color[2],
    );

    let (width, height) = area.dim_in_pixel();
    let col_width = width as i32 / 4;
    let header_height = config.layout.table_header_height;
    let value_height = height as i32 - header_height;

    let headers = ["Min", "Average", "Median", "Max"];
    let values = [
        format!("{}", stats.min),
        format!("{:.0}", stats.mean),
        format!("{:.0}", stats.median),
        format!("{}", stats.max),
    ];

    let header_font = ("sans-serif", config.font.table_header_size)
        .into_font()
        .style(FontStyle::Bold);
    let value_font = ("sans-serif", config.font.table_value_size)
        .into_font()
        .style(FontStyle::Bold);

    // Estimate character width based on font size
    let char_width = (config.font.table_header_size / 2) as i32;
    let value_char_width = (config.font.table_value_size / 2) as i32;

    for (i, (header, value)) in headers.iter().zip(values.iter()).enumerate() {
        let x_start = i as i32 * col_width;

        // Draw orange header background
        area.draw(&Rectangle::new(
            [(x_start, 0), (x_start + col_width, header_height)],
            orange_header.filled(),
        ))?;

        // Draw header border (right side)
        if i < 3 {
            area.draw(&PathElement::new(
                vec![
                    (x_start + col_width, 0),
                    (x_start + col_width, header_height),
                ],
                RGBColor(200, 100, 20).stroke_width(1),
            ))?;
        }

        // Draw header text (white, centered)
        let header_text_x = x_start + (col_width - header.len() as i32 * char_width) / 2;
        area.draw_text(header, &header_font.color(&WHITE), (header_text_x, 5))?;

        // Draw value background (white with border)
        area.draw(&Rectangle::new(
            [(x_start, header_height), (x_start + col_width, header_height + value_height)],
            WHITE.filled(),
        ))?;

        // Draw value cell border
        area.draw(&Rectangle::new(
            [(x_start, header_height), (x_start + col_width, header_height + value_height)],
            grid_color.stroke_width(1),
        ))?;

        // Draw value text (centered)
        let value_text_x = x_start + (col_width - value.len() as i32 * value_char_width) / 2;
        let value_text_y = header_height + (value_height - config.font.table_value_size as i32) / 2;
        area.draw_text(value, &value_font.color(&BLACK), (value_text_x, value_text_y))?;
    }

    Ok(())
}

/// Generate all column charts (9 charts total).
pub fn generate_all_charts(
    data: &DataSet,
    stats: &super::statistics::DataSetStats,
    output_dir: &Path,
    config: &ChartConfig,
) -> Result<Vec<std::path::PathBuf>> {
    let mut paths = Vec::new();

    for stage in 0..3 {
        for criterion in 0..3 {
            let idx = stage * 3 + criterion;
            let col_stats = &stats.columns[idx];
            let values = data.column_values(stage, criterion);
            let column_name = format!("S{}C{}", stage + 1, criterion + 1);
            let filename = format!("chart_{}.png", column_name.to_lowercase());
            let output_path = output_dir.join(&filename);

            generate_column_chart(
                &column_name,
                &values,
                col_stats,
                stats.total_runs,
                &output_path,
                config,
            )?;

            paths.push(output_path);
        }
    }

    Ok(paths)
}

/// Per-stage box colours shared by the on-disk combined plot and the live
/// in-memory plot (Stage 1 red, Stage 2 green, Stage 3 blue).
const STAGE_COLORS: [RGBColor; 3] = [
    RGBColor(220, 80, 80),
    RGBColor(80, 180, 80),
    RGBColor(80, 120, 200),
];
/// Whisker/cap line colour.
const WHISKER_COLOR: RGBColor = RGBColor(80, 80, 80);
/// Median line colour.
const MEDIAN_COLOR: RGBColor = RGBColor(200, 50, 50);
/// Box half-width and whisker-cap half-width, in x-axis units (one column = 1.0).
const BOX_HALF_WIDTH: f64 = 0.35;
const CAP_HALF_WIDTH: f64 = 0.2;

/// Stroke widths for one nine-box rendering. The live plot uses thicker
/// strokes than the on-disk plot because its 1200px image is downscaled to
/// the GUI panel width.
struct BoxStrokes {
    outline: u32,
    median: u32,
    whisker: u32,
}

/// Draws the nine box plots (box fill, outline, median, whiskers, caps) onto a
/// prepared chart whose x-range is 0.0..9.0 and whose y-range already spans the
/// data. Shared by `generate_combined_box_plot` (file) and
/// `render_live_box_plot_rgba` (in-memory RGBA); only stroke widths differ.
fn draw_nine_boxes<DB>(
    chart: &mut ChartContext<
        '_,
        DB,
        Cartesian2d<
            plotters::coord::types::RangedCoordf64,
            plotters::coord::types::RangedCoordf64,
        >,
    >,
    stats: &super::statistics::DataSetStats,
    strokes: BoxStrokes,
) -> Result<()>
where
    DB: DrawingBackend,
    DB::ErrorType: 'static,
{
    for (idx, col_stats) in stats.columns.iter().enumerate() {
        let stage = idx / 3;
        let x_center = idx as f64 + 0.5;
        let box_color = STAGE_COLORS[stage];

        let min_val = col_stats.min as f64;
        let max_val = col_stats.max as f64;

        // Box fill (Q1 to Q3)
        chart.draw_series(std::iter::once(Rectangle::new(
            [
                (x_center - BOX_HALF_WIDTH, col_stats.quartile_1),
                (x_center + BOX_HALF_WIDTH, col_stats.quartile_3),
            ],
            box_color.mix(0.4).filled(),
        )))?;

        // Box outline
        chart.draw_series(std::iter::once(Rectangle::new(
            [
                (x_center - BOX_HALF_WIDTH, col_stats.quartile_1),
                (x_center + BOX_HALF_WIDTH, col_stats.quartile_3),
            ],
            box_color.stroke_width(strokes.outline),
        )))?;

        // Median line
        chart.draw_series(std::iter::once(PathElement::new(
            vec![
                (x_center - BOX_HALF_WIDTH, col_stats.median),
                (x_center + BOX_HALF_WIDTH, col_stats.median),
            ],
            MEDIAN_COLOR.stroke_width(strokes.median),
        )))?;

        // Lower whisker
        chart.draw_series(std::iter::once(PathElement::new(
            vec![(x_center, min_val), (x_center, col_stats.quartile_1)],
            WHISKER_COLOR.stroke_width(strokes.whisker),
        )))?;

        // Upper whisker
        chart.draw_series(std::iter::once(PathElement::new(
            vec![(x_center, col_stats.quartile_3), (x_center, max_val)],
            WHISKER_COLOR.stroke_width(strokes.whisker),
        )))?;

        // Min cap
        chart.draw_series(std::iter::once(PathElement::new(
            vec![
                (x_center - CAP_HALF_WIDTH, min_val),
                (x_center + CAP_HALF_WIDTH, min_val),
            ],
            WHISKER_COLOR.stroke_width(strokes.whisker),
        )))?;

        // Max cap
        chart.draw_series(std::iter::once(PathElement::new(
            vec![
                (x_center - CAP_HALF_WIDTH, max_val),
                (x_center + CAP_HALF_WIDTH, max_val),
            ],
            WHISKER_COLOR.stroke_width(strokes.whisker),
        )))?;
    }

    Ok(())
}

/// Generate a combined box plot showing all 9 columns side by side.
pub fn generate_combined_box_plot(
    stats: &super::statistics::DataSetStats,
    output_path: &Path,
    _config: &ChartConfig, // Reserved for future use
) -> Result<()> {
    let root = BitMapBackend::new(output_path, (1200, 700)).into_drawing_area();
    root.fill(&WHITE)
        .context("Failed to fill chart background")?;

    // Find global min/max across all columns for Y-axis scaling
    let global_min = stats
        .columns
        .iter()
        .map(|c| c.min)
        .min()
        .unwrap_or(0) as f64;
    let global_max = stats
        .columns
        .iter()
        .map(|c| c.max)
        .max()
        .unwrap_or(100) as f64;

    let range = global_max - global_min;
    let y_min = (global_min - range * 0.05).max(0.0);
    let y_max = global_max + range * 0.05;

    let title = format!("Score Distribution ({} runs)", stats.total_runs);

    // Split into chart area and label area at bottom
    let (upper, lower) = root.split_vertically(650);

    let mut chart = ChartBuilder::on(&upper)
        .caption(&title, ("sans-serif", 24))
        .margin(20)
        .x_label_area_size(10)
        .y_label_area_size(80)
        .build_cartesian_2d(0.0f64..9.0f64, y_min..y_max)
        .context("Failed to build combined box plot")?;

    chart
        .configure_mesh()
        .disable_x_mesh()
        .disable_x_axis()
        .y_desc("Score")
        .y_label_formatter(&|y| format!("{:.0}", y))
        .draw()
        .context("Failed to draw mesh")?;

    // Draw X-axis labels manually
    let labels = ["S1C1", "S1C2", "S1C3", "S2C1", "S2C2", "S2C3", "S3C1", "S3C2", "S3C3"];
    let label_font = ("sans-serif", 16).into_font();
    let chart_left = 80; // Match y_label_area_size
    let chart_width = 1200 - chart_left - 20; // Total width minus margins
    let box_width = chart_width as f64 / 9.0;

    for (idx, label) in labels.iter().enumerate() {
        let x_pos = chart_left + (idx as i32 * chart_width / 9) + (box_width as i32 / 2) - 15;
        lower.draw_text(label, &label_font.color(&BLACK), (x_pos, 5))?;
    }

    draw_nine_boxes(
        &mut chart,
        stats,
        BoxStrokes {
            outline: 2,
            median: 2,
            whisker: 1,
        },
    )?;

    root.present().context("Failed to save combined box plot")?;
    Ok(())
}

/// Canvas dimensions for the live in-run distribution figure. The figure shows only
/// the nine box plots (no per-column statistics text — those are rendered as a live
/// table in the GUI instead), so it is shorter than the on-disk combined plot.
const LIVE_PLOT_W: u32 = 1200;
const LIVE_PLOT_H: u32 = 620;
/// Y of the split between the box-plot area and the column-label strip.
const LIVE_PLOT_SPLIT_Y: u32 = 550;

/// Render the combined nine-box distribution into an RGBA8 pixel buffer.
///
/// Returns `(width, height, rgba_bytes)` with `rgba_bytes.len() == width*height*4`,
/// ready for `egui::ColorImage::from_rgba_unmultiplied`. No file is written. This
/// mirrors `generate_combined_box_plot` (same colors and box geometry) but targets an
/// in-memory buffer for the live GUI view. Per-column statistics are NOT drawn here;
/// the GUI shows them as a separate live-updating table (see `render_live_stats_table`).
pub fn render_live_box_plot_rgba(
    stats: &super::statistics::DataSetStats,
) -> Result<(u32, u32, Vec<u8>)> {
    // plotters BitMapBackend writes RGB (3 bytes/pixel) into this buffer.
    let mut rgb = vec![0u8; (LIVE_PLOT_W * LIVE_PLOT_H * 3) as usize];

    {
        let root =
            BitMapBackend::with_buffer(&mut rgb, (LIVE_PLOT_W, LIVE_PLOT_H)).into_drawing_area();
        root.fill(&WHITE)
            .context("Failed to fill live chart background")?;

        // Global Y range across all columns (same approach as the on-disk figure).
        let global_min = stats.columns.iter().map(|c| c.min).min().unwrap_or(0) as f64;
        let global_max = stats.columns.iter().map(|c| c.max).max().unwrap_or(100) as f64;
        let range = (global_max - global_min).max(1.0);
        let y_min = (global_min - range * 0.05).max(0.0);
        let y_max = global_max + range * 0.05;

        let (upper, lower) = root.split_vertically(LIVE_PLOT_SPLIT_Y);

        let mut chart = ChartBuilder::on(&upper)
            .margin(20)
            .x_label_area_size(10)
            .y_label_area_size(80)
            .build_cartesian_2d(0.0f64..9.0f64, y_min..y_max)
            .context("Failed to build live box plot")?;

        // Fonts are sized generously because the 1200px-wide image is scaled down to
        // the panel width on screen; larger fonts here stay legible after downscaling.
        // Y-axis ticks use the same "k" abbreviation as the table.
        chart
            .configure_mesh()
            .disable_x_mesh()
            .disable_x_axis()
            .y_desc("Score")
            .axis_desc_style(("sans-serif", 22))
            .label_style(("sans-serif", 22))
            .y_label_formatter(&|y| {
                if *y >= 1000.0 {
                    format!("{}k", (y / 1000.0).round() as i64)
                } else {
                    format!("{}", y.round() as i64)
                }
            })
            .draw()
            .context("Failed to draw live mesh")?;

        let labels = [
            "S1C1", "S1C2", "S1C3", "S2C1", "S2C2", "S2C3", "S3C1", "S3C2", "S3C3",
        ];
        let label_font = ("sans-serif", 28, FontStyle::Bold).into_font();
        let chart_left = 80i32; // Match y_label_area_size
        let chart_width = LIVE_PLOT_W as i32 - chart_left - 20; // Total width minus margins
        let box_width_px = chart_width as f64 / 9.0;

        draw_nine_boxes(
            &mut chart,
            stats,
            BoxStrokes {
                outline: 3,
                median: 3,
                whisker: 2,
            },
        )?;

        // Column labels (S1C1 ..) in the lower strip, centered under each box. The
        // lower area's local origin (0,0) is its top-left; the box area sits above it.
        for (idx, label) in labels.iter().enumerate() {
            // Offset left by ~half the (bold, 28px) label width to center under the box.
            let label_x =
                chart_left + (idx as i32 * chart_width / 9) + (box_width_px as i32 / 2) - 38;
            lower.draw_text(label, &label_font.color(&BLACK), (label_x, 12))?;
        }

        root.present().context("Failed to render live box plot")?;
    }

    // plotters wrote RGB; egui wants RGBA. Expand with opaque alpha.
    let mut rgba = Vec::with_capacity((LIVE_PLOT_W * LIVE_PLOT_H * 4) as usize);
    for px in rgb.chunks_exact(3) {
        rgba.extend_from_slice(px);
        rgba.push(255);
    }

    Ok((LIVE_PLOT_W, LIVE_PLOT_H, rgba))
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::statistics::DataSetStats;

    fn sample_rows() -> Vec<[[u32; 3]; 3]> {
        // A handful of rows with distinct per-column values so quartiles are non-trivial.
        vec![
            [[100, 200, 300], [400, 500, 600], [700, 800, 900]],
            [[110, 210, 310], [410, 510, 610], [710, 810, 910]],
            [[120, 220, 320], [420, 520, 620], [720, 820, 920]],
            [[130, 230, 330], [430, 530, 630], [730, 830, 930]],
            [[140, 240, 340], [440, 540, 640], [740, 840, 940]],
        ]
    }

    #[test]
    fn render_live_box_plot_returns_rgba_buffer() {
        let stats = DataSetStats::from_score_rows(&sample_rows());
        let (w, h, rgba) = render_live_box_plot_rgba(&stats).unwrap();
        assert_eq!(w, LIVE_PLOT_W);
        assert_eq!(h, LIVE_PLOT_H);
        assert_eq!(rgba.len(), (w * h * 4) as usize);
        // Every 4th byte is the opaque alpha we appended.
        assert!(rgba.chunks_exact(4).all(|px| px[3] == 255));
    }

    #[test]
    fn render_live_box_plot_handles_empty() {
        // Zero usable rows must not panic (early-run case).
        let stats = DataSetStats::from_score_rows(&[]);
        let (w, h, rgba) = render_live_box_plot_rgba(&stats).unwrap();
        assert_eq!(rgba.len(), (w * h * 4) as usize);
    }

    // Eyeball preview: writes the live figure to temp/live_box_plot_preview.png.
    // Run with: GAKUMAS_NO_MANIFEST=1 cargo test live_box_plot_preview -- --ignored
    #[test]
    #[ignore]
    fn live_box_plot_preview() {
        let stats = DataSetStats::from_score_rows(&sample_rows());
        let (w, h, rgba) = render_live_box_plot_rgba(&stats).unwrap();
        let img = image::RgbaImage::from_raw(w, h, rgba).expect("buffer size matches dimensions");
        std::fs::create_dir_all("temp").ok();
        img.save("temp/live_box_plot_preview.png")
            .expect("write preview png");
    }

    #[test]
    fn test_calculate_bucket_size() {
        // Small range: should use MIN_BUCKET_SIZE
        assert_eq!(calculate_bucket_size(0, 50000), 1000);

        // Medium range
        assert_eq!(calculate_bucket_size(0, 150000), 1000);

        // Large range
        assert_eq!(calculate_bucket_size(0, 300000), 3000);
    }

    #[test]
    fn test_build_histogram() {
        let values = vec![1000, 1500, 2000, 2500, 3000, 3500];
        let bucket_size = 1000;
        let (starts, counts) = build_histogram(&values, bucket_size);

        assert_eq!(starts, vec![1000, 2000, 3000]);
        assert_eq!(counts, vec![2, 2, 2]);
    }

    #[test]
    fn test_build_histogram_empty() {
        let values: Vec<u32> = vec![];
        let (starts, counts) = build_histogram(&values, 1000);
        assert!(starts.is_empty());
        assert!(counts.is_empty());
    }
}
