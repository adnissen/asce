use gpui::{
    div, prelude::*, px, rgb, uniform_list, Context, Entity, IntoElement, MouseButton, Pixels,
    Point, Render, ScrollStrategy, UniformListScrollHandle, Window,
};

use crate::checkbox::{Checkbox, CheckboxEvent, CheckboxState};
use crate::search_input::{self, SearchInput};
use crate::select::{Select, SelectEvent, SelectItem, SelectState};
use crate::subtitle_detector::SubtitleStream;
use crate::subtitle_extractor::SubtitleEntry;
use crate::video_player::ClockTime;
use crate::AppState;

// Implement SelectItem for SubtitleStream
impl SelectItem for SubtitleStream {
    fn display_title(&self) -> String {
        self.display_title.clone()
    }
}

/// Subtitle window with stream selection and SRT display
pub struct SubtitleWindow {
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
    pub context_menu: Option<ContextMenuState>, // Right-click context menu state (public so unified window can close it)
}

/// State for the right-click context menu
pub struct ContextMenuState {
    pub position: Point<Pixels>,
    pub subtitle_index: usize,
}

// Data structure to hold loaded subtitle information
pub struct SubtitleData {
    pub streams: Vec<SubtitleStream>,
    pub first_stream_entries: Vec<SubtitleEntry>,
}

impl SubtitleWindow {
    /// Load subtitle data on a background thread (safe to call from non-UI thread)
    /// Returns the streams and parsed entries for the first stream
    pub fn load_subtitle_data_blocking(file_path: &str) -> Option<SubtitleData> {
        // Detect subtitle streams (blocking ffprobe call)
        let streams = crate::subtitle_detector::detect_subtitle_streams(file_path);

        if streams.is_empty() {
            println!("No text-based subtitle streams found");
            return None;
        }

        println!("Found {} subtitle stream(s)", streams.len());

        // Extract and parse the first stream (blocking ffmpeg call)
        let first_stream_entries =
            match crate::subtitle_extractor::extract_subtitle_stream(file_path, 0) {
                Ok(srt_content) => {
                    let entries = crate::subtitle_extractor::parse_srt(&srt_content);
                    println!("Loaded {} subtitle entries", entries.len());
                    entries
                }
                Err(e) => {
                    eprintln!("Failed to extract subtitle stream: {}", e);
                    Vec::new()
                }
            };

        Some(SubtitleData {
            streams,
            first_stream_entries,
        })
    }

    /// Update the subtitle window with pre-loaded data (must be called on UI thread)
    pub fn update_with_loaded_data(&mut self, data: SubtitleData, cx: &mut Context<Self>) {
        // Update select state with streams
        self.select_state.update(cx, |state, cx| {
            state.set_items(data.streams.clone(), cx);
            // Select the first stream by default
            if !data.streams.is_empty() {
                state.set_selected_index(Some(0), cx);
            }
        });

        // Set the subtitle entries
        self.subtitle_entries = data.first_stream_entries;

        cx.notify();
    }

