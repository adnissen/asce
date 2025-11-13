//! ASVE - Video Editor with GPUI
//!
//! A simple video player application built with GPUI and GStreamer.

mod checkbox;
mod ffmpeg_export;
mod search_input;
mod select;
mod slider;
mod subtitle_detector;
mod subtitle_extractor;
mod video_player;

use checkbox::{Checkbox, CheckboxEvent, CheckboxState};
use gpui::{
    AnyWindowHandle, App, Application, Context, Entity, Global, Menu, MenuItem, PathPromptOptions,
    ScrollStrategy, SystemMenuType, UniformListScrollHandle, Window, WindowOptions, actions, div,
    prelude::*, px, rgb, uniform_list,
};
use raw_window_handle::RawWindowHandle;
use search_input::SearchInput;
use select::{Select, SelectEvent, SelectItem, SelectState};
use slider::{Slider, SliderEvent, SliderState, SliderValue};
use subtitle_detector::SubtitleStream;
use subtitle_extractor::SubtitleEntry;

use std::sync::{Arc, Mutex};

// Implement SelectItem for SubtitleStream
impl SelectItem for SubtitleStream {
    fn display_title(&self) -> String {
        self.display_title.clone()
    }

    fn value(&self) -> usize {
        self.index
    }
}

/// Initial window that shows just an "Open File" button
struct InitialWindow;

impl Render for InitialWindow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .bg(rgb(0x1a1a1a))
            .size_full()
            .justify_center()
            .items_center()
            .child(
                div()
                    .id("open-file-button")
                    .px_8()
                    .py_4()
                    .bg(rgb(0x404040))
                    .rounded_lg()
                    .cursor_pointer()
                    .text_xl()
                    .text_color(rgb(0xffffff))
                    .hover(|style| style.bg(rgb(0x505050)))
                    .on_click(|_, _window, cx| {
                        let paths = cx.prompt_for_paths(PathPromptOptions {
                            files: true,
                            directories: false,
                            multiple: false,
                            prompt: Some("Select a video file".into()),
                        });

                        cx.spawn(async move |cx| {
                            if let Ok(Ok(Some(paths))) = paths.await {
                                if let Some(path) = paths.first() {
                                    // Check if the file has a valid extension
                                    let extension = path.extension().and_then(|e| e.to_str());
                                    let supported_extensions =
                                        ffmpeg_export::get_video_extensions();

                                    if let Some(ext) = extension {
                                        let ext_lower = ext.to_lowercase();
                                        if supported_extensions.contains(&ext_lower.as_str()) {
                                            let path_string = path.to_string_lossy().to_string();
                                            let path_clone = path_string.clone();

                                            cx.update(|cx| {
                                                create_video_windows(cx, path_string, path_clone);
                                            })
                                            .ok();
                                        } else {
                                            // Invalid file type
                                            eprintln!(
                                                "Invalid file type. Supported formats: {}",
                                                supported_extensions.join(", ")
                                            );
                                        }
                                    }
                                }
                            }
                        })
                        .detach();
                    })
                    .child("Open File"),
            )
    }
}

/// Video player window that displays the video
struct VideoPlayerWindow;

impl Render for VideoPlayerWindow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // Full window for video display - GStreamer will render directly to this window
        div().flex().bg(rgb(0x000000)).size_full()
    }
}

/// Subtitle window with stream selection and SRT display
struct SubtitleWindow {
    select_state: Entity<SelectState<SubtitleStream>>,
    sync_subtitles_to_video: Entity<CheckboxState>,
    search_input: Entity<SearchInput>,
    subtitle_entries: Vec<SubtitleEntry>,
    current_position: f32,                 // Current video position in seconds
    current_subtitle_index: Option<usize>, // Index of the currently active subtitle (from video position)
    scroll_handle: UniformListScrollHandle,
    search_result_indices: Vec<usize>, // All indices that match the search
    current_search_result_index: Option<usize>, // Index within search_result_indices of the current result
    last_scrolled_to_search: Option<usize>, // Last search result we scrolled to (to avoid re-scrolling)
    last_scrolled_to_video: Option<usize>,  // Last video position we scrolled to
}

