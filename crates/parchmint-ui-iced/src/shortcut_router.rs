//! One resolver for shipped and user-assigned keyboard shortcuts.
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{Operation, Tree},
};
use iced::{Element, Event, Length, Rectangle, Size, Vector, keyboard};
use parchmint_preferences::{
    Keybindings, Shortcut, ShortcutScope, effective_shortcut, shortcut_commands,
};

pub(crate) fn shortcut(key: &keyboard::Key, modifiers: keyboard::Modifiers) -> Shortcut {
    let key = match key {
        keyboard::Key::Character(value) => value.to_uppercase(),
        keyboard::Key::Named(value) => format!("{value:?}"),
        _ => String::new(),
    };
    Shortcut::new(
        key,
        u8::from(modifiers.control())
            | (u8::from(modifiers.alt()) << 1)
            | (u8::from(modifiers.shift()) << 2)
            | (u8::from(modifiers.logo()) << 3),
    )
}

fn commands() -> &'static [parchmint_preferences::ShortcutCommand] {
    static COMMANDS: std::sync::OnceLock<Vec<parchmint_preferences::ShortcutCommand>> =
        std::sync::OnceLock::new();
    COMMANDS.get_or_init(shortcut_commands)
}

pub(crate) fn resolve(
    key: &Shortcut,
    bindings: &Keybindings,
    context: ShortcutScope,
) -> Option<&'static str> {
    commands()
        .iter()
        .find(|command| {
            command.scope.is_active(context)
                && effective_shortcut(command, bindings).as_ref() == Some(key)
        })
        .map(|c| c.id)
}

pub(crate) fn canonical_event(shortcut: &Shortcut) -> Event {
    let key = match shortcut.key.as_str() {
        "F2" => keyboard::Key::Named(keyboard::key::Named::F2),
        "F6" => keyboard::Key::Named(keyboard::key::Named::F6),
        value => keyboard::Key::Character(value.to_lowercase().into()),
    };
    let mut modifiers = keyboard::Modifiers::empty();
    for (bit, flag) in [
        (1, keyboard::Modifiers::CTRL),
        (2, keyboard::Modifiers::ALT),
        (4, keyboard::Modifiers::SHIFT),
        (8, keyboard::Modifiers::LOGO),
    ] {
        if shortcut.modifiers & bit != 0 {
            modifiers |= flag;
        }
    }
    Event::Keyboard(keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
        modifiers,
        text: None,
        repeat: false,
    })
}

pub(crate) fn route<'a, Message: 'a>(
    content: Element<'a, Message>,
    bindings: &'a Keybindings,
    context: ShortcutScope,
    recording: bool,
    command: impl Fn(&'static str) -> Message + 'a,
    capture: impl Fn(Option<Shortcut>) -> Message + 'a,
) -> Element<'a, Message> {
    Element::new(ShortcutRouter {
        content,
        bindings,
        context,
        recording,
        command: Box::new(command),
        capture: Box::new(capture),
    })
}

struct ShortcutRouter<'a, Message, Renderer = iced::Renderer> {
    content: Element<'a, Message, iced::Theme, Renderer>,
    bindings: &'a Keybindings,
    context: ShortcutScope,
    recording: bool,
    command: Box<dyn Fn(&'static str) -> Message + 'a>,
    capture: Box<dyn Fn(Option<Shortcut>) -> Message + 'a>,
}

