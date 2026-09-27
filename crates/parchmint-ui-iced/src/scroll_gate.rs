//! Let a scrollable repaint without rebuilding the application for every tick.

use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{Operation, Tree, tree},
};
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector};
use std::time::{Duration, Instant};

thread_local! { static SYNTHETIC: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

pub(crate) fn without_momentum<T>(run: impl FnOnce() -> T) -> T {
    struct Reset(bool);
    impl Drop for Reset {
        fn drop(&mut self) {
            SYNTHETIC.set(self.0);
        }
    }
    let _reset = Reset(SYNTHETIC.replace(true));
    run()
}

const SCROLL_FRAME: Duration = Duration::from_millis(16);

#[derive(Default)]
struct MotionAxis {
    remaining: f32,
    inertia: f32,
    last_delta: f32,
    last_wheel: Option<Instant>,
    last_step: Option<Instant>,
}

impl MotionAxis {
    fn add_inertia(&mut self, delta: f32, now: Instant) {
        let continuing = self.last_wheel.is_some_and(|previous| {
            now.saturating_duration_since(previous) <= Duration::from_millis(180)
        }) && self.last_delta != 0.0
            && delta != 0.0
            && self.last_delta.signum() == delta.signum();
        self.inertia = if continuing {
            (self.inertia * 0.60 + delta * 0.35).clamp(-160.0, 160.0)
        } else {
            (delta * 0.30).clamp(-160.0, 160.0)
        };
        self.last_delta = delta;
        self.last_wheel = Some(now);
    }

    fn wheel(&mut self, delta: f32, now: Instant) {
        if self.remaining * delta < 0.0 {
            self.remaining = 0.0;
        }
        self.add_inertia(delta, now);
        self.remaining += delta;
    }

    fn pixel(&mut self, delta: f32, now: Instant) {
        self.remaining = 0.0;
        self.add_inertia(delta, now);
    }

    fn active(&self) -> bool {
        self.remaining.abs() > f32::EPSILON || self.inertia.abs() > f32::EPSILON
    }

    fn clear(&mut self) {
        self.remaining = 0.0;
        self.inertia = 0.0;
    }

    fn take_step(&mut self, now: Instant) -> f32 {
        let frames = self
            .last_step
            .replace(now)
            .map_or(1.0, |previous| {
                now.saturating_duration_since(previous).as_secs_f32() / SCROLL_FRAME.as_secs_f32()
            })
            .clamp(0.0, 4.0);
        let step = if self.remaining.abs() < 1.0 {
            self.remaining
        } else {
            self.remaining * (1.0 - 0.58_f32.powf(frames))
        };
        self.remaining -= step;
        let inertia = if self.last_wheel.is_some_and(|previous| {
            now.saturating_duration_since(previous) >= Duration::from_millis(32)
        }) {
            let decay = 0.86_f32.powf(frames);
            let velocity = self.inertia * (1.0 - decay) / (1.0 - 0.86);
            self.inertia *= decay;
            if self.inertia.abs() < 0.3 {
                self.inertia = 0.0;
            }
            velocity
        } else {
            0.0
        };
        step + inertia
    }
}

#[derive(Default)]
struct SmoothState {
    x: MotionAxis,
    y: MotionAxis,
    anchor: Option<Point>,
    shift: bool,
}

impl SmoothState {
    fn wheel(&mut self, delta: f32, now: Instant) {
        self.y.wheel(delta, now);
    }

    fn pixel(&mut self, delta: f32, now: Instant) {
        self.y.pixel(delta, now);
    }

    fn active(&self) -> bool {
        self.x.active() || self.y.active()
    }

    fn take_step(&mut self, now: Instant) -> f32 {
        self.y.take_step(now)
    }
}

/// Filters optional messages and smooths wheel input while forwarding widget state.
/// The scrollable uses `None` while its mounted rows still cover the viewport.
pub(crate) fn drop_none<'a, Message: 'a>(
    content: impl Into<Element<'a, Option<Message>>>,
) -> Element<'a, Message> {
    Element::new(DropNone {
        content: content.into(),
        // The scrollable's contents map every interactive message to `Some`.
        // Only its viewport notification may be `None`.
        overlay_message: |message| message.expect("overlays publish real messages"),
    })
}

/// Smooths line-wheel input while preserving child messages and pixel input.
pub(crate) fn smooth<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    drop_none(content.into().map(Some))
}

struct DropNone<'a, Message> {
    content: Element<'a, Option<Message>>,
    overlay_message: fn(Option<Message>) -> Message,
}