impl SubtitleWindow {
    fn new(cx: &mut Context<Self>) -> Self {
        // Create select state with empty items initially
        let select_state = cx.new(|_cx| SelectState::new(Vec::new()));

        // Subscribe to select events to load the selected subtitle stream
        cx.subscribe(&select_state, |this, _, event: &SelectEvent, cx| {
            let SelectEvent::Change(index) = event;
            this.load_subtitle_stream(*index, cx);

            // Update AppState with the selected subtitle track
            cx.update_global::<AppState, _>(|state, _| {
                state.selected_subtitle_track = Some(*index);
            });

            // If subtitle display is enabled in controls, update the video player
            let app_state = cx.global::<AppState>();
            let video_player = app_state.video_player.clone();
            if let Ok(player) = video_player.lock() {
                if let Err(e) = player.set_subtitle_track(*index as i32) {
                    eprintln!("Failed to set subtitle track: {}", e);
                }
            }
        })
        .detach();

        // Create checkbox state with default checked (synced to video)
        let sync_subtitles_to_video = cx.new(|_cx| CheckboxState::new(true));

        // Subscribe to checkbox events to update AppState
        cx.subscribe(
            &sync_subtitles_to_video,
            |_this, _, event: &CheckboxEvent, cx| {
                let CheckboxEvent::Change(checked) = event;
                cx.update_global::<AppState, _>(|state, _| {
                    state.synced_to_video = *checked;
                });
            },
        )
        .detach();

        // Create search input
        let search_input = cx.new(|cx| SearchInput::new(cx));

        // Subscribe to search input changes to update results in real-time
        cx.observe(&search_input, |this, _search_input, cx| {
            // When search input changes, update the search results
            this.update_search_results(cx);
        })
        .detach();

        Self {
            select_state,
            sync_subtitles_to_video,
            search_input,
            subtitle_entries: Vec::new(),
            current_position: 0.0,
            current_subtitle_index: None,
            scroll_handle: UniformListScrollHandle::new(),
            search_result_indices: Vec::new(),
            current_search_result_index: None,
            last_scrolled_to_search: None,
            last_scrolled_to_video: None,
        }
    }

    /// Load subtitle streams for the current video file
    fn load_subtitle_streams(&mut self, file_path: &str, cx: &mut Context<Self>) {
        // Detect subtitle streams
        let streams = subtitle_detector::detect_subtitle_streams(file_path);

        if streams.is_empty() {
            println!("No text-based subtitle streams found");
            return;
        }

        println!("Found {} subtitle stream(s)", streams.len());

        // Update select state with streams
        self.select_state.update(cx, |state, cx| {
            state.set_items(streams.clone(), cx);
            // Select the first stream by default
            if !streams.is_empty() {
                state.set_selected_index(Some(0), cx);
            }
        });

        // Load the first stream by default
        if !streams.is_empty() {
            self.load_subtitle_stream(0, cx);
        }
    }

    /// Update the current position from the video player
    fn update_position_from_player(&mut self, cx: &mut Context<Self>) {
        let app_state = cx.global::<AppState>();
        let video_player = app_state.video_player.clone();

        if let Ok(player) = video_player.lock() {
            if let Some((position, _duration)) = player.get_position_duration() {
                // Convert nanoseconds to seconds
                self.current_position = position.nseconds() as f32 / 1_000_000_000.0;

                // Find the current subtitle index if synced to video
                if app_state.synced_to_video {
                    self.current_subtitle_index = self.find_subtitle_at_time(self.current_position);
                } else {
                    self.current_subtitle_index = None;
                }
            }
        }
    }

    /// Find the subtitle entry that corresponds to the given time (in seconds)
    fn find_subtitle_at_time(&self, time_secs: f32) -> Option<usize> {
        let time_ms = (time_secs * 1000.0) as u64;

        self.subtitle_entries
            .iter()
            .position(|entry| entry.start_ms <= time_ms && time_ms <= entry.end_ms)
    }

    /// Search for all subtitles matching the search text and find all matches
    fn update_search_results(&mut self, cx: &mut Context<Self>) {
        let search_text = self.search_input.read(cx).content();

        // Clear previous results
        self.search_result_indices.clear();
        self.current_search_result_index = None;

        if search_text.is_empty() {
            cx.notify();
            return;
        }

        let search_text_lower = search_text.to_lowercase();

        // Find all matching indices
        for (i, entry) in self.subtitle_entries.iter().enumerate() {
            if entry.text.to_lowercase().contains(&search_text_lower) {
                self.search_result_indices.push(i);
            }
        }

        if self.search_result_indices.is_empty() {
            println!("No matches found for: {}", search_text);
        } else {
            println!(
                "Found {} match(es) for: {}",
                self.search_result_indices.len(),
                search_text
            );
            // Start at the first result
            self.current_search_result_index = Some(0);

            // Disable sync to video
            self.sync_subtitles_to_video.update(cx, |state, cx| {
                state.set_checked(false, cx);
            });
        }

        cx.notify();
    }

