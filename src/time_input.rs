use gpui::{
    actions, div, prelude::*, rgb, App, Bounds, Context, CursorStyle, Element, ElementId,
    ElementInputHandler, Entity, EntityInputHandler, FocusHandle, Focusable, GlobalElementId,
    LayoutId, Pixels, Point, ShapedLine, SharedString, Style, TextRun, UTF16Selection, Window,
};
use std::ops::Range;

actions!(time_input, [Backspace]);

pub struct TimeInput {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
}

impl TimeInput {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            content: "".into(),
            placeholder: "--:--.---".into(),
            last_layout: None,
            last_bounds: None,
        }
    }

    pub fn content(&self) -> String {
        self.content.to_string()
    }

    pub fn set_content(&mut self, content: String, cx: &mut Context<Self>) {
        self.content = content.into();
        cx.notify();
    }

    /// Parse time string in format MM:SS.mmm to milliseconds
    /// Returns None if invalid format
    pub fn parse_time_ms(&self) -> Option<f32> {
        let text = self.content.to_string();
        if text.is_empty() {
            return None;
        }

        // Expected format: MM:SS.mmm or M:SS.mmm
        let parts: Vec<&str> = text.split(':').collect();
        if parts.len() != 2 {
            return None;
        }

        let minutes: u64 = parts[0].parse().ok()?;

        let seconds_parts: Vec<&str> = parts[1].split('.').collect();
        if seconds_parts.is_empty() || seconds_parts.len() > 2 {
            return None;
        }

        let seconds: u64 = seconds_parts[0].parse().ok()?;
        if seconds >= 60 {
            return None;
        }

        let milliseconds: u64 = if seconds_parts.len() == 2 {
            // Pad or truncate to 3 digits
            let ms_str = seconds_parts[1];
            if ms_str.len() > 3 {
                return None;
            }
            let padded = format!("{:0<3}", ms_str);
            padded.parse().ok()?
        } else {
            0
        };

        let total_ms = (minutes * 60 * 1000) + (seconds * 1000) + milliseconds;
        Some(total_ms as f32)
    }

    /// Check if the current content is valid time format
    pub fn is_valid(&self) -> bool {
        self.content.is_empty() || self.parse_time_ms().is_some()
    }

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        let mut content = self.content.to_string();
        content.pop();
        self.content = content.into();
        cx.notify();
    }
}

impl EntityInputHandler for TimeInput {
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
        let len = self.content.len();
        Some(UTF16Selection {
            range: self.range_to_utf16(&(len..len)),
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
            .unwrap_or(content_len..content_len);

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
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
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.content.len())
    }
}

impl TimeInput {
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

pub struct TimeInputElement {
    input: Entity<TimeInput>,
}

impl TimeInputElement {
    pub fn new(input: Entity<TimeInput>) -> Self {
        Self { input }
    }
}

pub struct PrepaintState {
    line: Option<ShapedLine>,
}

impl IntoElement for TimeInputElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TimeInputElement {
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
        let mut style = Style::default();
        style.size.width = gpui::relative(1.).into();
        style.size.height = window.line_height().into();
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

        let (display_text, text_color) = if content.is_empty() {
            (input.placeholder.clone(), gpui::hsla(0., 0., 1., 0.4))
        } else {
            (content, style.color)
        };

        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &[run], None);

        PrepaintState { line: Some(line) }
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

        let line = prepaint.line.take().unwrap();
        let text_width = line.width;
        line.paint(bounds.origin, window.line_height(), window, cx)
            .unwrap();

        // Paint cursor when focused
        if is_focused {
            let cursor_x = bounds.origin.x + text_width;
            let cursor_top = bounds.origin.y;
            let cursor_bottom = cursor_top + window.line_height();

            window.paint_quad(gpui::quad(
                gpui::Bounds {
                    origin: gpui::point(cursor_x, cursor_top),
                    size: gpui::size(gpui::px(1.5), cursor_bottom - cursor_top),
                },
                gpui::Corners::default(),
                gpui::white(),
                gpui::Edges::default(),
                gpui::white(),
                gpui::BorderStyle::default(),
            ));
        }

        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
        });
    }
}

impl Render for TimeInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_focused = self.focus_handle.is_focused(window);
        let is_valid = self.is_valid();

        div()
            .w_full()
            .px_2()
            .py_1()
            .bg(rgb(0x2a2a2a))
            .border_1()
            .border_color(if !is_valid {
                rgb(0xff4444) // Faint red for invalid
            } else if is_focused {
                rgb(0x4caf50)
            } else {
                rgb(0x444444)
            })
            .rounded_md()
            .text_xs()
            .text_color(rgb(0xffffff))
            .cursor(CursorStyle::IBeam)
            .key_context("TimeInput")
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::backspace))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    window.focus(&this.focus_handle);
                    cx.notify();
                }),
            )
            .child(TimeInputElement::new(cx.entity()))
    }
}

impl Focusable for TimeInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
