use crate::theme::OneDarkExt;
use gpui::{div, prelude::*, px, Context, Entity, IntoElement, MouseButton, Render, Window};
use gpui_component::ActiveTheme;
use std::time::Instant;

use crate::font_utils;
use crate::time_input::TimeInput;
use crate::video_player::ClockTime;
use crate::AppState;
use gpui_component::{
    checkbox::Checkbox,
    select::{Select, SelectEvent, SelectItem, SelectState},
    slider::{Slider, SliderEvent, SliderState, SliderValue},
    IndexPath,
};

// Wrapper for font names to implement SelectItem
#[derive(Debug, Clone, PartialEq, Eq)]
struct FontName(String);

impl SelectItem for FontName {
    type Value = Self;

    fn title(&self) -> gpui::SharedString {
        self.0.clone().into()
    }

    fn value(&self) -> &Self::Value {
        self
    }
}

impl From<String> for FontName {
    fn from(s: String) -> Self {
        FontName(s)
    }
}

impl From<FontName> for String {
    fn from(f: FontName) -> Self {
        f.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Video,
    Gif,
    Audio,
}

impl ExportFormat {
    fn next(&self) -> Self {
        match self {
            ExportFormat::Video => ExportFormat::Gif,
            ExportFormat::Gif => ExportFormat::Audio,
            ExportFormat::Audio => ExportFormat::Video,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            ExportFormat::Video => "video",
            ExportFormat::Gif => "gif",
            ExportFormat::Audio => "audio",
        }
    }

    fn file_extension(&self) -> &'static str {
        match self {
            ExportFormat::Video => "_clip.mp4",
            ExportFormat::Gif => "_clip.gif",
            ExportFormat::Audio => "_clip.mp3",
        }
    }
}

/// Controls window with play/pause/stop buttons and video scrubber
pub struct ControlsWindow {
    slider_state: Option<Entity<SliderState>>,
    display_subtitles_enabled: bool,
    pub clip_start_input: Entity<TimeInput>,
    pub clip_end_input: Entity<TimeInput>,
    // Subtitle styling controls
    subtitle_font_select: Entity<SelectState<Vec<FontName>>>,
    subtitle_font_size_slider: Entity<SliderState>,
    subtitle_bold_enabled: bool,
    subtitle_italic_enabled: bool,
    export_format: ExportFormat,
    current_position: f32,
    duration: f32,
    is_playing: bool,
    pub clip_start: Option<f32>, // stored in milliseconds
    pub clip_end: Option<f32>,   // stored in milliseconds
    is_exporting: bool,
    is_playing_clip: bool,
    clip_playback_end: Option<f32>, // milliseconds - when to stop during clip playback
    last_seek_time: Option<f32>,    // milliseconds - video time when user clicked "Play Clip"
    last_render_time: Instant,      // For rate limiting renders to 30 FPS
}

impl ControlsWindow {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Slider will be created once we know the video duration

        // Subtitle display will start as disabled (false)

        // Create time input fields for clip start and end
        let clip_start_input = cx.new(|cx| TimeInput::new(cx));
        let clip_end_input = cx.new(|cx| TimeInput::new(cx));

        // Get system fonts for the font selector
        let system_fonts: Vec<FontName> = font_utils::get_system_fonts()
            .into_iter()
            .map(FontName::from)
            .collect();

        // Create subtitle font selector (default to Arial which should be first or near first in list)
        let subtitle_font_select = cx.new(|cx| {
            let mut state = SelectState::new(system_fonts.clone(), None, window, cx);
            // Set default to first font (Arial)
            state.set_selected_index(Some(IndexPath::new(0)), window, cx);
            state
        });

        // Subscribe to font selection changes
        cx.subscribe(
            &subtitle_font_select,
            |_this, _state_entity, event: &SelectEvent<Vec<FontName>>, cx| {
                if let SelectEvent::Confirm(Some(font_name)) = event {
                    let font_name_str: String = font_name.clone().into();
                    let video_player = cx.global::<AppState>().video_player.clone();
                    cx.update_global::<AppState, _>(|state, _| {
                        state.subtitle_settings.font_family = font_name_str.clone();
                    });
                    if let Ok(player) = video_player.lock() {
                        if let Err(e) = player.set_subtitle_font(&font_name_str) {
                            eprintln!("Failed to set subtitle font: {}", e);
                        }
                    };
                }
            },
        )
        .detach();

