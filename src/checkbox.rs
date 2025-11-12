//! Checkbox component
//!
//! A simple checkbox component for toggling boolean values.

use gpui::{
    div, prelude::*, px, rgb, App, Context, Empty, Entity, EventEmitter, IntoElement, MouseButton,
    MouseDownEvent, Render, RenderOnce, StyleRefinement, Styled, Window,
};

/// Events emitted by the CheckboxState
#[derive(Clone)]
pub enum CheckboxEvent {
    /// Fired when the checkbox state changes
    Change(bool),
}

/// State of the Checkbox component
pub struct CheckboxState {
    checked: bool,
}

impl CheckboxState {
    /// Create a new CheckboxState
    pub fn new(checked: bool) -> Self {
        Self { checked }
    }

    /// Set the checked state
    pub fn set_checked(&mut self, checked: bool, cx: &mut Context<Self>) {
        if checked != self.checked {
            self.checked = checked;
            cx.emit(CheckboxEvent::Change(checked));
            cx.notify();
        }
    }

    /// Get the checked state
    pub fn is_checked(&self) -> bool {
        self.checked
    }

    /// Toggle the checked state
    fn toggle(&mut self, cx: &mut Context<Self>) {
        self.set_checked(!self.checked, cx);
    }
}

impl EventEmitter<CheckboxEvent> for CheckboxState {}

impl Render for CheckboxState {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

/// A Checkbox element
#[derive(IntoElement)]
pub struct Checkbox {
    state: Entity<CheckboxState>,
    label: Option<String>,
    style: StyleRefinement,
}

impl Checkbox {
    /// Create a new Checkbox component
    pub fn new(state: &Entity<CheckboxState>) -> Self {
        Self {
            state: state.clone(),
            label: None,
            style: StyleRefinement::default(),
        }
    }

    /// Set the label text
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl Styled for Checkbox {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Checkbox {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = self.state.read(cx);
        let is_checked = state.is_checked();
        let _ = state;

        div()
            .id(("checkbox", self.state.entity_id()))
            .flex()
            .items_center()
            .gap_2()
            .child(
                // Checkbox box
                div()
                    .id("checkbox-box")
                    .size(px(18.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(if is_checked {
                        rgb(0x4caf50)
                    } else {
                        rgb(0x2d2d2d)
                    })
                    .border_1()
                    .border_color(rgb(0x444444))
                    .rounded(px(3.))
                    .cursor_pointer()
                    .hover(|style| {
                        style.bg(if is_checked {
                            rgb(0x66bb6a)
                        } else {
                            rgb(0x353535)
                        })
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        window.listener_for(&self.state, |state, _: &MouseDownEvent, _, cx| {
                            state.toggle(cx);
                        }),
                    )
                    .when(is_checked, |el| {
                        el.child(
                            div()
                                .text_sm()
                                .text_color(rgb(0xffffff))
                                .child("✓"),
                        )
                    }),
            )
            .when_some(self.label, |el, label| {
                el.child(
                    div()
                        .text_sm()
                        .text_color(rgb(0xcccccc))
                        .child(label),
                )
            })
    }
}
