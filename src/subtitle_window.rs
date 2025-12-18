use crate::theme::OneDarkExt;
use gpui::{
    div, prelude::*, px, size, Context, Entity, IntoElement, MouseButton, Pixels, Render,
    ScrollStrategy, Size, Window,
};
use gpui_component::ActiveTheme;
use gpui_component::{v_virtual_list, VirtualListScrollHandle};
use std::rc::Rc;

use gpui_component::{
    checkbox::Checkbox,
    input::{Input, InputState},
    menu::{ContextMenuExt, PopupMenuItem},
    select::{Select, SelectEvent, SelectItem, SelectState},
    IndexPath,
};

use crate::subtitle_clip_tab::SubtitleClipTab;
use crate::subtitle_detector::SubtitleStream;
use crate::subtitle_extractor::SubtitleEntry;
use crate::video_player::ClockTime;
use crate::AppState;

/// Active tab in the subtitle window
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SubtitleTab {
    Video, // Default view with sync, dropdown, search
    Clip,  // Custom subtitle editor for clips
}

// Implement SelectItem for SubtitleStream
impl SelectItem for SubtitleStream {
    type Value = Self;

    fn title(&self) -> gpui::SharedString {
        self.display_title.clone().into()
    }

    fn value(&self) -> &Self::Value {
        self
    }
}

/// Subtitle window with stream selection and SRT display
pub struct SubtitleWindow {
    select_state: Entity<SelectState<Vec<SubtitleStream>>>,
    sync_enabled: bool, // Whether subtitles are synced to video
    search_input: Entity<InputState>,
    pub subtitle_entries: Vec<SubtitleEntry>,
    current_position: f32,                 // Current video position in seconds
    current_subtitle_index: Option<usize>, // Index of the currently active subtitle (from video position)
    scroll_handle: VirtualListScrollHandle,
    search_result_indices: Vec<usize>, // All indices that match the search
    current_search_result_index: Option<usize>, // Index within search_result_indices of the current result
    last_scrolled_to_search: Option<usize>, // Last search result we scrolled to (to avoid re-scrolling)
    last_scrolled_to_video: Option<usize>,  // Last video position we scrolled to
    last_submitted_search_term: Option<String>, // Last search term submitted via Enter (to distinguish NEW vs SAME searches)
    active_tab: SubtitleTab,                    // Currently active tab
    clip_tab: Entity<SubtitleClipTab>,          // Clip tab component
    controls: Option<Entity<crate::controls_window::ControlsWindow>>, // Reference to controls window to check clip state
    right_clicked_item: Option<usize>, // Index of the right-clicked subtitle item
}

