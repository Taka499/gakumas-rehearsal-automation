//! Reusable "right-click copies this image" wrapper with a fade toast.
//!
//! Attach [`copy_on_right_click`] to any egui image `Response`: a right-click
//! calls the pixel provider, writes the pixels to the system clipboard (via
//! `super::clipboard`), and a pill reading 「📋 コピーしました」 (red
//! 「コピーに失敗しました」 on failure) fades in centered over that image and
//! fades out on its own. The toast state is a single shared slot (a new copy
//! simply replaces the previous toast); the pill is painted inside the same UI
//! pass that drew the image, so the wrapper works identically in the main
//! window and in separate viewports like the review window.
//!
//! New image surfaces opt in by capturing their `Response` and calling
//! [`copy_on_right_click`] with a unique `site` id and a closure producing
//! `(width, height, rgba_bytes)` at the image's native resolution.

use std::time::Instant;

use eframe::egui::{self, Color32, Vec2};

/// Fade timing in seconds: fade in, hold, fade out (1.5 s total).
const FADE_IN: f32 = 0.15;
const HOLD: f32 = 0.9;
const FADE_OUT: f32 = 0.45;

/// One live copy notice. Stored as a single `Option<CopyToast>` slot on
/// `GuiApp`; `site` ties it to the image it reports on so it is painted over
/// that image only.
pub struct CopyToast {
    site: egui::Id,
    text: &'static str,
    is_error: bool,
    born: Instant,
}

/// Copy the provided pixels to the clipboard on right-click, and paint this
/// image's fade toast while it is alive.
///
/// `site` must be unique per image surface (e.g.
/// `egui::Id::new(("copy_stage_crop", iteration, stage))`). `provide` is only
/// invoked on a click; it returns the image at native resolution as
/// `(width, height, tightly-packed RGBA8)`.
pub fn copy_on_right_click(
    ui: &egui::Ui,
    response: &egui::Response,
    site: egui::Id,
    toast: &mut Option<CopyToast>,
    provide: impl FnOnce() -> anyhow::Result<(u32, u32, Vec<u8>)>,
) {
    if response.secondary_clicked() {
        let result =
            provide().and_then(|(w, h, rgba)| super::clipboard::copy_rgba_image(w, h, &rgba));
        *toast = Some(match result {
            Ok(()) => CopyToast {
                site,
                text: "📋 コピーしました",
                is_error: false,
                born: Instant::now(),
            },
            Err(e) => {
                crate::log(&format!("GUI: image copy failed: {:#}", e));
                CopyToast {
                    site,
                    text: "コピーに失敗しました",
                    is_error: true,
                    born: Instant::now(),
                }
            }
        });
    }

    // Paint (or expire) the shared toast only over the image it belongs to.
    if toast.as_ref().is_some_and(|t| t.site == site) {
        let t = toast.as_ref().unwrap();
        match fade_alpha(t.born.elapsed().as_secs_f32()) {
            Some(alpha) => {
                draw_pill(ui, response.rect.center(), t.text, t.is_error, alpha);
                // Keep frames coming while the animation runs, even with no input.
                ui.ctx().request_repaint();
            }
            None => *toast = None,
        }
    }
}

/// Opacity of the toast `elapsed` seconds after creation; `None` once expired.
fn fade_alpha(elapsed: f32) -> Option<f32> {
    if elapsed < FADE_IN {
        Some(elapsed / FADE_IN)
    } else if elapsed < FADE_IN + HOLD {
        Some(1.0)
    } else if elapsed < FADE_IN + HOLD + FADE_OUT {
        Some(1.0 - (elapsed - FADE_IN - HOLD) / FADE_OUT)
    } else {
        None
    }
}

/// Paint the rounded toast pill centered at `center` with the given opacity.
fn draw_pill(ui: &egui::Ui, center: egui::Pos2, text: &str, is_error: bool, alpha: f32) {
    let bg = if is_error {
        Color32::from_rgb(170, 40, 40)
    } else {
        Color32::from_rgb(40, 40, 40)
    };
    let painter = ui.painter();
    let galley = painter.layout_no_wrap(
        text.to_owned(),
        egui::FontId::proportional(14.0),
        Color32::WHITE.gamma_multiply(alpha),
    );
    let pad = Vec2::new(12.0, 7.0);
    let rect = egui::Rect::from_center_size(center, galley.size() + pad * 2.0);
    painter.rect_filled(rect, 8.0, bg.gamma_multiply(0.9 * alpha));
    painter.galley(rect.min + pad, galley, Color32::WHITE);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_alpha_envelope() {
        assert_eq!(fade_alpha(0.0), Some(0.0)); // starts transparent
        assert_eq!(fade_alpha(FADE_IN), Some(1.0)); // fully in
        assert_eq!(fade_alpha(FADE_IN + HOLD * 0.5), Some(1.0)); // holds
        let mid_out = fade_alpha(FADE_IN + HOLD + FADE_OUT * 0.5).unwrap();
        assert!(mid_out > 0.4 && mid_out < 0.6); // fading out
        assert_eq!(fade_alpha(FADE_IN + HOLD + FADE_OUT + 0.01), None); // expired
    }
}
