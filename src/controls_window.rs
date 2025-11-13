use gpui::{
    div, prelude::*, px, rgb, Context, Entity, IntoElement, MouseButton, Render, Window,
};

use crate::checkbox::{Checkbox, CheckboxEvent, CheckboxState};
use crate::slider::{Slider, SliderEvent, SliderState, SliderValue};
use crate::video_player::ClockTime;
use crate::AppState;

/// Controls window with play/pause/stop buttons and video scrubber
pub struct ControlsWindow {
    slider_state: Entity<SliderState>,
    display_subtitles: Entity<CheckboxState>,
    current_position: f32,
    duration: f32,
    is_playing: bool,
    clip_start: Option<f32>, // stored in milliseconds
    clip_end: Option<f32>,   // stored in milliseconds
    is_exporting: bool,
}

impl ControlsWindow {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let slider_state = cx.new(|_cx| {
            SliderState::new()
                .min(0.0)
                .max(36000.0) // Max 10 hours (will be updated once duration is known)
                .step(0.1)
                .default_value(0.0)
        });

        // Subscribe to slider events
        cx.subscribe(&slider_state, |_this, _, event: &SliderEvent, cx| {
            let SliderEvent::Change(value) = event;
            let position_secs = value.end();

            // Seek the video
            let app_state = cx.global::<AppState>();
            let video_player = app_state.video_player.clone();

            if let Ok(player) = video_player.lock() {
                let nanos = (position_secs * 1_000_000_000.0) as u64;
                let clock_time = ClockTime::from_nseconds(nanos);
                if let Err(e) = player.seek(clock_time) {
                    eprintln!("Failed to seek: {}", e);
                }
            };
        })
        .detach();

        // Create checkbox state for subtitle display (unchecked by default)
        let display_subtitles = cx.new(|_cx| CheckboxState::new(false));

        // Subscribe to checkbox events to control subtitle display
        cx.subscribe(
            &display_subtitles,
            |_this, _, event: &CheckboxEvent, cx| {
                let CheckboxEvent::Change(checked) = event;
                let app_state = cx.global::<AppState>();
                let video_player = app_state.video_player.clone();
                let selected_track = app_state.selected_subtitle_track.map(|t| t as i32);

                if let Ok(player) = video_player.lock() {
                    if let Err(e) = player.set_subtitle_display(*checked, selected_track) {
                        eprintln!("Failed to set subtitle display: {}", e);
                    }
                } else {
                    eprintln!("Failed to lock video player for subtitle display toggle");
                };
            },
        )
        .detach();

