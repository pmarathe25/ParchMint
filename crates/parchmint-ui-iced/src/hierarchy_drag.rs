//! Hierarchy drag gestures and one hover decision per rendered surface.

use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{Operation, Tree, tree},
};
use iced::{Color, Element, Event, Length, Point, Rectangle, Size, Vector};
use std::time::{Duration, Instant};
use std::{cell::RefCell, rc::Rc};

const DRAG_THRESHOLD: f32 = 6.0;
const DOUBLE_CLICK_INTERVAL: Duration = Duration::from_millis(500);
/// Dragging near the top/bottom edge of a scrollable surface auto-scrolls
/// so cards can reach offscreen positions while the pointer stays in the viewport.
const DRAG_SCROLL_MARGIN: f32 = 64.0;
const DRAG_SCROLL_STEP: f32 = 14.0;

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
        yield_to_children: false,
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
        yield_to_children: false,
    }
    .into()
}

pub(crate) fn source_with_pointer_yielding<'a, Message: Clone + 'a>(
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
        yield_to_children: true,
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
    /// Grid cards sit side by side as well as stacked: a horizontal
    /// insertion line would point at the wrong edge. Vertical lists keep
    /// `false` and draw top/bottom lines; grid rows pass `true` so
    /// before/after draw as left/right lines.
    pub horizontal: bool,
}

impl DropIndicator {
    pub(crate) fn new(position: DropIndicatorPosition, color: Color) -> Self {
        Self {
            position,
            color,
            horizontal: false,
        }
    }
}

