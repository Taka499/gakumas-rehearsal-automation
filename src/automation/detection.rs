//! Loading state detection via histogram comparison and brightness analysis.
//!
//! This module provides functions to detect when the game finishes loading
//! by monitoring screen regions using:
//! - Histogram comparison: Detect when Skip button appears (matches reference image)
//! - Brightness analysis: Detect when Skip button becomes enabled (not dimmed)
//!
//! The histogram comparison is resolution-independent: captured regions are resized
//! to match the reference image dimensions before comparison.

use anyhow::{anyhow, Result};
use image::{ImageBuffer, Rgba};
use std::path::Path;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;

use crate::automation::config::{AutomationConfig, RelativeRect};
use crate::automation::input::click_at_relative;
use crate::capture::region::capture_region;

/// Calculates the average brightness (luminance) of an image.
///
/// Uses the ITU-R BT.601 luma formula: Y = 0.299*R + 0.587*G + 0.114*B
/// Returns a value from 0.0 (black) to 255.0 (white).
pub fn calculate_brightness(img: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> f32 {
    if img.width() == 0 || img.height() == 0 {
        return 0.0;
    }

    let mut total: f64 = 0.0;
    let pixel_count = (img.width() * img.height()) as f64;

    for pixel in img.pixels() {
        let r = pixel[0] as f64;
        let g = pixel[1] as f64;
        let b = pixel[2] as f64;
        let luminance = 0.299 * r + 0.587 * g + 0.114 * b;
        total += luminance;
    }

    (total / pixel_count) as f32
}

/// Calculates a grayscale histogram for an image.
///
/// Returns an array of 256 bins representing the distribution of pixel intensities.
/// Each bin is normalized to [0.0, 1.0] range.
fn calculate_histogram(img: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> [f32; 256] {
    let mut histogram = [0u32; 256];
    let pixel_count = (img.width() * img.height()) as f32;

    if pixel_count == 0.0 {
        return [0.0; 256];
    }

    for pixel in img.pixels() {
        // Convert to grayscale using luminance formula
        let gray = (0.299 * pixel[0] as f32
            + 0.587 * pixel[1] as f32
            + 0.114 * pixel[2] as f32) as u8;
        histogram[gray as usize] += 1;
    }

    // Normalize to [0.0, 1.0]
    let mut normalized = [0.0f32; 256];
    for i in 0..256 {
        normalized[i] = histogram[i] as f32 / pixel_count;
    }
    normalized
}

/// Calculates histogram similarity using Bhattacharyya coefficient.
///
/// Returns a value from 0.0 (completely different) to 1.0 (identical).
/// This metric is robust to lighting changes and works well for template matching.
fn histogram_similarity(hist1: &[f32; 256], hist2: &[f32; 256]) -> f32 {
    let mut bc = 0.0f32;
    for i in 0..256 {
        bc += (hist1[i] * hist2[i]).sqrt();
    }
    bc
}

/// Reference image data including histogram and dimensions for resolution-independent matching.
pub struct ReferenceImage {
    /// Normalized grayscale histogram (256 bins)
    pub histogram: [f32; 256],
    /// Original image dimensions (width, height)
    pub dimensions: (u32, u32),
}

/// Information needed to retry a click on the *previous* button during detection polling.
///
/// When a detection function polls for a new page element, it can optionally check
/// whether the previous button is still visible and retry the click if needed.
/// This eliminates the blocking 800ms verification wait after each click.
pub struct ClickRetryInfo<'a> {
    /// Window handle for clicking and capturing
    pub hwnd: HWND,
    /// Relative X position of the button to retry
    pub button_x: f32,
    /// Relative Y position of the button to retry
    pub button_y: f32,
    /// Region to capture for similarity check
    pub button_region: &'a RelativeRect,
    /// Reference image to compare against
    pub ref_img: &'a ReferenceImage,
    /// Similarity threshold above which the button is considered still visible
    pub histogram_threshold: f32,
    /// Maximum number of retry clicks allowed
    pub max_retries: u32,
}

/// Checks if the previous button is still visible and retries the click if needed.
///
/// Returns `true` if a retry click was performed, `false` otherwise.
fn maybe_retry_click(
    info: &ClickRetryInfo<'_>,
    elapsed_since_click: Duration,
    retries_used: &mut u32,
) -> bool {
    // Only check after 2 seconds have passed since the last click
    if elapsed_since_click < Duration::from_secs(2) {
        return false;
    }

    // No retries remaining
    if *retries_used >= info.max_retries {
        return false;
    }

    // Check if previous button is still visible
    match check_button_similarity(info.hwnd, info.button_region, info.ref_img) {
        Ok(similarity) => {
            if similarity >= info.histogram_threshold {
                *retries_used += 1;
                crate::log(&format!(
                    "Previous button still visible (similarity = {:.3}), retry click {}/{}",
                    similarity, *retries_used, info.max_retries
                ));
                if let Err(e) = click_at_relative(info.hwnd, info.button_x, info.button_y) {
                    crate::log(&format!("Warning: Retry click failed: {}", e));
                }
                true
            } else {
                crate::log(&format!(
                    "Previous button gone (similarity = {:.3}), no retry needed",
                    similarity
                ));
                false
            }
        }
        Err(e) => {
            crate::log(&format!(
                "Warning: Could not check previous button: {}", e
            ));
            false
        }
    }
}

/// Loads a reference image from disk and returns its histogram and dimensions.
///
/// The dimensions are stored so captured regions can be resized to match,
/// enabling resolution-independent histogram comparison.
pub fn load_reference_histogram(path: &Path) -> Result<ReferenceImage> {
    let img = image::open(path)
        .map_err(|e| anyhow!("Failed to load reference image {}: {}", path.display(), e))?
        .to_rgba8();
    let dimensions = (img.width(), img.height());
    Ok(ReferenceImage {
        histogram: calculate_histogram(&img),
        dimensions,
    })
}

/// Resizes an image to target dimensions for resolution-independent comparison.
fn resize_to_match(img: &ImageBuffer<Rgba<u8>, Vec<u8>>, target_width: u32, target_height: u32) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    if img.width() == target_width && img.height() == target_height {
        return img.clone();
    }

    // Use image crate's resize with triangle (bilinear) filter for speed
    let resized = image::imageops::resize(
        img,
        target_width,
        target_height,
        image::imageops::FilterType::Triangle,
    );
    resized
}

