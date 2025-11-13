use gpui::{div, prelude::*, rgb, Context, IntoElement, Render, Window};

/// Video player window that displays the video
pub struct VideoPlayerWindow;

impl Render for VideoPlayerWindow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // Full window for video display - GStreamer will render directly to this window
        div().flex().bg(rgb(0x000000)).size_full()
    }
}
