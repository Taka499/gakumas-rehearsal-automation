//! Cache for the live score-distribution figure (nine-box plot + stats table).
//!
//! Owns everything that describes one rendered live figure — the uploaded egui
//! texture, the statistics behind the table, the included/excluded row counts,
//! and the change-detection state — so the pieces can never drift apart. The
//! GUI calls [`LiveChartCache::update`] once per frame; it re-renders only when
//! the live buffer grew or [`LiveChartCache::invalidate`] was called.

use eframe::egui::{self, TextureHandle};

use crate::analysis::statistics::DataSetStats;

/// Cached live distribution figure. Lives on `GuiApp` (not `GuiState`) because
/// `TextureHandle` is not `Debug`.
pub struct LiveChartCache {
    /// Cached figure texture, rebuilt while a run is in progress.
    tex: Option<TextureHandle>,
    /// Live-buffer row count the cached texture was rendered from; used to
    /// re-render the figure only when new iteration data has arrived.
    rendered_count: usize,
    /// Latest per-column statistics for the live table (parallel to `tex`).
    stats: Option<DataSetStats>,
    /// Included run count and flagged-excluded count for the figure's heading.
    total: usize,
    excluded: usize,
    /// Forces a re-render on the next frame even if the buffer row count is
    /// unchanged. Set after a review save so manual corrections / verifications
    /// (which can change values or flags without changing the row count) are
    /// reflected.
    dirty: bool,
}

impl LiveChartCache {
    pub fn new() -> Self {
        Self {
            tex: None,
            rendered_count: 0,
            stats: None,
            total: 0,
            excluded: 0,
            dirty: false,
        }
    }

    /// Re-render the live distribution figure (and the statistics behind the
    /// table) when `enabled` and new iteration data has arrived since the last
    /// render. Runs whether or not a run is in progress, so the empty figure is
    /// already visible the moment the user enables it (or on launch when the
    /// preference is on). Flagged rows are excluded from the statistics (kept
    /// in the buffer but not plotted) until verified. Cheap on idle frames
    /// thanks to the row-count guard; only re-renders on a new data point.
    pub fn update(&mut self, ctx: &egui::Context, enabled: bool) {
        if !enabled {
            return;
        }

        let count = crate::automation::runner::live_score_count();
        if !self.dirty && count == self.rendered_count && self.tex.is_some() {
            return; // No new data and not force-invalidated since the last render.
        }

        let rows = crate::automation::runner::get_live_scores();
        let included: Vec<[[u32; 3]; 3]> = rows
            .iter()
            .filter(|r| !r.flagged)
            .map(|r| r.scores)
            .collect();
        let excluded = rows.len() - included.len();
        let stats = DataSetStats::from_score_rows(&included);

        match crate::analysis::charts::render_live_box_plot_rgba(&stats) {
            Ok((w, h, rgba)) => {
                let color =
                    egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                let tex = ctx.load_texture("live_box_plot", color, egui::TextureOptions::LINEAR);
                self.tex = Some(tex);
                self.total = included.len();
                self.excluded = excluded;
                self.stats = Some(stats);
                self.rendered_count = count;
                self.dirty = false;
            }
            Err(e) => {
                crate::log(&format!("Live distribution: render failed ({})", e));
            }
        }
    }

    /// Force a re-render on the next `update` call even if the live-buffer row
    /// count is unchanged (e.g. after a review save rewrote values/flags).
    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    pub fn texture(&self) -> Option<&TextureHandle> {
        self.tex.as_ref()
    }

    pub fn stats(&self) -> Option<&DataSetStats> {
        self.stats.as_ref()
    }

    /// (included, flagged-excluded) row counts of the current figure.
    pub fn counts(&self) -> (usize, usize) {
        (self.total, self.excluded)
    }
}
