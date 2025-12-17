use crate::subtitle_extractor::SubtitleEntry;
use gpui::{div, prelude::*, Context, Entity, IntoElement, Render, ScrollHandle, Window};
use gpui_component::{
    checkbox::Checkbox,
    input::{Input, InputState},
};

/// Clip tab for custom subtitle editing
pub struct SubtitleClipTab {
    custom_subtitle_input: Entity<InputState>,
    controls: Option<Entity<crate::controls_window::ControlsWindow>>,
    subtitle_entries: Vec<SubtitleEntry>, // Reference to subtitle entries
    scroll_handle: ScrollHandle,          // For scrolling the text box
    custom_mode_enabled: bool,            // Track custom subtitle mode state
    last_loaded_content: String,          // Track last loaded content to avoid redundant reloads
}

impl SubtitleClipTab {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Create custom subtitle input as multi-line for SRT content
        let custom_subtitle_input = cx.new(|cx| InputState::new(window, cx).multi_line(true));

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

        Self {
            custom_subtitle_input,
            controls: None,
            subtitle_entries: Vec::new(),
            scroll_handle: ScrollHandle::new(),
            custom_mode_enabled: false, // Start with custom mode disabled
            last_loaded_content: String::new(),
        }
    }

    /// Handle checkbox toggle for custom subtitle mode
    fn toggle_custom_mode(&mut self, checked: bool, cx: &mut Context<Self>) {
        self.custom_mode_enabled = checked;

        cx.update_global::<crate::AppState, _>(|state, _| {
            state.custom_subtitle_mode = checked;
        });

        // Load or unload custom subtitles
        if checked {
            // Custom mode enabled - load custom subtitles
            let srt_content = self.get_custom_subtitle_srt(cx);

            if !srt_content.trim().is_empty() {
                let video_player = cx.global::<crate::AppState>().video_player.clone();
                let lock_result = video_player.lock();

                match lock_result {
                    Ok(player) => match player.add_subtitle_from_text(&srt_content) {
                        Ok(track_id) => {
                            println!("Custom subtitle loaded as track {}", track_id);
                            // Update the last loaded content after successful load
                            self.last_loaded_content = srt_content.clone();
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
            self.last_loaded_content.clear();
        }

        cx.notify();
    }

    /// Set the controls window reference (called by SubtitleWindow)
    pub fn set_controls(&mut self, controls: Entity<crate::controls_window::ControlsWindow>) {
        self.controls = Some(controls);
    }

    /// Get the custom subtitle text
    pub fn get_custom_subtitle_text(&self, cx: &Context<Self>) -> String {
        self.custom_subtitle_input.read(cx).text().to_string()
    }

    /// Convert the custom subtitle text to proper SRT format with sequence numbers
    pub fn get_custom_subtitle_srt(&self, cx: &Context<Self>) -> String {
        let content = self.custom_subtitle_input.read(cx).text().to_string();

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
    pub fn update_for_clip_range(
        &mut self,
        start_ms: u64,
        end_ms: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

        // Update the text box
        self.custom_subtitle_input.update(cx, |input, cx| {
            input.set_value(srt_text, window, cx);
        });
    }

    /// Check if there's a valid clip range and update if needed
    fn check_and_update_clip(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
                    self.update_for_clip_range(start_u64, end_u64, window, cx);
                }
            }
        }
    }
}

impl Render for SubtitleClipTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // keep the clip subtitles aligned with the real ones unless the custom checkbox is checked
        if !self.custom_mode_enabled {
            self.check_and_update_clip(window, cx);
        }

        let custom_mode_enabled = self.custom_mode_enabled;

        div()
            .w_full()
            .flex_1()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                // Checkbox above the text input
                div().flex().items_center().px_2().py_1().child(
                    Checkbox::new("custom-mode-checkbox")
                        .label("Custom")
                        .checked(custom_mode_enabled)
                        .on_click(cx.listener(|this, checked, _, cx| {
                            this.toggle_custom_mode(*checked, cx);
                        })),
                ),
            )
            .child(
                div()
                    .id("clip-text-scroll")
                    .w_full()
                    .h_full()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll_handle)
                    .child(
                        div()
                            .w_full()
                            .h_full()
                            .child(Input::new(&self.custom_subtitle_input).h_full()),
                    ),
            )
    }
}