    /// Move to the next search result (cycling)
    fn search_next(&mut self, cx: &mut Context<Self>) {
        let search_text = self.search_input.read(cx).content();

        // If search text changed, update results
        if search_text.is_empty() {
            self.search_result_indices.clear();
            self.current_search_result_index = None;
            cx.notify();
            return;
        }

        // If we don't have results yet, find them
        if self.search_result_indices.is_empty() {
            self.update_search_results(cx);
            return;
        }

        // Move to next result (cycle)
        if let Some(current_idx) = self.current_search_result_index {
            let next_idx = (current_idx + 1) % self.search_result_indices.len();
            self.current_search_result_index = Some(next_idx);
            cx.notify();
        }
    }

    /// Get the subtitle index of the current search result
    fn current_search_subtitle_index(&self) -> Option<usize> {
        self.current_search_result_index
            .and_then(|idx| self.search_result_indices.get(idx).copied())
    }

    /// Handle Enter key in search input
    fn on_search_enter(&mut self, cx: &mut Context<Self>) {
        self.search_next(cx);
    }

    /// Handle Escape key in search input
    fn on_search_escape(&mut self, cx: &mut Context<Self>) {
        // Clear search and results
        self.search_input.update(cx, |input, cx| {
            input.clear(cx);
        });
        self.search_result_indices.clear();
        self.current_search_result_index = None;
        self.last_scrolled_to_search = None; // Clear scroll tracking
        cx.notify();
    }

    /// Load a specific subtitle stream by index
    fn load_subtitle_stream(&mut self, stream_index: usize, cx: &mut Context<Self>) {
        let app_state = cx.global::<AppState>();
        let file_path = match &app_state.file_path {
            Some(path) => path.clone(),
            None => {
                eprintln!("No video file loaded");
                return;
            }
        };

        println!("Loading subtitle stream {}", stream_index);

        // Extract subtitle stream to SRT
        match subtitle_extractor::extract_subtitle_stream(&file_path, stream_index) {
            Ok(srt_content) => {
                // Parse SRT content
                self.subtitle_entries = subtitle_extractor::parse_srt(&srt_content);
                println!("Loaded {} subtitle entries", self.subtitle_entries.len());

                cx.notify();
            }
            Err(e) => {
                eprintln!("Failed to extract subtitle stream: {}", e);
            }
        }
    }
}

