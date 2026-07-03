//! Automation state machine for rehearsal data collection.
//!
//! The state machine sequences through: Start → Wait → Skip → Capture → Loop
//! Each state transition checks for abort signals and window validity.
//!
//! The sequencing logic (`AutomationContext::step`) is pure: everything it asks
//! of the outside world goes through the [`GameOps`] trait. The production
//! implementation ([`LiveGame`]) talks to the real game window via Windows
//! APIs; unit tests drive the same `step()` with a scripted fake, so the
//! transition rules are testable without a game window
//! (`GAKUMAS_NO_MANIFEST=1 cargo test`).

use anyhow::{anyhow, Result};
use chrono::Local;
use image::{ImageBuffer, Rgba};
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

/// Everything the state machine asks of the outside world.
///
/// The production implementation ([`LiveGame`]) drives the real game window
/// (SendInput clicks, WGC captures, brightness/histogram waits); tests use a
/// scripted fake. Wait methods block until the screen condition holds,
/// returning `Err` on timeout or abort — when a wait fails because the user
/// aborted, `abort_requested()` returns true, which is how the state machine
/// distinguishes `Aborted` from `Error`.
pub trait GameOps {
    /// Is the game window still alive?
    fn is_window_valid(&self) -> bool;
    /// Focus the game window and click the Start button.
    fn click_start(&mut self) -> Result<()>;
    /// Focus the game window and click the Skip button.
    fn click_skip(&mut self) -> Result<()>;
    /// Focus the game window and click the End button.
    fn click_end(&mut self) -> Result<()>;
    /// Block until the rehearsal start page is showing. When `retry_end_click`
    /// is true (iterations after this run's first), the wait may re-click the
    /// End button if the page hasn't changed yet.
    fn wait_for_start_page(&mut self, retry_end_click: bool) -> Result<()>;
    /// Block until loading finished (Skip button present and enabled). May
    /// re-click the Start button while waiting.
    fn wait_for_loading(&mut self) -> Result<()>;
    /// Block until the result screen is showing (End button present). May
    /// re-click the Skip button while waiting.
    fn wait_for_result(&mut self) -> Result<()>;
    /// Capture the result screen as an RGBA image.
    fn capture_result(&mut self) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>>;
    /// Has the user requested an abort?
    fn abort_requested(&self) -> bool;
}

/// Production [`GameOps`]: drives the live game window via Windows APIs.
///
/// Owns the window handle, the automation config (button coordinates, regions,
/// thresholds), and the pre-loaded button reference images used for post-click
/// verification during the waits.
pub struct LiveGame {
    hwnd: HWND,
    config: AutomationConfig,
    /// Pre-loaded Start button reference for post-click verification
    start_button_ref: Option<ReferenceImage>,
    /// Pre-loaded Skip button reference for post-click verification
    skip_button_ref: Option<ReferenceImage>,
    /// Pre-loaded End button reference for post-click verification
    end_button_ref: Option<ReferenceImage>,
}

impl LiveGame {
    /// Creates the production game driver, pre-loading button reference images.
    pub fn new(hwnd: HWND, config: AutomationConfig) -> Self {
        let exe_dir = crate::paths::get_exe_dir();

        let start_button_ref = load_ref_image(&exe_dir, &config.start_button_reference, "Start");
        let skip_button_ref = load_ref_image(&exe_dir, &config.skip_button_reference, "Skip");
        let end_button_ref = load_ref_image(&exe_dir, &config.end_button_reference, "End");

        Self {
            hwnd,
            config,
            start_button_ref,
            skip_button_ref,
            end_button_ref,
        }
    }

    /// Clicks at a relative position after re-focusing the window.
    ///
    /// Re-focusing is important because the user might click elsewhere during
    /// automation.
    fn click_with_focus(&self, rel_x: f32, rel_y: f32) -> Result<()> {
        if !self.is_window_valid() {
            return Err(anyhow!("Game window no longer exists"));
        }

        // Bring window to foreground
        unsafe {
            let _ = SetForegroundWindow(self.hwnd);
        }
        std::thread::sleep(Duration::from_millis(50));

        click_at_relative(self.hwnd, rel_x, rel_y)
    }
}

/// The abort check injected into the detection waits (and read directly by
/// `LiveGame::abort_requested`).
fn abort_flag() -> bool {
    ABORT_REQUESTED.load(Ordering::SeqCst)
}

impl GameOps for LiveGame {
    fn is_window_valid(&self) -> bool {
        unsafe { IsWindow(self.hwnd).as_bool() }
    }

    fn click_start(&mut self) -> Result<()> {
        self.click_with_focus(self.config.start_button.x, self.config.start_button.y)
    }

    fn click_skip(&mut self) -> Result<()> {
        self.click_with_focus(self.config.skip_button.x, self.config.skip_button.y)
    }

    fn click_end(&mut self) -> Result<()> {
        self.click_with_focus(self.config.end_button.x, self.config.end_button.y)
    }