        Self {
            slider_state,
            display_subtitles,
            current_position: 0.0,
            duration: 0.0,
            is_playing: false,
            clip_start: None,
            clip_end: None,
            is_exporting: false,
        }
    }

    fn update_position_from_player(&mut self, cx: &mut Context<Self>) {
        let app_state = cx.global::<AppState>();
        let video_player = app_state.video_player.clone();

        if let Ok(player) = video_player.lock() {
            if let Some((position, duration)) = player.get_position_duration() {
                // Use nseconds() to get precise nanosecond timing, then convert to seconds
                self.current_position = position.nseconds() as f32 / 1_000_000_000.0;
                self.duration = duration.nseconds() as f32 / 1_000_000_000.0;
            }
            self.is_playing = player.is_playing();
        };
    }

    fn format_time(seconds: f32) -> String {
        let total_secs = seconds as u64;
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        format!("{:02}:{:02}", mins, secs)
    }

    fn format_time_ms(milliseconds: f32) -> String {
        let total_ms = milliseconds as u64;
        let total_secs = total_ms / 1000;
        let ms = total_ms % 1000;
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        format!("{:02}:{:02}.{:03}", mins, secs, ms)
    }

    fn handle_export_click(&mut self, cx: &mut Context<Self>) {
        // Get clip times (stored in milliseconds)
        let clip_start_ms = match self.clip_start {
            Some(start) => start,
            None => {
                eprintln!("Export error: clip start not set");
                return;
            }
        };

        let clip_end_ms = match self.clip_end {
            Some(end) => end,
            None => {
                eprintln!("Export error: clip end not set");
                return;
            }
        };

        // Convert milliseconds to seconds for ffmpeg
        let clip_start = clip_start_ms / 1000.0;
        let clip_end = clip_end_ms / 1000.0;

        // Get the input file path from AppState
        let app_state = cx.global::<AppState>();
        let input_path = match &app_state.file_path {
            Some(path) => path.clone(),
            None => {
                eprintln!("Export error: no input file loaded");
                return;
            }
        };

        // Generate default output filename and directory
        let input_path_buf = std::path::PathBuf::from(&input_path);
        let directory = input_path_buf
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));

        let default_filename = input_path_buf
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("video")
            .to_string()
            + "_clip.mp4";

        // Prompt for save location
        let path_receiver = cx.prompt_for_new_path(directory, Some(&default_filename));

        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(output_path))) = path_receiver.await {
                let output_path_str = output_path.to_string_lossy().to_string();

                // Set exporting state
                this.update(cx, |this, cx| {
                    this.is_exporting = true;
                    cx.notify();
                })
                .ok();

                // Run export on background thread
                let input_path_clone = input_path.clone();
                let output_path_str_clone = output_path_str.clone();
                let export_result = cx
                    .background_executor()
                    .spawn(async move {
                        crate::ffmpeg_export::export_clip(
                            &input_path_clone,
                            &output_path_str_clone,
                            clip_start,
                            clip_end,
                        )
                    })
                    .await;

                // Handle result and reset exporting state
                match export_result {
                    Ok(()) => {
                        println!("Export completed successfully: {}", output_path_str);
                    }
                    Err(e) => {
                        eprintln!("Export failed: {}", e);
                    }
                }

                this.update(cx, |this, cx| {
                    this.is_exporting = false;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }
}

impl Render for ControlsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        cx.on_next_frame(window, |t, _window, cx| {
            // Update from video player and request next frame
            t.update_position_from_player(cx);

            // Request another render on next frame to create continuous updates
            cx.notify();
        });

        // Update slider state to match current video position and duration
        if self.duration > 0.0 {
            // Update max if duration is known
            let current_max = self.slider_state.read(cx).get_max();
            if (current_max - self.duration).abs() > 0.1 {
                // Update the slider's max value to match the video duration
                self.slider_state.update(cx, |state, cx| {
                    state.set_max(self.duration, window, cx);
                });
            }
            // Update the position
            self.slider_state.update(cx, |state, cx| {
                state.set_value(SliderValue::Single(self.current_position), window, cx);
            });
        }

        let current_time = self.current_position;
        let duration = if self.duration > 0.0 {
            self.duration
        } else {
            100.0
        };

        div()
            .flex()
            .flex_col()
            .bg(rgb(0x1a1a1a))
            .size_full()
            .p_4()
            .gap_3()
            .on_mouse_move(window.listener_for(
                &self.slider_state,
                |state, _e: &gpui::MouseMoveEvent, window, cx| {
                    state.clear_hover(window, cx);
                },
            ))
            // Slider and time display section
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .w_full()
                    // Time display
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .w_full()
                            .text_sm()
                            .text_color(rgb(0xffffff))
                            .child(Self::format_time(current_time))
                            .child(Self::format_time(duration)),
                    )
                    // Slider
                    .child(Slider::new(&self.slider_state).horizontal()),
            )
            // Button controls section
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .w_full()
                    // Left side: Clip start/end buttons
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            // Buttons row
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .gap_2()
                                    // Clip start button with time display
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(div().text_xs().text_color(rgb(0xffffff)).child(
                                                if let Some(start) = self.clip_start {
                                                    format!(
                                                        "Start: {}",
                                                        Self::format_time_ms(start)
                                                    )
                                                } else {
                                                    "Start: --:--.---".to_string()
                                                },
                                            ))
                                            .child(
                                                div()
                                                    .px_4()
                                                    .py_2()
                                                    .bg(rgb(0x1976d2))
                                                    .rounded_md()
                                                    .cursor_pointer()
                                                    .text_sm()
                                                    .text_color(rgb(0xffffff))
                                                    .hover(|style| style.bg(rgb(0x2196f3)))
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        cx.listener(|this, _, _, cx| {
                                                            let current_time_ms =
                                                                this.current_position * 1000.0;

                                                            // Check if this would violate the constraint
                                                            if let Some(end) = this.clip_end {
                                                                if current_time_ms >= end {
                                                                    // Unset clip_end if start would be after it
                                                                    this.clip_end = None;
                                                                }
                                                            }

                                                            this.clip_start = Some(current_time_ms);
                                                            cx.notify();
                                                        }),
                                                    )
                                                    .child("Set Clip Start"),
                                            ),
                                    )
                                    // Clip end button with time display
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(div().text_xs().text_color(rgb(0xffffff)).child(
                                                if let Some(end) = self.clip_end {
                                                    format!("End: {}", Self::format_time_ms(end))
                                                } else {
                                                    "End: --:--.---".to_string()
                                                },
                                            ))
                                            .child(
                                                div()
                                                    .px_4()
                                                    .py_2()
                                                    .bg(rgb(0xc62828))
                                                    .rounded_md()
                                                    .cursor_pointer()
                                                    .text_sm()
                                                    .text_color(rgb(0xffffff))
                                                    .hover(|style| style.bg(rgb(0xe53935)))
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        cx.listener(|this, _, _, cx| {
                                                            let current_time_ms =
                                                                this.current_position * 1000.0;

                                                            // Check if this would violate the constraint
                                                            if let Some(start) = this.clip_start {
                                                                if current_time_ms <= start {
                                                                    // Unset clip_start if end would be before it
                                                                    this.clip_start = None;
                                                                }
                                                            }

                                                            this.clip_end = Some(current_time_ms);
                                                            cx.notify();
                                                        }),
                                                    )
                                                    .child("Set Clip End"),
                                            ),
                                    ),
                            )
                            // Display total clip length and export button if both times are set
                            .when_some(
                                self.clip_start
                                    .and_then(|start| self.clip_end.map(|end| end - start)),
                                |this, duration| {
                                    this.child(
                                        div()
                                            .flex()
                                            .flex_row()
                                            .gap_2()
                                            .items_center()
                                            .child(div().text_xs().text_color(rgb(0xffffff)).child(
                                                format!(
                                                    "Duration: {}",
                                                    Self::format_time_ms(duration)
                                                ),
                                            ))
                                            .child(
                                                div()
                                                    .px_3()
                                                    .py_1()
                                                    .bg(rgb(0xf57c00))
                                                    .rounded_md()
                                                    .cursor_pointer()
                                                    .text_xs()
                                                    .text_color(rgb(0xffffff))
                                                    .hover(|style| style.bg(rgb(0xfb8c00)))
                                                    .when(self.is_exporting, |this| {
                                                        this.bg(rgb(0x9e9e9e)).cursor_not_allowed()
                                                    })
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        cx.listener(|this, _, _, cx| {
                                                            if !this.is_exporting {
                                                                this.handle_export_click(cx);
                                                            }
                                                        }),
                                                    )
                                                    .child(if self.is_exporting {
                                                        "Exporting..."
                                                    } else {
                                                        "Export"
                                                    }),
                                            ),
                                    )
                                },
                            ),
                    )
                    // Center: Play/pause button
                    .child(
                        div()
                            .px_6()
                            .py_3()
                            .bg(rgb(0x404040))
                            .rounded_md()
                            .cursor_pointer()
                            .text_color(rgb(0xffffff))
                            .hover(|style| style.bg(rgb(0x505050)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    let app_state = cx.global::<AppState>();
                                    let video_player = app_state.video_player.clone();
                                    if let Ok(player) = video_player.lock() {
                                        if this.is_playing {
                                            if let Err(e) = player.pause() {
                                                eprintln!("Failed to pause: {}", e);
                                            }
                                        } else {
                                            if let Err(e) = player.play() {
                                                eprintln!("Failed to play: {}", e);
                                            }
                                        }
                                    };
                                }),
                            )
                            .child(if self.is_playing { "Pause" } else { "Play" }),
                    )
                    // Right side: Display subtitles checkbox
                    .child(
                        div()
                            .w(px(150.0))
                            .child(
                                Checkbox::new(&self.display_subtitles)
                                    .label("Display subtitles"),
                            ),
                    ),
            )
    }
}
