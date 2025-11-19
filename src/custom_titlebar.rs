use gpui::*;

pub struct CustomTitlebar {
    title: SharedString,
}

impl CustomTitlebar {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
        }
    }
}

impl Render for CustomTitlebar {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        div()
            .id("titlebar")
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .w_full()
            .h(px(37.0)) // Standard titlebar height
            .bg(rgb(0x1e1e1e))
            .border_b_1()
            .border_color(rgb(0x2d2d2d))
            .child(
                // Left side: Title
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    // macOS needs extra padding for traffic lights
                    .pl(if cfg!(target_os = "macos") {
                        px(74.0)
                    } else {
                        px(16.0)
                    })
                    .pr_4()
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0xcccccc))
                            .child(self.title.clone()),
                    ),
            )
            .child(
                // Right side: Window controls
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .gap_2()
                    .pr_2()
                    .children(vec![
                        // Minimize button
                        div()
                            .id("minimize-button")
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(30.0))
                            .rounded_sm()
                            .hover(|style| style.bg(rgb(0x2d2d2d)))
                            .active(|style| style.bg(rgb(0x404040)))
                            .on_click(cx.listener(|_, _, window, _| {
                                window.minimize_window();
                            }))
                            .child(
                                svg()
                                    .path("M 0,5 H 10")
                                    .size(px(10.0))
                                    .text_color(rgb(0xcccccc)),
                            ),
                        // Maximize/Restore button
                        div()
                            .id("maximize-button")
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(30.0))
                            .rounded_sm()
                            .hover(|style| style.bg(rgb(0x2d2d2d)))
                            .active(|style| style.bg(rgb(0x404040)))
                            .on_click(cx.listener(|_, _, window, _| {
                                window.zoom_window();
                            }))
                            .child(
                                svg()
                                    .path("M 0,0 H 10 V 10 H 0 Z M 0,1 H 10")
                                    .size(px(10.0))
                                    .text_color(rgb(0xcccccc)),
                            ),
                        // Close button
                        div()
                            .id("close-button")
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(30.0))
                            .rounded_sm()
                            .hover(|style| style.bg(rgb(0xe81123)))
                            .active(|style| style.bg(rgb(0xc50f1f)))
                            .on_click(cx.listener(|_, _, window, _| {
                                window.remove_window();
                            }))
                            .child(
                                svg()
                                    .path("M 0,0 L 10,10 M 10,0 L 0,10")
                                    .size(px(10.0))
                                    .text_color(rgb(0xffffff)),
                            ),
                    ]),
            )
    }
}
