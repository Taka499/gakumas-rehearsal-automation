//! Controller for the OCR result review/edit window.
//!
//! Owns the whole review lifecycle — open (load a session's rows), preview
//! (cache one iteration's screenshot texture for the inline per-stage crops),
//! render + action dispatch, and save (parse edit buffers, mark rows, rewrite
//! both CSVs) — so tracing a review operation stays within this module.
//!
//! Cross-module *reactions* to a save (finished-panel attention counts, live
//! distribution reload, chart regeneration) deliberately stay in `GuiApp`:
//! they touch subsystems the review window should not own. `show()` reports a
//! successful save via [`SaveEffects`] and `GuiApp` applies the reactions.

use std::path::PathBuf;

use eframe::egui;

use crate::automation::results_edit::{
    load_review_rows, save_review_rows, ReviewRow, RECOVERY_MANUAL, RECOVERY_VERIFIED,
};

use super::copyable::CopyToast;
use super::render::{self, ReviewActions};
use super::state::ReviewState;

/// What a successful save changed, for `GuiApp` to react to.
pub struct SaveEffects {
    /// The session whose CSVs were rewritten.
    pub session_path: PathBuf,
}

/// The review window's controller: state + lifecycle methods.
pub struct ReviewController {
    state: Option<ReviewState>,
}

impl ReviewController {
    pub fn new() -> Self {
        Self { state: None }
    }

    /// Load `session_path`'s results into the review window and open it.
    pub fn open(&mut self, session_path: PathBuf) {
        match load_review_rows(&session_path) {
            Ok(rows) => {
                let edits = edits_from_rows(&rows);
                self.state = Some(ReviewState::with_default_filters(session_path, rows, edits));
                crate::log("GUI: Opened OCR result review window");
            }
            Err(e) => {
                crate::log(&format!("GUI: Failed to load results for review: {}", e));
            }
        }
    }

    /// Load one iteration's screenshot into the review preview texture (cached
    /// until another row is chosen). No-op if it is already the previewed
    /// iteration. The texture is the sample source for the inline per-stage
    /// crops under an expanded row.
    fn load_preview(&mut self, ctx: &egui::Context, iteration: u32) {
        let review = match self.state.as_mut() {
            Some(r) => r,
            None => return,
        };
        if review.preview.as_ref().map_or(false, |(i, _)| *i == iteration) {
            return;
        }
        let path = match review.rows.iter().find(|r| r.iteration == iteration) {
            Some(r) => r.screenshot.clone(),
            None => return,
        };
        match image::open(&path) {
            Ok(img) => {
                let rgba = img.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                let color = egui::ColorImage::from_rgba_unmultiplied(size, &rgba.into_raw());
                let tex = ctx.load_texture(
                    format!("review_preview_{}", iteration),
                    color,
                    egui::TextureOptions::LINEAR,
                );
                review.preview = Some((iteration, tex));
            }
            Err(e) => {
                crate::log(&format!("GUI: Failed to open screenshot {}: {}", path, e));
                review.preview = None;
            }
        }
    }

    /// Persist the review edits: parse each row's buffers, mark changed rows
    /// `manual`, rewrite both CSVs, and re-seed the buffers from the saved rows.
    /// Returns `None` when there is nothing to save or the write failed.
    fn save(&mut self) -> Option<SaveEffects> {
        let review = self.state.as_mut()?;
        let mut changed = 0u32;
        for (i, row) in review.rows.iter_mut().enumerate() {
            let mut new_scores = row.scores;
            let mut row_changed = false;
            for s in 0..3 {
                for c in 0..3 {
                    match review.edits[i][s][c].trim().parse::<u32>() {
                        Ok(v) => {
                            if v != row.scores[s][c] {
                                new_scores[s][c] = v;
                                row_changed = true;
                            }
                        }
                        // Non-numeric input: keep the prior value, reset the buffer.
                        Err(_) => review.edits[i][s][c] = row.scores[s][c].to_string(),
                    }
                }
            }
            if row_changed {
                row.scores = new_scores;
                row.recovery = RECOVERY_MANUAL.to_string();
                changed += 1;
            }
        }
        let session_path = review.session_path.clone();
        match save_review_rows(&session_path, &review.rows) {
            Ok(()) => {
                review.dirty = false;
                review.edits = edits_from_rows(&review.rows);
                crate::log(&format!(
                    "GUI: Saved review edits ({} row(s) marked manual) to {}",
                    changed,
                    session_path.display()
                ));
                Some(SaveEffects { session_path })
            }
            Err(e) => {
                crate::log(&format!("GUI: Failed to save review edits: {}", e));
                None
            }
        }
    }

    /// Render the review/edit window (when open) and dispatch its actions.
    /// Returns the effects of a save performed this frame (保存 click or the
    /// auto-save on ✓ verify) so the caller can react.
    ///
    /// The review lives in its OWN top-level OS window (an egui *immediate
    /// viewport*), not a panel floating inside the main window, so it is resized
    /// independently and is never clipped by the main window's bounds.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        copy_toast: &mut Option<CopyToast>,
    ) -> Option<SaveEffects> {
        if !self.state.as_ref().map_or(false, |r| r.open) {
            return None;
        }
        let mut actions = ReviewActions::default();
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("ocr_review_viewport"),
            egui::ViewportBuilder::default()
                .with_title("結果の確認・修正")
                .with_inner_size([1200.0, 720.0])
                .with_min_inner_size([700.0, 420.0])
                // Match the main viewport: drag-and-drop off to avoid the
                // RoInitialize (multithreaded COM) conflict noted in run_gui.
                .with_drag_and_drop(false),
            |vp_ctx, _class| {
                egui::CentralPanel::default().show(vp_ctx, |ui| {
                    let review = self.state.as_mut().unwrap();
                    render::render_review_window_contents(ui, review, &mut actions, copy_toast);
                });
                if vp_ctx.input(|i| i.viewport().close_requested()) {
                    actions.close = true;
                }
            },
        );
        if let Some(iter) = actions.toggle_expand {
            // Toggle the expanded row; on expand, load that row's screenshot
            // texture so the inline per-stage crops have a source to sample.
            let expand = match self.state.as_mut() {
                Some(r) => {
                    if r.expanded == Some(iter) {
                        r.expanded = None;
                        false
                    } else {
                        r.expanded = Some(iter);
                        true
                    }
                }
                None => false,
            };
            if expand {
                self.load_preview(ctx, iter);
            }
        }
        if let Some(iter) = actions.mark_verified {
            if let Some(review) = self.state.as_mut() {
                if let Some(row) = review.rows.iter_mut().find(|r| r.iteration == iter) {
                    row.recovery = RECOVERY_VERIFIED.to_string();
                    review.dirty = true;
                    crate::log(&format!("GUI: Marked iteration {} verified", iter));
                }
            }
        }
        // A verify click persists immediately (no separate 保存 needed). It
        // routes through the one save path, so an unedited verified row saves as
        // `verified` while a row also edited this frame wins as `manual`.
        let effects = if actions.save || actions.mark_verified.is_some() {
            self.save()
        } else {
            None
        };
        if actions.close {
            if let Some(r) = self.state.as_mut() {
                r.open = false;
            }
        }
        effects
    }
}

/// Builds the per-row editable text buffers from a row's scores.
fn edits_from_rows(rows: &[ReviewRow]) -> Vec<[[String; 3]; 3]> {
    rows.iter()
        .map(|r| {
            let mut e: [[String; 3]; 3] = Default::default();
            for s in 0..3 {
                for c in 0..3 {
                    e[s][c] = r.scores[s][c].to_string();
                }
            }
            e
        })
        .collect()
}
