//! Native Iced event-loop integration for the desktop executable.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use iced::{
    Element, Length, Subscription, Task, Theme,
    widget::{button, column, container, row, text, text_input},
    window,
};
use parchmint_platform_api::WindowCapability;
use parchmint_preferences::ResolvedAppearance;

use crate::{LauncherState, RibbonDestination, Shell};

/// One project window that was registered before the native loop started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeProjectWindow {
    pub project: PathBuf,
    pub window: WindowCapability,
}

/// The result of routing a launcher project-open request through the desktop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeProjectOpenResult {
    Opened(NativeProjectWindow),
    Focused(WindowCapability),
    Locked,
}

/// Desktop lifecycle callbacks invoked by native window interactions.
pub trait NativeDesktopCallbacks: Send + Sync {
    fn open_project(&self, project: PathBuf) -> Result<NativeProjectOpenResult, String>;
    fn close_project(&self, project: PathBuf) -> Result<(), String>;

    /// Records the platform capability when this driver creates its native
    /// project window.
    fn project_window_created(&self, _window: WindowCapability) {}

    /// Retires the platform capability after this driver removes its native
    /// project window.
    fn project_window_destroyed(&self, _window: WindowCapability) {}
}

/// ParchMint-owned values supplied to the native Iced driver.
pub struct NativeDesktopStartup {
    pub appearance: ResolvedAppearance,
    pub projects: Vec<NativeProjectWindow>,
    pub locked_project: Option<PathBuf>,
    pub callbacks: Arc<dyn NativeDesktopCallbacks>,
}

/// A failure while creating or running the native desktop event loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeDesktopError {
    message: String,
}

impl NativeDesktopError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for NativeDesktopError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for NativeDesktopError {}

/// Runs the native launcher and project windows until the user closes them.
pub fn run_native_desktop(startup: NativeDesktopStartup) -> Result<(), NativeDesktopError> {
    let startup = Mutex::new(Some(startup));
    iced::daemon(
        move || {
            let startup = startup
                .lock()
                .expect("native desktop startup mutex poisoned")
                .take()
                .expect("native desktop may only boot once");
            NativeDesktop::boot(startup)
        },
        NativeDesktop::update,
        NativeDesktop::view,
    )
    .title(NativeDesktop::title)
    .theme(NativeDesktop::theme)
    .subscription(NativeDesktop::subscription)
    .run()
    .map_err(|error| NativeDesktopError::new(error.to_string()))
}

#[derive(Debug, Clone)]
enum Message {
    WindowOpened,
    CloseRequested(window::Id),
    ProjectPathChanged(String),
    OpenProject,
    ProjectOpenFinished {
        project: PathBuf,
        result: Result<NativeProjectOpenResult, String>,
    },
    SelectDestination {
        window: window::Id,
        destination: RibbonDestination,
    },
    ProjectCloseFinished {
        window: window::Id,
        result: Result<(), String>,
    },
}

struct NativeDesktop {
    appearance: ResolvedAppearance,
    launcher: LauncherState,
    windows: BTreeMap<window::Id, NativeWindow>,
    project_windows: BTreeMap<WindowCapability, window::Id>,
    closing_windows: BTreeSet<window::Id>,
    opening_project: bool,
    project_path: String,
    status: Option<String>,
    callbacks: Arc<dyn NativeDesktopCallbacks>,
}

enum NativeWindow {
    Launcher,
    Project { project: PathBuf, shell: Box<Shell> },
}

