//! Hierarchy drag gestures and one hover decision per rendered surface.

use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{Operation, Tree, tree},
};
use iced::{Color, Element, Event, Length, Point, Rectangle, Size, Vector};
use std::time::{Duration, Instant};
use std::{cell::RefCell, rc::Rc};

const DRAG_THRESHOLD: f32 = 4.0;
const DOUBLE_CLICK_INTERVAL: Duration = Duration::from_millis(500);

pub(crate) fn source<'a, Message>(
    id: impl Into<String>,
    content: impl Into<Element<'a, Message>>,
    on_click: Message,
    on_double_click: Option<Message>,
    on_drag_start: Message,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    HierarchyDragSource {
        id: id.into(),
        content: content.into(),
        on_click,
        on_double_click,
        on_drag_start: Box::new(move |_, _| on_drag_start.clone()),
    }
    .into()
}

pub(crate) fn source_with_pointer<'a, Message: Clone + 'a>(
    id: impl Into<String>,
    content: impl Into<Element<'a, Message>>,
    on_click: Message,
    on_double_click: Option<Message>,
    on_drag_start: impl Fn(Point, Rectangle) -> Message + 'a,
) -> Element<'a, Message> {
    HierarchyDragSource {
        id: id.into(),
        content: content.into(),
        on_click,
        on_double_click,
        on_drag_start: Box::new(on_drag_start),
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
    commit_on_click_away_maybe(content, Some(on_click_away))
}

/// Keeps a field's widget tree stable as its pending edit changes.
pub(crate) fn commit_on_click_away_maybe<'a, Message>(
    content: impl Into<Element<'a, Message>>,
    on_click_away: Option<Message>,
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

pub(crate) type HoverTargets<D> = Rc<RefCell<Option<(D, Rectangle)>>>;
type DropResolver<'a, D> = dyn Fn(Rectangle, Point) -> Option<(D, Rectangle)> + 'a;

pub(crate) fn targets<D>() -> HoverTargets<D> {
    Rc::new(RefCell::new(None))
}

pub(crate) fn target<'a, Message, Destination>(
    content: impl Into<Element<'a, Message>>,
    indicator: Option<DropIndicator>,
    targets: &HoverTargets<Destination>,
    destination_at: impl Fn(Rectangle, Point) -> Option<Destination> + 'a,
) -> Element<'a, Message>
where
    Destination: Clone + PartialEq + 'a + 'static,
    Message: 'a,
{
    target_with_zone(content, indicator, targets, move |bounds, point| {
        destination_at(bounds, point).map(|destination| (destination, bounds))
    })
}

pub(crate) fn target_with_zone<'a, Message: 'a, Destination: Clone + PartialEq + 'static>(
    content: impl Into<Element<'a, Message>>,
    indicator: Option<DropIndicator>,
    targets: &HoverTargets<Destination>,
    destination_at: impl Fn(Rectangle, Point) -> Option<(Destination, Rectangle)> + 'a,
) -> Element<'a, Message> {
    HierarchyDropTarget {
        content: content.into(),
        indicator,
        targets: Rc::clone(targets),
        destination_at: Box::new(destination_at),
    }
    .into()
}

