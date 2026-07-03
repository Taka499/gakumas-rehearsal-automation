//! Configuration types for automation.
//!
//! Loads settings from config.json at startup. Provides button positions,
//! detection thresholds, and timing parameters.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

/// Global configuration instance, initialized once at startup.
static CONFIG: OnceLock<AutomationConfig> = OnceLock::new();

/// A rectangle in relative coordinates (0.0 to 1.0).
/// Used for defining screen regions that scale with window size.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct RelativeRect {
    /// X position of top-left corner (0.0 = left edge, 1.0 = right edge)
    pub x: f32,
    /// Y position of top-left corner (0.0 = top edge, 1.0 = bottom edge)
    pub y: f32,
    /// Width as fraction of window width
    pub width: f32,
    /// Height as fraction of window height
    pub height: f32,
}

impl Default for RelativeRect {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 0.1,
            height: 0.1,
        }
    }
}

/// Adjustment applied on top of each `score_regions[stage]` to produce the
/// human-review crop shown inline in the review window. All values are window
/// fractions (0..1). One shared instance covers all three stages because the
/// icon-above-score relationship is identical per stage. Kept separate from the
/// OCR crop because OCR wants a tight digits-only band, while review wants the
/// character portraits above the digits so the user can see who/what they are
/// correcting. Reusing `score_regions`' x/width single-sources the horizontal
/// layout, which the game may change in a future update.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ReviewCropAdjust {
    /// Extend the crop upward (decreasing y) to include the character portraits
    /// that sit above the printed scores.
    pub top_extend: f32,
    /// Extend the crop downward for breathing room below the digits.
    pub bottom_extend: f32,
    /// Trim from the left edge (default 0 so x tracks score_regions).
    pub left_inset: f32,
    /// Trim from the right edge (drops the 詳細 button / right margin).
    pub right_inset: f32,
}

impl Default for ReviewCropAdjust {
    fn default() -> Self {
        // Calibrated against real result screens with scripts/region_tuner.py:
        // the character portraits + printed scores sit in the centred ~0.56-wide
        // column and extend below the score band, so the crop trims both sides and
        // extends downward (a readable ~1.8:1 block) rather than upward.
        Self {
            top_extend: 0.05,
            bottom_extend: 0.1,
            left_inset: 0.21,
            right_inset: 0.23,
        }
    }
}

fn default_review_crop_adjust() -> ReviewCropAdjust {
    ReviewCropAdjust::default()
}

/// Derive the inline review crop for `stage` (0..=2) from `score_regions[stage]`
/// and `review_crop_adjust`, clamped into `[0,1]` so the result is always a
/// valid UV rect (an over-extension never samples outside the image; an inset
/// wider than the region yields a zero — not negative — dimension).
pub fn review_crop_rect(config: &AutomationConfig, stage: usize) -> RelativeRect {
    let s = config.score_regions[stage];
    let a = config.review_crop_adjust;
    let x0 = (s.x + a.left_inset).clamp(0.0, 1.0);
    let y0 = (s.y - a.top_extend).clamp(0.0, 1.0);
    let x1 = (s.x + s.width - a.right_inset).clamp(0.0, 1.0);
    let y1 = (s.y + s.height + a.bottom_extend).clamp(0.0, 1.0);
    RelativeRect {
        x: x0,
        y: y0,
        width: (x1 - x0).max(0.0),
        height: (y1 - y0).max(0.0),
    }
}

/// The three per-stage OCR crop-region arrays, bundled so callers pass one
/// value instead of three parallel arrays. A *view* built from (not replacing)
/// the individually-serialized `AutomationConfig` fields, so existing config
/// files keep working unchanged.
#[derive(Clone, Copy, Debug)]
pub struct OcrRegions {
    /// Score-row crop per stage (the nine per-character values).
    pub score: [RelativeRect; 3],
    /// Isolated stage-total crop per stage (checksum source).
    pub total: [RelativeRect; 3],
    /// Bonus-badge crop per stage (checksum cross-check).
    pub bonus: [RelativeRect; 3],
}

impl AutomationConfig {
    /// Bundle the three OCR region arrays for passing through the OCR pipeline.
    pub fn ocr_regions(&self) -> OcrRegions {
        OcrRegions {
            score: self.score_regions,
            total: self.total_regions,
            bonus: self.bonus_regions,
        }
    }
}

