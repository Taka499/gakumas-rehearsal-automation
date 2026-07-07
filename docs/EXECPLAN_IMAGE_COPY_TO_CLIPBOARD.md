# Copy GUI images to the system clipboard via right-click

This ExecPlan is a living document. The sections `Progress`, `Surprises & Discoveries`, `Decision Log`, and `Outcomes & Retrospective` must be kept up to date as work proceeds.

This document must be maintained in accordance with `docs/PLANS.md` (repository root: `C:\Work\GitRepos\gakumas-rehearsal-automation`).

## Purpose / Big Picture

The GUI shows two kinds of images the user regularly wants to share or archive: the live nine-box score-distribution plot in the right-hand side panel, and the per-stage screenshot crops shown under an expanded row in the OCR review window. Today neither can be extracted without taking a manual screen capture, which loses resolution and picks up surrounding window chrome.

After this change, right-clicking either image copies it — at native pixel resolution, not the on-screen scaled size — straight into the Windows clipboard, and a small notice ("📋 コピーしました") fades in over the clicked image and fades out again to confirm success. The user can then paste the image directly into a chat app, an issue tracker, or an image editor. A failed copy (for example when another process holds the clipboard open) shows the same notice in red ("コピーに失敗しました") instead of failing silently.

The interaction machinery is deliberately built as a small reusable wrapper — one helper that can attach "right-click copies me, with a fade toast" to any egui image — because the user explicitly wants to reuse it on future image surfaces. Only the two surfaces above are wired up in this plan.

## Progress

- [x] (2026-07-07) Design interview complete; all scope and UX decisions recorded in the Decision Log.
- [x] (2026-07-07) M1: `arboard = "3"` added; `src/gui/clipboard.rs` writer + buffer-size guard test + `#[ignore]`d round-trip test. Round-trip could NOT be verified from the agent shell (no clipboard access in that context — see Surprises); it must be run once from an interactive terminal.
- [x] (2026-07-07) M2: `src/gui/copyable.rs` (`CopyToast` + `copy_on_right_click` + fade-envelope unit test); live box plot wired in `src/gui/mod.rs` (texture handles snapshotted first to avoid E0502 vs the `&mut copy_toast` borrow).
- [x] (2026-07-07) M3: toast slot threaded `GuiApp::render_review_window` → `ReviewController::show` → `render_review_window_contents` → `draw_stage_crop`; crop provider re-reads the screenshot PNG and cuts the `review_crop_rect` region at native resolution. `cargo check` clean, 124 unit tests pass, guarded release build links.
- [x] (2026-07-07) First acceptance attempt FAILED on both surfaces: right-click did nothing (root cause: images default to `Sense::hover`, see Surprises). Fixed with `.sense(egui::Sense::click())` at both call sites; 124 tests pass, release rebuilt.
- [x] (2026-07-07) The user's interactive round-trip runs showed NO test-harness output — explained: `#![windows_subsystem = "windows"]` made the test harness a GUI-subsystem exe, which never attaches to an interactive console (output only shows when stdout is a pipe, as in agent shells/CI). Fixed in `src/main.rs` with `#![cfg_attr(not(test), windows_subsystem = "windows")]`; verified via PE header that the release exe stays subsystem 2 (GUI) and the test harness is now 3 (console). The round-trip failure itself is still undiagnosed — output was invisible; awaiting a re-run.
- [x] (2026-07-07) In-app acceptance PASSED on both surfaces after the Sense::click fix (user-confirmed: right-click copies and the toast shows).
- [x] (2026-07-07) Round-trip failure diagnosed with visible output: `arboard::get_image`'s PNG read path fails even on the healthy desktop (write path + real paste targets fine). Test rewritten to read back via WinForms `Clipboard.GetImage` (independent consumer) and pixel-compare a saved temp PNG; Decision Log updated.
- [ ] Final acceptance (manual, needs the interactive desktop): run `GAKUMAS_NO_MANIFEST=1 cargo test clipboard_roundtrip -- --ignored --nocapture` from a normal terminal (expect 1 passed); optionally exercise the failure toast per Validation and Acceptance.

## Surprises & Discoveries

