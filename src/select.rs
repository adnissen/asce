//! Select/Dropdown component adapted from gpui-component
//!
//! A dropdown component for selecting from a list of options.

use gpui::{
    div, prelude::*, px, rgb, App, Context, Empty, Entity, EventEmitter, IntoElement, MouseButton,
    MouseDownEvent, Render, RenderOnce, StyleRefinement, Styled, Window,
};

/// Events emitted by the SelectState
#[derive(Clone)]
pub enum SelectEvent {
    /// Fired when an item is selected
    Change(usize),
}

/// Trait for items that can be displayed in a Select component
pub trait SelectItem: Clone + 'static {
    /// Get the display title for this item
    fn display_title(&self) -> String;
}

/// Implement SelectItem for String
impl SelectItem for String {
    fn display_title(&self) -> String {
        self.clone()
    }
}

/// State of the Select component
pub struct SelectState<T: SelectItem> {
    items: Vec<T>,
    selected_index: Option<usize>,
    is_open: bool,
}

impl<T: SelectItem> SelectState<T> {
    /// Create a new SelectState with the given items
    pub fn new(items: Vec<T>) -> Self {
        Self {
            items,
            selected_index: None,
            is_open: false,
        }
    }

    /// Set the selected index
    pub fn set_selected_index(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        if index != self.selected_index {
            self.selected_index = index;
            if let Some(idx) = index {
                cx.emit(SelectEvent::Change(idx));
            }
            cx.notify();
        }
    }

    /// Get the selected index
    pub fn selected_index(&self) -> Option<usize> {
        self.selected_index
    }

    /// Get the selected item
    pub fn selected_item(&self) -> Option<&T> {
        self.selected_index.and_then(|idx| self.items.get(idx))
    }

    /// Update the items list
    pub fn set_items(&mut self, items: Vec<T>, cx: &mut Context<Self>) {
        self.items = items;
        // Reset selection if it's out of bounds
        if let Some(idx) = self.selected_index {
            if idx >= self.items.len() {
                self.selected_index = None;
            }
        }
        cx.notify();
    }

    /// Get all items
    pub fn items(&self) -> &[T] {
        &self.items
    }

    /// Toggle dropdown open/closed state
    fn toggle_open(&mut self, cx: &mut Context<Self>) {
        self.is_open = !self.is_open;
        cx.notify();
    }

    /// Close the dropdown
    fn close(&mut self, cx: &mut Context<Self>) {
        if self.is_open {
            self.is_open = false;
            cx.notify();
        }
    }

    /// Select an item by index and close dropdown
    fn select_item(&mut self, index: usize, cx: &mut Context<Self>) {
        self.set_selected_index(Some(index), cx);
        self.close(cx);
    }
}

impl<T: SelectItem> EventEmitter<SelectEvent> for SelectState<T> {}

impl<T: SelectItem> Render for SelectState<T> {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

/// Direction the dropdown menu opens
#[derive(Clone, Copy, PartialEq)]
pub enum DropdownDirection {
    Down,
    Up,
}

/// A Select dropdown element
#[derive(IntoElement)]
pub struct Select<T: SelectItem> {
    state: Entity<SelectState<T>>,
    placeholder: String,
    direction: DropdownDirection,
    style: StyleRefinement,
}

impl<T: SelectItem> Select<T> {
    /// Create a new Select component
    pub fn new(state: &Entity<SelectState<T>>) -> Self {
        Self {
            state: state.clone(),
            placeholder: "Select...".to_string(),
            direction: DropdownDirection::Down,
            style: StyleRefinement::default(),
        }
    }

    /// Set the placeholder text
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Set the dropdown direction
    pub fn direction(mut self, direction: DropdownDirection) -> Self {
        self.direction = direction;
        self
    }
}

impl<T: SelectItem> Styled for Select<T> {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl<T: SelectItem + 'static> RenderOnce for Select<T> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = self.state.read(cx);
        let is_open = state.is_open;
        let items = state.items().to_vec();
        let selected_index = state.selected_index();
        let selected_title = state
            .selected_item()
            .map(|item| item.display_title())
            .unwrap_or_else(|| self.placeholder.clone());

        let _ = state;

        div()
            .id(("select", self.state.entity_id()))
            .relative()
            .w_full()
            .child(
                // Main select button
                div()
                    .id("select-button")
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .py_1()
                    .bg(rgb(0x2d2d2d))
                    .border_1()
                    .border_color(rgb(0x444444))
                    .rounded(px(3.))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(0x353535)))
                    .on_mouse_down(
                        MouseButton::Left,
                        window.listener_for(&self.state, |state, _: &MouseDownEvent, _, cx| {
                            state.toggle_open(cx);
                        }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0xffffff))
                            .child(selected_title),
                    )
                    .child(
                        // Dropdown arrow
                        div().text_xs().text_color(rgb(0x999999)).child(if is_open {
                            match self.direction {
                                DropdownDirection::Down => "▲",
                                DropdownDirection::Up => "▼",
                            }
                        } else {
                            match self.direction {
                                DropdownDirection::Down => "▼",
                                DropdownDirection::Up => "▲",
                            }
                        }),
                    ),
            )
            .when(is_open, |el| {
                // Dropdown menu
                let menu = div()
                    .id("select-menu")
                    .absolute()
                    .left(px(0.))
                    .w_full()
                    .max_h(px(300.))
                    .overflow_y_scroll()
                    .bg(rgb(0x1a1a1a)) // Fully opaque dark background
                    .border_1()
                    .border_color(rgb(0x444444))
                    .rounded(px(4.))
                    .shadow_lg()
                    .occlude();

                let menu = match self.direction {
                    DropdownDirection::Down => menu.top_full().mt_1(),
                    DropdownDirection::Up => menu.bottom_full().mb_1(),
                };

                el.child(menu.children(items.iter().enumerate().map(|(idx, item)| {
                    let is_selected = selected_index == Some(idx);
                    let state_clone = self.state.clone();

                    div()
                        .id(("select-item", idx))
                        .px_2()
                        .py_1()
                        .cursor_pointer()
                        .w_full()
                        .opacity(1.0)
                        .bg(rgb(0x1a1a1a)) // Fully opaque background for all items
                        .when(is_selected, |style| style.bg(rgb(0x4caf50)))
                        .when(!is_selected, |style| style.hover(|s| s.bg(rgb(0x2d2d2d))))
                        .on_mouse_down(
                            MouseButton::Left,
                            window.listener_for(
                                &state_clone,
                                move |state, _: &MouseDownEvent, _, cx| {
                                    state.select_item(idx, cx);
                                },
                            ),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(0xffffff))
                                .child(item.display_title()),
                        )
                })))
            })
    }
}
