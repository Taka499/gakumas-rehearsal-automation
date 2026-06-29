//! GUI application state management.
//!
//! Tracks user input values and automation status for display.

use crate::automation::results_edit::ReviewRow;
use crate::automation::session_meta::ResumableSession;
use eframe::egui::TextureHandle;
use std::path::PathBuf;
use std::time::Instant;

/// State for the OCR result review/edit window (see EXECPLAN_OCR_REVIEW_EDIT_GUI).
///
/// Holds the loaded result rows for one finished session, parallel editable text
/// buffers (one nine-cell grid per row), the filter/dirty flags, and the
/// currently-previewed screenshot texture. `Debug` is implemented by hand because
/// `TextureHandle` is a GPU handle we do not want to format.
pub struct ReviewState {
    pub session_path: PathBuf,
    pub rows: Vec<ReviewRow>,
    /// Per-row editable score strings, parallel to `rows`: `edits[row][stage][slot]`.
    pub edits: Vec<[[String; 3]; 3]>,
    /// Master override: when true, every status is shown regardless of the
    /// per-status toggles below (search still narrows).
    pub show_all: bool,
    /// Per-status visibility toggles. Default: flagged + repaired on, ok + manual
    /// + verified off (the attention-needed rows). Combined with `search` (AND).
    pub show_ok: bool,
    pub show_repaired: bool,
    pub show_flagged: bool,
    pub show_manual: bool,
    /// Rows the user reviewed and confirmed correct without editing (resolved).
    pub show_verified: bool,
    /// Live substring filter over the score cells + iteration (Ctrl+F style).
    /// Independent of the status toggles, so it persists as they change.
    pub search: String,
    /// An edit buffer differs from the saved value (enables 保存).
    pub dirty: bool,
    /// The screenshot currently rendered in the preview pane: `(iteration, texture)`.
    pub preview: Option<(u32, TextureHandle)>,
    /// Iteration whose row is expanded to show inline per-stage crops, or None.
    pub expanded: Option<u32>,
    /// Whether the review window is shown.
    pub open: bool,
}

impl std::fmt::Debug for ReviewState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReviewState")
            .field("session_path", &self.session_path)
            .field("rows", &self.rows.len())
            .field("show_flagged", &self.show_flagged)
            .field("show_repaired", &self.show_repaired)
            .field("show_verified", &self.show_verified)
            .field("search", &self.search)
            .field("dirty", &self.dirty)
            .field("open", &self.open)
            .field("expanded", &self.expanded)
            .field("preview_iter", &self.preview.as_ref().map(|(i, _)| *i))
            .finish()
    }
}

/// Automation status for display in GUI.
#[derive(Clone, Debug)]
pub enum AutomationStatus {
    /// Not running, ready to start
    Idle,
    /// Automation is running
    Running {
        current: u32,
        total: u32,
        state_description: String,
        start_time: Instant,
    },
    /// Automation completed successfully (all requested runs finished)
    Completed {
        completed: u32,
        total: u32,
        session_path: PathBuf,
    },
    /// Automation was aborted by the user before finishing all runs
    Aborted {
        completed: u32,
        total: u32,
        session_path: Option<PathBuf>,
    },
    /// Automation stopped early due to a timeout or error
    Error {
        completed: u32,
        total: u32,
        message: String,
        session_path: Option<PathBuf>,
    },
}

impl Default for AutomationStatus {
    fn default() -> Self {
        Self::Idle
    }
}

impl AutomationStatus {
    /// Get display text for current status.
    pub fn status_text(&self) -> String {
        match self {
            Self::Idle => "待機中".to_string(),
            Self::Running { current, total, state_description, .. } => {
                format!("実行中 ({}/{}) - {}", current, total, state_description)
            }
            Self::Completed { completed, total, session_path } => {
                // Extract folder name from path for display
                let folder_name = session_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("output");
                format!("完了 ({}/{}回) → {}", completed, total, folder_name)
            }
            Self::Aborted { completed, total, .. } => {
                format!("中断 ({}/{}回 完了)", completed, total)
            }
            Self::Error { completed, total, message, .. } => {
                format!("エラー ({}/{}回 完了): {}", completed, total, message)
            }
        }
    }

    /// Get progress as percentage (0.0 to 1.0).
    ///
    /// For terminal states the bar reflects how many of the requested runs
    /// actually completed, so a timeout/abort shows real progress, not 100%.
    pub fn progress(&self) -> f32 {
        match self {
            Self::Running { current, total, .. } if *total > 0 => {
                *current as f32 / *total as f32
            }
            Self::Completed { completed, total, .. }
            | Self::Aborted { completed, total, .. }
            | Self::Error { completed, total, .. }
                if *total > 0 =>
            {
                (*completed as f32 / *total as f32).clamp(0.0, 1.0)
            }
            _ => 0.0,
        }
    }

    /// Get elapsed time string if running.
    pub fn elapsed_text(&self) -> Option<String> {
        match self {
            Self::Running { start_time, .. } => {
                let elapsed = start_time.elapsed();
                let secs = elapsed.as_secs();
                let mins = secs / 60;
                let secs = secs % 60;
                Some(format!("{:02}:{:02}", mins, secs))
            }
            _ => None,
        }
    }

    /// Check if automation is currently running.
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running { .. })
    }

    /// If this terminal state can be resumed (interrupted with runs remaining and
    /// a known session folder), returns (completed, total, session_path).
    pub fn resumable(&self) -> Option<(u32, u32, std::path::PathBuf)> {
        match self {
            Self::Aborted { completed, total, session_path: Some(p) }
            | Self::Error { completed, total, session_path: Some(p), .. }
                if *completed < *total =>
            {
                Some((*completed, *total, p.clone()))
            }
            _ => None,
        }
    }
}

/// GUI application state.
#[derive(Debug)]
pub struct GuiState {
    /// Number of iterations to run (user input).
    pub iterations: u32,
    /// Number of *additional* runs for the 追加実行 (extend) control, kept
    /// separate from `iterations` so the fresh-run count and the extend count do
    /// not overwrite each other.
    pub additional_iterations: u32,
    /// Current automation status.
    pub status: AutomationStatus,
    /// Path to the latest session folder (for "Open Folder" button).
    pub latest_session_path: Option<PathBuf>,
    /// Start time of current automation run.
    pub automation_start_time: Option<Instant>,
    /// Interrupted sessions discovered on disk (for the resume picker).
    pub resumable_sessions: Vec<ResumableSession>,
    /// Index of the currently selected resumable session in the picker.
    pub selected_resume: Option<usize>,
    /// Open review/edit window for the latest session's OCR results, if any.
    pub review: Option<ReviewState>,
    /// (flagged, repaired) row counts for `latest_session_path`. Set when a run
    /// reaches a terminal state and re-computed after each review save, so the
    /// finished panel can prompt the user to check remaining attention rows.
    pub attention_counts: Option<(u32, u32)>,
}

impl Default for GuiState {
    fn default() -> Self {
        Self {
            iterations: 100,
            additional_iterations: 100,
            status: AutomationStatus::Idle,
            latest_session_path: None,
            automation_start_time: None,
            resumable_sessions: Vec::new(),
            selected_resume: None,
            review: None,
            attention_counts: None,
        }
    }
}
