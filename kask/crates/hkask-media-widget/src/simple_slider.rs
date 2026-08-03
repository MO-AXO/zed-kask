//! Lightweight GPUI-native slider — replaces the 618-dependency
//! `gpui-component` crate for transport controls.
//!
//! Supports linear and logarithmic scales. Emits `Change` while dragging
//! and `Release` on mouse up (seek-on-release semantics for media players).

use gpui::{
    AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, InteractiveElement,
    IntoElement, MouseButton, MouseMoveEvent, ParentElement, Pixels, Point, SharedString,
    StatefulInteractiveElement, Styled, Window, div, px,
};
use theme::ActiveTheme;

#[derive(Debug, Clone)]
pub enum SimpleSliderEvent {
    Change(f32),
    Release(f32),
}

pub struct SimpleSlider {
    focus_handle: FocusHandle,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    logarithmic: bool,
    is_dragging: bool,
}

impl SimpleSlider {
    pub fn new(cx: &mut Context<Self>, min: f32, max: f32, step: f32) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            value: min,
            min,
            max,
            step,
            logarithmic: false,
            is_dragging: false,
        }
    }

    pub fn logarithmic(mut self) -> Self {
        self.logarithmic = true;
        self
    }

    pub fn set_value(&mut self, value: f32, cx: &mut Context<Self>) {
        self.value = value.clamp(self.min, self.max);
        cx.notify();
    }

    fn fraction_from_value(&self) -> f32 {
        if (self.max - self.min).abs() < f32::EPSILON {
            return 0.0;
        }
        if self.logarithmic && self.min > 0.0 && self.max > 0.0 {
            let log_min = self.min.ln();
            let log_max = self.max.ln();
            ((self.value.ln() - log_min) / (log_max - log_min)).clamp(0.0, 1.0)
        } else {
            ((self.value - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
        }
    }

    fn value_from_fraction(&self, fraction: f32) -> f32 {
        let fraction = fraction.clamp(0.0, 1.0);
        let raw = if self.logarithmic && self.min > 0.0 && self.max > 0.0 {
            let log_min = self.min.ln();
            let log_max = self.max.ln();
            (log_min + fraction * (log_max - log_min)).exp()
        } else {
            self.min + fraction * (self.max - self.min)
        };
        let stepped = (raw / self.step).round() * self.step;
        stepped.clamp(self.min, self.max)
    }

    fn fraction_from_x(&self, click_x: Pixels, track_start: Pixels, track_width: Pixels) -> f32 {
        if track_width <= px(0.0) {
            return 0.0;
        }
        ((click_x - track_start) / track_width)
            .clamp(0.0, 1.0)
            .into()
    }
}

impl EventEmitter<SimpleSliderEvent> for SimpleSlider {}

impl Focusable for SimpleSlider {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl gpui::Render for SimpleSlider {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let fraction = self.fraction_from_value();
        let theme = cx.theme().clone();
        let track_color = theme.colors().scrollbar_track_background;
        let fill_color = theme.colors().text_accent;
        let thumb_color = theme.colors().scrollbar_thumb_background;
        let thumb_hover_color = theme.colors().scrollbar_thumb_hover_background;
        let is_dragging = self.is_dragging;
        let entity = cx.entity().downgrade();

        div()
            .id("simple-slider-track")
            .flex_1()
            .h(px(6.0))
            .rounded(px(3.0))
            .bg(track_color)
            .cursor_pointer()
            .relative()
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                let Some(entity) = entity.upgrade() else {
                    return;
                };
                let bounds = cx.bounds();
                let track_start = bounds.left();
                let track_width = bounds.width();
                entity.update(cx, |slider, cx| {
                    let fraction =
                        slider.fraction_from_x(event.position.x, track_start, track_width);
                    let value = slider.value_from_fraction(fraction);
                    slider.value = value;
                    slider.is_dragging = true;
                    cx.emit(SimpleSliderEvent::Change(value));
                    cx.notify();
                });
            })
            .on_drag_move(move |event, cx| {
                let Some(entity) = entity.upgrade() else {
                    return;
                };
                entity.update(cx, |slider, cx| {
                    if !slider.is_dragging {
                        return;
                    }
                    let bounds = cx.bounds();
                    let track_start = bounds.left();
                    let track_width = bounds.width();
                    let fraction =
                        slider.fraction_from_x(event.drag.position.x, track_start, track_width);
                    let value = slider.value_from_fraction(fraction);
                    slider.value = value;
                    cx.emit(SimpleSliderEvent::Change(value));
                    cx.notify();
                });
            })
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                let Some(entity) = entity.upgrade() else {
                    return;
                };
                entity.update(cx, |slider, cx| {
                    if slider.is_dragging {
                        slider.is_dragging = false;
                        cx.emit(SimpleSliderEvent::Release(slider.value));
                        cx.notify();
                    }
                });
            })
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .h_full()
                    .w(px(fraction * 100.0))
                    .min_w(px(4.0))
                    .rounded(px(3.0))
                    .bg(fill_color),
            )
            .child(
                div()
                    .absolute()
                    .top(px(-3.0))
                    .left(px(fraction * 100.0))
                    .ml(px(-6.0))
                    .size(px(12.0))
                    .rounded(px(6.0))
                    .bg(if is_dragging {
                        thumb_hover_color
                    } else {
                        thumb_color
                    })
                    .border_1()
                    .border_color(fill_color),
            )
    }
}
