use crate::checkbox::{Checkbox, CheckboxEvent, CheckboxState};
use crate::input::InputState;
use crate::subtitle_extractor::SubtitleEntry;
use gpui::{div, prelude::*, Context, Entity, IntoElement, Render, ScrollHandle, Window};

/// Clip tab for custom subtitle editing
pub struct SubtitleClipTab {
    custom_subtitle_input: Entity<InputState>,
    controls: Option<Entity<crate::controls_window::ControlsWindow>>,
    subtitle_entries: Vec<SubtitleEntry>, // Reference to subtitle entries
    scroll_handle: ScrollHandle,          // For scrolling the text box
    custom_checkbox: Entity<CheckboxState>, // Checkbox for custom subtitle mode
    last_loaded_content: String,          // Track last loaded content to avoid redundant reloads
}

impl SubtitleClipTab {
    pub fn new(cx: &mut Context<Self>) -> Self {
        // Create custom subtitle input with custom placeholder
        let custom_subtitle_input = cx.new(|cx| {
            let mut input = InputState::new(cx);
            input.set_placeholder("Enter text");
            input.set_multiline(true);
            input.set_fill_height(false); // Don't fill height - let it grow with content for scrolling
            input
        });

        // Create checkbox for custom mode (default off)
        let custom_checkbox = cx.new(|_cx| CheckboxState::new(false));

        // Subscribe to custom subtitle input changes to reload subtitles when in custom mode
        cx.observe(&custom_subtitle_input, |this, _input, cx| {
            // Only reload if custom mode is enabled
            let custom_mode = cx.global::<crate::AppState>().custom_subtitle_mode;
            let display_subtitles = cx.global::<crate::AppState>().display_subtitles;
            let srt_content = this.get_custom_subtitle_srt(cx);

            if custom_mode && display_subtitles && srt_content != this.last_loaded_content {
                if !srt_content.trim().is_empty() {
                    let video_player = cx.global::<crate::AppState>().video_player.clone();
                    let lock_result = video_player.lock();

                    match lock_result {
                        Ok(player) => match player.remove_custom_subtitles() {
                            Ok(_) => match player.add_subtitle_from_text(&srt_content) {
                                Ok(track_id) => {
                                    println!("Custom subtitle reloaded as track {}", track_id);
                                    // Update the last loaded content after successful reload
                                    this.last_loaded_content = srt_content.clone();
                                }
                                Err(e) => {
                                    eprintln!("Failed to reload custom subtitle: {}", e);
                                }
                            },
                            Err(e) => {
                                eprintln!("Failed to remove custom subtitles: {}", e);
                            }
                        },
                        Err(e) => {
                            eprintln!("Failed to lock video player: {}", e);
                        }
                    }
                }
            }
        })
        .detach();

        // Subscribe to checkbox changes to update global state and load/unload custom subtitles
        cx.subscribe(&custom_checkbox, |this, _, event: &CheckboxEvent, cx| {
            if let CheckboxEvent::Change(checked) = event {
                cx.update_global::<crate::AppState, _>(|state, _| {
                    state.custom_subtitle_mode = *checked;
                });

                // Load or unload custom subtitles
                if *checked {
                    // Custom mode enabled - load custom subtitles
                    let srt_content = this.get_custom_subtitle_srt(cx);

                    if !srt_content.trim().is_empty() {
                        let video_player = cx.global::<crate::AppState>().video_player.clone();
                        let lock_result = video_player.lock();

                        match lock_result {
                            Ok(player) => match player.add_subtitle_from_text(&srt_content) {
                                Ok(track_id) => {
                                    println!("Custom subtitle loaded as track {}", track_id);
                                    // Update the last loaded content after successful load
                                    this.last_loaded_content = srt_content.clone();
                                }
                                Err(e) => {
                                    eprintln!("Failed to load custom subtitle: {}", e);
                                }
                            },
                            Err(e) => {
                                eprintln!("Failed to lock video player: {}", e);
                            }
                        }
                    } else {
                        println!("No custom subtitle text to load");
                    }
                } else {
                    // Custom mode disabled - remove custom subtitles
                    let video_player = cx.global::<crate::AppState>().video_player.clone();
                    let selected_track = cx.global::<crate::AppState>().selected_subtitle_track;
                    let lock_result = video_player.lock();

                    match lock_result {
                        Ok(player) => {
                            if let Err(e) = player.remove_custom_subtitles() {
                                eprintln!("Failed to remove custom subtitles: {}", e);
                            }

                            // Re-enable the original subtitle track if one was selected
                            if let Some(track_index) = selected_track {
                                if let Err(e) = player.set_subtitle_track(track_index as i32) {
                                    eprintln!("Failed to restore original subtitle track: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to lock video player: {}", e);
                        }
                    }

                    // Clear last loaded content when disabling custom mode
                    this.last_loaded_content.clear();
                }
            }
        })
        .detach();

        Self {
            custom_subtitle_input,
            controls: None,
            subtitle_entries: Vec::new(),
            scroll_handle: ScrollHandle::new(),
            custom_checkbox,
            last_loaded_content: String::new(),
        }
    }

    /// Set the controls window reference (called by SubtitleWindow)
    pub fn set_controls(&mut self, controls: Entity<crate::controls_window::ControlsWindow>) {
        self.controls = Some(controls);
    }

    /// Get the custom subtitle text
    pub fn get_custom_subtitle_text(&self, cx: &Context<Self>) -> String {
        self.custom_subtitle_input.read(cx).content()
    }

    /// Convert the custom subtitle text to proper SRT format with sequence numbers
    pub fn get_custom_subtitle_srt(&self, cx: &Context<Self>) -> String {
        let content = self.custom_subtitle_input.read(cx).content();

        // Parse the content and add sequence numbers
        let mut srt_output = String::new();
        let mut sequence = 1;

        // Split by double newlines to get subtitle blocks
        let blocks: Vec<&str> = content.split("\n\n").collect();

        for block in blocks {
            let block = block.trim();
            if block.is_empty() {
                continue;
            }

            // Each block should have: timestamp line, then text
            let lines: Vec<&str> = block.lines().collect();
            if lines.is_empty() {
                continue;
            }

            // Check if the first line contains " --> " (timestamp)
            if lines[0].contains(" --> ") {
                // Add sequence number
                srt_output.push_str(&format!("{}\n", sequence));
                sequence += 1;

                // Add the rest of the block
                srt_output.push_str(block);
                srt_output.push_str("\n\n");
            }
        }

        srt_output
    }

    /// Update subtitle entries (called by SubtitleWindow when subtitles change)
    pub fn set_subtitle_entries(&mut self, entries: Vec<SubtitleEntry>) {
        self.subtitle_entries = entries;
    }

    /// Update the text box with subtitles in the given time range
    pub fn update_for_clip_range(&mut self, start_ms: u64, end_ms: u64, cx: &mut Context<Self>) {
        // Find all subtitles that overlap with the clip range
        let mut clip_subtitles = Vec::new();

        for (index, entry) in self.subtitle_entries.iter().enumerate() {
            // Check if subtitle overlaps with clip range
            if entry.start_ms <= end_ms && entry.end_ms >= start_ms {
                clip_subtitles.push((index + 1, entry.clone()));
            }
        }

        // Format without sequence numbers
        let mut srt_text = String::new();
        for (_index, entry) in clip_subtitles {
            srt_text.push_str(&format!(
                "{} --> {}\n",
                entry.format_start_time(),
                entry.format_end_time()
            ));
            srt_text.push_str(&format!("{}\n", entry.text));
            srt_text.push('\n');
        }

        // Update the text box without resetting cursor position
        self.custom_subtitle_input.update(cx, |input, cx| {
            input.set_content_with_cursor(srt_text.clone(), false, cx);
        });
    }

    /// Check if there's a valid clip range and update if needed
    fn check_and_update_clip(&mut self, cx: &mut Context<Self>) {
        if let Some(controls_entity) = &self.controls {
            let controls = controls_entity.read(cx);

            // Get clip times (as f32 milliseconds)
            let start_ms_f32 = controls
                .clip_start_input
                .read(cx)
                .parse_time_ms()
                .or(controls.clip_start);
            let end_ms_f32 = controls
                .clip_end_input
                .read(cx)
                .parse_time_ms()
                .or(controls.clip_end);

            if let (Some(start), Some(end)) = (start_ms_f32, end_ms_f32) {
                if start < end {
                    // Convert f32 to u64
                    let start_u64 = start as u64;
                    let end_u64 = end as u64;
                    self.update_for_clip_range(start_u64, end_u64, cx);
                }
            }
        }
    }
}

impl Render for SubtitleClipTab {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // keep the clip subtitles aligned with the real ones unless the custom checkbox is checked
        if !self.custom_checkbox.read(cx).is_checked() {
            self.check_and_update_clip(cx);
        }

        div()
            .w_full()
            .flex_1()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                // Checkbox above the text input
                div()
                    .flex()
                    .items_center()
                    .px_2()
                    .py_1()
                    .child(Checkbox::new(&self.custom_checkbox).label("Custom")),
            )
            .child(
                div()
                    .id("clip-text-scroll")
                    .w_full()
                    .flex_1()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll_handle)
                    .child(self.custom_subtitle_input.clone()),
            )
    }
}
