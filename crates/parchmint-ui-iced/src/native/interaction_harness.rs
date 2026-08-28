//! Feature-gated, headless interaction driver for the native desktop surface.

use std::{
    collections::{BTreeMap, VecDeque},
    fmt,
    path::Path,
    time::{Duration, Instant},
};

use iced::{
    Element, Event, Font, Point as IcedPoint, Rectangle, Settings, Size,
    advanced::{clipboard, widget::Operation},
    event,
    futures::StreamExt,
    keyboard, mouse, window,
};
use iced_test::{
    Selector, Simulator,
    core::SmolStr,
    renderer::Renderer,
    runtime::{self, UserInterface, user_interface},
    selector::{Bounded, Candidate, Target},
};

use super::*;
use crate::{EditorMessage, HarnessTarget, HierarchyRowKind, harness_target};

const LAUNCHER_SIZE: Size = Size::new(900.0, 620.0);
const PROJECT_SIZE: Size = Size::new(1280.0, 720.0);

/// A logical window address understood by the interaction harness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarnessWindow {
    Launcher,
    Project,
}

impl fmt::Display for HarnessWindow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Launcher => formatter.write_str("launcher"),
            Self::Project => formatter.write_str("project"),
        }
    }
}

/// One replayable user-level action recorded by the harness.
#[derive(Debug, Clone, PartialEq)]
pub struct HarnessTraceEntry {
    pub sequence: u64,
    pub window: HarnessWindow,
    pub action: String,
}

/// A non-text keyboard key supported by the interaction harness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarnessKey {
    Enter,
    Escape,
    Tab,
    F6,
    F2,
    ArrowDown,
    ArrowUp,
}

/// The semantic region of a destination that receives a hierarchy drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarnessDropPosition {
    Before,
    Into,
    After,
}

/// An opaque, serialized hierarchy node address obtained from the live project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessNode(String);

impl HarnessNode {
    fn id(&self) -> &str {
        &self.0
    }
}

/// The hierarchy projection receiving a drag interaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarnessHierarchySurface {
    Explorer,
    Cards,
}

impl HarnessKey {
    fn into_iced(self) -> keyboard::Key {
        keyboard::Key::Named(match self {
            Self::Enter => keyboard::key::Named::Enter,
            Self::Escape => keyboard::key::Named::Escape,
            Self::Tab => keyboard::key::Named::Tab,
            Self::F6 => keyboard::key::Named::F6,
            Self::F2 => keyboard::key::Named::F2,
            Self::ArrowDown => keyboard::key::Named::ArrowDown,
            Self::ArrowUp => keyboard::key::Named::ArrowUp,
        })
    }
}

/// A deterministic failure reported by the interaction harness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessError {
    message: String,
}

impl HarnessError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for HarnessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for HarnessError {}

/// The retained Iced state for one headless window.
///
/// Rebuilding the element from the current desktop state while carrying this
/// cache forward preserves focus, overlays, pointer capture, and custom widget
/// state exactly as the native Iced runtime does.
struct PersistentSurface {
    size: Size,
    renderer: Renderer,
    cache: user_interface::Cache,
    cursor: mouse::Cursor,
}

impl PersistentSurface {
    fn new(size: Size) -> Self {
        let settings = Settings::default();
        let default_font = match settings.default_font {
            Font::DEFAULT => Font::with_name("Fira Sans"),
            font => font,
        };
        let renderer = Renderer::new(default_font, settings.default_text_size);
        Self {
            size,
            renderer,
            cache: user_interface::Cache::default(),
            cursor: mouse::Cursor::Unavailable,
        }
    }

    fn resize(&mut self, size: Size) {
        self.size = size;
        // Layout and overlay geometry are size-dependent, while focus state
        // lives in the widgets themselves and is reconstructed by Iced.
        self.cache = user_interface::Cache::default();
    }