pub(crate) fn surface<'a, Message, Destination>(
    content: impl Into<Element<'a, Message>>,
    targets: HoverTargets<Destination>,
    active: bool,
    stabilize: bool,
    on_hover: impl Fn(Option<Destination>) -> Message + 'a,
    on_leave: Message,
) -> Element<'a, Message>
where
    Destination: Clone + PartialEq + 'a + 'static,
    Message: Clone + 'a,
{
    HierarchyDragSurface {
        content: content.into(),
        targets,
        active,
        stabilize,
        on_hover: Box::new(on_hover),
        on_leave,
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
    id: String,
    content: Element<'a, Message, Theme, Renderer>,
    on_click: Message,
    on_double_click: Option<Message>,
    on_drag_start: Box<dyn Fn(Point, Rectangle) -> Message + 'a>,
}

struct CommitOnClickAway<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    on_click_away: Option<Message>,
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for CommitOnClickAway<'_, Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: renderer::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::stateless()
    }

    fn state(&self) -> tree::State {
        tree::State::None
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
        if matches!(
            event,
            Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left))
        ) && !cursor.is_over(layout.bounds())
            && let Some(message) = &self.on_click_away
        {
            shell.publish(message.clone());
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
    id: String,
    last_pointer: Option<Point>,
    press_origin: Option<Point>,
    grab_offset: Vector,
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
        tree::State::new(SourceState {
            id: self.id.clone(),
            ..Default::default()
        })
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        let state = tree.state.downcast_mut::<SourceState>();
        if state.id != self.id {
            *state = SourceState {
                id: self.id.clone(),
                ..Default::default()
            };
        }
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
        // Nested MouseAreas must not open a document before a drag starts.
        let owns_left_pointer = {
            let state = tree.state.downcast_ref::<SourceState>();
            match event {
                Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)) => {
                    cursor
                        .position_over(layout.bounds())
                        .filter(|position| viewport.contains(*position))
                        .is_some()
                }
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
            Event::Mouse(iced::mouse::Event::CursorLeft)
            | Event::Window(iced::window::Event::Unfocused) => {
                state.press_origin = None;
                state.dragging = false;
                state.last_pointer = None;
            }
            Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                state.last_pointer = Some(*position);
                if let Some(origin) = state.press_origin
                    && !state.dragging
                    && origin.distance(*position) >= DRAG_THRESHOLD
                {
                    state.dragging = true;
                    state.last_click = None;
                    shell.publish((self.on_drag_start)(
                        layout.position() + state.grab_offset,
                        layout.bounds(),
                    ));
                }
            }
            Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)) => {
                let position = cursor
                    .position_over(layout.bounds())
                    .filter(|position| viewport.contains(*position));
                if let Some(position) = position {
                    state.press_origin = Some(position);
                    state.grab_offset = position - layout.position();
                    state.dragging = false;
                }
            }
            Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left))
                if state.press_origin.take().is_some() =>
            {
                if !state.dragging {
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
        let child = self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        );
        if child == mouse::Interaction::default() && cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            child
        }
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
    destination_at: Box<DropResolver<'a, Destination>>,
    targets: HoverTargets<Destination>,
}

impl<Message, Destination, Theme, Renderer> Widget<Message, Theme, Renderer>
    for HierarchyDropTarget<'_, Message, Destination, Theme, Renderer>