    pub fn new(cx: &mut Context<Self>) -> Self {
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
            } else {
                eprintln!("Failed to lock video player for subtitle track change");
            };
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
            context_menu: None,
        }
    }

    /// Load subtitle streams for the current video file
    pub fn load_subtitle_streams(&mut self, file_path: &str, cx: &mut Context<Self>) {
        // Detect subtitle streams
        let streams = crate::subtitle_detector::detect_subtitle_streams(file_path);

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
        };
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
        match crate::subtitle_extractor::extract_subtitle_stream(&file_path, stream_index) {
            Ok(srt_content) => {
                // Parse SRT content
                self.subtitle_entries = crate::subtitle_extractor::parse_srt(&srt_content);
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
            .relative() // Add relative positioning for absolute children
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
                                Checkbox::new(&self.sync_subtitles_to_video).label("Sync"),
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
                                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                            // Seek the video player to the start time of this subtitle
                                            let app_state = cx.global::<AppState>();
                                            let video_player = app_state.video_player.clone();

                                            if let Ok(player) = video_player.lock() {
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
                                            };
                                        })
                                        .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                                            // Show context menu on right-click
                                            println!("Right-click detected on subtitle entry {}", idx);
                                            let position = event.position;
                                            println!("Mouse position (window coords): {:?}", position);

                                            // Get window bounds to calculate subtitle window offset
                                            let window_bounds = window.bounds();
                                            let video_width = window_bounds.size.width * 0.76;

                                            // Convert to subtitle-window-relative coordinates
                                            // Subtract the video width since subtitle window starts after video
                                            let relative_x = position.x - video_width;
                                            let relative_y = position.y;

                                            println!("Relative position (subtitle window coords): x={}, y={}", relative_x, relative_y);

                                            let relative_position = Point {
                                                x: relative_x,
                                                y: relative_y,
                                            };

                                            // Use defer to update state after this render cycle
                                            cx.defer(move |cx| {
                                                println!("In deferred callback");
                                                let app_state = cx.global::<AppState>();
                                                let unified_window = app_state.unified_window();

                                                if let Some(window_handle) = unified_window {
                                                    println!("Got unified window handle");
                                                    let update_result = window_handle.update(cx, |any_view, _, app_cx| {
                                                        println!("Inside window.update, attempting downcast");
                                                        match any_view.downcast::<crate::unified_window::UnifiedWindow>() {
                                                            Ok(unified_window) => {
                                                                println!("Successfully downcast to UnifiedWindow");
                                                                let subtitles_entity = unified_window.read(app_cx).subtitles.clone();
                                                                subtitles_entity.update(app_cx, |subtitles, cx| {
                                                                    println!("Setting context menu state with relative position");
                                                                    subtitles.context_menu = Some(ContextMenuState {
                                                                        position: relative_position,
                                                                        subtitle_index: idx,
                                                                    });
                                                                    cx.notify();
                                                                    println!("Context menu state set and notified");
                                                                });
                                                            }
                                                            Err(e) => {
                                                                println!("Failed to downcast to UnifiedWindow: {:?}", e);
                                                            }
                                                        }
                                                    });
                                                    if let Err(e) = update_result {
                                                        println!("Failed to update window handle: {:?}", e);
                                                    }
                                                } else {
                                                    println!("No unified window handle available");
                                                }
                                            });
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
            // Render context menu if active
            .children(self.context_menu.as_ref().map(|menu_state| {
                let subtitle_index = menu_state.subtitle_index;
                let entry = &self.subtitle_entries[subtitle_index];
                let start_ms = entry.start_ms;
                let end_ms = entry.end_ms;

                div()
                    .absolute()
                    .left(menu_state.position.x)
                    .top(menu_state.position.y)
                    .bg(rgb(0x2a2a2a))
                    .border_1()
                    .border_color(rgb(0x404040))
                    .rounded_md()
                    .shadow_lg()
                    .min_w(px(140.0))
                    // Capture mouse events to prevent them from bubbling to parent
                    .on_mouse_down(MouseButton::Left, |_, _, _| {
                        println!("Context menu div clicked (event consumed)");
                        // Consume the event - don't close the menu
                    })
                    .on_mouse_down(MouseButton::Right, |_, _, _| {
                        println!("Context menu div right-clicked (event consumed)");
                        // Consume the event
                    })
                    .child(
                        div()
                            .px_4()
                            .py_2()
                            .cursor_pointer()
                            .text_sm()
                            .text_color(rgb(0xffffff))
                            .hover(|style| style.bg(rgb(0x404040)))
                            // Consume right-click events on the menu item itself
                            .on_mouse_down(MouseButton::Right, |_, _, cx| {
                                println!("Menu item right-clicked (event consumed)");
                                // Consume the event
                                cx.stop_propagation();
                            })
                            .on_mouse_down(MouseButton::Left, move |_event, _, cx| {
                                cx.stop_propagation();
                                println!("Clip block clicked! start_ms={}, end_ms={}", start_ms, end_ms);

                                // Use defer to update state after this render cycle
                                cx.defer(move |cx| {
                                    println!("In deferred callback for clip setting");
                                    // Set clip start and end times in the controls window
                                    let app_state = cx.global::<AppState>();
                                    let unified_window = app_state.unified_window();

                                    if let Some(window_handle) = unified_window {
                                        println!("Got unified window handle for clip setting");
                                        let update_result = window_handle.update(cx, |any_view, _, app_cx| {
                                            match any_view.downcast::<crate::unified_window::UnifiedWindow>() {
                                                Ok(unified_window) => {
                                                    println!("Successfully downcast to UnifiedWindow for clip setting");
                                                    let controls_entity = unified_window.read(app_cx).controls.clone();
                                                    controls_entity.update(app_cx, |controls, cx| {
                                                        println!("Calling set_clip_times with start={}, end={}", start_ms, end_ms);
                                                        controls.set_clip_times(start_ms, end_ms, cx);
                                                        println!("set_clip_times completed");
                                                    });

                                                    // Close the context menu
                                                    let subtitles_entity = unified_window.read(app_cx).subtitles.clone();
                                                    subtitles_entity.update(app_cx, |subtitles, cx| {
                                                        println!("Closing context menu after clip block click");
                                                        subtitles.context_menu = None;
                                                        cx.notify();
                                                    });
                                                }
                                                Err(e) => {
                                                    println!("Failed to downcast for clip setting: {:?}", e);
                                                }
                                            }
                                        });
                                        if let Err(e) = update_result {
                                            println!("Failed to update window for clip setting: {:?}", e);
                                        }
                                    } else {
                                        println!("No unified window handle available for clip setting");
                                    }
                                });
                            })
                            .child("Clip block")
                    )
            }))
            // Click anywhere to close context menu (except on the menu itself)
            .when(self.context_menu.is_some(), |div| {
                div.on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    println!("Closing context menu via background click");
                    this.context_menu = None;
                    cx.notify();
                }))
                .on_mouse_down(MouseButton::Right, cx.listener(|this, _, _, cx| {
                    println!("Closing context menu via right-click outside");
                    this.context_menu = None;
                    cx.notify();
                }))
            })
    }
}
