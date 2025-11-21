use crate::search_input::SearchInput;
use crate::subtitle_extractor::SubtitleEntry;
use gpui::{div, prelude::*, px, Context, Entity, IntoElement, Render, ScrollHandle, Window};

/// Clip tab for custom subtitle editing
pub struct SubtitleClipTab {
    custom_subtitle_input: Entity<SearchInput>,
    controls: Option<Entity<crate::controls_window::ControlsWindow>>,
    subtitle_entries: Vec<SubtitleEntry>, // Reference to subtitle entries
    scroll_handle: ScrollHandle,         // For scrolling the text box
}

impl SubtitleClipTab {
    pub fn new(cx: &mut Context<Self>) -> Self {
        // Create custom subtitle input with custom placeholder
        let custom_subtitle_input = cx.new(|cx| {
            let mut input = SearchInput::new(cx);
            input.set_placeholder("Enter text");
            input.set_multiline(true);
            input.set_fill_height(false); // Don't fill height - let it grow with content for scrolling
            input
        });

        Self {
            custom_subtitle_input,
            controls: None,
            subtitle_entries: Vec::new(),
            scroll_handle: ScrollHandle::new(),
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
            srt_text.push_str(&format!("{} --> {}\n", entry.format_start_time(), entry.format_end_time()));
            srt_text.push_str(&format!("{}\n", entry.text));
            srt_text.push('\n');
        }

        // Update the text box
        self.custom_subtitle_input.update(cx, |input, cx| {
            input.set_content(srt_text.clone(), cx);
        });
    }

    /// Check if there's a valid clip range and update if needed
    fn check_and_update_clip(&mut self, cx: &mut Context<Self>) {
        if let Some(controls_entity) = &self.controls {
            let controls = controls_entity.read(cx);

            // Get clip times (as f32 milliseconds)
            let start_ms_f32 = controls.clip_start_input
                .read(cx)
                .parse_time_ms()
                .or(controls.clip_start);
            let end_ms_f32 = controls.clip_end_input
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
        // Check and update clip subtitles when rendering
        self.check_and_update_clip(cx);

        div()
            .w_full()
            .flex_1()
            .child(
                div()
                    .id("clip-text-scroll")
                    .w_full()
                    .h(gpui::relative(0.5)) // Half height
                    .overflow_hidden()
                    .track_scroll(&self.scroll_handle)
                    .child(self.custom_subtitle_input.clone())
            )
    }
}
