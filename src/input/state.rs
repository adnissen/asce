use gpui::{
    actions, App, Context, EntityInputHandler, FocusHandle, Focusable, Pixels,
    SharedString, UTF16Selection, Window,
};
use ropey::Rope;
use std::ops::Range;
use sum_tree::Bias;

use super::{
    blink_cursor::BlinkCursor, mode::InputMode, rope_ext::RopeExt, selection::Selection,
};

actions!(
    input,
    [
        Backspace,
        Delete,
        Enter,
        Escape,
        Left,
        Right,
        Up,
        Down,
        SelectAll,
        Copy,
        Cut,
        Paste,
    ]
);

pub struct InputState {
    pub(super) focus_handle: FocusHandle,
    pub(super) text: Rope,
    pub(super) placeholder: SharedString,
    pub(super) selected_range: Selection,
    pub(super) cursor_offset: usize,
    pub(super) mode: InputMode,
    pub(super) blink_cursor: BlinkCursor,
    pub(super) disabled: bool,
}

impl InputState {
    pub fn new(cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            text: Rope::from(""),
            placeholder: "".into(),
            selected_range: Selection::default(),
            cursor_offset: 0,
            mode: InputMode::default(),
            blink_cursor: BlinkCursor::new(),
            disabled: false,
        }
    }

    pub fn set_text(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.text = Rope::from(text.into().as_str());
        self.cursor_offset = self.text.len_bytes();
        self.selected_range = Selection::new(self.cursor_offset, self.cursor_offset);
        cx.notify();
    }

    pub fn set_placeholder(&mut self, placeholder: impl Into<SharedString>) {
        self.placeholder = placeholder.into();
    }

    pub fn set_mode(&mut self, mode: InputMode) {
        self.mode = mode;
    }

    pub fn set_multiline(&mut self, multiline: bool) {
        if multiline {
            self.mode = InputMode::MultiLine {
                tab: Default::default(),
                rows: 3,
            };
        } else {
            self.mode = InputMode::SingleLine;
        }
    }

    pub fn set_fill_height(&mut self, fill_height: bool) {
        // For auto-grow mode
        if fill_height {
            if let InputMode::MultiLine { .. } = self.mode {
                self.mode = InputMode::AutoGrow {
                    rows: 3,
                    min_rows: 3,
                    max_rows: 20,
                };
            }
        }
    }

    pub fn text(&self) -> String {
        self.text.to_string()
    }

    pub fn content(&self) -> String {
        self.text.to_string()
    }

    pub fn set_content(&mut self, content: impl Into<String>, cx: &mut Context<Self>) {
        self.set_content_with_cursor(content, true, cx);
    }

    pub fn set_content_with_cursor(
        &mut self,
        content: impl Into<String>,
        reset_cursor: bool,
        cx: &mut Context<Self>,
    ) {
        self.text = Rope::from(content.into().as_str());
        if reset_cursor {
            self.cursor_offset = self.text.len_bytes();
        } else {
            self.cursor_offset = self.cursor_offset.min(self.text.len_bytes());
        }
        self.selected_range = Selection::new(self.cursor_offset, self.cursor_offset);
        cx.notify();
    }

    pub fn selected_text(&self) -> String {
        if self.selected_range.is_empty() {
            String::new()
        } else {
            self.text
                .slice(self.selected_range.start..self.selected_range.end)
                .to_string()
        }
    }

    pub fn focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.focus(window);
        self.blink_cursor.start();
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.text = Rope::from("");
        self.cursor_offset = 0;
        self.selected_range = Selection::default();
        cx.notify();
    }

    // Actions
    pub fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }

        if !self.selected_range.is_empty() {
            self.delete_selection(cx);
        } else if self.cursor_offset > 0 {
            // In ropey, find the previous char boundary
            let char_idx = self.text.byte_to_char(self.cursor_offset);
            if char_idx > 0 {
                let prev_byte = self.text.char_to_byte(char_idx - 1);
                self.text.replace(prev_byte..self.cursor_offset, "");
                self.cursor_offset = prev_byte;
                self.selected_range = Selection::new(self.cursor_offset, self.cursor_offset);
            }
        }
        self.blink_cursor.pause();
        cx.notify();
    }

    pub fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }

        if !self.selected_range.is_empty() {
            self.delete_selection(cx);
        } else if self.cursor_offset < self.text.len_bytes() {
            // In ropey, find the next char boundary
            let char_idx = self.text.byte_to_char(self.cursor_offset);
            if char_idx < self.text.len_chars() {
                let next_byte = self.text.char_to_byte(char_idx + 1);
                self.text.replace(self.cursor_offset..next_byte, "");
                self.selected_range = Selection::new(self.cursor_offset, self.cursor_offset);
            }
        }
        self.blink_cursor.pause();
        cx.notify();
    }

    fn delete_selection(&mut self, _cx: &mut Context<Self>) {
        self.text
            .replace(self.selected_range.start..self.selected_range.end, "");
        self.cursor_offset = self.selected_range.start;
        self.selected_range = Selection::new(self.cursor_offset, self.cursor_offset);
        self.blink_cursor.pause();
    }

    pub fn enter(&mut self, _: &Enter, _: &mut Window, cx: &mut Context<Self>) {
        if self.disabled || !self.mode.is_multi_line() {
            return;
        }

        self.insert_text("\n", cx);
    }

    pub fn escape(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        window.blur();
        cx.notify();
    }

    pub fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.cursor_offset > 0 {
            self.cursor_offset = self.text.clip_offset(self.cursor_offset - 1, Bias::Left);
            self.selected_range = Selection::new(self.cursor_offset, self.cursor_offset);
            self.blink_cursor.pause();
            cx.notify();
        }
    }

    pub fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.cursor_offset < self.text.len_bytes() {
            self.cursor_offset = self.text.clip_offset(self.cursor_offset + 1, Bias::Right);
            self.selected_range = Selection::new(self.cursor_offset, self.cursor_offset);
            self.blink_cursor.pause();
            cx.notify();
        }
    }

    pub fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        if !self.mode.is_multi_line() {
            return;
        }

        let pos = self.text.offset_to_position(self.cursor_offset);
        if pos.line > 0 {
            let new_pos = super::selection::Position::new(pos.line - 1, pos.character);
            self.cursor_offset = self.text.position_to_offset(&new_pos);
            self.selected_range = Selection::new(self.cursor_offset, self.cursor_offset);
            self.blink_cursor.pause();
            cx.notify();
        }
    }

    pub fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        if !self.mode.is_multi_line() {
            return;
        }

        let pos = self.text.offset_to_position(self.cursor_offset);
        if (pos.line as usize) < self.text.lines_len() - 1 {
            let new_pos = super::selection::Position::new(pos.line + 1, pos.character);
            self.cursor_offset = self.text.position_to_offset(&new_pos);
            self.selected_range = Selection::new(self.cursor_offset, self.cursor_offset);
            self.blink_cursor.pause();
            cx.notify();
        }
    }

    pub fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.selected_range = Selection::new(0, self.text.len_bytes());
        self.cursor_offset = self.text.len_bytes();
        cx.notify();
    }

    pub fn copy(&mut self, _: &Copy, _window: &mut Window, _: &mut Context<Self>) {
        // TODO: Implement clipboard copy when API is available
        // if !self.selected_range.is_empty() {
        //     let text = self.selected_text();
        //     window.write_clipboard_item(ClipboardItem::new_string(text));
        // }
    }

    pub fn cut(&mut self, _: &Cut, _window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }

        if !self.selected_range.is_empty() {
            // TODO: Implement clipboard cut when API is available
            self.delete_selection(cx);
            cx.notify();
        }
    }

    pub fn paste(&mut self, _: &Paste, _window: &mut Window, _cx: &mut Context<Self>) {
        // TODO: Implement clipboard paste when API is available
        // if self.disabled {
        //     return;
        // }
    }

    fn insert_text(&mut self, text: &str, cx: &mut Context<Self>) {
        // Delete selection if any
        if !self.selected_range.is_empty() {
            self.text
                .replace(self.selected_range.start..self.selected_range.end, "");
            self.cursor_offset = self.selected_range.start;
        }

        // Filter out newlines in single-line mode
        let text = if self.mode.is_single_line() {
            text.replace('\n', "").replace('\r', "")
        } else {
            text.to_string()
        };

        self.text.insert(self.cursor_offset, &text);
        self.cursor_offset += text.len();
        self.selected_range = Selection::new(self.cursor_offset, self.cursor_offset);
        self.blink_cursor.pause();
        cx.notify();
    }

    pub(super) fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.text.offset_to_offset_utf16(range.start)..self.text.offset_to_offset_utf16(range.end)
    }

    pub(super) fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.text.offset_utf16_to_offset(range_utf16.start)
            ..self.text.offset_utf16_to_offset(range_utf16.end)
    }
}

impl EntityInputHandler for InputState {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.text.slice(range).to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&(self.cursor_offset..self.cursor_offset)),
            reversed: false,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        None
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {}

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .unwrap_or(self.cursor_offset..self.cursor_offset);

        // Filter newlines in single-line mode
        let new_text = if self.mode.is_single_line() {
            new_text.replace('\n', "").replace('\r', "")
        } else {
            new_text.to_string()
        };

        self.text.replace(range.clone(), &new_text);
        self.cursor_offset = range.start + new_text.len();
        self.selected_range = Selection::new(self.cursor_offset, self.cursor_offset);
        self.blink_cursor.pause();
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
        bounds: gpui::Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<gpui::Bounds<Pixels>> {
        Some(bounds)
    }

    fn character_index_for_point(
        &mut self,
        _point: gpui::Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        // TODO: implement proper character index for point calculation
        None
    }
}

impl Focusable for InputState {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
