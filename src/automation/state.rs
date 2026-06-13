//! Automation state machine for rehearsal data collection.
//!
//! The state machine sequences through: Start → Wait → Skip → Capture → Loop
//! Each state transition checks for abort signals and window validity.

use anyhow::{anyhow, Result};
use chrono::Local;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{IsWindow, SetForegroundWindow};

use crate::automation::config::AutomationConfig;
use crate::automation::detection::{
    load_reference_histogram, wait_for_loading, wait_for_result,
    wait_for_start_page, ClickRetryInfo, ReferenceImage,
};
use crate::automation::input::click_at_relative;
use crate::automation::queue::OcrWorkItem;
use crate::capture::capture_gakumas_to_buffer;

/// Global abort flag - set by abort hotkey handler.
pub static ABORT_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Automation state machine states.
#[derive(Debug, Clone, PartialEq)]
pub enum AutomationState {
    /// Waiting to start (initial state)
    Idle,
    /// Waiting for rehearsal start page to appear
    WaitingForStartPage,
    /// Clicking the Start button
    ClickingStart,
    /// Waiting for loading to complete
    WaitingForLoading,
    /// Clicking the Skip button
    ClickingSkip,
    /// Waiting for result screen to appear
    WaitingForResult,
    /// Capturing the result screenshot
    Capturing,
    /// Clicking the End button to return to rehearsal page
    ClickingEnd,
    /// Checking if we should continue or stop
    CheckingLoop,
    /// All iterations complete
    Complete,
    /// Error occurred
    Error(String),
    /// User requested abort
    Aborted,
}

impl std::fmt::Display for AutomationState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AutomationState::Idle => write!(f, "Idle"),
            AutomationState::WaitingForStartPage => write!(f, "Waiting for start page"),
            AutomationState::ClickingStart => write!(f, "Clicking Start"),
            AutomationState::WaitingForLoading => write!(f, "Waiting for loading"),
            AutomationState::ClickingSkip => write!(f, "Clicking Skip"),
            AutomationState::WaitingForResult => write!(f, "Waiting for result"),
            AutomationState::Capturing => write!(f, "Capturing"),
            AutomationState::ClickingEnd => write!(f, "Clicking End"),
            AutomationState::CheckingLoop => write!(f, "Checking loop"),
            AutomationState::Complete => write!(f, "Complete"),
            AutomationState::Error(msg) => write!(f, "Error: {}", msg),
            AutomationState::Aborted => write!(f, "Aborted"),
        }
    }
}

impl AutomationState {
    /// Returns a Japanese description of the current state (for GUI display).
    pub fn description_ja(&self) -> String {
        match self {
            AutomationState::Idle => "待機中".to_string(),
            AutomationState::WaitingForStartPage => "開始画面を待機中".to_string(),
            AutomationState::ClickingStart => "開始ボタンをクリック中".to_string(),
            AutomationState::WaitingForLoading => "ローディング中".to_string(),
            AutomationState::ClickingSkip => "スキップボタンをクリック中".to_string(),
            AutomationState::WaitingForResult => "結果画面を待機中".to_string(),
            AutomationState::Capturing => "スクリーンショット取得中".to_string(),
            AutomationState::ClickingEnd => "終了ボタンをクリック中".to_string(),
            AutomationState::CheckingLoop => "次のループを確認中".to_string(),
            AutomationState::Complete => "完了".to_string(),
            AutomationState::Error(msg) => format!("エラー: {}", msg),
            AutomationState::Aborted => "中断".to_string(),
        }
    }
}

/// Automation context holding state and configuration.
pub struct AutomationContext {
    /// Current state
    pub state: AutomationState,
    /// Game window handle
    pub hwnd: HWND,
    /// Automation configuration
    pub config: AutomationConfig,
    /// Current iteration number (1-based)
    pub current_iteration: u32,
    /// Number of iterations whose result was successfully captured (0-based count)
    pub completed_iterations: u32,
    /// 1-based iteration this run begins at (1 for fresh, completed+1 for resume)
    pub start_iteration: u32,
    /// Maximum number of iterations
    pub max_iterations: u32,
    /// Channel sender for OCR work items
    pub work_sender: Sender<OcrWorkItem>,
    /// Time when automation started
    pub start_time: Instant,
    /// Directory for saving screenshots
    pub screenshot_dir: PathBuf,
    /// Pre-loaded Start button reference for post-click verification
    start_button_ref: Option<ReferenceImage>,
    /// Pre-loaded Skip button reference for post-click verification
    skip_button_ref: Option<ReferenceImage>,
    /// Pre-loaded End button reference for post-click verification
    end_button_ref: Option<ReferenceImage>,
}