where
    Destination: Clone + PartialEq + 'static,
    Renderer: renderer::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::stateless()
    }

    fn state(&self) -> tree::State {
        tree::State::None
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

        if let Some((destination, zone, point)) = cursor
            .position()
            .filter(|point| viewport.contains(*point))
            .and_then(|point| {
                (self.destination_at)(layout.bounds(), point)
                    .map(|(destination, zone)| (destination, zone, point))
            })
        {
            let mut target = self.targets.borrow_mut();
            if target.is_none() {
                *target = Some((
                    destination,
                    Rectangle {
                        x: zone.x - point.x,
                        y: zone.y - point.y,
                        ..zone
                    },
                ));
            }
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

#[derive(Default)]
struct SurfaceState {
    inside: bool,
    position: Option<Point>,
    scrolled: bool,
    accepted_zone: Option<Rectangle>,
}

struct HierarchyDragSurface<
    'a,
    Message,
    Destination,
    Theme = iced::Theme,
    Renderer = iced::Renderer,
> {
    content: Element<'a, Message, Theme, Renderer>,
    targets: HoverTargets<Destination>,
    active: bool,
    stabilize: bool,
    on_hover: Box<dyn Fn(Option<Destination>) -> Message + 'a>,
    on_leave: Message,
}

impl<Message, Destination, Theme, Renderer> Widget<Message, Theme, Renderer>
    for HierarchyDragSurface<'_, Message, Destination, Theme, Renderer>
where
    Destination: Clone + PartialEq + 'static,
    Message: Clone,
    Renderer: renderer::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<SurfaceState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(SurfaceState::default())
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
        *self.targets.borrow_mut() = None;
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
        let state = tree.state.downcast_mut::<SurfaceState>();
        if !self.active {
            *state = SurfaceState::default();
            return;
        }
        if matches!(
            event,
            Event::Mouse(iced::mouse::Event::WheelScrolled { .. })
        ) {
            // Hit test again after Scrollable applies the new offset.
            state.scrolled = true;
            return;
        }
        let after_scroll = state.scrolled
            && matches!(
                event,
                Event::Window(iced::window::Event::RedrawRequested(_))
            );
        if !matches!(
            event,
            Event::Mouse(
                iced::mouse::Event::CursorMoved { .. }
                    | iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)
            )
        ) && !after_scroll
        {
            return;
        }
        let point = cursor
            .position()
            .filter(|point| layout.bounds().contains(*point) && viewport.contains(*point));
        if let Some(point) = point {
            let moved = state
                .position
                .is_none_or(|previous| previous.distance(point) >= DRAG_THRESHOLD);
            if (moved || state.scrolled)
                && (!self.stabilize
                    || state.scrolled
                    || state.accepted_zone.is_none_or(|zone| !zone.contains(point)))
            {
                let candidate = self.targets.borrow_mut().take();
                state.accepted_zone = candidate.as_ref().map(|(_, zone)| Rectangle {
                    x: zone.x + point.x,
                    y: zone.y + point.y,
                    ..*zone
                });
                shell.publish((self.on_hover)(
                    candidate.map(|(destination, _)| destination),
                ));
                state.position = Some(point);
            }
            state.inside = true;
            state.scrolled = false;
        } else if state.inside {
            shell.publish(self.on_leave.clone());
            *state = SurfaceState::default();
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
        if self.active && cursor.position_over(layout.bounds()).is_some() {
            return mouse::Interaction::Grabbing;
        }
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
    From<HierarchyDragSurface<'a, Message, Destination, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Destination: Clone + PartialEq + 'static,
    Theme: 'a,
    Renderer: renderer::Renderer + 'a,
{
    fn from(surface: HierarchyDragSurface<'a, Message, Destination, Theme, Renderer>) -> Self {
        Element::new(surface)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::advanced::renderer::Headless;

    #[test]
    fn live_drop_target_stays_put_until_the_pointer_leaves_its_zone() {
        use iced::advanced::renderer::Headless;
        let renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            iced::Font::DEFAULT,
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .unwrap();
        let make = |destination: &'static str| {
            let targets = targets();
            surface(
                target_with_zone(
                    iced::widget::Space::new().width(300).height(180),
                    None,
                    &targets,
                    move |bounds, point| {
                        bounds.contains(point).then_some((
                            destination,
                            Rectangle {
                                height: 24.0,
                                ..bounds
                            },
                        ))
                    },
                ),
                targets,
                true,
                true,
                |target| target,
                None,
            )
        };
        let mut element = make("before group");
        let mut tree = Tree::new(&element);
        let limits = layout::Limits::new(Size::ZERO, Size::new(600.0, 400.0));
        let node = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits)
            .move_to(Point::new(50.0, 50.0));
        let mut messages = Vec::new();
        for (destination, point) in [
            ("before group", Point::new(80.0, 60.0)),
            ("after document", Point::new(82.0, 66.0)),
            ("after document", Point::new(82.0, 90.0)),
        ] {
            element = make(destination);
            tree.diff(&element);
            element.as_widget_mut().update(
                &mut tree,
                &Event::Mouse(mouse::Event::CursorMoved { position: point }),
                Layout::new(&node),
                mouse::Cursor::Available(point),
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut Shell::new(&mut messages),
                &Rectangle::with_size(Size::new(600.0, 400.0)),
            );
        }
        assert_eq!(messages, [Some("before group"), Some("after document")]);
    }

    #[test]
    fn pickup_offset_uses_scrolled_content_coordinates() {
        let renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            iced::Font::DEFAULT,
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .unwrap();
        let mut element = source_with_pointer(
            "card",
            iced::widget::Space::new().width(200).height(100),
            None,
            None,
            |point, bounds| Some(point - bounds.position()),
        );
        let mut tree = Tree::new(&element);
        let node = element
            .as_widget_mut()
            .layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(Size::ZERO, Size::new(200.0, 100.0)),
            )
            .move_to(Point::new(0.0, 400.0));
        let mut messages = Vec::new();
        for event in [
            mouse::Event::CursorMoved {
                position: Point::new(90.0, 30.0),
            },
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::CursorMoved {
                position: Point::new(96.0, 30.0),
            },
        ] {
            element.as_widget_mut().update(
                &mut tree,
                &Event::Mouse(event),
                Layout::new(&node),
                mouse::Cursor::Available(Point::new(90.0, 430.0)),
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut Shell::new(&mut messages),
                &Rectangle::new(Point::new(0.0, 400.0), Size::new(200.0, 100.0)),
            );
        }
        assert_eq!(messages, vec![Some(Vector::new(90.0, 30.0))]);
    }
}