    fn wait_for_start_page(&mut self, retry_end_click: bool) -> Result<()> {
        let click_retry = if retry_end_click {
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
        wait_for_start_page(self.hwnd, &self.config, click_retry, &abort_flag)
    }

    fn wait_for_loading(&mut self) -> Result<()> {
        let click_retry = self.start_button_ref.as_ref().map(|ref_img| ClickRetryInfo {
            hwnd: self.hwnd,
            button_x: self.config.start_button.x,
            button_y: self.config.start_button.y,
            button_region: &self.config.start_button_region,
            ref_img,
            histogram_threshold: self.config.histogram_threshold,
            max_retries: self.config.max_click_retries,
        });
        wait_for_loading(self.hwnd, &self.config, click_retry, &abort_flag)
    }

    fn wait_for_result(&mut self) -> Result<()> {
        let click_retry = self.skip_button_ref.as_ref().map(|ref_img| ClickRetryInfo {
            hwnd: self.hwnd,
            button_x: self.config.skip_button.x,
            button_y: self.config.skip_button.y,
            button_region: &self.config.skip_button_region,
            ref_img,
            histogram_threshold: self.config.histogram_threshold,
            max_retries: self.config.max_click_retries,
        });
        wait_for_result(self.hwnd, &self.config, click_retry, &abort_flag)
    }

    fn capture_result(&mut self) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>> {
        capture_gakumas_to_buffer(self.hwnd)
    }

    fn abort_requested(&self) -> bool {
        abort_flag()
    }
}

/// Automation context holding state and configuration.
pub struct AutomationContext<G: GameOps> {
    /// Current state
    pub state: AutomationState,
    /// Interface to the game window (production: [`LiveGame`]; tests: a fake)
    ops: G,
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
}

impl<G: GameOps> AutomationContext<G> {
    /// Creates a new automation context around a game driver.
    pub fn new(
        ops: G,
        max_iterations: u32,
        start_iteration: u32,
        work_sender: Sender<OcrWorkItem>,
        screenshot_dir: PathBuf,
    ) -> Self {
        Self {
            state: AutomationState::Idle,
            ops,
            current_iteration: 0,
            completed_iterations: start_iteration.saturating_sub(1),
            start_iteration,
            max_iterations,
            work_sender,
            start_time: Instant::now(),
            screenshot_dir,
        }
    }

