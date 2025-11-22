use crate::theme::OneDarkTheme;
use gpui::{
    actions, div, prelude::*, px, App, Bounds, Context, CursorStyle, Element, ElementId,
    ElementInputHandler, Entity, EntityInputHandler, FocusHandle, Focusable, GlobalElementId,
    LayoutId, Pixels, Point, ShapedLine, SharedString, Style, TextRun, UTF16Selection, Window,
};
use std::ops::Range;

actions!(search_input, [Backspace, Enter, Escape]);

pub struct SearchInput {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    last_lines: Vec<ShapedLine>, // Store all shaped lines for cursor positioning
    cursor_position: usize,       // Current cursor position (UTF-8 offset)
    fill_height: bool,            // Whether to fill the parent's height
    multiline: bool,              // Whether to support multiple lines (Enter inserts newline)
}

impl SearchInput {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: "".into(),
            placeholder: "Search subtitles...".into(),
            last_layout: None,
            last_bounds: None,
            last_lines: Vec::new(),
            cursor_position: 0,
            fill_height: false,
            multiline: false,
        }
    }

    pub fn content(&self) -> String {
        self.content.to_string()
    }

    pub fn set_placeholder(&mut self, placeholder: impl Into<SharedString>) {
        self.placeholder = placeholder.into();
    }

    pub fn set_fill_height(&mut self, fill_height: bool) {
        self.fill_height = fill_height;
    }

    pub fn set_multiline(&mut self, multiline: bool) {
        self.multiline = multiline;
    }

    pub fn set_content(&mut self, content: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.set_content_with_cursor(content, true, cx);
    }

    pub fn set_content_with_cursor(&mut self, content: impl Into<SharedString>, reset_cursor: bool, cx: &mut Context<Self>) {
        self.content = content.into();
        if reset_cursor {
            // Move cursor to end when content is set
            self.cursor_position = self.content.len();
        } else {
            // Clamp cursor position to content length
            self.cursor_position = self.cursor_position.min(self.content.len());
        }
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.content = "".into();
        self.cursor_position = 0;
        cx.notify();
    }

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        if self.cursor_position > 0 {
            let content = self.content.to_string();

            // Find the start of the previous character
            let mut prev_char_start = self.cursor_position - 1;
            while prev_char_start > 0 && !content.is_char_boundary(prev_char_start) {
                prev_char_start -= 1;
            }

            // Remove the character and update content
            let new_content = content[..prev_char_start].to_string() + &content[self.cursor_position..];
            self.cursor_position = prev_char_start;
            self.content = new_content.into();
            cx.notify();
        }
    }

    fn insert_newline(&mut self, _: &Enter, _: &mut Window, cx: &mut Context<Self>) {
        if self.multiline {
            let mut content = self.content.to_string();
            content.insert(self.cursor_position, '\n');
            self.cursor_position += 1;
            self.content = content.into();
            cx.notify();
        }
    }
}

impl EntityInputHandler for SearchInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&(self.cursor_position..self.cursor_position)),
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        None
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let content_len = self.content.len();
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .unwrap_or(self.cursor_position..self.cursor_position);

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();

        // Update cursor position to end of inserted text
        self.cursor_position = range.start + new_text.len();
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range_utf16: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_text_in_range(range_utf16, new_text, window, cx);
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(bounds)
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let bounds = self.last_bounds?;
        let lines = &self.last_lines;

        if lines.is_empty() {
            self.cursor_position = 0;
            return Some(0);
        }

        let line_height = window.line_height();

        // Determine which line was clicked
        let relative_y = point.y - bounds.origin.y;
        let line_index = ((relative_y / line_height).floor() as usize).min(lines.len() - 1);

        // Calculate byte offset up to the start of this line
        let mut byte_offset = 0;
        let content_str = self.content.to_string();
        let mut current_line = 0;

        for ch in content_str.chars() {
            if current_line >= line_index {
                // We're on the target line, now find the character
                break;
            }
            if ch == '\n' {
                current_line += 1;
            }
            byte_offset += ch.len_utf8();
        }

        // Now find the character within the line
        let shaped_line = &lines[line_index];
        let relative_x = point.x - bounds.origin.x;

        // Find the closest character in this line (returns byte index within the line)
        let line_byte_index = shaped_line.index_for_x(relative_x).unwrap_or(0);

        // Add the line's byte offset to get the total byte offset in the content
        let final_offset = byte_offset + line_byte_index;
        let final_offset = final_offset.min(self.content.len());

        self.cursor_position = final_offset;
        Some(self.offset_to_utf16(final_offset))
    }
}

impl SearchInput {
    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }

        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;

        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }
}

pub struct SearchInputElement {
    input: Entity<SearchInput>,
}

impl SearchInputElement {
    pub fn new(input: Entity<SearchInput>) -> Self {
        Self { input }
    }
}

