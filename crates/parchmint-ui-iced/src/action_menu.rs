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
            row![icon_sized(icon, 16), icon_sized(Icon::ChevronDown, 10)]
                .spacing(4)
                .align_y(iced::alignment::Vertical::Center),
        )
        .center(Length::Fill),
    )
    .width(44)
    .height(32)
    .padding([0, 7])
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
        trigger,
        enabled: !options.is_empty(),
        options: options
            .into_iter()
            .map(|(label, message)| Choice {
                label,
                message,
                icon: None,
                target: None,
            })
            .collect(),
        theme,
        width,
        menu_style: Box::new(move |_| components::menu_style(theme)),
    })
}

struct ActionMenu<'a, Message> {
    trigger: Element<'a, ()>,
    options: Vec<Choice<Message>>,
    enabled: bool,
    theme: ParchMintTheme,
    width: f32,
    menu_style: iced::widget::overlay::menu::StyleFn<'a, iced::Theme>,
}

struct State {
    motion: crate::motion::MenuMotion,
    open: bool,
    selected: Option<usize>,
    menu: Tree,
    text_menu: iced::widget::overlay::menu::State,
}

impl<Message: Clone> Widget<Message, iced::Theme, iced::Renderer> for ActionMenu<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State {
            motion: Default::default(),
            open: false,
            selected: None,
            menu: Tree::empty(),
            text_menu: Default::default(),
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
                shell.capture_event();
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
        if !messages.is_empty() {
            state.open = !state.open;
            if state.open {
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
        if self.options.iter().all(|choice| choice.icon.is_none()) {
            return Some(
                state.motion.overlay(
                    iced::widget::overlay::menu::Menu::new(
                        &mut state.text_menu,
                        &self.options,
                        &mut state.selected,
                        |choice| {
                            state.open = false;
                            choice.message
                        },
                        None,
                        &self.menu_style,
                    )
                    .width(width)
                    .text_size(13)
                    .text_line_height(iced::Pixels(18.0))
                    .padding([7, 10])
                    .overlay(
                        point,
                        *viewport,
                        bounds.height + 4.0,
                        Length::Shrink,
                    ),
                ),
            );
        }
        let theme = self.theme;
        let options =
            self.options
                .iter()
                .enumerate()
                .fold(column![].spacing(2), |items, (index, choice)| {
                    let mut label = row![]
                        .spacing(10)
                        .align_y(iced::alignment::Vertical::Center);
                    if let Some(icon) = choice.icon {
                        label = label.push(icon_sized(icon, 18));
                    }
                    label = label.push(text(choice.label.clone()).size(13));
                    let selected = state.selected == Some(index);
                    let button = components::semantic_button(label)
                        .width(Length::Fill)
                        .padding([7, 10])
                        .on_press(choice.message.clone())
                        .style(move |_, status| {
                            components::button_style(
                                theme,
                                ButtonKind::Quiet,
                                components::button_interaction(status, selected),
                            )
                        });
                    items.push(if let Some(target) = choice.target {
                        crate::harness_target::target(target, button)
                    } else {
                        button.into()
                    })
                });
        let content: Element<'_, Message> = container(options)
            .width(width)
            .padding(4)
            .style(move |_| components::surface(theme, Surface::Elevated, Interaction::Rest))
            .into();
        state.menu.diff(&content);
        Some(
            state
                .motion
                .overlay(overlay::Element::new(Box::new(ChoiceOverlay {
                    content,
                    tree: &mut state.menu,
                    open: &mut state.open,
                    point: Point::new(point.x, point.y + bounds.height + 4.0),
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
        trigger,
        enabled: true,
        theme,
        width: 184.0,
        menu_style: Box::new(move |_| components::menu_style(theme)),
        options: options
            .into_iter()
            .map(|(icon, label, target, message)| Choice {
                label: label.to_owned(),
                message,
                icon: Some(icon),
                target: Some(target),
            })
            .collect(),
    })
}

struct ChoiceOverlay<'a, Message> {
    content: Element<'a, Message>,
    tree: &'a mut Tree,
    open: &'a mut bool,
    point: Point,
}
impl<Message: Clone> overlay::Overlay<Message, iced::Theme, iced::Renderer>
    for ChoiceOverlay<'_, Message>
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
        if !messages.is_empty() {
            *self.open = false;
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
}
