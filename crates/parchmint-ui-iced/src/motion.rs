//! Short, interruptible motion driven only by requested redraws.

use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, overlay, renderer,
    widget::{Operation, Tree, tree},
};
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector, mouse};
use std::{
    cell::Cell,
    collections::BTreeMap,
    time::{Duration, Instant},
};

pub(crate) const LAYOUT: Duration = Duration::from_millis(220);
const ENTRANCE: Duration = Duration::from_millis(140);
thread_local! {
    static CAPTURE: Cell<bool> = const { Cell::new(false) };
    static REDUCED: Cell<bool> = const { Cell::new(false) };
    static SETTLED: Cell<bool> = const { Cell::new(false) };
}
pub(crate) fn reduced() -> bool {
    REDUCED.get()
}
pub(crate) fn set_reduced(value: bool) {
    REDUCED.set(value);
}
pub(crate) fn set_capture(value: bool) {
    CAPTURE.set(value);
}
fn enabled() -> bool {
    !reduced() && !SETTLED.get() && !CAPTURE.get()
}

pub(crate) fn now() -> Instant {
    #[cfg(any(test, feature = "interaction-harness"))]
    if let Some(now) = FRAME_TIME.get() {
        return now;
    }
    Instant::now()
}
#[cfg(any(test, feature = "interaction-harness"))]
thread_local! { static FRAME_TIME: Cell<Option<Instant>> = const { Cell::new(None) }; }
#[cfg(any(test, feature = "interaction-harness"))]
pub(crate) struct FixedTime(Option<Instant>);
#[cfg(any(test, feature = "interaction-harness"))]
impl FixedTime {
    pub(crate) fn new(now: Instant) -> Self {
        Self(FRAME_TIME.replace(Some(now)))
    }
    #[cfg(feature = "interaction-harness")]
    pub(crate) fn advance(&mut self, elapsed: Duration) {
        FRAME_TIME.set(Some(now() + elapsed));
    }
}
#[cfg(any(test, feature = "interaction-harness"))]
impl Drop for FixedTime {
    fn drop(&mut self) {
        FRAME_TIME.set(self.0);
    }
}

#[cfg(any(test, feature = "interaction-harness", feature = "visual-verification"))]
pub(crate) struct SettledMotion(bool);
#[cfg(any(test, feature = "interaction-harness", feature = "visual-verification"))]
impl SettledMotion {
    pub(crate) fn new() -> Self {
        Self(SETTLED.replace(true))
    }
}
#[cfg(any(test, feature = "interaction-harness", feature = "visual-verification"))]
impl Drop for SettledMotion {
    fn drop(&mut self) {
        SETTLED.set(self.0);
    }
}

fn clipped_cursor(cursor: mouse::Cursor, bounds: Rectangle) -> mouse::Cursor {
    if cursor
        .position()
        .is_some_and(|position| bounds.contains(position))
    {
        cursor
    } else {
        mouse::Cursor::Unavailable
    }
}

#[derive(Clone, Debug)]
struct Tween {
    from: f32,
    target: f32,
    started: Instant,
    duration: Duration,
}
impl Tween {
    fn new(value: f32, now: Instant, duration: Duration) -> Self {
        Self {
            from: value,
            target: value,
            started: now,
            duration,
        }
    }
    fn value(&self, now: Instant) -> f32 {
        let t = (now.saturating_duration_since(self.started).as_secs_f32()
            / self.duration.as_secs_f32())
        .clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - t).powi(3);
        self.from + (self.target - self.from) * eased
    }
    fn active(&self, now: Instant) -> bool {
        self.from != self.target && now.saturating_duration_since(self.started) < self.duration
    }
    fn set(&mut self, target: f32, now: Instant, animate: bool) {
        if !animate {
            self.from = target;
            self.target = target;
        } else if self.target != target {
            self.from = self.value(now);
            self.target = target;
            self.started = now;
        }
    }
}

