//! Stable pointer ownership for hierarchy drag sources and drop targets.
//!
//! Iced dispatches pointer events through the retained widget tree even when a
//! pointer has left an individual widget. Keeping the press state here lets a
//! source finish a drag outside its original row without making row layout
//! depend on transient drag state.

use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{Operation, Tree, tree},
};
use iced::{Color, Element, Event, Length, Point, Rectangle, Size, Vector};
use std::time::{Duration, Instant};

const DRAG_THRESHOLD: f32 = 4.0;
const DOUBLE_CLICK_INTERVAL: Duration = Duration::from_millis(500);

pub(crate) fn source<'a, Message>(
    content: impl Into<Element<'a, Message>>,
    on_click: Message,
    on_double_click: Option<Message>,
    on_drag_start: Message,
    on_finish: Message,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    HierarchyDragSource {
        content: content.into(),
        on_click,
        on_double_click,
        on_drag_start,
        on_finish,
    }
    .into()
}

/// Commits an inline field when the user presses outside its bounds.
///
/// Iced does not emit a text-input submit message merely because it loses
/// focus. Keeping this behavior next to the drag widgets lets the Explorer
/// retain a single editable control while treating click-away like Enter.
pub(crate) fn commit_on_click_away<'a, Message>(
    content: impl Into<Element<'a, Message>>,
    on_click_away: Message,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    CommitOnClickAway {
        content: content.into(),
        on_click_away,
    }
    .into()
}

pub(crate) fn target<'a, Message, Destination>(
    content: impl Into<Element<'a, Message>>,
    indicator: Option<DropIndicator>,
    destination_at: impl Fn(Rectangle, Point) -> Option<Destination> + 'a,
    on_target: impl Fn(Destination) -> Message + 'a,
    on_clear: impl Fn(Destination) -> Message + 'a,
) -> Element<'a, Message>
where
    Destination: Clone + PartialEq + 'a + 'static,
    Message: 'a,
{
    HierarchyDropTarget {
        content: content.into(),
        indicator,
        destination_at: Box::new(destination_at),
        on_target: Box::new(on_target),
        on_clear: Box::new(on_clear),
    }
    .into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DropIndicatorPosition {
    Before,
    Into,
    After,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DropIndicator {
    pub position: DropIndicatorPosition,
    pub color: Color,
}

struct HierarchyDragSource<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    on_click: Message,
    on_double_click: Option<Message>,
    on_drag_start: Message,
    on_finish: Message,
}

struct CommitOnClickAway<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    on_click_away: Message,
}

#[derive(Default)]
struct ClickAwayState {
    last_pointer: Option<Point>,
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for CommitOnClickAway<'_, Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: renderer::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<ClickAwayState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(ClickAwayState::default())
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
        let state = tree.state.downcast_mut::<ClickAwayState>();
        match event {
            Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                state.last_pointer = Some(*position);
            }
            Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left))
                if state
                    .last_pointer
                    .or_else(|| cursor.position())
                    .is_some_and(|position| !layout.bounds().contains(position)) =>
            {
                shell.publish(self.on_click_away.clone());
            }
            _ => {}
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

#[derive(Default)]
struct SourceState {
    last_pointer: Option<Point>,
    press_origin: Option<Point>,
    dragging: bool,
    last_click: Option<(Point, Instant)>,
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for HierarchyDragSource<'_, Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: renderer::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<SourceState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(SourceState::default())
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
        // Click and drag ownership belongs to this wrapper. Forwarding a
        // second press to a nested MouseArea lets Iced's double-click tracker
        // activate a document before the pointer has moved far enough to be
        // recognized as a drag. That makes a selected Cards document
        // disappear into Editor when an author starts to reorder it.
        let owns_left_pointer = {
            let state = tree.state.downcast_ref::<SourceState>();
            match event {
                Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)) => state
                    .last_pointer
                    .filter(|position| layout.bounds().contains(*position))
                    .or_else(|| cursor.position_over(layout.bounds()))
                    .is_some(),
                Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
                    state.press_origin.is_some()
                }
                _ => false,
            }
        };
        if !owns_left_pointer {
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

        let state = tree.state.downcast_mut::<SourceState>();
        match event {
            Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                state.last_pointer = Some(*position);
                if let Some(origin) = state.press_origin
                    && !state.dragging
                    && origin.distance(*position) >= DRAG_THRESHOLD
                {
                    state.dragging = true;
                    state.last_click = None;
                    shell.publish(self.on_drag_start.clone());
                }
            }
            Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)) => {
                let position = state
                    .last_pointer
                    .filter(|position| layout.bounds().contains(*position))
                    .or_else(|| cursor.position_over(layout.bounds()));
                if let Some(position) = position {
                    state.press_origin = Some(position);
                    state.dragging = false;
                }
            }
            Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left))
                if state.press_origin.take().is_some() =>
            {
                if state.dragging {
                    shell.publish(self.on_finish.clone());
                } else {
                    // A group click must not also run when that press became
                    // a drag. In particular, collapsing a Cards group before
                    // its drag begins can remove its child Cards from the
                    // virtualized surface mid-gesture.
                    shell.publish(self.on_click.clone());
                    let now = Instant::now();
                    let is_double_click = state.last_click.is_some_and(|(last, at)| {
                        now.duration_since(at) <= DOUBLE_CLICK_INTERVAL
                            && last.distance(state.last_pointer.unwrap_or(last)) < DRAG_THRESHOLD
                    });
                    if is_double_click {
                        state.last_click = None;
                        if let Some(on_double_click) = &self.on_double_click {
                            shell.publish(on_double_click.clone());
                        }
                    } else if self.on_double_click.is_some() {
                        state.last_click = state
                            .last_pointer
                            .or_else(|| cursor.position_over(layout.bounds()))
                            .map(|position| (position, now));
                    }
                }
                state.dragging = false;
            }
            _ => {}
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

