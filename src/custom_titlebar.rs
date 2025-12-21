use crate::theme::{OneDarkExt, ThemeRegistry};
use crate::SwitchTheme;
use gpui::{
    div, prelude::FluentBuilder, px, svg, Context, Corner, FontWeight, InteractiveElement,
    IntoElement, MouseButton, ParentElement, Render, SharedString, StatefulInteractiveElement,
    Styled, Window,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::menu::DropdownMenu;
use gpui_component::{ActiveTheme, Theme};

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
    fn start_window_drag(window: &mut Window) {
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
    fn start_window_drag(_window: &mut Window) {
        // No-op on non-Windows platforms
    }

    /// Render the theme palette button with dropdown theme picker (Windows only)
    #[cfg(target_os = "windows")]
    fn render_theme_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::global(cx);
        let current_theme_name = theme.theme_name().to_string();
        let icon_color = theme.muted_foreground;

        Button::new("theme-menu")
            .ghost()
            .compact()
            .mr(px(8.0))
            .child(
                svg()
                    .path("icons/palette.svg")
                    .size(px(16.0))
                    .text_color(icon_color),
            )
            .dropdown_menu_with_anchor(Corner::TopLeft, move |menu, _window, _cx| {
                let current_name = current_theme_name.clone();
                let registry = ThemeRegistry::new();

                let mut menu = menu.scrollable(true).max_h(px(300.0));
                for theme_variant in &registry.themes {
                    let is_current = theme_variant.name == current_name;
                    let theme_name: SharedString = theme_variant.name.clone().into();
                    menu = menu.menu_with_check(
                        theme_variant.name.clone(),
                        is_current,
                        Box::new(SwitchTheme(theme_name)),
                    );
                }
                menu
            })
    }
}

impl Render for CustomTitlebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Render theme menu first (Windows only) to avoid borrow issues
        #[cfg(target_os = "windows")]
        let theme_menu = Some(self.render_theme_menu(cx).into_any_element());
        #[cfg(not(target_os = "windows"))]
        let theme_menu: Option<gpui::AnyElement> = None;

        let theme = cx.theme();
        // Pre-capture colors for use in closures
        let hover_bg = theme.element_hover();
        let active_bg = theme.element_active();
        let error_bg = theme.error();
        let text_muted_color = theme.text_muted();
        let text_color = theme.text();

        div()
            .id("titlebar")
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .w_full()
            .h(px(37.0)) // Standard titlebar height
            .bg(theme.editor_background())
            .border_b_1()
            .border_color(hover_bg)
            // Theme menu button (Windows only) - before the title area
            .when_some(theme_menu, |this, menu| this.child(menu))
            .child(
                // Left side: Title - this area is draggable on Windows
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .h_full()
                    .flex_grow() // Take up remaining space to create a larger drag area
                    // macOS needs extra padding for traffic lights, Windows has no left padding (theme menu is there)
                    .when(cfg!(target_os = "macos"), |this| this.pl(px(74.0)))
                    .pr_4()
                    // Enable window dragging on Windows when clicking title area
                    .when(cfg!(target_os = "windows"), |this| {
                        this.on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|_, _, window, _| {
                                Self::start_window_drag(window);
                            }),
                        )
                    })
                    // App name label
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(text_color)
                            .child("asve"),
                    )
                    // Version number
                    .child(
                        div()
                            .text_sm()
                            .text_color(text_muted_color)
                            .child(concat!(" v", env!("CARGO_PKG_VERSION"))),
                    ),
            )
            // Right side: Window controls (Windows only - macOS uses native traffic lights)
            .when(cfg!(target_os = "windows"), |this| {
                this.child(
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
                                .hover(move |style| style.bg(hover_bg))
                                .active(move |style| style.bg(active_bg))
                                .on_click(cx.listener(|_, _, window, _| {
                                    window.minimize_window();
                                }))
                                .child(
                                    div()
                                        .text_size(px(16.0))
                                        .text_color(text_muted_color)
                                        .child("─"),
                                ),
                            // Maximize/Restore button
                            div()
                                .id("maximize-button")
                                .flex()
                                .items_center()
                                .justify_center()
                                .size(px(30.0))
                                .rounded_sm()
                                .hover(move |style| style.bg(hover_bg))
                                .active(move |style| style.bg(active_bg))
                                .on_click(cx.listener(|_, _, window, _| {
                                    window.zoom_window();
                                }))
                                .child(
                                    div()
                                        .text_size(px(14.0))
                                        .text_color(text_muted_color)
                                        .child("□"),
                                ),
                            // Close button
                            div()
                                .id("close-button")
                                .flex()
                                .items_center()
                                .justify_center()
                                .size(px(30.0))
                                .rounded_sm()
                                .hover(move |style| style.bg(error_bg))
                                .active(move |style| style.bg(error_bg))
                                .on_click(cx.listener(|_, _, window, _| {
                                    window.remove_window();
                                }))
                                .child(
                                    div()
                                        .text_size(px(14.0))
                                        .text_color(text_color)
                                        .child("✕"),
                                ),
                        ]),
                )
            })
    }
}
