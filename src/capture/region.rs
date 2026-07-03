//! Region capture functionality for extracting sub-areas of the game window.
//!
//! This module provides the ability to capture arbitrary rectangular regions
//! of the game window, which is used for brightness detection and OCR. The
//! actual D3D11 capture work is shared with full-window screenshots via
//! `super::screenshot::capture_and_crop`; this module only owns the
//! relative→absolute coordinate math.

use anyhow::Result;
use image::{ImageBuffer, Rgba};

use windows::Win32::Foundation::HWND;

use crate::automation::RelativeRect;

use super::screenshot::{capture_and_crop, CropBox};
use super::window::get_client_area_info;

/// Captures a rectangular region of the game window.
///
/// The region is specified in relative coordinates (0.0 to 1.0) which are
/// converted to absolute pixel coordinates based on the window's client area size.
///
/// Returns an ImageBuffer containing the captured region in RGBA format.
pub fn capture_region(hwnd: HWND, rel_rect: &RelativeRect) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>> {
    let (client_rect, client_offset) = get_client_area_info(hwnd)?;
    let client_width = (client_rect.right - client_rect.left) as u32;
    let client_height = (client_rect.bottom - client_rect.top) as u32;

    // Convert relative coordinates to absolute pixels within client area
    let region_x = (rel_rect.x * client_width as f32) as u32;
    let region_y = (rel_rect.y * client_height as f32) as u32;
    let region_width = (rel_rect.width * client_width as f32) as u32;
    let region_height = (rel_rect.height * client_height as f32) as u32;

    // Clamp to valid bounds
    let region_x = region_x.min(client_width.saturating_sub(1));
    let region_y = region_y.min(client_height.saturating_sub(1));
    let region_width = region_width.min(client_width - region_x).max(1);
    let region_height = region_height.min(client_height - region_y).max(1);

    // Absolute crop position within the captured frame (client offset + region offset)
    capture_and_crop(
        hwnd,
        CropBox {
            x: client_offset.x as u32 + region_x,
            y: client_offset.y as u32 + region_y,
            width: region_width,
            height: region_height,
        },
        false,
    )
}