impl<'a, Message, Theme, Renderer> From<HierarchyDragSource<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Theme: 'a,
    Renderer: renderer::Renderer + 'a,
{
    fn from(source: HierarchyDragSource<'a, Message, Theme, Renderer>) -> Self {
        Element::new(source)
    }
}

impl<'a, Message, Theme, Renderer> From<CommitOnClickAway<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Theme: 'a,
    Renderer: renderer::Renderer + 'a,
{
    fn from(commit: CommitOnClickAway<'a, Message, Theme, Renderer>) -> Self {
        Element::new(commit)
    }
}

struct HierarchyDropTarget<'a, Message, Destination, Theme = iced::Theme, Renderer = iced::Renderer>
where
    Destination: Clone + PartialEq,
{
    content: Element<'a, Message, Theme, Renderer>,
    indicator: Option<DropIndicator>,
    destination_at: Box<dyn Fn(Rectangle, Point) -> Option<Destination> + 'a>,
    on_target: Box<dyn Fn(Destination) -> Message + 'a>,
    on_clear: Box<dyn Fn(Destination) -> Message + 'a>,
}

struct TargetState<Destination> {
    left_down: bool,
    active_target: Option<Destination>,
}

impl<Destination> Default for TargetState<Destination> {
    fn default() -> Self {
        Self {
            left_down: false,
            active_target: None,
        }
    }
}

impl<Message, Destination, Theme, Renderer> Widget<Message, Theme, Renderer>
    for HierarchyDropTarget<'_, Message, Destination, Theme, Renderer>
where
    Destination: Clone + PartialEq + 'static,
    Renderer: renderer::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<TargetState<Destination>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(TargetState::<Destination>::default())
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

        let state = tree.state.downcast_mut::<TargetState<Destination>>();
        match event {
            Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)) => {
                state.left_down = true;
            }
            Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
                state.left_down = false;
                state.active_target = None;
            }
            Event::Mouse(iced::mouse::Event::CursorMoved { position }) if state.left_down => {
                let next = (self.destination_at)(layout.bounds(), *position);
                match (state.active_target.as_ref(), next) {
                    (Some(current), Some(next)) if current == &next => {}
                    (_, Some(next)) => {
                        state.active_target = Some(next.clone());
                        shell.publish((self.on_target)(next));
                    }
                    (Some(current), None) => {
                        let current = current.clone();
                        state.active_target = None;
                        shell.publish((self.on_clear)(current));
                    }
                    (None, None) => {}
                }
            }
            _ => {}
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
        let Some(indicator) = self.indicator else {
            return;
        };
        let bounds = layout.bounds();
        let bounds = match indicator.position {
            DropIndicatorPosition::Before => Rectangle {
                width: bounds.width,
                height: 2.0,
                ..bounds
            },
            DropIndicatorPosition::After => Rectangle {
                y: bounds.y + (bounds.height - 2.0).max(0.0),
                width: bounds.width,
                height: 2.0,
                ..bounds
            },
            DropIndicatorPosition::Into => Rectangle {
                x: bounds.x + 2.0,
                y: bounds.y + 2.0,
                width: (bounds.width - 4.0).max(0.0),
                height: (bounds.height - 4.0).max(0.0),
            },
        };
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: Default::default(),
                shadow: Default::default(),
                snap: true,
            },
            iced::Background::Color(indicator.color),
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

impl<'a, Message, Destination, Theme, Renderer>
    From<HierarchyDropTarget<'a, Message, Destination, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Destination: Clone + PartialEq + 'static,
    Message: 'a,
    Theme: 'a,
    Renderer: renderer::Renderer + 'a,
{
    fn from(target: HierarchyDropTarget<'a, Message, Destination, Theme, Renderer>) -> Self {
        Element::new(target)
    }
}
