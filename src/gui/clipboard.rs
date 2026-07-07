//! System-clipboard image writer.
//!
//! One thin seam over the `arboard` crate so GUI code can put pixels on the
//! Windows clipboard with a single call. Kept separate from `copyable.rs`
//! (the right-click + toast interaction wrapper) so future non-GUI callers
//! can copy an image without pulling in egui types.

use std::borrow::Cow;

use anyhow::Context;

/// Copy a tightly-packed RGBA8 image to the system clipboard. `arboard`
/// writes it as CF_DIBV5 on Windows, so pasting works in chat apps and
/// image editors alike.
pub fn copy_rgba_image(width: u32, height: u32, rgba: &[u8]) -> anyhow::Result<()> {
    anyhow::ensure!(
        rgba.len() == width as usize * height as usize * 4,
        "RGBA buffer length {} does not match {}x{}x4",
        rgba.len(),
        width,
        height
    );
    let mut clipboard = arboard::Clipboard::new().context("Failed to open system clipboard")?;
    clipboard
        .set_image(arboard::ImageData {
            width: width as usize,
            height: height as usize,
            bytes: Cow::Borrowed(rgba),
        })
        .context("Failed to write image to clipboard")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trips a small image through the real system clipboard. `#[ignore]`d
    /// because it mutates global system state (and clobbers whatever the user
    /// had copied); run explicitly with
    /// `GAKUMAS_NO_MANIFEST=1 cargo test clipboard_roundtrip -- --ignored`.
    ///
    /// MUST be run from an interactive desktop terminal. Processes spawned
    /// from agent/service contexts have no clipboard access — and note that in
    /// such contexts `set_image` "succeeds" silently, so a passing *write*
    /// proves nothing without this read-back.
    ///
    /// The read-back deliberately goes through an INDEPENDENT consumer
    /// (WinForms `Clipboard.GetImage` via PowerShell — the same DIB path real
    /// paste targets use), NOT `arboard::get_image`: arboard's own read prefers
    /// the custom "PNG" clipboard format and fails on it ("failed to read
    /// clipboard PNG data") even on a healthy interactive desktop where every
    /// real application pastes the image fine. Our feature only ever writes,
    /// so the test should assert what paste targets see.
    #[test]
    #[ignore]
    fn clipboard_roundtrip() {
        // Opaque test pattern: alpha 255 everywhere, because the DIB paste
        // path round-trips opaque pixels losslessly while transparent ones
        // may come back premultiplied.
        let (w, h) = (4u32, 2u32);
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for i in 0..(w * h) as u8 {
            rgba.extend_from_slice(&[i * 25, 128, 255 - i * 25, 255]);
        }
        copy_rgba_image(w, h, &rgba).expect("copy should succeed");

        let out = std::env::temp_dir().join("gakumas_clipboard_roundtrip.png");
        let _ = std::fs::remove_file(&out);
        let script = format!(
            "Add-Type -AssemblyName System.Windows.Forms; \
             Add-Type -AssemblyName System.Drawing; \
             $i = [System.Windows.Forms.Clipboard]::GetImage(); \
             if ($i -eq $null) {{ exit 2 }}; \
             $i.Save('{}', [System.Drawing.Imaging.ImageFormat]::Png)",
            out.display()
        );
        let status = std::process::Command::new("powershell")
            .args(["-NoProfile", "-STA", "-Command", &script])
            .status()
            .expect("failed to launch powershell");
        assert!(
            status.success(),
            "external clipboard consumer could not read an image back (status {:?}); \
             exit code 2 means no image was on the clipboard",
            status.code()
        );

        let img = image::open(&out).expect("read the PNG the consumer saved").to_rgba8();
        assert_eq!((img.width(), img.height()), (w, h));
        assert_eq!(img.into_raw(), rgba, "pixels differ after round-trip");
        let _ = std::fs::remove_file(&out);
    }

    #[test]
    fn rejects_mismatched_buffer() {
        let err = copy_rgba_image(10, 10, &[0u8; 8]).unwrap_err();
        assert!(err.to_string().contains("does not match"));
    }
}
