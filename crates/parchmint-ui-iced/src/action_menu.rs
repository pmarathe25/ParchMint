//! Compact toolbar buttons with anchored action menus.

use crate::{
    components::{self, ButtonKind, Interaction, Surface},
    design_tokens::ParchMintTheme,
    icons::{Icon, icon_sized},
};
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{Operation, Tree, tree},
};
use iced::widget::{column, container, row, text};
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector, keyboard};

pub(crate) fn action_menu<'a, Message: Clone + 'a>(
    icon: Icon,
    options: Vec<(&'static str, Message)>,
    enabled: bool,
    theme: ParchMintTheme,
) -> Element<'a, Message> {
    let trigger = components::semantic_button(
        container(
            row![icon_sized(icon, 20), icon_sized(Icon::ChevronDown, 10)]
                .spacing(2)
                .align_y(iced::alignment::Vertical::Center),
        )
        .center(Length::Fill),
    )
    .width(36)
    .height(32)
    .padding([0, 4])
    .on_press_maybe(enabled.then_some(()))
    .style(move |_, status| {
        components::button_style(
            theme,
            ButtonKind::Quiet,
            components::button_interaction(status, false),
        )
    });
    anchored_menu(
        trigger.into(),
        options
            .into_iter()
            .map(|(label, message)| (label.to_owned(), message))
            .collect(),
        theme,
        176.0,
    )
}

pub(crate) fn anchored_menu<'a, Message: Clone + 'a>(
    trigger: Element<'a, ()>,
    options: Vec<(String, Message)>,
    theme: ParchMintTheme,
    width: f32,
) -> Element<'a, Message> {
    Element::new(ActionMenu {
        horizontal: false,
        menu_content: None,
        on_toggle: None,
        trigger,
        panel: None,
        enabled: !options.is_empty(),
        options: options
            .into_iter()
            .map(|(label, message)| Choice {
                label,
                message,
                icon: None,
                target: None,
                divider_before: false,
            })
            .collect(),
        theme,
        width,
    })
}

pub(crate) fn menu_with_footer<'a, Message: Clone + 'a>(
    trigger: Element<'a, ()>,
    options: Vec<(String, Message)>,
    footer: (String, Message, crate::HarnessTarget),
    theme: ParchMintTheme,
    width: f32,
) -> Element<'a, Message> {
    let mut options: Vec<_> = options
        .into_iter()
        .map(|(label, message)| Choice {
            label,
            message,
            icon: None,
            target: None,
            divider_before: false,
        })
        .collect();
    options.push(Choice {
        label: footer.0,
        message: footer.1,
        icon: None,
        target: Some(footer.2),
        divider_before: true,
    });
    Element::new(ActionMenu {
        horizontal: false,
        menu_content: None,
        on_toggle: None,
        trigger,
        panel: None,
        options,
        enabled: true,
        theme,
        width,
    })
}

struct ActionMenu<'a, Message> {
    horizontal: bool,
    on_toggle: Option<fn(bool) -> Message>,
    menu_content: Option<(Option<usize>, Element<'a, Message>)>,
    trigger: Element<'a, ()>,
    panel: Option<Element<'a, Message>>,
    options: Vec<Choice<Message>>,
    enabled: bool,
    theme: ParchMintTheme,
    width: f32,
}

struct State {
    motion: crate::motion::MenuMotion,
    open: bool,
    selected: Option<usize>,
    menu: Tree,
}

