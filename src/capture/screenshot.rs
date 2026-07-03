//! Screenshot capture using Windows Graphics Capture API.
//!
//! All capture variants (full client area to file, full client area to buffer,
//! arbitrary sub-region to buffer) share one D3D11 pipeline: `capture_and_crop`.

use anyhow::{anyhow, Context, Result};
use chrono::Local;
use image::{ImageBuffer, Rgba};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use windows::core::Interface;
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem};
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Resource, ID3D11Texture2D,
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ, D3D11_SDK_VERSION,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::System::WinRT::Direct3D11::CreateDirect3D11DeviceFromDXGIDevice;
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;

use super::window::{find_gakumas_window, get_client_area_info};

/// A rectangle to extract from the captured frame, in full-window coordinates
/// (i.e. including the client-area offset from the window's top-left).
pub(crate) struct CropBox {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Captures a screenshot of the gakumas.exe game window.
///
/// This function:
/// 1. Finds the game window
/// 2. Captures it via the shared D3D11 pipeline, cropped to the client area
///    (excluding title bar and borders)
/// 3. Saves as a PNG file with timestamp
///
/// Returns the path to the saved screenshot file.
pub fn capture_gakumas() -> Result<PathBuf> {
    crate::log("Starting capture...");

    let hwnd = find_gakumas_window()?;
    crate::log(&format!("Window handle: {:?}", hwnd));

    let (client_rect, client_offset) = get_client_area_info(hwnd)?;
    let client_width = client_rect.right - client_rect.left;
    let client_height = client_rect.bottom - client_rect.top;
    crate::log(&format!(
        "Client area: {}x{} at offset ({}, {})",
        client_width, client_height, client_offset.x, client_offset.y
    ));

    let img = capture_and_crop(
        hwnd,
        CropBox {
            x: client_offset.x as u32,
            y: client_offset.y as u32,
            width: client_width as u32,
            height: client_height as u32,
        },
        true,
    )?;

    // Save to file
    crate::log("Saving image...");
    let timestamp = Local::now().format("%Y%m%d_%H%M%S");
    let filename = format!("gakumas_{}.png", timestamp);
    let path = crate::paths::get_screenshots_dir().join(&filename);

    img.save(&path)?;
    crate::log(&format!("Saved to {}", crate::paths::relative_display(&path)));

    Ok(path)
}

/// Captures a screenshot of the specified game window and returns it as an ImageBuffer.
///
/// This is similar to `capture_gakumas` but:
/// - Takes an HWND parameter instead of finding the window
/// - Returns the image data instead of saving to file
/// - Does not log
pub fn capture_gakumas_to_buffer(hwnd: HWND) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>> {
    let (client_rect, client_offset) = get_client_area_info(hwnd)?;
    let client_width = client_rect.right - client_rect.left;
    let client_height = client_rect.bottom - client_rect.top;

    capture_and_crop(
        hwnd,
        CropBox {
            x: client_offset.x as u32,
            y: client_offset.y as u32,
            width: client_width as u32,
            height: client_height as u32,
        },
        false,
    )
}

/// Runs the full Windows Graphics Capture pipeline for `hwnd` and returns the
/// sub-image described by `crop`.
///
/// The pipeline: create D3D11 device → create capture item → frame pool →
/// capture session (cursor disabled) → wait for a frame (5s timeout) → copy to
/// a CPU-readable staging texture → map → per-pixel BGRA→RGBA crop copy →
/// unmap → close.
///
/// `verbose` gates all logging: brightness detection polls this several times a
/// second via `capture_region`, so only the interactive screenshot hotkey logs.
pub(crate) fn capture_and_crop(
    hwnd: HWND,
    crop: CropBox,
    verbose: bool,
) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>> {
    let log_v = |msg: &str| {
        if verbose {
            crate::log(msg);
        }
    };

    // Create D3D11 device
    log_v("Creating D3D11 device...");
    let (device, context) = create_d3d11_device()?;
    log_v("D3D11 device created");

    // Create capture item from window
    log_v("Creating capture item...");
    let item = create_capture_item(hwnd, verbose)?;
    log_v("Capture item created");
    let size = item.Size()?;
    if verbose {
        crate::log(&format!("Capture size: {}x{}", size.Width, size.Height));
    }

    // Create frame pool
    log_v("Creating Direct3D device wrapper...");
    let d3d_device = create_direct3d_device(&device)?;
    log_v("Creating frame pool...");
    let frame_pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
        &d3d_device,
        DirectXPixelFormat::B8G8R8A8UIntNormalized,
        1,
        size,
    )?;

    // Create capture session
    log_v("Creating capture session...");
    let session = frame_pool.CreateCaptureSession(&item)?;
    log_v("Capture session created");

    // Disable cursor capture to exclude cursor from screenshots
    session.SetIsCursorCaptureEnabled(false)?;

    // Set up frame arrival handling
    let frame_arrived = Arc::new(AtomicBool::new(false));
    let frame_arrived_clone = frame_arrived.clone();

    frame_pool.FrameArrived(&TypedEventHandler::new(
        move |_pool: &Option<Direct3D11CaptureFramePool>, _| {
            frame_arrived_clone.store(true, Ordering::SeqCst);
            Ok(())
        },
    ))?;

    // Start capture
    log_v("Starting capture session...");
    session.StartCapture()?;
    log_v("Capture started, waiting for frame...");

