//! Complete-application interaction harness built from the production graph.

use std::{
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

use parchmint_diagnostics::DiagnosticEvent;
use parchmint_platform_api::ApplicationPaths;
use parchmint_platform_native::testing::NativeFixture;
use parchmint_ui_iced::{
    HarnessTraceEntry, HarnessWindow, NativeDesktopError, NativeDesktopHarness,
    NativeDesktopStartup,
};

use super::{
    ProductionControls, ProductionObservation, composition::assemble_interaction_harness,
    native_callbacks::NativeDesktopDriver,
};
use crate::{LaunchRequest, StartupError};

const START_TIMEOUT: Duration = Duration::from_secs(10);

/// A failure while assembling or driving the complete-application harness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractionHarnessError {
    message: String,
}

impl InteractionHarnessError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for InteractionHarnessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for InteractionHarnessError {}

impl From<StartupError> for InteractionHarnessError {
    fn from(error: StartupError) -> Self {
        Self::new(error.to_string())
    }
}

enum HarnessAction {
    HasWindow(HarnessWindow),
    ClickText(HarnessWindow, String),
    TypeInto(HarnessWindow, String, String),
    TypeAt(HarnessWindow, (f32, f32), String),
    ContainsText(HarnessWindow, String),
    ElapseAutosaveIdle,
    Close(HarnessWindow),
    ActiveEditorBody,
    Snapshot(HarnessWindow, PathBuf),
    Trace,
    Shutdown,
}

enum HarnessValue {
    Unit,
    Bool(bool),
    Text(String),
    Trace(Vec<HarnessTraceEntry>),
}

struct HarnessCommand {
    action: HarnessAction,
    reply: mpsc::SyncSender<Result<HarnessValue, String>>,
}

enum HarnessReady {
    Started,
    Failed(String),
}

struct HeadlessDesktopDriver {
    commands: Mutex<Option<mpsc::Receiver<HarnessCommand>>>,
    ready: mpsc::Sender<HarnessReady>,
}

impl NativeDesktopDriver for HeadlessDesktopDriver {
    fn run(&self, startup: NativeDesktopStartup) -> Result<(), NativeDesktopError> {
        let mut harness = match NativeDesktopHarness::boot(startup) {
            Ok(harness) => harness,
            Err(error) => {
                let message = error.to_string();
                let _ = self.ready.send(HarnessReady::Failed(message.clone()));
                return Err(NativeDesktopError::new(message));
            }
        };
        let commands = self
            .commands
            .lock()
            .map_err(|_| NativeDesktopError::new("interaction command receiver is unavailable"))?
            .take()
            .ok_or_else(|| NativeDesktopError::new("interaction driver was already started"))?;
        self.ready
            .send(HarnessReady::Started)
            .map_err(|_| NativeDesktopError::new("interaction harness start was abandoned"))?;

        while let Ok(command) = commands.recv() {
            let shutdown = matches!(command.action, HarnessAction::Shutdown);
            let result = execute_action(&mut harness, command.action);
            let _ = command.reply.send(result);
            if shutdown {
                break;
            }
        }
        Ok(())
    }
}

fn execute_action(
    harness: &mut NativeDesktopHarness,
    action: HarnessAction,
) -> Result<HarnessValue, String> {
    let result = match action {
        HarnessAction::HasWindow(window) => {
            return Ok(HarnessValue::Bool(harness.has_window(window)));
        }
        HarnessAction::ClickText(window, label) => harness.click_text(window, &label),
        HarnessAction::TypeInto(window, placeholder, value) => {
            harness.type_into(window, &placeholder, &value)
        }
        HarnessAction::TypeAt(window, point, value) => harness.type_at(window, point, &value),
        HarnessAction::ContainsText(window, text) => {
            return harness
                .contains_text(window, &text)
                .map(HarnessValue::Bool)
                .map_err(|error| error.to_string());
        }
        HarnessAction::ElapseAutosaveIdle => harness.elapse_autosave_idle(),
        HarnessAction::Close(window) => harness.close(window),
        HarnessAction::ActiveEditorBody => {
            return harness
                .active_editor_body()
                .map(HarnessValue::Text)
                .map_err(|error| error.to_string());
        }
        HarnessAction::Snapshot(window, path) => harness.snapshot(window, path),
        HarnessAction::Trace => return Ok(HarnessValue::Trace(harness.trace().to_vec())),
        HarnessAction::Shutdown => return Ok(HarnessValue::Unit),
    };
    result
        .map(|()| HarnessValue::Unit)
        .map_err(|error| error.to_string())
}

/// Controls one headless instance of the real production desktop graph.
///
/// This API is compile-time absent unless the non-default
/// `interaction-harness` feature is enabled.
pub struct DesktopInteractionHarness {
    commands: mpsc::Sender<HarnessCommand>,
    thread: Option<thread::JoinHandle<Result<(), String>>>,
    controls: ProductionControls,
}