    /// Advances the state machine by one step.
    ///
    /// Returns `Ok(true)` if automation should continue, `Ok(false)` if complete/error/aborted.
    pub fn step(&mut self) -> Result<bool> {
        // Check for abort before each state transition
        if self.ops.abort_requested() {
            crate::log("Abort requested, stopping automation");
            self.state = AutomationState::Aborted;
            return Ok(false);
        }

        // Check if window is still valid
        if !self.ops.is_window_valid() {
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
                let retry_end_click = self.current_iteration > self.start_iteration;

                match self.ops.wait_for_start_page(retry_end_click) {
                    Ok(()) => {
                        self.state = AutomationState::ClickingStart;
                        Ok(true)
                    }
                    Err(e) => {
                        // Check if this was an abort request
                        if self.ops.abort_requested() {
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

                if let Err(e) = self.ops.click_start() {
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

                match self.ops.wait_for_loading() {
                    Ok(()) => {
                        self.state = AutomationState::ClickingSkip;
                        Ok(true)
                    }
                    Err(e) => {
                        // Check if this was an abort request
                        if self.ops.abort_requested() {
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

                if let Err(e) = self.ops.click_skip() {
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

                match self.ops.wait_for_result() {
                    Ok(()) => {
                        self.state = AutomationState::Capturing;
                        Ok(true)
                    }
                    Err(e) => {
                        // Check if this was an abort request
                        if self.ops.abort_requested() {
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
                let img = match self.ops.capture_result() {
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

                if let Err(e) = self.ops.click_end() {
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
    use crate::automation::queue::create_work_queue;
    use std::collections::VecDeque;

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

    /// Scripted outcome for one mock wait call. `AbortFail` models the user
    /// pressing the abort hotkey during the wait: the global flag flips and the
    /// wait returns Err, exactly like the real detection functions.
    #[derive(Clone, Copy)]
    enum WaitScript {
        Ok,
        Fail,
        AbortFail,
    }

    /// Scripted [`GameOps`] fake. Records every call (with the argument that
    /// matters) so tests can assert on the exact operation sequence. Wait
    /// queues default to `Ok` when empty.
    struct MockGame {
        window_valid: bool,
        abort: bool,
        calls: Vec<String>,
        wait_start_page: VecDeque<WaitScript>,
        wait_loading: VecDeque<WaitScript>,
        wait_result: VecDeque<WaitScript>,
    }

    impl MockGame {
        fn new() -> Self {
            Self {
                window_valid: true,
                abort: false,
                calls: Vec::new(),
                wait_start_page: VecDeque::new(),
                wait_loading: VecDeque::new(),
                wait_result: VecDeque::new(),
            }
        }

        fn apply(&mut self, script: WaitScript) -> Result<()> {
            match script {
                WaitScript::Ok => Ok(()),
                WaitScript::Fail => Err(anyhow!("scripted wait failure")),
                WaitScript::AbortFail => {
                    self.abort = true;
                    Err(anyhow!("Abort requested"))
                }
            }
        }
    }

    impl GameOps for MockGame {
        fn is_window_valid(&self) -> bool {
            self.window_valid
        }
        fn click_start(&mut self) -> Result<()> {
            self.calls.push("click_start".into());
            Ok(())
        }
        fn click_skip(&mut self) -> Result<()> {
            self.calls.push("click_skip".into());
            Ok(())
        }
        fn click_end(&mut self) -> Result<()> {
            self.calls.push("click_end".into());
            Ok(())
        }
        fn wait_for_start_page(&mut self, retry_end_click: bool) -> Result<()> {
            self.calls.push(format!("wait_start_page({})", retry_end_click));
            let s = self.wait_start_page.pop_front().unwrap_or(WaitScript::Ok);
            self.apply(s)
        }
        fn wait_for_loading(&mut self) -> Result<()> {
            self.calls.push("wait_loading".into());
            let s = self.wait_loading.pop_front().unwrap_or(WaitScript::Ok);
            self.apply(s)
        }
        fn wait_for_result(&mut self) -> Result<()> {
            self.calls.push("wait_result".into());
            let s = self.wait_result.pop_front().unwrap_or(WaitScript::Ok);
            self.apply(s)
        }
        fn capture_result(&mut self) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>> {
            self.calls.push("capture".into());
            Ok(ImageBuffer::new(1, 1))
        }
        fn abort_requested(&self) -> bool {
            self.abort
        }
    }

    /// Drives the state machine to a terminal state and returns the context.
    fn run_to_end(
        mock: MockGame,
        max_iterations: u32,
        start_iteration: u32,
    ) -> AutomationContext<MockGame> {
        let (sender, _receiver) = create_work_queue();
        let dir = tempfile::tempdir().unwrap();
        let mut ctx = AutomationContext::new(
            mock,
            max_iterations,
            start_iteration,
            sender,
            dir.path().to_path_buf(),
        );
        // The dir must outlive the loop (screenshots are saved into it).
        while ctx.step().unwrap() {}
        // Keep tempdir alive until after the loop; it drops here.
        ctx
    }

    #[test]
    fn happy_path_runs_two_iterations_in_order() {
        let ctx = run_to_end(MockGame::new(), 2, 1);
        assert_eq!(ctx.state, AutomationState::Complete);
        assert_eq!(ctx.completed_iterations, 2);
        assert_eq!(
            ctx.ops.calls,
            vec![
                // Iteration 1: first iteration of the run never retries End.
                "wait_start_page(false)",
                "click_start",
                "wait_loading",
                "click_skip",
                "wait_result",
                "capture",
                "click_end",
                // Iteration 2: now past the run's first iteration -> retry allowed.
                "wait_start_page(true)",
                "click_start",
                "wait_loading",
                "click_skip",
                "wait_result",
                "capture",
                "click_end",
            ]
        );
    }

    #[test]
    fn resume_starts_at_start_iteration_and_counts_from_completed() {
        // Resuming 3..=4 of a 4-iteration series: completed starts at 2.
        let ctx = run_to_end(MockGame::new(), 4, 3);
        assert_eq!(ctx.state, AutomationState::Complete);
        assert_eq!(ctx.completed_iterations, 4);
        // Iteration 3 is this run's first (3 > 3 is false) -> no End retry;
        // iteration 4 retries.
        let waits: Vec<&str> = ctx
            .ops
            .calls
            .iter()
            .filter(|c| c.starts_with("wait_start_page"))
            .map(|s| s.as_str())
            .collect();
        assert_eq!(waits, vec!["wait_start_page(false)", "wait_start_page(true)"]);
    }

    #[test]
    fn abort_during_wait_ends_aborted_not_error() {
        let mut mock = MockGame::new();
        mock.wait_loading.push_back(WaitScript::AbortFail);
        let ctx = run_to_end(mock, 2, 1);
        assert_eq!(ctx.state, AutomationState::Aborted);
        assert_eq!(ctx.completed_iterations, 0);
    }

    #[test]
    fn wait_failure_without_abort_ends_error() {
        let mut mock = MockGame::new();
        mock.wait_loading.push_back(WaitScript::Fail);
        let ctx = run_to_end(mock, 2, 1);
        match &ctx.state {
            AutomationState::Error(msg) => {
                assert!(msg.contains("Loading wait failed"), "unexpected: {msg}")
            }
            other => panic!("expected Error, got {other}"),
        }
    }

    #[test]
    fn abort_before_step_ends_aborted_immediately() {
        let mut mock = MockGame::new();
        mock.abort = true;
        let ctx = run_to_end(mock, 2, 1);
        assert_eq!(ctx.state, AutomationState::Aborted);
        assert!(ctx.ops.calls.is_empty());
    }

    #[test]
    fn invalid_window_ends_error() {
        let mut mock = MockGame::new();
        mock.window_valid = false;
        let ctx = run_to_end(mock, 2, 1);
        assert_eq!(
            ctx.state,
            AutomationState::Error("Game window closed".to_string())
        );
    }
}
