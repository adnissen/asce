use crate::theme::OneDarkExt;
use gpui::{div, prelude::*, Context, IntoElement, Render, Window};
use gpui_component::ActiveTheme;

/// Video player window that displays the video
pub struct VideoPlayerWindow;

impl Render for VideoPlayerWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        // Full window for video display - GStreamer will render directly to this window
        div()
            .flex()
            .bg(theme.editor_background())
            .size_full()
    }
}