impl Render for SubtitleWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Update position from video player every frame
        cx.on_next_frame(window, |this, _window, cx| {
            this.update_position_from_player(cx);
            // Request another render on next frame for continuous updates
            cx.notify();
        });

        // Auto-scroll: prioritize search result over video position
        // Only scroll when the target actually changes to avoid overriding manual scrolling
        if let Some(search_subtitle_idx) = self.current_search_subtitle_index() {
            // If actively searching, scroll to the current search result (only if it changed)
            if self.last_scrolled_to_search != Some(search_subtitle_idx) {
                self.scroll_handle
                    .scroll_to_item(search_subtitle_idx, ScrollStrategy::Center);
                self.last_scrolled_to_search = Some(search_subtitle_idx);
            }
        } else if let Some(current_idx) = self.current_subtitle_index {
            // Otherwise, scroll to current video position subtitle (only if it changed)
            if self.last_scrolled_to_video != Some(current_idx) {
                self.scroll_handle
                    .scroll_to_item(current_idx, ScrollStrategy::Bottom);
                self.last_scrolled_to_video = Some(current_idx);
            }
        }

        let entries = self.subtitle_entries.clone();
        let item_count = entries.len();
        let current_subtitle_index = self.current_subtitle_index;
        let search_result_indices = self.search_result_indices.clone();
        let current_search_subtitle_idx = self.current_search_subtitle_index();

        div()
            .flex()
            .flex_col()
            .bg(rgb(0x1a1a1a))
            .size_full()
            .p_4()
            .gap_4()
            .child(
                // Controls section
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap_2()
                    // First row: Checkbox and Select
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .items_center()
                            .child(
                                // Checkbox for syncing to video
                                Checkbox::new(&self.sync_subtitles_to_video)
                                    .label("Synced to video"),
                            )
                            .child(
                                // Select dropdown at half width
                                div().flex_1().child(
                                    Select::new(&self.select_state)
                                        .placeholder("No subtitles available"),
                                ),
                            ),
                    )
                    // Second row: Search input
                    .child(
                        div()
                            .w_full()
                            .on_action(cx.listener(|this, _: &search_input::Enter, _, cx| {
                                this.on_search_enter(cx);
                            }))
                            .on_action(cx.listener(|this, _: &search_input::Escape, _, cx| {
                                this.on_search_escape(cx);
                            }))
                            .child(self.search_input.clone()),
                    ),
            )
            .child(
                // Uniform list for displaying subtitles
                div().id("subtitle-list-container").flex_1().w_full().child(
                    uniform_list("subtitle-list", item_count, move |range, _window, _cx| {
                        range
                            .filter_map(|idx| {
                                entries.get(idx).map(|entry| {
                                    let is_current_video_subtitle =
                                        current_subtitle_index == Some(idx);
                                    let is_search_result = search_result_indices.contains(&idx);
                                    let is_active_search_result =
                                        current_search_subtitle_idx == Some(idx);
                                    let start_ms = entry.start_ms;
                                    div()
                                        .w_full()
                                        .h(px(60.0))
                                        .px_3()
                                        .py_2()
                                        .border_b_1()
                                        .border_color(rgb(0x333333))
                                        .cursor_pointer()
                                        .hover(|style| style.bg(rgb(0x404040)))
                                        // Prioritize active search result > current video subtitle > regular search result
                                        .when(is_active_search_result, |div| {
                                            div.bg(rgb(0xffa726)) // Bright orange for active search result
                                        })
                                        .when(is_search_result && !is_active_search_result, |div| {
                                            div.bg(rgb(0x5d4037)) // Dark brown for other search results
                                        })
                                        .when(
                                            is_current_video_subtitle
                                                && !is_active_search_result
                                                && !is_search_result,
                                            |div| {
                                                div.bg(rgb(0x2e7d32)) // Green for current video subtitle
                                            },
                                        )
                                        .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                                            // Seek the video player to the start time of this subtitle
                                            let app_state = cx.global::<AppState>();
                                            let video_player = app_state.video_player.clone();

                                            if let Ok(player) = video_player.lock() {
                                                use gstreamer::ClockTime;
                                                // Convert milliseconds to nanoseconds
                                                let nanos = start_ms * 1_000_000;
                                                let clock_time = ClockTime::from_nseconds(nanos);

                                                println!(
                                                    "Seeking to subtitle at: {}ms ({}ns)",
                                                    start_ms, nanos
                                                );

                                                if let Err(e) = player.seek(clock_time) {
                                                    eprintln!("Failed to seek: {}", e);
                                                }
                                            }
                                        })
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap_1()
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(rgb(0x888888))
                                                        .child(format!(
                                                            "{} --> {}",
                                                            entry.format_start_time(),
                                                            entry.format_end_time()
                                                        )),
                                                )
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .text_color(rgb(0xffffff))
                                                        .child(entry.text.clone()),
                                                ),
                                        )
                                })
                            })
                            .collect()
                    })
                    .track_scroll(self.scroll_handle.clone())
                    .w_full()
                    .h_full(),
                ),
            )
    }
}

/// Controls window with play/pause/stop buttons and video scrubber
struct ControlsWindow {
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
    fn new(cx: &mut Context<Self>) -> Self {
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
                use gstreamer::ClockTime;
                let nanos = (position_secs * 1_000_000_000.0) as u64;
                let clock_time = ClockTime::from_nseconds(nanos);
                if let Err(e) = player.seek(clock_time) {
                    eprintln!("Failed to seek: {}", e);
                }
            }
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
                    // Enable or disable subtitle display
                    if let Err(e) = player.set_subtitle_display(*checked, selected_track) {
                        eprintln!("Failed to set subtitle display: {}", e);
                    }
                }
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
        }
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
                        ffmpeg_export::export_clip(
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
                                                        gpui::MouseButton::Left,
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
                                                        gpui::MouseButton::Left,
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
                                                        gpui::MouseButton::Left,
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
                                gpui::MouseButton::Left,
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
                                    }
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