impl<Message, Renderer: renderer::Renderer> Widget<Message, iced::Theme, Renderer>
    for ShortcutRouter<'_, Message, Renderer>
{
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
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
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
        if shell.is_event_captured() {
            return;
        }
        if let Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            modifiers,
            repeat,
            ..
        }) = event
        {
            if self.recording {
                if matches!(
                    key,
                    keyboard::Key::Named(
                        keyboard::key::Named::Control
                            | keyboard::key::Named::Alt
                            | keyboard::key::Named::Shift
                            | keyboard::key::Named::Super
                            | keyboard::key::Named::Meta
                    )
                ) {
                    shell.capture_event();
                    return;
                }
                if !repeat {
                    shell.publish((self.capture)(
                        if *key == keyboard::Key::Named(keyboard::key::Named::Escape) {
                            None
                        } else {
                            Some(shortcut(key, *modifiers))
                        },
                    ));
                }
                shell.capture_event();
                return;
            }
            // Plain Delete belongs to a focused text editor before the outline.
            // Let the real widget consume it, including multiline editors whose
            // focus is not exposed through the text-input operation.
            if *key == keyboard::Key::Named(keyboard::key::Named::Delete) && modifiers.is_empty() {
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
                if shell.is_event_captured() {
                    return;
                }
            }
            let pressed = shortcut(key, *modifiers);
            if let Some(command) = resolve(&pressed, self.bindings, self.context) {
                if !repeat {
                    if command.starts_with("edit.") {
                        // Every editing command, including a shipped default, uses this
                        // adapter so the actual focused text widget owns its edit history
                        // and clipboard. The application handles an unconsumed command.
                        let canonical = commands()
                            .iter()
                            .find(|c| c.id == command)
                            .and_then(|c| c.default.clone())
                            .unwrap();
                        let event = canonical_event(&canonical);
                        let Event::Keyboard(keyboard::Event::KeyPressed {
                            modifiers: canonical_modifiers,
                            ..
                        }) = &event
                        else {
                            unreachable!()
                        };
                        let canonical_modifiers = *canonical_modifiers;
                        let mut handled = false;
                        for (index, event) in [
                            Event::Keyboard(keyboard::Event::ModifiersChanged(canonical_modifiers)),
                            event,
                            Event::Keyboard(keyboard::Event::ModifiersChanged(*modifiers)),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            let mut messages = Vec::new();
                            let mut child = Shell::new(&mut messages);
                            self.content.as_widget_mut().update(
                                &mut tree.children[0],
                                &event,
                                layout,
                                cursor,
                                renderer,
                                clipboard,
                                &mut child,
                                viewport,
                            );
                            if index == 1 {
                                handled = child.is_event_captured();
                            }
                            shell.merge(child, |message| message);
                        }
                        let mut focus = FocusedTextInput::default();
                        self.content.as_widget_mut().operate(
                            &mut tree.children[0],
                            layout,
                            renderer,
                            &mut focus,
                        );
                        if !handled && !focus.focused {
                            shell.publish((self.command)(command));
                        }
                    } else {
                        let mut focus = FocusedTextInput::default();
                        self.content.as_widget_mut().operate(
                            &mut tree.children[0],
                            layout,
                            renderer,
                            &mut focus,
                        );
                        if !command.starts_with("format.") || !focus.focused {
                            shell.publish((self.command)(command));
                        }
                    }
                }
                shell.capture_event();
                return;
            }
            // Prevent a disabled/reassigned shipped binding reaching a text widget's
            // built-in accelerator. This applies to the complete effective keymap.
            if commands()
                .iter()
                .any(|c| c.default.as_ref() == Some(&pressed))
            {
                shell.capture_event();
                return;
            }
        }
        let select_on_focus = if matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        ) {
            let mut focus = FocusedTextInput::default();
            self.content.as_widget_mut().operate(
                &mut tree.children[0],
                layout,
                renderer,
                &mut focus,
            );
            cursor.position().and_then(|point| {
                focus
                    .inputs
                    .iter()
                    .find(|bounds| bounds.contains(point) && focus.focused_bounds != Some(**bounds))
                    .copied()
            })
        } else {
            None
        };
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
        if let Some(bounds) = select_on_focus {
            self.content.as_widget_mut().operate(
                &mut tree.children[0],
                layout,
                renderer,
                &mut SelectInputAt(bounds),
            );
        }
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
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