// Data structure to hold loaded subtitle information
#[derive(Clone)]
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
    pub fn update_with_loaded_data(
        &mut self,
        window: &mut Window,
        data: SubtitleData,
        cx: &mut Context<Self>,
    ) {
        // Recreate select state with new streams
        // This ensures the Select component properly reflects the new data
        let new_select_state = cx.new(|cx| {
            let selected_index = if !data.streams.is_empty() {
                Some(IndexPath::new(0))
            } else {
                None
            };
            SelectState::new(data.streams.clone(), selected_index, window, cx)
        });

        // Subscribe to select events for the new state
        cx.subscribe(
            &new_select_state,
            |this, _state_entity, event: &SelectEvent<Vec<SubtitleStream>>, cx| {
                if let SelectEvent::Confirm(Some(_selected_stream)) = event {
                    // Get the selected index from the SelectState
                    if let Some(index_path) = this.select_state.read(cx).selected_index(cx) {
                        let index = index_path.row;
                        this.load_subtitle_stream(index, cx);

                        // Update AppState with the selected subtitle track
                        cx.update_global::<AppState, _>(|state, _| {
                            state.selected_subtitle_track = Some(index + 1);
                        });

                        // If subtitle display is enabled in controls, update the video player
                        let (display_subtitles, video_player) = {
                            let app_state = cx.global::<AppState>();
                            (app_state.display_subtitles, app_state.video_player.clone())
                        };

                        if display_subtitles {
                            if let Ok(player) = video_player.lock() {
                                if let Err(e) = player.set_subtitle_track((index + 1) as i32) {
                                    eprintln!("Failed to set subtitle track: {}", e);
                                }
                            }
                        }
                    }
                }
            },
        )
        .detach();

        self.select_state = new_select_state;

        // Set the subtitle entries
        self.subtitle_entries = data.first_stream_entries.clone();

        // Update clip tab with new subtitle entries
        self.clip_tab.update(cx, |clip_tab, _cx| {
            clip_tab.set_subtitle_entries(data.first_stream_entries);
        });

        cx.notify();
    }

    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Create select state with empty items initially
        let select_state =
            cx.new(|cx| SelectState::new(Vec::<SubtitleStream>::new(), None, window, cx));

        // Subscribe to select events to load the selected subtitle stream
        cx.subscribe(
            &select_state,
            |this, _state_entity, event: &SelectEvent<Vec<SubtitleStream>>, cx| {
                if let SelectEvent::Confirm(Some(_selected_stream)) = event {
                    // Get the selected index from the SelectState
                    if let Some(index_path) = this.select_state.read(cx).selected_index(cx) {
                        let index = index_path.row;
                        this.load_subtitle_stream(index, cx);

                        // Update AppState with the selected subtitle track
                        cx.update_global::<AppState, _>(|state, _| {
                            state.selected_subtitle_track = Some(index + 1);
                        });

                        // If subtitle display is enabled in controls, update the video player
                        let app_state = cx.global::<AppState>();
                        if app_state.display_subtitles {
                            let video_player = app_state.video_player.clone();

                            if let Ok(player) = video_player.lock() {
                                if let Err(e) = player.set_subtitle_track((index + 1) as i32) {
                                    eprintln!("Failed to set subtitle track: {}", e);
                                }
                            } else {
                                eprintln!("Failed to lock video player for subtitle track change");
                            };
                        }
                    }
                }
            },
        )
        .detach();

        // Create search input
        let search_input = cx.new(|cx| InputState::new(window, cx));

        // Create clip tab component
        let clip_tab = cx.new(|cx| SubtitleClipTab::new(window, cx));

        Self {
            select_state,
            sync_enabled: true, // Default to synced to video
            search_input,
            subtitle_entries: Vec::new(),
            current_position: 0.0,
            current_subtitle_index: None,
            scroll_handle: VirtualListScrollHandle::new(),
            search_result_indices: Vec::new(),
            current_search_result_index: None,
            last_scrolled_to_search: None,
            last_scrolled_to_video: None,
            last_submitted_search_term: None,
            active_tab: SubtitleTab::Video, // Default to Video tab
            clip_tab,
            controls: None, // Will be set by UnifiedWindow after creation
            right_clicked_item: None,
        }
    }

    /// Handle sync checkbox toggle
    fn toggle_sync(&mut self, checked: bool, cx: &mut Context<Self>) {
        self.sync_enabled = checked;

        // Update AppState
        cx.update_global::<AppState, _>(|state, _| {
            state.synced_to_video = checked;
        });

        // When turning ON sync to video, clear all search state
        if checked {
            self.search_result_indices.clear();
            self.current_search_result_index = None;
            self.last_submitted_search_term = None;
            self.last_scrolled_to_search = None;
        }

        cx.notify();
    }

    /// Set the controls window reference (called by UnifiedWindow)
    pub fn set_controls(
        &mut self,
        controls: Entity<crate::controls_window::ControlsWindow>,
        cx: &mut Context<Self>,
    ) {
        self.controls = Some(controls.clone());

        // Also set controls on the clip tab
        self.clip_tab.update(cx, |clip_tab, _cx| {
            clip_tab.set_controls(controls);
        });
    }

    /// Load subtitle streams for the current video file
    pub fn load_subtitle_streams(
        &mut self,
        window: &mut Window,
        file_path: &str,
        cx: &mut Context<Self>,
    ) {
        // Detect subtitle streams
        let streams = crate::subtitle_detector::detect_subtitle_streams(file_path);

        if streams.is_empty() {
            println!("No text-based subtitle streams found");
            return;
        }

        println!("Found {} subtitle stream(s)", streams.len());

        // Update select state with streams
        self.select_state.update(cx, |state, cx| {
            state.set_items(streams.clone(), window, cx);
            // Select the first stream by default
            if !streams.is_empty() {
                state.set_selected_index(Some(IndexPath::new(0)), window, cx);
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
        let search_text = self.search_input.read(cx).text().to_string();

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
            self.toggle_sync(false, cx);
        }

        cx.notify();
    }

    /// Move to the next search result (cycling/wrapping)
    fn search_next(&mut self, cx: &mut Context<Self>) {
        // If we don't have any results, do nothing
        if self.search_result_indices.is_empty() {
            return;
        }

        // Move to next result (wrap to 0 if at end)
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
        let current_search_text = self.search_input.read(cx).text().to_string();

        // If search text is empty, do nothing
        if current_search_text.is_empty() {
            return;
        }

        // Compare with last submitted search term
        if self.last_submitted_search_term.as_ref() != Some(&current_search_text) {
            // NEW search term
            self.last_submitted_search_term = Some(current_search_text.clone());

            // Turn OFF sync to video
            self.toggle_sync(false, cx);

            // Perform search and jump to first result (index 0)
            self.update_search_results(cx);
        } else {
            // SAME search term - increment to next result
            self.search_next(cx);
        }
    }

    /// Handle Escape key in search input
    fn on_search_escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Clear search and results
        self.search_input.update(cx, |input, cx| {
            input.set_value("".to_string(), window, cx);
        });
        self.search_result_indices.clear();
        self.current_search_result_index = None;
        self.last_scrolled_to_search = None;
        self.last_submitted_search_term = None;
        cx.notify();
    }

    /// Check if the Clip tab should be enabled (when there's a valid clip)
    fn is_clip_tab_enabled(&self, cx: &Context<Self>) -> bool {
        if let Some(controls_entity) = &self.controls {
            let controls = controls_entity.read(cx);

            // Inline the clip validation logic
            let start_ms = controls
                .clip_start_input
                .read(cx)
                .parse_time_ms()
                .or(controls.clip_start);
            let end_ms = controls
                .clip_end_input
                .read(cx)
                .parse_time_ms()
                .or(controls.clip_end);

            return start_ms.is_some() && end_ms.is_some() && start_ms.unwrap() < end_ms.unwrap();
        }
        false
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

                // Update clip tab with new subtitle entries
                let entries_clone = self.subtitle_entries.clone();
                self.clip_tab.update(cx, |clip_tab, _cx| {
                    clip_tab.set_subtitle_entries(entries_clone);
                });

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

        // Calculate item sizes for virtual list (fixed height of 60px for each item)
        let item_sizes: Rc<Vec<Size<Pixels>>> = Rc::new(
            (0..item_count)
                .map(|_| size(px(0.0), px(60.0))) // width will be measured, height is 60px
                .collect(),
        );

        // Get the view entity before entering the div builder
        let view = cx.entity().clone();

        // Check if Clip tab should be enabled
        let clip_tab_enabled = self.is_clip_tab_enabled(cx);
        let active_tab = self.active_tab;

        let theme = cx.theme();
        // Pre-capture colors for closures
        let surface_bg = theme.surface_background();
        let border_variant_color = theme.border_variant();
        let element_active_bg = theme.element_active();
        let element_bg = theme.element_background();
        let element_hover_bg = theme.element_hover();
        let text_color = theme.text();
        let text_muted_color = theme.text_muted();
        let text_disabled_color = theme.text_disabled();
        let ring_color = theme.ring(); // For active tab borders and search highlights
        let list_active_bg = theme.list_active_background(); // For current subtitle highlight
        let info_bg = theme.info();

        div()
            .flex()
            .flex_col()
            .bg(surface_bg)
            .size_full()
            .p_4()
            .gap_4()
            .relative() // Add relative positioning for absolute children
            // Tab bar
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_1()
                    .border_b_1()
                    .border_color(border_variant_color)
                    .pb_2()
                    .child(
                        // Video tab
                        div()
                            .px_4()
                            .py_2()
                            .rounded_t_md()
                            .cursor_pointer()
                            .text_sm()
                            .when(active_tab == SubtitleTab::Video, |div| {
                                div.bg(element_active_bg)
                                    .text_color(text_color)
                                    .border_b_2()
                                    .border_color(ring_color)
                            })
                            .when(active_tab != SubtitleTab::Video, |div| {
                                div.bg(element_bg)
                                    .text_color(text_muted_color)
                                    .hover(move |style| style.bg(element_hover_bg))
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.active_tab = SubtitleTab::Video;
                                    cx.notify();
                                }),
                            )
                            .child("Video")
                    )
                    .child(
                        // Clip tab
                        div()
                            .px_4()
                            .py_2()
                            .rounded_t_md()
                            .text_sm()
                            .when(clip_tab_enabled, |div| div.cursor_pointer())
                            .when(!clip_tab_enabled, |div| div.cursor_not_allowed())
                            .when(active_tab == SubtitleTab::Clip, |div| {
                                div.bg(element_active_bg)
                                    .text_color(text_color)
                                    .border_b_2()
                                    .border_color(ring_color)
                            })
                            .when(active_tab != SubtitleTab::Clip && clip_tab_enabled, |div| {
                                div.bg(element_bg)
                                    .text_color(text_muted_color)
                                    .hover(move |style| style.bg(element_hover_bg))
                            })
                            .when(!clip_tab_enabled, |div| {
                                div.bg(element_bg)
                                    .text_color(text_disabled_color)
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, _, cx| {
                                    if clip_tab_enabled {
                                        this.active_tab = SubtitleTab::Clip;
                                        cx.notify();
                                    }
                                }),
                            )
                            .child("Clip"),
                    ),
            )
            // Video tab content
            .when(active_tab == SubtitleTab::Video, |parent| {
                let sync_enabled = self.sync_enabled;

                parent.child(
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
                                Checkbox::new("sync-to-video-checkbox")
                                    .label("Sync")
                                    .checked(sync_enabled)
                                    .on_click(cx.listener(|this, checked, _, cx| {
                                        this.toggle_sync(*checked, cx);
                                    })),
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
                            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                                match event.keystroke.key.as_str() {
                                    "enter" => {
                                        this.on_search_enter(cx);
                                    }
                                    "escape" => {
                                        this.on_search_escape(window, cx);
                                    }
                                    _ => {}
                                }
                            }))
                            .child(Input::new(&self.search_input)),
                    ),
            )
            .child(
                // Virtual list for displaying subtitles
                // Use dynamic ID so VirtualList gets recreated when data changes
                div().id("subtitle-list-container").flex_1().w_full()
                .context_menu({
                    let view_for_menu = view.clone();
                    move |menu, _window, cx| {
                        // Context menu for the right-clicked subtitle item
                        // Get the right-clicked item info
                        let menu_data = view_for_menu.read(cx).right_clicked_item.and_then(|idx| {
                            view_for_menu.read(cx).subtitle_entries.get(idx).map(|entry| {
                                (entry.start_ms, entry.end_ms)
                            })
                        });

                        if let Some((start_ms, end_ms)) = menu_data {
                            menu.item(
                                PopupMenuItem::new("Set clip start").on_click(move |_, _, cx| {
                                    eprintln!("=== SET CLIP START CLICKED! time_ms={} ===", start_ms);
                                    let app_state = cx.global::<AppState>();
                                    let unified_window_entity = app_state.unified_window_entity.clone();

                                    if let Some(unified_window_entity) = unified_window_entity {
                                        unified_window_entity.update(cx, |unified_window, app_cx| {
                                            let controls_entity = unified_window.controls.clone();
                                            controls_entity.update(app_cx, |controls, cx| {
                                                controls.set_clip_start(start_ms, cx);
                                            });
                                        });
                                    }
                                })
                            ).item(
                                PopupMenuItem::new("Set clip end").on_click(move |_, _, cx| {
                                    eprintln!("=== SET CLIP END CLICKED! time_ms={} ===", end_ms);
                                    let app_state = cx.global::<AppState>();
                                    let unified_window_entity = app_state.unified_window_entity.clone();

                                    if let Some(unified_window_entity) = unified_window_entity {
                                        unified_window_entity.update(cx, |unified_window, app_cx| {
                                            let controls_entity = unified_window.controls.clone();
                                            controls_entity.update(app_cx, |controls, cx| {
                                                controls.set_clip_end(end_ms, cx);
                                            });
                                        });
                                    }
                                })
                            ).item(
                                PopupMenuItem::new("Clip block").on_click(move |_, _, cx| {
                                    eprintln!("=== CLIP BLOCK CLICKED! start_ms={}, end_ms={} ===", start_ms, end_ms);
                                    let app_state = cx.global::<AppState>();
                                    let unified_window_entity = app_state.unified_window_entity.clone();

                                    if let Some(unified_window_entity) = unified_window_entity {
                                        unified_window_entity.update(cx, |unified_window, app_cx| {
                                            let controls_entity = unified_window.controls.clone();
                                            controls_entity.update(app_cx, |controls, cx| {
                                                controls.set_clip_times(start_ms, end_ms, cx);
                                            });
                                        });
                                    }
                                })
                            )
                        } else {
                            menu
                        }
                    }
                })
                .child({
                    let view_for_list = view.clone();
                    v_virtual_list(
                        view,
                        format!("subtitle-list-{}", item_count),
                        item_sizes,
                        move |_view, range, _window, _cx| {
                            range
                                .filter_map(|idx| {
                                entries.get(idx).map(|entry| {
                                    let is_current_video_subtitle =
                                        current_subtitle_index == Some(idx);
                                    let is_search_result = search_result_indices.contains(&idx);
                                    let is_active_search_result =
                                        current_search_subtitle_idx == Some(idx);
                                    let start_ms = entry.start_ms;
                                    let end_ms = entry.end_ms;

                                    div()
                                        .w_full()
                                        .h(px(60.0))
                                        .px_3()
                                        .py_2()
                                        .border_b_1()
                                        .border_color(border_variant_color)
                                        .cursor_pointer()
                                        // Prioritize active search result > current video subtitle > regular search result
                                        .when(is_active_search_result, |div| {
                                            div.bg(ring_color) // Theme ring color for active search result
                                        })
                                        .when(is_search_result && !is_active_search_result, |div| {
                                            div.bg(element_active_bg) // Secondary active for other search results
                                        })
                                        .when(
                                            is_current_video_subtitle
                                                && !is_active_search_result
                                                && !is_search_result,
                                            |div| {
                                                div.bg(list_active_bg) // Theme list active for current video subtitle
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
                                        } )
                                        .on_mouse_down(MouseButton::Right, {
                                            let view_clone = view_for_list.clone();
                                            move |_, _, cx| {
                                                // Track which item was right-clicked
                                                view_clone.update(cx, |this, cx| {
                                                    this.right_clicked_item = Some(idx);
                                                    cx.notify();
                                                });
                                            }
                                        })
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap_1()
                                                .child(
                                                    div()
                                                        .flex()
                                                        .flex_row()
                                                        .gap_1()
                                                        .text_xs()
                                                        .text_color(text_muted_color)
                                                        .child(
                                                            div()
                                                                .px_1()
                                                                .rounded(px(3.0))
                                                                .text_color(text_muted_color)
                                                                .child(entry.format_start_time())
                                                        )
                                                        .child(" --> ")
                                                        .child(
                                                            div()
                                                                .px_1()
                                                                .rounded(px(3.0))
                                                                .text_color(text_muted_color)
                                                                .child(entry.format_end_time())
                                                        )
                                                )
                                                .child(
                                                    div()
                                                        .text_sm()
                                                        .text_color(text_color)
                                                        .child(entry.text.clone()),
                                                ),
                                        )
                                })
                            })
                            .collect::<Vec<_>>()
                        },
                    )
                    .track_scroll(&self.scroll_handle)
                    .w_full()
                    .h_full()
                })
            )
            })
            // Clip tab content
            .when(active_tab == SubtitleTab::Clip, |parent| {
                parent.child(self.clip_tab.clone())
            })
    }
}