/// Saves the current start button region as a reference image.
pub fn save_start_button_reference(hwnd: HWND, config: &AutomationConfig, path: &Path) -> Result<()> {
    let region_img = capture_region(hwnd, &config.start_button_region)?;
    region_img.save(path)
        .map_err(|e| anyhow!("Failed to save reference image: {}", e))?;
    crate::log(&format!("Saved Start button reference to {}", crate::paths::relative_display(path)));
    Ok(())
}

/// Saves the current skip button region as a reference image.
pub fn save_skip_button_reference(hwnd: HWND, config: &AutomationConfig, path: &Path) -> Result<()> {
    let region_img = capture_region(hwnd, &config.skip_button_region)?;
    region_img.save(path)
        .map_err(|e| anyhow!("Failed to save reference image: {}", e))?;
    crate::log(&format!("Saved Skip button reference to {}", crate::paths::relative_display(path)));
    Ok(())
}

/// Saves the current end button region as a reference image.
pub fn save_end_button_reference(hwnd: HWND, config: &AutomationConfig, path: &Path) -> Result<()> {
    let region_img = capture_region(hwnd, &config.end_button_region)?;
    region_img.save(path)
        .map_err(|e| anyhow!("Failed to save reference image: {}", e))?;
    crate::log(&format!("Saved End button reference to {}", crate::paths::relative_display(path)));
    Ok(())
}

