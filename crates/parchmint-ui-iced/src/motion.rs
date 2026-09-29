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
pub(crate) const FOCUS_LAYOUT: Duration = Duration::from_millis(320);
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

fn encloses(outer: Rectangle, inner: Rectangle) -> bool {
    inner.x >= outer.x - 0.5
        && inner.y >= outer.y - 0.5
        && inner.x + inner.width <= outer.x + outer.width + 0.5
        && inner.y + inner.height <= outer.y + outer.height + 0.5
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
#[cfg(test)]
pub(crate) fn row<'a, Message: 'a>(slots: Vec<Slot<'a, Message>>) -> Element<'a, Message> {
    Element::new(MotionRow {
        slots,
        animated: true,
        shrink: false,
        page: String::new(),
        duration: LAYOUT,
        anchor_leading: false,
    })
}

#[cfg(test)]
pub(crate) fn row_focus<'a, Message: 'a>(slots: Vec<Slot<'a, Message>>) -> Element<'a, Message> {
    row_for_focus_page("", slots)
}

pub(crate) fn row_shrink_focus_for_page<'a, Message: 'a>(
    page: impl Into<String>,
    slots: Vec<Slot<'a, Message>>,
) -> Element<'a, Message> {
    Element::new(MotionRow {
        slots,
        animated: true,
        shrink: true,
        page: page.into(),
        duration: FOCUS_LAYOUT,
        anchor_leading: false,
    })
}

#[cfg(test)]
pub(crate) fn row_shrink<'a, Message: 'a>(slots: Vec<Slot<'a, Message>>) -> Element<'a, Message> {
    Element::new(MotionRow {
        slots,
        animated: true,
        shrink: true,
        page: String::new(),
        duration: LAYOUT,
        anchor_leading: false,
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
        duration: LAYOUT,
        anchor_leading: false,
    })
}

pub(crate) fn row_for_focus_page<'a, Message: 'a>(
    page: impl Into<String>,
    slots: Vec<Slot<'a, Message>>,
) -> Element<'a, Message> {
    Element::new(MotionRow {
        slots,
        animated: true,
        shrink: false,
        page: page.into(),
        duration: FOCUS_LAYOUT,
        anchor_leading: false,
    })
}

/// A compact fixed-slot row whose outside bounds follow the reveal itself.
/// Used by shared header controls so an outer container cannot cut the exit
/// short by snapping immediately to the final collapsed width.
pub(crate) fn row_shrink_focus<'a, Message: 'a>(
    slots: Vec<Slot<'a, Message>>,
) -> Element<'a, Message> {
    Element::new(MotionRow {
        slots,
        animated: true,
        shrink: true,
        page: String::new(),
        duration: FOCUS_LAYOUT,
        anchor_leading: false,
    })
}

pub(crate) fn row_instant<'a, Message: 'a>(slots: Vec<Slot<'a, Message>>) -> Element<'a, Message> {
    Element::new(MotionRow {
        slots,
        animated: false,
        shrink: false,
        page: String::new(),
        duration: LAYOUT,
        anchor_leading: false,
    })
}
pub(crate) fn row_editor<'a, Message: 'a>(slots: Vec<Slot<'a, Message>>) -> Element<'a, Message> {
    Element::new(MotionRow {
        slots,
        animated: true,
        shrink: false,
        page: String::new(),
        duration: FOCUS_LAYOUT,
        anchor_leading: true,
    })
}
struct MotionRow<'a, Message> {
    slots: Vec<Slot<'a, Message>>,
    animated: bool,
    shrink: bool,
    page: String,
    duration: Duration,
    anchor_leading: bool,
}
struct RowState {
    reveals: Vec<Tween>,
    shares: Vec<Tween>,
    widths: Vec<f32>,
    now: Instant,
    page: String,
}

/// Interpolate pane shares directly. Normalizing each frame's revealed weights
/// makes the remaining pane surge toward its destination near the end of a
/// Focus transition, even when the reveal tween itself is smooth.
fn fill_shares<Message>(slots: &[Slot<'_, Message>]) -> Vec<f32> {
    let total: f32 = slots
        .iter()
        .filter(|slot| slot.visible)
        .map(|slot| match slot.width {
            Length::FillPortion(weight) => f32::from(weight),
            Length::Fill => 1.0,
            _ => 0.0,
        })
        .sum();
    slots
        .iter()
        .map(|slot| {
            if !slot.visible {
                return 0.0;
            }
            match slot.width {
                Length::FillPortion(weight) => f32::from(weight) / total.max(0.001),
                Length::Fill => 1.0 / total.max(0.001),
                _ => 0.0,
            }
        })
        .collect()
}
impl<Message> Widget<Message, iced::Theme, iced::Renderer> for MotionRow<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<RowState>()
    }
    fn state(&self) -> tree::State {
        let now = now();
        tree::State::new(RowState {
            widths: vec![320.0; self.slots.len()],
            shares: fill_shares(&self.slots)
                .into_iter()
                .map(|share| Tween::new(share, now, self.duration))
                .collect(),
            reveals: self
                .slots
                .iter()
                .map(|slot| Tween::new(if slot.visible { 1.0 } else { 0.0 }, now, self.duration))
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
        // replay sidebar motion.
        let navigated = state.page != self.page;
        if navigated {
            state.page.clone_from(&self.page);
        }
        let previous_len = state.reveals.len();
        state.widths.resize(self.slots.len(), 320.0);
        state
            .reveals
            .resize_with(self.slots.len(), || Tween::new(0.0, now, self.duration));
        state
            .shares
            .resize_with(self.slots.len(), || Tween::new(0.0, now, self.duration));
        let target_shares = fill_shares(&self.slots);
        for (index, (reveal, slot)) in state.reveals.iter_mut().zip(&self.slots).enumerate() {
            reveal.duration = self.duration;
            state.shares[index].duration = self.duration;
            // Toggling a pane within a screen animates. Mounting entirely new
            // slots snaps in instead: sliding sidebars on structural changes
            // is disruptive. Page changes snap via the page key above.
            let added = index >= previous_len;
            reveal.set(
                if slot.visible { 1.0 } else { 0.0 },
                now,
                self.animated && enabled() && !added && !navigated,
            );
            state.shares[index].set(
                target_shares[index],
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
            .enumerate()
            .map(|(index, (((slot, tree), fraction), previous_width))| {
                let width = match slot.width {
                    Length::Fixed(w) => w * fraction,
                    Length::FillPortion(_) | Length::Fill => {
                        let share = if self.animated && enabled() {
                            state.shares[index].value(state.now)
                        } else {
                            state.shares[index].target
                        };
                        (size.width - fixed).max(0.0) * share
                    }
                    _ => 0.0,
                };
                // Fully hidden panes have no geometry to paint or hit. In
                // particular, do not lay out a second rich-text editor on
                // every animation frame when the companion pane is closed.
                if width <= 0.5 && !slot.visible {
                    let child = layout::Node::with_children(
                        Size::new(width, size.height),
                        vec![layout::Node::new(Size::ZERO)],
                    )
                    .move_to(Point::new(x, 0.0));
                    x += width;
                    return child;
                }
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
                    // Side panels travel with the edge from which they enter.
                    // Keeping their full layout width avoids rewrapping labels
                    // while the visible slot crosses the screen.
                    let offset = if self.shrink || self.anchor_leading {
                        0.0
                    } else if index == 0 {
                        width - content_width
                    } else {
                        0.0
                    };
                    content.move_to(Point::new(offset, 0.0))
                } else if self.anchor_leading {
                    // The primary editor leaves through the left edge when
                    // the companion takes Focus. Move its full-width content
                    // with that edge instead of clipping its first glyphs
                    // into a narrow stationary column.
                    if index == 0 && width < content_width {
                        content.move_to(Point::new(width - content_width, 0.0))
                    } else {
                        content
                    }
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
            let was_active = state.reveals.iter().any(|reveal| reveal.active(state.now))
                || state.shares.iter().any(|share| share.active(state.now));
            state.now = *now;
            if was_active {
                shell.invalidate_layout();
            }
            if self.animated
                && enabled()
                && (state.reveals.iter().any(|reveal| reveal.active(*now))
                    || state.shares.iter().any(|share| share.active(*now)))
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
                    .or_else(|| {
                        // A just-opened pane already owns focus even on its
                        // zero-width first frame. Text input is not hit testing.
                        matches!(event, Event::Keyboard(_) | Event::InputMethod(_))
                            .then(|| layout.child(0).bounds())
                    })
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
                let draw = |renderer: &mut iced::Renderer| {
                    slot.content.as_widget().draw(
                        tree,
                        renderer,
                        theme,
                        style,
                        layout.child(0),
                        clipped_cursor(cursor, viewport),
                        &viewport,
                    );
                };
                if encloses(viewport, layout.child(0).bounds()) {
                    draw(renderer);
                } else {
                    renderer.with_layer(viewport, draw);
                }
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
            .filter(|((slot, _), layout)| slot.visible && layout.bounds().width > 0.5)
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
            .filter(|((slot, _), layout)| slot.visible && layout.bounds().width > 0.5)
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

/// Keep manuscript line breaks at their destination width while the pane
/// around them moves. Only the body is measured this way; the pane surface,
/// tabs, and breadcrumbs still fill their animated bounds.
pub(crate) fn stable_measure<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    destination_width: f32,
) -> Element<'a, Message> {
    Element::new(StableMeasure {
        content: content.into(),
        destination_width: destination_width.max(1.0),
    })
}

struct StableMeasure<'a, Message> {
    content: Element<'a, Message>,
    destination_width: f32,
}

impl<Message> Widget<Message, iced::Theme, iced::Renderer> for StableMeasure<'_, Message> {
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
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
        let size = limits.max();
        let width = self.destination_width;
        let child = self.content.as_widget_mut().layout(
            &mut tree.children[0],
            renderer,
            &layout::Limits::new(Size::new(width, size.height), Size::new(width, size.height)),
        );
        let center = |width: f32| (width - width.min(800.0)) * 0.5;
        layout::Node::with_children(
            size,
            vec![child.move_to(Point::new(center(size.width) - center(width), 0.0))],
        )
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
        if let Some(clip) = viewport.intersection(&layout.bounds()) {
            self.content.as_widget_mut().update(
                &mut tree.children[0],
                event,
                layout.child(0),
                clipped_cursor(cursor, clip),
                renderer,
                clipboard,
                shell,
                &clip,
            );
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
        if let Some(clip) = viewport.intersection(&layout.bounds()) {
            let draw = |renderer: &mut iced::Renderer| {
                self.content.as_widget().draw(
                    &tree.children[0],
                    renderer,
                    theme,
                    style,
                    layout.child(0),
                    clipped_cursor(cursor, clip),
                    &clip,
                );
            };
            if encloses(clip, layout.child(0).bounds()) {
                draw(renderer);
            } else {
                renderer.with_layer(clip, draw);
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
        viewport
            .intersection(&layout.bounds())
            .map_or(mouse::Interaction::default(), |clip| {
                self.content.as_widget().mouse_interaction(
                    &tree.children[0],
                    layout.child(0),
                    clipped_cursor(cursor, clip),
                    &clip,
                    renderer,
                )
            })
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

pub(crate) fn enter<'a, Message: 'a>(
    key: impl Into<String>,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    Element::new(Entrance {
        key: key.into(),
        content: content.into(),
        reveal: None,
        resize: false,
        clip_resize: true,
        disclosure: None,
        on_complete: None,
        origin: None,
        slide_from_top: false,
        clip_below: None,
        focus_duration: false,
        allocated_height: None,
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
        clip_resize: true,
        disclosure: None,
        on_complete: None,
        origin: None,
        slide_from_top: false,
        clip_below: None,
        focus_duration: false,
        allocated_height: None,
    })
}
pub(crate) fn reveal_focus_for_page<'a, Message: 'a>(
    page: impl Into<String>,
    visible: bool,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    Element::new(Entrance {
        key: page.into(),
        content: content.into(),
        reveal: Some(visible),
        resize: false,
        clip_resize: true,
        disclosure: None,
        on_complete: None,
        origin: None,
        slide_from_top: false,
        clip_below: None,
        focus_duration: true,
        allocated_height: None,
    })
}
/// Reveal a row from the edge above it while allocating its height. The
/// content and the rows below travel on the same timeline.
pub(crate) fn reveal_down<'a, Message: 'a>(
    visible: bool,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    Element::new(Entrance {
        key: String::new(),
        content: content.into(),
        reveal: Some(visible),
        resize: false,
        clip_resize: true,
        disclosure: None,
        on_complete: None,
        origin: None,
        slide_from_top: true,
        clip_below: None,
        focus_duration: true,
        allocated_height: None,
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
        clip_resize: true,
        disclosure: None,
        on_complete: None,
        origin: None,
        slide_from_top: false,
        clip_below: None,
        focus_duration: false,
        allocated_height: None,
    })
}
/// Group fields retain their own trajectories while the header surface resizes.
/// Their visual envelope may exceed the allocated header until they settle;
/// the enclosing group already reserves this space and the scroll viewport clips it.
pub(crate) fn resize_group_height<'a, Message: 'a>(
    key: impl Into<String>,
    timeline: Option<Disclosure>,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    Element::new(Entrance {
        key: key.into(),
        content: content.into(),
        reveal: Some(true),
        resize: true,
        clip_resize: false,
        disclosure: timeline,
        on_complete: None,
        origin: None,
        slide_from_top: false,
        clip_below: None,
        focus_duration: false,
        allocated_height: None,
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
        clip_resize: true,
        disclosure: Some(timeline),
        on_complete,
        origin: None,
        slide_from_top: false,
        clip_below: None,
        focus_duration: false,
        allocated_height: None,
    })
}

/// On collapse, the compact heading travels across the outgoing child rows.
/// Paint those rows only below its current lower edge so their text cannot
/// cross over the moving heading.
pub(crate) fn disclosure_below_card<'a, Message: 'a>(
    timeline: Disclosure,
    positions: Positions,
    card_id: String,
    content: impl Into<Element<'a, Message>>,
    on_complete: Option<Message>,
) -> Element<'a, Message> {
    Element::new(Entrance {
        key: String::new(),
        content: content.into(),
        reveal: Some(timeline.visible()),
        resize: false,
        clip_resize: true,
        disclosure: Some(timeline),
        on_complete,
        origin: None,
        slide_from_top: false,
        clip_below: Some((positions, card_id)),
        focus_duration: false,
        allocated_height: None,
    })
}

/// Space whose height is read during layout. Disclosure rows already request
/// layout on every frame; a reserve captured while building the view would
/// otherwise remain at its opening value while the children keep growing.
pub(crate) fn live_height<'a, Message: 'a>(height: impl Fn() -> f32 + 'a) -> Element<'a, Message> {
    Element::new(LiveHeight {
        height: Box::new(height),
    })
}

/// Follow the projected grid allocation without clipping cards traveling into
/// a neighboring row. The nested disclosures own redraws and child clipping.
pub(crate) fn allocate_height<'a, Message: 'a>(
    height: impl Fn() -> Option<f32> + 'a,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    Element::new(Entrance {
        key: String::new(),
        content: content.into(),
        reveal: Some(true),
        resize: false,
        clip_resize: false,
        disclosure: None,
        on_complete: None,
        origin: None,
        slide_from_top: false,
        clip_below: None,
        focus_duration: false,
        allocated_height: Some(Box::new(height)),
    })
}

struct LiveHeight<'a> {
    height: Box<dyn Fn() -> f32 + 'a>,
}

impl<Message> Widget<Message, iced::Theme, iced::Renderer> for LiveHeight<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Shrink)
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(
            limits,
            Length::Shrink,
            Length::Fixed((self.height)().max(0.0)),
        )
    }

    fn draw(
        &self,
        _tree: &Tree,
        _renderer: &mut iced::Renderer,
        _theme: &iced::Theme,
        _style: &renderer::Style,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
    }
}

struct Entrance<'a, Message> {
    key: String,
    content: Element<'a, Message>,
    reveal: Option<bool>,
    resize: bool,
    clip_resize: bool,
    disclosure: Option<Disclosure>,
    on_complete: Option<Message>,
    origin: Option<Rc<Cell<Point>>>,
    slide_from_top: bool,
    clip_below: Option<(Positions, String)>,
    focus_duration: bool,
    allocated_height: Option<Box<dyn Fn() -> Option<f32> + 'a>>,
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
        let duration = if self.focus_duration {
            FOCUS_LAYOUT
        } else if self.reveal.is_some() {
            LAYOUT
        } else {
            ENTRANCE
        };
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
            state.progress = Tween::new(
                f32::from(self.reveal.unwrap_or(false)),
                state.now,
                if self.reveal.is_some() {
                    if self.focus_duration {
                        FOCUS_LAYOUT
                    } else {
                        LAYOUT
                    }
                } else {
                    ENTRANCE
                },
            );
            state.progress.set(
                f32::from(self.reveal.unwrap_or(true)),
                state.now,
                enabled() && self.reveal.is_none(),
            );
        }
        if let Some(visible) = self.reveal.filter(|_| !self.resize) {
            state.now = now();
            state.progress.duration = if self.focus_duration {
                FOCUS_LAYOUT
            } else {
                LAYOUT
            };
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
        let child = self
            .content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        let offset = if self.allocated_height.is_some() {
            0.0
        } else if self.slide_from_top {
            // The tab strip and breadcrumbs emerge from the toolbar edge.
            // Sliding a full row height kept their labels clipped until late
            // in the transition, so they appeared abruptly after the panes
            // had already moved. A short travel keeps them visible earlier
            // while the allocated rows still expand on the shared timeline.
            -6.0 * (1.0 - progress)
        } else if self.reveal.is_none() && self.origin.is_none() {
            8.0 * (1.0 - progress)
        } else {
            0.0
        };
        let mut size = child.size();
        if self.resize {
            let now = now();
            let started = self
                .disclosure
                .as_ref()
                .map_or(now, |disclosure| disclosure.0.started);
            if state.height == 0.0 {
                state.progress = Tween::new(size.height, now, LAYOUT);
            }
            state.height = size.height;
            state.progress.set(size.height, started, enabled());
            state.now = now;
            size.height = state.progress.value(now);
        } else if self.reveal.is_some() {
            if self.disclosure.is_some() || self.reveal == Some(true) || state.height == 0.0 {
                state.height = size.height;
            }
            size.height = state.height * progress;
        }
        if let Some(height) = self.allocated_height.as_ref().and_then(|height| height()) {
            size.height = height.max(0.0);
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
        if self.allocated_height.is_none()
            && self.reveal.is_some()
            && (!self.resize || self.clip_resize)
        {
            if let Some(mut clip) = viewport
                .intersection(&layout.bounds())
                .filter(|bounds| bounds.height > 0.0)
            {
                if let Some((positions, card_id)) = &self.clip_below
                    && let Some(bottom) = positions.visual_bottom(card_id, now())
                {
                    // Clear the heading's lower edge plus the child card's
                    // first text line. Clipping precisely at the edge leaves
                    // partial glyphs visible for a frame as the two move.
                    let top = if bottom > clip.y {
                        bottom + 28.0
                    } else {
                        clip.y
                    };
                    clip.height = (clip.y + clip.height - top).max(0.0);
                    clip.y = top;
                }
                if clip.height <= 0.0 {
                    return;
                }
                let draw = |renderer: &mut iced::Renderer| {
                    self.content.as_widget().draw(
                        &tree.children[0],
                        renderer,
                        theme,
                        style,
                        layout.child(0),
                        cursor,
                        &clip,
                    );
                };
                if encloses(clip, layout.child(0).bounds()) {
                    draw(renderer);
                } else {
                    renderer.with_layer(clip, draw);
                }
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
            morph_width: true,
            allocation_height: None,
            timeline: None,
            destination: Box::new(|point| point),
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
            clip_resize: true,
            disclosure: None,
            on_complete: None,
            origin: Some(self.origin.clone()),
            slide_from_top: false,
            clip_below: None,
            focus_duration: false,
            allocated_height: None,
        })
    }
}

#[derive(Debug)]
struct Placement {
    generation: u64,
    target: Point,
    layout_target: Point,
    width: f32,
    width_motion: Tween,
    height_motion: Tween,
    x: Tween,
    y: Tween,
}
impl Positions {
    pub(crate) fn layout_position(&self, id: &str) -> Option<Point> {
        self.0
            .lock()
            .expect("motion positions")
            .get(id)
            .map(|place| place.layout_target)
    }
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

    pub(crate) fn frame_horizontal(&self, id: &str, at: Instant) -> Option<(f32, f32)> {
        self.0
            .lock()
            .expect("motion positions")
            .get(id)
            .map(|place| {
                (
                    place.x.value(at) - place.layout_target.x,
                    place.width_motion.value(at),
                )
            })
    }

    pub(crate) fn vertical_offset(&self, id: &str, now: Instant) -> f32 {
        self.0
            .lock()
            .expect("motion positions")
            .get(id)
            .map(|place| place.y.value(now) - place.layout_target.y)
            .unwrap_or(0.0)
    }

    fn visual_bottom(&self, id: &str, at: Instant) -> Option<f32> {
        self.0
            .lock()
            .expect("motion positions")
            .get(id)
            .map(|place| place.y.value(at) + place.height_motion.value(at))
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
        morph_width: false,
        allocation_height: None,
        timeline: None,
        destination: Box::new(|point| point),
        content: content.into(),
    })
}
pub(crate) fn reflow_to<'a, Message: 'a>(
    positions: Positions,
    id: impl Into<String>,
    generation: u64,
    destination: impl Fn(Point) -> Point + 'a,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    Element::new(Reflow {
        positions,
        id: id.into(),
        generation,
        animate: true,
        origin: None,
        morph_width: false,
        allocation_height: None,
        timeline: None,
        destination: Box::new(destination),
        content: content.into(),
    })
}
/// Overview card surfaces change width while their content remains real text.
/// The grid reserves final slots; the moving surface interpolates its measured
/// width rather than scaling typography or snapping between card and heading.
#[cfg(test)]
pub(crate) fn reflow_card<'a, Message: 'a>(
    positions: Positions,
    id: impl Into<String>,
    generation: u64,
    animate: bool,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    reflow_card_height_to(
        positions,
        id,
        generation,
        animate,
        None,
        |point| point,
        content,
    )
}

