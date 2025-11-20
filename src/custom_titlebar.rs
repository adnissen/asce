use crate::theme::OneDarkTheme;
use gpui::prelude::*;
use gpui::*;

#[cfg(target_os = "windows")]
use raw_window_handle::HasWindowHandle;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::*;
#[cfg(target_os = "windows")]
use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::*;

pub struct CustomTitlebar {
    title: SharedString,
}

impl CustomTitlebar {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
        }
    }

    pub fn set_title(&mut self, title: impl Into<SharedString>) {
        self.title = title.into();
    }

    #[cfg(target_os = "windows")]
    fn start_window_drag(window: &mut gpui::Window) {
        // Get the raw window handle (HWND)
        if let Ok(handle) = window.window_handle() {
            if let raw_window_handle::RawWindowHandle::Win32(win32_handle) = handle.as_raw() {
                unsafe {
                    let hwnd = HWND(win32_handle.hwnd.get() as _);
                    // Release mouse capture to allow Windows to handle dragging
                    let _ = ReleaseCapture();
                    // Send WM_NCLBUTTONDOWN with HTCAPTION to start window dragging
                    SendMessageW(
                        hwnd,
                        WM_NCLBUTTONDOWN,
                        Some(WPARAM(HTCAPTION as usize)),
                        Some(LPARAM(0)),
                    );
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    fn start_window_drag(_window: &mut gpui::Window) {
        // No-op on non-Windows platforms
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
            .bg(OneDarkTheme::editor_background())
            .border_b_1()
            .border_color(OneDarkTheme::element_hover())
            .child(
                // Left side: Title - this area is draggable on Windows
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .flex_grow() // Take up remaining space to create a larger drag area
                    // macOS needs extra padding for traffic lights
                    .pl(if cfg!(target_os = "macos") {
                        px(74.0)
                    } else {
                        px(16.0)
                    })
                    .pr_4()
                    // Enable window dragging on Windows when clicking title area
                    .when(cfg!(target_os = "windows"), |this| {
                        this.on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(|_, _, window, _| {
                                Self::start_window_drag(window);
                            }),
                        )
                    })
                    .child(
                        div()
                            .text_xs()
                            .text_color(OneDarkTheme::text_muted())
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
                            .hover(|style| style.bg(OneDarkTheme::element_hover()))
                            .active(|style| style.bg(OneDarkTheme::element_active()))
                            .on_click(cx.listener(|_, _, window, _| {
                                window.minimize_window();
                            }))
                            .child(
                                // Use Unicode character on Windows, SVG on other platforms
                                div()
                                    .when(cfg!(target_os = "windows"), |this| {
                                        this.text_size(px(16.0))
                                            .text_color(OneDarkTheme::text_muted())
                                            .child("─")
                                    })
                                    .when(!cfg!(target_os = "windows"), |this| {
                                        this.text_color(OneDarkTheme::text_muted()).child(
                                            svg()
                                                .path("M 0,5 H 10")
                                                .size(px(10.0))
                                                .text_color(OneDarkTheme::text_muted()),
                                        )
                                    }),
                            ),
                        // Maximize/Restore button
                        div()
                            .id("maximize-button")
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(30.0))
                            .rounded_sm()
                            .hover(|style| style.bg(OneDarkTheme::element_hover()))
                            .active(|style| style.bg(OneDarkTheme::element_active()))
                            .on_click(cx.listener(|_, _, window, _| {
                                window.zoom_window();
                            }))
                            .child(
                                // Use Unicode character on Windows, SVG on other platforms
                                div()
                                    .when(cfg!(target_os = "windows"), |this| {
                                        this.text_size(px(14.0))
                                            .text_color(OneDarkTheme::text_muted())
                                            .child("□")
                                    })
                                    .when(!cfg!(target_os = "windows"), |this| {
                                        this.text_color(OneDarkTheme::text_muted()).child(
                                            svg()
                                                .path("M 0,0 H 10 V 10 H 0 Z M 0,1 H 10")
                                                .size(px(10.0))
                                                .text_color(OneDarkTheme::text_muted()),
                                        )
                                    }),
                            ),
                        // Close button
                        div()
                            .id("close-button")
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(30.0))
                            .rounded_sm()
                            .hover(|style| style.bg(OneDarkTheme::error()))
                            .active(|style| style.bg(OneDarkTheme::error()))
                            .on_click(cx.listener(|_, _, window, _| {
                                window.remove_window();
                            }))
                            .child(
                                // Use Unicode character on Windows, SVG on other platforms
                                div()
                                    .when(cfg!(target_os = "windows"), |this| {
                                        this.text_size(px(14.0))
                                            .text_color(OneDarkTheme::text())
                                            .child("✕")
                                    })
                                    .when(!cfg!(target_os = "windows"), |this| {
                                        this.text_color(OneDarkTheme::text()).child(
                                            svg()
                                                .path("M 0,0 L 10,10 M 10,0 L 0,10")
                                                .size(px(10.0))
                                                .text_color(OneDarkTheme::text()),
                                        )
                                    }),
                            ),
                    ]),
            )
    }
}