/// Waits for loading to complete using two-phase detection.
///
/// Phase 1: Wait for Skip button to appear (histogram matches reference image)
/// Phase 2: Wait for Skip button to become enabled (brightness exceeds threshold)
///
/// If no reference image exists, falls back to brightness-only detection.
///
/// `abort` is polled inside the wait loops; when it returns true the wait ends
/// with an error (injected by the caller so this module has no dependency on
/// the global abort flag).
pub fn wait_for_loading(
    hwnd: HWND,
    config: &AutomationConfig,
    click_retry: Option<ClickRetryInfo<'_>>,
    abort: &dyn Fn() -> bool,
) -> Result<()> {
    let start = Instant::now();
    let timeout = Duration::from_millis(config.loading_timeout_ms);
    let mut retries_used: u32 = 0;
    let last_click_time = Instant::now();

    // Try to load reference histogram
    let ref_path = crate::paths::get_exe_dir().join(&config.skip_button_reference);

    let reference = if ref_path.exists() {
        match load_reference_histogram(&ref_path) {
            Ok(ref_img) => {
                crate::log(&format!(
                    "Loaded Skip button reference from {} ({}x{})",
                    crate::paths::relative_display(&ref_path),
                    ref_img.dimensions.0,
                    ref_img.dimensions.1
                ));
                Some(ref_img)
            }
            Err(e) => {
                crate::log(&format!(
                    "Warning: Failed to load reference image: {}. Using brightness-only detection.",
                    e
                ));
                None
            }
        }
    } else {
        crate::log(&format!(
            "Warning: Reference image {} not found. Using brightness-only detection.",
            crate::paths::relative_display(&ref_path)
        ));
        crate::log("Hint: Use 'Capture Skip Reference' from tray menu to create it.");
        None
    };

    // Phase 1: Wait for Skip button to appear (if reference exists)
    if let Some(ref ref_img) = reference {
        crate::log("Phase 1: Waiting for Skip button to appear...");
        let confirm_needed = config.detection_confirm_count.max(1);
        let mut consecutive_matches: u32 = 0;
        loop {
            if abort() {
                return Err(anyhow!("Abort requested"));
            }

            if start.elapsed() > timeout {
                return Err(anyhow!(
                    "Timeout waiting for Skip button (phase 1) after {}ms",
                    config.loading_timeout_ms
                ));
            }

            let region_img = capture_region(hwnd, &config.skip_button_region)?;
            // Resize to match reference dimensions for resolution-independent comparison
            let resized = resize_to_match(&region_img, ref_img.dimensions.0, ref_img.dimensions.1);
            let current_hist = calculate_histogram(&resized);
            let similarity = histogram_similarity(&ref_img.histogram, &current_hist);

            if similarity >= config.histogram_threshold {
                consecutive_matches += 1;
                crate::log(&format!(
                    "Phase 1: similarity = {:.3} - match {}/{} (threshold = {:.3})",
                    similarity, consecutive_matches, confirm_needed, config.histogram_threshold
                ));
                if consecutive_matches >= confirm_needed {
                    crate::log("Skip button detected (histogram match confirmed)");
                    break;
                }
            } else {
                if consecutive_matches > 0 {
                    crate::log(&format!(
                        "Phase 1: similarity = {:.3} - match streak reset (threshold = {:.3})",
                        similarity, config.histogram_threshold
                    ));
                } else {
                    crate::log(&format!(
                        "Phase 1: similarity = {:.3} (threshold = {:.3})",
                        similarity, config.histogram_threshold
                    ));
                }
                consecutive_matches = 0;

                // Retry previous button click if needed (only in Phase 1)
                if let Some(ref retry_info) = click_retry {
                    maybe_retry_click(retry_info, last_click_time.elapsed(), &mut retries_used);
                }
            }

            std::thread::sleep(Duration::from_millis(100));
        }
    }

    // Phase 2: Wait for Skip button to become enabled (brightness)
    crate::log("Phase 2: Waiting for Skip button to become enabled...");
    loop {
        if abort() {
            return Err(anyhow!("Abort requested"));
        }

        if start.elapsed() > timeout {
            return Err(anyhow!(
                "Timeout waiting for Skip enabled (phase 2) after {}ms",
                config.loading_timeout_ms
            ));
        }

        let region_img = capture_region(hwnd, &config.skip_button_region)?;
        let brightness = calculate_brightness(&region_img);

        crate::log(&format!(
            "Phase 2: brightness = {:.2} (threshold = {:.2})",
            brightness, config.brightness_threshold
        ));

        if brightness > config.brightness_threshold {
            crate::log("Skip button enabled (brightness exceeded threshold)");
            return Ok(());
        }

        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Captures a region and returns its brightness value.
///
/// This is a convenience function for calibration - it captures the specified
/// region and calculates its brightness without any threshold checking.
pub fn measure_region_brightness(hwnd: HWND, config: &AutomationConfig) -> Result<f32> {
    let region_img = capture_region(hwnd, &config.skip_button_region)?;
    Ok(calculate_brightness(&region_img))
}

/// Waits for the result page to appear by detecting the "終了" (End) button.
///
/// Uses histogram comparison against a reference image of the End button region.
/// Returns Ok(()) when the End button is detected, or Err on timeout or abort.
///
/// If no reference image exists, falls back to a fixed delay.
///
/// `abort` is polled inside the wait loop; see `wait_for_loading`.
pub fn wait_for_result(
    hwnd: HWND,
    config: &AutomationConfig,
    click_retry: Option<ClickRetryInfo<'_>>,
    abort: &dyn Fn() -> bool,
) -> Result<()> {
    let start = Instant::now();
    let timeout = Duration::from_millis(config.result_timeout_ms);
    let mut retries_used: u32 = 0;
    let last_click_time = Instant::now();

    // Try to load reference histogram
    let ref_path = crate::paths::get_exe_dir().join(&config.end_button_reference);

    let reference = if ref_path.exists() {
        match load_reference_histogram(&ref_path) {
            Ok(ref_img) => {
                crate::log(&format!(
                    "Loaded End button reference from {} ({}x{})",
                    crate::paths::relative_display(&ref_path),
                    ref_img.dimensions.0,
                    ref_img.dimensions.1
                ));
                Some(ref_img)
            }
            Err(e) => {
                crate::log(&format!(
                    "Warning: Failed to load End button reference: {}. Using fixed delay.",
                    e
                ));
                None
            }
        }
    } else {
        crate::log(&format!(
            "Warning: End button reference {} not found. Using fixed delay.",
            crate::paths::relative_display(&ref_path)
        ));
        crate::log("Hint: Use 'Capture End Reference' from tray menu to create it.");
        None
    };

    // If no reference, use fixed delay fallback
    if reference.is_none() {
        crate::log(&format!(
            "Waiting {} ms for result page (no reference image)...",
            config.capture_delay_ms
        ));
        std::thread::sleep(Duration::from_millis(config.capture_delay_ms));
        return Ok(());
    }

    let ref_img = reference.unwrap();

    // Wait for End button to appear (histogram comparison)
    crate::log("Waiting for End button to appear (result page)...");
    let confirm_needed = config.detection_confirm_count.max(1);
    let mut consecutive_matches: u32 = 0;
    loop {
        if abort() {
            return Err(anyhow!("Abort requested"));
        }

        if start.elapsed() > timeout {
            return Err(anyhow!(
                "Timeout waiting for result page after {}ms",
                config.result_timeout_ms
            ));
        }

        let region_img = capture_region(hwnd, &config.end_button_region)?;
        // Resize to match reference dimensions for resolution-independent comparison
        let resized = resize_to_match(&region_img, ref_img.dimensions.0, ref_img.dimensions.1);
        let current_hist = calculate_histogram(&resized);
        let similarity = histogram_similarity(&ref_img.histogram, &current_hist);

        if similarity >= config.histogram_threshold {
            consecutive_matches += 1;
            crate::log(&format!(
                "Result page detection: similarity = {:.3} - match {}/{} (threshold = {:.3})",
                similarity, consecutive_matches, confirm_needed, config.histogram_threshold
            ));
            if consecutive_matches >= confirm_needed {
                crate::log("End button detected (result page loaded, confirmed)");
                return Ok(());
            }
        } else {
            if consecutive_matches > 0 {
                crate::log(&format!(
                    "Result page detection: similarity = {:.3} - match streak reset (threshold = {:.3})",
                    similarity, config.histogram_threshold
                ));
            } else {
                crate::log(&format!(
                    "Result page detection: similarity = {:.3} (threshold = {:.3})",
                    similarity, config.histogram_threshold
                ));
            }
            consecutive_matches = 0;

            // Retry previous button click if needed
            if let Some(ref retry_info) = click_retry {
                maybe_retry_click(retry_info, last_click_time.elapsed(), &mut retries_used);
            }
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Waits for the rehearsal start page to appear by detecting the "開始する" (Start) button.
///
/// Uses histogram comparison against a reference image of the Start button region.
/// Returns Ok(()) when the Start button is detected, or Err on timeout or abort.
///
/// If no reference image exists, returns immediately (assumes page is ready).
///
/// `abort` is polled inside the wait loop; see `wait_for_loading`.
pub fn wait_for_start_page(
    hwnd: HWND,
    config: &AutomationConfig,
    click_retry: Option<ClickRetryInfo<'_>>,
    abort: &dyn Fn() -> bool,
) -> Result<()> {
    let start = Instant::now();
    let timeout = Duration::from_millis(config.loading_timeout_ms);
    let mut retries_used: u32 = 0;
    let last_click_time = Instant::now();

    // Try to load reference histogram
    let ref_path = crate::paths::get_exe_dir().join(&config.start_button_reference);

    let reference = if ref_path.exists() {
        match load_reference_histogram(&ref_path) {
            Ok(ref_img) => {
                crate::log(&format!(
                    "Loaded Start button reference from {} ({}x{})",
                    crate::paths::relative_display(&ref_path),
                    ref_img.dimensions.0,
                    ref_img.dimensions.1
                ));
                Some(ref_img)
            }
            Err(e) => {
                crate::log(&format!(
                    "Warning: Failed to load Start button reference: {}. Skipping page detection.",
                    e
                ));
                None
            }
        }
    } else {
        crate::log(&format!(
            "Warning: Start button reference {} not found. Skipping page detection.",
            crate::paths::relative_display(&ref_path)
        ));
        crate::log("Hint: Use 'Capture Start Reference' from tray menu to create it.");
        None
    };

    // If no reference, skip detection (assume we're on the right page)
    if reference.is_none() {
        return Ok(());
    }

    let ref_img = reference.unwrap();

    // Wait for Start button to appear (histogram comparison)
    crate::log("Waiting for Start button to appear (rehearsal page)...");
    let confirm_needed = config.detection_confirm_count.max(1);
    let mut consecutive_matches: u32 = 0;
    loop {
        if abort() {
            return Err(anyhow!("Abort requested"));
        }

        if start.elapsed() > timeout {
            return Err(anyhow!(
                "Timeout waiting for rehearsal page after {}ms",
                config.loading_timeout_ms
            ));
        }

        let region_img = capture_region(hwnd, &config.start_button_region)?;
        // Resize to match reference dimensions for resolution-independent comparison
        let resized = resize_to_match(&region_img, ref_img.dimensions.0, ref_img.dimensions.1);
        let current_hist = calculate_histogram(&resized);
        let similarity = histogram_similarity(&ref_img.histogram, &current_hist);

        if similarity >= config.histogram_threshold {
            consecutive_matches += 1;
            crate::log(&format!(
                "Rehearsal page detection: similarity = {:.3} - match {}/{} (threshold = {:.3})",
                similarity, consecutive_matches, confirm_needed, config.histogram_threshold
            ));
            if consecutive_matches >= confirm_needed {
                crate::log("Start button detected (rehearsal page loaded, confirmed)");
                return Ok(());
            }
        } else {
            if consecutive_matches > 0 {
                crate::log(&format!(
                    "Rehearsal page detection: similarity = {:.3} - match streak reset (threshold = {:.3})",
                    similarity, config.histogram_threshold
                ));
            } else {
                crate::log(&format!(
                    "Rehearsal page detection: similarity = {:.3} (threshold = {:.3})",
                    similarity, config.histogram_threshold
                ));
            }
            consecutive_matches = 0;

            // Retry previous button click if needed
            if let Some(ref retry_info) = click_retry {
                maybe_retry_click(retry_info, last_click_time.elapsed(), &mut retries_used);
            }
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Checks the current histogram similarity of a button region against a reference image.
///
/// Returns the similarity score (0.0 to 1.0). Useful for post-click verification
/// to check whether a button has disappeared from screen.
pub fn check_button_similarity(
    hwnd: HWND,
    region: &RelativeRect,
    ref_img: &ReferenceImage,
) -> Result<f32> {
    let region_img = capture_region(hwnd, region)?;
    let resized = resize_to_match(&region_img, ref_img.dimensions.0, ref_img.dimensions.1);
    let current_hist = calculate_histogram(&resized);
    Ok(histogram_similarity(&ref_img.histogram, &current_hist))
}
