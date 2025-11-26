use std::ops::Range;

use ropey::{Rope, RopeSlice};
use sum_tree::Bias;

use crate::input::selection::Position;

/// An iterator over the lines of a `Rope`.
pub struct RopeLines<'a> {
    rope: &'a Rope,
    row: usize,
    end_row: usize,
}

impl<'a> RopeLines<'a> {
    /// Create a new `RopeLines` iterator.
    pub fn new(rope: &'a Rope) -> Self {
        let end_row = rope.lines_len();
        Self {
            row: 0,
            end_row,
            rope,
        }
    }
}

impl<'a> Iterator for RopeLines<'a> {
    type Item = RopeSlice<'a>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.row >= self.end_row {
            return None;
        }

        let line = self.rope.slice_line(self.row);
        self.row += 1;
        Some(line)
    }

    #[inline]
    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.row = self.row.saturating_add(n);
        self.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.end_row - self.row;
        (len, Some(len))
    }
}

impl std::iter::ExactSizeIterator for RopeLines<'_> {}
impl std::iter::FusedIterator for RopeLines<'_> {}

/// An extension trait for [`Rope`] to provide additional utility methods.
pub trait RopeExt {
    fn line_start_offset(&self, row: usize) -> usize;
    fn line_end_offset(&self, row: usize) -> usize;
    fn slice_line(&self, row: usize) -> RopeSlice<'_>;
    fn slice_lines(&self, rows_range: Range<usize>) -> RopeSlice<'_>;
    fn iter_lines(&self) -> RopeLines<'_>;
    fn lines_len(&self) -> usize;
    fn line_len(&self, row: usize) -> usize;
    fn replace(&mut self, range: Range<usize>, new_text: &str);
    fn char_at(&self, offset: usize) -> Option<char>;
    fn position_to_offset(&self, line_col: &Position) -> usize;
    fn offset_to_position(&self, offset: usize) -> Position;
    fn word_range(&self, offset: usize) -> Option<Range<usize>>;
    fn word_at(&self, offset: usize) -> String;
    fn offset_utf16_to_offset(&self, offset_utf16: usize) -> usize;
    fn offset_to_offset_utf16(&self, offset: usize) -> usize;
    fn clip_offset(&self, offset: usize, bias: Bias) -> usize;
    fn char_index_to_offset(&self, char_index: usize) -> usize;
    fn offset_to_char_index(&self, offset: usize) -> usize;
}

impl RopeExt for Rope {
    fn slice_line(&self, row: usize) -> RopeSlice<'_> {
        let total_lines = self.lines_len();
        if row >= total_lines {
            return self.slice(0..0);
        }

        let line = self.line(row);
        let line_len = line.len_bytes();
        if line_len > 0 {
            let line_end = line_len - 1;
            if line_end < line_len && line.char(line_end) == '\n' {
                return line.slice(..line_end);
            }
        }

        line
    }

    fn slice_lines(&self, rows_range: Range<usize>) -> RopeSlice<'_> {
        let start = self.line_start_offset(rows_range.start);
        let end = self.line_end_offset(rows_range.end.saturating_sub(1));
        self.slice(start..end)
    }

    fn iter_lines(&self) -> RopeLines<'_> {
        RopeLines::new(&self)
    }

    fn line_len(&self, row: usize) -> usize {
        self.slice_line(row).len_bytes()
    }

    fn line_start_offset(&self, row: usize) -> usize {
        if row >= self.lines_len() {
            return self.len_bytes();
        }

        self.line_to_byte(row)
    }

    fn position_to_offset(&self, pos: &Position) -> usize {
        let line = self.slice_line(pos.line as usize);
        self.line_start_offset(pos.line as usize)
            + line
                .chars()
                .take(pos.character as usize)
                .map(|c| c.len_utf8())
                .sum::<usize>()
    }

    fn offset_to_position(&self, offset: usize) -> Position {
        let offset = self.clip_offset(offset, Bias::Left);
        let row = self.byte_to_line(offset);
        let line_start = self.line_to_byte(row);
        let column_offset = offset.saturating_sub(line_start);

        let line = self.slice_line(row);
        let character = line.slice(..column_offset.min(line.len_bytes())).chars().count();

        Position::new(row as u32, character as u32)
    }

    fn line_end_offset(&self, row: usize) -> usize {
        if row >= self.lines_len() {
            return self.len_bytes();
        }

        self.line_start_offset(row) + self.line_len(row)
    }

    fn lines_len(&self) -> usize {
        self.len_lines()
    }

    fn char_at(&self, offset: usize) -> Option<char> {
        if offset >= self.len_bytes() {
            return None;
        }

        Some(self.char(offset))
    }

    fn word_range(&self, offset: usize) -> Option<Range<usize>> {
        if offset >= self.len_bytes() {
            return None;
        }

        let mut left = String::new();
        let offset = self.clip_offset(offset, Bias::Left);
        for c in self.chars_at(offset).reversed() {
            if c.is_alphanumeric() || c == '_' {
                left.insert(0, c);
            } else {
                break;
            }
        }
        let start = offset.saturating_sub(left.len());

        let right = self
            .chars_at(offset)
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect::<String>();

        let end = offset + right.len();

        if start == end {
            None
        } else {
            Some(start..end)
        }
    }

    fn word_at(&self, offset: usize) -> String {
        if let Some(range) = self.word_range(offset) {
            self.slice(range).to_string()
        } else {
            String::new()
        }
    }

    #[inline]
    fn offset_utf16_to_offset(&self, offset_utf16: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for ch in self.chars() {
            if utf16_count >= offset_utf16 {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }

        utf8_offset.min(self.len_bytes())
    }

    #[inline]
    fn offset_to_offset_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;

        for ch in self.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        utf16_offset
    }

    fn replace(&mut self, range: Range<usize>, new_text: &str) {
        let range =
            self.clip_offset(range.start, Bias::Left)..self.clip_offset(range.end, Bias::Right);
        self.remove(range.clone());
        self.insert(range.start, new_text);
    }

    fn clip_offset(&self, offset: usize, bias: Bias) -> usize {
        let offset = offset.min(self.len_bytes());

        // In ropey, try_byte_to_char returns Ok if it's a valid char boundary
        if self.try_byte_to_char(offset).is_ok() {
            return offset;
        }

        if bias == Bias::Left {
            // Find previous char boundary
            let mut check_offset = offset;
            while check_offset > 0 && self.try_byte_to_char(check_offset).is_err() {
                check_offset -= 1;
            }
            check_offset
        } else {
            // Find next char boundary
            let mut check_offset = offset;
            let len = self.len_bytes();
            while check_offset < len && self.try_byte_to_char(check_offset).is_err() {
                check_offset += 1;
            }
            check_offset
        }
    }

    fn char_index_to_offset(&self, char_offset: usize) -> usize {
        self.chars().take(char_offset).map(|c| c.len_utf8()).sum()
    }

    fn offset_to_char_index(&self, offset: usize) -> usize {
        let offset = self.clip_offset(offset, Bias::Right);
        self.slice(..offset).chars().count()
    }
}
