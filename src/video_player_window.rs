use crate::theme::OneDarkTheme;
use gpui::{div, prelude::*, Context, IntoElement, Render, Window};

/// Video player window that displays the video
pub struct VideoPlayerWindow;

impl Render for VideoPlayerWindow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // Full window for video display - GStreamer will render directly to this window
        div()
            .flex()
            .bg(OneDarkTheme::editor_background())
            .size_full()
    }
}