pub(crate) struct Slot<'a, Message> {
    content: Element<'a, Message>,
    width: Length,
    visible: bool,
}
pub(crate) fn slot<'a, Message>(
    content: impl Into<Element<'a, Message>>,
    width: Length,
    visible: bool,
) -> Slot<'a, Message> {
    Slot {
        content: content.into(),
        width,
        visible,
    }
}
pub(crate) fn row<'a, Message: 'a>(slots: Vec<Slot<'a, Message>>) -> Element<'a, Message> {
    Element::new(MotionRow {
        slots,
        animated: true,
    })
}

pub(crate) fn row_instant<'a, Message: 'a>(slots: Vec<Slot<'a, Message>>) -> Element<'a, Message> {
    Element::new(MotionRow {
        slots,
        animated: false,
    })
}
struct MotionRow<'a, Message> {
    slots: Vec<Slot<'a, Message>>,
    animated: bool,
}
struct RowState {
    reveals: Vec<Tween>,
    widths: Vec<f32>,
    now: Instant,
}
impl<Message> Widget<Message, iced::Theme, iced::Renderer> for MotionRow<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<RowState>()
    }
    fn state(&self) -> tree::State {
        let now = now();
        tree::State::new(RowState {
            widths: vec![320.0; self.slots.len()],
            reveals: self
                .slots
                .iter()
                .map(|slot| Tween::new(if slot.visible { 1.0 } else { 0.0 }, now, LAYOUT))
                .collect(),
            now,
        })
    }
    fn children(&self) -> Vec<Tree> {
        self.slots
            .iter()
            .map(|slot| Tree::new(&slot.content))
            .collect()
    }
    fn diff(&self, tree: &mut Tree) {
        let state = tree.state.downcast_mut::<RowState>();
        let now = now();
        state.widths.resize(self.slots.len(), 320.0);
        state
            .reveals
            .resize_with(self.slots.len(), || Tween::new(0.0, now, LAYOUT));
        for (reveal, slot) in state.reveals.iter_mut().zip(&self.slots) {
            reveal.set(
                if slot.visible { 1.0 } else { 0.0 },
                now,
                self.animated && enabled(),
            );
        }
        state.now = now;
        tree.diff_children_custom(
            &self.slots,
            |tree, slot| tree.diff(&slot.content),
            |slot| Tree::new(&slot.content),
        );
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<RowState>();
        let size = limits.max();
        let fractions: Vec<f32> = self
            .slots
            .iter()
            .enumerate()
            .map(|(i, slot)| {
                if self.animated && enabled() {
                    state.reveals[i].value(state.now)
                } else {
                    if slot.visible { 1.0 } else { 0.0 }
                }
            })
            .collect();
        let fixed: f32 = self
            .slots
            .iter()
            .zip(&fractions)
            .map(|(slot, f)| match slot.width {
                Length::Fixed(w) => w * f,
                _ => 0.0,
            })
            .sum();
        let weight: f32 = self
            .slots
            .iter()
            .zip(&fractions)
            .map(|(slot, f)| match slot.width {
                Length::FillPortion(w) => f32::from(w) * f,
                Length::Fill => *f,
                _ => 0.0,
            })
            .sum();
        let target_fixed: f32 = self
            .slots
            .iter()
            .filter(|slot| slot.visible)
            .map(|slot| {
                if let Length::Fixed(width) = slot.width {
                    width
                } else {
                    0.0
                }
            })
            .sum();
        let target_weight: f32 = self
            .slots
            .iter()
            .filter(|slot| slot.visible)
            .map(|slot| match slot.width {
                Length::FillPortion(weight) => f32::from(weight),
                Length::Fill => 1.0,
                _ => 0.0,
            })
            .sum();
        let mut x = 0.0;
        let children = self
            .slots
            .iter_mut()
            .zip(&mut tree.children)
            .zip(fractions)
            .zip(&mut state.widths)
            .map(|(((slot, tree), fraction), previous_width)| {
                let width = match slot.width {
                    Length::Fixed(w) => w * fraction,
                    Length::FillPortion(w) => {
                        (size.width - fixed).max(0.0) * f32::from(w) * fraction / weight.max(0.001)
                    }
                    _ => (size.width - fixed).max(0.0) * fraction / weight.max(0.001),
                };
                let content_width = match slot.width {
                    Length::Fixed(w) => w,
                    _ => {
                        let weight = match slot.width {
                            Length::FillPortion(weight) => f32::from(weight),
                            _ => 1.0,
                        };
                        let target_width = (size.width - target_fixed).max(0.0) * weight
                            / target_weight.max(0.001);
                        if slot.visible {
                            // Reveal incoming panes at a readable width instead of rewrapping
                            // their prose into a shrinking sliver on every frame.
                            *previous_width = if fraction < 1.0 {
                                width.max(target_width)
                            } else {
                                width
                            };
                        }
                        *previous_width
                    }
                };
                let content = slot.content.as_widget_mut().layout(
                    tree,
                    renderer,
                    &layout::Limits::new(
                        Size::new(content_width, size.height),
                        Size::new(content_width, size.height),
                    ),
                );
                let content = if matches!(slot.width, Length::Fixed(_)) {
                    content
                } else {
                    content.move_to(Point::new((width - content_width).min(0.0), 0.0))
                };
                let child =
                    layout::Node::with_children(Size::new(width, size.height), vec![content])
                        .move_to(Point::new(x, 0.0));
                x += width;
                child
            })
            .collect();
        layout::Node::with_children(size, children)
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
        let state = tree.state.downcast_mut::<RowState>();
        if let Event::Window(iced::window::Event::RedrawRequested(now)) = event {
            let was_active = state.reveals.iter().any(|reveal| reveal.active(state.now));
            state.now = *now;
            if was_active {
                shell.invalidate_layout();
            }
            if self.animated && enabled() && state.reveals.iter().any(|reveal| reveal.active(*now))
            {
                shell.request_redraw();
            }
        }
        for ((slot, tree), layout) in self
            .slots
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
        {
            if slot.visible
                && let Some(viewport) = viewport
                    .intersection(&layout.bounds())
                    .filter(|bounds| bounds.width > 0.5)
            {
                slot.content.as_widget_mut().update(
                    tree,
                    event,
                    layout.child(0),
                    clipped_cursor(cursor, viewport),
                    renderer,
                    clipboard,
                    shell,
                    &viewport,
                );
            }
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
        use renderer::Renderer;
        for ((slot, tree), layout) in self.slots.iter().zip(&tree.children).zip(layout.children()) {
            if let Some(viewport) = viewport
                .intersection(&layout.bounds())
                .filter(|bounds| bounds.width > 0.5)
            {
                renderer.with_layer(viewport, |renderer| {
                    slot.content.as_widget().draw(
                        tree,
                        renderer,
                        theme,
                        style,
                        layout.child(0),
                        clipped_cursor(cursor, viewport),
                        &viewport,
                    )
                });
            }
        }
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.traverse(&mut |operation| {
            for ((slot, tree), layout) in self
                .slots
                .iter_mut()
                .zip(&mut tree.children)
                .zip(layout.children())
            {
                if slot.visible {
                    slot.content.as_widget_mut().operate(
                        tree,
                        layout.child(0),
                        renderer,
                        operation,
                    );
                }
            }
        });
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.slots
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .filter(|((slot, _), _)| slot.visible)
            .filter_map(|((slot, tree), layout)| {
                let clip = viewport.intersection(&layout.bounds())?;
                Some(slot.content.as_widget().mouse_interaction(
                    tree,
                    layout.child(0),
                    clipped_cursor(cursor, clip),
                    &clip,
                    renderer,
                ))
            })
            .max()
            .unwrap_or_default()
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        let overlays: Vec<_> = self
            .slots
            .iter_mut()
            .zip(&mut tree.children)
            .zip(layout.children())
            .filter(|((slot, _), _)| slot.visible)
            .filter_map(|((slot, tree), layout)| {
                slot.content.as_widget_mut().overlay(
                    tree,
                    layout.child(0),
                    renderer,
                    viewport,
                    translation,
                )
            })
            .collect();
        (!overlays.is_empty()).then(|| overlay::Group::with_children(overlays).overlay())
    }
}

