use gpui::{div, prelude::*, px, rgb, Context, Entity, IntoElement, MouseButton, Render, Window};

use crate::checkbox::{Checkbox, CheckboxEvent, CheckboxState};
use crate::slider::{Slider, SliderEvent, SliderState, SliderValue};
use crate::time_input::TimeInput;
use crate::video_player::ClockTime;
use crate::AppState;

/// Controls window with play/pause/stop buttons and video scrubber
pub struct ControlsWindow {
    slider_state: Entity<SliderState>,
    display_subtitles: Entity<CheckboxState>,
    clip_start_input: Entity<TimeInput>,
    clip_end_input: Entity<TimeInput>,
    current_position: f32,
    duration: f32,
    is_playing: bool,
    clip_start: Option<f32>, // stored in milliseconds
    clip_end: Option<f32>,   // stored in milliseconds
    is_exporting: bool,
    is_playing_clip: bool,
    clip_playback_end: Option<f32>, // milliseconds - when to stop during clip playback
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
        cx.subscribe(&display_subtitles, |_this, _, event: &CheckboxEvent, cx| {
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
        })
        .detach();

        // Create time input fields for clip start and end
        let clip_start_input = cx.new(|cx| TimeInput::new(cx));
        let clip_end_input = cx.new(|cx| TimeInput::new(cx));