impl NativeDesktop {
    fn boot(startup: NativeDesktopStartup) -> (Self, Task<Message>) {
        let mut desktop = Self {
            appearance: startup.appearance,
            launcher: LauncherState::default(),
            windows: BTreeMap::new(),
            project_windows: BTreeMap::new(),
            closing_windows: BTreeSet::new(),
            opening_project: false,
            project_path: String::new(),
            status: startup
                .locked_project
                .map(|path| format!("Project is already open: {}", path.display())),
            callbacks: startup.callbacks,
        };
        let mut tasks = vec![desktop.open_launcher_window()];
        tasks.extend(
            startup
                .projects
                .into_iter()
                .map(|project| desktop.open_project_window(project)),
        );
        (desktop, Task::batch(tasks))
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::WindowOpened => Task::none(),
            Message::CloseRequested(id) => self.close_window(id),
            Message::ProjectPathChanged(path) => {
                self.project_path = path;
                Task::none()
            }
            Message::OpenProject => self.route_project_open(),
            Message::ProjectOpenFinished { project, result } => {
                self.opening_project = false;
                self.finish_project_open(project, result)
            }
            Message::SelectDestination {
                window,
                destination,
            } => {
                if let Some(NativeWindow::Project { shell, .. }) = self.windows.get_mut(&window) {
                    shell.select_destination(destination);
                }
                Task::none()
            }
            Message::ProjectCloseFinished { window, result } => match result {
                Ok(()) => self.finish_close(window),
                Err(error) => {
                    self.closing_windows.remove(&window);
                    self.status = Some(error);
                    Task::none()
                }
            },
        }
    }

    fn view(&self, id: window::Id) -> Element<'_, Message> {
        match self.windows.get(&id) {
            Some(NativeWindow::Launcher) => self.launcher_view(),
            Some(NativeWindow::Project { project, shell }) => {
                Self::project_view(id, project, shell, self.status.as_deref())
            }
            None => container(text("Opening ParchMint…"))
                .center(Length::Fill)
                .into(),
        }
    }

    fn title(&self, id: window::Id) -> String {
        match self.windows.get(&id) {
            Some(NativeWindow::Launcher) | None => "ParchMint".to_owned(),
            Some(NativeWindow::Project { project, .. }) => project
                .file_name()
                .and_then(|name| name.to_str())
                .map_or_else(
                    || "ParchMint".to_owned(),
                    |name| format!("{name} — ParchMint"),
                ),
        }
    }

    fn theme(&self, _id: window::Id) -> Theme {
        match self.appearance {
            ResolvedAppearance::Light => Theme::Light,
            ResolvedAppearance::Dark => Theme::Dark,
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        window::close_requests().map(Message::CloseRequested)
    }

    fn launcher_view(&self) -> Element<'_, Message> {
        let recent_projects = if self.launcher.recent_projects().is_empty() {
            text("No recent projects yet.").size(14)
        } else {
            text("Recent projects are available.").size(14)
        };
        let mut content = column![
            text("ParchMint").size(36),
            text("Write and organize a project in ordinary files.").size(16),
            recent_projects,
            text_input("Project directory", &self.project_path)
                .on_input(Message::ProjectPathChanged)
                .on_submit(Message::OpenProject)
                .padding(10),
            if self.opening_project {
                button("Opening Project…")
            } else {
                button("Open Project").on_press(Message::OpenProject)
            },
        ]
        .spacing(18)
        .max_width(720);
        if let Some(status) = &self.status {
            content = content.push(text(status).size(14));
        }
        container(content)
            .padding(40)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    }

    fn project_view<'a>(
        id: window::Id,
        project: &'a Path,
        shell: &'a Shell,
        status: Option<&'a str>,
    ) -> Element<'a, Message> {
        let navigation = row![
            destination_button(id, "Editor", RibbonDestination::Editor),
            destination_button(id, "Cards", RibbonDestination::Cards),
            destination_button(id, "History", RibbonDestination::History),
            destination_button(id, "Recently Deleted", RibbonDestination::RecentlyDeleted),
            destination_button(id, "Export", RibbonDestination::Export),
            destination_button(id, "Settings", RibbonDestination::Settings),
        ]
        .spacing(8);
        let mut content = column![
            navigation,
            text(format!("{:?}", shell.destination())).size(24),
            text(project.display().to_string()).size(14),
            text("The project session is open and connected to the production service graph.")
                .size(16),
        ]
        .spacing(20);
        if let Some(status) = status {
            content = content.push(text(status).size(14));
        }
        container(content)
            .padding(24)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn open_launcher_window(&mut self) -> Task<Message> {
        let (id, task) = window::open(window_settings((900.0, 620.0)));
        self.windows.insert(id, NativeWindow::Launcher);
        task.map(|_| Message::WindowOpened)
    }

    fn open_project_window(&mut self, project: NativeProjectWindow) -> Task<Message> {
        let (id, task) = window::open(window_settings((1280.0, 720.0)));
        self.project_windows.insert(project.window, id);
        self.callbacks.project_window_created(project.window);
        self.windows.insert(
            id,
            NativeWindow::Project {
                project: project.project,
                shell: Box::new(Shell::new(project.window)),
            },
        );
        task.map(|_| Message::WindowOpened)
    }

    fn route_project_open(&mut self) -> Task<Message> {
        if self.opening_project {
            return Task::none();
        }
        if self.project_path.trim().is_empty() {
            self.status = Some("Enter a project directory to open.".to_owned());
            return Task::none();
        }
        let project = PathBuf::from(self.project_path.trim());
        self.opening_project = true;
        let callbacks = Arc::clone(&self.callbacks);
        Task::perform(
            async move {
                let result = callbacks.open_project(project.clone());
                (project, result)
            },
            |(project, result)| Message::ProjectOpenFinished { project, result },
        )
    }

    fn finish_project_open(
        &mut self,
        project: PathBuf,
        result: Result<NativeProjectOpenResult, String>,
    ) -> Task<Message> {
        match result {
            Ok(NativeProjectOpenResult::Opened(window)) => {
                self.status = None;
                self.open_project_window(window)
            }
            Ok(NativeProjectOpenResult::Focused(capability)) => {
                self.status = None;
                self.project_windows
                    .get(&capability)
                    .copied()
                    .map_or_else(Task::none, window::gain_focus)
            }
            Ok(NativeProjectOpenResult::Locked) => {
                self.status = Some(format!("Project is already open: {}", project.display()));
                Task::none()
            }
            Err(error) => {
                self.status = Some(error);
                Task::none()
            }
        }
    }

    fn close_window(&mut self, id: window::Id) -> Task<Message> {
        let Some(window) = self.windows.get(&id) else {
            return Task::none();
        };
        let NativeWindow::Project { project, .. } = window else {
            return self.finish_close(id);
        };
        if !self.closing_windows.insert(id) {
            return Task::none();
        }
        let project = project.clone();
        let callbacks = Arc::clone(&self.callbacks);
        Task::perform(
            async move { callbacks.close_project(project) },
            move |result| Message::ProjectCloseFinished { window: id, result },
        )
    }

    fn finish_close(&mut self, id: window::Id) -> Task<Message> {
        self.closing_windows.remove(&id);
        let removed = self.windows.remove(&id);
        if let Some(NativeWindow::Project { shell, .. }) = removed {
            self.project_windows.remove(&shell.window());
            self.callbacks.project_window_destroyed(shell.window());
        }
        if self.windows.is_empty() {
            Task::batch([window::close(id), iced::exit()])
        } else {
            window::close(id)
        }
    }
}