impl<'a, Message: Clone + 'a> Widget<Message, iced::Theme, iced::Renderer>
    for ActionMenu<'a, Message>
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State {
            motion: Default::default(),
            open: false,
            selected: None,
            menu: Tree::empty(),
        })
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.trigger)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.trigger));
        tree.state.downcast_mut::<State>().selected = tree
            .state
            .downcast_ref::<State>()
            .selected
            .filter(|i| *i < self.options.len());
        if !self.enabled {
            tree.state.downcast_mut::<State>().open = false;
        }
    }
    fn size(&self) -> Size<Length> {
        self.trigger.as_widget().size()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.trigger
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
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        if state.open {
            if let Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(key),
                ..
            }) = event
            {
                if self.panel.is_some()
                    && !matches!(
                        key,
                        keyboard::key::Named::Escape | keyboard::key::Named::Tab
                    )
                {
                    return;
                }
                match key {
                    keyboard::key::Named::Escape | keyboard::key::Named::Tab => state.open = false,
                    keyboard::key::Named::ArrowDown => {
                        state.selected =
                            Some(state.selected.map_or(0, |i| (i + 1) % self.options.len()))
                    }
                    keyboard::key::Named::ArrowUp => {
                        state.selected = Some(state.selected.map_or(self.options.len() - 1, |i| {
                            (i + self.options.len() - 1) % self.options.len()
                        }))
                    }
                    keyboard::key::Named::Enter | keyboard::key::Named::Space => {
                        if let Some(i) = state.selected {
                            shell.publish(self.options[i].message.clone());
                        }
                        state.open = false;
                    }
                    _ => return,
                }
                if !state.open
                    && let Some(on_toggle) = self.on_toggle
                {
                    shell.publish(on_toggle(false));
                }
                shell.capture_event();
                shell.request_redraw();
                return;
            }
            if matches!(
                event,
                Event::Mouse(mouse::Event::ButtonPressed(
                    mouse::Button::Left | mouse::Button::Right
                ))
            ) {
                state.open = false;
                if let Some(on_toggle) = self.on_toggle {
                    shell.publish(on_toggle(false));
                }
                if self.panel.is_none() {
                    shell.capture_event();
                }
                shell.request_redraw();
                return;
            }
        }
        let mut messages = Vec::new();
        let mut local = Shell::new(&mut messages);
        self.trigger.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            &mut local,
            viewport,
        );
        if local.is_event_captured() {
            shell.capture_event();
        }
        shell.request_redraw_at(local.redraw_request());
        if local.is_layout_invalid() {
            shell.invalidate_layout();
        }
        if local.are_widgets_invalid() {
            shell.invalidate_widgets();
        }
        if !messages.is_empty() {
            state.open = !state.open;
            if let Some(on_toggle) = self.on_toggle {
                shell.publish(on_toggle(state.open));
            }
            if state.open {
                if self.panel.is_some() {
                    state.menu = Tree::empty();
                }
                state.motion.start();
            }
            state.selected = None;
            shell.request_redraw();
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
        if tree.state.downcast_ref::<State>().open {
            container::draw_background(
                renderer,
                &components::surface(self.theme, Surface::Panel, Interaction::Selected),
                layout.bounds(),
            );
        }
        self.trigger.as_widget().draw(
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
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.trigger.as_widget().mouse_interaction(
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
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.trigger
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }
    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        _renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        let state = tree.state.downcast_mut::<State>();
        if !state.open {
            return None;
        }
        let bounds = layout.bounds();
        let width = self.width.min(viewport.width);
        let point = Point::new(
            (bounds.x + translation.x).clamp(
                viewport.x,
                (viewport.x + viewport.width - width).max(viewport.x),
            ),
            bounds.y + translation.y,
        );
        if let Some(content) = self.panel.as_mut() {
            state.menu.diff(&*content);
            return Some(
                state
                    .motion
                    .overlay(overlay::Element::new(Box::new(PanelOverlay {
                        content,
                        tree: &mut state.menu,
                        point: Point::new(point.x, point.y + bounds.height + 4.0),
                        viewport: *viewport,
                    }))),
            );
        }
        if self
            .menu_content
            .as_ref()
            .is_none_or(|(selected, _)| *selected != state.selected)
        {
            let theme = self.theme;
            let mut choices = Vec::new();
            for (index, choice) in self.options.iter().enumerate() {
                if choice.divider_before {
                    choices.push(
                        container(iced::widget::rule::horizontal(1))
                            .padding([4, 6])
                            .into(),
                    );
                }
                let mut label = row![]
                    .spacing(10)
                    .align_y(iced::alignment::Vertical::Center);
                if let Some(icon) = choice.icon {
                    label = label.push(icon_sized(icon, 18));
                }
                if !self.horizontal {
                    label = label.push(
                        text(choice.label.clone())
                            .size(13)
                            .color(theme.palette().primary_text),
                    );
                }
                let selected = state.selected == Some(index);
                let button = components::semantic_button(label)
                    .width(if self.horizontal {
                        Length::Fixed(32.0)
                    } else {
                        Length::Fill
                    })
                    .height(if self.horizontal {
                        Length::Fixed(32.0)
                    } else {
                        Length::Shrink
                    })
                    .padding(if self.horizontal { [6, 6] } else { [5, 8] })
                    .on_press(choice.message.clone())
                    .style(move |_, status| {
                        let mut style = components::button_style(
                            theme,
                            ButtonKind::Quiet,
                            components::button_interaction(status, selected),
                        );
                        if matches!(status, iced::widget::button::Status::Hovered) {
                            style.background = Some(theme.palette().accent_subtle.into());
                            style.text_color = theme.palette().primary_text;
                        }
                        style
                    });
                let item = if let Some(target) = choice.target {
                    crate::harness_target::target(target, button)
                } else {
                    button.into()
                };
                choices.push(if self.horizontal {
                    crate::stationary_tooltip::tooltip(
                        item,
                        text(choice.label.clone()).size(12),
                        components::surface(theme, Surface::Elevated, Interaction::Rest),
                    )
                } else {
                    item
                });
            }
            let options: Element<'_, Message> = if self.horizontal {
                row(choices).spacing(2).into()
            } else {
                column(choices).spacing(2).into()
            };

            let content: Element<'_, Message> =
                container(iced::widget::scrollable(options).height(Length::Shrink))
                    .max_height(440)
                    .width(width)
                    .padding(4)
                    .style(move |_| {
                        components::surface(theme, Surface::Elevated, Interaction::Rest)
                    })
                    .into();
            self.menu_content = Some((state.selected, content));
        }
        let (_, content) = self.menu_content.as_mut().expect("open menu has content");
        state.menu.diff(&*content);
        Some(
            state
                .motion
                .overlay(overlay::Element::new(Box::new(ChoiceOverlay {
                    content,
                    tree: &mut state.menu,
                    open: &mut state.open,
                    on_toggle: self.on_toggle,
                    point: Point::new(point.x, point.y + bounds.height + 4.0),
                    viewport: *viewport,
                }))),
        )
    }
}

