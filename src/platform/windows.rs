//! Windows-specific platform implementation using HWND and hidden windows for OpenGL rendering

use gpui::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::sync::{Arc, Mutex};
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::video_player::VideoPlayer;

/// Window class name for the hidden video window
const HIDDEN_WINDOW_CLASS: &str = "VideoPlayerHiddenWindow";

/// Create a hidden HWND for OpenGL context and off-screen video rendering
///
/// This function:
/// 1. Registers a window class for the hidden window
/// 2. Creates a hidden window with fixed dimensions (for OpenGL context)
/// 3. Configures the video player with the hidden HWND handle
/// 4. Window is never shown - used only for OpenGL context and FBO rendering
///
/// # Returns
/// The HWND as a usize, or None if creation fails
pub fn create_child_video_surface(
    window: &mut Window,
    video_player: Arc<Mutex<VideoPlayer>>,
) -> Option<usize> {
    // Get the unified window bounds to calculate video area size
    let window_bounds = window.bounds();

    // Calculate video area dimensions (76% width, 75% height)
    let video_width_px = window_bounds.size.width * 0.76;
    let video_height_px = window_bounds.size.height * 0.75;

    // Extract numeric values from Pixels
    let width_str = format!("{}", video_width_px);
    let height_str = format!("{}", video_height_px);
    let video_width: i32 = width_str
        .trim_end_matches("px")
        .parse()
        .unwrap_or(960.0) as i32;
    let video_height: i32 = height_str
        .trim_end_matches("px")
        .parse()
        .unwrap_or(540.0) as i32;

    println!(
        "Creating hidden window with size: {}x{} for OpenGL context",
        video_width, video_height
    );

    unsafe {
        // Register window class for the hidden window
        let h_instance = HINSTANCE(GetModuleHandleW(None).ok()?.0);

        let class_name = PCWSTR::from_raw(
            HIDDEN_WINDOW_CLASS
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect::<Vec<u16>>()
                .as_ptr(),
        );

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW | CS_OWNDC,
            lpfnWndProc: Some(hidden_window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: h_instance,
            hIcon: HICON::default(),
            hCursor: LoadCursorW(None, IDC_ARROW).ok()?,
            hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as *mut _),
            lpszMenuName: PCWSTR::null(),
            lpszClassName: class_name,
            hIconSm: HICON::default(),
        };

        // Register the class (ignore error if already registered)
        let _ = RegisterClassExW(&wc);

        println!("Creating hidden video window with size {}x{}", video_width, video_height);

        // Create a hidden popup window (no parent, not visible)
        // WS_POPUP creates an independent window
        // CS_OWNDC ensures we can create an OpenGL context
        let hidden_hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            w!("Video Player Hidden"),
            WS_POPUP,
            0,
            0,
            video_width,
            video_height,
            None, // No parent window
            None,
            Some(h_instance),
            None,
        ).ok()?;

        if hidden_hwnd.0.is_null() {
            eprintln!("Failed to create hidden window");
            return None;
        }

        println!("Hidden HWND created: {:?}", hidden_hwnd);

        // Do NOT call ShowWindow - keep window hidden
        // Window is only used for OpenGL context, rendering happens to FBO

        let hidden_hwnd_ptr = hidden_hwnd.0 as isize as usize;

        // Pass the hidden HWND to the video player
        if let Ok(mut player) = video_player.lock() {
            player.set_window_handle(hidden_hwnd_ptr);
            println!("Hidden HWND set on video player");
        } else {
            eprintln!("Failed to lock video player mutex");
        }

        Some(hidden_hwnd_ptr)
    }
}

/// Resize the hidden HWND to match new video dimensions
///
/// Note: The hidden window is resized to ensure the OpenGL context has the correct dimensions
/// for FBO creation, but the window itself is never displayed.
///
/// # Arguments
/// * `hidden_handle` - The HWND as a usize
/// * `width` - New width in pixels
/// * `height` - New height in pixels
pub fn resize_child_video_surface(hidden_handle: usize, width: f64, height: f64) {
    unsafe {
        let hidden_hwnd = HWND(hidden_handle as isize as *mut _);
        if hidden_hwnd.0.is_null() {
            return;
        }

        // Resize the hidden window (no position/visibility changes needed)
        let _ = SetWindowPos(
            hidden_hwnd,
            None,
            0,
            0,
            width as i32,
            height as i32,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );

        println!(
            "Resized hidden HWND to {}x{}",
            width as i32, height as i32
        );
    }
}

/// Window procedure for the hidden video window
///
/// This handles basic window messages for the hidden window.
/// The window is never shown, so most messages are not relevant.
unsafe extern "system" fn hidden_window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_DESTROY => {
            println!("Hidden window destroyed");
            LRESULT(0)
        }
        WM_PAINT => {
            // Window is hidden and rendering happens to FBO
            // Just validate the region
            let mut ps = PAINTSTRUCT::default();
            let _hdc = BeginPaint(hwnd, &mut ps);
            EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Stub function for compatibility - no longer needed for hidden windows
///
/// Previously, this added WS_CLIPCHILDREN to prevent GPUI from rendering over child windows.
/// With hidden windows, this is not necessary as there are no visible child windows.
///
/// # Returns
/// Always returns Some(()) for compatibility
pub fn enable_child_window_support(_window: &mut Window) -> Option<()> {
    println!("enable_child_window_support called but not needed for hidden windows");
    Some(())
}