pub(crate) fn reflow_card_height_to<'a, Message: 'a>(
    positions: Positions,
    id: impl Into<String>,
    generation: u64,
    animate: bool,
    allocation: Option<(f32, Option<Disclosure>)>,
    destination: impl Fn(Point) -> Point + 'a,
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    Element::new(Reflow {
        positions,
        id: id.into(),
        generation,
        animate,
        origin: None,
        morph_width: true,
        allocation_height: allocation.as_ref().map(|(height, _)| *height),
        timeline: allocation.and_then(|(_, timeline)| timeline),
        destination: Box::new(destination),
        content: content.into(),
    })
}

struct Reflow<'a, Message> {
    positions: Positions,
    id: String,
    generation: u64,
    animate: bool,
    origin: Option<Rc<Cell<Point>>>,
    morph_width: bool,
    allocation_height: Option<f32>,
    timeline: Option<Disclosure>,
    destination: Box<dyn Fn(Point) -> Point + 'a>,
    content: Element<'a, Message>,
}
fn resize_shell_width(node: &layout::Node, original: f32, width: f32) -> layout::Node {
    if (node.size().width - original).abs() > 0.1 {
        return node.clone();
    }
    let children = if node.children().len() == 1 {
        vec![resize_shell_width(&node.children()[0], original, width)]
    } else {
        node.children().to_vec()
    };
    layout::Node::with_children(Size::new(width, node.size().height), children)
        .move_to(node.bounds().position())
}