    fn find_bounds<S>(
        &mut self,
        view: Element<'_, Message>,
        selector: S,
    ) -> Result<Rectangle, HarnessError>
    where
        S: Selector + Send,
        S::Output: Bounded + Clone + Send + Sync + 'static,
    {
        let description = selector.description();
        let mut interface = UserInterface::build(
            view,
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let mut operation = selector.find();
        interface.operate(
            &self.renderer,
            &mut iced::advanced::widget::operation::black_box(&mut operation),
        );
        let outcome = operation.finish();
        self.cache = interface.into_cache();
        match outcome {
            iced::advanced::widget::operation::Outcome::Some(Some(target)) => target
                .visible_bounds()
                .ok_or_else(|| HarnessError::new(format!("{description} is not visible"))),
            _ => Err(HarnessError::new(format!(
                "no matching widget was found for selector: {description}"
            ))),
        }
    }

    fn dispatch(
        &mut self,
        view: Element<'_, Message>,
        events: impl IntoIterator<Item = Event>,
    ) -> (Vec<event::Status>, Vec<Message>) {
        let events = events.into_iter().collect::<Vec<_>>();
        for event in &events {
            if let Event::Mouse(mouse::Event::CursorMoved { position }) = event {
                self.cursor = mouse::Cursor::Available(*position);
            }
        }
        let mut interface = UserInterface::build(
            view,
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let mut messages = Vec::new();
        let (_, statuses) = interface.update(
            &events,
            self.cursor,
            &mut self.renderer,
            &mut clipboard::Null,
            &mut messages,
        );
        self.cache = interface.into_cache();
        (statuses, messages)
    }

    fn operate(&mut self, view: Element<'_, Message>, operation: &mut dyn Operation) {
        let mut interface = UserInterface::build(
            view,
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        interface.operate(&self.renderer, operation);
        self.cache = interface.into_cache();
    }

    fn is_focused(&mut self, view: Element<'_, Message>, id: iced::widget::Id) -> bool {
        self.find_bounds(view, move |candidate: Candidate<'_>| match candidate {
            Candidate::Focusable {
                id: Some(candidate_id),
                state,
                ..
            } if candidate_id == &id && state.is_focused() => Some(Target::from(candidate)),
            _ => None,
        })
        .is_ok()
    }
}

/// Runs the real native Iced view/update code without creating OS windows.
///
/// This type exists only when the `interaction-harness` feature is selected.
/// It acknowledges Iced window actions in memory and routes every emitted
/// product message back through [`NativeDesktop::update`].
pub struct NativeDesktopHarness {
    desktop: NativeDesktop,
    surfaces: BTreeMap<window::Id, PersistentSurface>,
    trace: Vec<HarnessTraceEntry>,
    next_sequence: u64,
    exited: bool,
}

impl NativeDesktopHarness {
    pub fn boot(startup: NativeDesktopStartup) -> Result<Self, HarnessError> {
        let (desktop, task) = NativeDesktop::boot(startup);
        let mut harness = Self {
            desktop,
            surfaces: BTreeMap::new(),
            trace: Vec::new(),
            next_sequence: 1,
            exited: false,
        };
        harness.run_task(task)?;
        Ok(harness)
    }

    pub fn trace(&self) -> &[HarnessTraceEntry] {
        &self.trace
    }

    pub fn has_window(&self, window: HarnessWindow) -> bool {
        self.window_id(window).is_ok()
    }

    pub fn exited(&self) -> bool {
        self.exited
    }

    pub fn click_text(&mut self, window: HarnessWindow, label: &str) -> Result<(), HarnessError> {
        let bounds = self.find_text_bounds(window, label)?;
        self.dispatch_events(
            window,
            Self::click_events(bounds.center(), mouse::Button::Left),
        )?;
        self.record(window, format!("click text {label:?}"));
        Ok(())
    }

    /// Clicks a stable production control through its rendered widget bounds.
    pub fn click_target(
        &mut self,
        window: HarnessWindow,
        target: HarnessTarget,
    ) -> Result<(), HarnessError> {
        let bounds = self.find_target_bounds(window, target)?;
        self.dispatch_events(
            window,
            Self::click_events(bounds.center(), mouse::Button::Left),
        )?;
        self.record(window, format!("click target {target:?}"));
        Ok(())
    }

    /// Closes one specific tab by its stable document identity.
    pub fn close_editor_tab(
        &mut self,
        window: HarnessWindow,
        pane: EditorPane,
        document_id: &str,
    ) -> Result<(), HarnessError> {
        let id = self.window_id(window)?;
        let task = self.desktop.update(Message::ProjectSurface {
            window: id,
            message: crate::iced_project_surface::ProjectSurfaceMessage::EditorCenter(
                crate::iced_editor_surface::EditorCenterMessage::Workspace(
                    EditorMessage::CloseTab {
                        pane,
                        document_id: document_id.to_owned(),
                    },
                ),
            ),
        });
        self.run_task(task)?;
        self.record(window, format!("close {pane:?} tab {document_id:?}"));
        Ok(())
    }

    /// Opens a stable production target's context menu through its rendered
    /// bounds. This keeps popover tests independent of window geometry.
    pub fn right_click_target(
        &mut self,
        window: HarnessWindow,
        target: HarnessTarget,
    ) -> Result<(), HarnessError> {
        let bounds = self.find_target_bounds(window, target)?;
        self.dispatch_events(
            window,
            Self::click_events(bounds.center(), mouse::Button::Right),
        )?;
        self.record(window, format!("right-click target {target:?}"));
        Ok(())
    }

    /// Opens a stable target's context menu at a fractional position in its
    /// current bounds. Fractions make text-selection popovers testable while
    /// remaining resilient to window size and layout changes.
    pub fn right_click_target_at(
        &mut self,
        window: HarnessWindow,
        target: HarnessTarget,
        position: (f32, f32),
    ) -> Result<(), HarnessError> {
        let point = Self::relative_position(self.find_target_bounds(window, target)?, position)?;
        self.dispatch_events(window, Self::click_events(point, mouse::Button::Right))?;
        self.record(
            window,
            format!("right-click target {target:?} at {position:?}"),
        );
        Ok(())
    }

    /// Opens the context menu for the rendered text without depending on its
    /// window coordinates.
    pub fn right_click_text(
        &mut self,
        window: HarnessWindow,
        label: &str,
    ) -> Result<(), HarnessError> {
        let bounds = self.find_text_bounds(window, label)?;
        self.dispatch_events(
            window,
            Self::click_events(bounds.center(), mouse::Button::Right),
        )?;
        self.record(window, format!("right-click text {label:?}"));
        Ok(())
    }

    pub fn type_into(
        &mut self,
        window: HarnessWindow,
        placeholder: &str,
        value: &str,
    ) -> Result<(), HarnessError> {
        self.click_text(window, placeholder)?;
        self.type_focused(window, value)?;
        self.record(
            window,
            format!(
                "type into {placeholder:?} {} characters",
                value.chars().count()
            ),
        );
        Ok(())
    }

    pub fn type_at(
        &mut self,
        window: HarnessWindow,
        point: (f32, f32),
        value: &str,
    ) -> Result<(), HarnessError> {
        self.dispatch_events(
            window,
            Self::click_events(IcedPoint::new(point.0, point.1), mouse::Button::Left),
        )?;
        self.type_focused(window, value)?;
        self.record(
            window,
            format!(
                "type at ({}, {}) {} characters",
                point.0,
                point.1,
                value.chars().count()
            ),
        );
        Ok(())
    }

    /// Focuses a stable production target and types during the same rendered
    /// interaction, preserving real widget event routing.
    pub fn type_into_target(
        &mut self,
        window: HarnessWindow,
        target: HarnessTarget,
        value: &str,
    ) -> Result<(), HarnessError> {
        self.click_target(window, target)?;
        self.type_focused(window, value)?;
        self.record(
            window,
            format!(
                "type into target {target:?} {} characters",
                value.chars().count()
            ),
        );
        Ok(())
    }

    /// Reports whether a stable target owns retained keyboard focus.
    pub fn target_is_focused(
        &mut self,
        window: HarnessWindow,
        target: HarnessTarget,
    ) -> Result<bool, HarnessError> {
        let id = self.window_id(window)?;
        self.ensure_surface(id, window)?;
        let (desktop, surfaces) = (&self.desktop, &mut self.surfaces);
        Ok(surfaces
            .get_mut(&id)
            .expect("surface was created")
            .is_focused(desktop.view(id), target.id()))
    }

    /// Reports whether a stable production target is presently in the
    /// rendered window, without changing its focus or state.
    pub fn target_is_visible(
        &mut self,
        window: HarnessWindow,
        target: HarnessTarget,
    ) -> Result<bool, HarnessError> {
        let id = self.window_id(window)?;
        self.ensure_surface(id, window)?;
        let (desktop, surfaces) = (&self.desktop, &mut self.surfaces);
        Ok(surfaces
            .get_mut(&id)
            .expect("surface was created")
            .find_bounds(desktop.view(id), target.id())
            .is_ok())
    }

    /// Reports whether a document is rendered as a tab in the requested pane.
    pub fn editor_tab_is_visible(
        &mut self,
        window: HarnessWindow,
        pane: EditorPane,
        document_id: &str,
    ) -> Result<bool, HarnessError> {
        let id = self.window_id(window)?;
        self.ensure_surface(id, window)?;
        let (desktop, surfaces) = (&self.desktop, &mut self.surfaces);
        Ok(surfaces
            .get_mut(&id)
            .expect("surface was created")
            .find_bounds(
                desktop.view(id),
                harness_target::editor_tab_id(pane, document_id),
            )
            .is_ok())
    }

    /// Types into the widget that retained focus from an earlier interaction.
    pub fn type_focused(&mut self, window: HarnessWindow, value: &str) -> Result<(), HarnessError> {
        let statuses = self.dispatch_events(window, iced_test::simulator::typewrite(value))?;
        let status = statuses
            .into_iter()
            .fold(event::Status::Ignored, event::Status::merge);
        if status == event::Status::Ignored {
            return Err(HarnessError::new(format!(
                "typing into the focused widget in {window} was ignored"
            )));
        }
        self.record(
            window,
            format!(
                "type into focused widget {} characters",
                value.chars().count()
            ),
        );
        Ok(())
    }

    /// Sends a non-text key to the widget that retained focus from an earlier interaction.
    pub fn press_key(
        &mut self,
        window: HarnessWindow,
        key: HarnessKey,
    ) -> Result<(), HarnessError> {
        self.dispatch_events(
            window,
            Self::key_tap_events(key.into_iced(), keyboard::Modifiers::NONE),
        )?;
        self.record(window, format!("press key {key:?}"));
        Ok(())
    }

    /// Sends the platform command-modifier shortcut to the focused widget.
    pub fn press_command_key(
        &mut self,
        window: HarnessWindow,
        key: char,
    ) -> Result<(), HarnessError> {
        self.dispatch_events(
            window,
            Self::key_tap_events(
                keyboard::Key::Character(key.to_string().into()),
                keyboard::Modifiers::COMMAND,
            ),
        )?;
        self.record(window, format!("press command-{key}"));
        Ok(())
    }

    /// Sends the platform command-and-shift modifier shortcut to the focused
    /// widget, used by the standard macOS redo binding.
    pub fn press_command_shift_key(
        &mut self,
        window: HarnessWindow,
        key: char,
    ) -> Result<(), HarnessError> {
        self.dispatch_events(
            window,
            Self::key_tap_events(
                keyboard::Key::Character(key.to_string().into()),
                keyboard::Modifiers::COMMAND | keyboard::Modifiers::SHIFT,
            ),
        )?;
        self.record(window, format!("press command-shift-{key}"));
        Ok(())
    }

    /// Replaces the current contents of a focused text input using the
    /// platform-standard Select All command before typing.
    pub fn replace_text(
        &mut self,
        window: HarnessWindow,
        current_value: &str,
        replacement: &str,
    ) -> Result<(), HarnessError> {
        self.replace_text_with_submit(window, current_value, replacement, false)
    }

    /// Replaces the current input contents and submits the input with Enter.
    pub fn replace_text_and_submit(
        &mut self,
        window: HarnessWindow,
        current_value: &str,
        replacement: &str,
    ) -> Result<(), HarnessError> {
        self.replace_text_with_submit(window, current_value, replacement, true)
    }

    /// Replaces a stable input's complete value with the platform-standard
    /// Select All command, avoiding a text-label lookup when the current value
    /// is duplicated elsewhere in the UI.
    pub fn replace_target(
        &mut self,
        window: HarnessWindow,
        target: HarnessTarget,
        replacement: &str,
    ) -> Result<(), HarnessError> {
        self.click_target(window, target)?;
        self.replace_focused(window, replacement, false)?;
        self.record(
            window,
            format!(
                "replace target {target:?} with {} characters",
                replacement.chars().count()
            ),
        );
        Ok(())
    }

    fn replace_text_with_submit(
        &mut self,
        window: HarnessWindow,
        current_value: &str,
        replacement: &str,
        submit: bool,
    ) -> Result<(), HarnessError> {
        self.click_text(window, current_value)?;
        self.replace_focused(window, replacement, submit)?;
        self.record(
            window,
            format!(
                "replace text {current_value:?} with {} characters{}",
                replacement.chars().count(),
                if submit { " and submit" } else { "" },
            ),
        );
        Ok(())
    }

    fn replace_focused(
        &mut self,
        window: HarnessWindow,
        replacement: &str,
        submit: bool,
    ) -> Result<(), HarnessError> {
        let mut events = vec![
            Event::Keyboard(keyboard::Event::ModifiersChanged(
                keyboard::Modifiers::COMMAND,
            )),
            Self::key_pressed(
                keyboard::Key::Character("a".into()),
                None,
                keyboard::Modifiers::COMMAND,
            ),
            Self::key_released(
                keyboard::Key::Character("a".into()),
                keyboard::Modifiers::COMMAND,
            ),
            Event::Keyboard(keyboard::Event::ModifiersChanged(keyboard::Modifiers::NONE)),
        ];
        events.extend(iced_test::simulator::typewrite(replacement));
        if submit {
            events.extend(Self::key_tap_events(
                keyboard::Key::Named(keyboard::key::Named::Enter),
                keyboard::Modifiers::NONE,
            ));
        }
        let statuses = self.dispatch_events(window, events)?;
        let status = statuses
            .into_iter()
            .fold(event::Status::Ignored, event::Status::merge);
        if status == event::Status::Ignored {
            return Err(HarnessError::new(format!(
                "replacing the focused widget in {window} was ignored"
            )));
        }
        Ok(())
    }

    /// Drags a visible text source onto a visible text destination through the
    /// retained Iced pointer lifecycle.
    pub fn drag_text_to_text(
        &mut self,
        window: HarnessWindow,
        source: &str,
        destination: &str,
    ) -> Result<(), HarnessError> {
        self.drag_text_to_text_at(window, source, destination, HarnessDropPosition::Into)
    }

    /// Drags a visible text source to a semantic region of a visible text
    /// destination. The destination point is derived from current rendered
    /// bounds, never a fixed window coordinate.
    pub fn drag_text_to_text_at(
        &mut self,
        window: HarnessWindow,
        source: &str,
        destination: &str,
        position: HarnessDropPosition,
    ) -> Result<(), HarnessError> {
        let source_bounds = self.find_text_bounds(window, source)?;
        let source_position = source_bounds.center();
        let threshold_position = IcedPoint::new(source_position.x + 5.0, source_position.y);
        self.dispatch_events(
            window,
            [
                Event::Mouse(mouse::Event::CursorMoved {
                    position: source_position,
                }),
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                // The hierarchy source publishes drag-start after four pixels.
                // This separate move rebuilds Cards with its live drop strips
                // before the destination receives its hover event.
                Event::Mouse(mouse::Event::CursorMoved {
                    position: threshold_position,
                }),
            ],
        )?;
        let destination_position =
            Self::drop_position(self.find_text_bounds(window, destination)?, position);
        self.dispatch_events(
            window,
            [
                Event::Mouse(mouse::Event::CursorMoved {
                    position: destination_position,
                }),
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            ],
        )?;
        self.record(
            window,
            format!("drag text {source:?} {position:?} {destination:?}"),
        );
        Ok(())
    }

    /// Drags between two fractional positions in one stable production target.
    ///
    /// The events are routed through the retained Iced tree, so custom editor
    /// selections and their subsequent context-menu popovers behave as they do
    /// in the native application. Coordinates are fractions of live widget
    /// bounds, never window coordinates.
    pub fn drag_within_target(
        &mut self,
        window: HarnessWindow,
        target: HarnessTarget,
        from: (f32, f32),
        to: (f32, f32),
    ) -> Result<(), HarnessError> {
        let bounds = self.find_target_bounds(window, target)?;
        let from = Self::relative_position(bounds, from)?;
        let to = Self::relative_position(bounds, to)?;
        self.dispatch_events(
            window,
            [
                Event::Mouse(mouse::Event::CursorMoved { position: from }),
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Event::Mouse(mouse::Event::CursorMoved { position: to }),
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            ],
        )?;
        self.record(
            window,
            format!("drag within target {target:?} from {from:?} to {to:?}"),
        );
        Ok(())
    }

    /// Selects one uniquely occurring run of prose in a mounted editor by its
    /// document position. This semantic harness action deliberately avoids
    /// fractional canvas drags, whose geometry changes with wrapping and font
    /// metrics, while still routes the real mounted-editor selection message.
    pub fn select_editor_text(
        &mut self,
        window: HarnessWindow,
        pane: crate::EditorPane,
        text: &str,
    ) -> Result<(), HarnessError> {
        let id = self.window_id(window)?;
        let (view, selection) = {
            let NativeWindow::Project(state) = self
                .desktop
                .windows
                .get(&id)
                .ok_or_else(|| HarnessError::new("project window is unavailable"))?
            else {
                return Err(HarnessError::new("selected window is not a project"));
            };
            let binding = state
                .editor_bindings
                .get(&pane)
                .ok_or_else(|| HarnessError::new(format!("{pane:?} editor pane is not mounted")))?;
            let adapter = state
                .project
                .editor_adapter()
                .ok_or_else(|| HarnessError::new("project editor adapter is unavailable"))?;
            let body = adapter
                .primary_visible_block(binding.session())
                .map_err(|error| HarnessError::new(error.to_string()))?
                .text()
                .to_owned();
            let matches = body.match_indices(text).collect::<Vec<_>>();
            let [(byte_start, _)] = matches.as_slice() else {
                return Err(HarnessError::new(if matches.is_empty() {
                    format!("editor prose {text:?} was not found")
                } else {
                    format!("editor prose {text:?} is ambiguous")
                }));
            };
            let start = body[..*byte_start].chars().count() as u64;
            let end = start.saturating_add(text.chars().count() as u64);
            (
                binding.view(),
                parchmint_editor_api::EditorSelection::new(start.into(), end.into()),
            )
        };
        let task = self.desktop.update(Message::ProjectSurface {
            window: id,
            message: crate::iced_project_surface::ProjectSurfaceMessage::EditorCenter(
                crate::iced_editor_surface::EditorCenterMessage::Mounted {
                    pane,
                    view,
                    message: parchmint_editor_iced::MountedEditorMessage::SetSelection(selection),
                },
            ),
        });
        self.run_task(task)?;
        self.record(window, format!("select editor prose {text:?} in {pane:?}"));
        Ok(())
    }

    /// Resolves one uniquely titled node to its opaque live hierarchy address.
    pub fn hierarchy_node(&self, title: &str) -> Result<HarnessNode, HarnessError> {
        let id = self.window_id(HarnessWindow::Project)?;
        let NativeWindow::Project(state) = self
            .desktop
            .windows
            .get(&id)
            .ok_or_else(|| HarnessError::new("project window is unavailable"))?
        else {
            return Err(HarnessError::new("selected window is not a project"));
        };
        let workspace = state
            .workspace
            .as_ref()
            .ok_or_else(|| HarnessError::new("project workspace has not loaded"))?;
        let matches = workspace
            .explorer()
            .rows()
            .into_iter()
            .filter(|row| row.title == title)
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [row] => Ok(HarnessNode(row.id.to_owned())),
            [] => Err(HarnessError::new(format!(
                "no hierarchy node is titled {title:?}"
            ))),
            _ => Err(HarnessError::new(format!(
                "hierarchy title {title:?} is ambiguous"
            ))),
        }
    }

    /// Returns the editor's live comment-composer feedback for diagnostics and
    /// workflow assertions.
    pub fn comment_feedback(&self) -> Result<String, HarnessError> {
        let id = self.window_id(HarnessWindow::Project)?;
        let NativeWindow::Project(state) = self
            .desktop
            .windows
            .get(&id)
            .ok_or_else(|| HarnessError::new("project window is unavailable"))?
        else {
            return Err(HarnessError::new("selected window is not a project"));
        };
        state
            .workspace
            .as_ref()
            .map(|workspace| {
                workspace
                    .editor()
                    .comment_feedback()
                    .unwrap_or_default()
                    .to_owned()
            })
            .ok_or_else(|| HarnessError::new("project workspace has not loaded"))
    }

    /// Summarizes the live History comparison state for workflow diagnostics.
    pub fn history_status(&self) -> Result<String, HarnessError> {
        let id = self.window_id(HarnessWindow::Project)?;
        let NativeWindow::Project(state) = self
            .desktop
            .windows
            .get(&id)
            .ok_or_else(|| HarnessError::new("project window is unavailable"))?
        else {
            return Err(HarnessError::new("selected window is not a project"));
        };
        state
            .workspace
            .as_ref()
            .map(|workspace| {
                let history = workspace.history();
                format!(
                    "selected={:?}, preview={}, current={}, comparison={}, error={:?}",
                    history.selected_checkpoint_id(),
                    history.preview().is_some(),
                    history.current_document().is_some(),
                    history.comparison().is_some(),
                    history.error(),
                )
            })
            .ok_or_else(|| HarnessError::new("project workspace has not loaded"))
    }

    /// Summarizes the live project-wide search state for workflow diagnostics.
    pub fn global_search_status(&self) -> Result<String, HarnessError> {
        let id = self.window_id(HarnessWindow::Project)?;
        let NativeWindow::Project(state) = self
            .desktop
            .windows
            .get(&id)
            .ok_or_else(|| HarnessError::new("project window is unavailable"))?
        else {
            return Err(HarnessError::new("selected window is not a project"));
        };
        state
            .workspace
            .as_ref()
            .map(|workspace| {
                let search = workspace.global_search();
                let documents = search
                    .results()
                    .iter()
                    .map(|result| result.document_id.as_str())
                    .collect::<std::collections::BTreeSet<_>>();
                format!(
                    "query={:?}, results={}, documents={}, complete={}, error={:?}",
                    search.query(),
                    search.results().len(),
                    documents.len(),
                    search.is_complete(),
                    search.error(),
                )
            })
            .ok_or_else(|| HarnessError::new("project workspace has not loaded"))
    }

    /// Invokes the production single-click behavior for a live Explorer row
    /// through its opaque hierarchy identity. The row is a drag source, so
    /// routing its semantic action avoids confusing the source's nested
    /// pointer ownership with a geometry-dependent synthetic click.
    pub fn click_hierarchy_node(
        &mut self,
        window: HarnessWindow,
        node: &HarnessNode,
    ) -> Result<(), HarnessError> {
        let id = self.window_id(window)?;
        let message = {
            let NativeWindow::Project(state) = self
                .desktop
                .windows
                .get(&id)
                .ok_or_else(|| HarnessError::new("project window is unavailable"))?
            else {
                return Err(HarnessError::new("selected window is not a project"));
            };
            let row = state
                .workspace
                .as_ref()
                .and_then(|workspace| workspace.explorer().row(node.id()))
                .ok_or_else(|| HarnessError::new("hierarchy node is unavailable"))?;
            match row.kind {
                HierarchyRowKind::Document => {
                    ProjectMessage::PreviewHierarchyNode(node.id().to_owned())
                }
                HierarchyRowKind::Group => {
                    ProjectMessage::ToggleHierarchyExpanded(node.id().to_owned())
                }
                HierarchyRowKind::Root => ProjectMessage::SelectHierarchy {
                    node_id: node.id().to_owned(),
                    gesture: SelectionGesture::Replace,
                },
            }
        };
        let task = self.desktop.update(Message::ProjectSurface {
            window: id,
            message: crate::iced_project_surface::ProjectSurfaceMessage::Project(message),
        });
        self.run_task(task)?;
        self.record(window, format!("click hierarchy node {node:?}"));
        Ok(())
    }

    /// Opens a hierarchy row's production context menu through its stable
    /// identity while anchoring it at the live rendered row bounds.
    pub fn right_click_hierarchy_node(
        &mut self,
        window: HarnessWindow,
        node: &HarnessNode,
    ) -> Result<(), HarnessError> {
        let bounds = self.find_id_bounds(window, harness_target::explorer_row_id(node.id()))?;
        let id = self.window_id(window)?;
        let task = self.desktop.update(Message::ProjectSurface {
            window: id,
            message: crate::iced_project_surface::ProjectSurfaceMessage::Project(
                ProjectMessage::OpenHierarchyContextMenu {
                    node_id: node.id().to_owned(),
                    point: Point::new(bounds.center().x, bounds.center().y),
                },
            ),
        });
        self.run_task(task)?;
        self.record(window, format!("right-click hierarchy node {node:?}"));
        Ok(())
    }

    /// Reports whether a resolved hierarchy row is currently rendered in the
    /// Explorer, rather than merely existing in its authoritative projection.
    pub fn hierarchy_node_is_visible(
        &mut self,
        window: HarnessWindow,
        node: &HarnessNode,
    ) -> Result<bool, HarnessError> {
        let id = self.window_id(window)?;
        self.ensure_surface(id, window)?;
        let (desktop, surfaces) = (&self.desktop, &mut self.surfaces);
        Ok(surfaces
            .get_mut(&id)
            .expect("surface was created")
            .find_bounds(desktop.view(id), harness_target::explorer_row_id(node.id()))
            .is_ok())
    }

    /// Selects a loaded History checkpoint by its visible list position.
    /// Checkpoint labels intentionally repeat (for example, automatic saves),
    /// so this resolves the stable opaque ID before dispatching the real click.
    pub fn click_history_checkpoint(
        &mut self,
        window: HarnessWindow,
        position: usize,
    ) -> Result<(), HarnessError> {
        let id = self.window_id(window)?;
        let NativeWindow::Project(state) = self
            .desktop
            .windows
            .get(&id)
            .ok_or_else(|| HarnessError::new("project window is unavailable"))?
        else {
            return Err(HarnessError::new("selected window is not a project"));
        };
        let checkpoint_id = state
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.history().windowed_checkpoints().nth(position))
            .map(|checkpoint| checkpoint.checkpoint_id.clone())
            .ok_or_else(|| {
                HarnessError::new(format!(
                    "history checkpoint at visible position {position} is unavailable"
                ))
            })?;
        let bounds = self.find_id_bounds(
            window,
            harness_target::history_checkpoint_id(&checkpoint_id),
        )?;
        self.dispatch_events(
            window,
            Self::click_events(bounds.center(), mouse::Button::Left),
        )?;
        self.record(
            window,
            format!("click history checkpoint at visible position {position}"),
        );
        Ok(())
    }

