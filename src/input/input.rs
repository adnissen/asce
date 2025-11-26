use gpui::{
    div, fill, point, px, relative, size, App, Bounds, Context, CursorStyle, Element, ElementId,
    ElementInputHandler, Entity, GlobalElementId, IntoElement, InteractiveElement,
    ParentElement, Pixels, Render, ShapedLine, Styled, TextRun, Window,
};
use gpui::prelude::FluentBuilder;

use crate::theme::OneDarkTheme;

use super::{rope_ext::RopeExt, state::InputState};

pub struct Input {
    state: Entity<InputState>,
}

impl Input {
    pub fn new(state: Entity<InputState>) -> Self {
        Self { state }
    }
}

pub struct PrepaintState {
    lines: Vec<ShapedLine>,
    bounds: Bounds<Pixels>,
}

impl IntoElement for Input {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Input {
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
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        let state = self.state.read(cx);
        let mut style = gpui::Style::default();
        style.size.width = relative(1.).into();

        // Calculate height based on mode
        if state.mode.is_multi_line() {
            let line_count = state.text.lines_len();
            let rows = state.mode.rows().max(line_count);
            style.size.height = (window.line_height() * rows as f32).into();
        } else {
            style.size.height = window.line_height().into();
        }

        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let state = self.state.read(cx);
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());

        let (display_text, text_color) = if state.text.len_bytes() == 0 {
            (
                state.placeholder.clone(),
                OneDarkTheme::text_placeholder(),
            )
        } else {
            (state.text.to_string().into(), OneDarkTheme::text())
        };

        let mut lines = Vec::new();

        if state.mode.is_multi_line() {
            // Split by newlines and shape each line
            let text_lines: Vec<String> = display_text.split('\n').map(|s| s.to_string()).collect();
            for line_text in text_lines {
                let run = TextRun {
                    len: line_text.len(),
                    font: style.font(),
                    color: text_color.into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };

                let shaped_line = window
                    .text_system()
                    .shape_line(line_text.into(), font_size, &[run], None);
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

        PrepaintState { lines, bounds }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        // Clone necessary data before painting to avoid borrow conflicts
        let (focus_handle, cursor_visible, cursor_offset, text) = {
            let state = self.state.read(cx);
            (
                state.focus_handle.clone(),
                state.blink_cursor.visible(),
                state.cursor_offset,
                state.text.to_string(),
            )
        };

        let is_focused = focus_handle.is_focused(window);

        // Register input handler
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(prepaint.bounds, self.state.clone()),
            cx,
        );

        let line_height = window.line_height();
        let mut y_offset = prepaint.bounds.origin.y;

        // Paint each line
        for line in &prepaint.lines {
            let origin = point(prepaint.bounds.origin.x, y_offset);
            let _ = line.paint(origin, line_height, window, cx);
            y_offset += line_height;
        }

        // Paint cursor if focused
        if is_focused && cursor_visible {
            // Find which line the cursor is on
            let mut current_byte = 0;
            let mut current_line = 0;
            let mut byte_offset_in_line = 0;

            for ch in text.chars() {
                if current_byte >= cursor_offset {
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
                let cursor_x = prepaint.bounds.origin.x + x_pos;
                let cursor_y = prepaint.bounds.origin.y + (current_line as f32 * line_height);

                // Draw cursor
                let cursor_bounds = gpui::Bounds {
                    origin: point(cursor_x, cursor_y),
                    size: size(px(2.0), line_height),
                };

                window.paint_quad(fill(cursor_bounds, OneDarkTheme::text()));
            }
        }
    }
}

impl Render for InputState {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_focused = self.focus_handle.is_focused(window);
        let is_multiline = self.mode.is_multi_line();
        let entity = cx.entity();

        div()
            .flex()
            .w_full()
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
            .key_context("InputState")
            .track_focus(&self.focus_handle)
            .when(!self.disabled, |div| {
                div.on_action(cx.listener(InputState::backspace))
                    .on_action(cx.listener(InputState::delete))
                    .on_action(cx.listener(InputState::escape))
                    .on_action(cx.listener(InputState::left))
                    .on_action(cx.listener(InputState::right))
                    .on_action(cx.listener(InputState::select_all))
                    .on_action(cx.listener(InputState::copy))
                    .on_action(cx.listener(InputState::cut))
                    .on_action(cx.listener(InputState::paste))
                    .when(is_multiline, |div| {
                        div.on_action(cx.listener(InputState::enter))
                            .on_action(cx.listener(InputState::up))
                            .on_action(cx.listener(InputState::down))
                    })
            })
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|state, _, window, cx| {
                    state.focus(window, cx);
                }),
            )
            .child(Input::new(entity))
    }
}