struct HierarchyDragSource<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer> {
    id: String,
    content: Element<'a, Message, Theme, Renderer>,
    on_click: Message,
    on_double_click: Option<Message>,
    on_drag_start: Box<dyn Fn(Point, Rectangle) -> Message + 'a>,
    yield_to_children: bool,
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
        if self.yield_to_children {
            let mut child_messages = Vec::new();
            let mut child_shell = Shell::new(&mut child_messages);
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
            let captured = child_shell.is_event_captured();
            if captured {
                shell.capture_event();
            }
            shell.request_redraw_at(child_shell.redraw_request());
            if child_shell.is_layout_invalid() {
                shell.invalidate_layout();
            }
            if child_shell.are_widgets_invalid() {
                shell.invalidate_widgets();
            }
            drop(child_shell);
            let child_captured = captured;
            for message in child_messages {
                shell.publish(message);
            }

            let state = tree.state.downcast_mut::<SourceState>();
            if child_captured {
                state.press_origin = None;
                state.dragging = false;
                return;
            }

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
                                && last.distance(state.last_pointer.unwrap_or(last))
                                    < DRAG_THRESHOLD
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
            return;
        }

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
            DropIndicatorPosition::Before if indicator.horizontal => Rectangle {
                width: 2.0,
                height: bounds.height,
                ..bounds
            },
            DropIndicatorPosition::Before => Rectangle {
                width: bounds.width,
                height: 2.0,
                ..bounds
            },
            DropIndicatorPosition::After if indicator.horizontal => Rectangle {
                x: bounds.x + (bounds.width - 2.0).max(0.0),
                width: 2.0,
                height: bounds.height,
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

/// Minimum hover time before a drop placeholder appears, so sweeping a card
/// across the outline never strobes previews on and off. The placeholder is
/// otherwise a pure function of the cursor: no sticky zones, no memory of
/// where the card has been.
pub(crate) const DROP_HOVER_DWELL: Duration = Duration::from_millis(120);

struct SurfaceState<Destination> {
    inside: bool,
    position: Option<Point>,
    scrolled: bool,
    pending: Option<(Option<Destination>, Instant)>,
    accepted: Option<Option<Destination>>,
}

impl<Destination> Default for SurfaceState<Destination> {
    fn default() -> Self {
        Self {
            inside: false,
            position: None,
            scrolled: false,
            pending: None,
            accepted: None,
        }
    }
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
    on_hover: Box<dyn Fn(Option<Destination>) -> Message + 'a>,
    on_leave: Message,
}

impl<Message, Destination, Theme, Renderer>
    HierarchyDragSurface<'_, Message, Destination, Theme, Renderer>
where
    Destination: Clone + PartialEq + 'static,
    Message: Clone,
{
    /// Stage a hover candidate behind the dwell: the placeholder only appears
    /// once the cursor rests on one target, so sweeping across the outline
    /// never strobes previews.
    fn hover_candidate(
        state: &mut SurfaceState<Destination>,
        on_hover: &dyn Fn(Option<Destination>) -> Message,
        shell: &mut Shell<'_, Message>,
        candidate: Option<Destination>,
    ) {
        let now = crate::motion::now();
        if state.accepted.as_ref() == Some(&candidate) {
            state.pending = None;
            return;
        }
        if candidate.is_none() || !crate::motion::enabled() {
            shell.publish(on_hover(candidate.clone()));
            state.accepted = Some(candidate);
            state.pending = None;
            return;
        }
        if let Some((pending, since)) = state.pending.as_ref()
            && *pending == candidate
        {
            if now.saturating_duration_since(*since) >= DROP_HOVER_DWELL {
                shell.publish(on_hover(candidate.clone()));
                state.accepted = Some(candidate);
                state.pending = None;
            } else {
                shell.request_redraw_at(*since + DROP_HOVER_DWELL);
            }
            return;
        }
        // An unconfirmed destination must never commit the previous hover.
        if state.accepted.as_ref().is_some_and(Option::is_some) {
            shell.publish(on_hover(None));
        }
        state.accepted = Some(None);
        state.pending = Some((candidate, now));
        shell.request_redraw_at(now + DROP_HOVER_DWELL);
    }
}

impl<Message, Destination, Theme, Renderer> Widget<Message, Theme, Renderer>
    for HierarchyDragSurface<'_, Message, Destination, Theme, Renderer>
where
    Destination: Clone + PartialEq + 'static,
    Message: Clone,
    Renderer: renderer::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<SurfaceState<Destination>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(SurfaceState::<Destination>::default())
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
        let state = tree.state.downcast_mut::<SurfaceState<Destination>>();
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
        let cursor_moved = matches!(event, Event::Mouse(iced::mouse::Event::CursorMoved { .. }));
        // Edge auto-scroll: holding the drag near the top/bottom edge
        // scrolls so offscreen rows stay reachable. Pointer moves only
        // kickstart the frame loop; the scroll steps run on animation
        // frames, which re-hit-test afterwards through the scrolled path.
        let near_edge = || {
            let probe = cursor.position()?;
            if !viewport.contains(probe) {
                return None;
            }
            let bounds = layout.bounds();
            if probe.x < bounds.x - 8.0 || probe.x > bounds.x + bounds.width + 8.0 {
                return None;
            }
            if probe.y <= bounds.y + DRAG_SCROLL_MARGIN {
                // Positive pixel deltas scroll up toward earlier rows.
                Some(DRAG_SCROLL_STEP)
            } else if probe.y >= bounds.y + bounds.height - DRAG_SCROLL_MARGIN {
                Some(-DRAG_SCROLL_STEP)
            } else {
                None
            }
        };
        if cursor_moved {
            if near_edge().is_some() {
                shell.request_redraw();
            }
        } else if matches!(
            event,
            Event::Window(iced::window::Event::RedrawRequested(_))
        ) && let Some(y) = near_edge()
        {
            crate::scroll_gate::without_momentum(|| {
                self.content.as_widget_mut().update(
                    &mut tree.children[0],
                    &Event::Mouse(iced::mouse::Event::WheelScrolled {
                        delta: iced::mouse::ScrollDelta::Pixels { x: 0.0, y },
                    }),
                    layout,
                    cursor,
                    renderer,
                    clipboard,
                    shell,
                    viewport,
                );
            });
            state.scrolled = true;
            shell.request_redraw();
        }
        // Recompute on frames too: scrolling can change the hit under a still cursor.
        let redraw = matches!(
            event,
            Event::Window(iced::window::Event::RedrawRequested(_))
        );
        if !cursor_moved && !after_scroll && !redraw {
            return;
        }
        let point = cursor
            .position()
            .filter(|point| layout.bounds().contains(*point) && viewport.contains(*point));
        if let Some(point) = point {
            // The candidate is a pure function of this frame's cursor: the
            // first hit wins, with no memory of previous drag positions.
            let candidate = self
                .targets
                .borrow_mut()
                .take()
                .map(|(destination, _)| destination);
            Self::hover_candidate(state, &self.on_hover, shell, candidate);
            state.position = Some(point);
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
    fn hover_dwell_confirms_placeholders_and_sweeps_stay_quiet() {
        use iced::advanced::renderer::Headless;
        use std::time::{Duration, Instant};
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
        let viewport = Rectangle::with_size(Size::new(600.0, 400.0));
        let mut messages = Vec::new();
        let start = Instant::now();
        let at = |millis: u64| start + Duration::from_millis(millis);
        let send = |element: &mut iced::Element<'_, Option<&'static str>>,
                    tree: &mut Tree,
                    event: &Event,
                    point: mouse::Cursor,
                    at: Instant,
                    messages: &mut Vec<Option<&'static str>>| {
            let _clock = crate::motion::FixedTime::new(at);
            let mut shell = Shell::new(messages);
            element.as_widget_mut().update(
                tree,
                event,
                Layout::new(&node),
                point,
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut shell,
                &viewport,
            );
        };
        let point = Point::new(80.0, 200.0);
        // Hovering stages a candidate but publishes nothing until the dwell
        // elapses, so sweeping across targets stays quiet.
        send(
            &mut element,
            &mut tree,
            &Event::Mouse(mouse::Event::CursorMoved { position: point }),
            mouse::Cursor::Available(point),
            at(0),
            &mut messages,
        );
        assert!(messages.is_empty());
        send(
            &mut element,
            &mut tree,
            &Event::Window(iced::window::Event::RedrawRequested(at(200))),
            mouse::Cursor::Available(point),
            at(200),
            &mut messages,
        );
        assert_eq!(messages, [Some("before group")]);
        // Moving to a new target resets to the origin until the hover confirms.
        element = make("after document");
        tree.diff(&element);
        let point = Point::new(82.0, 210.0);
        send(
            &mut element,
            &mut tree,
            &Event::Mouse(mouse::Event::CursorMoved { position: point }),
            mouse::Cursor::Available(point),
            at(210),
            &mut messages,
        );
        assert_eq!(messages, [Some("before group"), None]);
        send(
            &mut element,
            &mut tree,
            &Event::Window(iced::window::Event::RedrawRequested(at(400))),
            mouse::Cursor::Available(point),
            at(400),
            &mut messages,
        );
        assert_eq!(
            messages,
            [Some("before group"), None, Some("after document")]
        );
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

    #[test]
    fn holding_near_the_bottom_edge_scrolls_without_moving_the_pointer() {
        use std::time::Instant;
        let renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            iced::Font::DEFAULT,
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .unwrap();
        let targets = targets::<()>();
        let mut element = surface(
            iced::widget::scrollable(iced::widget::Space::new().width(200).height(1000))
                .height(100)
                .on_scroll(|viewport| Some(viewport.absolute_offset().y)),
            targets,
            true,
            |_| None,
            None,
        );
        let mut tree = Tree::new(&element);
        let viewport = Rectangle::with_size(Size::new(200.0, 100.0));
        let node = element.as_widget_mut().layout(
            &mut tree,
            &renderer,
            &layout::Limits::new(Size::ZERO, viewport.size()),
        );
        // Park the pointer inside the bottom auto-scroll margin and hold it:
        // animation frames must scroll the wrapped scrollable on their own.
        let point = Point::new(100.0, 90.0);
        let mut messages: Vec<Option<f32>> = Vec::new();
        element.as_widget_mut().update(
            &mut tree,
            &Event::Mouse(mouse::Event::CursorMoved { position: point }),
            Layout::new(&node),
            mouse::Cursor::Available(point),
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut Shell::new(&mut messages),
            &viewport,
        );
        messages.clear();
        let mut shell = Shell::new(&mut messages);
        element.as_widget_mut().update(
            &mut tree,
            &Event::Window(iced::window::Event::RedrawRequested(Instant::now())),
            Layout::new(&node),
            mouse::Cursor::Available(point),
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut shell,
            &viewport,
        );
        drop(shell);
        let offsets: Vec<f32> = messages.into_iter().flatten().collect();
        assert!(
            offsets.iter().any(|offset| *offset > 0.0),
            "holding near the bottom edge must scroll down, got {offsets:?}"
        );
    }

    #[test]
    fn leaving_the_window_clears_the_drag_preview() {
        let renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            iced::Font::DEFAULT,
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .unwrap();
        let targets = targets();
        let mut element = surface(
            target_with_zone(
                iced::widget::Space::new().width(300).height(180),
                None,
                &targets,
                move |bounds, point| {
                    bounds.contains(point).then_some((
                        "into group",
                        Rectangle {
                            height: 24.0,
                            ..bounds
                        },
                    ))
                },
            ),
            targets,
            true,
            |target| target,
            None,
        );
        let mut tree = Tree::new(&element);
        let limits = layout::Limits::new(Size::ZERO, Size::new(600.0, 400.0));
        let node = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits)
            .move_to(Point::new(50.0, 50.0));
        let viewport = Rectangle::with_size(Size::new(600.0, 400.0));
        let mut messages = Vec::new();
        let start = std::time::Instant::now();
        // Dwell first so the hover confirms.
        let point = Point::new(80.0, 200.0);
        for (event, cursor, at) in [
            (
                Event::Mouse(mouse::Event::CursorMoved { position: point }),
                mouse::Cursor::Available(point),
                start,
            ),
            (
                Event::Window(iced::window::Event::RedrawRequested(
                    start + std::time::Duration::from_millis(200),
                )),
                mouse::Cursor::Available(point),
                start + std::time::Duration::from_millis(200),
            ),
            // Leaving the window clears the accepted target.
            (
                Event::Mouse(mouse::Event::CursorMoved {
                    position: Point::new(-50.0, -50.0),
                }),
                mouse::Cursor::Unavailable,
                start + std::time::Duration::from_millis(210),
            ),
        ] {
            let _clock = crate::motion::FixedTime::new(at);
            let mut shell = Shell::new(&mut messages);
            element.as_widget_mut().update(
                &mut tree,
                &event,
                Layout::new(&node),
                cursor,
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut shell,
                &viewport,
            );
        }
        assert_eq!(messages, [Some("into group"), None]);
    }
}