#[derive(Clone)]
struct Choice<Message> {
    label: String,
    message: Message,
    icon: Option<Icon>,
    target: Option<crate::HarnessTarget>,
    divider_before: bool,
}
impl<Message> std::fmt::Display for Choice<Message> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

pub(crate) fn icon_menu<'a, Message: Clone + 'a>(
    trigger: Element<'a, ()>,
    options: Vec<(Icon, &'static str, crate::HarnessTarget, Message)>,
    theme: ParchMintTheme,
) -> Element<'a, Message> {
    Element::new(ActionMenu {
        horizontal: true,
        menu_content: None,
        on_toggle: None,
        trigger,
        panel: None,
        enabled: true,
        theme,
        width: (options.len() as f32 * 34.0 + 8.0),
        options: options
            .into_iter()
            .map(|(icon, label, target, message)| Choice {
                label: label.to_owned(),
                message,
                icon: Some(icon),
                target: Some(target),
                divider_before: false,
            })
            .collect(),
    })
}

pub(crate) fn symbol_menu<'a, Message: Clone + 'a>(
    icon: Icon,
    options: Vec<(Icon, &'static str, Message)>,
    theme: ParchMintTheme,
) -> Element<'a, Message> {
    Element::new(ActionMenu {
        horizontal: true,
        menu_content: None,
        on_toggle: None,
        trigger: components::semantic_button(
            row![icon_sized(icon, 20), icon_sized(Icon::ChevronDown, 10)]
                .spacing(2)
                .align_y(iced::alignment::Vertical::Center),
        )
        .padding([4, 3])
        .height(32)
        .on_press(())
        .style(move |_, status| {
            components::button_style(
                theme,
                ButtonKind::Quiet,
                components::button_interaction(status, false),
            )
        })
        .into(),
        panel: None,
        enabled: true,
        theme,
        width: (options.len() as f32 * 34.0 + 8.0),
        options: options
            .into_iter()
            .map(|(icon, label, message)| Choice {
                icon: Some(icon),
                label: label.into(),
                message,
                target: None,
                divider_before: false,
            })
            .collect(),
    })
}

struct ChoiceOverlay<'a, 'b, Message> {
    content: &'a mut Element<'b, Message>,
    tree: &'a mut Tree,
    open: &'a mut bool,
    on_toggle: Option<fn(bool) -> Message>,
    point: Point,
    viewport: Rectangle,
}
impl<Message: Clone> overlay::Overlay<Message, iced::Theme, iced::Renderer>
    for ChoiceOverlay<'_, '_, Message>
{
    fn layout(&mut self, renderer: &iced::Renderer, bounds: Size) -> layout::Node {
        let node = self.content.as_widget_mut().layout(
            self.tree,
            renderer,
            &layout::Limits::new(Size::ZERO, bounds),
        );
        let point = Point::new(
            self.point
                .x
                .min((bounds.width - node.size().width).max(0.0)),
            self.point
                .y
                .min((bounds.height - node.size().height).max(0.0)),
        );
        node.move_to(point)
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
        let mut messages = Vec::new();
        let mut local = Shell::new(&mut messages);
        self.content.as_widget_mut().update(
            self.tree,
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            &mut local,
            &layout.bounds(),
        );
        if local.is_event_captured() {
            shell.capture_event();
        }
        shell.request_redraw_at(local.redraw_request());
        if local.is_layout_invalid() {
            shell.invalidate_layout();
        }
        if local.are_widgets_invalid() {
            shell.invalidate_widgets();
        }
        if !messages.is_empty() {
            *self.open = false;
            if let Some(on_toggle) = self.on_toggle {
                shell.publish(on_toggle(false));
            }
            shell.request_redraw();
            for message in messages {
                shell.publish(message);
            }
        }
    }
    fn draw(
        &self,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.content.as_widget().draw(
            self.tree,
            renderer,
            theme,
            style,
            layout,
            cursor,
            &layout.bounds(),
        );
    }
    fn operate(
        &mut self,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(self.tree, layout, renderer, operation);
    }
    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            self.tree,
            layout,
            cursor,
            &layout.bounds(),
            renderer,
        )
    }
    fn overlay<'c>(
        &'c mut self,
        layout: Layout<'c>,
        renderer: &iced::Renderer,
    ) -> Option<overlay::Element<'c, Message, iced::Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            self.tree,
            layout,
            renderer,
            &self.viewport,
            Vector::ZERO,
        )
    }
}