fn main() {
    // Initialize GStreamer before creating the GPUI application
    if let Err(e) = video_player::init() {
        eprintln!("Failed to initialize GStreamer: {}", e);
        eprintln!(
            "Make sure GStreamer is installed: brew install gstreamer gst-plugins-base gst-plugins-good"
        );
        std::process::exit(1);
    }

    Application::new().run(|cx: &mut App| {
        cx.set_global(AppState::new());

        // Bring the menu bar to the foreground (so you can see the menu bar)
        cx.activate(true);
        // Register the `quit` function so it can be referenced by the `MenuItem::action` in the menu bar
        cx.on_action(quit);
        cx.on_action(open_file);

        // Bind keys for search input
        cx.bind_keys([
            gpui::KeyBinding::new("enter", search_input::Enter, Some("SearchInput")),
            gpui::KeyBinding::new("escape", search_input::Escape, Some("SearchInput")),
            gpui::KeyBinding::new("backspace", search_input::Backspace, Some("SearchInput")),
        ]);

        // Add menu items
        set_app_menus(cx);

        // Create a small initial window with just the "Open File" button
        let initial_window_options = WindowOptions {
            window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds::centered(
                None,
                gpui::size(px(300.0), px(200.0)),
                cx,
            ))),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("asve".into()),
                appears_transparent: false,
                ..Default::default()
            }),
            ..Default::default()
        };

        let window = cx
            .open_window(initial_window_options, |_window, cx| {
                cx.new(|_| InitialWindow {})
            })
            .unwrap();

        // Store the initial window handle
        cx.update_global::<AppState, _>(|state, _| {
            state.initial_window = Some(window.into());
        });

        println!("Initial window created");
    });
}