// Labels are presentation data scoped to the UI thread, like motion preferences.
// A frame installs its effective keymap before constructing menus and tooltips.
thread_local! { static LABELS: std::cell::RefCell<Keybindings> = const { std::cell::RefCell::new(Keybindings::new()) }; }
pub(crate) fn set_labels(bindings: &Keybindings) {
    LABELS.with(|labels| labels.replace(bindings.clone()));
}
pub(crate) fn label(id: &str) -> String {
    LABELS.with(|labels| {
        commands()
            .iter()
            .find(|c| c.id == id)
            .and_then(|c| effective_shortcut(c, &labels.borrow()))
            .map(|s| s.to_string())
            .unwrap_or_default()
    })
}
pub(crate) fn formatting_command(id: &str) -> Option<crate::FormattingCommand> {
    use crate::FormattingCommand as F;
    use parchmint_domain::TextAlignment as A;
    use parchmint_editor_api::ParagraphFormatCommand as P;
    Some(match id {
        "format.bold" => F::Bold,
        "format.italic" => F::Italic,
        "format.underline" => F::Underline,
        "format.strikethrough" => F::Strikethrough,
        "format.link" => F::Link,
        "format.numbered" => F::NumberedList,
        "format.bulleted" => F::BulletedList,
        "format.quote" => F::BlockQuote,
        "format.page-break" => F::PageBreak,
        "format.scene-break" => F::SceneBreak,
        "format.align-left" => F::ParagraphFormat(P::Alignment(A::Start)),
        "format.align-center" => F::ParagraphFormat(P::Alignment(A::Center)),
        "format.align-right" => F::ParagraphFormat(P::Alignment(A::End)),
        "format.align-justify" => F::ParagraphFormat(P::Alignment(A::Justify)),
        "format.spacing-single" => F::ParagraphFormat(P::LineSpacing(100)),
        "format.spacing-double" => F::ParagraphFormat(P::LineSpacing(200)),
        "format.body" => F::ParagraphStyle("Body".into()),
        "format.heading-1" => F::ParagraphStyle("Heading 1".into()),
        "format.heading-2" => F::ParagraphStyle("Heading 2".into()),
        "format.heading-3" => F::ParagraphStyle("Heading 3".into()),
        _ => return None,
    })
}

#[derive(Default)]
struct FocusedTextInput {
    inputs: Vec<Rectangle>,
    focused: bool,
    focused_bounds: Option<Rectangle>,
}
impl Operation for FocusedTextInput {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn text_input(
        &mut self,
        _id: Option<&iced::widget::Id>,
        bounds: Rectangle,
        _state: &mut dyn iced::advanced::widget::operation::TextInput,
    ) {
        self.inputs.push(bounds);
    }
    fn focusable(
        &mut self,
        _id: Option<&iced::widget::Id>,
        bounds: Rectangle,
        state: &mut dyn iced::advanced::widget::operation::Focusable,
    ) {
        if state.is_focused() && self.inputs.contains(&bounds) {
            self.focused = true;
            self.focused_bounds = Some(bounds);
        }
    }
}

