//! Pointer-positioned secondary-click handling for context-menu targets.

use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{Operation, Tree, tree},
};
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector};

/// Wraps content and publishes a right-click in window coordinates.
///
/// `mouse_area` can publish a fixed right-press message, but obtaining the
/// pointer position through `on_move` rebuilds the whole project surface for
/// every mouse move. That both costs frames and can dismiss a just-opened
/// context menu before it is painted.
pub(crate) fn right_click_area<'a, Message>(
    content: impl Into<Element<'a, Message>>,
    on_right_press: impl Fn(Point) -> Message + 'a,
) -> Element<'a, Message>
where
    Message: 'a,
{
    RightClickArea {
        before_children: false,
        content: content.into(),
        on_right_press: Box::new(on_right_press),
    }
    .into()
}

pub(crate) fn before_right_click<'a, Message: 'a>(
    content: Element<'a, Message>,
    on_right_press: impl Fn(Point) -> Message + 'a,
) -> Element<'a, Message> {
    RightClickArea {
        content,
        on_right_press: Box::new(on_right_press),
        before_children: true,
    }
    .into()
}

struct RightClickArea<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    before_children: bool,
    on_right_press: Box<dyn Fn(Point) -> Message + 'a>,
}

#[derive(Default)]
struct PointerState {
    window_position: Option<Point>,
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for RightClickArea<'_, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<PointerState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(PointerState::default())
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
        renderer: &Renderer,
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
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if self.before_children
            && matches!(
                event,
                Event::Mouse(iced::mouse::Event::ButtonPressed(
                    iced::mouse::Button::Right
                ))
            )
        {
            shell.publish((self.on_right_press)(cursor.position().unwrap_or_default()));
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
        let state = tree.state.downcast_mut::<PointerState>();
        if let Event::Mouse(iced::mouse::Event::CursorMoved { position }) = event {
            // Scrollables translate the cursor, but leave event coordinates unchanged.
            state.window_position = Some(*position);
        }
        if matches!(
            event,
            Event::Mouse(iced::mouse::Event::ButtonPressed(
                iced::mouse::Button::Right
            ))
        ) && !self.before_children
            && !shell.is_event_captured()
            && let Some(point) = cursor
                .position_over(layout.bounds())
                .filter(|point| viewport.contains(*point))
        {
            shell.publish((self.on_right_press)(
                state.window_position.unwrap_or(point),
            ));
            shell.capture_event();
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
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
        renderer: &Renderer,
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
        renderer: &Renderer,
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
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message, Theme, Renderer> From<RightClickArea<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: renderer::Renderer + 'a,
{
    fn from(area: RightClickArea<'a, Message, Theme, Renderer>) -> Self {
        Element::new(area)
    }
}
