use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{
        Operation, Tree,
        operation::{Focusable, Outcome, Scrollable, TextInput},
        tree,
    },
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
        outline: true,
        input_enabled: None,
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
    FocusableRegion {
        id: region_id(target).expect("an F6 region always has a stable widget ID"),
        content: content.into(),
        outline: target != F6Region::FocusedEditor,
        input_enabled: None,
    }
    .into()
}

pub(crate) fn input_scope<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    enabled: bool,
) -> Element<'a, Message> {
    FocusableRegion {
        id: widget::Id::new("project-input-scope"),
        content: content.into(),
        outline: false,
        input_enabled: Some(enabled),
    }
    .into()
}

struct FocusableRegion<'a, Message, Renderer = iced::Renderer> {
    id: widget::Id,
    content: Element<'a, Message, iced::Theme, Renderer>,
    outline: bool,
    input_enabled: Option<bool>,
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

impl<Message, Renderer> Widget<Message, iced::Theme, Renderer>
    for FocusableRegion<'_, Message, Renderer>
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
        if self.input_enabled == Some(false) {
            return;
        }
        if self.input_enabled.is_none() {
            operation.focusable(
                Some(&self.id),
                layout.bounds(),
                tree.state.downcast_mut::<State>(),
            );
        }
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
        if self.input_enabled == Some(false) && !matches!(event, Event::Window(_)) {
            return;
        }
        if matches!(event, Event::Mouse(mouse::Event::ButtonPressed(_))) {
            tree.state.downcast_mut::<State>().focused = false;
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
        if self.outline && tree.state.downcast_ref::<State>().focused {
            renderer.fill_quad(
                renderer::Quad {
                    bounds: layout.bounds(),
                    border: Border {
                        color: theme.palette().primary,
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
    ) -> Option<overlay::Element<'b, Message, iced::Theme, Renderer>> {
        if self.input_enabled == Some(false) {
            return None;
        }
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

impl<'a, Message, Renderer> From<FocusableRegion<'a, Message, Renderer>>
    for Element<'a, Message, iced::Theme, Renderer>
where
    Message: 'a,
    Renderer: renderer::Renderer + 'a,
{
    fn from(region: FocusableRegion<'a, Message, Renderer>) -> Self {
        Element::new(region)
    }
}

pub(crate) fn reveal_text_input<Message: Send + 'static>(
    scroll_id: widget::Id,
    input_id: widget::Id,
) -> iced::Task<Message> {
    struct LocateInput {
        scroll_id: widget::Id,
        input_id: widget::Id,
        viewport: Option<(Rectangle, Vector)>,
        input: Option<Rectangle>,
    }

    impl Operation<f32> for LocateInput {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<f32>)) {
            operate(self);
        }

        fn scrollable(
            &mut self,
            id: Option<&widget::Id>,
            bounds: Rectangle,
            _content_bounds: Rectangle,
            translation: Vector,
            _state: &mut dyn Scrollable,
        ) {
            if id == Some(&self.scroll_id) {
                self.viewport = Some((bounds, translation));
            }
        }

        fn text_input(
            &mut self,
            id: Option<&widget::Id>,
            bounds: Rectangle,
            _state: &mut dyn TextInput,
        ) {
            if id == Some(&self.input_id) {
                self.input = Some(bounds);
            }
        }

        fn finish(&self) -> Outcome<f32> {
            let (Some((viewport, translation)), Some(input)) = (self.viewport, self.input) else {
                return Outcome::None;
            };
            let top = input.y - translation.y;
            let bottom = top + input.height;
            let delta = if top < viewport.y {
                top - viewport.y
            } else {
                (bottom - viewport.y - viewport.height).max(0.0)
            };
            Outcome::Some(delta)
        }
    }

    iced::advanced::widget::operate(LocateInput {
        scroll_id: scroll_id.clone(),
        input_id,
        viewport: None,
        input: None,
    })
    .then(move |y| {
        widget::operation::scroll_by(
            scroll_id.clone(),
            widget::operation::AbsoluteOffset { x: 0.0, y },
        )
    })
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
