//! Window discovery functions for finding the gakumas.exe game window.

use anyhow::{anyhow, Result};
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;

use windows::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT, TRUE};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClientRect, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible,
};

/// The exact process name to match (case-insensitive).
const GAKUMAS_PROCESS_NAME: &str = "gakumas.exe";

/// Finds the main window of gakumas.exe by enumerating all visible windows
/// and matching the process executable name.
///
/// Returns the window handle (HWND) if found, or an error if the game is not running.
pub fn find_gakumas_window() -> Result<HWND> {
    struct EnumData {
        hwnd: Option<HWND>,
        process_name: Option<String>,
        debug: bool,
    }

    unsafe extern "system" fn enum_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        unsafe {
            let data = &mut *(lparam.0 as *mut EnumData);

            // Skip invisible windows
            if !IsWindowVisible(hwnd).as_bool() {
                return TRUE;
            }

            // Get window title for debug
            let title_len = GetWindowTextLengthW(hwnd);
            let title = if title_len > 0 {
                let mut title_buf: Vec<u16> = vec![0; (title_len + 1) as usize];
                GetWindowTextW(hwnd, &mut title_buf);
                OsString::from_wide(&title_buf[..title_len as usize])
                    .to_string_lossy()
                    .to_string()
            } else {
                String::new()
            };

            // Skip windows without title (usually not main windows)
            if title.is_empty() {
                return TRUE;
            }

            // Get process ID from window
            let mut process_id: u32 = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut process_id));
            if process_id == 0 {
                return TRUE;
            }

            // Open the process to get its name
            let process_handle =
                OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id);
            let Ok(process_handle) = process_handle else {
                if data.debug {
                    crate::log(&format!(
                        "  [{}] \"{}\" - failed to open process",
                        process_id, title
                    ));
                }
                return TRUE;
            };

            // Get process executable name
            let mut name_buf: Vec<u16> = vec![0; 1024];
            let mut len = name_buf.len() as u32;
            let result = QueryFullProcessImageNameW(
                process_handle,
                PROCESS_NAME_WIN32,
                windows::core::PWSTR(name_buf.as_mut_ptr()),
                &mut len,
            );
            let _ = windows::Win32::Foundation::CloseHandle(process_handle);

            if result.is_err() || len == 0 {
                if data.debug {
                    crate::log(&format!(
                        "  [{}] \"{}\" - failed to get process name",
                        process_id, title
                    ));
                }
                return TRUE;
            }

            let full_path = OsString::from_wide(&name_buf[..len as usize])
                .to_string_lossy()
                .to_string();
            // Extract just the filename from the full path
            let process_name = full_path
                .rsplit('\\')
                .next()
                .unwrap_or(&full_path)
                .to_string();
            let process_name_lower = process_name.to_lowercase();

            if data.debug {
                crate::log(&format!(
                    "  [{}] {} - \"{}\"",
                    process_id, process_name, title
                ));
            }

            // Check if this is exactly gakumas.exe (not gakumas-rehearsal-automation.exe, etc.)
            if process_name_lower == GAKUMAS_PROCESS_NAME {
                data.hwnd = Some(hwnd);
                data.process_name = Some(process_name);
                return BOOL(0); // Stop enumeration
            }

            TRUE
        }
    }

    crate::log("Searching for gakumas.exe window...");
    crate::log("Listing visible windows:");
    let mut data = EnumData {
        hwnd: None,
        process_name: None,
        debug: true,
    };
    unsafe {
        // Don't use ? here - EnumWindows returns FALSE when callback stops it early,
        // which is expected behavior, not an error
        let _ = EnumWindows(Some(enum_callback), LPARAM(&mut data as *mut _ as isize));
    }

    if let Some(name) = &data.process_name {
        crate::log(&format!("Found process: \"{}\"", name));
    }

    data.hwnd
        .ok_or_else(|| anyhow!("Could not find gakumas.exe window. Is the game running?"))
}

/// Gets the client area rectangle and its offset relative to the window origin.
///
/// The client area is the drawable portion of the window, excluding the title bar
/// and window borders. The offset tells how far the client area is from the
/// top-left corner of the full window (needed for cropping screenshots).
///
/// Returns a tuple of (client_rect, offset) where offset is the position of the
/// client area's top-left corner relative to the window's top-left corner.
pub fn get_client_area_info(hwnd: HWND) -> Result<(RECT, POINT)> {
    let mut client_rect = RECT::default();
    unsafe { GetClientRect(hwnd, &mut client_rect)? };

    // Get the offset of client area relative to window
    let mut client_origin = POINT { x: 0, y: 0 };
    unsafe {
        if !ClientToScreen(hwnd, &mut client_origin).as_bool() {
            return Err(anyhow!("ClientToScreen failed"));
        }
    }

    let mut window_rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut window_rect)? };

    let offset = POINT {
        x: client_origin.x - window_rect.left,
        y: client_origin.y - window_rect.top,
    };

    Ok((client_rect, offset))
}