impl DesktopInteractionHarness {
    pub fn launch(
        application_root: impl AsRef<Path>,
        request: LaunchRequest,
    ) -> Result<Self, InteractionHarnessError> {
        let application_root = application_root.as_ref();
        let configuration = application_root.join("configuration");
        let data = application_root.join("data");
        let cache = application_root.join("cache");
        for path in [&configuration, &data, &cache] {
            fs::create_dir_all(path).map_err(|error| {
                InteractionHarnessError::new(format!(
                    "could not create harness directory {}: {error}",
                    path.display()
                ))
            })?;
        }

        let fixture = NativeFixture::with_application_paths(ApplicationPaths::new(
            configuration,
            data,
            cache,
        ));
        let controls = ProductionControls::default();
        let (command_sender, command_receiver) = mpsc::channel();
        let (ready_sender, ready_receiver) = mpsc::channel();
        let driver = Arc::new(HeadlessDesktopDriver {
            commands: Mutex::new(Some(command_receiver)),
            ready: ready_sender.clone(),
        });
        let bootstrap = assemble_interaction_harness(controls.clone(), fixture.platform(), driver)?;
        let thread = thread::Builder::new()
            .name("parchmint-interaction-harness".to_owned())
            .spawn(move || {
                let result = bootstrap
                    .run(request)
                    .map(|_| ())
                    .map_err(|error| error.to_string());
                if let Err(error) = &result {
                    let _ = ready_sender.send(HarnessReady::Failed(error.clone()));
                }
                result
            })
            .map_err(|error| {
                InteractionHarnessError::new(format!(
                    "could not start interaction harness thread: {error}"
                ))
            })?;

        match ready_receiver.recv_timeout(START_TIMEOUT) {
            Ok(HarnessReady::Started) => Ok(Self {
                commands: command_sender,
                thread: Some(thread),
                controls,
            }),
            Ok(HarnessReady::Failed(error)) => {
                let _ = thread.join();
                Err(InteractionHarnessError::new(error))
            }
            Err(error) => {
                let _ = thread.join();
                Err(InteractionHarnessError::new(format!(
                    "interaction harness did not start: {error}"
                )))
            }
        }
    }

    pub fn has_window(&self, window: HarnessWindow) -> Result<bool, InteractionHarnessError> {
        self.request(HarnessAction::HasWindow(window))?.into_bool()
    }

    pub fn click_text(
        &self,
        window: HarnessWindow,
        label: impl Into<String>,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::ClickText(window, label.into()))?
            .into_unit()
    }

    pub fn type_into(
        &self,
        window: HarnessWindow,
        placeholder: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::TypeInto(
            window,
            placeholder.into(),
            value.into(),
        ))?
        .into_unit()
    }

    pub fn type_at(
        &self,
        window: HarnessWindow,
        point: (f32, f32),
        value: impl Into<String>,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::TypeAt(window, point, value.into()))?
            .into_unit()
    }

    pub fn contains_text(
        &self,
        window: HarnessWindow,
        text: impl Into<String>,
    ) -> Result<bool, InteractionHarnessError> {
        self.request(HarnessAction::ContainsText(window, text.into()))?
            .into_bool()
    }

    pub fn elapse_autosave_idle(&self) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::ElapseAutosaveIdle)?.into_unit()
    }

    pub fn close(&self, window: HarnessWindow) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::Close(window))?.into_unit()
    }

    pub fn active_editor_body(&self) -> Result<String, InteractionHarnessError> {
        self.request(HarnessAction::ActiveEditorBody)?.into_text()
    }

    pub fn snapshot(
        &self,
        window: HarnessWindow,
        path: impl Into<PathBuf>,
    ) -> Result<(), InteractionHarnessError> {
        self.request(HarnessAction::Snapshot(window, path.into()))?
            .into_unit()
    }

    pub fn trace(&self) -> Result<Vec<HarnessTraceEntry>, InteractionHarnessError> {
        self.request(HarnessAction::Trace)?.into_trace()
    }

    pub fn observations(&self) -> Vec<ProductionObservation> {
        self.controls.observations()
    }

    pub fn take_diagnostics(&self) -> Vec<DiagnosticEvent> {
        parchmint_diagnostics::take_captured_events()
    }

    pub fn shutdown(mut self) -> Result<(), InteractionHarnessError> {
        self.stop()
    }

    fn request(&self, action: HarnessAction) -> Result<HarnessValue, InteractionHarnessError> {
        let (reply_sender, reply_receiver) = mpsc::sync_channel(1);
        self.commands
            .send(HarnessCommand {
                action,
                reply: reply_sender,
            })
            .map_err(|_| InteractionHarnessError::new("interaction harness has stopped"))?;
        reply_receiver
            .recv()
            .map_err(|_| InteractionHarnessError::new("interaction harness dropped its reply"))?
            .map_err(InteractionHarnessError::new)
    }

    fn stop(&mut self) -> Result<(), InteractionHarnessError> {
        if self.thread.is_none() {
            return Ok(());
        }
        let _ = self.request(HarnessAction::Shutdown);
        let thread = self.thread.take().expect("thread presence was checked");
        thread
            .join()
            .map_err(|_| InteractionHarnessError::new("interaction harness thread panicked"))?
            .map_err(InteractionHarnessError::new)
    }
}

impl Drop for DesktopInteractionHarness {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

impl HarnessValue {
    fn into_unit(self) -> Result<(), InteractionHarnessError> {
        match self {
            Self::Unit => Ok(()),
            _ => Err(InteractionHarnessError::new(
                "interaction harness returned an unexpected response",
            )),
        }
    }

    fn into_bool(self) -> Result<bool, InteractionHarnessError> {
        match self {
            Self::Bool(value) => Ok(value),
            _ => Err(InteractionHarnessError::new(
                "interaction harness returned an unexpected response",
            )),
        }
    }

    fn into_text(self) -> Result<String, InteractionHarnessError> {
        match self {
            Self::Text(value) => Ok(value),
            _ => Err(InteractionHarnessError::new(
                "interaction harness returned an unexpected response",
            )),
        }
    }

    fn into_trace(self) -> Result<Vec<HarnessTraceEntry>, InteractionHarnessError> {
        match self {
            Self::Trace(value) => Ok(value),
            _ => Err(InteractionHarnessError::new(
                "interaction harness returned an unexpected response",
            )),
        }
    }
}