pub(crate) fn enter<'a, Message: 'a>(
    key: impl Into<String>,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    Element::new(Entrance {
        key: key.into(),
        content: content.into(),
        reveal: None,
        resize: false,
    })
}
pub(crate) fn reveal<'a, Message: 'a>(
    visible: bool,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    Element::new(Entrance {
        key: String::new(),
        content: content.into(),
        reveal: Some(visible),
        resize: false,
    })
}
/// Animate allocated height so following rows move with the card boundary.
pub(crate) fn resize_height<'a, Message: 'a>(
    key: impl Into<String>,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    Element::new(Entrance {
        key: key.into(),
        content: content.into(),
        reveal: Some(true),
        resize: true,
    })
}
struct Entrance<'a, Message> {
    key: String,
    content: Element<'a, Message>,
    reveal: Option<bool>,
    resize: bool,
}
struct EntranceState {
    height: f32,
    key: String,
    progress: Tween,
    now: Instant,
}
impl<Message> Widget<Message, iced::Theme, iced::Renderer> for Entrance<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<EntranceState>()
    }
    fn state(&self) -> tree::State {
        let now = now();
        let mut progress = Tween::new(f32::from(self.reveal.unwrap_or(false)), now, ENTRANCE);
        progress.set(f32::from(self.reveal.unwrap_or(true)), now, enabled());
        tree::State::new(EntranceState {
            height: 0.0,
            key: self.key.clone(),
            progress,
            now,
        })
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut Tree) {
        let state = tree.state.downcast_mut::<EntranceState>();
        if state.key != self.key {
            state.key = self.key.clone();
            state.now = now();
            state.progress = Tween::new(0.0, state.now, ENTRANCE);
            state.progress.set(1.0, state.now, enabled());
        }
        if let Some(visible) = self.reveal.filter(|_| !self.resize) {
            state.now = now();
            state.progress.set(f32::from(visible), state.now, enabled());
        }
        tree.diff_children(std::slice::from_ref(&self.content));
    }
    fn size(&self) -> Size<Length> {
        let mut size = self.content.as_widget().size();
        if self.reveal.is_some() {
            size.height = Length::Shrink;
        }
        size
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<EntranceState>();
        let progress = if enabled() {
            state.progress.value(state.now)
        } else {
            f32::from(self.reveal.unwrap_or(true))
        };
        let offset = if self.reveal.is_none() {
            8.0 * (1.0 - progress)
        } else {
            0.0
        };
        let child = self
            .content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        let mut size = child.size();
        if self.resize {
            let now = now();
            if state.height == 0.0 {
                state.progress = Tween::new(size.height, now, ENTRANCE);
            }
            state.height = size.height;
            state.progress.set(size.height, now, enabled());
            state.now = now;
            size.height = state.progress.value(now);
        } else if self.reveal.is_some() {
            if self.reveal == Some(true) || state.height == 0.0 {
                state.height = size.height;
            }
            size.height = state.height * progress;
        }
        layout::Node::with_children(size, vec![child.move_to(Point::new(0.0, offset))])
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
        let state = tree.state.downcast_mut::<EntranceState>();
        if let Event::Window(iced::window::Event::RedrawRequested(now)) = event {
            if state.progress.active(state.now) {
                shell.invalidate_layout();
            }
            state.now = *now;
            if enabled() && state.progress.active(*now) {
                shell.request_redraw();
            }
        }
        if self.reveal == Some(false) {
            return;
        }
        let cursor = if self.reveal.is_some() {
            clipped_cursor(cursor, layout.bounds())
        } else {
            cursor
        };
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout.child(0),
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
        use renderer::Renderer;
        if self.reveal.is_some() {
            if let Some(clip) = viewport
                .intersection(&layout.bounds())
                .filter(|bounds| bounds.height > 0.0)
            {
                renderer.with_layer(clip, |renderer| {
                    self.content.as_widget().draw(
                        &tree.children[0],
                        renderer,
                        theme,
                        style,
                        layout.child(0),
                        cursor,
                        &clip,
                    )
                });
            }
        } else {
            self.content.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                layout.child(0),
                cursor,
                viewport,
            );
        }
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        if self.reveal == Some(false) {
            return;
        }
        self.content.as_widget_mut().operate(
            &mut tree.children[0],
            layout.child(0),
            renderer,
            operation,
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
        if self.reveal == Some(false) {
            return mouse::Interaction::None;
        }
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout.child(0),
            cursor,
            viewport,
            renderer,
        )
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        if self.reveal == Some(false) {
            return None;
        }
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout.child(0),
            renderer,
            viewport,
            translation,
        )
    }
}