/// Extract the native NSView handle from GPUI and set it on the video player
///
/// This function uses the stored AnyWindowHandle to access the window's window_handle()
/// method, which provides raw window handle access via the raw-window-handle crate.
/// On macOS, this extracts the NSView pointer needed for GStreamer video rendering.
fn extract_and_set_display_handle(cx: &mut App) {
    let app_state = cx.global::<AppState>();

    if let Some(window_handle) = app_state.video_window() {
        let video_player = app_state.video_player.clone();

        // Access the window through the handle to get the window handle
        window_handle
            .update(cx, |_view, window, _app| {
                // Get the raw window handle from the window using the HasWindowHandle trait
                use raw_window_handle::HasWindowHandle;
                match window.window_handle() {
                    Ok(window_handle_obj) => {
                        // Extract the platform-specific handle
                        let raw_handle = window_handle_obj.as_raw();

                        match raw_handle {
                            RawWindowHandle::AppKit(appkit_handle) => {
                                // Extract the NSView pointer from the AppKit handle
                                // The ns_view field is a NonNull<c_void> which is safe to access
                                let ns_view_ptr = appkit_handle.ns_view.as_ptr() as usize;

                                println!("NSView pointer extracted: 0x{:x}", ns_view_ptr);

                                // Get the window bounds to calculate render rectangle
                                let bounds = window.bounds();
                                // Pixels is a wrapper around f32, we need to extract the value
                                // Using format! to convert to string then parse is a workaround
                                let width_str = format!("{}", bounds.size.width);
                                let height_str = format!("{}", bounds.size.height);
                                let window_width: i32 =
                                    width_str.trim_end_matches("px").parse().unwrap_or(800);
                                let window_height: i32 =
                                    height_str.trim_end_matches("px").parse().unwrap_or(600);

                                println!("Window size: {}x{}", window_width, window_height);

                                // Pass the NSView pointer and render rectangle to the video player
                                if let Ok(mut player) = video_player.lock() {
                                    player.set_window_handle(ns_view_ptr);
                                    println!(
                                        "NSView pointer and render rectangle set on video player"
                                    );
                                } else {
                                    eprintln!("Failed to lock video player mutex");
                                }
                            }
                            _ => {
                                eprintln!(
                                    "Unsupported platform window handle type: {:?}",
                                    raw_handle
                                );
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to get window handle: {:?}", e);
                    }
                }
            })
            .ok();
    } else {
        eprintln!("No video window handle stored in AppState");
    }
}

struct AppState {
    file_path: Option<String>,
    initial_window: Option<AnyWindowHandle>,
    video_window: Option<AnyWindowHandle>,
    controls_window: Option<AnyWindowHandle>,
    subtitle_window: Option<AnyWindowHandle>,
    video_player: Arc<Mutex<video_player::VideoPlayer>>,
    synced_to_video: bool,
    selected_subtitle_track: Option<usize>, // Currently selected subtitle track index
}

impl AppState {
    fn new() -> Self {
        Self {
            file_path: None,
            initial_window: None,
            video_window: None,
            controls_window: None,
            subtitle_window: None,
            video_player: Arc::new(Mutex::new(video_player::VideoPlayer::new())),
            synced_to_video: true, // Default to checked/synced
            selected_subtitle_track: None, // No track selected initially
        }
    }

    /// Get the video window handle
    pub fn video_window(&self) -> Option<AnyWindowHandle> {
        self.video_window
    }
}

impl Global for AppState {}

fn set_app_menus(cx: &mut App) {
    cx.set_menus(vec![Menu {
        name: "set_menus".into(),
        items: vec![
            MenuItem::os_submenu("Services", SystemMenuType::Services),
            MenuItem::separator(),
            MenuItem::action("Open...", OpenFile),
            MenuItem::separator(),
            MenuItem::action("Quit", Quit),
        ],
    }]);
}

// Associate actions using the `actions!` macro (or `Action` derive macro)
actions!(set_menus, [Quit, OpenFile]);

// Define the quit function that is registered with the App
fn quit(_: &Quit, cx: &mut App) {
    println!("Gracefully quitting the application . . .");
    cx.quit();
}

/// Create the video player and controls windows and load the video file
fn create_video_windows(cx: &mut App, path_string: String, path_clone: String) {
    // Close existing windows by calling remove_window()
    println!("Closing existing windows");

    // Get handles before clearing state
    let app_state = cx.global::<AppState>();
    let initial_window = app_state.initial_window;
    let video_window = app_state.video_window;
    let controls_window = app_state.controls_window;
    let subtitle_window = app_state.subtitle_window;

    // Close the windows by calling remove_window() on each
    if let Some(window) = initial_window {
        window
            .update(cx, |_, window, _| {
                window.remove_window();
            })
            .ok();
    }
    if let Some(window) = video_window {
        window
            .update(cx, |_, window, _| {
                window.remove_window();
            })
            .ok();
    }
    if let Some(window) = controls_window {
        window
            .update(cx, |_, window, _| {
                window.remove_window();
            })
            .ok();
    }
    if let Some(window) = subtitle_window {
        window
            .update(cx, |_, window, _| {
                window.remove_window();
            })
            .ok();
    }

    // Clear the handles from state
    cx.update_global::<AppState, _>(|state, _| {
        state.initial_window = None;
        state.video_window = None;
        state.controls_window = None;
        state.subtitle_window = None;
    });

    // Extract just the file name from the path for the window title
    let file_name = std::path::Path::new(&path_string)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Video Player")
        .to_string();

    // Calculate video window size (half of typical 1920px screen, maintain 16:9 aspect ratio)
    let video_width = 960.0;
    let video_height = video_width * 9.0 / 16.0; // 540px to maintain 16:9 aspect ratio

    // Create the video player window (closable)
    let video_window_options = WindowOptions {
        window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds {
            origin: gpui::point(px(20.0), px(20.0)), // Start at top of screen with small margin
            size: gpui::size(px(video_width), px(video_height)),
        })),
        window_background: gpui::WindowBackgroundAppearance::Opaque,
        focus: true,
        is_movable: true,
        titlebar: Some(gpui::TitlebarOptions {
            title: Some(file_name.into()),
            appears_transparent: false,
            ..Default::default()
        }),
        ..Default::default()
    };

    let video_window = cx
        .open_window(video_window_options, |_window, cx| {
            cx.new(|_| VideoPlayerWindow {})
        })
        .unwrap();

    println!("Video window created");

    // Get the video window's bounds to position controls below it
    let video_bounds = video_window
        .update(cx, |_, window, _| window.bounds())
        .unwrap();

    // Calculate position for controls window (directly below video window)
    let controls_x = video_bounds.origin.x;
    let controls_y = video_bounds.origin.y + video_bounds.size.height;
    let controls_width = video_bounds.size.width; // Match video window width

    // Create the controls window (not closable, no title)
    let controls_window_options = WindowOptions {
        window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds {
            origin: gpui::point(controls_x, controls_y),
            size: gpui::size(controls_width, px(180.0)),
        })),
        window_background: gpui::WindowBackgroundAppearance::Opaque,
        focus: false,
        is_movable: true,
        titlebar: Some(gpui::TitlebarOptions {
            title: None,
            appears_transparent: false,
            ..Default::default()
        }),
        ..Default::default()
    };

    let controls_window = cx
        .open_window(controls_window_options, |_window, cx| {
            cx.new(|cx| ControlsWindow::new(cx))
        })
        .unwrap();

    println!("Controls window created");

    // Calculate position for subtitle window (to the right of video window)
    let subtitle_x = video_bounds.origin.x + video_bounds.size.width;
    let subtitle_y = video_bounds.origin.y;
    let subtitle_width = px(300.0); // Proportionally scaled subtitle window width
    let subtitle_height = video_bounds.size.height; // Match video window height

    // Create the subtitle window
    let subtitle_window_options = WindowOptions {
        window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds {
            origin: gpui::point(subtitle_x, subtitle_y),
            size: gpui::size(subtitle_width, subtitle_height),
        })),
        window_background: gpui::WindowBackgroundAppearance::Opaque,
        focus: false,
        is_movable: true,
        titlebar: Some(gpui::TitlebarOptions {
            title: Some("Subtitles".into()),
            appears_transparent: false,
            ..Default::default()
        }),
        ..Default::default()
    };

    let subtitle_window = cx
        .open_window(subtitle_window_options, |_window, cx| {
            cx.new(|cx| SubtitleWindow::new(cx))
        })
        .unwrap();

    println!("Subtitle window created");

    // Update AppState with new windows and file path
    cx.update_global::<AppState, _>(|state, _| {
        state.video_window = Some(video_window.into());
        state.controls_window = Some(controls_window.into());
        state.subtitle_window = Some(subtitle_window.into());
        state.file_path = Some(path_string.clone());
    });

    // Extract and set the display handle for the video window
    extract_and_set_display_handle(cx);

    // Load the video file
    let app_state = cx.global::<AppState>();
    let video_player = app_state.video_player.clone();
    if let Ok(mut player) = video_player.lock() {
        println!("Loading video file: {}", path_clone);

        // Load the file into the pipeline
        match player.load_file(&path_clone) {
            Ok(()) => {
                println!("Video file loaded successfully");

                // Start the bus watch to handle messages via GLib main loop
                match player.start_message_watch() {
                    Ok(()) => {
                        println!("Bus watch started successfully");
                    }
                    Err(e) => {
                        eprintln!("Failed to start bus watch: {}", e);
                    }
                }

                // Auto-play and immediately pause to get duration information
                if let Err(e) = player.play() {
                    eprintln!("Failed to auto-play: {}", e);
                } else {
                    println!("Auto-played video to get duration");
                    // Immediately pause
                    if let Err(e) = player.pause() {
                        eprintln!("Failed to pause: {}", e);
                    } else {
                        println!("Video paused and ready");
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to load video file: {}", e);
            }
        }
    }

    // Load subtitle streams in the subtitle window
    if let Some(subtitle_window_handle) = cx.global::<AppState>().subtitle_window {
        subtitle_window_handle
            .update(cx, |any_view, _, app_cx| {
                if let Ok(subtitle_window) = any_view.downcast::<SubtitleWindow>() {
                    subtitle_window.update(app_cx, |subtitle_window, cx| {
                        subtitle_window.load_subtitle_streams(&path_clone, cx);
                    });
                }
            })
            .ok();
    }
}

// Define the open file function that prompts for a file path
fn open_file(_: &OpenFile, cx: &mut App) {
    let paths = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: false,
        prompt: Some("Select a video file".into()),
    });

    cx.spawn(async move |cx| {
        if let Ok(Ok(Some(paths))) = paths.await {
            if let Some(path) = paths.first() {
                // Check if the file has a valid extension
                let extension = path.extension().and_then(|e| e.to_str());
                let supported_extensions = ffmpeg_export::get_video_extensions();

                if let Some(ext) = extension {
                    let ext_lower = ext.to_lowercase();
                    if supported_extensions.contains(&ext_lower.as_str()) {
                        let path_string = path.to_string_lossy().to_string();
                        let path_clone = path_string.clone();

                        cx.update(|cx| {
                            create_video_windows(cx, path_string, path_clone);
                        })
                        .ok();
                    } else {
                        // Invalid file type
                        eprintln!(
                            "Invalid file type. Supported formats: {}",
                            supported_extensions.join(", ")
                        );
                    }
                }
            }
        }
    })
    .detach();
}
