
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TabSize {
    pub tab_size: usize,
    pub hard_tabs: bool,
}

impl Default for TabSize {
    fn default() -> Self {
        Self {
            tab_size: 2,
            hard_tabs: false,
        }
    }
}

impl TabSize {
    pub fn new(tab_size: usize, hard_tabs: bool) -> Self {
        Self {
            tab_size,
            hard_tabs,
        }
    }

    pub fn to_string(&self) -> String {
        if self.hard_tabs {
            "\t".to_string()
        } else {
            " ".repeat(self.tab_size)
        }
    }
}

#[derive(Default, Clone)]
pub enum InputMode {
    #[default]
    SingleLine,
    MultiLine {
        tab: TabSize,
        rows: usize,
    },
    AutoGrow {
        rows: usize,
        min_rows: usize,
        max_rows: usize,
    },
}

impl InputMode {
    #[inline]
    pub fn is_single_line(&self) -> bool {
        matches!(self, InputMode::SingleLine)
    }

    #[inline]
    pub fn is_auto_grow(&self) -> bool {
        matches!(self, InputMode::AutoGrow { .. })
    }

    #[inline]
    pub fn is_multi_line(&self) -> bool {
        matches!(
            self,
            InputMode::MultiLine { .. } | InputMode::AutoGrow { .. }
        )
    }

    pub fn set_rows(&mut self, new_rows: usize) {
        match self {
            InputMode::MultiLine { rows, .. } => {
                *rows = new_rows;
            }
            InputMode::AutoGrow {
                rows,
                min_rows,
                max_rows,
            } => {
                *rows = new_rows.clamp(*min_rows, *max_rows);
            }
            _ => {}
        }
    }

    /// At least 1 row be return.
    pub fn rows(&self) -> usize {
        match self {
            InputMode::MultiLine { rows, .. } => *rows,
            InputMode::AutoGrow { rows, .. } => *rows,
            _ => 1,
        }
        .max(1)
    }

    /// At least 1 row be return.
    #[allow(unused)]
    pub fn min_rows(&self) -> usize {
        match self {
            InputMode::MultiLine { .. } => 1,
            InputMode::AutoGrow { min_rows, .. } => *min_rows,
            _ => 1,
        }
        .max(1)
    }

    #[allow(unused)]
    pub fn max_rows(&self) -> usize {
        match self {
            InputMode::MultiLine { .. } => usize::MAX,
            InputMode::AutoGrow { max_rows, .. } => *max_rows,
            _ => 1,
        }
    }
}
