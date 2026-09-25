//! Let a scrollable repaint without rebuilding the application for every tick.

use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{Operation, Tree, tree},
};
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector};
use std::time::{Duration, Instant};

const SCROLL_FRAME: Duration = Duration::from_millis(16);

#[derive(Default)]
struct SmoothState {
    remaining_y: f32,
    inertia_y: f32,
    last_wheel: Option<Instant>,
    last_delta: f32,
    anchor: Option<Point>,
    shift: bool,
}

impl SmoothState {
    fn add_inertia(&mut self, delta: f32, now: Instant) {
        let continuing = self.last_wheel.is_some_and(|previous| {
            now.saturating_duration_since(previous) <= Duration::from_millis(140)
        }) && self.last_delta.signum() == delta.signum();
        self.inertia_y = if continuing {
            (self.inertia_y * 0.55 + delta * 0.18).clamp(-32.0, 32.0)
        } else if delta.abs() >= 90.0 {
            (delta * 0.12).clamp(-32.0, 32.0)
        } else {
            0.0
        };
        self.last_delta = delta;
        self.last_wheel = Some(now);
    }

    fn wheel(&mut self, delta: f32, now: Instant) {
        self.add_inertia(delta, now);
        self.remaining_y += delta;
    }

    fn pixel(&mut self, delta: f32, now: Instant) {
        self.remaining_y = 0.0;
        self.add_inertia(delta, now);
    }

    fn active(&self) -> bool {
        self.remaining_y.abs() > f32::EPSILON || self.inertia_y.abs() > f32::EPSILON
    }

    fn take_step(&mut self, now: Instant) -> f32 {
        let step = if self.remaining_y.abs() < 1.0 {
            self.remaining_y
        } else {
            self.remaining_y * 0.35
        };
        self.remaining_y -= step;
        let inertia = if self.last_wheel.is_some_and(|previous| {
            now.saturating_duration_since(previous) >= Duration::from_millis(48)
        }) {
            let velocity = self.inertia_y;
            self.inertia_y *= 0.86;
            if self.inertia_y.abs() < 0.75 {
                self.inertia_y = 0.0;
            }
            velocity
        } else {
            0.0
        };
        step + inertia
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
        let smooth_lines = match event {
            Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Lines { x, y },
            }) if *x == 0.0
                && *y != 0.0
                && !state.shift
                && !crate::motion::reduced()
                && cursor.is_over(layout.bounds()) =>
            {
                state.wheel(*y * 60.0, crate::motion::now());
                state.anchor = cursor.position();
                true
            }
            Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Pixels { y, .. },
            }) => {
                if cursor.is_over(layout.bounds()) && !crate::motion::reduced() {
                    state.pixel(*y, crate::motion::now());
                    state.anchor = cursor.position();
                } else {
                    state.remaining_y = 0.0;
                    state.inertia_y = 0.0;
                }
                false
            }
            _ => false,
        };
        let redraw_tick = matches!(
            event,
            Event::Window(iced::window::Event::RedrawRequested(_))
        );
        let step = if smooth_lines || redraw_tick && state.active() {
            let delta = state.take_step(crate::motion::now());
            (delta.abs() > f32::EPSILON).then_some(delta)
        } else {
            None
        };
        let anchor = state.anchor;
        let synthetic = step.map(|y| {
            Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Pixels { x: 0.0, y },
            })
        });
        let mut messages = Vec::new();
        let mut child_shell = Shell::new(&mut messages);
        if let Some(synthetic) = &synthetic {
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
    fn wheel_burst_keeps_moving_then_reversal_cancels_its_tail() {
        let start = Instant::now();
        let mut state = SmoothState::default();
        state.wheel(-60.0, start);
        assert_eq!(state.inertia_y, 0.0);
        state.take_step(start);
        state.wheel(-60.0, start + Duration::from_millis(40));
        assert!(state.inertia_y < 0.0);
        let tail = state.inertia_y;
        state.take_step(start + Duration::from_millis(100));
        assert!(state.inertia_y.abs() < tail.abs());
        state.wheel(60.0, start + Duration::from_millis(120));
        assert_eq!(state.inertia_y, 0.0);
    }

    #[test]
    fn one_large_wheel_flick_has_a_decaying_tail() {
        let start = Instant::now();
        let mut state = SmoothState::default();
        state.wheel(-180.0, start);
        assert!(state.inertia_y < 0.0);
        let first = state.take_step(start + Duration::from_millis(50));
        let second = state.take_step(start + Duration::from_millis(66));
        assert!(first < second);
        assert!(state.active());
    }

    #[test]
    fn a_large_pixel_flick_preserves_its_immediate_delta_and_adds_momentum() {
        let mut state = SmoothState::default();
        state.pixel(-120.0, Instant::now());
        assert_eq!(state.remaining_y, 0.0);
        assert!(state.inertia_y < 0.0);
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