fn destination_button(
    window: window::Id,
    label: &'static str,
    destination: RibbonDestination,
) -> iced::widget::Button<'static, Message> {
    button(label).on_press(Message::SelectDestination {
        window,
        destination,
    })
}

fn window_settings(size: (f32, f32)) -> window::Settings {
    window::Settings {
        size: iced::Size::new(size.0, size.1),
        min_size: Some(iced::Size::new(720.0, 480.0)),
        position: window::Position::Centered,
        exit_on_close_request: false,
        ..window::Settings::default()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    struct RecordingCallbacks {
        open_result: Mutex<Option<NativeProjectOpenResult>>,
        closed: Mutex<Vec<PathBuf>>,
        created: Mutex<Vec<WindowCapability>>,
        destroyed: Mutex<Vec<WindowCapability>>,
    }

    impl RecordingCallbacks {
        fn opening(result: NativeProjectOpenResult) -> Self {
            Self {
                open_result: Mutex::new(Some(result)),
                closed: Mutex::new(Vec::new()),
                created: Mutex::new(Vec::new()),
                destroyed: Mutex::new(Vec::new()),
            }
        }
    }

    impl NativeDesktopCallbacks for RecordingCallbacks {
        fn open_project(&self, _project: PathBuf) -> Result<NativeProjectOpenResult, String> {
            self.open_result
                .lock()
                .expect("open result mutex poisoned")
                .take()
                .ok_or_else(|| "no open result configured".to_owned())
        }

        fn close_project(&self, project: PathBuf) -> Result<(), String> {
            self.closed
                .lock()
                .expect("closed projects mutex poisoned")
                .push(project);
            Ok(())
        }

        fn project_window_created(&self, window: WindowCapability) {
            self.created
                .lock()
                .expect("created windows mutex poisoned")
                .push(window);
        }

        fn project_window_destroyed(&self, window: WindowCapability) {
            self.destroyed
                .lock()
                .expect("destroyed windows mutex poisoned")
                .push(window);
        }
    }

    #[test]
    fn boot_plans_a_real_launcher_and_each_registered_project_window() {
        let project = NativeProjectWindow {
            project: PathBuf::from("/tmp/novel.parchmint"),
            window: WindowCapability::new(4, 1),
        };
        let callbacks = Arc::new(RecordingCallbacks::opening(NativeProjectOpenResult::Locked));

        let (desktop, _open_tasks) = NativeDesktop::boot(NativeDesktopStartup {
            appearance: ResolvedAppearance::Dark,
            projects: vec![project.clone()],
            locked_project: None,
            callbacks: callbacks.clone(),
        });

        assert_eq!(desktop.windows.len(), 2);
        assert_eq!(desktop.project_windows.len(), 1);
        assert!(desktop.project_windows.contains_key(&project.window));
        assert_eq!(desktop.appearance, ResolvedAppearance::Dark);
        assert_eq!(
            callbacks
                .created
                .lock()
                .expect("created windows mutex poisoned")
                .as_slice(),
            [project.window]
        );
    }

    #[test]
    fn launcher_project_open_uses_the_desktop_callback_and_adds_the_native_window() {
        let project = NativeProjectWindow {
            project: PathBuf::from("/tmp/routed.parchmint"),
            window: WindowCapability::new(7, 2),
        };
        let callbacks = Arc::new(RecordingCallbacks::opening(
            NativeProjectOpenResult::Opened(project.clone()),
        ));
        let (mut desktop, _open_tasks) = NativeDesktop::boot(NativeDesktopStartup {
            appearance: ResolvedAppearance::Light,
            projects: Vec::new(),
            locked_project: None,
            callbacks: callbacks.clone(),
        });
        desktop.project_path = project.project.display().to_string();

        let _pending_open = desktop.route_project_open();
        assert!(desktop.opening_project);
        assert_eq!(desktop.windows.len(), 1);
        let _open_task = desktop.update(Message::ProjectOpenFinished {
            project: project.project.clone(),
            result: Ok(NativeProjectOpenResult::Opened(project.clone())),
        });

        assert!(desktop.project_windows.contains_key(&project.window));
        assert_eq!(desktop.windows.len(), 2);
        assert!(desktop.status.is_none());
        assert!(!desktop.opening_project);
    }

    #[test]
    fn final_save_completion_controls_when_a_project_window_is_removed() {
        let project = NativeProjectWindow {
            project: PathBuf::from("/tmp/closing.parchmint"),
            window: WindowCapability::new(8, 3),
        };
        let callbacks = Arc::new(RecordingCallbacks::opening(NativeProjectOpenResult::Locked));
        let (mut desktop, _open_tasks) = NativeDesktop::boot(NativeDesktopStartup {
            appearance: ResolvedAppearance::Light,
            projects: vec![project.clone()],
            locked_project: None,
            callbacks: callbacks.clone(),
        });
        let native_window = desktop.project_windows[&project.window];
        desktop.closing_windows.insert(native_window);

        let _failed_close = desktop.update(Message::ProjectCloseFinished {
            window: native_window,
            result: Err("save failed".to_owned()),
        });
        assert!(desktop.windows.contains_key(&native_window));
        assert!(!desktop.closing_windows.contains(&native_window));
        assert_eq!(desktop.status.as_deref(), Some("save failed"));

        desktop.closing_windows.insert(native_window);
        let _successful_close = desktop.update(Message::ProjectCloseFinished {
            window: native_window,
            result: Ok(()),
        });
        assert!(!desktop.windows.contains_key(&native_window));
        assert!(!desktop.project_windows.contains_key(&project.window));
        assert_eq!(
            callbacks
                .destroyed
                .lock()
                .expect("destroyed windows mutex poisoned")
                .as_slice(),
            [project.window]
        );
    }
}
