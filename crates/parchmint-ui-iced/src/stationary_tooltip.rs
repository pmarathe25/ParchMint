//! Cursor-positioned tooltips that only appear after a stationary hover.

use std::time::{Duration, Instant};

use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{Operation, Tree, tree},
};
use iced::widget::container;
use iced::{Element, Event, Length, Padding, Point, Rectangle, Size, Vector};

const DELAY: Duration = Duration::from_millis(600);
const PADDING: f32 = 5.0;

/// Wraps an icon-only control in a compact tooltip. The pointer must remain
/// still for a short interval; moving it hides the bubble until it re-enters.
pub(crate) fn tooltip<'a, Message>(
    content: impl Into<Element<'a, Message>>,
    bubble: impl Into<Element<'a, Message>>,
    style: container::Style,
) -> Element<'a, Message>
where
    Message: 'a,
{
    StationaryTooltip {
        content: content.into(),
        bubble: bubble.into(),
        style,
    }
    .into()
}

struct StationaryTooltip<'a, Message> {
    content: Element<'a, Message>,
    bubble: Element<'a, Message>,
    style: container::Style,
}

#[derive(Debug, Clone, Copy, Default)]
enum State {
    #[default]
    Idle,
    Hovered {
        at: Instant,
        point: Point,
    },
    Open {
        point: Point,
    },
    Suppressed,
}

impl<Message> Widget<Message, iced::Theme, iced::Renderer> for StationaryTooltip<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content), Tree::new(&self.bubble)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.content, &self.bubble]);
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
        if matches!(
            event,
            Event::Mouse(_) | Event::Window(iced::window::Event::RedrawRequested(_))
        ) {
            let state = tree.state.downcast_mut::<State>();
            let now = Instant::now();
            let point = cursor.position_over(layout.bounds());
            match (*state, point) {
                (State::Idle, Some(point)) => {
                    *state = State::Hovered { at: now, point };
                    shell.request_redraw_at(now + DELAY);
                }
                (State::Hovered { .. }, None)
                | (State::Open { .. }, None)
                | (State::Suppressed, None) => {
                    *state = State::Idle;
                    shell.invalidate_layout();
                }
                (
                    State::Hovered {
                        point: previous, ..
                    },
                    Some(point),
                ) if point != previous => {
                    *state = State::Hovered { at: now, point };
                    shell.request_redraw_at(now + DELAY);
                }
                (State::Hovered { at, .. }, Some(_)) if at.elapsed() < DELAY => {
                    shell.request_redraw_at(now + DELAY - at.elapsed());
                }
                (State::Hovered { .. }, Some(point)) => {
                    *state = State::Open { point };
                    shell.invalidate_layout();
                }
                (State::Open { point: previous }, Some(point)) if point != previous => {
                    *state = State::Suppressed;
                    shell.invalidate_layout();
                }
                (State::Open { .. }, Some(_))
                | (State::Suppressed, Some(_))
                | (State::Idle, None) => {}
            }
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
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
        let state = *tree.state.downcast_ref::<State>();
        let mut children = tree.children.iter_mut();
        let content = self.content.as_widget_mut().overlay(
            children.next().expect("tooltip content tree"),
            layout,
            renderer,
            viewport,
            translation,
        );
        let bubble = match state {
            State::Open { point } => Some(overlay::Element::new(Box::new(TooltipOverlay {
                bubble: &mut self.bubble,
                tree: children.next().expect("tooltip bubble tree"),
                point,
                style: self.style,
            }))),
            _ => None,
        };
        if content.is_some() || bubble.is_some() {
            Some(
                overlay::Group::with_children(content.into_iter().chain(bubble).collect())
                    .overlay(),
            )
        } else {
            None
        }
    }
}

struct TooltipOverlay<'a, 'b, Message> {
    bubble: &'b mut Element<'a, Message>,
    tree: &'b mut Tree,
    point: Point,
    style: container::Style,
}

impl<Message> overlay::Overlay<Message, iced::Theme, iced::Renderer>
    for TooltipOverlay<'_, '_, Message>
{
    fn layout(&mut self, renderer: &iced::Renderer, bounds: Size) -> layout::Node {
        let viewport = Rectangle::with_size(bounds);
        let bubble = self.bubble.as_widget_mut().layout(
            self.tree,
            renderer,
            &layout::Limits::new(Size::ZERO, viewport.size()).shrink(Padding::new(PADDING)),
        );
        let size = bubble.bounds().size();
        let mut bounds = Rectangle::new(
            Point::new(self.point.x, self.point.y - size.height - PADDING * 2.0),
            Size::new(size.width + PADDING * 2.0, size.height + PADDING * 2.0),
        );
        bounds.x = bounds
            .x
            .clamp(viewport.x, (viewport.width - bounds.width).max(viewport.x));
        bounds.y = bounds.y.clamp(
            viewport.y,
            (viewport.height - bounds.height).max(viewport.y),
        );
        layout::Node::with_children(
            bounds.size(),
            vec![bubble.translate(Vector::new(PADDING, PADDING))],
        )
        .translate(Vector::new(bounds.x, bounds.y))
    }

    fn draw(
        &self,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        inherited: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        container::draw_background(renderer, &self.style, layout.bounds());
        let style = renderer::Style {
            text_color: self.style.text_color.unwrap_or(inherited.text_color),
        };
        self.bubble.as_widget().draw(
            self.tree,
            renderer,
            theme,
            &style,
            layout.children().next().expect("tooltip child"),
            cursor,
            &Rectangle::with_size(Size::INFINITE),
        );
    }
}

impl<'a, Message> From<StationaryTooltip<'a, Message>> for Element<'a, Message>
where
    Message: 'a,
{
    fn from(tooltip: StationaryTooltip<'a, Message>) -> Self {
        Element::new(tooltip)
    }
}