    /// Drags opaque hierarchy nodes through a projection-specific set of
    /// production targets. Cards use their live insertion strips; Explorer
    /// uses the before/inside/after regions of its complete row.
    pub fn drag_hierarchy_node(
        &mut self,
        window: HarnessWindow,
        surface: HarnessHierarchySurface,
        source: &HarnessNode,
        destination: &HarnessNode,
        position: HarnessDropPosition,
    ) -> Result<(), HarnessError> {
        let source_id = match surface {
            HarnessHierarchySurface::Explorer => harness_target::explorer_row_id(source.id()),
            HarnessHierarchySurface::Cards => harness_target::card_id(source.id()),
        };
        let source_position = self.find_id_bounds(window, source_id)?.center();
        let threshold_position = IcedPoint::new(source_position.x + 5.0, source_position.y);
        self.dispatch_events(
            window,
            [
                Event::Mouse(mouse::Event::CursorMoved {
                    position: source_position,
                }),
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Event::Mouse(mouse::Event::CursorMoved {
                    position: threshold_position,
                }),
            ],
        )?;
        let destination_position = match surface {
            HarnessHierarchySurface::Explorer => {
                let bounds =
                    self.find_id_bounds(window, harness_target::explorer_row_id(destination.id()))?;
                Self::drop_position(bounds, position)
            }
            HarnessHierarchySurface::Cards => {
                let id = match position {
                    HarnessDropPosition::Before => {
                        harness_target::card_drop_before_id(destination.id())
                    }
                    HarnessDropPosition::Into => harness_target::card_id(destination.id()),
                    HarnessDropPosition::After => {
                        harness_target::card_drop_after_id(destination.id())
                    }
                };
                let bounds = self.find_id_bounds(window, id.clone())?;
                bounds.center()
            }
        };
        self.dispatch_events(
            window,
            [
                Event::Mouse(mouse::Event::CursorMoved {
                    position: destination_position,
                }),
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            ],
        )?;
        self.record(
            window,
            format!(
                "drag hierarchy node {:?} {position:?} {:?} through {surface:?}",
                source, destination
            ),
        );
        Ok(())
    }