- Observation: `arboard::get_image`'s read path is broken in this setup EVERYWHERE — including the user's healthy interactive desktop — while its write path works and every real paste target (Paint, chat apps) reads the written image fine. arboard prefers its own custom "PNG" clipboard format on read and `GetClipboardData` fails on it ("failed to read clipboard PNG data"; underlying OS error 1418 in the agent-shell diagnostic). Consequence: the round-trip test must read the clipboard back through an independent consumer (WinForms `Clipboard.GetImage` via `powershell -STA`, saved to a temp PNG and pixel-compared), not through arboard.
  Evidence: identical "failed to read clipboard PNG data" panic from the user's interactive terminal (2026-07-07, after the console-subsystem fix made harness output visible) and from the agent shell; in-app copy + paste confirmed working by the user at the same time.
- Observation: The coding-agent shell session additionally has no usable clipboard at all (a separate, environment-specific limitation), so even the fixed round-trip test cannot be validated by an agent — it must be run from an interactive desktop terminal.
  Evidence: PowerShell's own `Set-Clipboard` failed there with "Requested Clipboard operation did not succeed" (STA and non-STA alike, sandboxed and unsandboxed alike), and a `clipboard-win` diagnostic showed `EnumClipboardFormats` yielding nothing right after a successful write.
- Observation: `#![windows_subsystem = "windows"]` (crate-level in `src/main.rs`) silently applied to the TEST harness too, so any `cargo test` run from an interactive console printed nothing — not even `running 1 test` — while agent shells/CI (piped stdout) saw everything. This repo-wide paper cut predates this plan; every past "manual test run" instruction implicitly relied on a piped context. Fixed with `#![cfg_attr(not(test), windows_subsystem = "windows")]`.
  Evidence: user's `--nocapture` run showed cargo's "Running unittests…" line followed immediately by "error: test failed" with nothing in between; after the gate, the harness binary's PE subsystem field reads 3 (console) while the release exe still reads 2 (GUI).
- Observation: egui image widgets (`ui.image(...)` / `egui::Image`) register with `Sense::hover()` by default, so `Response::secondary_clicked()` NEVER fires on a bare image — the first in-app acceptance attempt showed no reaction to right-clicks at all (no clipboard change, no toast). Any image passed to `copy_on_right_click` must be added with `.sense(egui::Sense::click())`.
  Evidence: user's manual acceptance 2026-07-07 ("Right-clicking the image has no change in clipboard and no toast shows up neither"); both call sites fixed by building the widget as `egui::Image::new(…).sense(egui::Sense::click())`.
- Observation: In that clipboard-less context, `arboard::set_image` **succeeds silently** — a write that reports `Ok` proves nothing without a read-back. This is why the round-trip test reads the image back instead of only asserting the write succeeded. Irrelevant to the real GUI app (which runs on the interactive desktop), but it means the error toast cannot be relied on to surface this particular exotic failure mode.
  Evidence: `copy_rgba_image` returned `Ok(())` in the same test runs where all reads failed with error 1418.

## Decision Log

- Decision: Right-click copies immediately — no context menu and no hover tooltip.
  Rationale: Matches the user's stated ideal exactly (right-click → in clipboard → toast). A menu adds a click every time; the user declined the hover-hint variant when offered.
  Date/Author: 2026-07-07 / user interview (grill-me).

- Decision: The success/failure notice is a fade-in/fade-out pill overlaid on the clicked image itself, not a global corner toast and not cursor-attached.
  Rationale: Feedback spatially tied to what was copied is unambiguous when several stage crops are visible at once.
  Date/Author: 2026-07-07 / user interview.

- Decision: On the live side panel, copy captures the box-plot image only. The 6×9 stats table (separate egui widgets, not part of the image) is not composited in and gets no copy affordance in this plan.
  Rationale: The table is not pixels; compositing it would mean a second rendering of the same data to maintain. A copy-as-text for the table can be a later feature if demand appears.
  Date/Author: 2026-07-07 / user interview.

- Decision: Stage-crop copies are bare pixels — exactly the region shown on screen, no burned-in iteration/stage caption.
  Rationale: WYSIWYG; annotation would need Japanese-capable text drawing and would make the copy differ from the display. The review row itself supplies context when the user pastes.
  Date/Author: 2026-07-07 / user interview.

