//! Short, interruptible motion driven only by requested redraws.

mod dismiss;
pub(crate) use dismiss::dismissible;

use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, overlay, renderer,
    widget::{Operation, Tree, tree},
};
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector, mouse};
use std::{
    cell::Cell,
    collections::BTreeMap,
    rc::Rc,
    time::{Duration, Instant},
};

pub(crate) const LAYOUT: Duration = Duration::from_millis(200);
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
pub(crate) fn enabled() -> bool {
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

/// Material's standard curve: accelerate promptly, then settle gently. Solving
/// the cubic's X coordinate keeps timing correct (the Bezier parameter is not
/// elapsed time). Used by shared surfaces, panes, disclosures and reordering.
fn standard_easing(time: f32) -> f32 {
    if time <= 0.0 || time >= 1.0 {
        return time.clamp(0.0, 1.0);
    }
    let mut low: f32 = 0.0;
    let mut high: f32 = 1.0;
    for _ in 0..16 {
        let t = (low + high) * 0.5;
        let x = 3.0 * (1.0 - t).powi(2) * t * 0.4 + 3.0 * (1.0 - t) * t * t * 0.2 + t * t * t;
        if x < time {
            low = t;
        } else {
            high = t;
        }
    }
    let t = (low + high) * 0.5;
    3.0 * (1.0 - t) * t * t + t * t * t
}

/// A single interruptible timeline shared by every row of a disclosure.
/// Keeping this in workspace state lets newly mounted rows join the same
/// expansion and keeps outgoing rows mounted until their collapse finishes.
#[derive(Clone, Debug)]
pub(crate) struct Disclosure(Tween);
impl Disclosure {
    pub(crate) fn new(visible: bool) -> Self {
        Self(Tween::new(f32::from(visible), now(), LAYOUT))
    }
    pub(crate) fn set(&mut self, visible: bool) {
        self.0.set(f32::from(visible), now(), enabled());
    }
    pub(crate) fn visible_fraction(&self) -> f32 {
        if enabled() {
            self.0.value(now())
        } else {
            self.0.target
        }
    }
    pub(crate) fn active(&self) -> bool {
        enabled() && self.0.active(now())
    }
    pub(crate) fn visible(&self) -> bool {
        self.0.target > 0.0
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
        let eased = standard_easing(t);
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
        shrink: false,
        page: String::new(),
    })
}

/// A pane row that snaps instead of sliding when the page changes, so
/// navigating screens never plays sidebar collapse/expand motion. Toggles
/// within the same page still animate. Each distinct page gets its own
/// reveal state; only matching pages animate between each other.
pub(crate) fn row_for_page<'a, Message: 'a>(
    page: impl Into<String>,
    slots: Vec<Slot<'a, Message>>,
) -> Element<'a, Message> {
    Element::new(MotionRow {
        slots,
        animated: true,
        shrink: false,
        page: page.into(),
    })
}

/// A compact fixed-slot row whose outside bounds follow the reveal itself.
/// Used by shared header controls so an outer container cannot cut the exit
/// short by snapping immediately to the final collapsed width.
pub(crate) fn row_shrink<'a, Message: 'a>(slots: Vec<Slot<'a, Message>>) -> Element<'a, Message> {
    Element::new(MotionRow {
        slots,
        animated: true,
        shrink: true,
        page: String::new(),
    })
}

pub(crate) fn row_instant<'a, Message: 'a>(slots: Vec<Slot<'a, Message>>) -> Element<'a, Message> {
    Element::new(MotionRow {
        slots,
        animated: false,
        shrink: false,
        page: String::new(),
    })
}
struct MotionRow<'a, Message> {
    slots: Vec<Slot<'a, Message>>,
    animated: bool,
    shrink: bool,
    page: String,
}
struct RowState {
    reveals: Vec<Tween>,
    widths: Vec<f32>,
    now: Instant,
    page: String,
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
            page: self.page.clone(),
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
        // A page change snaps every reveal instantly: navigating must not
        // replay sidebar motion. The whole-page entrance (if any) still plays.
        let navigated = state.page != self.page;
        if navigated {
            state.page.clone_from(&self.page);
        }
        let previous_len = state.reveals.len();
        state.widths.resize(self.slots.len(), 320.0);
        state
            .reveals
            .resize_with(self.slots.len(), || Tween::new(0.0, now, LAYOUT));
        for (index, (reveal, slot)) in state.reveals.iter_mut().zip(&self.slots).enumerate() {
            // Toggling a pane within a screen animates. Mounting entirely new
            // slots snaps in instead: sliding sidebars on structural changes
            // is disruptive. Page changes snap via the page key above.
            let added = index >= previous_len;
            reveal.set(
                if slot.visible { 1.0 } else { 0.0 },
                now,
                self.animated && enabled() && !added && !navigated,
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
        Size::new(
            if self.shrink {
                Length::Shrink
            } else {
                Length::Fill
            },
            Length::Fill,
        )
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<RowState>();
        let mut size = limits.max();
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
        if self.shrink {
            size.width = fixed.min(size.width);
        }
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
        disclosure: None,
        on_complete: None,
        origin: None,
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
        disclosure: None,
        on_complete: None,
        origin: None,
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
        disclosure: None,
        on_complete: None,
        origin: None,
    })
}
/// Reveal rows from their top edge without sliding their text against the
/// movement of the containing group. Pass a completion message on one row per
/// group so the workspace can discard retained collapsing descendants.
pub(crate) fn disclosure<'a, Message: 'a>(
    timeline: Disclosure,
    content: impl Into<Element<'a, Message>>,
    on_complete: Option<Message>,
) -> Element<'a, Message> {
    Element::new(Entrance {
        key: String::new(),
        content: content.into(),
        reveal: Some(timeline.visible()),
        resize: false,
        disclosure: Some(timeline),
        on_complete,
        origin: None,
    })
}

