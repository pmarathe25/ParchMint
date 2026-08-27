use std::{
    error::Error,
    fs,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
};

use parchmint_desktop::{DesktopInteractionHarness, HarnessWindow, LaunchRequest};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum WindowName {
    Launcher,
    Project,
}

impl From<WindowName> for HarnessWindow {
    fn from(window: WindowName) -> Self {
        match window {
            WindowName::Launcher => Self::Launcher,
            WindowName::Project => Self::Project,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "command", rename_all = "snake_case")]
enum Command {
    HasWindow {
        window: WindowName,
    },
    ClickText {
        window: WindowName,
        text: String,
    },
    TypeInto {
        window: WindowName,
        placeholder: String,
        value: String,
    },
    TypeAt {
        window: WindowName,
        x: f32,
        y: f32,
        value: String,
    },
    ContainsText {
        window: WindowName,
        text: String,
    },
    ElapseAutosaveIdle,
    Close {
        window: WindowName,
    },
    ActiveEditorBody,
    Snapshot {
        window: WindowName,
        path: PathBuf,
    },
    Trace,
    Observations,
    Shutdown,
}

struct Options {
    application_root: PathBuf,
    project: Option<PathBuf>,
    artifacts: PathBuf,
}

fn main() -> Result<(), Box<dyn Error>> {
    let options = options()?;
    let request = options
        .project
        .map_or_else(LaunchRequest::launcher, LaunchRequest::open);
    let mut harness = Some(DesktopInteractionHarness::launch(
        &options.application_root,
        request,
    )?);

    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let command = match serde_json::from_str::<Command>(&line) {
            Ok(command) => command,
            Err(error) => {
                write_json(
                    &mut stdout,
                    &json!({"ok": false, "error": error.to_string()}),
                )?;
                continue;
            }
        };
        let command_json = serde_json::to_value(&command)?;
        let is_shutdown = matches!(command, Command::Shutdown);
        let result = if is_shutdown {
            harness
                .take()
                .expect("harness is live until shutdown")
                .shutdown()
                .map(|()| Value::Null)
        } else {
            execute(harness.as_ref().expect("harness is live"), command)
        };
        match result {
            Ok(value) => write_json(&mut stdout, &json!({"ok": true, "value": value}))?,
            Err(error) => {
                if let Some(harness) = harness.as_ref() {
                    write_failure_bundle(
                        harness,
                        &options.artifacts,
                        &command_json,
                        &error.to_string(),
                    );
                }
                write_json(
                    &mut stdout,
                    &json!({
                        "ok": false,
                        "error": error.to_string(),
                        "artifacts": options.artifacts,
                    }),
                )?;
            }
        }
        if is_shutdown {
            break;
        }
    }
    Ok(())
}

fn execute(
    harness: &DesktopInteractionHarness,
    command: Command,
) -> Result<Value, parchmint_desktop::InteractionHarnessError> {
    match command {
        Command::HasWindow { window } => harness.has_window(window.into()).map(Value::Bool),
        Command::ClickText { window, text } => harness
            .click_text(window.into(), text)
            .map(|()| Value::Null),
        Command::TypeInto {
            window,
            placeholder,
            value,
        } => harness
            .type_into(window.into(), placeholder, value)
            .map(|()| Value::Null),
        Command::TypeAt {
            window,
            x,
            y,
            value,
        } => harness
            .type_at(window.into(), (x, y), value)
            .map(|()| Value::Null),
        Command::ContainsText { window, text } => {
            harness.contains_text(window.into(), text).map(Value::Bool)
        }
        Command::ElapseAutosaveIdle => harness.elapse_autosave_idle().map(|()| Value::Null),
        Command::Close { window } => harness.close(window.into()).map(|()| Value::Null),
        Command::ActiveEditorBody => harness.active_editor_body().map(Value::String),
        Command::Snapshot { window, path } => {
            harness.snapshot(window.into(), path).map(|()| Value::Null)
        }
        Command::Trace => harness.trace().map(|trace| {
            Value::Array(
                trace
                    .into_iter()
                    .map(|entry| {
                        json!({
                            "sequence": entry.sequence,
                            "window": entry.window.to_string(),
                            "action": entry.action,
                        })
                    })
                    .collect(),
            )
        }),
        Command::Observations => Ok(Value::Array(
            harness
                .observations()
                .into_iter()
                .map(|observation| Value::String(format!("{observation:?}")))
                .collect(),
        )),
        Command::Shutdown => unreachable!("shutdown is handled by the command loop"),
    }
}

fn write_failure_bundle(
    harness: &DesktopInteractionHarness,
    directory: &Path,
    command: &Value,
    error: &str,
) {
    if fs::create_dir_all(directory).is_err() {
        return;
    }
    let trace = harness.trace().unwrap_or_default();
    let diagnostics = harness.take_diagnostics();
    let report = json!({
        "error": error,
        "command": command,
        "trace": trace.into_iter().map(|entry| json!({
            "sequence": entry.sequence,
            "window": entry.window.to_string(),
            "action": entry.action,
        })).collect::<Vec<_>>(),
        "observations": harness.observations().into_iter()
            .map(|observation| format!("{observation:?}"))
            .collect::<Vec<_>>(),
        "diagnostics": diagnostics.into_iter().map(|event| json!({
            "sequence": event.sequence,
            "timestamp_millis": event.timestamp_millis,
            "level": format!("{:?}", event.level).to_lowercase(),
            "target": event.target,
            "message": event.message,
            "fields": event.fields,
        })).collect::<Vec<_>>(),
    });
    if let Ok(bytes) = serde_json::to_vec_pretty(&report) {
        let _ = fs::write(directory.join("failure.json"), bytes);
    }
    for window in [HarnessWindow::Project, HarnessWindow::Launcher] {
        if harness.has_window(window).unwrap_or(false) {
            let _ = harness.snapshot(window, directory.join("failure.png"));
            break;
        }
    }
}

fn write_json(output: &mut impl Write, value: &Value) -> io::Result<()> {
    serde_json::to_writer(&mut *output, value)?;
    output.write_all(b"\n")?;
    output.flush()
}

fn options() -> Result<Options, Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let mut application_root = None;
    let mut project = None;
    let mut artifacts = None;
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--app-root") => application_root = arguments.next().map(PathBuf::from),
            Some("--project") => project = arguments.next().map(PathBuf::from),
            Some("--artifacts") => artifacts = arguments.next().map(PathBuf::from),
            _ => return Err(format!("unknown or non-UTF-8 argument: {argument:?}").into()),
        }
    }
    let application_root = application_root.ok_or("--app-root PATH is required")?;
    let artifacts = artifacts.unwrap_or_else(|| application_root.join("ui-failure"));
    Ok(Options {
        application_root,
        project,
        artifacts,
    })
}