/// A point in relative coordinates for button centers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ButtonConfig {
    /// X position (0.0 = left edge, 1.0 = right edge)
    pub x: f32,
    /// Y position (0.0 = top edge, 1.0 = bottom edge)
    pub y: f32,
}

impl Default for ButtonConfig {
    fn default() -> Self {
        Self { x: 0.5, y: 0.5 }
    }
}

/// Complete automation configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutomationConfig {
    /// Position of the "開始する" (Start) button
    pub start_button: ButtonConfig,
    /// Region around start button for histogram comparison (detecting rehearsal page)
    #[serde(default = "default_start_button_region")]
    pub start_button_region: RelativeRect,
    /// Path to Start button reference image for histogram comparison
    #[serde(default = "default_start_button_reference")]
    pub start_button_reference: String,
    /// Position of the "スキップ" (Skip) button
    pub skip_button: ButtonConfig,
    /// Region around skip button for brightness detection
    pub skip_button_region: RelativeRect,
    /// Brightness threshold: above this = Skip button enabled, below = disabled/dimmed
    pub brightness_threshold: f32,
    /// Histogram similarity threshold: above this = Skip button detected (0.0-1.0)
    #[serde(default = "default_histogram_threshold")]
    pub histogram_threshold: f32,
    /// Path to Skip button reference image for histogram comparison
    #[serde(default = "default_skip_button_reference")]
    pub skip_button_reference: String,
    /// Position of the "終了" (End) button on result page
    #[serde(default = "default_end_button")]
    pub end_button: ButtonConfig,
    /// Region around end button for histogram comparison (detecting result page)
    #[serde(default = "default_end_button_region")]
    pub end_button_region: RelativeRect,
    /// Path to End button reference image for histogram comparison
    #[serde(default = "default_end_button_reference")]
    pub end_button_reference: String,
    /// Maximum time to wait for loading (milliseconds)
    pub loading_timeout_ms: u64,
    /// Maximum time to wait for result page (milliseconds)
    #[serde(default = "default_result_timeout_ms")]
    pub result_timeout_ms: u64,
    /// Delay after clicking skip before capturing result (milliseconds)
    pub capture_delay_ms: u64,
    /// Test position for relative click hotkey
    pub test_click_position: ButtonConfig,
    /// OCR brightness threshold (pixels with R, G, B all > threshold are kept)
    #[serde(default = "default_ocr_threshold")]
    pub ocr_threshold: u8,
    /// Per-stage score regions for cropped OCR (3 stages)
    #[serde(default = "default_score_regions")]
    pub score_regions: [RelativeRect; 3],
    /// Adjustment producing the inline review crop (character portraits + scores)
    /// from `score_regions`. Used only by the review window, not by OCR.
    #[serde(default = "default_review_crop_adjust")]
    pub review_crop_adjust: ReviewCropAdjust,
    /// Per-stage stage-total regions (the big isolated number used as the
    /// reconstruction checksum input). One per stage.
    #[serde(default = "default_total_regions")]
    pub total_regions: [RelativeRect; 3],
    /// Per-stage bonus-badge regions (light-blue "+NNN" crown badge, used only
    /// as an optional cross-check). Spans all three character columns since the
    /// badge sits under whichever column has the largest score.
    #[serde(default = "default_bonus_regions")]
    pub bonus_regions: [RelativeRect; 3],
    /// Brightness threshold for binarizing the stage-total crop (white text).
    #[serde(default = "default_total_threshold")]
    pub total_threshold: u8,
    /// Minimum blue channel for the bonus blue-selective mask. 190 (not 150)
    /// because the dimmer blue in character portraits leaks digits at 150.
    #[serde(default = "default_bonus_blue_min")]
    pub bonus_blue_min: u8,
    /// Minimum (blue - red) margin for the bonus blue-selective mask; drops the
    /// gold crown icon while keeping the light-blue digits.
    #[serde(default = "default_bonus_br_margin")]
    pub bonus_br_margin: u8,
    /// Number of consecutive histogram matches required to confirm detection (default 3)
    #[serde(default = "default_detection_confirm_count")]
    pub detection_confirm_count: u32,
    /// Maximum number of click retry attempts if button is still visible (default 3)
    #[serde(default = "default_max_click_retries")]
    pub max_click_retries: u32,
    /// Developer mode: when enabled, runs as tray app with advanced features
    #[serde(default)]
    pub developer_mode: bool,
}

