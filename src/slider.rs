//! Slider component adapted from gpui-component

use std::ops::Range;

use gpui::{
    Along, App, Axis, Bounds, Context, DragMoveEvent, Empty, Entity, EntityId, EventEmitter,
    IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, Point, Render, RenderOnce,
    StyleRefinement, Styled, Window, canvas, div, prelude::*, px, rgb,
};

#[derive(Clone)]
struct DragThumb((EntityId, bool));

impl Render for DragThumb {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

#[derive(Clone)]
struct DragSlider(EntityId);

impl Render for DragSlider {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

/// Events emitted by the [`SliderState`].
pub enum SliderEvent {
    Change(SliderValue),
}

/// The value of the slider (single value only for now)
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SliderValue {
    Single(f32),
}

impl From<f32> for SliderValue {
    fn from(value: f32) -> Self {
        SliderValue::Single(value)
    }
}

impl Default for SliderValue {
    fn default() -> Self {
        SliderValue::Single(0.)
    }
}

impl SliderValue {
    pub fn end(&self) -> f32 {
        match self {
            SliderValue::Single(value) => *value,
        }
    }

    fn set_end(&mut self, value: f32) {
        *self = SliderValue::Single(value);
    }
}

/// State of the [`Slider`].
pub struct SliderState {
    min: f32,
    max: f32,
    step: f32,
    value: SliderValue,
    percentage: Range<f32>,
    bounds: Bounds<Pixels>,
    hover_position: Option<f32>, // Percentage position of hover (0.0 to 1.0)
}

impl SliderState {
    pub fn new() -> Self {
        Self {
            min: 0.0,
            max: 100.0,
            step: 1.0,
            value: SliderValue::default(),
            percentage: (0.0..0.0),
            bounds: Bounds::default(),
            hover_position: None,
        }
    }

    pub fn min(mut self, min: f32) -> Self {
        self.min = min;
        self.update_thumb_pos();
        self
    }

    pub fn max(mut self, max: f32) -> Self {
        self.max = max;
        self.update_thumb_pos();
        self
    }

    pub fn set_max(&mut self, max: f32, _: &mut Window, cx: &mut Context<Self>) {
        self.max = max;
        self.update_thumb_pos();
        cx.notify();
    }

    pub fn step(mut self, step: f32) -> Self {
        self.step = step;
        self
    }

    pub fn default_value(mut self, value: impl Into<SliderValue>) -> Self {
        self.value = value.into();
        self.update_thumb_pos();
        self
    }

    pub fn set_value(
        &mut self,
        value: impl Into<SliderValue>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.value = value.into();
        self.update_thumb_pos();
        cx.notify();
    }

    pub fn get_max(&self) -> f32 {
        self.max
    }

    fn percentage_to_value(&self, percentage: f32) -> f32 {
        self.min + (self.max - self.min) * percentage
    }

    fn value_to_percentage(&self, value: f32) -> f32 {
        let range = self.max - self.min;
        if range <= 0.0 {
            0.0
        } else {
            (value - self.min) / range
        }
    }

    fn update_thumb_pos(&mut self) {
        match self.value {
            SliderValue::Single(value) => {
                let percentage = self.value_to_percentage(value.clamp(self.min, self.max));
                self.percentage = 0.0..percentage;
            }
        }
    }

    fn update_value_by_position(
        &mut self,
        axis: Axis,
        position: Point<Pixels>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let bounds = self.bounds;
        let step = self.step;

        let inner_pos = if matches!(axis, Axis::Horizontal) {
            position.x - bounds.left()
        } else {
            bounds.bottom() - position.y
        };
        let total_size = bounds.size.along(axis);
        let percentage = (inner_pos.clamp(px(0.), total_size) / total_size).clamp(0.0, 1.0);

        let value = self.percentage_to_value(percentage);
        let value = (value / step).round() * step;

        self.percentage.end = percentage;
        self.value.set_end(value);
        cx.emit(SliderEvent::Change(self.value));
        cx.notify();
    }

    fn update_hover_position(
        &mut self,
        axis: Axis,
        position: Point<Pixels>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let bounds = self.bounds;

        let inner_pos = if matches!(axis, Axis::Horizontal) {
            position.x - bounds.left()
        } else {
            bounds.bottom() - position.y
        };
        let total_size = bounds.size.along(axis);
        let percentage = (inner_pos.clamp(px(0.), total_size) / total_size).clamp(0.0, 1.0);
        if Some(percentage) != self.hover_position {
            self.hover_position = Some(percentage);
            cx.stop_propagation();
            cx.notify();
        }
    }

    pub fn clear_hover(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.hover_position = None;
        cx.notify();
    }
}

impl EventEmitter<SliderEvent> for SliderState {}
impl Render for SliderState {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

/// A horizontal Slider element.
#[derive(IntoElement)]
pub struct Slider {
    state: Entity<SliderState>,
    axis: Axis,
    style: StyleRefinement,
}

impl Slider {
    pub fn new(state: &Entity<SliderState>) -> Self {
        Self {
            axis: Axis::Horizontal,
            state: state.clone(),
            style: StyleRefinement::default(),
        }
    }

