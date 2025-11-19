use gpui::{div, prelude::*, rgb, Context, Entity, IntoElement, PathPromptOptions, Render, Window};

use crate::custom_titlebar::CustomTitlebar;
use crate::ffmpeg_export;

/// Initial window that shows just an "Open File" button
pub struct InitialWindow {
    titlebar: Entity<CustomTitlebar>,
}

impl InitialWindow {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let titlebar = cx.new(|_| CustomTitlebar::new("asve"));
        Self { titlebar }
    }
}

impl Render for InitialWindow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .bg(rgb(0x1a1a1a))
            .size_full()
            .child(self.titlebar.clone())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .justify_center()
                    .items_center()
                    .child(
                div()
                    .id("open-file-button")
                    .px_8()
                    .py_4()
                    .bg(rgb(0x404040))
                    .rounded_lg()
                    .cursor_pointer()
                    .text_xl()
                    .text_color(rgb(0xffffff))
                    .hover(|style| style.bg(rgb(0x505050)))
                    .on_click(|_, _window, cx| {
                        let paths = cx.prompt_for_paths(PathPromptOptions {
                            files: true,
                            directories: false,
                            multiple: false,
                            prompt: Some("Select a video file".into()),
                        });

                        cx.spawn(async move |cx| {
                            if let Ok(Ok(Some(paths))) = paths.await {
                                if let Some(path) = paths.first() {
                                    // Check if the file has a valid extension
                                    let extension = path.extension().and_then(|e| e.to_str());
                                    let supported_extensions =
                                        ffmpeg_export::get_video_extensions();

                                    if let Some(ext) = extension {
                                        let ext_lower = ext.to_lowercase();
                                        if supported_extensions.contains(&ext_lower.as_str()) {
                                            let path_string = path.to_string_lossy().to_string();
                                            let path_clone = path_string.clone();

                                            cx.update(|cx| {
                                                crate::create_video_windows(
                                                    cx,
                                                    path_string,
                                                    path_clone,
                                                );
                                            })
                                            .ok();
                                        } else {
                                            // Invalid file type
                                            eprintln!(
                                                "Invalid file type. Supported formats: {}",
                                                supported_extensions.join(", ")
                                            );
                                        }
                                    }
                                }
                            }
                        })
                        .detach();
                    })
                    .child("Open File"),
                    )
            )
    }
}