fn default_score_regions() -> [RelativeRect; 3] {
    [
        RelativeRect { x: 0.0, y: 0.179, width: 1.0, height: 0.022 },  // Stage 1
        RelativeRect { x: 0.0, y: 0.430, width: 1.0, height: 0.022 },  // Stage 2
        RelativeRect { x: 0.0, y: 0.685, width: 1.0, height: 0.022 },  // Stage 3
    ]
}

fn default_total_regions() -> [RelativeRect; 3] {
    [
        RelativeRect { x: 0.29, y: 0.137, width: 0.4, height: 0.035 },  // Stage 1
        RelativeRect { x: 0.29, y: 0.388, width: 0.4, height: 0.035 },  // Stage 2
        RelativeRect { x: 0.29, y: 0.641, width: 0.4, height: 0.035 },  // Stage 3
    ]
}

fn default_bonus_regions() -> [RelativeRect; 3] {
    [
        RelativeRect { x: 0.28, y: 0.201, width: 0.45, height: 0.022 },  // Stage 1
        RelativeRect { x: 0.28, y: 0.452, width: 0.45, height: 0.022 },  // Stage 2
        RelativeRect { x: 0.28, y: 0.706, width: 0.45, height: 0.022 },  // Stage 3
    ]
}

fn default_ocr_threshold() -> u8 {
    190
}

fn default_total_threshold() -> u8 {
    // 210, not 190: the leading "3" of a 3,XXX,XXX total is misread as "5" at
    // 190 (samples 005/102842); the crisper strokes at 210 disambiguate it.
    // A faint "Pt" suffix can leak a trailing digit at some thresholds, but the
    // >7-digit guard in recognize_single_number rejects that (→ None).
    210
}

fn default_bonus_blue_min() -> u8 {
    190
}

fn default_bonus_br_margin() -> u8 {
    30
}

fn default_histogram_threshold() -> f32 {
    0.85 // 85% similarity required to detect buttons
}

fn default_start_button_region() -> RelativeRect {
    // Region around the "開始する" button for histogram comparison
    RelativeRect {
        x: 0.358,
        y: 0.834,
        width: 0.265,
        height: 0.029,
    }
}

fn default_start_button_reference() -> String {
    "resources/template/rehearsal/start_button_ref.png".to_string()
}

fn default_skip_button_reference() -> String {
    "resources/template/rehearsal/skip_button_ref.png".to_string()
}

fn default_end_button() -> ButtonConfig {
    // Default position for "終了" button (bottom center of result page)
    ButtonConfig { x: 0.5, y: 0.9 }
}

fn default_end_button_region() -> RelativeRect {
    // Region around the "終了" button for histogram comparison
    RelativeRect {
        x: 0.45,
        y: 0.89,
        width: 0.14,
        height: 0.035,
    }
}

fn default_end_button_reference() -> String {
    "resources/template/rehearsal/end_button_ref.png".to_string()
}

fn default_detection_confirm_count() -> u32 {
    3 // Require 3 consecutive matches to confirm detection
}

fn default_max_click_retries() -> u32 {
    3 // Retry clicking up to 3 times if button is still visible
}

fn default_result_timeout_ms() -> u64 {
    30000 // 30 seconds to wait for result page
}

impl Default for AutomationConfig {
    fn default() -> Self {
        Self {
            start_button: ButtonConfig { x: 0.5, y: 0.85 },
            start_button_region: default_start_button_region(),
            start_button_reference: default_start_button_reference(),
            skip_button: ButtonConfig { x: 0.82, y: 0.82 },
            skip_button_region: RelativeRect {
                x: 0.7,
                y: 0.8,
                width: 0.22,
                height: 0.04,
            },
            // Brightness threshold: Skip button dimmed ~92, enabled ~97
            // Set to 94 to detect when Skip button becomes enabled
            brightness_threshold: 94.0,
            histogram_threshold: default_histogram_threshold(),
            skip_button_reference: default_skip_button_reference(),
            end_button: default_end_button(),
            end_button_region: default_end_button_region(),
            end_button_reference: default_end_button_reference(),
            loading_timeout_ms: 30000,
            result_timeout_ms: default_result_timeout_ms(),
            capture_delay_ms: 500,
            test_click_position: ButtonConfig { x: 0.5, y: 0.5 },
            ocr_threshold: default_ocr_threshold(),
            score_regions: default_score_regions(),
            review_crop_adjust: default_review_crop_adjust(),
            total_regions: default_total_regions(),
            bonus_regions: default_bonus_regions(),
            total_threshold: default_total_threshold(),
            bonus_blue_min: default_bonus_blue_min(),
            bonus_br_margin: default_bonus_br_margin(),
            detection_confirm_count: default_detection_confirm_count(),
            max_click_retries: default_max_click_retries(),
            developer_mode: false,
        }
    }
}

