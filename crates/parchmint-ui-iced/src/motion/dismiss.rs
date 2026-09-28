//! Retain an overlay through its exit, then deliver the original action once.
use super::*;

pub(crate) fn dismissible<'a, M: Clone + 'static>(
    content: impl Into<Element<'a, M>>,
    exits: impl Fn(&M) -> bool + 'a,
    escape: Option<M>,
) -> Element<'a, M> {
    Element::new(Dismissible {
        content: content.into(),
        exits: Box::new(exits),
        escape,
    })
}
struct Dismissible<'a, M> {
    content: Element<'a, M>,
    exits: Box<dyn Fn(&M) -> bool + 'a>,
    escape: Option<M>,
}
struct State<M> {
    progress: Tween,
    pending: Option<M>,
    now: Instant,
}
impl<M: Clone + 'static> Widget<M, iced::Theme, iced::Renderer> for Dismissible<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State<M>>()
    }
    fn state(&self) -> tree::State {
        let at = now();
        tree::State::new(State::<M> {
            progress: Tween::new(1.0, at, ENTRANCE),
            pending: None,
            now: at,
        })
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut Tree) {
        let state = tree.state.downcast_mut::<State<M>>();
        // A rejected action may leave the same dialog open with validation.
        if state.pending.is_none() && state.progress.target == 0.0 {
            state.progress.set(1.0, now(), false);
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
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State<M>>();
        if state.pending.is_some() {
            if let Event::Window(iced::window::Event::RedrawRequested(at)) = event {
                state.now = *at;
                if enabled() && state.progress.active(*at) {
                    shell.request_redraw();
                } else if let Some(message) = state.pending.take() {
                    shell.publish(message);
                }
            } else if matches!(event, Event::Keyboard(_) | Event::Mouse(_)) {
                shell.capture_event();
            }
            return;
        }
        let mut messages = Vec::new();
        if let Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
            ..
        }) = event
            && let Some(message) = &self.escape
        {
            messages.push(message.clone());
            shell.capture_event();
        } else {
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
            shell.request_redraw_at(child_shell.redraw_request());
            if child_shell.is_event_captured() {
                shell.capture_event();
            }
            if child_shell.is_layout_invalid() {
                shell.invalidate_layout();
            }
            if child_shell.are_widgets_invalid() {
                shell.invalidate_widgets();
            }
            shell.input_method_mut().merge(child_shell.input_method());
        }
        for message in messages {
            if enabled() && (self.exits)(&message) {
                if state.pending.is_none() {
                    state.now = now();
                    state.progress.set(0.0, state.now, true);
                    state.pending = Some(message);
                    shell.request_redraw();
                }
            } else {
                shell.publish(message);
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
        let state = tree.state.downcast_ref::<State<M>>();
        let progress = if enabled() {
            state.progress.value(state.now)
        } else {
            1.0
        };
        let bounds = layout.bounds();
        let clip = Rectangle {
            height: bounds.height * progress,
            ..bounds
        };
        if let Some(clip) = viewport
            .intersection(&clip)
            .filter(|clip| clip.height > 0.0)
        {
            renderer.with_layer(clip, |renderer| {
                renderer.with_translation(Vector::new(0.0, -6.0 * (1.0 - progress)), |renderer| {
                    self.content.as_widget().draw(
                        &tree.children[0],
                        renderer,
                        theme,
                        style,
                        layout,
                        if state.pending.is_some() {
                            mouse::Cursor::Unavailable
                        } else {
                            cursor
                        },
                        viewport,
                    );
                });
            });
        }
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        if tree.state.downcast_ref::<State<M>>().pending.is_none() {
            self.content.as_widget_mut().operate(
                &mut tree.children[0],
                layout,
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
        if tree.state.downcast_ref::<State<M>>().pending.is_some() {
            mouse::Interaction::None
        } else {
            self.content.as_widget().mouse_interaction(
                &tree.children[0],
                layout,
                cursor,
                viewport,
                renderer,
            )
        }
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, M, iced::Theme, iced::Renderer>> {
        if tree.state.downcast_ref::<State<M>>().pending.is_some() {
            None
        } else {
            self.content.as_widget_mut().overlay(
                &mut tree.children[0],
                layout,
                renderer,
                viewport,
                translation,
            )
        }
    }
}
