use gpui::{
    canvas, div, prelude::*, px, rgb, Bounds, Context, Corners, Entity, IntoElement, Render,
    RenderImage, Size, Window,
};
use serde::Deserialize;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::controls_window::ControlsWindow;
use crate::custom_titlebar::CustomTitlebar;
use crate::platform;
use crate::subtitle_window::SubtitleWindow;

#[derive(Deserialize)]
struct TriangleFrames {
    frames: Vec<String>,
}

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
    animation_start_time: Instant,
    triangle_frames: Vec<String>,
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

        // Load triangle frames from JSON file
        let triangle_frames = Self::load_triangle_frames();

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
            animation_start_time: Instant::now(),
            triangle_frames,
        }
    }

    /// Load triangle frames from JSON file
    fn load_triangle_frames() -> Vec<String> {
        let json_path = "assets/triangle_frames.json";

        match std::fs::read_to_string(json_path) {
            Ok(contents) => match serde_json::from_str::<TriangleFrames>(&contents) {
                Ok(data) => data.frames,
                Err(e) => {
                    eprintln!("Failed to parse triangle frames JSON: {}", e);
                    Self::default_triangle_frames()
                }
            },
            Err(e) => {
                eprintln!("Failed to read triangle frames file: {}", e);
                Self::default_triangle_frames()
            }
        }
    }

    /// Fallback to default triangle frames if JSON loading fails
    fn default_triangle_frames() -> Vec<String> {
        vec![
            "       /\\       \n      /  \\      \n     /    \\     \n    /      \\    \n   /        \\   \n  /          \\  \n /            \\ \n/______________\\".to_string(),
        ]
    }

    /// Generate a rotating ASCII triangle
    fn generate_rotating_triangle(&self) -> String {
        if self.triangle_frames.is_empty() {
            return String::new();
        }

        let elapsed = self.animation_start_time.elapsed().as_secs_f32();

        // Rotate through frames based on the number of frames loaded
        let frame_count = self.triangle_frames.len();
        let frame = (elapsed * 2.0) as usize % frame_count;

        self.triangle_frames[frame].clone()
    }

    /// Get a color that slowly changes over time
    fn get_animated_color(&self) -> gpui::Rgba {
        let elapsed = self.animation_start_time.elapsed().as_secs_f32();

        // Cycle through colors over 10 seconds
        let hue = (elapsed * 36.0) % 360.0;

        // Convert HSV to RGB (simple conversion)
        let h = hue / 60.0;
        let i = h.floor();
        let f = h - i;
        let q = 1.0 - f;

        let (r, g, b) = match i as i32 % 6 {
            0 => (1.0, f, 0.0),
            1 => (q, 1.0, 0.0),
            2 => (0.0, 1.0, f),
            3 => (0.0, q, 1.0),
            4 => (f, 0.0, 1.0),
            _ => (1.0, 0.0, q),
        };

        gpui::Rgba { r, g, b, a: 1.0 }
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
        // Check if a video is loaded
        let app_state = cx.global::<crate::AppState>();
        let has_video_loaded = app_state.has_video_loaded;

        // Request continuous animation when no video is loaded
        if !has_video_loaded {
            cx.on_next_frame(window, |this, _window, cx| {
                // Request another frame for continuous portal animation
                cx.notify();
            });
        }

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
                    // Video area - either show portal or video canvas
                    .child(
                        div()
                            .id("video-area")
                            .w(video_width)
                            .h(video_section_height)
                            .bg(rgb(0x000000))
                            .when(!has_video_loaded, |el| {
                                // Show rotating triangle when no video is loaded
                                let triangle = self.generate_rotating_triangle();
                                let color = self.get_animated_color();

                                el.flex().items_center().justify_center().child(
                                    div()
                                        .text_color(color)
                                        .font_family("courier")
                                        .text_size(px(24.0))
                                        .line_height(px(20.0))
                                        .child(triangle),
                                )
                            })
                            .when(has_video_loaded, |el| {
                                // Show video canvas when video is loaded
                                let last_image = self.last_video_render_image.clone();

                                el.child(
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
                                    .h_full(),
                                )
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