/// Loads configuration from config.json or returns defaults.
/// Looks for config.json in the same directory as the executable.
fn load_config() -> AutomationConfig {
    // Try to find config.json next to the executable
    let config_path = crate::paths::get_exe_dir().join("config.json");

    crate::log(&format!("Looking for config at: {}", crate::paths::relative_display(&config_path)));

    if config_path.exists() {
        match fs::read_to_string(config_path) {
            Ok(contents) => match serde_json::from_str(&contents) {
                Ok(config) => {
                    crate::log("Config loaded from config.json");
                    return config;
                }
                Err(e) => {
                    crate::log(&format!(
                        "Failed to parse config.json: {}. Using defaults.",
                        e
                    ));
                }
            },
            Err(e) => {
                crate::log(&format!(
                    "Failed to read config.json: {}. Using defaults.",
                    e
                ));
            }
        }
    } else {
        crate::log("config.json not found. Using default config.");
    }

    AutomationConfig::default()
}

/// Initializes the global configuration. Call once at startup.
pub fn init_config() {
    let _ = CONFIG.set(load_config());
}

/// Returns a reference to the global configuration.
/// Panics if called before init_config().
pub fn get_config() -> &'static AutomationConfig {
    CONFIG
        .get()
        .expect("Config not initialized. Call init_config() first.")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_unit(v: f32) -> bool {
        (0.0..=1.0).contains(&v)
    }

    #[test]
    fn review_crop_default_frames_portrait_column() {
        let cfg = AutomationConfig::default();
        let s = cfg.score_regions[0]; // { x:0.0, y:0.179, width:1.0, height:0.022 }
        let crop = review_crop_rect(&cfg, 0);

        // Calibrated default trims both sides to the centred portrait column...
        assert!(crop.x > s.x, "crop.x {} should be right of score x {}", crop.x, s.x);
        assert!(crop.width < s.width, "crop.width {} should be < {}", crop.width, s.width);
        assert!((crop.width - 0.56).abs() < 1e-5, "crop.width {} ~ 0.56", crop.width);
        // ...slightly above the digits band and extending below it (portraits sit
        // under the scores on this screen).
        assert!(crop.y < s.y, "crop.y {} should be above score y {}", crop.y, s.y);
        assert!(crop.y + crop.height > s.y + s.height, "crop should extend below the score band");
        // Valid UV rect inside the image.
        assert!(in_unit(crop.x) && in_unit(crop.y));
        assert!(in_unit(crop.x + crop.width) && in_unit(crop.y + crop.height));
        assert!(crop.width > 0.0 && crop.height > 0.0);
    }

    #[test]
    fn review_crop_clamps_overextension_to_zero() {
        let mut cfg = AutomationConfig::default();
        // stage 0 y = 0.179; a huge top_extend must clamp y to 0, never negative.
        cfg.review_crop_adjust = ReviewCropAdjust {
            top_extend: 0.5,
            bottom_extend: 0.0,
            left_inset: 0.0,
            right_inset: 0.22,
        };
        let crop = review_crop_rect(&cfg, 0);
        assert_eq!(crop.y, 0.0);
        assert!(crop.height > 0.0);
        assert!(in_unit(crop.y + crop.height));
    }

    #[test]
    fn review_crop_inset_wider_than_region_is_zero_not_negative() {
        let mut cfg = AutomationConfig::default();
        // right_inset > width (1.0) collapses width to 0, not a negative number.
        cfg.review_crop_adjust = ReviewCropAdjust {
            top_extend: 0.05,
            bottom_extend: 0.0,
            left_inset: 0.0,
            right_inset: 1.5,
        };
        let crop = review_crop_rect(&cfg, 0);
        assert_eq!(crop.width, 0.0);
        assert!(crop.height > 0.0);
    }
}