struct SelectInputAt(Rectangle);
impl Operation for SelectInputAt {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn text_input(
        &mut self,
        _id: Option<&iced::widget::Id>,
        bounds: Rectangle,
        state: &mut dyn iced::advanced::widget::operation::TextInput,
    ) {
        if bounds == self.0 {
            state.select_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Settings;
    use iced_test::Simulator;
    #[derive(Debug, Clone, PartialEq)]
    enum Message {
        Command(&'static str),
        Capture(Option<Shortcut>),
        Text(String),
    }

    #[test]
    fn editor_and_overview_commands_share_contextual_resolution_for_defaults_and_overrides() {
        let primary = if cfg!(target_os = "macos") { 8 } else { 1 };
        let key = Shortcut::new("Enter", primary);
        let mut bindings = Keybindings::new();
        assert_eq!(
            resolve(&key, &bindings, ShortcutScope::Editor),
            Some("format.page-break")
        );
        assert_eq!(
            resolve(&key, &bindings, ShortcutScope::Overview),
            Some("outline.next-document")
        );
        let custom = Shortcut::new("D", primary | 4);
        bindings.insert("outline.next-document".into(), Some(custom.clone()));
        assert!(parchmint_preferences::validate_keybindings(&bindings).is_ok());
        assert_eq!(resolve(&key, &bindings, ShortcutScope::Overview), None);
        assert_eq!(
            resolve(&key, &bindings, ShortcutScope::Editor),
            Some("format.page-break")
        );
        assert_eq!(
            resolve(&custom, &bindings, ShortcutScope::Overview),
            Some("outline.next-document")
        );
        assert_eq!(resolve(&custom, &bindings, ShortcutScope::Editor), None);
    }

    #[test]
    fn defaults_reassigned_and_disabled_bindings_use_one_resolver_and_dispatcher() {
        let defaults = Keybindings::new();
        let default = shortcut_commands()
            .into_iter()
            .find(|c| c.id == "file.save")
            .unwrap()
            .default
            .unwrap();
        let custom = Shortcut::new("S", default.modifiers | 2);
        for bindings in [
            defaults,
            Keybindings::from([("file.save".into(), Some(custom.clone()))]),
            Keybindings::from([("file.save".into(), None)]),
        ] {
            let mut simulator = Simulator::new(route(
                iced::widget::text("Document").into(),
                &bindings,
                ShortcutScope::Editor,
                false,
                Message::Command,
                Message::Capture,
            ));
            simulator.simulate([canonical_event(&default), canonical_event(&custom)]);
            let messages = simulator.into_messages().collect::<Vec<_>>();
            assert_eq!(
                messages,
                if bindings.get("file.save") == Some(&None) {
                    vec![]
                } else {
                    vec![Message::Command("file.save")]
                }
            );
        }
    }

    #[test]
    fn clicking_an_unfocused_input_selects_its_existing_text() {
        let bindings = Keybindings::new();
        let mut simulator = Simulator::with_size(
            Settings::default(),
            iced::Size::new(400.0, 80.0),
            route(
                iced::widget::text_input("Name", "original")
                    .id("name")
                    .on_input(Message::Text)
                    .into(),
                &bindings,
                ShortcutScope::Editor,
                false,
                Message::Command,
                Message::Capture,
            ),
        );
        simulator.click(iced::widget::Id::new("name")).unwrap();
        simulator.simulate([iced_test::simulator::press_key(
            keyboard::Key::Character("x".into()),
            Some("x".into()),
        )]);
        assert_eq!(
            simulator.into_messages().collect::<Vec<_>>(),
            vec![Message::Text("x".into())]
        );
    }

    #[test]
    fn plain_delete_edits_focused_text_instead_of_deleting_outline_selection() {
        let bindings = Keybindings::default();
        let mut simulator = Simulator::with_size(
            Settings::default(),
            iced::Size::new(400.0, 80.0),
            route(
                iced::widget::text_input("Name", "original")
                    .id("name")
                    .on_input(Message::Text)
                    .into(),
                &bindings,
                ShortcutScope::Overview,
                false,
                Message::Command,
                Message::Capture,
            ),
        );
        simulator.click(iced::widget::Id::new("name")).unwrap();
        simulator.simulate([iced_test::simulator::press_key(
            keyboard::Key::Named(keyboard::key::Named::Delete),
            None,
        )]);
        assert_eq!(
            simulator.into_messages().collect::<Vec<_>>(),
            [Message::Text(String::new())]
        );
        assert_eq!(
            resolve(
                &Shortcut::new("Delete", 0),
                &bindings,
                ShortcutScope::Overview
            ),
            Some("outline.delete")
        );
    }

    #[test]
    fn default_and_custom_select_all_use_the_focused_text_widget() {
        let default = shortcut_commands()
            .into_iter()
            .find(|c| c.id == "edit.select-all")
            .unwrap()
            .default
            .unwrap();
        for binding in [default.clone(), Shortcut::new("A", default.modifiers | 2)] {
            let bindings = Keybindings::from([("edit.select-all".into(), Some(binding.clone()))]);
            let input = iced::widget::text_input("Search", "original")
                .id("test-input")
                .on_input(Message::Text);
            let mut simulator = Simulator::with_size(
                Settings::default(),
                iced::Size::new(400.0, 80.0),
                route(
                    input.into(),
                    &bindings,
                    ShortcutScope::Editor,
                    false,
                    Message::Command,
                    Message::Capture,
                ),
            );
            simulator
                .click(iced::widget::Id::new("test-input"))
                .unwrap();
            simulator.simulate([
                canonical_event(&binding),
                Event::Keyboard(keyboard::Event::ModifiersChanged(
                    keyboard::Modifiers::empty(),
                )),
                iced_test::simulator::press_key(
                    keyboard::Key::Character("x".into()),
                    Some("x".into()),
                ),
            ]);
            assert_eq!(
                simulator.into_messages().collect::<Vec<_>>(),
                vec![Message::Text("x".into())]
            );
        }
    }

    #[test]
    fn recording_never_executes_a_command_or_types_into_the_underlying_field() {
        let bindings = Keybindings::new();
        let save = shortcut_commands()
            .into_iter()
            .find(|c| c.id == "file.save")
            .unwrap()
            .default
            .unwrap();
        let mut simulator = Simulator::new(route(
            iced::widget::text("Recorder").into(),
            &bindings,
            ShortcutScope::Editor,
            true,
            Message::Command,
            Message::Capture,
        ));
        simulator.simulate([canonical_event(&save)]);
        assert_eq!(
            simulator.into_messages().collect::<Vec<_>>(),
            vec![Message::Capture(Some(save))]
        );
    }
}