pub struct PrepaintState {
    lines: Vec<ShapedLine>,
}

impl IntoElement for SearchInputElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SearchInputElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let input = self.input.read(cx);
        let mut style = Style::default();
        style.size.width = gpui::relative(1.).into();

        // Calculate height based on multiline support
        if input.multiline && input.fill_height {
            // Fill parent height when both multiline and fill_height are true
            style.size.height = gpui::relative(1.).into();
        } else if input.multiline {
            // Calculate height based on number of lines
            let line_count = input.content.chars().filter(|&c| c == '\n').count() + 1;
            let line_count = line_count.max(1); // At least one line
            style.size.height = (window.line_height() * line_count as f32).into();
        } else {
            // Single line height for non-multiline inputs
            style.size.height = window.line_height().into();
        }

        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = input.content.clone();
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());

        let (display_text, text_color) = if content.is_empty() {
            (input.placeholder.clone(), OneDarkTheme::text_placeholder())
        } else {
            (content, OneDarkTheme::text())
        };

        let mut lines = Vec::new();

        if input.multiline {
            // Split by newlines and shape each line separately
            let text_lines: Vec<String> = display_text.split('\n').map(|s| s.to_string()).collect();
            for text_line in text_lines {
                let run = TextRun {
                    len: text_line.len(),
                    font: style.font(),
                    color: text_color.into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };

                let shaped_line = window
                    .text_system()
                    .shape_line(text_line.into(), font_size, &[run], None);
                lines.push(shaped_line);
            }
        } else {
            // Single line
            let run = TextRun {
                len: display_text.len(),
                font: style.font(),
                color: text_color.into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };

            let line = window
                .text_system()
                .shape_line(display_text, font_size, &[run], None);
            lines.push(line);
        }

        PrepaintState { lines }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        let is_focused = focus_handle.is_focused(window);

        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );

        let line_height = window.line_height();
        let mut y_offset = bounds.origin.y;

        // Paint each line at the correct vertical position
        for line in &prepaint.lines {
            let origin = gpui::point(bounds.origin.x, y_offset);
            line.paint(origin, line_height, window, cx).unwrap();
            y_offset += line_height;
        }

        // Store the last line for compatibility (if needed)
        if let Some(last_line) = prepaint.lines.last() {
            self.input.update(cx, |input, _cx| {
                input.last_layout = Some(last_line.clone());
                input.last_bounds = Some(bounds);
                input.last_lines = prepaint.lines.clone();
            });
        }

        // Paint cursor if focused
        if is_focused {
            let cursor_position = self.input.read(cx).cursor_position;
            let content = self.input.read(cx).content.to_string();

            // Find which line the cursor is on and the byte offset within that line
            let mut current_byte = 0;
            let mut current_line = 0;
            let mut byte_offset_in_line = 0;

            for ch in content.chars() {
                if current_byte >= cursor_position {
                    break;
                }
                if ch == '\n' {
                    current_line += 1;
                    byte_offset_in_line = 0;
                } else {
                    byte_offset_in_line += ch.len_utf8();
                }
                current_byte += ch.len_utf8();
            }

            // Get the x position within the line
            if current_line < prepaint.lines.len() {
                let shaped_line = &prepaint.lines[current_line];
                let x_pos = shaped_line.x_for_index(byte_offset_in_line);
                let cursor_x = bounds.origin.x + x_pos;
                let cursor_y = bounds.origin.y + (current_line as f32 * line_height);

                // Draw cursor as a vertical line
                use gpui::{fill, point, size};
                let cursor_bounds = gpui::Bounds {
                    origin: point(cursor_x, cursor_y),
                    size: size(px(2.0), line_height),
                };

                window.paint_quad(fill(cursor_bounds, OneDarkTheme::text()));
            }
        }
    }
}

impl Render for SearchInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_focused = self.focus_handle.is_focused(window);
        let fill_height = self.fill_height;
        let multiline = self.multiline;

        div()
            .id("search-input")
            .w_full()
            .when(fill_height, |div| div.h_full())
            .px_3()
            .py_2()
            .bg(OneDarkTheme::element_background())
            .border_1()
            .border_color(if is_focused {
                OneDarkTheme::border_focused()
            } else {
                OneDarkTheme::border()
            })
            .rounded_md()
            .text_sm()
            .text_color(OneDarkTheme::text())
            .cursor(CursorStyle::IBeam)
            .key_context("SearchInput")
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::backspace))
            .when(multiline, |div| {
                div.on_action(cx.listener(Self::insert_newline))
            })
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    window.focus(&this.focus_handle);
                    cx.notify();
                }),
            )
            .child(SearchInputElement::new(cx.entity()))
    }
}

impl Focusable for SearchInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