struct Entrance<'a, Message> {
    key: String,
    content: Element<'a, Message>,
    reveal: Option<bool>,
    resize: bool,
    disclosure: Option<Disclosure>,
    on_complete: Option<Message>,
    origin: Option<Rc<Cell<Point>>>,
}
struct EntranceState {
    height: f32,
    key: String,
    progress: Tween,
    now: Instant,
    completed: Option<Instant>,
}
impl<Message> Widget<Message, iced::Theme, iced::Renderer> for Entrance<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<EntranceState>()
    }
    fn state(&self) -> tree::State {
        let now = now();
        let duration = if self.resize { LAYOUT } else { ENTRANCE };
        let mut progress = Tween::new(
            f32::from(self.reveal.unwrap_or(self.origin.is_some())),
            now,
            duration,
        );
        progress.set(f32::from(self.reveal.unwrap_or(true)), now, enabled());
        tree::State::new(EntranceState {
            height: 0.0,
            key: self.key.clone(),
            progress,
            now,
            completed: None,
        })
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut Tree) {
        let state = tree.state.downcast_mut::<EntranceState>();
        if state.key != self.key {
            state.key = self.key.clone();
            // Virtualized rows reuse widget trees. A new identity has no
            // relationship to the previous card's measured height.
            if self.resize {
                state.height = 0.0;
            }
            state.now = now();
            state.progress =
                Tween::new(0.0, state.now, if self.resize { LAYOUT } else { ENTRANCE });
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
        let progress = if let Some(disclosure) = &self.disclosure {
            disclosure.visible_fraction()
        } else if enabled() {
            state.progress.value(state.now)
        } else {
            f32::from(self.reveal.unwrap_or(true))
        };
        let offset = if self.reveal.is_none() && self.origin.is_none() {
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
                state.progress = Tween::new(size.height, now, LAYOUT);
            }
            state.height = size.height;
            state.progress.set(size.height, now, enabled());
            state.now = now;
            size.height = state.progress.value(now);
        } else if self.reveal.is_some() {
            if self.disclosure.is_some() || self.reveal == Some(true) || state.height == 0.0 {
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
        if let Some(origin) = &self.origin {
            origin.set(layout.position());
        }
        let state = tree.state.downcast_mut::<EntranceState>();
        if let Event::Window(iced::window::Event::RedrawRequested(now)) = event {
            let progress = self
                .disclosure
                .as_ref()
                .map_or(&state.progress, |value| &value.0);
            if progress.active(state.now) {
                shell.invalidate_layout();
            }
            state.now = *now;
            if enabled() && progress.active(*now) {
                shell.request_redraw();
            } else if self.disclosure.is_some() && state.completed != Some(progress.started) {
                state.completed = Some(progress.started);
                if let Some(message) = self.on_complete.take() {
                    shell.publish(message);
                }
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
        let cursor = if self.reveal.is_some() {
            clipped_cursor(cursor, layout.bounds())
        } else {
            cursor
        };
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
/// Position field blocks relative to their own card, so card travel and field
/// rearrangement compose without applying the parent's translation twice.
#[derive(Clone)]
pub(crate) struct LocalPositions {
    positions: Positions,
    origin: Rc<Cell<Point>>,
}
impl LocalPositions {
    pub(crate) fn new(positions: Positions) -> Self {
        Self {
            positions,
            origin: Rc::new(Cell::new(Point::ORIGIN)),
        }
    }
    pub(crate) fn item<'a, Message: 'a>(
        &self,
        id: String,
        generation: u64,
        content: impl Into<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        Element::new(Reflow {
            positions: self.positions.clone(),
            id,
            generation,
            animate: true,
            origin: Some(self.origin.clone()),
            both_axes: true,
            morph_width: false,
            content: content.into(),
        })
    }
    pub(crate) fn group<'a, Message: 'a>(
        &self,
        content: impl Into<Element<'a, Message>>,
    ) -> Element<'a, Message> {
        Element::new(Entrance {
            key: String::new(),
            content: content.into(),
            reveal: None,
            resize: false,
            disclosure: None,
            on_complete: None,
            origin: Some(self.origin.clone()),
        })
    }
}

#[derive(Debug)]
struct Placement {
    generation: u64,
    target: Point,
    width: f32,
    width_motion: Tween,
    height_motion: Tween,
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

    pub(crate) fn retain_card_fields(&self, ids: &[&str]) {
        self.0.lock().expect("motion positions").retain(|key, _| {
            key.split_once('\0')
                .is_some_and(|(node, _)| ids.contains(&node))
        });
    }

    pub(crate) fn vertical_offset(&self, id: &str, now: Instant) -> f32 {
        self.0
            .lock()
            .expect("motion positions")
            .get(id)
            .map(|place| place.y.value(now) - place.target.y)
            .unwrap_or(0.0)
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
        origin: None,
        both_axes: false,
        morph_width: false,
        content: content.into(),
    })
}
/// Overview card surfaces change width while their content remains real text.
/// The grid reserves final slots; the moving surface interpolates its measured
/// width rather than scaling typography or snapping between card and heading.
pub(crate) fn reflow_card<'a, Message: 'a>(
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
        origin: None,
        both_axes: true,
        morph_width: true,
        content: content.into(),
    })
}

struct Reflow<'a, Message> {
    positions: Positions,
    id: String,
    generation: u64,
    animate: bool,
    origin: Option<Rc<Cell<Point>>>,
    both_axes: bool,
    morph_width: bool,
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
        let mut child =
            self.content
                .as_widget_mut()
                .layout(&mut tree.children[0], renderer, limits);
        let allocated = child.size();
        if self.morph_width {
            let at = now();
            let width = {
                let mut positions = self.positions.0.lock().expect("motion positions");
                positions.get_mut(&self.id).map(|place| {
                    place
                        .width_motion
                        .set(allocated.width, at, self.animate && enabled());
                    let morphing = place.width_motion.active(at) || place.height_motion.active(at);
                    place.height_motion.set(
                        allocated.height,
                        at,
                        self.animate && enabled() && morphing,
                    );
                    place.width_motion.value(at)
                })
            };
            if let Some(width) = width.filter(|width| (*width - allocated.width).abs() > 0.1) {
                let full = Size::new(width.max(1.0), limits.max().height);
                child = self.content.as_widget_mut().layout(
                    &mut tree.children[0],
                    renderer,
                    &layout::Limits::new(Size::new(full.width, 0.0), full),
                );
            }
        }
        layout::Node::with_children(allocated, vec![child.move_to(Point::ORIGIN + offset)])
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
            let target = self.origin.as_ref().map_or(layout.position(), |origin| {
                Point::ORIGIN + (layout.position() - origin.get())
            });
            let mut positions = self.positions.0.lock().expect("motion positions");
            let place = positions
                .entry(self.id.clone())
                .or_insert_with(|| Placement {
                    generation: self.generation,
                    target,
                    width: layout.bounds().width,
                    width_motion: Tween::new(layout.bounds().width, *now, LAYOUT),
                    height_motion: Tween::new(layout.bounds().height, *now, LAYOUT),
                    x: Tween::new(target.x, *now, LAYOUT),
                    y: Tween::new(target.y, *now, LAYOUT),
                });
            let resized = (place.width - layout.bounds().width).abs() > 0.5;
            if place.target != target || resized || !self.animate || !enabled() {
                // Growing to a full-width heading cannot travel through the
                // former compact row without covering its neighbors. A heading
                // that has become compact can travel back into its packed row.
                let grew = layout.bounds().width > place.width + 0.5;
                let animate = self.animate
                    && enabled()
                    && (!grew || self.both_axes)
                    && place.generation != self.generation;
                if animate {
                    // Material motion: list elements travel vertically from
                    // their start to their end. A group expanding downward
                    // pushes siblings down, so the meaningful axis is always
                    // Y: snapping Y would teleport cards in the wrong
                    // direction. Animate Y whenever it changes; animate X
                    // only for pure horizontal shifts (same-row reorder, tab
                    // strips) where there is no vertical story to tell.
                    let current_x = place.x.value(*now);
                    let current_y = place.y.value(*now);
                    let dx = (target.x - current_x).abs();
                    let dy = (target.y - current_y).abs();
                    let animate_y = dy >= 1.0;
                    let animate_x = dx >= 1.0 && (dy < 1.0 || self.both_axes);
                    place.x.set(target.x, *now, animate_x);
                    place.y.set(target.y, *now, animate_y);
                } else {
                    place.x.set(target.x, *now, false);
                    place.y.set(target.y, *now, false);
                }
            }
            place.generation = self.generation;
            place.target = target;
            place.width = layout.bounds().width;
            if !self.morph_width {
                place.width_motion.set(place.width, *now, false);
                place.height_motion.set(layout.bounds().height, *now, false);
            }
            if self.morph_width {
                if place.width_motion.active(*now)
                    || (layout.child(0).bounds().width - place.width).abs() > 0.1
                {
                    shell.invalidate_layout();
                }
                if place.width_motion.active(*now) || place.height_motion.active(*now) {
                    shell.request_redraw();
                }
            }
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
        // A redraw can advance the translation before Iced lays out the next
        // frame. Hit testing must use the same correction as drawing, so card
        // targets follow the visible card even on that first moving frame.
        let correction = tree.state.downcast_ref::<ReflowState>().offset
            - (layout.child(0).position() - layout.position());
        let cursor = cursor
            .position()
            .map_or(mouse::Cursor::Unavailable, |point| {
                mouse::Cursor::Available(point - correction)
            });
        let viewport = Rectangle {
            x: viewport.x - correction.x,
            y: viewport.y - correction.y,
            ..*viewport
        };
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout.child(0),
            cursor,
            renderer,
            clipboard,
            shell,
            &viewport,
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
        let visible_height = self
            .morph_width
            .then(|| {
                self.positions
                    .0
                    .lock()
                    .expect("motion positions")
                    .get(&self.id)
                    .filter(|place| enabled() && place.height_motion.active(now()))
                    .map(|place| place.height_motion.value(now()))
            })
            .flatten();
        let clip = visible_height
            .and_then(|height| {
                viewport.intersection(&Rectangle {
                    x: layout.child(0).bounds().x + correction.x,
                    y: layout.child(0).bounds().y + correction.y,
                    width: layout.child(0).bounds().width,
                    height,
                })
            })
            .unwrap_or(*viewport);
        // Retain the source height when packing moves a header into a new row
        // and therefore mounts a fresh child widget tree.
        if correction == Vector::ZERO && visible_height.is_none() {
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
        renderer.with_layer(clip, |renderer| {
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
            progress: Tween::new(0.0, now, ENTRANCE),
            now,
        }
    }
}
impl MenuMotion {
    pub(crate) fn start(&mut self) {
        self.now = now();
        self.progress.set(1.0, self.now, enabled());
    }
    pub(crate) fn close(&mut self) {
        self.progress.set(0.0, now(), enabled());
    }
    pub(crate) fn exiting(&self) -> bool {
        enabled() && self.progress.target == 0.0 && self.progress.active(now())
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
        if self.motion.progress.target == 0.0 {
            return;
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
        use renderer::Renderer;
        let bounds = layout.bounds();
        let fraction = if enabled() {
            self.motion.progress.value(self.motion.now)
        } else {
            self.motion.progress.target
        };
        if fraction <= 0.0 {
            return;
        }
        renderer.with_layer(
            Rectangle {
                height: bounds.height * fraction,
                ..bounds
            },
            |renderer| {
                self.content
                    .as_overlay()
                    .draw(renderer, theme, style, layout, cursor);
            },
        );
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
    fn project_selector_width_follows_its_exit_instead_of_clipping_it_immediately() {
        set_reduced(false);
        let at = now();
        let _clock = FixedTime::new(at);
        let renderer = renderer();
        let make = |visible| {
            row_shrink(vec![
                slot(
                    iced::widget::Space::new().width(48).height(48),
                    Length::Fixed(48.0),
                    true,
                ),
                slot(
                    iced::widget::Space::new().width(280).height(48),
                    Length::Fixed(280.0),
                    visible,
                ),
            ])
        };
        let mut element: Element<'_, ()> = make(true);
        let mut tree = Tree::new(&element);
        let limits = layout::Limits::new(Size::ZERO, Size::new(900.0, 48.0));
        let original = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        assert_eq!(original.size().width, 328.0);
        element = make(false);
        tree.diff(&element);
        let initial = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        assert_eq!(initial.size().width, 328.0);
        frame(
            &mut element,
            &mut tree,
            &renderer,
            &initial,
            at + LAYOUT / 2,
        );
        let middle = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        assert!(middle.size().width > 48.0 && middle.size().width < 328.0);
        frame(&mut element, &mut tree, &renderer, &middle, at + LAYOUT);
        assert_eq!(
            element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &limits)
                .size()
                .width,
            48.0
        );
    }

    #[test]
    fn fields_move_on_both_axes_in_card_coordinates_when_the_card_itself_moves() {
        set_reduced(false);
        let at = now();
        let _clock = FixedTime::new(at);
        let renderer = renderer();
        let positions = Positions::default();
        let make = |generation, left, top| {
            let fields = LocalPositions::new(positions.clone());
            fields.group(
                iced::widget::container(fields.item(
                    "field".into(),
                    generation,
                    iced::widget::Space::new().width(80).height(30),
                ))
                .padding(iced::Padding {
                    left,
                    top,
                    ..iced::Padding::ZERO
                }),
            )
        };
        let mut element: Element<'_, ()> = make(0, 0.0, 100.0);
        let mut tree = Tree::new(&element);
        let limits = layout::Limits::new(Size::ZERO, Size::new(800.0, 600.0));
        let original = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits)
            .move_to(Point::new(20.0, 40.0));
        frame(&mut element, &mut tree, &renderer, &original, at);
        element = make(1, 300.0, 0.0);
        tree.diff(&element);
        let destination = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits)
            .move_to(Point::new(100.0, 200.0));
        frame(&mut element, &mut tree, &renderer, &destination, at);
        {
            let values = positions.0.lock().unwrap();
            let field = &values["field"];
            assert_eq!(field.target, Point::new(300.0, 0.0));
            assert_eq!(field.x.value(at), 0.0);
            assert_eq!(field.y.value(at), 100.0);
        }
        frame(
            &mut element,
            &mut tree,
            &renderer,
            &destination,
            at + LAYOUT / 2,
        );
        let values = positions.0.lock().unwrap();
        assert!(values["field"].x.value(at + LAYOUT / 2) > 0.0);
        assert!(values["field"].y.value(at + LAYOUT / 2) < 100.0);
    }

    #[test]
    fn moving_card_hit_target_matches_the_first_rendered_animation_frame() {
        set_reduced(false);
        let at = now();
        let _clock = FixedTime::new(at);
        let renderer = renderer();
        let positions = Positions::default();
        let targets = crate::hierarchy_drag::targets();
        let make = |generation| {
            reflow(
                positions.clone(),
                "card",
                generation,
                true,
                crate::hierarchy_drag::target(
                    iced::widget::Space::new().width(100).height(40),
                    None,
                    &targets,
                    |bounds, point| bounds.contains(point).then_some("card"),
                ),
            )
        };
        let mut card: Element<'_, ()> = make(0);
        let mut tree = Tree::new(&card);
        let limits = layout::Limits::new(Size::ZERO, Size::new(400.0, 900.0));
        let initial = card
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits)
            .move_to(Point::new(0.0, 20.0));
        frame(&mut card, &mut tree, &renderer, &initial, at);
        card = make(1);
        tree.diff(&card);
        let destination = card
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits)
            .move_to(Point::new(0.0, 220.0));
        let mut messages = Vec::new();
        card.as_widget_mut().update(
            &mut tree,
            &Event::Window(iced::window::Event::RedrawRequested(at)),
            Layout::new(&destination),
            mouse::Cursor::Available(Point::new(30.0, 40.0)),
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut Shell::new(&mut messages),
            &Rectangle::with_size(Size::new(400.0, 900.0)),
        );
        assert_eq!(targets.borrow().as_ref().map(|(id, _)| *id), Some("card"));
    }

    #[test]
    fn material_standard_curve_is_monotonic_and_settles_exactly() {
        assert_eq!(standard_easing(0.0), 0.0);
        assert_eq!(standard_easing(1.0), 1.0);
        let samples: Vec<_> = (0..=100)
            .map(|i| standard_easing(i as f32 / 100.0))
            .collect();
        assert!(samples.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(standard_easing(0.1) < 0.1, "surfaces accelerate from rest");
        assert!(
            standard_easing(0.5) > 0.7,
            "surfaces decelerate before settling"
        );
    }

    #[test]
    fn recycled_card_height_does_not_inherit_an_unrelated_card_animation() {
        let _clock = FixedTime::new(Instant::now());
        let renderer = renderer();
        let limits = layout::Limits::new(Size::ZERO, Size::new(320.0, 900.0));
        let mut card: Element<'_, ()> =
            resize_height("old", iced::widget::Space::new().height(192));
        let mut tree = Tree::new(&card);
        card.as_widget_mut().layout(&mut tree, &renderer, &limits);
        card = resize_height("new", iced::widget::Space::new().height(400));
        tree.diff(&card);
        assert_eq!(
            card.as_widget_mut()
                .layout(&mut tree, &renderer, &limits)
                .size()
                .height,
            400.0
        );
    }

    #[test]
    fn disclosure_rows_reveal_downward_and_reverse_without_jumping() {
        set_reduced(false);
        let _clock = FixedTime::new(Instant::now());
        let renderer = renderer();
        let limits = layout::Limits::new(Size::ZERO, Size::new(320.0, 900.0));
        let mut timeline = Disclosure::new(false);
        timeline.set(true);
        let make =
            |timeline| disclosure(timeline, iced::widget::Space::new().height(200), None::<()>);
        let mut row = make(timeline.clone());
        let mut tree = Tree::new(&row);
        let start = row.as_widget_mut().layout(&mut tree, &renderer, &limits);
        assert_eq!(start.size().height, 0.0);
        FRAME_TIME.set(Some(now() + LAYOUT / 2));
        let middle = row.as_widget_mut().layout(&mut tree, &renderer, &limits);
        assert!(middle.size().height > 0.0 && middle.size().height < 200.0);
        assert_eq!(
            middle.children()[0].bounds().y,
            0.0,
            "content stays anchored to the group top"
        );
        assert_eq!(
            middle.children()[0].size().height,
            200.0,
            "text keeps its natural geometry"
        );
        timeline.set(false);
        row = make(timeline.clone());
        tree.diff(&row);
        let reversed = row.as_widget_mut().layout(&mut tree, &renderer, &limits);
        assert_eq!(reversed.size().height, middle.size().height);
        FRAME_TIME.set(Some(now() + LAYOUT));
        assert!(!timeline.active());
        assert_eq!(
            row.as_widget_mut()
                .layout(&mut tree, &renderer, &limits)
                .size()
                .height,
            0.0
        );
    }

    #[test]
    fn disclosure_completion_is_sent_once_and_reduced_motion_settles() {
        set_reduced(false);
        let _clock = FixedTime::new(Instant::now());
        let renderer = renderer();
        let limits = layout::Limits::new(Size::ZERO, Size::new(320.0, 900.0));
        let mut timeline = Disclosure::new(true);
        timeline.set(false);
        let mut row = disclosure(
            timeline.clone(),
            iced::widget::Space::new().height(200),
            Some("done"),
        );
        let mut tree = Tree::new(&row);
        let node = row.as_widget_mut().layout(&mut tree, &renderer, &limits);
        let mut messages = Vec::new();
        FRAME_TIME.set(Some(now() + LAYOUT));
        for _ in 0..2 {
            row.as_widget_mut().update(
                &mut tree,
                &Event::Window(iced::window::Event::RedrawRequested(now())),
                Layout::new(&node),
                mouse::Cursor::Unavailable,
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut Shell::new(&mut messages),
                &Rectangle::with_size(Size::new(320.0, 900.0)),
            );
        }
        assert_eq!(messages, vec!["done"]);
        timeline.set(true);
        set_reduced(true);
        assert_eq!(timeline.visible_fraction(), 1.0);
        assert!(!timeline.active());
        set_reduced(false);
    }

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
        FRAME_TIME.set(Some(now() + LAYOUT / 2));
        let middle = card.as_widget_mut().layout(&mut tree, &renderer, &limits);
        assert!(middle.size().height > 192.0 && middle.size().height < 400.0);
        FRAME_TIME.set(Some(now() + LAYOUT));
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
    fn page_change_snaps_sidebars_while_same_page_toggles_animate() {
        set_reduced(false);
        let renderer = renderer();
        let pane = || {
            iced::widget::Space::new()
                .width(Length::Fill)
                .height(Length::Fill)
        };
        let mut element: Element<'_, ()> = row_for_page(
            "Editor",
            vec![
                slot(pane(), Length::Fill, true),
                slot(pane(), Length::Fill, true),
            ],
        );
        let mut tree = Tree::new(&element);
        let limits = layout::Limits::new(Size::ZERO, Size::new(800.0, 600.0));
        let original = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        assert_eq!(original.children()[0].size().width, 400.0);
        // Navigating away with a sidebar hidden snaps: no intermediate
        // geometry and no further frames.
        element = row_for_page(
            "Cards",
            vec![
                slot(pane(), Length::Fill, true),
                slot(pane(), Length::Fill, false),
            ],
        );
        tree.diff(&element);
        let snapped = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        assert_eq!(snapped.children()[0].size().width, 800.0);
        assert_eq!(snapped.children()[1].size().width, 0.0);
        let settled_at = tree.state.downcast_ref::<RowState>().now + LAYOUT;
        assert_eq!(
            frame(&mut element, &mut tree, &renderer, &snapped, settled_at),
            iced::window::RedrawRequest::Wait
        );
        // Toggling within the same page animates through intermediate widths.
        element = row_for_page(
            "Cards",
            vec![
                slot(pane(), Length::Fill, false),
                slot(pane(), Length::Fill, true),
            ],
        );
        tree.diff(&element);
        let start = tree.state.downcast_ref::<RowState>().now;
        assert_eq!(
            frame(
                &mut element,
                &mut tree,
                &renderer,
                &snapped,
                start + LAYOUT / 2
            ),
            iced::window::RedrawRequest::NextFrame
        );
        let middle = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        let width = middle.children()[1].size().width;
        assert!(width > 400.0 && width < 800.0);
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
    fn group_header_width_morphs_without_scaling_or_changing_row_allocation() {
        set_reduced(false);
        let _clock = FixedTime::new(Instant::now());
        let renderer = renderer();
        let positions = Positions::default();
        let make = |width, generation| {
            reflow_card(
                positions.clone(),
                "header",
                generation,
                true,
                iced::widget::Space::new().width(width).height(100),
            )
        };
        let mut element: Element<'_, ()> = make(800.0, 1);
        let mut tree = Tree::new(&element);
        let limits = layout::Limits::new(Size::ZERO, Size::new(800.0, 600.0));
        let node = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        frame(&mut element, &mut tree, &renderer, &node, now());
        element = make(200.0, 2);
        tree.diff(&element);
        let start = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        assert_eq!(start.size().width, 200.0);
        assert_eq!(start.children()[0].size().width, 800.0);
        FRAME_TIME.set(Some(now() + LAYOUT / 2));
        let middle = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        assert!(middle.children()[0].size().width > 200.0);
        assert!(middle.children()[0].size().width < 800.0);
        assert_eq!(middle.children()[0].size().height, 100.0);
        FRAME_TIME.set(Some(now() + LAYOUT));
        let end = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        assert_eq!(end.children()[0].size().width, 200.0);
    }

    #[test]
    fn packed_header_retains_its_height_when_the_grid_mounts_a_new_widget_tree() {
        set_reduced(false);
        let _clock = FixedTime::new(Instant::now());
        let renderer = renderer();
        let positions = Positions::default();
        let mut expanded: Element<'_, ()> = reflow_card(
            positions.clone(),
            "header",
            1,
            true,
            iced::widget::Space::new().width(800).height(84),
        );
        let mut tree = Tree::new(&expanded);
        let limits = layout::Limits::new(Size::ZERO, Size::new(800.0, 600.0));
        let node = expanded
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        frame(&mut expanded, &mut tree, &renderer, &node, now());
        let mut packed: Element<'_, ()> = reflow_card(
            positions.clone(),
            "header",
            2,
            true,
            iced::widget::Space::new().width(200).height(112),
        );
        let mut new_tree = Tree::new(&packed);
        packed
            .as_widget_mut()
            .layout(&mut new_tree, &renderer, &limits);
        assert_eq!(
            positions.0.lock().unwrap()["header"]
                .height_motion
                .value(now()),
            84.0
        );
        FRAME_TIME.set(Some(now() + LAYOUT));
        assert_eq!(
            positions.0.lock().unwrap()["header"]
                .height_motion
                .value(now()),
            112.0
        );
    }

    #[test]
    fn menu_exit_retains_geometry_and_reverses_without_restarting() {
        set_reduced(false);
        let _clock = FixedTime::new(Instant::now());
        let mut menu = MenuMotion::default();
        assert!(!menu.exiting());
        menu.start();
        FRAME_TIME.set(Some(now() + ENTRANCE));
        menu.close();
        assert!(menu.exiting());
        FRAME_TIME.set(Some(now() + ENTRANCE / 2));
        let middle = menu.progress.value(now());
        assert!(middle > 0.0 && middle < 1.0);
        menu.start();
        assert_eq!(menu.progress.value(now()), middle);
        set_reduced(true);
        menu.close();
        assert!(!menu.exiting());
        set_reduced(false);
    }

    #[test]
    fn overlay_escape_retains_content_and_delivers_action_once_after_exit() {
        set_reduced(false);
        let _clock = FixedTime::new(Instant::now());
        let renderer = renderer();
        let mut element = dismissible(
            iced::widget::Space::new().width(100).height(100),
            |_| true,
            Some("close"),
        );
        let mut tree = Tree::new(&element);
        let limits = layout::Limits::new(Size::ZERO, Size::new(320.0, 900.0));
        let node = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        let mut messages = Vec::new();
        let escape = Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
            modified_key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
            physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Escape),
            location: iced::keyboard::Location::Standard,
            modifiers: iced::keyboard::Modifiers::empty(),
            text: None,
            repeat: false,
        });
        for (advance, event) in [
            (Duration::ZERO, escape),
            (
                ENTRANCE / 2,
                Event::Window(iced::window::Event::RedrawRequested(now() + ENTRANCE / 2)),
            ),
            (
                ENTRANCE,
                Event::Window(iced::window::Event::RedrawRequested(now() + ENTRANCE)),
            ),
            (
                ENTRANCE,
                Event::Window(iced::window::Event::RedrawRequested(now() + ENTRANCE)),
            ),
        ] {
            element.as_widget_mut().update(
                &mut tree,
                &event,
                Layout::new(&node),
                mouse::Cursor::Unavailable,
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut Shell::new(&mut messages),
                &Rectangle::with_size(Size::new(320.0, 900.0)),
            );
            if advance < ENTRANCE {
                assert!(messages.is_empty());
            }
        }
        assert_eq!(messages, vec!["close"]);
    }

    #[test]
    fn a_group_growing_to_full_width_does_not_slide_over_its_previous_row() {
        set_reduced(false);
        let renderer = renderer();
        let positions = Positions::default();
        let mut element: Element<'_, ()> = reflow(
            positions.clone(),
            "group",
            1,
            true,
            iced::widget::Space::new().width(200).height(100),
        );
        let mut tree = Tree::new(&element);
        let limits = layout::Limits::new(Size::ZERO, Size::new(800.0, 600.0));
        let start = Instant::now();
        let compact = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits)
            .move_to(Point::new(220.0, 0.0));
        frame(&mut element, &mut tree, &renderer, &compact, start);
        element = reflow(
            positions,
            "group",
            2,
            true,
            iced::widget::Space::new().width(800).height(100),
        );
        tree.diff(&element);
        let expanded = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits)
            .move_to(Point::new(0.0, 120.0));
        frame(
            &mut element,
            &mut tree,
            &renderer,
            &expanded,
            start + Duration::from_millis(16),
        );
        assert_eq!(
            tree.state.downcast_ref::<ReflowState>().offset,
            Vector::ZERO
        );
    }

    #[test]
    fn collapsing_group_travels_back_to_its_compact_row_after_content_reveal() {
        set_reduced(false);
        let renderer = renderer();
        let positions = Positions::default();
        let mut element: Element<'_, ()> = reflow(
            positions.clone(),
            "group",
            1,
            false,
            iced::widget::Space::new().width(800).height(100),
        );
        let mut tree = Tree::new(&element);
        let limits = layout::Limits::new(Size::ZERO, Size::new(800.0, 600.0));
        let start = Instant::now();
        let expanded = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits)
            .move_to(Point::new(0.0, 120.0));
        frame(&mut element, &mut tree, &renderer, &expanded, start);
        element = reflow(
            positions,
            "group",
            2,
            true,
            iced::widget::Space::new().width(200).height(100),
        );
        tree.diff(&element);
        let compact = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits)
            .move_to(Point::new(220.0, 0.0));
        frame(&mut element, &mut tree, &renderer, &compact, start);
        assert_eq!(
            tree.state.downcast_ref::<ReflowState>().offset,
            Vector::new(0.0, 120.0)
        );
        frame(
            &mut element,
            &mut tree,
            &renderer,
            &compact,
            start + LAYOUT / 2,
        );
        let middle = tree.state.downcast_ref::<ReflowState>().offset;
        assert!(middle.y > 0.0 && middle.y < 120.0);
        frame(&mut element, &mut tree, &renderer, &compact, start + LAYOUT);
        assert_eq!(
            tree.state.downcast_ref::<ReflowState>().offset,
            Vector::ZERO
        );
    }

    #[test]
    fn reflow_slides_vertically_and_snaps_horizontally_on_diagonal_moves() {
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
        element = reflow(positions, "card", 2, true, card());
        tree.diff(&element);
        // A group collapse shifts siblings mostly vertically with a small
        // column shift: the card should slide vertically and snap
        // horizontally instead of flying diagonally.
        let node = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits)
            .move_to(Point::new(40.0, 300.0));
        frame(&mut element, &mut tree, &renderer, &node, start + LAYOUT);
        let offset = tree.state.downcast_ref::<ReflowState>().offset;
        assert_ne!(offset, Vector::ZERO);
        assert_eq!(offset.x, 0.0);
        assert!(offset.y < 0.0);
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
