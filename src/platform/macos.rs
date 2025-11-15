//! macOS-specific platform implementation using NSWindow for OpenGL rendering

use cocoa::foundation::{NSPoint, NSRect, NSSize};
use gpui::{Pixels, Window};
use objc::runtime::Object;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::sync::{Arc, Mutex};

use crate::video_player::VideoPlayer;

/// Create a hidden NSWindow for OpenGL context and off-screen video rendering
///
/// This function:
/// 1. Creates a hidden NSWindow with appropriate dimensions
/// 2. Gets the window's content view for OpenGL context attachment
/// 3. Configures the video player with the content view handle
/// 4. Window is never shown - used only for OpenGL context and FBO rendering
///
/// # Returns
/// The NSView pointer (content view of hidden window) as a usize, or None if creation fails
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
    let video_width: f64 = width_str.trim_end_matches("px").parse().unwrap_or(960.0);
    let video_height: f64 = height_str.trim_end_matches("px").parse().unwrap_or(540.0);

    println!(
        "Creating hidden window with size: {}x{} for OpenGL context",
        video_width as i32, video_height as i32
    );

    // Create a hidden NSWindow for OpenGL context using Objective-C
    unsafe {
        // Get the NSWindow class
        let ns_window_class = objc::runtime::Class::get("NSWindow").unwrap();

        // Create a content rect for the hidden window
        // Position doesn't matter since window is hidden
        let content_rect = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(video_width, video_height),
        );

        // NSWindowStyleMask values
        let NSWindowStyleMaskBorderless: usize = 0; // Borderless window (minimal overhead)

        // Allocate and initialize the hidden NSWindow
        let hidden_window: *mut Object = msg_send![ns_window_class, alloc];
        let hidden_window: *mut Object = msg_send![hidden_window,
            initWithContentRect:content_rect
            styleMask:NSWindowStyleMaskBorderless
            backing:2  // NSBackingStoreBuffered
            defer:0    // NO
        ];

        if !hidden_window.is_null() {
            // Keep the window hidden (orderOut: removes from screen)
            let _: () = msg_send![hidden_window, orderOut: hidden_window];

            // Get the content view from the hidden window
            let content_view: *mut Object = msg_send![hidden_window, contentView];

            if !content_view.is_null() {
                // Set wantsLayer to YES for OpenGL support
                let _: () = msg_send![content_view, setWantsLayer: true];

                let content_view_ptr = content_view as usize;
                println!("Hidden NSWindow created with content view at: 0x{:x}", content_view_ptr);

                // Pass the content view pointer to the video player
                // (OpenGL context will attach to this view)
                if let Ok(mut player) = video_player.lock() {
                    player.set_window_handle(content_view_ptr);
                    println!("Hidden window content view set on video player");
                } else {
                    eprintln!("Failed to lock video player mutex");
                }

                Some(content_view_ptr)
            } else {
                eprintln!("Failed to get content view from hidden window");
                None
            }
        } else {
            eprintln!("Failed to create hidden NSWindow");
            None
        }
    }
}

/// Resize the hidden window's content view to match new video dimensions
///
/// Note: The hidden window is resized to ensure the OpenGL context has the correct dimensions
/// for FBO creation, but the window itself is never displayed.
///
/// # Arguments
/// * `content_view_handle` - The NSView pointer (content view) as a usize
/// * `width` - New width in pixels
/// * `height` - New height in pixels
/// * `_window_height` - Not used for hidden windows (kept for compatibility)
pub fn resize_child_video_surface(
    content_view_handle: usize,
    width: f64,
    height: f64,
    _window_height: f64,
) {
    unsafe {
        let content_view = content_view_handle as *mut Object;
        if !content_view.is_null() {
            // Resize the content view (position doesn't matter for hidden window)
            let new_frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, height));

            let _: () = msg_send![content_view, setFrame: new_frame];
            println!(
                "Resized hidden window content view to {}x{}",
                width as i32, height as i32
            );
        }
    }
}
