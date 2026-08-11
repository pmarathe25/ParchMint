use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{Operation, Tree, operation::Focusable, tree},
};
use iced::{Background, Border, Color, Element, Event, Length, Rectangle, Size, Vector, widget};

use crate::F6Region;

const REGION_IDS: [&str; 7] = [
    "parchmint-focus-mode-switch",
    "parchmint-focus-formatting-toolbar",
    "parchmint-focus-explorer",
    "parchmint-focus-tab-strip",
    "parchmint-focus-editor",
    "parchmint-focus-inspector",
    "parchmint-focus-status",
];
const MODAL_CANCEL_ID: &str = "parchmint-focus-modal-cancel";
const MODAL_CONFIRM_ID: &str = "parchmint-focus-modal-confirm";

pub(crate) fn region_id(region: F6Region) -> Option<widget::Id> {
    let index = match region {
        F6Region::None => return None,
        F6Region::ModeSwitch => 0,
        F6Region::FormattingToolbar => 1,
        F6Region::Explorer => 2,
        F6Region::ActiveTab => 3,
        F6Region::FocusedEditor => 4,
        F6Region::Inspector => 5,
        F6Region::StatusBar => 6,
    };
    Some(widget::Id::new(REGION_IDS[index]))
}

pub(crate) fn modal_cancel_id() -> widget::Id {
    widget::Id::new(MODAL_CANCEL_ID)
}

pub(crate) fn modal_confirm_id() -> widget::Id {
    widget::Id::new(MODAL_CONFIRM_ID)
}

pub(crate) fn region<'a, Message>(
    id: widget::Id,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message>
where
    Message: 'a,
{
    FocusableRegion {
        id,
        content: content.into(),
    }
    .into()
}

pub(crate) fn f6_region<'a, Message>(
    target: F6Region,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message>
where
    Message: 'a,
{
    region(
        region_id(target).expect("an F6 region always has a stable widget ID"),
        content,
    )
}

struct FocusableRegion<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer> {
    id: widget::Id,
    content: Element<'a, Message, Theme, Renderer>,
}

#[derive(Default)]
struct State {
    focused: bool,
}

impl Focusable for State {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for FocusableRegion<'_, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
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

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.focusable(
            Some(&self.id),
            layout.bounds(),
            tree.state.downcast_mut::<State>(),
        );
        operation.traverse(&mut |operation| {
            self.content.as_widget_mut().operate(
                &mut tree.children[0],
                layout,
                renderer,
                operation,
            );
        });
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
        if tree.state.downcast_ref::<State>().focused {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: layout.bounds(),
                    border: Border {
                        color: Color::from_rgb8(58, 132, 245),
                        width: 2.0,
                        radius: 3.0.into(),
                    },
                    ..renderer::Quad::default()
                },
                Background::Color(Color::TRANSPARENT),
            );
        }
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

impl<'a, Message, Theme, Renderer> From<FocusableRegion<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: renderer::Renderer + 'a,
{
    fn from(region: FocusableRegion<'a, Message, Theme, Renderer>) -> Self {
        Element::new(region)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_f6_region_has_a_distinct_stable_widget_id() {
        let regions = [
            F6Region::ModeSwitch,
            F6Region::FormattingToolbar,
            F6Region::Explorer,
            F6Region::ActiveTab,
            F6Region::FocusedEditor,
            F6Region::Inspector,
            F6Region::StatusBar,
        ];
        let ids = regions
            .into_iter()
            .map(|region| region_id(region).expect("focusable region ID"))
            .collect::<Vec<_>>();
        for (index, id) in ids.iter().enumerate() {
            assert!(ids[index + 1..].iter().all(|candidate| candidate != id));
        }
        assert_ne!(modal_cancel_id(), modal_confirm_id());
    }
}
