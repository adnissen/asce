use gpui::{px, Pixels};

pub(super) const CURSOR_WIDTH: Pixels = px(1.5);

/// Simplified cursor for the Input component
pub(crate) struct BlinkCursor {
    visible: bool,
}

impl BlinkCursor {
    pub fn new() -> Self {
        Self { visible: true }
    }

    pub fn visible(&self) -> bool {
        self.visible
    }

    /// Pause just sets visible to true (simplified version)
    pub fn pause(&mut self) {
        self.visible = true;
    }

    /// Start just sets visible to true (simplified version)
    pub fn start(&mut self) {
        self.visible = true;
    }
}
