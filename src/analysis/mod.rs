//! Statistics calculation and chart visualization.
//!
//! This module provides:
//! - CSV reading for automation results
//! - Statistics calculation (mean, median, mode, std_dev, quartiles)
//! - Per-character charts with box plot, histogram, and statistics table
//! - JSON export of statistics
//! - Configurable chart styling via chart_config.json

pub mod charts;
pub mod config;
pub mod csv_reader;
pub mod export;
pub mod statistics;

pub use config::ChartConfig;
pub use csv_reader::DataSet;
pub use statistics::DataSetStats;

use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

/// Runs the full analysis pipeline for a session folder.
///
/// Reads results.csv from the session folder, generates charts in a charts/ subfolder,
/// and exports statistics.json to the session folder.
///
/// Returns (chart_paths, json_path) where chart_paths contains per-column PNGs plus combined box plot.
pub fn generate_analysis_for_session(session_dir: &Path) -> Result<(Vec<PathBuf>, PathBuf)> {
    let csv_path = session_dir.join("results.csv");
    let charts_dir = session_dir.join("charts");
    let json_path = session_dir.join("statistics.json");
    let combined_chart_path = charts_dir.join("chart_combined.png");
    let config_path = crate::paths::get_exe_dir().join("chart_config.json");

    // Create charts directory if it doesn't exist
    if !charts_dir.exists() {
        std::fs::create_dir_all(&charts_dir)?;
        crate::log(&format!("Created charts directory: {}", crate::paths::relative_display(&charts_dir)));
    }

    // Load chart config (creates default if not exists)
    let config = config::ChartConfig::load(&config_path);

    // Save default config if it doesn't exist (for reference)
    if !config_path.exists() {
        if let Err(e) = config::ChartConfig::save_default(&config_path) {
            crate::log(&format!("Failed to save default chart config: {}", e));
        } else {
            crate::log(&format!(
                "Created default chart_config.json at {}",
                crate::paths::relative_display(&config_path)
            ));
        }
    }

    // Load data
    let data = csv_reader::DataSet::from_csv(&csv_path)?;
    if data.is_empty() {
        return Err(anyhow!("No data in CSV file"));
    }

    crate::log(&format!("Loaded {} runs from CSV", data.len()));

    // Calculate statistics
    let stats = statistics::DataSetStats::from_dataset(&data);

    // Generate per-column charts (9 charts total)
    let mut chart_paths = charts::generate_all_charts(&data, &stats, &charts_dir, &config)?;
    crate::log(&format!("Generated {} per-column charts", chart_paths.len()));

    // Generate combined box plot
    charts::generate_combined_box_plot(&stats, &combined_chart_path, &config)?;
    chart_paths.push(combined_chart_path.clone());
    crate::log(&format!(
        "Generated combined box plot: {}",
        crate::paths::relative_display(&combined_chart_path)
    ));

    // Export JSON
    export::export_to_json(&stats, &json_path)?;
    crate::log(&format!("Statistics JSON saved: {}", crate::paths::relative_display(&json_path)));

    Ok((chart_paths, json_path))
}

/// Runs the full analysis pipeline using the current session folder.
///
/// This is a convenience function that gets the session path from the runner.
/// Falls back to legacy behavior if no session is active.
pub fn generate_analysis() -> Result<(Vec<PathBuf>, PathBuf)> {
    // Try to use current session path from runner
    if let Some(session_dir) = crate::automation::runner::get_progress().session_path {
        return generate_analysis_for_session(&session_dir);
    }

    // Fallback: legacy behavior for backward compatibility
    let exe_dir = crate::paths::get_exe_dir();
    let csv_path = exe_dir.join("results.csv");

    if !csv_path.exists() {
        return Err(anyhow!("No results.csv found. Run automation first."));
    }

    // Use output/ directory for legacy mode
    let output_dir = exe_dir.join("output");
    let json_path = output_dir.join("statistics.json");
    let combined_chart_path = output_dir.join("chart_combined.png");
    let config_path = exe_dir.join("chart_config.json");

    // Create output directory if it doesn't exist
    if !output_dir.exists() {
        std::fs::create_dir_all(&output_dir)?;
        crate::log(&format!("Created output directory: {}", crate::paths::relative_display(&output_dir)));
    }

    // Load chart config
    let config = config::ChartConfig::load(&config_path);

    if !config_path.exists() {
        if let Err(e) = config::ChartConfig::save_default(&config_path) {
            crate::log(&format!("Failed to save default chart config: {}", e));
        }
    }

    // Load data
    let data = csv_reader::DataSet::from_csv(&csv_path)?;
    if data.is_empty() {
        return Err(anyhow!("No data in CSV file"));
    }

    crate::log(&format!("Loaded {} runs from CSV (legacy mode)", data.len()));

    // Calculate statistics
    let stats = statistics::DataSetStats::from_dataset(&data);

    // Generate charts
    let mut chart_paths = charts::generate_all_charts(&data, &stats, &output_dir, &config)?;
    charts::generate_combined_box_plot(&stats, &combined_chart_path, &config)?;
    chart_paths.push(combined_chart_path.clone());

    // Export JSON
    export::export_to_json(&stats, &json_path)?;

    Ok((chart_paths, json_path))
}
