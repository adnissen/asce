use gpui::{
    canvas, div, prelude::*, px, rgb, Bounds, Context, Corners, Entity, IntoElement, Render,
    RenderImage, Size, Window,
};
use std::sync::{Arc, Mutex};

use crate::controls_window::ControlsWindow;
use crate::custom_titlebar::CustomTitlebar;
use crate::platform;
use crate::subtitle_window::SubtitleWindow;

/// Unified window that contains video player area, controls, and subtitle window
/// The video area will have a child window/view created for mpv rendering
/// (NSView on macOS, HWND on Windows)
pub struct UnifiedWindow {
    pub titlebar: Entity<CustomTitlebar>,
    pub controls: Entity<ControlsWindow>,
    pub subtitles: Entity<SubtitleWindow>,
    video_area_size: Size<gpui::Pixels>,
    last_bounds: Option<Bounds<gpui::Pixels>>,
    last_video_render_image: Arc<Mutex<Option<Arc<RenderImage>>>>,
}

impl UnifiedWindow {
    pub fn new(cx: &mut Context<Self>) -> Self {
        // Get the file name from AppState for the titlebar
        let app_state = cx.global::<crate::AppState>();
        let file_name = app_state
            .file_path
            .as_ref()
            .and_then(|path| {
                std::path::Path::new(path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| "asve".to_string());

        // Create the titlebar, controls, and subtitle views
        let titlebar = cx.new(|_| CustomTitlebar::new(file_name));
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
            titlebar,
            controls,
            subtitles,
            video_area_size: Size {
                width: px(960.0),
                height: px(540.0),
            },
            last_bounds: None,
            last_video_render_image: Arc::new(Mutex::new(None)),
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
        let bounds_changed = self
            .last_bounds
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
        // Titlebar takes 37px, remaining height is split: 75% video, 25% controls
        let titlebar_height = px(37.0);
        let available_height = total_height - titlebar_height;
        let video_section_height = available_height * 0.75;
        let controls_height = available_height * 0.25;

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
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    // Check if subtitle window has an open context menu and close it
                    this.subtitles.update(cx, |subtitles, cx| {
                        if subtitles.context_menu.is_some() {
                            println!("Closing context menu from unified window click");
                            subtitles.context_menu = None;
                            cx.notify();
                        }
                    });
                }),
            )
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(|this, _, _, cx| {
                    // Close context menu on right-click anywhere
                    this.subtitles.update(cx, |subtitles, cx| {
                        if subtitles.context_menu.is_some() {
                            println!("Closing context menu from unified window right-click");
                            subtitles.context_menu = None;
                            cx.notify();
                        }
                    });
                }),
            )
            // Custom titlebar
            .child(self.titlebar.clone())
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
                            .child({
                                // Clone the Arc<Mutex<>> to share with paint closure
                                let last_image = self.last_video_render_image.clone();

                                canvas(
                                    move |_bounds, _window, cx| {
                                        // Get frame buffer from video player (prepaint phase)
                                        let app_state = cx.global::<crate::AppState>();
                                        let video_player = app_state.video_player.clone();

                                        // Prepare frame data
                                        if let Ok(player) = video_player.lock() {
                                            // Get Arc<Vec<u8>> - cheap Arc clone, no Vec clone!
                                            let frame_buffer_arc = player.get_frame_buffer();
                                            let (width, height) = player.get_video_dimensions();

                                            // Release the player lock
                                            drop(player);

                                            // Use Arc::try_unwrap or clone the data for RgbaImage
                                            // RgbaImage::from_raw takes ownership of Vec, so we need to extract it
                                            let buffer = Arc::try_unwrap(frame_buffer_arc)
                                                .unwrap_or_else(|arc| (*arc).clone());

                                            // Buffer is in BGRA format from OpenGL ReadPixels
                                            // No channel swap needed - pass directly to RgbaImage

                                            // Create image::Frame from buffer
                                            use image::{Delay, Frame, RgbaImage};

                                            if let Some(rgba_image) =
                                                RgbaImage::from_raw(width, height, buffer)
                                            {
                                                let frame = Frame::from_parts(
                                                    rgba_image.into(),
                                                    0,
                                                    0,
                                                    Delay::from_numer_denom_ms(0, 1),
                                                );
                                                return Some(smallvec::smallvec![frame]);
                                            }
                                        }
                                        None
                                    },
                                    move |bounds, frame_data, window, _cx| {
                                        // Paint the frame (paint phase)
                                        if let Some(frames) = frame_data {
                                            let new_image = Arc::new(RenderImage::new(frames));

                                            // Drop the previous frame from sprite atlas before painting new one
                                            if let Ok(mut last) = last_image.lock() {
                                                if let Some(old_image) = last.take() {
                                                    let _ = window.drop_image(old_image);
                                                }

                                                // Paint the new frame
                                                let _ = window.paint_image(
                                                    bounds,
                                                    Corners::default(),
                                                    new_image.clone(),
                                                    0,     // frame_index
                                                    false, // grayscale
                                                );

                                                // Store the new image for next frame
                                                *last = Some(new_image);
                                            }
                                        }
                                    },
                                )
                                .w_full()
                                .h_full()
                            }),
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