impl<Message> Widget<Message, iced::Theme, iced::Renderer> for DropNone<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<SmoothState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(SmoothState::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<SmoothState>();
        if let Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)) = event {
            state.shift = modifiers.shift();
        }
        if !crate::motion::enabled()
            || SYNTHETIC.get()
            || matches!(
                event,
                Event::Mouse(mouse::Event::ButtonPressed(_))
                    | Event::Keyboard(iced::keyboard::Event::KeyPressed { .. })
                    | Event::Window(iced::window::Event::Unfocused)
            )
        {
            state.x.clear();
            state.y.clear();
        }
        let smooth_lines = match event {
            Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Lines { x, y },
            }) if cursor.is_over(layout.bounds())
                && crate::motion::enabled()
                && !SYNTHETIC.get()
                && (*x != 0.0 || *y != 0.0) =>
            {
                let now = crate::motion::now();
                if state.shift {
                    let delta = if *x != 0.0 { *x } else { *y };
                    state.x.wheel(delta * 60.0, now);
                } else {
                    if *x != 0.0 {
                        state.x.wheel(*x * 60.0, now);
                    }
                    if *y != 0.0 {
                        state.wheel(*y * 60.0, now);
                    }
                }
                state.anchor = cursor.position();
                true
            }
            Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Pixels { x, y },
            }) => {
                if cursor.is_over(layout.bounds()) && crate::motion::enabled() && !SYNTHETIC.get() {
                    let now = crate::motion::now();
                    if *x != 0.0 {
                        state.x.pixel(*x, now);
                    }
                    if *y != 0.0 {
                        state.pixel(*y, now);
                    }
                    state.anchor = cursor.position();
                } else {
                    state.x.clear();
                    state.y.clear();
                }
                false
            }
            _ => false,
        };
        let redraw_tick = matches!(
            event,
            Event::Window(iced::window::Event::RedrawRequested(_))
        );
        let step_x = if smooth_lines || (redraw_tick && state.x.active()) {
            let delta = state.x.take_step(crate::motion::now());
            (delta.abs() > f32::EPSILON).then_some(delta)
        } else {
            None
        };
        let step_y = if smooth_lines || (redraw_tick && state.y.active()) {
            let delta = state.take_step(crate::motion::now());
            (delta.abs() > f32::EPSILON).then_some(delta)
        } else {
            None
        };
        let anchor = state.anchor;
        let synthetic = if step_x.is_some() || step_y.is_some() {
            Some(Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Pixels {
                    x: step_x.unwrap_or(0.0),
                    y: step_y.unwrap_or(0.0),
                },
            }))
        } else {
            None
        };
        let mut messages = Vec::new();
        let mut child_shell = Shell::new(&mut messages);
        if let Some(synthetic) = &synthetic {
            without_momentum(|| {
                self.content.as_widget_mut().update(
                    &mut tree.children[0],
                    synthetic,
                    layout,
                    anchor.map_or(cursor, mouse::Cursor::Available),
                    renderer,
                    clipboard,
                    &mut child_shell,
                    viewport,
                );
            });
        }
        if !smooth_lines {
            self.content.as_widget_mut().update(
                &mut tree.children[0],
                event,
                layout,
                cursor,
                renderer,
                clipboard,
                &mut child_shell,
                viewport,
            );
        }
        if state.active() {
            // Scrollable may publish a viewport update without capturing the
            // wheel event. Its capture status cannot decide whether to keep
            // the short, bounded momentum animation alive.
            shell.request_redraw_at(crate::motion::now() + SCROLL_FRAME);
        }
        if child_shell.is_event_captured() {
            shell.capture_event();
        }
        shell.request_redraw_at(child_shell.redraw_request());
        if child_shell.is_layout_invalid() {
            shell.invalidate_layout();
        }
        if child_shell.are_widgets_invalid() {
            shell.invalidate_widgets();
        }
        shell.input_method_mut().merge(child_shell.input_method());
        drop(child_shell);
        if messages.iter().any(Option::is_none) {
            shell.request_redraw();
        }
        for message in messages.into_iter().flatten() {
            shell.publish(message);
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        self.content
            .as_widget_mut()
            .overlay(
                &mut tree.children[0],
                layout,
                renderer,
                viewport,
                translation,
            )
            .map(|overlay| overlay.map(&self.overlay_message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::advanced::renderer::Headless;
    use iced::widget::{Space, scrollable};

    #[test]
    fn momentum_distance_is_independent_of_frame_rate() {
        let start = Instant::now();
        let distance = |interval: u64| {
            let mut axis = MotionAxis {
                inertia: 30.0,
                last_wheel: Some(start),
                last_step: Some(start + Duration::from_millis(32)),
                ..Default::default()
            };
            (1..=160 / interval)
                .map(|frame| axis.take_step(start + Duration::from_millis(32 + frame * interval)))
                .sum::<f32>()
        };
        assert!((distance(8) - distance(16)).abs() < 0.01);
        assert!((distance(16) - distance(32)).abs() < 0.01);
    }

    #[test]
    fn reversing_wheel_discards_unapplied_movement() {
        let start = Instant::now();
        let mut axis = MotionAxis::default();
        axis.wheel(-180.0, start);
        axis.wheel(60.0, start + Duration::from_millis(16));
        assert!(axis.take_step(start + Duration::from_millis(16)) > 0.0);
    }

    #[test]
    fn wheel_burst_keeps_moving_then_reversal_cancels_its_tail() {
        let start = Instant::now();
        let mut state = SmoothState::default();
        state.wheel(-60.0, start);
        assert!(state.y.inertia < 0.0);
        state.take_step(start);
        state.wheel(-60.0, start + Duration::from_millis(40));
        assert!(state.y.inertia < 0.0);
        let tail = state.y.inertia;
        state.take_step(start + Duration::from_millis(100));
        assert!(state.y.inertia.abs() < tail.abs());
        state.wheel(60.0, start + Duration::from_millis(120));
        assert!(state.y.inertia > 0.0);
    }

    #[test]
    fn upward_and_downward_flicks_have_symmetric_momentum() {
        let start = Instant::now();
        let mut down = SmoothState::default();
        down.wheel(-60.0, start);
        let mut up = SmoothState::default();
        up.wheel(60.0, start);
        assert!(down.y.inertia < 0.0);
        assert!(up.y.inertia > 0.0);
        assert!((down.y.inertia.abs() - up.y.inertia.abs()).abs() < f32::EPSILON);
        let down_step = down.take_step(start + Duration::from_millis(50));
        let up_step = up.take_step(start + Duration::from_millis(50));
        assert!(down_step < 0.0);
        assert!(up_step > 0.0);
        assert!((down_step.abs() - up_step.abs()).abs() < f32::EPSILON);
    }

    #[test]
    fn one_large_wheel_flick_has_a_decaying_tail() {
        let start = Instant::now();
        let mut state = SmoothState::default();
        state.wheel(-180.0, start);
        assert!(state.y.inertia < 0.0);
        let first = state.take_step(start + Duration::from_millis(50));
        let second = state.take_step(start + Duration::from_millis(66));
        assert!(first < second);
        assert!(state.active());
    }

    #[test]
    fn a_large_pixel_flick_preserves_its_immediate_delta_and_adds_momentum() {
        let mut state = SmoothState::default();
        state.pixel(-120.0, Instant::now());
        assert_eq!(state.y.remaining, 0.0);
        assert!(state.y.inertia < 0.0);
    }

    #[test]
    fn pixel_wheel_momentum_reaches_a_wrapped_scrollable() {
        crate::motion::set_reduced(false);
        let start = Instant::now();
        let _clock = crate::motion::FixedTime::new(start);
        let renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            iced::Font::DEFAULT,
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .unwrap();
        let mut element: Element<'_, f32> = drop_none(
            scrollable(Space::new().width(200).height(1000))
                .height(100)
                .on_scroll(|viewport| Some(viewport.absolute_offset().y)),
        );
        let mut tree = Tree::new(&element);
        let viewport = Rectangle::with_size(Size::new(200.0, 100.0));
        let node = element.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, viewport.size()),
        );
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        element.as_widget_mut().update(
            &mut tree,
            &Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Pixels { x: 0.0, y: -120.0 },
            }),
            Layout::new(&node),
            mouse::Cursor::Available(Point::new(50.0, 50.0)),
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut shell,
            &viewport,
        );
        assert_ne!(shell.redraw_request(), iced::window::RedrawRequest::Wait);
        drop(shell);
        assert!(messages.iter().any(|offset| *offset > 0.0), "{messages:?}");
        let immediate_offset = *messages.last().unwrap();
        messages.clear();
        let _later = crate::motion::FixedTime::new(start + Duration::from_millis(64));
        let mut shell = Shell::new(&mut messages);
        element.as_widget_mut().update(
            &mut tree,
            &Event::Window(iced::window::Event::RedrawRequested(
                start + Duration::from_millis(64),
            )),
            Layout::new(&node),
            mouse::Cursor::Available(Point::new(50.0, 50.0)),
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut shell,
            &viewport,
        );
        assert_ne!(shell.redraw_request(), iced::window::RedrawRequest::Wait);
        drop(shell);
        assert!(
            messages.iter().any(|offset| *offset > immediate_offset),
            "immediate={immediate_offset}, tail={messages:?}"
        );
    }
}