- Decision: Pixels are re-derived on demand at copy time rather than retaining RGBA buffers alongside the GPU textures. The box plot is re-rendered from the cached `DataSetStats`; the stage crop is re-read from the screenshot PNG on disk and cropped.
  Rationale: Both sources reproduce the displayed image exactly at native resolution. Retaining buffers would cost ~3 MB per screenshot / ~3 MB per plot permanently to save a sub-100 ms operation performed rarely.
  Date/Author: 2026-07-07 / design (Claude), user informed.

- Decision: Clipboard writing uses the `arboard` crate (v3).
  Rationale: It is the de-facto Rust clipboard crate, writes CF_DIBV5 on Windows so image pastes work in chat apps and editors, and saves ~60 lines of hand-rolled unsafe `OpenClipboard`/`SetClipboardData` code. The repo is Windows-only so arboard's cross-platform weight is idle but harmless.
  Date/Author: 2026-07-07 / design (Claude), user informed.

- Decision: The right-click-copy + toast behavior must be a reusable component, attachable to future image surfaces without redesign.
  Rationale: Explicit user requirement ("this feature should also be re-usable when new demand appears").
  Date/Author: 2026-07-07 / user interview.

- Decision: The round-trip test verifies the copy through an independent clipboard consumer (WinForms `Clipboard.GetImage` via `powershell -NoProfile -STA`, saving to a temp PNG that the test pixel-compares), not through `arboard::get_image`.
  Rationale: arboard's own read path fails on its custom "PNG" format even on a healthy desktop (see Surprises), and the feature only ever writes — what matters is what paste targets see, and WinForms reads the same DIB path they do.
  Date/Author: 2026-07-07 / debugging session (Claude), after the user's interactive run reproduced the arboard read failure.

- Decision: Scope is exactly two surfaces — live box plot, review stage crops. The finished-panel's generated chart PNGs stay out of scope (they already exist as files reachable via the 📁 button).
  Rationale: User confirmation during the interview.
  Date/Author: 2026-07-07 / user interview.

## Outcomes & Retrospective

- (to be written at completion)

## Context and Orientation

This is a Windows-only Rust application (egui/eframe 0.29 GUI, `eframe = "0.29"` in `Cargo.toml`) that automates rehearsal runs of the game `gakumas.exe`, OCRs score screenshots, and shows results in a GUI. Two GUI images matter here.

**The live box plot.** `src/analysis/charts.rs` has `render_live_box_plot_rgba(stats: &DataSetStats) -> Result<(u32, u32, Vec<u8>)>` (around line 668), which uses the plotters crate to draw a nine-box score-distribution figure into an in-memory buffer and returns `(width, height, rgba_bytes)` — currently 1200×620, constants `LIVE_PLOT_W`/`LIVE_PLOT_H` near line 656. `src/gui/live_chart.rs` defines `LiveChartCache`, which lives on `GuiApp`. Its `update()` method calls that renderer, uploads the RGBA into an egui `TextureHandle` (`ctx.load_texture("live_box_plot", …)`), and keeps the texture, the `DataSetStats` behind it (`stats: Option<DataSetStats>`, accessor `stats()`), and row counts. The RGBA buffer itself is dropped after upload. The GUI draws the texture in the right side panel in `src/gui/mod.rs` (around line 798): `ui.image((tex.id(), Vec2::new(w, w * aspect)))`. Because the cache retains the exact `DataSetStats` used for the last render, calling `render_live_box_plot_rgba` again with those stats reproduces the displayed image bit-for-bit at native resolution — that is the copy source.

**The review stage crops.** The OCR review window lists one row per iteration; expanding a row shows, under each of the three stages' editable score columns, an inline image of that stage's character portraits and printed scores. These crops are not separate images: `src/gui/review.rs::load_preview` (around line 73) opens the row's full screenshot PNG from disk (`review.rows[…].screenshot` holds the path), converts to RGBA, uploads it as one texture, and stores it as `review.preview: Option<(u32 /*iteration*/, TextureHandle)>`. `src/gui/render.rs::draw_stage_crop` (around line 703) then draws a UV sub-rectangle of that texture: it gets the crop rectangle from `crate::automation::review_crop_rect(cfg, stage)` (defined in `src/automation/config.rs`), which returns a `RelativeRect` in window fractions (0..1), and passes it as `.uv(…)` on `egui::Image`. So the on-screen crop is defined entirely by (screenshot path, `review_crop_rect(cfg, stage)`), and the copy source is: re-open that PNG, multiply the relative rect by the image's pixel dimensions, crop, done.

