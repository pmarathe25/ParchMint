//! Popovers anchored to editor geometry, with one continuous hover region.
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{Operation, Tree},
};
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dismissal {
    Explicit,
    OutsideClick,
    Hover,
}

pub(crate) fn anchored<'a, M: Clone + 'a>(
    body: Element<'a, M>,
    card: Element<'a, M>,
    anchor: crate::Rect,
    dismissal: Dismissal,
    dismiss: M,
) -> Element<'a, M> {
    Element::new(Anchored {
        body,
        card,
        anchor,
        dismissal,
        dismiss,
    })
}
struct Anchored<'a, M> {
    body: Element<'a, M>,
    card: Element<'a, M>,
    anchor: crate::Rect,
    dismissal: Dismissal,
    dismiss: M,
}
impl<M: Clone> Widget<M, iced::Theme, iced::Renderer> for Anchored<'_, M> {
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.body), Tree::new(&self.card)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[&self.body, &self.card]);
    }
    fn size(&self) -> Size<Length> {
        self.body.as_widget().size()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        r: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.body
            .as_widget_mut()
            .layout(&mut tree.children[0], r, limits)
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        r: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        self.body.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            r,
            clipboard,
            shell,
            viewport,
        );
    }
    fn draw(
        &self,
        tree: &Tree,
        r: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.body
            .as_widget()
            .draw(&tree.children[0], r, theme, style, layout, cursor, viewport);
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        r: &iced::Renderer,
    ) -> mouse::Interaction {
        self.body
            .as_widget()
            .mouse_interaction(&tree.children[0], layout, cursor, viewport, r)
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        r: &iced::Renderer,
        op: &mut dyn Operation,
    ) {
        self.body
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, r, op);
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        _r: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, iced::Theme, iced::Renderer>> {
        let origin = layout.position() + translation;
        let anchor = Rectangle {
            x: origin.x + self.anchor.left(),
            y: origin.y + self.anchor.top(),
            width: self.anchor.width(),
            height: self.anchor.height(),
        };
        Some(overlay::Element::new(Box::new(Popover {
            card: &mut self.card,
            tree: &mut tree.children[1],
            anchor,
            viewport: *viewport,
            dismissal: self.dismissal,
            dismiss: self.dismiss.clone(),
        })))
    }
}
struct Popover<'a, 'b, M> {
    card: &'a mut Element<'b, M>,
    tree: &'a mut Tree,
    anchor: Rectangle,
    viewport: Rectangle,
    dismissal: Dismissal,
    dismiss: M,
}
impl<M: Clone> overlay::Overlay<M, iced::Theme, iced::Renderer> for Popover<'_, '_, M> {
    fn layout(&mut self, r: &iced::Renderer, bounds: Size) -> layout::Node {
        let node = self.card.as_widget_mut().layout(
            self.tree,
            r,
            &layout::Limits::new(Size::ZERO, bounds),
        );
        let x = self.anchor.x.clamp(
            self.viewport.x,
            (self.viewport.x + self.viewport.width - node.size().width).max(self.viewport.x),
        );
        let below = self.anchor.y + self.anchor.height + 6.0;
        let y = if below + node.size().height <= self.viewport.y + self.viewport.height {
            below
        } else {
            (self.anchor.y - node.size().height - 6.0).max(self.viewport.y)
        };
        node.move_to(Point::new(x, y))
    }
    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        r: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
    ) {
        self.card.as_widget_mut().update(
            self.tree,
            event,
            layout,
            cursor,
            r,
            clipboard,
            shell,
            &self.viewport,
        );
        let card = layout.bounds();
        let union = self.anchor.union(&card);
        let (top, bottom) = if card.y >= self.anchor.y + self.anchor.height {
            (self.anchor.y + self.anchor.height, card.y)
        } else {
            (card.y + card.height, self.anchor.y)
        };
        let bridge = Rectangle {
            x: union.x,
            y: top,
            width: union.width,
            height: (bottom - top).max(0.0),
        };
        let outside = cursor
            .position()
            .is_some_and(|p| !self.anchor.contains(p) && !card.contains(p) && !bridge.contains(p));
        if !shell.is_event_captured()
            && ((self.dismissal == Dismissal::Hover
                && outside
                && matches!(event, Event::Mouse(mouse::Event::CursorMoved { .. })))
                || (self.dismissal != Dismissal::Explicit
                    && !cursor.is_over(layout.bounds())
                    && matches!(
                        event,
                        Event::Mouse(mouse::Event::ButtonPressed(
                            mouse::Button::Left | mouse::Button::Right
                        ))
                    )))
        {
            shell.publish(self.dismiss.clone());
        }
        // Empty space and disabled controls belong to the popover too. Never
        // let their pointer events place a caret in the document underneath.
        if cursor.is_over(card) && matches!(event, Event::Mouse(_)) {
            shell.capture_event();
        }
    }
    fn draw(
        &self,
        r: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.card
            .as_widget()
            .draw(self.tree, r, theme, style, layout, cursor, &self.viewport);
    }
    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        r: &iced::Renderer,
    ) -> mouse::Interaction {
        self.card
            .as_widget()
            .mouse_interaction(self.tree, layout, cursor, &self.viewport, r)
    }
    fn operate(&mut self, layout: Layout<'_>, r: &iced::Renderer, op: &mut dyn Operation) {
        self.card.as_widget_mut().operate(self.tree, layout, r, op);
    }
    fn overlay<'c>(
        &'c mut self,
        layout: Layout<'c>,
        r: &iced::Renderer,
    ) -> Option<overlay::Element<'c, M, iced::Theme, iced::Renderer>> {
        self.card
            .as_widget_mut()
            .overlay(self.tree, layout, r, &self.viewport, Vector::ZERO)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Settings;
    use iced::widget::{Space, button, container, text};
    use iced_test::Simulator;

    #[test]
    fn hover_bridge_and_card_keep_the_anchor_stable_then_dismiss_outside() {
        let card = container(button(text("Reply")).on_press("reply"))
            .width(220)
            .height(120);
        let mut simulator = Simulator::with_size(
            Settings::default(),
            Size::new(500.0, 400.0),
            anchored(
                Space::new().width(Length::Fill).height(Length::Fill).into(),
                card.into(),
                crate::Rect::new(50.0, 60.0, 100.0, 20.0),
                Dismissal::Hover,
                "dismiss",
            ),
        );
        let initial = simulator.find("Reply").unwrap().bounds();
        for point in [
            Point::new(80.0, 83.0),
            initial.center(),
            Point::new(200.0, 180.0),
        ] {
            simulator.point_at(point);
            simulator.simulate([Event::Mouse(mouse::Event::CursorMoved { position: point })]);
            assert_eq!(simulator.find("Reply").unwrap().bounds(), initial);
        }
        let point = Point::new(450.0, 350.0);
        simulator.point_at(point);
        simulator.simulate([Event::Mouse(mouse::Event::CursorMoved { position: point })]);
        assert_eq!(
            simulator.into_messages().collect::<Vec<_>>(),
            vec!["dismiss"]
        );
    }
    #[test]
    fn disabled_controls_and_blank_popover_space_do_not_click_the_document() {
        let body = button(text("Underlying editor"))
            .width(Length::Fill)
            .height(Length::Fill)
            .on_press("document-click");
        let card = container(button(text("Disabled"))).width(220).height(120);
        let mut simulator = Simulator::with_size(
            Settings::default(),
            Size::new(500.0, 400.0),
            anchored(
                body.into(),
                card.into(),
                crate::Rect::new(50.0, 60.0, 100.0, 20.0),
                Dismissal::OutsideClick,
                "dismiss",
            ),
        );
        simulator.click("Disabled").unwrap();
        simulator.point_at(Point::new(200.0, 180.0));
        simulator.simulate(iced_test::simulator::click());
        assert!(simulator.into_messages().next().is_none());
    }
}