        Self {
            slider_state,
            display_subtitles,
            clip_start_input,
            clip_end_input,
            current_position: 0.0,
            duration: 0.0,
            is_playing: false,
            clip_start: None,
            clip_end: None,
            is_exporting: false,
            is_playing_clip: false,
            clip_playback_end: None,
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

    /// Set clip start and end times from milliseconds (e.g., from subtitle blocks)
    pub fn set_clip_times(&mut self, start_ms: u64, end_ms: u64, cx: &mut Context<Self>) {
        let start_ms_f32 = start_ms as f32;
        let end_ms_f32 = end_ms as f32;

        // Set the clip start and end
        self.clip_start = Some(start_ms_f32);
        self.clip_end = Some(end_ms_f32);

        // Update the input fields
        let start_formatted = Self::format_time_ms(start_ms_f32);
        let end_formatted = Self::format_time_ms(end_ms_f32);

        self.clip_start_input.update(cx, |input, cx| {
            input.set_content(start_formatted, cx);
        });

        self.clip_end_input.update(cx, |input, cx| {
            input.set_content(end_formatted, cx);
        });

        cx.notify();
    }

    fn handle_export_click(&mut self, cx: &mut Context<Self>) {
        // Try to get times from input fields first, fall back to stored values
        let clip_start_ms = self
            .clip_start_input
            .read(cx)
            .parse_time_ms()
            .or(self.clip_start)
            .unwrap_or_else(|| {
                eprintln!("Export error: clip start not set");
                0.0
            });

        let clip_end_ms = self
            .clip_end_input
            .read(cx)
            .parse_time_ms()
            .or(self.clip_end)
            .unwrap_or_else(|| {
                eprintln!("Export error: clip end not set");
                0.0
            });

        if clip_start_ms >= clip_end_ms {
            eprintln!("Export error: clip start must be before clip end");
            return;
        }

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

            // Check if we need to pause during clip playback
            if t.is_playing_clip {
                if let Some(end_time_ms) = t.clip_playback_end {
                    let current_time_ms = t.current_position * 1000.0;
                    if current_time_ms >= end_time_ms {
                        // Stop clip playback
                        t.is_playing_clip = false;
                        t.clip_playback_end = None;

                        // Pause the player
                        let app_state = cx.global::<AppState>();
                        let video_player = app_state.video_player.clone();
                        if let Ok(player) = video_player.lock() {
                            if let Err(e) = player.pause() {
                                eprintln!("Failed to pause after clip playback: {}", e);
                            }
                        };
                    }
                }
            }

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
                    // Left side: Clip start/end buttons and inputs
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            // Buttons and inputs row
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .gap_2()
                                    .items_end()
                                    // Clip start section
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .w(px(100.0))
                                            .child(self.clip_start_input.clone())
                                            .child(
                                                div()
                                                    .px_2()
                                                    .py_1()
                                                    .bg(rgb(0x1976d2))
                                                    .rounded_md()
                                                    .cursor_pointer()
                                                    .text_xs()
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
                                                                    this.clip_end_input.update(
                                                                        cx,
                                                                        |input, cx| {
                                                                            input.set_content(
                                                                                "".to_string(),
                                                                                cx,
                                                                            );
                                                                        },
                                                                    );
                                                                }
                                                            }

                                                            this.clip_start = Some(current_time_ms);

                                                            // Update the input field
                                                            let formatted = Self::format_time_ms(
                                                                current_time_ms,
                                                            );
                                                            this.clip_start_input.update(
                                                                cx,
                                                                |input, cx| {
                                                                    input
                                                                        .set_content(formatted, cx);
                                                                },
                                                            );

                                                            cx.notify();
                                                        }),
                                                    )
                                                    .child("Set Start"),
                                            ),
                                    )
                                    // Clip end section
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .w(px(100.0))
                                            .child(self.clip_end_input.clone())
                                            .child(
                                                div()
                                                    .px_2()
                                                    .py_1()
                                                    .bg(rgb(0xc62828))
                                                    .rounded_md()
                                                    .cursor_pointer()
                                                    .text_xs()
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
                                                                    this.clip_start_input.update(
                                                                        cx,
                                                                        |input, cx| {
                                                                            input.set_content(
                                                                                "".to_string(),
                                                                                cx,
                                                                            );
                                                                        },
                                                                    );
                                                                }
                                                            }

                                                            this.clip_end = Some(current_time_ms);

                                                            // Update the input field
                                                            let formatted = Self::format_time_ms(
                                                                current_time_ms,
                                                            );
                                                            this.clip_end_input.update(
                                                                cx,
                                                                |input, cx| {
                                                                    input
                                                                        .set_content(formatted, cx);
                                                                },
                                                            );

                                                            cx.notify();
                                                        }),
                                                    )
                                                    .child("Set End"),
                                            ),
                                    )
                                    .child({
                                        let start_ms = self
                                            .clip_start_input
                                            .read(cx)
                                            .parse_time_ms()
                                            .or(self.clip_start);
                                        let end_ms = self
                                            .clip_end_input
                                            .read(cx)
                                            .parse_time_ms()
                                            .or(self.clip_end);
                                        let duration = start_ms.and_then(|start| {
                                            end_ms.map(
                                                |end| if end > start { end - start } else { 0.0 },
                                            )
                                        });
                                        let is_valid =
                                            duration.is_some() && duration.unwrap() > 0.0;

                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(if is_valid {
                                                        rgb(0xffffff)
                                                    } else {
                                                        rgb(0x666666)
                                                    })
                                                    .child(format!(
                                                        "Duration: {}",
                                                        Self::format_time_ms(
                                                            duration.unwrap_or(0.0)
                                                        )
                                                    )),
                                            )
                                            .child(
                                                div()
                                                    .px_3()
                                                    .py_1()
                                                    .rounded_md()
                                                    .text_xs()
                                                    .when(is_valid && !self.is_exporting, |this| {
                                                        this.bg(rgb(0xf57c00))
                                                            .cursor_pointer()
                                                            .text_color(rgb(0xffffff))
                                                            .hover(|style| style.bg(rgb(0xfb8c00)))
                                                    })
                                                    .when(!is_valid || self.is_exporting, |this| {
                                                        this.bg(rgb(0x404040))
                                                            .cursor_not_allowed()
                                                            .text_color(rgb(0x666666))
                                                    })
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        cx.listener(|this, _, _, cx| {
                                                            let start_ms = this
                                                                .clip_start_input
                                                                .read(cx)
                                                                .parse_time_ms()
                                                                .or(this.clip_start);
                                                            let end_ms = this
                                                                .clip_end_input
                                                                .read(cx)
                                                                .parse_time_ms()
                                                                .or(this.clip_end);
                                                            let duration =
                                                                start_ms.and_then(|start| {
                                                                    end_ms.map(|end| {
                                                                        if end > start {
                                                                            end - start
                                                                        } else {
                                                                            0.0
                                                                        }
                                                                    })
                                                                });
                                                            let is_valid = duration.is_some()
                                                                && duration.unwrap() > 0.0;

                                                            if !this.is_exporting && is_valid {
                                                                this.handle_export_click(cx);
                                                            }
                                                        }),
                                                    )
                                                    .child(if self.is_exporting {
                                                        "Exporting..."
                                                    } else {
                                                        "Export"
                                                    }),
                                            )
                                    }),
                            ), // Display total clip length and export button (always visible, greyed out if invalid)
                    )
                    // Center: Play/pause and Play Clip buttons
                    .child(
                        div()
                            .flex()
                            .gap_2()
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
                            .child({
                                let start_ms = self
                                    .clip_start_input
                                    .read(cx)
                                    .parse_time_ms()
                                    .or(self.clip_start);
                                let end_ms = self
                                    .clip_end_input
                                    .read(cx)
                                    .parse_time_ms()
                                    .or(self.clip_end);
                                let is_valid = start_ms.is_some()
                                    && end_ms.is_some()
                                    && start_ms.unwrap() < end_ms.unwrap();

                                div()
                                    .px_6()
                                    .py_3()
                                    .rounded_md()
                                    .when(is_valid, |this| {
                                        this.bg(rgb(0x2e7d32))
                                            .cursor_pointer()
                                            .text_color(rgb(0xffffff))
                                            .hover(|style| style.bg(rgb(0x388e3c)))
                                    })
                                    .when(!is_valid, |this| {
                                        this.bg(rgb(0x404040))
                                            .cursor_not_allowed()
                                            .text_color(rgb(0x666666))
                                    })
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|this, _, _, cx| {
                                            let start_ms = this
                                                .clip_start_input
                                                .read(cx)
                                                .parse_time_ms()
                                                .or(this.clip_start);
                                            let end_ms = this
                                                .clip_end_input
                                                .read(cx)
                                                .parse_time_ms()
                                                .or(this.clip_end);

                                            if let (Some(start), Some(end)) = (start_ms, end_ms) {
                                                if start < end {
                                                    // Seek to start and play
                                                    let app_state = cx.global::<AppState>();
                                                    let video_player = app_state.video_player.clone();

                                                    if let Ok(player) = video_player.lock() {
                                                        // Convert milliseconds to nanoseconds for seeking
                                                        let nanos = (start * 1_000_000.0) as u64;
                                                        let clock_time = ClockTime::from_nseconds(nanos);

                                                        if let Err(e) = player.seek(clock_time) {
                                                            eprintln!("Failed to seek to clip start: {}", e);
                                                        } else if let Err(e) = player.play() {
                                                            eprintln!("Failed to play clip: {}", e);
                                                        } else {
                                                            // Set up clip playback mode
                                                            this.is_playing_clip = true;
                                                            this.clip_playback_end = Some(end);
                                                        }
                                                    };
                                                }
                                            }
                                        }),
                                    )
                                    .child("Play Clip")
                            }),
                    )
                    // Right side: Display subtitles checkbox
                    .child(
                        div().w(px(150.0)).child(
                            Checkbox::new(&self.display_subtitles).label("Display subtitles"),
                        ),
                    ),
            )
    }
}