    pub fn horizontal(mut self) -> Self {
        self.axis = Axis::Horizontal;
        self
    }
}

impl Styled for Slider {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

fn format_time(seconds: f32) -> String {
    let total_seconds = seconds as i32;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let secs = total_seconds % 60;

    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, minutes, secs)
    } else {
        format!("{}:{:02}", minutes, secs)
    }
}

impl RenderOnce for Slider {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let axis = self.axis;
        let entity_id = self.state.entity_id();
        let state = self.state.read(cx);
        let bar_size = state.bounds.size.along(axis);
        let bar_end = state.percentage.end * bar_size;
        let hover_info = state
            .hover_position
            .map(|p| (p, state.percentage_to_value(p)));

        let bar_color = rgb(0x4caf50);
        let thumb_color = rgb(0xffffff);

        div()
            .id(("slider", self.state.entity_id()))
            .flex()
            .flex_1()
            .items_center()
            .justify_center()
            .w_full()
            .child(
                div()
                    .id("slider-bar-container")
                    .on_mouse_down(
                        MouseButton::Left,
                        window.listener_for(
                            &self.state,
                            move |state, e: &MouseDownEvent, window, cx| {
                                state.update_value_by_position(axis, e.position, window, cx)
                            },
                        ),
                    )
                    .on_mouse_move(window.listener_for(
                        &self.state,
                        move |state, e: &MouseMoveEvent, window, cx| {
                            state.update_hover_position(axis, e.position, window, cx);
                            cx.stop_propagation();
                        },
                    ))
                    .on_drag(DragSlider(entity_id), |drag, _, _, cx| {
                        cx.stop_propagation();
                        cx.new(|_| drag.clone())
                    })
                    .on_drag_move(window.listener_for(
                        &self.state,
                        move |view, e: &DragMoveEvent<DragSlider>, window, cx| match e.drag(cx) {
                            DragSlider(id) => {
                                if *id != entity_id {
                                    return;
                                }

                                view.update_value_by_position(axis, e.event.position, window, cx)
                            }
                        },
                    ))
                    .items_center()
                    .h_6()
                    .w_full()
                    .flex_shrink_0()
                    .child(
                        div()
                            .id("slider-bar")
                            .relative()
                            .w_full()
                            .h(px(6.0))
                            .bg(rgb(0x2d2d2d))
                            .rounded_full()
                            .child(
                                div()
                                    .absolute()
                                    .h_full()
                                    .left(px(0.0))
                                    .right(bar_size - bar_end)
                                    .bg(bar_color)
                                    .rounded_full(),
                            )
                            .child(
                                div()
                                    .id("slider-thumb")
                                    .absolute()
                                    .top(px(-5.))
                                    .left(bar_end)
                                    .ml(-px(8.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .flex_shrink_0()
                                    .rounded_full()
                                    .shadow_md()
                                    .size_4()
                                    .p(px(1.))
                                    .bg(rgb(0x555555))
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .size_full()
                                            .rounded_full()
                                            .bg(thumb_color),
                                    )
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation();
                                    })
                                    .on_drag(DragThumb((entity_id, false)), |drag, _, _, cx| {
                                        cx.stop_propagation();
                                        cx.new(|_| drag.clone())
                                    })
                                    .on_drag_move(window.listener_for(
                                        &self.state,
                                        move |view, e: &DragMoveEvent<DragThumb>, window, cx| {
                                            match e.drag(cx) {
                                                DragThumb((id, _)) => {
                                                    if *id != entity_id {
                                                        return;
                                                    }

                                                    view.update_value_by_position(
                                                        axis,
                                                        e.event.position,
                                                        window,
                                                        cx,
                                                    )
                                                }
                                            }
                                        },
                                    )),
                            )
                            .child({
                                let state = self.state.clone();
                                canvas(
                                    move |bounds, _, cx| state.update(cx, |r, _| r.bounds = bounds),
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full()
                            })
                            .when_some(hover_info, |el, (percentage, value)| {
                                let hover_pos = percentage * bar_size;
                                let time_text = format_time(value);

                                el.child(
                                    div()
                                        .id("slider-tooltip")
                                        .absolute()
                                        .bottom(px(20.))
                                        .left(hover_pos)
                                        .ml(-px(25.))
                                        .px_2()
                                        .py_1()
                                        .bg(rgb(0x1a1a1a))
                                        .border_1()
                                        .border_color(rgb(0x444444))
                                        .rounded(px(4.))
                                        .shadow_lg()
                                        .text_xs()
                                        .text_color(rgb(0xffffff))
                                        .whitespace_nowrap()
                                        .child(time_text),
                                )
                            }),
                    ),
            )
    }
}