#[derive(Clone, Default, Debug)]
pub(crate) struct Positions(std::sync::Arc<std::sync::Mutex<BTreeMap<String, Placement>>>);
#[derive(Debug)]
struct Placement {
    generation: u64,
    target: Point,
    x: Tween,
    y: Tween,
}
impl Positions {
    pub(crate) fn retain(&self, ids: &[&str]) {
        self.0
            .lock()
            .expect("motion positions")
            .retain(|id, _| ids.contains(&id.as_str()));
    }
}
pub(crate) fn reflow<'a, Message: 'a>(
    positions: Positions,
    id: impl Into<String>,
    generation: u64,
    animate: bool,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    Element::new(Reflow {
        positions,
        id: id.into(),
        generation,
        animate,
        content: content.into(),
    })
}
struct Reflow<'a, Message> {
    positions: Positions,
    id: String,
    generation: u64,
    animate: bool,
    content: Element<'a, Message>,
}
struct ReflowState {
    id: String,
    offset: Vector,
}
impl<Message> Widget<Message, iced::Theme, iced::Renderer> for Reflow<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<ReflowState>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(ReflowState {
            id: self.id.clone(),
            offset: Vector::ZERO,
        })
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut Tree) {
        let state = tree.state.downcast_mut::<ReflowState>();
        if state.id != self.id {
            state.id.clone_from(&self.id);
            state.offset = Vector::ZERO;
        }
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
        let offset = if enabled() {
            tree.state.downcast_ref::<ReflowState>().offset
        } else {
            Vector::ZERO
        };
        let child = self
            .content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        layout::Node::with_children(child.size(), vec![child.move_to(Point::ORIGIN + offset)])
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
        if let Event::Window(iced::window::Event::RedrawRequested(now)) = event {
            let target = layout.position();
            let mut positions = self.positions.0.lock().expect("motion positions");
            let place = positions
                .entry(self.id.clone())
                .or_insert_with(|| Placement {
                    generation: self.generation,
                    target,
                    x: Tween::new(target.x, *now, LAYOUT),
                    y: Tween::new(target.y, *now, LAYOUT),
                });
            if place.target != target || !self.animate || !enabled() {
                let animate = self.animate && enabled() && place.generation != self.generation;
                place.x.set(target.x, *now, animate);
                place.y.set(target.y, *now, animate);
            }
            place.generation = self.generation;
            place.target = target;
            let offset = if enabled() {
                Vector::new(
                    place.x.value(*now) - target.x,
                    place.y.value(*now) - target.y,
                )
            } else {
                Vector::ZERO
            };
            let previous = &mut tree.state.downcast_mut::<ReflowState>().offset;
            if *previous != offset {
                *previous = offset;
                shell.invalidate_layout();
            }
            if enabled() && (place.x.active(*now) || place.y.active(*now)) {
                shell.request_redraw();
            }
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout.child(0),
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
        use renderer::Renderer;
        let desired = if enabled() {
            tree.state.downcast_ref::<ReflowState>().offset
        } else {
            Vector::ZERO
        };
        let correction = desired - (layout.child(0).position() - layout.position());
        // A settled card needs no extra GPU layer; keep clipping while it moves.
        if correction == Vector::ZERO {
            self.content.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                layout.child(0),
                cursor,
                viewport,
            );
            return;
        }
        renderer.with_layer(*viewport, |renderer| {
            renderer.with_translation(correction, |renderer| {
                self.content.as_widget().draw(
                    &tree.children[0],
                    renderer,
                    theme,
                    style,
                    layout.child(0),
                    cursor
                        .position()
                        .map_or(mouse::Cursor::Unavailable, |point| {
                            mouse::Cursor::Available(point - correction)
                        }),
                    &Rectangle {
                        x: viewport.x - correction.x,
                        y: viewport.y - correction.y,
                        ..*viewport
                    },
                )
            });
        });
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content.as_widget_mut().operate(
            &mut tree.children[0],
            layout.child(0),
            renderer,
            operation,
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
            layout.child(0),
            cursor,
            viewport,
            renderer,
        )
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout.child(0),
            renderer,
            viewport,
            translation,
        )
    }
}