    // Wait for frame
    let start = std::time::Instant::now();
    while !frame_arrived.load(Ordering::SeqCst) {
        if start.elapsed().as_secs() > 5 {
            return Err(anyhow!("Timeout waiting for frame"));
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    log_v("Frame arrived");

    // Get the frame
    log_v("Getting frame...");
    let frame = frame_pool.TryGetNextFrame()?;
    log_v("Got frame");
    let surface = frame.Surface()?;

    // Get the D3D11 texture from the surface
    let access: windows::Win32::System::WinRT::Direct3D11::IDirect3DDxgiInterfaceAccess =
        surface.cast()?;
    let texture: ID3D11Texture2D = unsafe { access.GetInterface()? };

    // Get texture description
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    unsafe { texture.GetDesc(&mut desc) };

    // Create staging texture for CPU read
    let staging_desc = D3D11_TEXTURE2D_DESC {
        Width: desc.Width,
        Height: desc.Height,
        MipLevels: 1,
        ArraySize: 1,
        Format: desc.Format,
        SampleDesc: desc.SampleDesc,
        Usage: D3D11_USAGE_STAGING,
        BindFlags: Default::default(),
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        MiscFlags: Default::default(),
    };

    let staging_texture = unsafe {
        let mut staging: Option<ID3D11Texture2D> = None;
        device.CreateTexture2D(&staging_desc, None, Some(&mut staging))?;
        staging.ok_or_else(|| anyhow!("Failed to create staging texture"))?
    };

    // Copy to staging texture
    unsafe {
        context.CopyResource(
            &staging_texture.cast::<ID3D11Resource>()?,
            &texture.cast::<ID3D11Resource>()?,
        );
    }

    // Map the staging texture
    let mapped = unsafe {
        let mut mapped = Default::default();
        context.Map(
            &staging_texture.cast::<ID3D11Resource>()?,
            0,
            D3D11_MAP_READ,
            0,
            Some(&mut mapped),
        )?;
        mapped
    };

    if verbose {
        crate::log(&format!(
            "Cropping from ({}, {}) size {}x{}",
            crop.x, crop.y, crop.width, crop.height
        ));
    }

    // Create image from mapped data (cropped to the requested box)
    let mut img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(crop.width, crop.height);

    let src_data = unsafe {
        std::slice::from_raw_parts(
            mapped.pData as *const u8,
            (mapped.RowPitch * desc.Height) as usize,
        )
    };
    let row_pitch = mapped.RowPitch as usize;

    for y in 0..crop.height {
        let src_y = (crop.y + y) as usize;
        if src_y >= desc.Height as usize {
            break;
        }
        for x in 0..crop.width {
            let src_x = (crop.x + x) as usize;
            if src_x >= desc.Width as usize {
                break;
            }
            let offset = src_y * row_pitch + src_x * 4;
            // BGRA -> RGBA
            let b = src_data[offset];
            let g = src_data[offset + 1];
            let r = src_data[offset + 2];
            let a = src_data[offset + 3];
            img.put_pixel(x, y, Rgba([r, g, b, a]));
        }
    }

    // Unmap
    unsafe {
        context.Unmap(&staging_texture.cast::<ID3D11Resource>()?, 0);
    }

    // Stop capture
    session.Close()?;
    frame_pool.Close()?;

    Ok(img)
}

/// Creates a Direct3D 11 device and immediate context.
///
/// The device is used for GPU-accelerated graphics operations.
/// The context is used to issue rendering commands.
fn create_d3d11_device() -> Result<(ID3D11Device, ID3D11DeviceContext)> {
    let mut device: Option<ID3D11Device> = None;
    let mut context: Option<ID3D11DeviceContext> = None;

    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            None,
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )?;
    }

    Ok((
        device.ok_or_else(|| anyhow!("Failed to create D3D11 device"))?,
        context.ok_or_else(|| anyhow!("Failed to create D3D11 context"))?,
    ))
}

/// Creates a WinRT Direct3D device wrapper from a D3D11 device.
///
/// This wrapper is required by the Windows Graphics Capture API.
fn create_direct3d_device(
    device: &ID3D11Device,
) -> Result<windows::Graphics::DirectX::Direct3D11::IDirect3DDevice> {
    let dxgi_device: windows::Win32::Graphics::Dxgi::IDXGIDevice = device.cast()?;
    let inspectable = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi_device)? };
    inspectable
        .cast()
        .context("Failed to cast to IDirect3DDevice")
}

/// Creates a GraphicsCaptureItem for the specified window.
///
/// The capture item represents the window that will be captured.
fn create_capture_item(hwnd: HWND, verbose: bool) -> Result<GraphicsCaptureItem> {
    let class_name = windows::core::h!("Windows.Graphics.Capture.GraphicsCaptureItem");
    if verbose {
        crate::log("Getting activation factory...");
    }
    let interop: IGraphicsCaptureItemInterop = unsafe {
        windows::Win32::System::WinRT::RoGetActivationFactory(class_name)
            .context("Failed to get IGraphicsCaptureItemInterop")?
    };
    if verbose {
        crate::log("Got activation factory");
        crate::log(&format!("Creating capture item for window {:?}...", hwnd));
    }

    unsafe {
        interop
            .CreateForWindow(hwnd)
            .context("Failed to create capture item for window")
    }
}
