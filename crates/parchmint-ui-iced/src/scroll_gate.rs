//! Let a scrollable repaint without rebuilding the application for every tick.

use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{Operation, Tree},
};
use iced::{Element, Event, Length, Rectangle, Size, Vector};

/// Filters optional messages from a child while forwarding its widget state.
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

struct DropNone<'a, Message> {
    content: Element<'a, Option<Message>>,
    overlay_message: fn(Option<Message>) -> Message,
}

impl<Message> Widget<Message, iced::Theme, iced::Renderer> for DropNone<'_, Message> {
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
        let mut messages = Vec::new();
        let mut child_shell = Shell::new(&mut messages);
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