        // Create subtitle font size slider (20-100)
        let subtitle_font_size_slider = cx.new(|_cx| {
            SliderState::new()
                .min(20.0)
                .max(100.0)
                .step(1.0)
                .default_value(55.0)
        });

        // Subscribe to font size changes
        cx.subscribe(
            &subtitle_font_size_slider,
            |_this, _, event: &SliderEvent, cx| {
                let SliderEvent::Change(value) = event;
                let size = value.end();
                let app_state = cx.global::<AppState>();
                let video_player = app_state.video_player.clone();
                cx.update_global::<AppState, _>(|state, _| {
                    state.subtitle_settings.font_size = size as f64;
                });
                if let Ok(player) = video_player.lock() {
                    if let Err(e) = player.set_subtitle_font_size(size as f64) {
                        eprintln!("Failed to set subtitle font size: {}", e);
                    }
                };
            },
        )
        .detach();

        Self {
            slider_state: None,
            display_subtitles_enabled: false,
            clip_start_input,
            clip_end_input,
            subtitle_font_select,
            subtitle_font_size_slider,
            subtitle_bold_enabled: false,
            subtitle_italic_enabled: false,
            export_format: ExportFormat::Video,
            current_position: 0.0,
            duration: 0.0,
            is_playing: false,
            clip_start: None,
            clip_end: None,
            is_exporting: false,
            is_playing_clip: false,
            clip_playback_end: None,
            last_seek_time: None,
            last_render_time: Instant::now(),
        }
    }

    /// Handle display subtitles checkbox toggle
    fn toggle_display_subtitles(&mut self, checked: bool, _: &mut Window, cx: &mut Context<Self>) {
        self.display_subtitles_enabled = checked;

        let app_state = cx.global::<AppState>();
        let video_player = app_state.video_player.clone();
        let selected_track = app_state.selected_subtitle_track.map(|t| t as i32);
        cx.update_global::<AppState, _>(|state, _| {
            state.display_subtitles = checked;
        });
        if let Ok(player) = video_player.lock() {
            if let Err(e) = player.set_subtitle_display(checked, selected_track) {
                eprintln!("Failed to set subtitle display: {}", e);
            }
        } else {
            eprintln!("Failed to lock video player for subtitle display toggle");
        };
        cx.notify();
    }

    /// Handle subtitle bold checkbox toggle
    fn toggle_subtitle_bold(&mut self, enabled: bool, _: &mut Window, cx: &mut Context<Self>) {
        self.subtitle_bold_enabled = enabled;

        let app_state = cx.global::<AppState>();
        let video_player = app_state.video_player.clone();
        cx.update_global::<AppState, _>(|state, _| {
            state.subtitle_settings.bold = enabled;
        });
        if let Ok(player) = video_player.lock() {
            if let Err(e) = player.set_subtitle_bold(enabled) {
                eprintln!("Failed to set subtitle bold: {}", e);
            }
        };
        cx.notify();
    }

    /// Handle subtitle italic checkbox toggle
    fn toggle_subtitle_italic(&mut self, enabled: bool, _: &mut Window, cx: &mut Context<Self>) {
        self.subtitle_italic_enabled = enabled;

        let app_state = cx.global::<AppState>();
        let video_player = app_state.video_player.clone();
        cx.update_global::<AppState, _>(|state, _| {
            state.subtitle_settings.italic = enabled;
        });
        if let Ok(player) = video_player.lock() {
            if let Err(e) = player.set_subtitle_italic(enabled) {
                eprintln!("Failed to set subtitle italic: {}", e);
            }
        };
        cx.notify();
    }

    fn update_position_from_player(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let app_state = cx.global::<AppState>();
        let video_player = app_state.video_player.clone();

        if let Ok(player) = video_player.lock() {
            if let Some((position, duration)) = player.get_position_duration() {
                // Use nseconds() to get precise nanosecond timing, then convert to seconds
                self.current_position = position.nseconds() as f32 / 1_000_000_000.0;
                self.duration = duration.nseconds() as f32 / 1_000_000_000.0;

                // Create the slider if we don't have one yet and we have a valid duration
                if self.slider_state.is_none() && self.duration > 0.0 {
                    let slider_state = cx.new(|_cx| {
                        SliderState::new()
                            .min(0.0)
                            .max(self.duration)
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

                    self.slider_state = Some(slider_state);
                }

                // Update slider position if it exists
                if let Some(slider) = &self.slider_state {
                    slider.update(cx, |state, cx| {
                        state.set_value(SliderValue::Single(self.current_position), window, cx);
                    });
                }
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

    /// Set only the clip start time from milliseconds
    pub fn set_clip_start(&mut self, start_ms: u64, cx: &mut Context<Self>) {
        let start_ms_f32 = start_ms as f32;

        // Check if this would violate the constraint (start >= end)
        if let Some(end) = self.clip_end {
            if start_ms_f32 >= end {
                // Unset clip_end if start would be after or equal to it
                self.clip_end = None;
                self.clip_end_input.update(cx, |input, cx| {
                    input.set_content("".to_string(), cx);
                });
            }
        }

        // Set the clip start
        self.clip_start = Some(start_ms_f32);

        // Update the input field
        let start_formatted = Self::format_time_ms(start_ms_f32);

        self.clip_start_input.update(cx, |input, cx| {
            input.set_content(start_formatted, cx);
        });

        cx.notify();
    }

    /// Set only the clip end time from milliseconds
    pub fn set_clip_end(&mut self, end_ms: u64, cx: &mut Context<Self>) {
        let end_ms_f32 = end_ms as f32;

        // Check if this would violate the constraint (end <= start)
        if let Some(start) = self.clip_start {
            if end_ms_f32 <= start {
                // Unset clip_start if end would be before or equal to it
                self.clip_start = None;
                self.clip_start_input.update(cx, |input, cx| {
                    input.set_content("".to_string(), cx);
                });
            }
        }

        // Set the clip end
        self.clip_end = Some(end_ms_f32);

        // Update the input field
        let end_formatted = Self::format_time_ms(end_ms_f32);

        self.clip_end_input.update(cx, |input, cx| {
            input.set_content(end_formatted, cx);
        });

        cx.notify();
    }

    /// Check if there's a valid clip (start and end times set with start < end)
    pub fn has_valid_clip(&self, cx: &Context<Self>) -> bool {
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

        start_ms.is_some() && end_ms.is_some() && start_ms.unwrap() < end_ms.unwrap()
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

        // Get the current export format
        let export_format = self.export_format;

        // Generate default output filename and directory
        let input_path_buf = std::path::PathBuf::from(&input_path);
        let directory = input_path_buf
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));

        // Use appropriate file extension based on export format
        let default_filename = input_path_buf
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("video")
            .to_string()
            + export_format.file_extension();

        // Prompt for save location
        let path_receiver = cx.prompt_for_new_path(directory, Some(&default_filename));

        // Get subtitle settings from AppState
        let subtitle_settings = app_state.subtitle_settings.clone();
        let display_subtitles = app_state.display_subtitles;
        let selected_subtitle_track = app_state.selected_subtitle_track;
        let source_video_width = app_state.source_video_width;

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
                let subtitle_settings_clone = subtitle_settings.clone();

                let export_result = cx
                    .background_executor()
                    .spawn(async move {
                        match export_format {
                            ExportFormat::Gif => {
                                // Export as GIF with subtitle settings
                                crate::ffmpeg_export::export_gif(
                                    &input_path_clone,
                                    &output_path_str_clone,
                                    clip_start,
                                    clip_end,
                                    if display_subtitles {
                                        Some(&subtitle_settings_clone)
                                    } else {
                                        None
                                    },
                                    display_subtitles,
                                    selected_subtitle_track,
                                    source_video_width,
                                )
                            }
                            ExportFormat::Audio => {
                                // Audio export - no-op for now
                                Ok(())
                            }
                            ExportFormat::Video => {
                                // Export as video (MP4)
                                crate::ffmpeg_export::export_clip(
                                    &input_path_clone,
                                    &output_path_str_clone,
                                    clip_start,
                                    clip_end,
                                    if display_subtitles {
                                        Some(&subtitle_settings_clone)
                                    } else {
                                        None
                                    },
                                    display_subtitles,
                                    selected_subtitle_track,
                                    source_video_width,
                                )
                            }
                        }
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
        // Check if a video is loaded
        let app_state = cx.global::<AppState>();
        let has_video_loaded = app_state.has_video_loaded;

        // Only update position if video is loaded
        if has_video_loaded {
            cx.on_next_frame(window, |t, window, cx| {
                // Update from video player and request next frame
                t.update_position_from_player(window, cx);

                // Check if we need to pause during clip playback
                if t.is_playing_clip {
                    if let Some(end_time_ms) = t.clip_playback_end {
                        let current_time_ms = t.current_position * 1000.0;
                        // it takes a moment for the video player to actually update its position when we seek
                        // this means that for a small amount of time, the current_time_ms might be
                        // well ahead or behind or the actual position the user is seeking to.
                        //
                        // it appears to almost always (on this computer) be less than a millisecond or two,
                        // but potentially more than one frame.
                        //
                        // to prevent automatically pausing when the user is trying to play a clip
                        // (because for a moment we think we're past our desired pause point),
                        // we store the time in milliseconds the user was AT when they hit the "play clip" button
                        // if the video player is reporting it's still in the same millisecond or +/- 1 ms, don't auto pause
                        let past_seek_time = t
                            .last_seek_time
                            .map_or(true, |seek_time| (current_time_ms - seek_time).abs() > 0.1);
                        if past_seek_time && current_time_ms >= end_time_ms {
                            // Stop clip playback
                            t.is_playing_clip = false;
                            t.clip_playback_end = None;

                            // Pause the player
                            let app_state = cx.global::<AppState>();
                            let video_player = app_state.video_player.clone();
                            if let Ok(player) = video_player.lock() {
                                println!("Pausing because of the clip playback end check");
                                if let Err(e) = player.pause() {
                                    eprintln!("Failed to pause after clip playback: {}", e);
                                }
                            };
                        }
                    }
                }

                // Rate limit renders to 30 FPS (33.33ms per frame)
                const FRAME_DURATION_MS: u128 = 33; // 1000ms / 30fps ≈ 33.33ms
                let now = Instant::now();
                let elapsed = now.duration_since(t.last_render_time).as_millis();

                if elapsed >= FRAME_DURATION_MS {
                    t.last_render_time = now;
                    // Request another render on next frame to create continuous updates
                    cx.notify();
                }
            });
        }

        // Slider is updated in update_position_from_player

        let current_time = self.current_position;
        let duration = if self.duration > 0.0 {
            self.duration
        } else {
            100.0
        };

        let theme = cx.theme();
        // Pre-capture colors for closures
        let hover_bg = theme.element_hover();
        let bg = theme.element_background();
        let text_color = theme.text();
        let text_muted_color = theme.text_muted();
        let text_disabled_color = theme.text_disabled();
        let warning_bg = theme.warning();
        let border_variant_color = theme.border_variant();
        let surface_bg = theme.surface_background();

        div()
            .flex()
            .flex_col()
            .bg(surface_bg)
            .size_full()
            .p_4()
            .gap_3()
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
                            .text_color(text_color)
                            .child(Self::format_time(current_time))
                            .child(Self::format_time(duration)),
                    )
                    // Slider (only shown when video is loaded)
                    .when_some(self.slider_state.as_ref(), |this, slider_state| {
                        this.child(Slider::new(slider_state).horizontal())
                    }),
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
                                                    .bg(hover_bg)
                                                    .rounded_md()
                                                    .cursor_pointer()
                                                    .text_xs()
                                                    .text_color(text_color)
                                                    .hover(move |style| style.bg(bg))
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
                                                    .bg(hover_bg)
                                                    .rounded_md()
                                                    .cursor_pointer()
                                                    .text_xs()
                                                    .text_color(text_color)
                                                    .hover(move |style| style.bg(bg))
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
                                            // Export format button - cycles through video/gif/audio
                                            .child(
                                                div()
                                                    .px_2()
                                                    .py_1()
                                                    .bg(hover_bg)
                                                    .rounded_md()
                                                    .cursor_pointer()
                                                    .text_xs()
                                                    .text_color(text_color)
                                                    .hover(move |style| style.bg(bg))
                                                    .on_mouse_down(
                                                        MouseButton::Left,
                                                        cx.listener(|this, _, _, cx| {
                                                            // Cycle through formats: video -> gif -> audio -> video
                                                            this.export_format =
                                                                this.export_format.next();
                                                            cx.notify();
                                                        }),
                                                    )
                                                    .child(format!(
                                                        "{}",
                                                        self.export_format.as_str().to_uppercase()
                                                    )),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(if is_valid {
                                                        text_color
                                                    } else {
                                                        text_disabled_color
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
                                                        this.bg(warning_bg)
                                                            .cursor_pointer()
                                                            .text_color(text_color)
                                                            .hover(move |style| {
                                                                style.bg(warning_bg)
                                                            })
                                                    })
                                                    .when(!is_valid || self.is_exporting, |this| {
                                                        this.bg(bg)
                                                            .cursor_not_allowed()
                                                            .text_color(text_disabled_color)
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
                                    .bg(hover_bg)
                                    .rounded_md()
                                    .cursor_pointer()
                                    .text_color(text_color)
                                    .hover(move |style| style.bg(bg))
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
                                        this.bg(hover_bg)
                                            .cursor_pointer()
                                            .text_color(text_color)
                                            .hover(move |style| style.bg(bg))
                                    })
                                    .when(!is_valid, |this| {
                                        this.bg(bg)
                                            .cursor_not_allowed()
                                            .text_color(text_disabled_color)
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
                                                    let video_player =
                                                        app_state.video_player.clone();

                                                    if let Ok(player) = video_player.lock() {
                                                        // Convert milliseconds to nanoseconds for seeking
                                                        let nanos = (start * 1_000_000.0) as u64;
                                                        let clock_time =
                                                            ClockTime::from_nseconds(nanos);

                                                        if let Err(e) = player.seek(clock_time) {
                                                            eprintln!(
                                                                "Failed to seek to clip start: {}",
                                                                e
                                                            );
                                                        } else if let Err(e) = player.play() {
                                                            eprintln!("Failed to play clip: {}", e);
                                                        } else {
                                                            // Set up clip playback mode
                                                            this.is_playing_clip = true;
                                                            this.clip_playback_end = Some(end);
                                                            this.last_seek_time = Some(
                                                                this.current_position
                                                                    * (1000 as f32),
                                                            );
                                                        }
                                                    };
                                                }
                                            }
                                        }),
                                    )
                                    .child("Play Clip")
                            }),
                    )
                    // Right side: Display subtitles checkbox and styling controls
                    .child({
                        let display_subtitles_enabled = self.display_subtitles_enabled;
                        let subtitle_bold_enabled = self.subtitle_bold_enabled;
                        let subtitle_italic_enabled = self.subtitle_italic_enabled;

                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .p_2()
                            .bg(surface_bg)
                            .border_1()
                            .border_color(border_variant_color)
                            .rounded(px(4.))
                            .min_w(px(250.0))
                            // Top row: Display subtitles, Bold, Italic checkboxes
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        Checkbox::new("display-subtitles-checkbox")
                                            .label("Subtitles")
                                            .checked(display_subtitles_enabled)
                                            .on_click(cx.listener(|this, checked, window, cx| {
                                                this.toggle_display_subtitles(*checked, window, cx);
                                            })),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .gap_3()
                                            .child(
                                                Checkbox::new("subtitle-bold-checkbox")
                                                    .label("Bold")
                                                    .checked(subtitle_bold_enabled)
                                                    .on_click(cx.listener(
                                                        |this, checked, window, cx| {
                                                            this.toggle_subtitle_bold(
                                                                *checked, window, cx,
                                                            );
                                                        },
                                                    )),
                                            )
                                            .child(
                                                Checkbox::new("subtitle-italic-checkbox")
                                                    .label("Italic")
                                                    .checked(subtitle_italic_enabled)
                                                    .on_click(cx.listener(
                                                        |this, checked, window, cx| {
                                                            this.toggle_subtitle_italic(
                                                                *checked, window, cx,
                                                            );
                                                        },
                                                    )),
                                            ),
                                    ),
                            )
                            // Font selector and size slider
                            .child(
                                div()
                                    .flex()
                                    .items_start()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex_1()
                                            .child(Select::new(&self.subtitle_font_select)),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .flex()
                                            .flex_col()
                                            .gap_1()
                                            .child(
                                                div().text_xs().text_color(text_muted_color).child(
                                                    format!(
                                                        "Size: {:.0}",
                                                        self.subtitle_font_size_slider
                                                            .read(cx)
                                                            .value()
                                                            .end()
                                                    ),
                                                ),
                                            )
                                            .child(Slider::new(&self.subtitle_font_size_slider))
                                            .pt_neg_1(), //this moves the "size: x" and slider below it up ever so slightly to be even with the font dropdown
                                    ),
                            )
                            .pb_neg_1()
                    }),
            )
    }
}