**What does not exist yet.** There is no clipboard code anywhere (no arboard/clipboard-win/copypasta dependency, no use of egui's clipboard), no right-click handling (`secondary_clicked`/`context_menu` unused), and no toast/notification mechanism — feedback today is `crate::log()` lines. All three are introduced by this plan.

**Term: toast.** A short-lived overlay message that appears, holds, and disappears on its own without user interaction. Here it is a rounded pill of text painted over an image's on-screen rectangle.

The GUI is entirely Japanese; all new user-visible strings are Japanese.

## Plan of Work

**Milestone 1 — clipboard writer.** Add `arboard = "3"` to `Cargo.toml`. Create `src/gui/clipboard.rs` with one public function:

    pub fn copy_rgba_image(width: u32, height: u32, rgba: &[u8]) -> anyhow::Result<()>

It constructs `arboard::Clipboard::new()?` and calls `set_image(arboard::ImageData { width: width as usize, height: height as usize, bytes: Cow::Borrowed(rgba) })`. Register the module in `src/gui/mod.rs` (`mod clipboard;`). arboard expects tightly-packed RGBA8, which is exactly what both pixel sources produce. Add a `#[ignore]`d round-trip test (set an image, `get_image`, compare dimensions and a few pixels) — ignored because it touches global system state; run it explicitly with `GAKUMAS_NO_MANIFEST=1 cargo test clipboard_roundtrip -- --ignored`.

**Milestone 2 — reusable wrapper + toast, wired to the live box plot.** Create `src/gui/copyable.rs` holding two pieces.

First, `CopyToast`, a single-slot toast state stored on `GuiApp`:

    pub struct CopyToast {
        site: egui::Id,        // which image the toast belongs to
        text: &'static str,    // "📋 コピーしました" or "コピーに失敗しました"
        is_error: bool,
        born: std::time::Instant,
    }

(One slot is enough: a new copy simply replaces the previous toast.)

Second, the reusable attach-point:

    pub fn copy_on_right_click(
        ui: &egui::Ui,
        response: &egui::Response,
        site: egui::Id,
        toast: &mut Option<CopyToast>,
        provide: impl FnOnce() -> anyhow::Result<(u32, u32, Vec<u8>)>,
    )

If `response.secondary_clicked()`, call `provide()`, pass the pixels to `clipboard::copy_rgba_image`, set `*toast` accordingly, and `crate::log()` the failure detail if any. Then — every frame, not only on click — if `toast` is `Some` and its `site` matches, paint the pill centered on `response.rect` via `ui.painter()` with an alpha computed from `born.elapsed()`: fade in 150 ms, hold 900 ms, fade out 450 ms (total 1.5 s), then clear the slot. While a toast is live, call `ui.ctx().request_repaint()` so the animation advances without user input. Because the pill is painted inside the same UI pass that draws the image, it works identically in the main window and the review window with no cross-viewport plumbing.

Wire the live box plot: in `src/gui/mod.rs` where `ui.image((tex.id(), …))` draws the plot (around line 798), capture the returned `Response` and call `copy_on_right_click` with `site = egui::Id::new("copy_live_plot")` and a provider closure that clones `self.live_chart.stats()` and calls `crate::analysis::charts::render_live_box_plot_rgba` on it. (Snapshot the stats before the closure to avoid borrow conflicts with the `&mut` toast slot; `stats()` is `Some` whenever the texture is, since `LiveChartCache::update` sets both together.) Add the `toast: Option<CopyToast>` field to `GuiApp`.

**Milestone 3 — wire the review stage crops.** In `src/gui/render.rs::draw_stage_crop`, capture the `Response` from `ui.add(egui::Image::new(…).uv(uv))` and call `copy_on_right_click` with `site = egui::Id::new(("copy_stage_crop", iteration, stage))`. The provider closure re-derives the crop from disk: look up the row's screenshot path in `review.rows`, `image::open(path)?.to_rgba8()`, compute pixel coordinates by multiplying the same `review_crop_rect(cfg, stage)` fractions by the image dimensions (rounding, clamped to bounds, minimum 1×1 — mirroring the `max(1.0)` guards already in `draw_stage_crop`), crop with `image::imageops::crop_imm`, and return its raw RGBA. `draw_stage_crop` currently takes `review: &ReviewState`; it needs access to the toast slot, so thread `&mut Option<CopyToast>` (or the `GuiApp` field) down through its caller — follow the existing pattern used to pass `ReviewState` in, and keep the change mechanical. Note the crop is only drawn once the preview texture is loaded (otherwise the "画像を読み込み中…" label shows instead), so a right-click can never race the texture load; the disk file it re-reads is the same one `load_preview` already opened.

No changes to `reconcile.rs`, the automation state machine, or CSV handling. The feature is GUI-only and additive.

## Concrete Steps

All commands run from the repository root `C:\Work\GitRepos\gakumas-rehearsal-automation` in PowerShell.

Build with the guarded wrapper (a running app instance locks the exe and a bare cargo build fails only at the link step):

    powershell -ExecutionPolicy Bypass -File scripts/build.ps1

Expect ~30 known warnings (unused `pub use` re-exports, OCR dead code); check for real failures with:

    cargo check 2>&1 | grep "^error"

Unit tests (the admin manifest requires this env var to run unelevated):

    $env:GAKUMAS_NO_MANIFEST=1; cargo test

Clipboard round-trip test, explicitly:

    $env:GAKUMAS_NO_MANIFEST=1; cargo test clipboard_roundtrip -- --ignored

## Validation and Acceptance

Milestone 1: the round-trip test passes — it sets a small known RGBA image on the clipboard, reads it back through an independent consumer (WinForms `Clipboard.GetImage` via PowerShell, saved to a temp PNG), and pixel-compares. IMPORTANT: it must be run from an interactive desktop terminal, not from an agent/CI shell — those contexts have no clipboard access and the test fails there for environmental reasons (exit code 2 from the consumer means no image was readable; see Surprises & Discoveries). Do not "fix" it to read back via `arboard::get_image`: that path is broken even on healthy desktops.

Milestone 2 (manual): launch `.\target\release\gakumas-rehearsal-automation.exe`, ensure the live distribution panel is enabled (it defaults on). Right-click the box-plot image. A pill reading 📋 コピーしました fades in centered on the plot and fades out within ~1.5 s. Open ペイント (MS Paint) and paste: the full figure appears at 1200×620, regardless of how narrow the side panel was on screen. With no run data the empty figure still renders — right-click must still copy that empty figure successfully.

Milestone 3 (manual): open the review window on a session with rows, expand a row (📷), right-click one stage's inline crop. The toast appears over that specific crop; pasting into Paint shows just that stage's portraits+scores strip at the screenshot's native crop resolution (for a 721×1281 screenshot, roughly a wide short strip, not the tiny on-screen size). Repeat on a different stage of the same row and confirm the toast moves to the newly clicked crop.

Failure path (manual): simulate a busy clipboard (e.g. run a tight loop holding the clipboard open, or temporarily make `copy_rgba_image` return an error) and confirm the red コピーに失敗しました toast appears and a failure line lands in `logs/gakumas_screenshot.log`.

## Idempotence and Recovery

All changes are additive GUI code plus one dependency; re-running builds and tests is always safe. If arboard misbehaves at runtime the failure is contained to the error toast — no automation, OCR, or CSV path is touched. Rolling back is deleting the two new modules, the call sites, and the Cargo entry.

## Interfaces and Dependencies

New dependency in `Cargo.toml`: `arboard = "3"` (image support is in its default features).

In `src/gui/clipboard.rs`:

    pub fn copy_rgba_image(width: u32, height: u32, rgba: &[u8]) -> anyhow::Result<()>

In `src/gui/copyable.rs`:

    pub struct CopyToast { /* site, text, is_error, born */ }

    pub fn copy_on_right_click(
        ui: &egui::Ui,
        response: &egui::Response,
        site: egui::Id,
        toast: &mut Option<CopyToast>,
        provide: impl FnOnce() -> anyhow::Result<(u32, u32, Vec<u8>)>,
    )

`GuiApp` (in `src/gui/mod.rs`) gains `copy_toast: Option<CopyToast>`. Existing functions reused, not modified: `crate::analysis::charts::render_live_box_plot_rgba`, `crate::automation::review_crop_rect`, `LiveChartCache::stats()`.
