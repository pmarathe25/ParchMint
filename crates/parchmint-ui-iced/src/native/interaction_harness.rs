//! Feature-gated, headless interaction driver for the native desktop surface.

use std::{collections::VecDeque, fmt, path::Path, time::Duration};

use iced::{Point as IcedPoint, Settings, Size, event, futures::StreamExt, window};
use iced_test::{Simulator, runtime};

use super::*;

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

/// Runs the real native Iced view/update code without creating OS windows.
///
/// This type exists only when the `interaction-harness` feature is selected.
/// It acknowledges Iced window actions in memory and routes every emitted
/// product message back through [`NativeDesktop::update`].
pub struct NativeDesktopHarness {
    desktop: NativeDesktop,
    trace: Vec<HarnessTraceEntry>,
    next_sequence: u64,
    exited: bool,
}

impl NativeDesktopHarness {
    pub fn boot(startup: NativeDesktopStartup) -> Result<Self, HarnessError> {
        let (desktop, task) = NativeDesktop::boot(startup);
        let mut harness = Self {
            desktop,
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
        let id = self.window_id(window)?;
        let messages = {
            let mut simulator = Simulator::<Message>::with_size(
                Settings::default(),
                Self::window_size(window),
                self.desktop.view(id),
            );
            simulator.click(label).map_err(|error| {
                HarnessError::new(format!("could not click {label:?} in {window}: {error}"))
            })?;
            simulator.into_messages().collect::<Vec<_>>()
        };
        self.record(window, format!("click text {label:?}"));
        self.route_messages(messages)
    }

    pub fn type_into(
        &mut self,
        window: HarnessWindow,
        placeholder: &str,
        value: &str,
    ) -> Result<(), HarnessError> {
        let id = self.window_id(window)?;
        let (status, messages) = {
            let mut simulator = Simulator::<Message>::with_size(
                Settings::default(),
                Self::window_size(window),
                self.desktop.view(id),
            );
            simulator.click(placeholder).map_err(|error| {
                HarnessError::new(format!(
                    "could not focus {placeholder:?} in {window}: {error}"
                ))
            })?;
            let status = simulator.typewrite(value);
            let messages = simulator.into_messages().collect::<Vec<_>>();
            (status, messages)
        };
        if status == event::Status::Ignored {
            return Err(HarnessError::new(format!(
                "typing into {placeholder:?} in {window} was ignored"
            )));
        }
        self.record(
            window,
            format!(
                "type into {placeholder:?} {} characters",
                value.chars().count()
            ),
        );
        self.route_messages(messages)
    }

    pub fn type_at(
        &mut self,
        window: HarnessWindow,
        point: (f32, f32),
        value: &str,
    ) -> Result<(), HarnessError> {
        let id = self.window_id(window)?;
        let (status, messages) = {
            let mut simulator = Simulator::<Message>::with_size(
                Settings::default(),
                Self::window_size(window),
                self.desktop.view(id),
            );
            simulator.point_at(IcedPoint::new(point.0, point.1));
            let _ = simulator.simulate(iced_test::simulator::click());
            let status = simulator.typewrite(value);
            let messages = simulator.into_messages().collect::<Vec<_>>();
            (status, messages)
        };
        if status == event::Status::Ignored {
            return Err(HarnessError::new(format!(
                "typing at ({}, {}) in {window} was ignored",
                point.0, point.1
            )));
        }
        self.record(
            window,
            format!(
                "type at ({}, {}) {} characters",
                point.0,
                point.1,
                value.chars().count()
            ),
        );
        self.route_messages(messages)
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
                    runtime::Action::Widget(_)
                    | runtime::Action::Image(_)
                    | runtime::Action::Reload => {}
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