/// An anchored formatting panel. Child menus retain their own keyboard handling.
pub(crate) fn panel<'a, Message: Clone + 'a>(
    trigger: Element<'a, ()>,
    content: Element<'a, Message>,
    theme: ParchMintTheme,
    width: f32,
) -> Element<'a, Message> {
    Element::new(ActionMenu {
        horizontal: false,
        menu_content: None,
        on_toggle: None,
        trigger,
        panel: Some(
            container(content)
                .padding(12)
                .width(width)
                .style(move |_| components::surface(theme, Surface::Elevated, Interaction::Rest))
                .into(),
        ),
        options: Vec::new(),
        enabled: true,
        theme,
        width,
    })
}

struct PanelOverlay<'a, 'b, Message> {
    viewport: Rectangle,
    content: &'a mut Element<'b, Message>,
    tree: &'a mut Tree,
    point: Point,
}
impl<Message> overlay::Overlay<Message, iced::Theme, iced::Renderer>
    for PanelOverlay<'_, '_, Message>
{
    fn layout(&mut self, renderer: &iced::Renderer, bounds: Size) -> layout::Node {
        let node = self.content.as_widget_mut().layout(
            self.tree,
            renderer,
            &layout::Limits::new(Size::ZERO, bounds),
        );
        let point = Point::new(
            self.point
                .x
                .min((bounds.width - node.size().width).max(0.0)),
            self.point
                .y
                .min((bounds.height - node.size().height).max(0.0)),
        );
        node.move_to(point)
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
        self.content.as_widget_mut().update(
            self.tree,
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            &layout.bounds(),
        );
    }
    fn draw(
        &self,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.content.as_widget().draw(
            self.tree,
            renderer,
            theme,
            style,
            layout,
            cursor,
            &layout.bounds(),
        );
    }
    fn operate(
        &mut self,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(self.tree, layout, renderer, operation);
    }
    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            self.tree,
            layout,
            cursor,
            &layout.bounds(),
            renderer,
        )
    }
    fn overlay<'c>(
        &'c mut self,
        layout: Layout<'c>,
        renderer: &iced::Renderer,
    ) -> Option<overlay::Element<'c, Message, iced::Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            self.tree,
            layout,
            renderer,
            &self.viewport,
            Vector::ZERO,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::advanced::{overlay::Overlay, renderer::Headless};
    use std::time::{Duration, Instant};

    #[test]
    fn menu_tooltip_can_invalidate_layout_and_open_its_nested_overlay() {
        let renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            iced::Font::DEFAULT,
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .unwrap();
        let mut content: Element<'_, ()> = crate::stationary_tooltip::tooltip(
            iced::widget::button(text("List")).on_press(()),
            text("Numbered list"),
            iced::widget::container::Style::default(),
        );
        let mut tree = Tree::new(&content);
        let mut open = true;
        let mut overlay = ChoiceOverlay {
            content: &mut content,
            tree: &mut tree,
            open: &mut open,
            on_toggle: None,
            point: Point::new(10.0, 10.0),
            viewport: Rectangle::with_size(Size::new(800.0, 600.0)),
        };
        let node = overlay.layout(&renderer, Size::new(800.0, 600.0));
        let now = Instant::now();
        for (offset, should_open) in [(0, false), (700, true)] {
            let mut messages = Vec::new();
            let mut shell = Shell::new(&mut messages);
            overlay.update(
                &Event::Window(iced::window::Event::RedrawRequested(
                    now + Duration::from_millis(offset),
                )),
                Layout::new(&node),
                mouse::Cursor::Available(Point::new(15.0, 15.0)),
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut shell,
            );
            assert_eq!(shell.is_layout_invalid(), should_open);
            assert_eq!(
                overlay.overlay(Layout::new(&node), &renderer).is_some(),
                should_open
            );
        }
    }
}