impl AutomationContext {
    /// Creates a new automation context.
    ///
    /// Pre-loads button reference images for post-click verification.
    pub fn new(
        hwnd: HWND,
        config: AutomationConfig,
        max_iterations: u32,
        start_iteration: u32,
        work_sender: Sender<OcrWorkItem>,
        screenshot_dir: PathBuf,
    ) -> Self {
        let exe_dir = crate::paths::get_exe_dir();

        let start_button_ref = load_ref_image(&exe_dir, &config.start_button_reference, "Start");
        let skip_button_ref = load_ref_image(&exe_dir, &config.skip_button_reference, "Skip");
        let end_button_ref = load_ref_image(&exe_dir, &config.end_button_reference, "End");

        Self {
            state: AutomationState::Idle,
            hwnd,
            config,
            current_iteration: 0,
            completed_iterations: start_iteration.saturating_sub(1),
            start_iteration,
            max_iterations,
            work_sender,
            start_time: Instant::now(),
            screenshot_dir,
            start_button_ref,
            skip_button_ref,
            end_button_ref,
        }
    }

    /// Advances the state machine by one step.
    ///
    /// Returns `Ok(true)` if automation should continue, `Ok(false)` if complete/error/aborted.
    pub fn step(&mut self) -> Result<bool> {
        // Check for abort before each state transition
        if ABORT_REQUESTED.load(Ordering::SeqCst) {
            crate::log("Abort requested, stopping automation");
            self.state = AutomationState::Aborted;
            return Ok(false);
        }

        // Check if window is still valid
        if !is_window_valid(self.hwnd) {
            crate::log("Game window no longer exists, aborting");
            self.state = AutomationState::Error("Game window closed".to_string());
            return Ok(false);
        }

        match &self.state {
            AutomationState::Idle => {
                self.current_iteration = self.start_iteration;
                crate::log(&format!(
                    "Starting automation: {} iterations",
                    self.max_iterations
                ));
                self.state = AutomationState::WaitingForStartPage;
                Ok(true)
            }

            AutomationState::WaitingForStartPage => {
                crate::log(&format!(
                    "Iteration {}/{}: Waiting for rehearsal page...",
                    self.current_iteration, self.max_iterations
                ));

                // Only retry End button click after the first iteration of this run
                // (the first iteration — fresh or resumed — hasn't clicked End yet)
                let click_retry = if self.current_iteration > self.start_iteration {
                    self.end_button_ref.as_ref().map(|ref_img| ClickRetryInfo {
                        hwnd: self.hwnd,
                        button_x: self.config.end_button.x,
                        button_y: self.config.end_button.y,
                        button_region: &self.config.end_button_region,
                        ref_img,
                        histogram_threshold: self.config.histogram_threshold,
                        max_retries: self.config.max_click_retries,
                    })
                } else {
                    None
                };

                match wait_for_start_page(self.hwnd, &self.config, click_retry) {
                    Ok(()) => {
                        self.state = AutomationState::ClickingStart;
                        Ok(true)
                    }
                    Err(e) => {
                        // Check if this was an abort request
                        if ABORT_REQUESTED.load(Ordering::SeqCst) {
                            crate::log("Abort requested during start page wait");
                            self.state = AutomationState::Aborted;
                        } else {
                            self.state =
                                AutomationState::Error(format!("Start page wait failed: {}", e));
                        }
                        Ok(false)
                    }
                }
            }

            AutomationState::ClickingStart => {
                crate::log(&format!(
                    "Iteration {}/{}: Clicking Start button",
                    self.current_iteration, self.max_iterations
                ));

                if let Err(e) = click_with_focus(
                    self.hwnd,
                    self.config.start_button.x,
                    self.config.start_button.y,
                ) {
                    self.state = AutomationState::Error(format!("Failed to click Start: {}", e));
                    return Ok(false);
                }

                self.state = AutomationState::WaitingForLoading;
                Ok(true)
            }

            AutomationState::WaitingForLoading => {
                crate::log(&format!(
                    "Iteration {}/{}: Waiting for loading...",
                    self.current_iteration, self.max_iterations
                ));

                let click_retry = self.start_button_ref.as_ref().map(|ref_img| ClickRetryInfo {
                    hwnd: self.hwnd,
                    button_x: self.config.start_button.x,
                    button_y: self.config.start_button.y,
                    button_region: &self.config.start_button_region,
                    ref_img,
                    histogram_threshold: self.config.histogram_threshold,
                    max_retries: self.config.max_click_retries,
                });

                match wait_for_loading(self.hwnd, &self.config, click_retry) {
                    Ok(()) => {
                        self.state = AutomationState::ClickingSkip;
                        Ok(true)
                    }
                    Err(e) => {
                        // Check if this was an abort request
                        if ABORT_REQUESTED.load(Ordering::SeqCst) {
                            crate::log("Abort requested during loading wait");
                            self.state = AutomationState::Aborted;
                        } else {
                            self.state =
                                AutomationState::Error(format!("Loading wait failed: {}", e));
                        }
                        Ok(false)
                    }
                }
            }

            AutomationState::ClickingSkip => {
                crate::log(&format!(
                    "Iteration {}/{}: Clicking Skip button",
                    self.current_iteration, self.max_iterations
                ));

                if let Err(e) = click_with_focus(
                    self.hwnd,
                    self.config.skip_button.x,
                    self.config.skip_button.y,
                ) {
                    self.state = AutomationState::Error(format!("Failed to click Skip: {}", e));
                    return Ok(false);
                }

                self.state = AutomationState::WaitingForResult;
                Ok(true)
            }

            AutomationState::WaitingForResult => {
                crate::log(&format!(
                    "Iteration {}/{}: Waiting for result screen...",
                    self.current_iteration, self.max_iterations
                ));

                let click_retry = self.skip_button_ref.as_ref().map(|ref_img| ClickRetryInfo {
                    hwnd: self.hwnd,
                    button_x: self.config.skip_button.x,
                    button_y: self.config.skip_button.y,
                    button_region: &self.config.skip_button_region,
                    ref_img,
                    histogram_threshold: self.config.histogram_threshold,
                    max_retries: self.config.max_click_retries,
                });

                match wait_for_result(self.hwnd, &self.config, click_retry) {
                    Ok(()) => {
                        self.state = AutomationState::Capturing;
                        Ok(true)
                    }
                    Err(e) => {
                        // Check if this was an abort request
                        if ABORT_REQUESTED.load(Ordering::SeqCst) {
                            crate::log("Abort requested during result wait");
                            self.state = AutomationState::Aborted;
                        } else {
                            self.state =
                                AutomationState::Error(format!("Result wait failed: {}", e));
                        }
                        Ok(false)
                    }
                }
            }

            AutomationState::Capturing => {
                crate::log(&format!(
                    "Iteration {}/{}: Capturing screenshot",
                    self.current_iteration, self.max_iterations
                ));

                // Capture screenshot
                let img = match capture_gakumas_to_buffer(self.hwnd) {
                    Ok(img) => img,
                    Err(e) => {
                        self.state =
                            AutomationState::Error(format!("Failed to capture: {}", e));
                        return Ok(false);
                    }
                };

                // Generate filename with timestamp
                let timestamp = Local::now().format("%Y%m%d_%H%M%S");
                let filename = format!("{:03}_{}.png", self.current_iteration, timestamp);
                let screenshot_path = self.screenshot_dir.join(&filename);

                // Save screenshot
                if let Err(e) = img.save(&screenshot_path) {
                    self.state =
                        AutomationState::Error(format!("Failed to save screenshot: {}", e));
                    return Ok(false);
                }

                crate::log(&format!(
                    "Iteration {}/{}: Screenshot saved to {}",
                    self.current_iteration,
                    self.max_iterations,
                    crate::paths::relative_display(&screenshot_path)
                ));

                // Queue for OCR processing
                let work_item = OcrWorkItem::new(screenshot_path, self.current_iteration);
                if let Err(e) = self.work_sender.send(work_item) {
                    crate::log(&format!("Warning: Failed to queue OCR work item: {}", e));
                    // Don't fail automation for this - OCR is secondary
                }

                // This run produced a result; count it as completed.
                self.completed_iterations += 1;

                self.state = AutomationState::ClickingEnd;
                Ok(true)
            }

            AutomationState::ClickingEnd => {
                crate::log(&format!(
                    "Iteration {}/{}: Clicking End button",
                    self.current_iteration, self.max_iterations
                ));

                if let Err(e) = click_with_focus(
                    self.hwnd,
                    self.config.end_button.x,
                    self.config.end_button.y,
                ) {
                    self.state = AutomationState::Error(format!("Failed to click End: {}", e));
                    return Ok(false);
                }

                self.state = AutomationState::CheckingLoop;
                Ok(true)
            }

            AutomationState::CheckingLoop => {
                if self.current_iteration >= self.max_iterations {
                    crate::log(&format!(
                        "Automation complete: {} iterations in {:.1}s",
                        self.max_iterations,
                        self.start_time.elapsed().as_secs_f32()
                    ));
                    self.state = AutomationState::Complete;
                    Ok(false)
                } else {
                    self.current_iteration += 1;
                    // Wait for start page before clicking Start again
                    self.state = AutomationState::WaitingForStartPage;
                    Ok(true)
                }
            }

            AutomationState::Complete | AutomationState::Error(_) | AutomationState::Aborted => {
                Ok(false)
            }
        }
    }

