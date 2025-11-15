use gpui::{
    canvas, div, prelude::*, px, rgb, Bounds, Context, Corners, Entity, IntoElement, Render, RenderImage, Size, Window,
};
use std::sync::Arc;

use crate::controls_window::ControlsWindow;
use crate::platform;
use crate::subtitle_window::SubtitleWindow;

/// Unified window that contains video player area, controls, and subtitle window
/// The video area will have a child window/view created for mpv rendering
/// (NSView on macOS, HWND on Windows)
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

        // TODO: Set up continuous refresh for video playback (~60fps)
        // Currently commented out due to type inference issues with cx
        // We'll need to trigger redraws when MPV renders new frames
        // let _refresh_task = cx.spawn(|this, mut cx| async move {
        //     loop {
        //         cx.background_executor().timer(std::time::Duration::from_millis(16)).await;
        //         let _ = cx.update(|cx| {
        //             let _ = this.update(cx, |_, cx| {
        //                 cx.notify();
        //             });
        //         });
        //     }
        // });

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

    /// Get the current video area size for positioning the child video surface
    pub fn video_area_size(&self) -> Size<gpui::Pixels> {
        self.video_area_size
    }

    /// Resize the child window/view when the window bounds change
    fn resize_video_nsview(&self, window_bounds: Bounds<gpui::Pixels>, cx: &mut Context<Self>) {
        let app_state = cx.global::<crate::AppState>();

        // Get the child window/view handle if it exists
        if let Some(child_handle) = app_state.video_nsview {
            // Calculate new video area dimensions
            let video_width_px = window_bounds.size.width * 0.76;
            let video_height_px = window_bounds.size.height * 0.75;

            let width_str = format!("{}", video_width_px);
            let height_str = format!("{}", video_height_px);
            let window_height_str = format!("{}", window_bounds.size.height);

            let video_width: f64 = width_str.trim_end_matches("px").parse().unwrap_or(960.0);
            let video_height: f64 = height_str.trim_end_matches("px").parse().unwrap_or(540.0);
            let window_height: f64 = window_height_str
                .trim_end_matches("px")
                .parse()
                .unwrap_or(720.0);

            // Use platform-specific resize function
            platform::resize_child_video_surface(
                child_handle,
                video_width,
                video_height,
                window_height,
            );
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
                    // Video area - canvas for GPUI rendering
                    .child(
                        div()
                            .id("video-area")
                            .w(video_width)
                            .h(video_section_height)
                            .bg(rgb(0x000000))
                            .child(
                                canvas(
                                    move |_bounds, _window, cx| {
                                        // Get frame buffer from video player (prepaint phase)
                                        let app_state = cx.global::<crate::AppState>();
                                        let video_player = app_state.video_player.clone();

                                        // Prepare frame data
                                        if let Ok(player) = video_player.lock() {
                                            let frame_buffer_arc = player.get_frame_buffer();
                                            let (width, height) = player.get_video_dimensions();

                                            // Clone the Arc and release the player lock before locking frame_buffer
                                            drop(player);

                                            // Clone the buffer data to avoid lifetime issues
                                            let buffer_clone = if let Ok(buffer_data) = frame_buffer_arc.lock() {
                                                Some(buffer_data.clone())
                                            } else {
                                                None
                                            };

                                            if let Some(buffer) = buffer_clone {
                                                // Buffer is in BGRA format from OpenGL ReadPixels
                                                // No channel swap needed - pass directly to RgbaImage

                                                // Create image::Frame from buffer
                                                use image::{RgbaImage, Frame, Delay};

                                                if let Some(rgba_image) = RgbaImage::from_raw(width, height, buffer) {
                                                    let frame = Frame::from_parts(
                                                        rgba_image.into(),
                                                        0,
                                                        0,
                                                        Delay::from_numer_denom_ms(0, 1),
                                                    );
                                                    return Some(smallvec::smallvec![frame]);
                                                }
                                            }
                                        }
                                        None
                                    },
                                    move |bounds, frame_data, window, _cx| {
                                        // Paint the frame (paint phase)
                                        if let Some(frames) = frame_data {
                                            let image = RenderImage::new(frames);
                                            let _ = window.paint_image(
                                                bounds,
                                                Corners::default(),
                                                Arc::new(image),
                                                0,  // frame_index
                                                false,  // grayscale
                                            );
                                        }
                                    },
                                ).w_full().h_full()
                            )
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
