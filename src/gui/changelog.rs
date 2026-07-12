//! In-app 更新履歴 (release history) window.
//!
//! The repo-root `CHANGELOG.md` is embedded into the binary at compile time,
//! so the displayed history always matches the running exe and needs no
//! network or disk access. Shipping the file in the release zip would NOT
//! work: the updater never overwrites an existing root file (see
//! `src/update/install.rs::stage_from_zip`), so it would go stale after the
//! first auto-update. Per docs/EXECPLAN_CHANGELOG_AND_JP_NOTES.md.
//!
//! The file is bilingual — Japanese for users, `### English` subsections for
//! maintainers — and this window shows the Japanese parts only.

use eframe::egui;

/// The full bilingual changelog, embedded at compile time.
pub const CHANGELOG_MD: &str = include_str!("../../CHANGELOG.md");

/// Returns the changelog with the maintainer-facing parts removed: everything
/// before the first version heading (`## `) is dropped (format comment, file
/// title), and each `### English` subsection is dropped from that heading up
/// to (excluding) the next `## ` line.
pub fn japanese_only(md: &str) -> String {
    let mut out = String::new();
    let mut seen_version = false;
    let mut in_english = false;
    for line in md.lines() {
        if line.starts_with("## ") {
            seen_version = true;
            in_english = false;
        } else if line.trim() == "### English" {
            in_english = true;
        }
        if seen_version && !in_english {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Renders the (already filtered) changelog text: version headings strong and
/// larger, bullets indented, blank lines as small gaps. Deliberately not a
/// markdown engine — the CHANGELOG format (see its header comment) only uses
/// these three constructs.
pub fn render_changelog(ui: &mut egui::Ui, text: &str) {
    let mut first_heading = true;
    for line in text.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            if !first_heading {
                ui.add_space(10.0);
                ui.separator();
            }
            first_heading = false;
            ui.add_space(4.0);
            ui.label(egui::RichText::new(heading).strong().size(16.0));
            ui.add_space(2.0);
        } else if let Some(bullet) = line.strip_prefix("- ") {
            ui.horizontal_wrapped(|ui| {
                ui.add_space(8.0);
                ui.label(format!("• {}", bullet));
            });
        } else if line.trim().is_empty() {
            ui.add_space(4.0);
        } else {
            // The one-line Japanese summary under each version heading.
            ui.label(egui::RichText::new(line).italics());
        }
    }
}

/// Shows the 更新履歴 window while `open` is true (the window's own close
/// button clears it).
pub fn show_window(ctx: &egui::Context, open: &mut bool) {
    if !*open {
        return;
    }
    egui::Window::new("更新履歴")
        .open(open)
        .collapsible(false)
        .default_width(520.0)
        .default_height(480.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    render_changelog(ui, &japanese_only(CHANGELOG_MD));
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "<!--\nformat rules\n-->\n\n# Title\n\n## v0.2.0 — 2026-02-01\nJP summary two\n\n- JP bullet A\n\n### English\n- EN bullet A\n- EN bullet B\n\n## v0.1.0 — 2026-01-01\nJP summary one\n\n- JP bullet B\n\n### English\n- EN bullet C\n";

    #[test]
    fn drops_preamble_before_first_version() {
        let out = japanese_only(SAMPLE);
        assert!(!out.contains("format rules"));
        assert!(!out.contains("# Title"));
        assert!(out.starts_with("## v0.2.0"));
    }

    #[test]
    fn drops_english_subsections_keeps_japanese() {
        let out = japanese_only(SAMPLE);
        assert!(!out.contains("English"));
        assert!(!out.contains("EN bullet"));
        assert!(out.contains("JP summary two"));
        assert!(out.contains("- JP bullet A"));
        assert!(out.contains("JP summary one"));
        assert!(out.contains("- JP bullet B"));
    }

    #[test]
    fn both_version_sections_survive() {
        let out = japanese_only(SAMPLE);
        assert!(out.contains("## v0.2.0"));
        assert!(out.contains("## v0.1.0"));
    }

    #[test]
    fn trailing_english_section_drops_to_eof() {
        // The oldest entry's English subsection has no following "## " line;
        // it must still be dropped (skip runs to end of file).
        let out = japanese_only(SAMPLE);
        assert!(!out.contains("EN bullet C"));
    }

    /// The real embedded CHANGELOG.md must satisfy the format the window
    /// depends on: at least one version heading, no English leaking through
    /// the filter, and every version section present after filtering.
    #[test]
    fn embedded_changelog_filters_cleanly() {
        let out = japanese_only(CHANGELOG_MD);
        assert!(out.starts_with("## v"));
        assert!(!out.contains("### English"));
        let versions = CHANGELOG_MD.lines().filter(|l| l.starts_with("## v")).count();
        let kept = out.lines().filter(|l| l.starts_with("## v")).count();
        assert_eq!(versions, kept, "every version section must survive filtering");
        assert!(versions >= 15, "full backfill expected (got {})", versions);
    }
}