    /// Returns a progress string for display (e.g., in tray tooltip).
    pub fn progress_string(&self) -> String {
        match &self.state {
            AutomationState::Complete => {
                format!("Complete ({} iterations)", self.max_iterations)
            }
            AutomationState::Error(msg) => format!("Error: {}", msg),
            AutomationState::Aborted => "Aborted".to_string(),
            _ => format!("{}/{} - {}", self.current_iteration, self.max_iterations, self.state),
        }
    }
}

/// Tries to load a reference image for post-click verification.
/// Returns None with a log message if the image doesn't exist or fails to load.
fn load_ref_image(
    exe_dir: &std::path::Path,
    relative_path: &str,
    button_name: &str,
) -> Option<ReferenceImage> {
    let path = exe_dir.join(relative_path);
    if !path.exists() {
        return None;
    }
    match load_reference_histogram(&path) {
        Ok(ref_img) => {
            crate::log(&format!(
                "Pre-loaded {} button reference for click verification",
                button_name
            ));
            Some(ref_img)
        }
        Err(e) => {
            crate::log(&format!(
                "Warning: Failed to load {} button reference: {}",
                button_name, e
            ));
            None
        }
    }
}

/// Checks if a window handle is still valid.
fn is_window_valid(hwnd: HWND) -> bool {
    unsafe { IsWindow(hwnd).as_bool() }
}

/// Clicks at a relative position after re-focusing the window.
///
/// Re-focusing is important because the user might click elsewhere during automation.
fn click_with_focus(hwnd: HWND, rel_x: f32, rel_y: f32) -> Result<()> {
    if !is_window_valid(hwnd) {
        return Err(anyhow!("Game window no longer exists"));
    }

    // Bring window to foreground
    unsafe {
        let _ = SetForegroundWindow(hwnd);
    }
    std::thread::sleep(Duration::from_millis(50));

    click_at_relative(hwnd, rel_x, rel_y)
}

/// Resets the abort flag. Call before starting automation.
pub fn reset_abort_flag() {
    ABORT_REQUESTED.store(false, Ordering::SeqCst);
}

/// Requests abort of running automation.
pub fn request_abort() {
    ABORT_REQUESTED.store(true, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_display() {
        assert_eq!(format!("{}", AutomationState::Idle), "Idle");
        assert_eq!(
            format!("{}", AutomationState::WaitingForLoading),
            "Waiting for loading"
        );
        assert_eq!(
            format!("{}", AutomationState::Error("test".to_string())),
            "Error: test"
        );
    }
}