    pub fn contains_text(&self, window: HarnessWindow, text: &str) -> Result<bool, HarnessError> {
        let id = self.window_id(window)?;
        let mut simulator = Simulator::<Message>::with_size(
            Settings::default(),
            Self::window_size(window),
            self.desktop.view(id),
        );
        Ok(simulator.find(text).is_ok())
    }

    /// Resizes a headless production window and dispatches the matching Iced
    /// window event. Author flows use this to exercise responsive layouts
    /// through the same update path as a real desktop resize.
    pub fn resize(
        &mut self,
        window: HarnessWindow,
        width: f32,
        height: f32,
    ) -> Result<(), HarnessError> {
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return Err(HarnessError::new(format!(
                "window dimensions must be finite and positive, got {width}×{height}"
            )));
        }
        let id = self.window_id(window)?;
        self.ensure_surface(id, window)?;
        let size = Size::new(width, height);
        self.surfaces
            .get_mut(&id)
            .expect("surface was created")
            .resize(size);
        self.dispatch_events(window, [Event::Window(window::Event::Resized(size))])?;
        self.record(window, format!("resize window to {width}×{height}"));
        Ok(())
    }

    /// Delivers the next window render frame. Some production controls, such
    /// as a newly inserted inline text field, can only receive focus once the
    /// toolkit has placed them in the rendered tree.
    pub fn redraw(&mut self, window: HarnessWindow) -> Result<(), HarnessError> {
        self.dispatch_events(
            window,
            [Event::Window(
                window::Event::RedrawRequested(Instant::now()),
            )],
        )?;
        self.record(window, "render window frame".to_owned());
        Ok(())
    }

    /// Advances the product autosave clock past the idle delay without sleeping.
    pub fn elapse_autosave_idle(&mut self) -> Result<(), HarnessError> {
        let now = Instant::now();
        let elapsed = AutosaveState::IDLE_DELAY + Duration::from_millis(1);
        let mut found_dirty = false;
        for native in self.desktop.windows.values_mut() {
            let NativeWindow::Project(state) = native else {
                continue;
            };
            if !state.autosave.dirty_sessions.is_empty() {
                state.autosave.first_dirty = Some(now - elapsed);
                state.autosave.last_edit = Some(now - elapsed);
                found_dirty = true;
            }
        }
        if !found_dirty {
            return Err(HarnessError::new(
                "autosave could not advance because no editor session is dirty",
            ));
        }
        self.record(HarnessWindow::Project, "elapse autosave idle".to_owned());
        let task = self.desktop.update(Message::AutosaveTick(now));
        self.run_task(task)
    }

    pub fn close(&mut self, window: HarnessWindow) -> Result<(), HarnessError> {
        let id = self.window_id(window)?;
        self.record(window, "close window".to_owned());
        let task = self.desktop.update(Message::CloseRequested(id));
        self.run_task(task)
    }

    pub fn active_editor_body(&self) -> Result<String, HarnessError> {
        let id = self.window_id(HarnessWindow::Project)?;
        let NativeWindow::Project(state) = self
            .desktop
            .windows
            .get(&id)
            .ok_or_else(|| HarnessError::new("project window is unavailable"))?
        else {
            return Err(HarnessError::new("selected window is not a project"));
        };
        let workspace = state
            .workspace
            .as_ref()
            .ok_or_else(|| HarnessError::new("project workspace has not loaded"))?;
        let pane = workspace.editor().focused_pane();
        let binding = state
            .editor_bindings
            .get(&pane)
            .ok_or_else(|| HarnessError::new("focused editor is not mounted"))?;
        let adapter = state
            .project
            .editor_adapter()
            .ok_or_else(|| HarnessError::new("project editor adapter is unavailable"))?;
        let session = binding.session();
        let revision = adapter
            .revision(session.clone())
            .map_err(|error| HarnessError::new(error.to_string()))?;
        let projection = iced::futures::executor::block_on(adapter.project(session, revision))
            .map_err(|error| HarnessError::new(error.to_string()))?;
        Ok(projection.body().to_owned())
    }

    /// Returns the title of the document in the focused authoring pane.
    pub fn active_editor_tab_title(&self) -> Result<String, HarnessError> {
        let id = self.window_id(HarnessWindow::Project)?;
        let NativeWindow::Project(state) = self
            .desktop
            .windows
            .get(&id)
            .ok_or_else(|| HarnessError::new("project window is unavailable"))?
        else {
            return Err(HarnessError::new("selected window is not a project"));
        };
        let workspace = state
            .workspace
            .as_ref()
            .ok_or_else(|| HarnessError::new("project workspace has not loaded"))?;
        let pane = workspace.editor().focused_pane();
        let document_id = workspace
            .editor()
            .pane(pane)
            .active_document()
            .ok_or_else(|| HarnessError::new("focused editor has no active document"))?;
        workspace
            .editor()
            .pane(pane)
            .tabs()
            .iter()
            .find(|tab| tab.id() == document_id)
            .map(|tab| tab.title().to_owned())
            .ok_or_else(|| HarnessError::new("focused editor tab is unavailable"))
    }

    /// Returns the active document identity for the requested authoring pane.
    pub fn active_editor_document_id(&self, pane: EditorPane) -> Result<String, HarnessError> {
        let id = self.window_id(HarnessWindow::Project)?;
        let NativeWindow::Project(state) = self
            .desktop
            .windows
            .get(&id)
            .ok_or_else(|| HarnessError::new("project window is unavailable"))?
        else {
            return Err(HarnessError::new("selected window is not a project"));
        };
        state
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.editor().pane(pane).active_document())
            .map(str::to_owned)
            .ok_or_else(|| HarnessError::new("editor pane has no active document"))
    }

    /// Describes the live replacement-preview state for deterministic flow
    /// diagnostics when a semantic action is unavailable.
    pub fn replacement_status(&self) -> Result<String, HarnessError> {
        let id = self.window_id(HarnessWindow::Project)?;
        let NativeWindow::Project(state) = self
            .desktop
            .windows
            .get(&id)
            .ok_or_else(|| HarnessError::new("project window is unavailable"))?
        else {
            return Err(HarnessError::new("selected window is not a project"));
        };
        let workspace = state
            .workspace
            .as_ref()
            .ok_or_else(|| HarnessError::new("project workspace has not loaded"))?;
        let preview = workspace.replacement_preview();
        let revisions = format!(
            "workspace={}, snapshot={}",
            workspace.project_revision(),
            state
                .project
                .project_ui
                .as_ref()
                .map_or(0, |project| project.snapshot.project.revision.value()),
        );
        Ok(if let Some(error) = preview.validation_error() {
            format!("failed ({revisions}): {error}")
        } else if preview.is_validating() {
            format!("validating ({revisions})")
        } else if preview.is_revalidated() {
            format!("ready ({revisions})")
        } else {
            format!("draft ({revisions})")
        })
    }

    /// Returns the current Explorer projection in its visible hierarchy order.
    pub fn hierarchy_titles(&self) -> Result<Vec<String>, HarnessError> {
        let id = self.window_id(HarnessWindow::Project)?;
        let NativeWindow::Project(state) = self
            .desktop
            .windows
            .get(&id)
            .ok_or_else(|| HarnessError::new("project window is unavailable"))?
        else {
            return Err(HarnessError::new("selected window is not a project"));
        };
        let workspace = state
            .workspace
            .as_ref()
            .ok_or_else(|| HarnessError::new("project workspace has not loaded"))?;
        Ok(workspace
            .explorer()
            .rows()
            .into_iter()
            .map(|row| row.title.to_owned())
            .collect())
    }

    /// Returns the primary pane's tabs in author-visible order. This observes
    /// the live workspace only; flows still activate tabs through their
    /// rendered strip or overflow control.
    pub fn tab_titles(&self) -> Result<Vec<String>, HarnessError> {
        let id = self.window_id(HarnessWindow::Project)?;
        let NativeWindow::Project(state) = self
            .desktop
            .windows
            .get(&id)
            .ok_or_else(|| HarnessError::new("project window is unavailable"))?
        else {
            return Err(HarnessError::new("selected window is not a project"));
        };
        let workspace = state
            .workspace
            .as_ref()
            .ok_or_else(|| HarnessError::new("project workspace has not loaded"))?;
        Ok(workspace
            .editor()
            .pane(EditorPane::Primary)
            .tabs()
            .iter()
            .map(|tab| tab.title().to_owned())
            .collect())
    }

    pub fn snapshot(
        &self,
        window: HarnessWindow,
        path: impl AsRef<Path>,
    ) -> Result<(), HarnessError> {
        let id = self.window_id(window)?;
        let mut simulator = Simulator::<Message>::with_size(
            Settings::default(),
            Self::window_size(window),
            self.desktop.view(id),
        );
        simulator
            .snapshot(&self.desktop.theme(id))
            .and_then(|snapshot| snapshot.matches_image(path))
            .map(|_| ())
            .map_err(|error| HarnessError::new(error.to_string()))
    }

    fn find_text_bounds(
        &mut self,
        window: HarnessWindow,
        text: &str,
    ) -> Result<Rectangle, HarnessError> {
        let id = self.window_id(window)?;
        self.ensure_surface(id, window)?;
        let (desktop, surfaces) = (&self.desktop, &mut self.surfaces);
        surfaces
            .get_mut(&id)
            .expect("surface was created")
            .find_bounds(desktop.view(id), text)
            .map_err(|error| {
                HarnessError::new(format!("could not find {text:?} in {window}: {error}"))
            })
    }

    fn find_target_bounds(
        &mut self,
        window: HarnessWindow,
        target: HarnessTarget,
    ) -> Result<Rectangle, HarnessError> {
        let id = self.window_id(window)?;
        self.ensure_surface(id, window)?;
        let (desktop, surfaces) = (&self.desktop, &mut self.surfaces);
        surfaces
            .get_mut(&id)
            .expect("surface was created")
            .find_bounds(desktop.view(id), target.id())
            .map_err(|error| {
                HarnessError::new(format!("could not find {target:?} in {window}: {error}"))
            })
    }

    fn find_id_bounds(
        &mut self,
        window: HarnessWindow,
        id: iced::widget::Id,
    ) -> Result<Rectangle, HarnessError> {
        let window_id = self.window_id(window)?;
        self.ensure_surface(window_id, window)?;
        let (desktop, surfaces) = (&self.desktop, &mut self.surfaces);
        surfaces
            .get_mut(&window_id)
            .expect("surface was created")
            .find_bounds(desktop.view(window_id), id)
            .map_err(|error| {
                HarnessError::new(format!("could not find target in {window}: {error}"))
            })
    }

    fn dispatch_events(
        &mut self,
        window: HarnessWindow,
        events: impl IntoIterator<Item = Event>,
    ) -> Result<Vec<event::Status>, HarnessError> {
        let id = self.window_id(window)?;
        self.ensure_surface(id, window)?;
        let mut statuses = Vec::new();
        for event in events {
            let (event_statuses, messages) = {
                let (desktop, surfaces) = (&self.desktop, &mut self.surfaces);
                surfaces
                    .get_mut(&id)
                    .expect("surface was created")
                    .dispatch(desktop.view(id), [event.clone()])
            };
            let status = event_statuses
                .iter()
                .copied()
                .fold(event::Status::Ignored, event::Status::merge);
            self.route_messages(messages)?;
            if let Some(message) = super::runtime_event(event, status, id) {
                let task = self.desktop.update(message);
                self.run_task(task)?;
            }
            statuses.extend(event_statuses);
        }
        Ok(statuses)
    }

    fn ensure_surface(
        &mut self,
        id: window::Id,
        window: HarnessWindow,
    ) -> Result<(), HarnessError> {
        if self.surfaces.contains_key(&id) {
            return Ok(());
        }
        self.surfaces
            .insert(id, PersistentSurface::new(Self::window_size(window)));
        Ok(())
    }

    fn execute_widget_operation(
        &mut self,
        operation: Box<dyn Operation>,
    ) -> Result<(), HarnessError> {
        let windows = self
            .desktop
            .windows
            .iter()
            .map(|(id, native)| {
                let window = match native {
                    NativeWindow::Launcher => HarnessWindow::Launcher,
                    NativeWindow::Project(_) => HarnessWindow::Project,
                };
                (*id, window)
            })
            .collect::<Vec<_>>();
        let mut operation = Some(operation);
        while let Some(mut current) = operation.take() {
            for (id, window) in &windows {
                self.ensure_surface(*id, *window)?;
                let (desktop, surfaces) = (&self.desktop, &mut self.surfaces);
                surfaces
                    .get_mut(id)
                    .expect("surface was created")
                    .operate(desktop.view(*id), current.as_mut());
            }
            if let iced::advanced::widget::operation::Outcome::Chain(next) = current.finish() {
                operation = Some(next);
            }
        }
        Ok(())
    }

    fn click_events(position: IcedPoint, button: mouse::Button) -> [Event; 3] {
        [
            Event::Mouse(mouse::Event::CursorMoved { position }),
            Event::Mouse(mouse::Event::ButtonPressed(button)),
            Event::Mouse(mouse::Event::ButtonReleased(button)),
        ]
    }

    fn drop_position(bounds: Rectangle, position: HarnessDropPosition) -> IcedPoint {
        let offset = match position {
            HarnessDropPosition::Before => 0.1,
            HarnessDropPosition::Into => 0.5,
            HarnessDropPosition::After => 0.9,
        };
        IcedPoint::new(
            bounds.x + bounds.width / 2.0,
            bounds.y + bounds.height * offset,
        )
    }

    fn relative_position(bounds: Rectangle, (x, y): (f32, f32)) -> Result<IcedPoint, HarnessError> {
        if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
            return Err(HarnessError::new(format!(
                "target-relative positions must be in 0.0..=1.0, got ({x}, {y})"
            )));
        }
        Ok(IcedPoint::new(
            bounds.x + bounds.width * x,
            bounds.y + bounds.height * y,
        ))
    }

    fn key_tap_events(key: keyboard::Key, modifiers: keyboard::Modifiers) -> [Event; 2] {
        [
            Self::key_pressed(key.clone(), None, modifiers),
            Self::key_released(key, modifiers),
        ]
    }

    fn key_pressed(
        key: keyboard::Key,
        text: Option<SmolStr>,
        modifiers: keyboard::Modifiers,
    ) -> Event {
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers,
            repeat: false,
            text,
        })
    }

    fn key_released(key: keyboard::Key, modifiers: keyboard::Modifiers) -> Event {
        Event::Keyboard(keyboard::Event::KeyReleased {
            key: key.clone(),
            modified_key: key,
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers,
        })
    }

    fn window_size(window: HarnessWindow) -> Size {
        match window {
            HarnessWindow::Launcher => LAUNCHER_SIZE,
            HarnessWindow::Project => PROJECT_SIZE,
        }
    }

    fn window_id(&self, window: HarnessWindow) -> Result<window::Id, HarnessError> {
        self.desktop
            .windows
            .iter()
            .find_map(|(id, native)| match (window, native) {
                (HarnessWindow::Launcher, NativeWindow::Launcher)
                | (HarnessWindow::Project, NativeWindow::Project(_)) => Some(*id),
                _ => None,
            })
            .ok_or_else(|| HarnessError::new(format!("{window} window is not open")))
    }

    fn record(&mut self, window: HarnessWindow, action: String) {
        self.trace.push(HarnessTraceEntry {
            sequence: self.next_sequence,
            window,
            action,
        });
        self.next_sequence = self.next_sequence.saturating_add(1);
    }

    fn route_messages(
        &mut self,
        messages: impl IntoIterator<Item = Message>,
    ) -> Result<(), HarnessError> {
        for message in messages {
            let task = self.desktop.update(message);
            self.run_task(task)?;
        }
        Ok(())
    }

    fn run_task(&mut self, task: Task<Message>) -> Result<(), HarnessError> {
        let mut pending = VecDeque::from([task]);
        while let Some(task) = pending.pop_front() {
            let Some(mut stream) = runtime::task::into_stream(task) else {
                continue;
            };
            while let Some(action) = iced::futures::executor::block_on(stream.next()) {
                match action {
                    runtime::Action::Output(message) => {
                        pending.push_back(self.desktop.update(message));
                    }
                    runtime::Action::LoadFont { channel, .. } => {
                        let _ = channel.send(Ok(()));
                    }
                    runtime::Action::Window(action) => self.handle_window_action(action),
                    runtime::Action::System(action) => self.handle_system_action(action),
                    runtime::Action::Clipboard(action) => self.handle_clipboard_action(action),
                    runtime::Action::Widget(operation) => {
                        self.execute_widget_operation(operation)?
                    }
                    runtime::Action::Image(_) | runtime::Action::Reload => {}
                    runtime::Action::Exit => self.exited = true,
                }
            }
        }
        Ok(())
    }

    fn handle_window_action(&self, action: runtime::window::Action) {
        match action {
            runtime::window::Action::Open(id, _, sender) => {
                let _ = sender.send(id);
            }
            runtime::window::Action::GetOldest(sender)
            | runtime::window::Action::GetLatest(sender) => {
                let _ = sender.send(self.desktop.windows.keys().next().copied());
            }
            runtime::window::Action::GetSize(id, sender) => {
                let size = if matches!(self.desktop.windows.get(&id), Some(NativeWindow::Launcher))
                {
                    LAUNCHER_SIZE
                } else {
                    PROJECT_SIZE
                };
                let _ = sender.send(size);
            }
            runtime::window::Action::GetMaximized(_, sender) => {
                let _ = sender.send(false);
            }
            runtime::window::Action::GetMinimized(_, sender) => {
                let _ = sender.send(Some(false));
            }
            runtime::window::Action::GetPosition(_, sender) => {
                let _ = sender.send(Some(IcedPoint::ORIGIN));
            }
            runtime::window::Action::GetScaleFactor(_, sender) => {
                let _ = sender.send(1.0);
            }
            runtime::window::Action::GetMode(_, sender) => {
                let _ = sender.send(window::Mode::Windowed);
            }
            runtime::window::Action::GetRawId(_, sender) => {
                let _ = sender.send(0);
            }
            runtime::window::Action::GetMonitorSize(_, sender) => {
                let _ = sender.send(Some(PROJECT_SIZE));
            }
            runtime::window::Action::Close(_)
            | runtime::window::Action::Drag(_)
            | runtime::window::Action::DragResize(_, _)
            | runtime::window::Action::Resize(_, _)
            | runtime::window::Action::Maximize(_, _)
            | runtime::window::Action::Minimize(_, _)
            | runtime::window::Action::Move(_, _)
            | runtime::window::Action::SetMode(_, _)
            | runtime::window::Action::ToggleMaximize(_)
            | runtime::window::Action::ToggleDecorations(_)
            | runtime::window::Action::RequestUserAttention(_, _)
            | runtime::window::Action::GainFocus(_)
            | runtime::window::Action::SetLevel(_, _)
            | runtime::window::Action::ShowSystemMenu(_)
            | runtime::window::Action::SetIcon(_, _)
            | runtime::window::Action::Run(_, _)
            | runtime::window::Action::Screenshot(_, _)
            | runtime::window::Action::EnableMousePassthrough(_)
            | runtime::window::Action::DisableMousePassthrough(_)
            | runtime::window::Action::SetMinSize(_, _)
            | runtime::window::Action::SetMaxSize(_, _)
            | runtime::window::Action::SetResizable(_, _)
            | runtime::window::Action::SetResizeIncrements(_, _)
            | runtime::window::Action::SetAllowAutomaticTabbing(_)
            | runtime::window::Action::RedrawAll
            | runtime::window::Action::RelayoutAll => {}
        }
    }

    fn handle_system_action(&self, action: runtime::system::Action) {
        match action {
            runtime::system::Action::GetTheme(sender) => {
                let mode = match self.desktop.appearance {
                    ResolvedAppearance::Light => iced::theme::Mode::Light,
                    ResolvedAppearance::Dark => iced::theme::Mode::Dark,
                };
                let _ = sender.send(mode);
            }
            runtime::system::Action::GetInformation(sender) => {
                let _ = sender.send(runtime::system::Information {
                    system_name: Some("ParchMint headless harness".to_owned()),
                    system_kernel: None,
                    system_version: None,
                    system_short_version: None,
                    cpu_brand: "headless".to_owned(),
                    cpu_cores: None,
                    memory_total: 0,
                    memory_used: None,
                    graphics_backend: "headless".to_owned(),
                    graphics_adapter: "iced_test".to_owned(),
                });
            }
            runtime::system::Action::NotifyTheme(_) => {}
        }
    }

    fn handle_clipboard_action(&self, action: runtime::clipboard::Action) {
        match action {
            runtime::clipboard::Action::Read { channel, .. } => {
                let _ = channel.send(None);
            }
            runtime::clipboard::Action::Write { .. } => {}
        }
    }
}