struct ReflowState {
    id: String,
    offset: Vector,
}

fn node_from_layout(layout: Layout<'_>) -> layout::Node {
    let position = layout.position();
    layout::Node::with_children(
        layout.bounds().size(),
        layout
            .children()
            .map(|child| {
                let offset = child.position() - position;
                node_from_layout(child).move_to(Point::ORIGIN + offset)
            })
            .collect(),
    )
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
        let mut allocated = child.size();
        if let Some(height) = self.allocation_height {
            allocated.height = height;
        }
        if self.morph_width {
            let at = now();
            let started = self
                .timeline
                .as_ref()
                .map_or(at, |timeline| timeline.0.started);
            let width = {
                let mut positions = self.positions.0.lock().expect("motion positions");
                positions.get_mut(&self.id).map(|place| {
                    place
                        .width_motion
                        .set(allocated.width, started, self.animate && enabled());
                    let morphing = place.width_motion.active(at) || place.height_motion.active(at);
                    place.height_motion.set(
                        allocated.height,
                        started,
                        self.animate && enabled() && morphing,
                    );
                    place.width_motion.value(at)
                })
            };
            let width = width.map(|width| {
                if let Some((node, "synopsis")) = self.id.split_once('\0') {
                    if (width - allocated.width).abs() > 0.1 {
                        let full = Size::new(width.max(1.0), f32::INFINITY);
                        child = self.content.as_widget_mut().layout(
                            &mut tree.children[0],
                            renderer,
                            &layout::Limits::new(Size::new(full.width, 0.0), full),
                        );
                    }
                    let positions = self.positions.0.lock().expect("motion positions");
                    if let Some(synopsis) = positions.get(&self.id) {
                        let left = synopsis.x.value(at);
                        let top = synopsis.y.value(at);
                        let bottom = top + child.size().height;
                        let minimum = synopsis.width_motion.from.min(synopsis.width_motion.target);
                        return positions
                            .iter()
                            .filter(|(id, _)| {
                                id.split_once('\0').is_some_and(|(other, field)| {
                                    other == node
                                        && !matches!(field, "synopsis" | "title" | "words")
                                })
                            })
                            .fold(width, |width, (_, metadata)| {
                                let metadata_top = metadata.y.value(at);
                                let metadata_bottom =
                                    metadata_top + metadata.height_motion.value(at);
                                let available = metadata.x.value(at) - left - 12.0;
                                if metadata_top < bottom
                                    && metadata_bottom > top
                                    && available >= 32.0
                                {
                                    width.min(available.max(minimum))
                                } else {
                                    width
                                }
                            });
                    }
                }
                width
            });
            if let Some(width) = width.filter(|width| (*width - allocated.width).abs() > 0.1) {
                if self.allocation_height.is_some() {
                    // The shell paints and receives hits at the animated width.
                    // Padding marks the boundary of its content: leave that
                    // subtree at final width so local field trajectories have
                    // stable destinations on both opening and closing.
                    child = resize_shell_width(&child, child.size().width, width.max(1.0));
                } else {
                    let full = Size::new(
                        width.max(1.0),
                        if self.origin.is_some() {
                            f32::INFINITY
                        } else {
                            limits.max().height
                        },
                    );
                    child = self.content.as_widget_mut().layout(
                        &mut tree.children[0],
                        renderer,
                        &layout::Limits::new(Size::new(full.width, 0.0), full),
                    );
                }
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
            let layout_target = self.origin.as_ref().map_or(layout.position(), |origin| {
                Point::ORIGIN + (layout.position() - origin.get())
            });
            let target = (self.destination)(layout_target);
            let target_shift = target - layout_target;
            let mut positions = self.positions.0.lock().expect("motion positions");
            let place = positions
                .entry(self.id.clone())
                .or_insert_with(|| Placement {
                    generation: self.generation,
                    target,
                    layout_target,
                    width: layout.bounds().width,
                    width_motion: Tween::new(layout.bounds().width, *now, LAYOUT),
                    height_motion: Tween::new(layout.bounds().height, *now, LAYOUT),
                    x: Tween::new(target.x, *now, LAYOUT),
                    y: Tween::new(target.y, *now, LAYOUT),
                });
            let resized = (place.width - layout.bounds().width).abs() > 0.5;
            let was_shifted = place.target != place.layout_target;
            if place.target != target || resized || !self.animate || !enabled() {
                // Growing to a full-width heading cannot travel through the
                // former compact row without covering its neighbors. A heading
                // that has become compact can travel back into its packed row.
                let grew = layout.bounds().width > place.width + 0.5;
                let animate = self.animate
                    && enabled()
                    && (!grew || self.morph_width)
                    && place.generation != self.generation;
                if animate {
                    // Move both coordinates on the same eased timeline. A
                    // diagonal reflow otherwise teleports the horizontal
                    // coordinate before beginning its vertical travel.
                    let current_x = place.x.value(*now);
                    let current_y = place.y.value(*now);
                    let dx = (target.x - current_x).abs();
                    let dy = (target.y - current_y).abs();
                    // The card already carries its heading as it moves and
                    // changes width. A second local tween would leave its
                    // title and word count floating outside the surface.
                    let heading = self.origin.is_some()
                        && self
                            .id
                            .split_once('\0')
                            .is_some_and(|(_, field)| matches!(field, "title" | "words"));
                    let animate_y = dy >= 1.0 && !heading;
                    let animate_x = dx >= 1.0 && !heading;
                    // A heading clears the compact row before it reaches its
                    // full width, so its growing edge does not cover a card
                    // that preceded it in that row.
                    place.y.duration = if self.morph_width && grew {
                        LAYOUT / 2
                    } else {
                        LAYOUT
                    };
                    let started = self
                        .timeline
                        .as_ref()
                        .map_or(*now, |timeline| timeline.0.started);
                    place.x.set(target.x, started, animate_x);
                    place.y.set(target.y, started, animate_y);
                } else if target_shift != Vector::ZERO && self.animate && enabled() {
                    // The closing rows change their allocated heights every
                    // frame. Their settled target can drift a few pixels as
                    // the grid width and height resolve. Retarget the same
                    // tween without restarting it or snapping to the new
                    // target on each frame.
                    place.x.target = target.x;
                    place.y.target = target.y;
                } else if was_shifted && self.animate && enabled() {
                    // The closing rows have unmounted. Continue from the
                    // card's current painted position into its new allocated
                    // row, without a discontinuity at the handoff.
                    place.x.duration = LAYOUT / 2;
                    place.y.duration = LAYOUT / 2;
                    place.x.set(target.x, *now, true);
                    place.y.set(target.y, *now, true);
                } else {
                    place.x.set(target.x, *now, false);
                    place.y.set(target.y, *now, false);
                }
            }
            place.generation = self.generation;
            place.target = target;
            place.layout_target = layout_target;
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
                    place.x.value(*now) - layout_target.x,
                    place.y.value(*now) - layout_target.y,
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
        // A group heading can become wider while its height contracts (or
        // vice versa). Its final-layout children must never paint outside the
        // surface that is actually moving through this frame. Without this
        // clip, synopsis and metadata draw over the departing child rows.
        let moving_bounds = self
            .morph_width
            .then(|| {
                let positions = self.positions.0.lock().expect("motion positions");
                positions
                    .get(&self.id)
                    .filter(|place| {
                        enabled()
                            && (self.allocation_height.is_some()
                                || place.width_motion.active(now())
                                || place.height_motion.active(now()))
                    })
                    .map(|place| {
                        let child = layout.child(0).bounds();
                        Rectangle {
                            x: child.x + correction.x,
                            y: child.y + correction.y,
                            width: if self.allocation_height.is_some() {
                                child.width
                            } else {
                                place.width_motion.value(now())
                            },
                            height: if self.allocation_height.is_some() {
                                child.height
                            } else {
                                place.height_motion.value(now())
                            },
                        }
                    })
            })
            .flatten();
        let clip = moving_bounds
            .and_then(|bounds| viewport.intersection(&bounds))
            .unwrap_or(*viewport);
        // Retain the source height when packing moves a header into a new row
        // and therefore mounts a fresh child widget tree.
        if correction == Vector::ZERO && moving_bounds.is_none() {
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
        let desired = if enabled() {
            tree.state.downcast_ref::<ReflowState>().offset
        } else {
            Vector::ZERO
        };
        let child = layout.child(0);
        let correction = desired - (child.position() - layout.position());
        if correction == Vector::ZERO {
            self.content
                .as_widget_mut()
                .operate(&mut tree.children[0], child, renderer, operation);
        } else {
            // Redraw can advance the painted translation before layout catches
            // up. Queries and focus operations must see those same bounds.
            let node = node_from_layout(child).move_to(child.position() + correction);
            self.content.as_widget_mut().operate(
                &mut tree.children[0],
                Layout::new(&node),
                renderer,
                operation,
            );
        }
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
    fn group_children_reveal_while_the_header_shrinks() {
        set_reduced(false);
        let start = Instant::now();
        let _clock = FixedTime::new(start);
        let renderer = renderer();
        let limits = layout::Limits::new(Size::ZERO, Size::new(800.0, 900.0));
        let mut timeline = Disclosure::new(false);
        let make = |height, timeline: &Disclosure| -> Element<'_, ()> {
            iced::widget::column![
                resize_height("heading", iced::widget::Space::new().height(height)),
                disclosure(
                    timeline.clone(),
                    iced::widget::Space::new().height(200),
                    None
                ),
                disclosure(
                    timeline.clone(),
                    iced::widget::Space::new().height(200),
                    None
                ),
            ]
            .into()
        };
        let mut group = make(210, &timeline);
        let mut tree = Tree::new(&group);
        group.as_widget_mut().layout(&mut tree, &renderer, &limits);
        timeline.set(true);
        group = make(100, &timeline);
        tree.diff(&group);
        let mut concurrent = false;
        for millis in (0..=208).step_by(16) {
            let at = start + Duration::from_millis(millis);
            FRAME_TIME.set(Some(at));
            let node = group.as_widget_mut().layout(&mut tree, &renderer, &limits);
            frame(&mut group, &mut tree, &renderer, &node, at);
            let node = group.as_widget_mut().layout(&mut tree, &renderer, &limits);
            concurrent |=
                node.children()[0].size().height < 210.0 && node.children()[1].size().height > 0.0;
        }
        assert!(concurrent, "header and children should move together");
        timeline.set(false);
        FRAME_TIME.set(Some(now() + LAYOUT / 2));
        let partial = timeline.visible_fraction();
        timeline.set(true);
        assert_eq!(
            timeline.visible_fraction(),
            partial,
            "reversing does not pause or jump"
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
    fn live_reserve_shrinks_as_disclosure_grows_without_rebuilding_the_view() {
        set_reduced(false);
        let start = Instant::now();
        let _clock = FixedTime::new(start);
        let renderer = renderer();
        let limits = layout::Limits::new(Size::ZERO, Size::new(320.0, 900.0));
        let mut timeline = Disclosure::new(false);
        timeline.set(true);
        let reserve = timeline.clone();
        let children = disclosure(timeline, iced::widget::Space::new().height(200), None::<()>);
        let mut group: Element<'_, ()> = iced::widget::column![
            children,
            live_height(move || 200.0 * (1.0 - reserve.visible_fraction()))
        ]
        .into();
        let mut tree = Tree::new(&group);
        for millis in [0, 32, 96, 160, 200] {
            FRAME_TIME.set(Some(start + Duration::from_millis(millis)));
            let node = group.as_widget_mut().layout(&mut tree, &renderer, &limits);
            assert!(
                (node.size().height - 200.0).abs() < 0.01,
                "group extent changed at {millis} ms: {}",
                node.size().height
            );
        }
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
        assert_eq!(final_frame.children()[1].children()[0].size(), Size::ZERO);
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
    fn focus_pane_edges_follow_one_eased_path_in_both_directions() {
        set_reduced(false);
        let start = Instant::now();
        let _clock = FixedTime::new(start);
        let renderer = renderer();
        let pane = || {
            iced::widget::Space::new()
                .width(Length::Fill)
                .height(Length::Fill)
        };
        let make = |companion| {
            row_focus(vec![
                slot(pane(), Length::Fill, true),
                slot(pane(), Length::Fill, companion),
            ])
        };
        let mut element: Element<'_, ()> = make(true);
        let mut tree = Tree::new(&element);
        let limits = layout::Limits::new(Size::ZERO, Size::new(800.0, 600.0));
        element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        element = make(false);
        tree.diff(&element);
        for millis in [0, 48, 96, 160, 240, 320] {
            let at = start + Duration::from_millis(millis);
            let node = element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &limits);
            frame(&mut element, &mut tree, &renderer, &node, at);
            let node = element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &limits);
            let expected = 400.0 + 400.0 * standard_easing(millis as f32 / 320.0);
            assert!((node.children()[0].size().width - expected).abs() < 0.1);
            assert!((node.children()[1].size().width - (800.0 - expected)).abs() < 0.1);
        }
        FRAME_TIME.set(Some(start + FOCUS_LAYOUT));
        element = make(true);
        tree.diff(&element);
        for millis in [0, 48, 96, 160, 240, 320] {
            let at = start + FOCUS_LAYOUT + Duration::from_millis(millis);
            let node = element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &limits);
            frame(&mut element, &mut tree, &renderer, &node, at);
            let node = element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &limits);
            let expected = 800.0 - 400.0 * standard_easing(millis as f32 / 320.0);
            assert!((node.children()[0].size().width - expected).abs() < 0.1);
            assert!((node.children()[1].size().width - (800.0 - expected)).abs() < 0.1);
        }
    }

    #[test]
    fn focus_side_panels_slide_from_their_screen_edges() {
        set_reduced(false);
        let start = Instant::now();
        let _clock = FixedTime::new(start);
        let renderer = renderer();
        let pane = || {
            iced::widget::Space::new()
                .width(Length::Fill)
                .height(Length::Fill)
        };
        let make = |sides| {
            row_focus(vec![
                slot(pane(), Length::Fixed(200.0), sides),
                slot(pane(), Length::Fill, true),
                slot(pane(), Length::Fixed(100.0), sides),
            ])
        };
        let mut element: Element<'_, ()> = make(true);
        let mut tree = Tree::new(&element);
        let limits = layout::Limits::new(Size::ZERO, Size::new(800.0, 600.0));
        element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        element = make(false);
        tree.diff(&element);
        let node = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        frame(
            &mut element,
            &mut tree,
            &renderer,
            &node,
            start + FOCUS_LAYOUT / 2,
        );
        let middle = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        let left = &middle.children()[0];
        let right = &middle.children()[2];
        assert!(left.children()[0].bounds().x < left.bounds().x);
        assert_eq!(left.children()[0].bounds().x + 200.0, left.bounds().width);
        assert_eq!(right.children()[0].bounds().x, 0.0);
        assert!(right.bounds().x > 700.0 && right.bounds().x < 800.0);
    }

    #[test]
    fn primary_editor_exits_left_when_companion_takes_focus() {
        set_reduced(false);
        let start = Instant::now();
        let _clock = FixedTime::new(start);
        let renderer = renderer();
        let pane = || {
            iced::widget::Space::new()
                .width(Length::Fill)
                .height(Length::Fill)
        };
        let make = |primary| {
            row_editor(vec![
                slot(pane(), Length::Fill, primary),
                slot(pane(), Length::Fill, true),
            ])
        };
        let mut element: Element<'_, ()> = make(true);
        let mut tree = Tree::new(&element);
        let limits = layout::Limits::new(Size::ZERO, Size::new(800.0, 600.0));
        element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        element = make(false);
        tree.diff(&element);
        let node = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        frame(
            &mut element,
            &mut tree,
            &renderer,
            &node,
            start + FOCUS_LAYOUT / 2,
        );
        let middle = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        let outgoing = &middle.children()[0];
        assert!(outgoing.bounds().width > 0.0 && outgoing.bounds().width < 400.0);
        assert_eq!(
            outgoing.children()[0].bounds().x + 400.0,
            outgoing.bounds().width
        );
        assert_eq!(middle.children()[1].children()[0].bounds().x, 0.0);
    }

    #[test]
    fn navigating_snaps_project_label_and_focus_chrome() {
        set_reduced(false);
        let start = Instant::now();
        let _clock = FixedTime::new(start);
        let renderer = renderer();
        let limits = layout::Limits::new(Size::ZERO, Size::new(800.0, 600.0));
        let make_label = |page, expanded| -> Element<'_, ()> {
            row_shrink_focus_for_page(
                page,
                vec![
                    slot(
                        iced::widget::Space::new().width(48).height(48),
                        Length::Fixed(48.0),
                        true,
                    ),
                    slot(
                        iced::widget::Space::new().width(240).height(48),
                        Length::Fixed(240.0),
                        expanded,
                    ),
                ],
            )
        };
        let mut label = make_label("Cards", false);
        let mut tree = Tree::new(&label);
        label.as_widget_mut().layout(&mut tree, &renderer, &limits);
        label = make_label("Editor", true);
        tree.diff(&label);
        let snapped = label.as_widget_mut().layout(&mut tree, &renderer, &limits);
        assert_eq!(snapped.size().width, 288.0);
        assert_eq!(
            frame(&mut label, &mut tree, &renderer, &snapped, start),
            iced::window::RedrawRequest::Wait
        );
        label = make_label("Editor", false);
        tree.diff(&label);
        assert!(tree.state.downcast_ref::<RowState>().reveals[1].active(start));

        let chrome = |page, visible| -> Element<'_, ()> {
            reveal_focus_for_page(
                page,
                visible,
                iced::widget::Space::new().width(240).height(48),
            )
        };
        let mut element = chrome("Editor", false);
        let mut tree = Tree::new(&element);
        element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        element = chrome("Cards", true);
        tree.diff(&element);
        assert_eq!(
            element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &limits)
                .size()
                .height,
            48.0
        );
        element = chrome("Editor", false);
        tree.diff(&element);
        assert_eq!(
            element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &limits)
                .size()
                .height,
            0.0
        );
        element = chrome("Editor", true);
        tree.diff(&element);
        assert!(
            tree.state
                .downcast_ref::<EntranceState>()
                .progress
                .active(start)
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
    fn incoming_zero_width_pane_accepts_focus_and_immediate_typing() {
        set_reduced(false);
        let _clock = FixedTime::new(Instant::now());
        let renderer = renderer();
        let make = |visible| -> Element<'_, String> {
            row(vec![
                slot(
                    iced::widget::Space::new().width(Length::Fill),
                    Length::Fill,
                    true,
                ),
                slot(
                    iced::widget::text_input("", "")
                        .id("incoming")
                        .on_input(std::convert::identity),
                    Length::Fill,
                    visible,
                ),
            ])
        };
        let mut pane = make(false);
        let mut tree = Tree::new(&pane);
        let limits = layout::Limits::new(Size::ZERO, Size::new(800.0, 600.0));
        pane.as_widget_mut().layout(&mut tree, &renderer, &limits);
        pane = make(true);
        tree.diff(&pane);
        let node = pane.as_widget_mut().layout(&mut tree, &renderer, &limits);
        assert_eq!(node.children()[1].size().width, 0.0);
        let mut focus =
            iced::advanced::widget::operation::focusable::focus::<()>("incoming".into());
        pane.as_widget_mut()
            .operate(&mut tree, Layout::new(&node), &renderer, &mut focus);
        let event = Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: iced::keyboard::Key::Character("x".into()),
            modified_key: iced::keyboard::Key::Character("x".into()),
            physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::KeyX),
            location: iced::keyboard::Location::Standard,
            modifiers: iced::keyboard::Modifiers::empty(),
            text: Some("x".into()),
            repeat: false,
        });
        let mut messages = Vec::new();
        pane.as_widget_mut().update(
            &mut tree,
            &event,
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &renderer,
            &mut iced::advanced::clipboard::Null,
            &mut Shell::new(&mut messages),
            &Rectangle::with_size(Size::new(800.0, 600.0)),
        );
        assert_eq!(messages, ["x"]);
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
            assert_eq!(
                incoming.children()[0].size().width,
                400.0,
                "incoming panes need real layout even before revealing, so focus operations can reach them"
            );
            assert!(incoming.size().width <= 400.0);
        }
    }

    #[test]
    fn moving_card_queries_follow_the_painted_translation_before_relayout() {
        struct Bounds(Option<Rectangle>);
        impl Operation for Bounds {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                operate(self);
            }
            fn container(&mut self, id: Option<&iced::widget::Id>, bounds: Rectangle) {
                if id == Some(&iced::widget::Id::from("moving-card")) {
                    self.0 = Some(bounds);
                }
            }
        }
        set_reduced(false);
        let start = Instant::now();
        let _clock = FixedTime::new(start);
        let renderer = renderer();
        let positions = Positions::default();
        let make = |generation| -> Element<'_, ()> {
            reflow(
                positions.clone(),
                "card",
                generation,
                true,
                iced::widget::container(iced::widget::Space::new().width(80).height(40))
                    .id("moving-card"),
            )
        };
        let mut element = make(1);
        let mut tree = Tree::new(&element);
        let limits = layout::Limits::new(Size::ZERO, Size::new(800.0, 600.0));
        let node = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        frame(&mut element, &mut tree, &renderer, &node, start);
        element = make(2);
        tree.diff(&element);
        let node = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits)
            .move_to(Point::new(200.0, 100.0));
        frame(&mut element, &mut tree, &renderer, &node, start);
        let mut bounds = Bounds(None);
        element
            .as_widget_mut()
            .operate(&mut tree, Layout::new(&node), &renderer, &mut bounds);
        assert_eq!(bounds.0.unwrap().position(), Point::ORIGIN);
        frame(&mut element, &mut tree, &renderer, &node, start + LAYOUT);
        element
            .as_widget_mut()
            .operate(&mut tree, Layout::new(&node), &renderer, &mut bounds);
        assert_eq!(bounds.0.unwrap().position(), Point::new(200.0, 100.0));
    }

    #[test]
    fn packing_target_stays_fixed_when_rows_shrink_without_a_view_rebuild() {
        set_reduced(false);
        let start = Instant::now();
        let _clock = FixedTime::new(start);
        let renderer = renderer();
        let positions = Positions::default();
        let shift = Rc::new(Cell::new(Vector::new(0.0, -320.0)));
        let mut element: Element<'_, ()> = reflow(
            positions.clone(),
            "add:parent",
            1,
            true,
            iced::widget::Space::new().width(80).height(40),
        );
        let mut tree = Tree::new(&element);
        let limits = layout::Limits::new(Size::ZERO, Size::new(800.0, 600.0));
        let node = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits)
            .move_to(Point::new(0.0, 400.0));
        frame(&mut element, &mut tree, &renderer, &node, start);
        element = reflow_to(
            positions.clone(),
            "add:parent",
            2,
            {
                let shift = Rc::clone(&shift);
                move |point| point + shift.get()
            },
            iced::widget::Space::new().width(80).height(40),
        );
        tree.diff(&element);
        frame(&mut element, &mut tree, &renderer, &node, start);
        let mut previous = 400.0;
        for (millis, row_y) in [
            (40, 330.0),
            (80, 220.0),
            (120, 150.0),
            (160, 100.0),
            (200, 80.0),
        ] {
            shift.set(Vector::new(0.0, 80.0 - row_y));
            let node = element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &limits)
                .move_to(Point::new(0.0, row_y));
            let at = start + Duration::from_millis(millis);
            frame(&mut element, &mut tree, &renderer, &node, at);
            let positions = positions.0.lock().unwrap();
            let place = &positions["add:parent"];
            assert_eq!(place.target.y, 80.0);
            let y = place.y.value(at);
            assert!(y >= 80.0 && y <= previous);
            previous = y;
        }
        assert_eq!(previous, 80.0);
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
    fn wrapped_synopsis_keeps_natural_height_during_width_morph() {
        set_reduced(false);
        let at = Instant::now();
        let _clock = FixedTime::new(at);
        let renderer = renderer();
        let fields = LocalPositions::new(Positions::default());
        let make = |generation| {
            fields.item(
                "group\0synopsis".into(),
                generation,
                iced::widget::text(
                    "An unfamiliar station, an impossible map, and a door that remembers heat.",
                )
                .size(14)
                .width(Length::Fill),
            )
        };
        let mut field: Element<'_, ()> = make(0);
        let mut tree = Tree::new(&field);
        let compact = layout::Limits::new(Size::ZERO, Size::new(180.0, 200.0));
        let initial = field.as_widget_mut().layout(&mut tree, &renderer, &compact);
        let height = initial.children()[0].size().height;
        assert!(height > 20.0);
        frame(&mut field, &mut tree, &renderer, &initial, at);
        field = make(1);
        tree.diff(&field);
        let expanded = layout::Limits::new(Size::ZERO, Size::new(800.0, 20.0));
        let opening = field
            .as_widget_mut()
            .layout(&mut tree, &renderer, &expanded);
        assert_eq!(
            opening.children()[0].size().height,
            height,
            "destination row height must not clip the still-wrapped synopsis"
        );
    }

    #[test]
    fn shell_morph_preserves_padded_content_layout() {
        let content = layout::Node::new(Size::new(176.0, 80.0)).move_to(Point::new(12.0, 12.0));
        let shell = layout::Node::with_children(Size::new(200.0, 104.0), vec![content]);
        let wrapper = layout::Node::with_children(shell.size(), vec![shell]);
        let animated = resize_shell_width(&wrapper, 200.0, 500.0);
        assert_eq!(animated.size().width, 500.0);
        assert_eq!(animated.children()[0].size().width, 500.0);
        assert_eq!(animated.children()[0].children()[0].size().width, 176.0);
    }

    #[test]
    fn expanding_heading_never_gives_its_synopsis_zero_width() {
        set_reduced(false);
        let start = Instant::now();
        let _clock = FixedTime::new(start);
        let renderer = renderer();
        let positions = Positions::default();
        let make = |expanded| {
            let content: Element<'_, ()> = if expanded {
                iced::widget::row![
                    iced::widget::Space::new().width(Length::Fill).height(60),
                    iced::widget::Space::new().width(520).height(60),
                ]
                .into()
            } else {
                iced::widget::Space::new()
                    .width(Length::Fill)
                    .height(200)
                    .into()
            };
            reflow_card_height_to(
                positions.clone(),
                "heading",
                u64::from(expanded),
                true,
                Some((if expanded { 100.0 } else { 200.0 }, None)),
                |point| point,
                content,
            )
        };
        let mut heading = make(false);
        let mut tree = Tree::new(&heading);
        let compact = layout::Limits::new(Size::ZERO, Size::new(220.0, 900.0));
        let node = heading
            .as_widget_mut()
            .layout(&mut tree, &renderer, &compact);
        frame(&mut heading, &mut tree, &renderer, &node, start);
        heading = make(true);
        tree.diff(&heading);
        let wide = layout::Limits::new(Size::ZERO, Size::new(800.0, 900.0));
        for millis in (0..=224).step_by(16) {
            let at = start + Duration::from_millis(millis);
            FRAME_TIME.set(Some(at));
            let node = heading.as_widget_mut().layout(&mut tree, &renderer, &wide);
            frame(&mut heading, &mut tree, &renderer, &node, at);
            let synopsis = &node.children()[0].children()[0];
            assert!(
                synopsis.size().width >= 220.0,
                "metadata stole synopsis width at {millis}ms: {:?}",
                synopsis.size()
            );
        }
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
            Vector::new(-220.0, 120.0)
        );
        frame(
            &mut element,
            &mut tree,
            &renderer,
            &compact,
            start + LAYOUT / 2,
        );
        let middle = tree.state.downcast_ref::<ReflowState>().offset;
        assert!(middle.x > -220.0 && middle.x < 0.0);
        assert!(middle.y > 0.0 && middle.y < 120.0);
        frame(&mut element, &mut tree, &renderer, &compact, start + LAYOUT);
        assert_eq!(
            tree.state.downcast_ref::<ReflowState>().offset,
            Vector::ZERO
        );
    }

    #[test]
    fn reflow_moves_both_coordinates_on_a_diagonal_path() {
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
        // A column shift belongs to the same travel as the vertical move.
        let node = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits)
            .move_to(Point::new(40.0, 300.0));
        frame(&mut element, &mut tree, &renderer, &node, start + LAYOUT);
        let offset = tree.state.downcast_ref::<ReflowState>().offset;
        assert_ne!(offset, Vector::ZERO);
        assert!(offset.x < 0.0);
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