#[derive(Debug)]
pub(crate) struct MenuMotion {
    progress: Tween,
    now: Instant,
}
impl Default for MenuMotion {
    fn default() -> Self {
        let now = now();
        Self {
            progress: Tween::new(1.0, now, ENTRANCE),
            now,
        }
    }
}
impl MenuMotion {
    pub(crate) fn start(&mut self) {
        self.now = now();
        self.progress = Tween::new(0.0, self.now, ENTRANCE);
        self.progress.set(1.0, self.now, enabled());
    }
    pub(crate) fn overlay<'a, Message: 'a>(
        &'a mut self,
        content: overlay::Element<'a, Message, iced::Theme, iced::Renderer>,
    ) -> overlay::Element<'a, Message, iced::Theme, iced::Renderer> {
        overlay::Element::new(Box::new(MenuEntrance {
            motion: self,
            content,
        }))
    }
}
struct MenuEntrance<'a, Message> {
    motion: &'a mut MenuMotion,
    content: overlay::Element<'a, Message, iced::Theme, iced::Renderer>,
}
impl<Message> overlay::Overlay<Message, iced::Theme, iced::Renderer> for MenuEntrance<'_, Message> {
    fn layout(&mut self, renderer: &iced::Renderer, bounds: Size) -> layout::Node {
        let node = self.content.as_overlay_mut().layout(renderer, bounds);
        let rect = node.bounds();
        let offset = if enabled() {
            4.0 * (1.0 - self.motion.progress.value(self.motion.now))
        } else {
            0.0
        };
        node.move_to(Point::new(rect.x, (rect.y - offset).max(0.0)))
    }
    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        if let Event::Window(iced::window::Event::RedrawRequested(now)) = event {
            if self.motion.progress.active(self.motion.now) {
                shell.invalidate_layout();
            }
            self.motion.now = *now;
            if enabled() && self.motion.progress.active(*now) {
                shell.request_redraw();
            }
        }
        self.content
            .as_overlay_mut()
            .update(event, layout, cursor, renderer, clipboard, shell);
    }
    fn draw(
        &self,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.content
            .as_overlay()
            .draw(renderer, theme, style, layout, cursor);
    }
    fn operate(
        &mut self,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_overlay_mut()
            .operate(layout, renderer, operation);
    }
    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content
            .as_overlay()
            .mouse_interaction(layout, cursor, renderer)
    }
    fn overlay<'a>(
        &'a mut self,
        layout: Layout<'a>,
        renderer: &iced::Renderer,
    ) -> Option<overlay::Element<'a, Message, iced::Theme, iced::Renderer>> {
        self.content.as_overlay_mut().overlay(layout, renderer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expanded_height_moves_following_rows_without_overlap() {
        let _clock = FixedTime::new(Instant::now());
        let renderer = renderer();
        let limits = layout::Limits::new(Size::ZERO, Size::new(320.0, 900.0));
        let mut card: Element<'_, ()> =
            resize_height("card", iced::widget::Space::new().width(320).height(192));
        let mut tree = Tree::new(&card);
        assert_eq!(
            card.as_widget_mut()
                .layout(&mut tree, &renderer, &limits)
                .size()
                .height,
            192.0
        );
        card = resize_height("card", iced::widget::Space::new().width(320).height(400));
        tree.diff(&card);
        let initial = card.as_widget_mut().layout(&mut tree, &renderer, &limits);
        assert_eq!(initial.size().height, 192.0);
        FRAME_TIME.set(Some(now() + ENTRANCE / 2));
        let middle = card.as_widget_mut().layout(&mut tree, &renderer, &limits);
        assert!(middle.size().height > 192.0 && middle.size().height < 400.0);
        FRAME_TIME.set(Some(now() + ENTRANCE));
        assert_eq!(
            card.as_widget_mut()
                .layout(&mut tree, &renderer, &limits)
                .size()
                .height,
            400.0
        );
    }

    #[test]
    fn interrupted_motion_continues_from_its_visible_position_and_settles() {
        let start = Instant::now();
        let mut tween = Tween::new(0.0, start, LAYOUT);
        tween.set(1.0, start, true);
        let halfway = start + LAYOUT / 2;
        let visible = tween.value(halfway);
        assert!(visible > 0.5 && visible < 1.0);
        tween.set(0.0, halfway, true);
        assert_eq!(tween.value(halfway), visible);
        assert_eq!(tween.value(halfway + LAYOUT), 0.0);
        assert!(!tween.active(halfway + LAYOUT));
    }
    #[test]
    fn reduced_motion_finishes_an_active_transition_immediately() {
        let now = Instant::now();
        let mut tween = Tween::new(0.0, now, LAYOUT);
        tween.set(1.0, now, true);
        tween.set(1.0, now + Duration::from_millis(20), false);
        assert_eq!(tween.value(now), 1.0);
        assert!(!tween.active(now));
    }
    fn renderer() -> iced::Renderer {
        use iced::advanced::renderer::Headless;
        iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            iced::Font::DEFAULT,
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .unwrap()
    }

    fn frame<Message>(
        element: &mut Element<'_, Message>,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        node: &layout::Node,
        now: Instant,
    ) -> iced::window::RedrawRequest {
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        element.as_widget_mut().update(
            tree,
            &Event::Window(iced::window::Event::RedrawRequested(now)),
            Layout::new(node),
            mouse::Cursor::Unavailable,
            renderer,
            &mut iced::advanced::clipboard::Null,
            &mut shell,
            &Rectangle::with_size(Size::new(800.0, 600.0)),
        );
        shell.redraw_request()
    }

    #[test]
    fn pane_expansion_has_intermediate_geometry_and_stops_requesting_frames() {
        set_reduced(false);
        let renderer = renderer();
        let pane = || {
            iced::widget::Space::new()
                .width(Length::Fill)
                .height(Length::Fill)
        };
        let mut element: Element<'_, ()> = row(vec![
            slot(pane(), Length::Fill, true),
            slot(pane(), Length::Fill, true),
        ]);
        let mut tree = Tree::new(&element);
        let limits = layout::Limits::new(Size::ZERO, Size::new(800.0, 600.0));
        let original = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        assert_eq!(original.children()[0].size().width, 400.0);
        element = row(vec![
            slot(pane(), Length::Fill, true),
            slot(pane(), Length::Fill, false),
        ]);
        tree.diff(&element);
        let start = tree.state.downcast_ref::<RowState>().now;
        assert_eq!(
            frame(
                &mut element,
                &mut tree,
                &renderer,
                &original,
                start + LAYOUT / 2
            ),
            iced::window::RedrawRequest::NextFrame
        );
        let middle = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        let width = middle.children()[0].size().width;
        assert!(width > 400.0 && width < 800.0);
        assert!(middle.children()[1].size().width > 0.0);
        assert_eq!(
            frame(&mut element, &mut tree, &renderer, &middle, start + LAYOUT),
            iced::window::RedrawRequest::Wait
        );
        let final_frame = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        assert_eq!(final_frame.children()[0].size().width, 800.0);
        assert_eq!(final_frame.children()[1].size().width, 0.0);
        assert!(final_frame.children()[1].children()[0].size().width >= 160.0);
        assert_eq!(
            frame(
                &mut element,
                &mut tree,
                &renderer,
                &final_frame,
                start + LAYOUT * 2
            ),
            iced::window::RedrawRequest::Wait
        );
    }

    #[test]
    fn incoming_panes_reveal_at_their_final_text_width() {
        set_reduced(false);
        let start = Instant::now();
        let _clock = FixedTime::new(start);
        let renderer = renderer();
        let pane = || {
            iced::widget::Space::new()
                .width(Length::Fill)
                .height(Length::Fill)
        };
        let mut element: Element<'_, ()> = row(vec![
            slot(pane(), Length::Fill, true),
            slot(pane(), Length::Fill, false),
        ]);
        let mut tree = Tree::new(&element);
        let limits = layout::Limits::new(Size::ZERO, Size::new(800.0, 600.0));
        element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        element = row(vec![
            slot(pane(), Length::Fill, true),
            slot(pane(), Length::Fill, true),
        ]);
        tree.diff(&element);
        for millis in [0, 16, 48, 96, 160, 240] {
            let node = element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &limits);
            frame(
                &mut element,
                &mut tree,
                &renderer,
                &node,
                start + Duration::from_millis(millis),
            );
            let node = element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &limits);
            let incoming = &node.children()[1];
            assert_eq!(incoming.children()[0].size().width, 400.0);
            assert!(incoming.size().width <= 400.0);
        }
    }

    #[test]
    fn grabbed_cards_stop_reflow_and_keep_the_drop_slot_stationary() {
        set_reduced(false);
        let renderer = renderer();
        let positions = Positions::default();
        let card = || iced::widget::Space::new().width(80).height(40);
        let mut element: Element<'_, ()> = reflow(positions.clone(), "card", 1, true, card());
        let mut tree = Tree::new(&element);
        let limits = layout::Limits::new(Size::ZERO, Size::new(800.0, 600.0));
        let start = Instant::now();
        let node = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        frame(&mut element, &mut tree, &renderer, &node, start);
        element = reflow(positions.clone(), "card", 2, true, card());
        tree.diff(&element);
        let node = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits)
            .move_to(Point::new(200.0, 0.0));
        frame(&mut element, &mut tree, &renderer, &node, start + LAYOUT);
        assert_ne!(
            tree.state.downcast_ref::<ReflowState>().offset,
            Vector::ZERO
        );
        element = reflow(positions, "card", 2, false, card());
        tree.diff(&element);
        assert_eq!(
            frame(
                &mut element,
                &mut tree,
                &renderer,
                &node,
                start + LAYOUT + Duration::from_millis(16)
            ),
            iced::window::RedrawRequest::Wait
        );
        assert_eq!(
            tree.state.downcast_ref::<ReflowState>().offset,
            Vector::ZERO
        );
    }

    #[test]
    fn moving_cards_keep_their_hit_boxes_and_scroll_without_animation() {
        set_reduced(false);
        let renderer = renderer();
        let positions = Positions::default();
        let card =
            || iced::widget::button(iced::widget::Space::new().width(80).height(40)).on_press(());
        let mut element = reflow(positions.clone(), "card", 1, true, card());
        let mut tree = Tree::new(&element);
        let limits = layout::Limits::new(Size::ZERO, Size::new(800.0, 600.0));
        let original = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        let start = Instant::now();
        frame(&mut element, &mut tree, &renderer, &original, start);
        element = reflow(positions, "card", 2, true, card());
        tree.diff(&element);
        let destination = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits)
            .move_to(Point::new(200.0, 0.0));
        // Idle UI does not receive continuous redraws.
        let moved = start + Duration::from_secs(30);
        assert_eq!(
            frame(&mut element, &mut tree, &renderer, &destination, moved),
            iced::window::RedrawRequest::NextFrame
        );
        frame(
            &mut element,
            &mut tree,
            &renderer,
            &destination,
            moved + LAYOUT / 2,
        );
        let middle = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits)
            .move_to(Point::new(200.0, 0.0));
        let bounds = Layout::new(&middle).child(0).bounds();
        assert!(bounds.x > 0.0 && bounds.x < 200.0);
        let mut messages = Vec::new();
        for event in [
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            element.as_widget_mut().update(
                &mut tree,
                &Event::Mouse(event),
                Layout::new(&middle),
                mouse::Cursor::Available(bounds.center()),
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut Shell::new(&mut messages),
                &Rectangle::with_size(Size::new(800.0, 600.0)),
            );
        }
        assert_eq!(messages, [()]);
        let scrolled = middle.move_to(Point::new(200.0, -60.0));
        assert_eq!(
            frame(
                &mut element,
                &mut tree,
                &renderer,
                &scrolled,
                moved + LAYOUT
            ),
            iced::window::RedrawRequest::Wait
        );
        assert_eq!(
            tree.state.downcast_ref::<ReflowState>().offset,
            Vector::ZERO
        );
    }

    #[test]
    fn search_disclosure_clips_intermediate_height_and_removes_hidden_controls() {
        set_reduced(false);
        let renderer = renderer();
        let content =
            || iced::widget::button(iced::widget::Space::new().width(80).height(40)).on_press(());
        let mut element = reveal(false, content());
        let mut tree = Tree::new(&element);
        let limits = layout::Limits::new(Size::ZERO, Size::new(800.0, 600.0));
        assert_eq!(
            element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &limits)
                .size()
                .height,
            0.0
        );
        element = reveal(true, content());
        tree.diff(&element);
        let start = tree.state.downcast_ref::<EntranceState>().now;
        let node = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        frame(
            &mut element,
            &mut tree,
            &renderer,
            &node,
            start + ENTRANCE / 2,
        );
        let middle = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        assert!(
            middle.size().height > 0.0 && middle.size().height < middle.children()[0].size().height
        );
        element = reveal(false, content());
        tree.diff(&element);
        let mut messages = Vec::new();
        for event in [
            mouse::Event::ButtonPressed(mouse::Button::Left),
            mouse::Event::ButtonReleased(mouse::Button::Left),
        ] {
            element.as_widget_mut().update(
                &mut tree,
                &Event::Mouse(event),
                Layout::new(&middle),
                mouse::Cursor::Available(Point::new(10.0, 10.0)),
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut Shell::new(&mut messages),
                &Rectangle::with_size(Size::new(800.0, 600.0)),
            );
        }
        assert!(messages.is_empty());
        let _motion = SettledMotion::new();
        assert_eq!(
            element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &limits)
                .size()
                .height,
            0.0
        );
    }
}
