use gpui::{
    div, prelude::*, px, rgb, Bounds, Context, Entity, IntoElement, Render, Size, Window,
};

use crate::controls_window::ControlsWindow;
use crate::subtitle_window::SubtitleWindow;

/// Unified window that contains video player area, controls, and subtitle window
/// The video area will have a child NSView created for mpv rendering
pub struct UnifiedWindow {
    pub controls: Entity<ControlsWindow>,
    pub subtitles: Entity<SubtitleWindow>,
    video_area_size: Size<gpui::Pixels>,
    last_bounds: Option<Bounds<gpui::Pixels>>,
}

impl UnifiedWindow {
    pub fn new(cx: &mut Context<Self>) -> Self {
        // Create the controls and subtitle views
        let controls = cx.new(|cx| ControlsWindow::new(cx));
        let subtitles = cx.new(|cx| SubtitleWindow::new(cx));

        Self {
            controls,
            subtitles,
            video_area_size: Size {
                width: px(960.0),
                height: px(540.0),
            },
            last_bounds: None,
        }
    }

    /// Get the current video area size for NSView positioning
    pub fn video_area_size(&self) -> Size<gpui::Pixels> {
        self.video_area_size
    }

    /// Resize the child NSView when the window bounds change
    fn resize_video_nsview(&self, window_bounds: Bounds<gpui::Pixels>, cx: &mut Context<Self>) {
        let app_state = cx.global::<crate::AppState>();

        // Get the child NSView pointer if it exists
        if let Some(child_view_ptr) = app_state.video_nsview {
            // Calculate new video area dimensions
            let video_width_px = window_bounds.size.width * 0.76;
            let video_height_px = window_bounds.size.height * 0.75;

            let width_str = format!("{}", video_width_px);
            let height_str = format!("{}", video_height_px);
            let window_height_str = format!("{}", window_bounds.size.height);

            let video_width: f64 = width_str.trim_end_matches("px").parse().unwrap_or(960.0);
            let video_height: f64 = height_str.trim_end_matches("px").parse().unwrap_or(540.0);
            let window_height: f64 = window_height_str.trim_end_matches("px").parse().unwrap_or(720.0);

            // Calculate Y position (bottom-left origin)
            let video_y = window_height * 0.25;

            // Update the child NSView frame using Cocoa
            unsafe {
                use objc::runtime::Object;

                let child_view = child_view_ptr as *mut Object;
                let new_frame = cocoa::foundation::NSRect::new(
                    cocoa::foundation::NSPoint::new(0.0, video_y),
                    cocoa::foundation::NSSize::new(video_width, video_height),
                );

                let _: () = msg_send![child_view, setFrame:new_frame];

                println!("Resized child NSView to {}x{} at y={}", video_width as i32, video_height as i32, video_y as i32);
            }
        }
    }
}

impl Render for UnifiedWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Get the window bounds to calculate proportions
        let window_bounds = window.bounds();
        let total_width = window_bounds.size.width;
        let total_height = window_bounds.size.height;

        // Check if bounds have changed and update child NSView if needed
        let bounds_changed = self.last_bounds
            .map(|last| {
                let width_changed = format!("{}", last.size.width) != format!("{}", total_width);
                let height_changed = format!("{}", last.size.height) != format!("{}", total_height);
                width_changed || height_changed
            })
            .unwrap_or(false);

        if bounds_changed {
            // Resize the child NSView to match new window size
            self.resize_video_nsview(window_bounds, cx);
        }

        // Update last known bounds
        self.last_bounds = Some(window_bounds);

        // Calculate layout dimensions based on proportions
        // Video section takes 75% of height, controls take 25%
        let video_section_height = total_height * 0.75;
        let controls_height = total_height * 0.25;

        // Video takes 76% of width, subtitles take 24%
        let video_width = total_width * 0.76;
        let subtitle_width = total_width * 0.24;

        // Update stored video area size for NSView positioning
        self.video_area_size = Size {
            width: video_width,
            height: video_section_height,
        };

        div()
            .flex()
            .flex_col()
            .bg(rgb(0x000000))
            .size_full()
            // Close subtitle context menu on any click outside the subtitle window
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| {
                // Check if subtitle window has an open context menu and close it
                this.subtitles.update(cx, |subtitles, cx| {
                    if subtitles.context_menu.is_some() {
                        println!("Closing context menu from unified window click");
                        subtitles.context_menu = None;
                        cx.notify();
                    }
                });
            }))
            .on_mouse_down(gpui::MouseButton::Right, cx.listener(|this, _, _, cx| {
                // Close context menu on right-click anywhere
                this.subtitles.update(cx, |subtitles, cx| {
                    if subtitles.context_menu.is_some() {
                        println!("Closing context menu from unified window right-click");
                        subtitles.context_menu = None;
                        cx.notify();
                    }
                });
            }))
            // Top section: video (left) and subtitles (right)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .w(total_width)
                    .h(video_section_height)
                    // Video area (black background, child NSView will be created here)
                    .child(
                        div()
                            .id("video-area")
                            .flex()
                            .bg(rgb(0x000000))
                            .w(video_width)
                            .h(video_section_height),
                    )
                    // Subtitle window area
                    .child(
                        div()
                            .id("subtitle-area")
                            .flex()
                            .w(subtitle_width)
                            .h(video_section_height)
                            .child(self.subtitles.clone()),
                    ),
            )
            // Bottom section: controls
            .child(
                div()
                    .id("controls-area")
                    .flex()
                    .w(total_width)
                    .h(controls_height)
                    .child(self.controls.clone()),
            )
    }
}
