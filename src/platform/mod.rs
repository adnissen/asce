//! Platform-specific window handling
//!
//! This module provides platform-specific implementations for creating child windows/views
//! for video rendering within GPUI windows.

use gpui::{Bounds, Pixels, Window};
use raw_window_handle::RawWindowHandle;
use std::sync::{Arc, Mutex};

use crate::video_player::VideoPlayer;

/// Platform-specific window information needed for creating video rendering surface
pub struct WindowInfo {
    pub raw_handle: RawWindowHandle,
    pub bounds: Bounds<Pixels>,
}

/// Create a platform-specific child window/view for video rendering
///
/// This function extracts the platform-specific window handle and creates
/// an appropriate child surface for video rendering.
///
/// # Arguments
/// * `window` - The GPUI window
/// * `video_player` - The video player instance to configure
///
/// # Returns
/// The platform-specific handle (NSView pointer on macOS, HWND on Windows) as a usize
#[cfg(target_os = "macos")]
pub fn create_child_video_surface(
    window: &mut Window,
    video_player: Arc<Mutex<VideoPlayer>>,
) -> Option<usize> {
    macos::create_child_video_surface(window, video_player)
}

#[cfg(target_os = "windows")]
pub fn create_child_video_surface(
    window: &mut Window,
    video_player: Arc<Mutex<VideoPlayer>>,
) -> Option<usize> {
    windows::create_child_video_surface(window, video_player)
}

/// Resize the child video surface
///
/// # Arguments
/// * `child_handle` - Platform-specific handle (NSView* or HWND)
/// * `width` - New width in pixels
/// * `height` - New height in pixels
/// * `window_height` - Total window height (needed for macOS coordinate system)
#[cfg(target_os = "macos")]
pub fn resize_child_video_surface(
    child_handle: usize,
    width: f64,
    height: f64,
    window_height: f64,
) {
    macos::resize_child_video_surface(child_handle, width, height, window_height);
}

#[cfg(target_os = "windows")]
pub fn resize_child_video_surface(
    child_handle: usize,
    width: f64,
    height: f64,
    _window_height: f64,
) {
    windows::resize_child_video_surface(child_handle, width, height);
}

/// Enable child window support by adding WS_CLIPCHILDREN to the parent window
///
/// This prevents GPUI's GPU rendering from painting over child windows.
/// Must be called after the window is created but before creating child windows.
///
/// # Arguments
/// * `window` - The GPUI window to modify
///
/// # Returns
/// Some(()) if successful, None if failed or unsupported on this platform
#[cfg(target_os = "windows")]
pub fn enable_child_window_support(window: &mut Window) -> Option<()> {
    windows::enable_child_window_support(window)
}

#[cfg(target_os = "macos")]
pub fn enable_child_window_support(_window: &mut Window) -> Option<()> {
    // macOS doesn't need this - NSViews handle clipping automatically
    Some(())
}

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "windows")]
mod windows;
