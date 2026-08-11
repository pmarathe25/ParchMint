use std::{env, path::PathBuf};

use parchmint_desktop::{DesktopBootstrap, ExitCode, LaunchRequest};
use parchmint_preferences::ResolvedAppearance;
use parchmint_ui_iced::{NativeCaptureRequest, NativeCaptureTarget, RibbonDestination};

const CAPTURE_USAGE: &str = "Usage:\n  parchmint [PROJECT]\n  parchmint capture --target <launcher|editor|cards|history|recently-deleted|export|settings|global-search> --appearance <light|dark> --output <ABSOLUTE-PNG> [--project <PROJECT>] [--scale <1|2>] [--logical-width <PIXELS> --logical-height <PIXELS>] [--require-size <WIDTHxHEIGHT>] [--keep-open]";

enum ProcessRequest {
    Run(LaunchRequest),
    Capture {
        launch: LaunchRequest,
        capture: NativeCaptureRequest,
    },
}

fn main() -> std::process::ExitCode {
    let exit = parse_process_request(env::args_os())
        .and_then(|request| match request {
            ProcessRequest::Run(launch) => DesktopBootstrap::production()?.run(launch),
            ProcessRequest::Capture { launch, capture } => {
                DesktopBootstrap::production()?.run_native_capture(launch, capture)
            }
        })
        .unwrap_or_else(|error| error.report_and_exit());
    process_exit_code(exit)
}

fn parse_process_request(
    arguments: impl IntoIterator<Item = impl Into<std::ffi::OsString>>,
) -> Result<ProcessRequest, parchmint_desktop::StartupError> {
    let mut arguments = arguments.into_iter().map(Into::into);
    let _executable = arguments.next();
    match arguments
        .next()
        .as_deref()
        .and_then(std::ffi::OsStr::to_str)
    {
        Some("capture") => parse_capture(arguments),
        Some(argument) => Ok(ProcessRequest::Run(LaunchRequest::open(argument))),
        None => Ok(ProcessRequest::Run(LaunchRequest::launcher())),
    }
}

fn parse_capture(
    mut arguments: impl Iterator<Item = std::ffi::OsString>,
) -> Result<ProcessRequest, parchmint_desktop::StartupError> {
    let mut target = None;
    let mut appearance = None;
    let mut output = None;
    let mut project = None;
    let mut scale = None;
    let mut logical_width = None;
    let mut logical_height = None;
    let mut required_size = None;
    let mut keep_open = false;

    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--keep-open") if !keep_open => keep_open = true,
            Some("--target") => set_once(
                &mut target,
                parse_target(next_value(&mut arguments, &argument)?)?,
                "--target",
            )?,
            Some("--appearance") => set_once(
                &mut appearance,
                parse_appearance(next_value(&mut arguments, &argument)?)?,
                "--appearance",
            )?,
            Some("--output") => set_once(
                &mut output,
                PathBuf::from(next_value(&mut arguments, &argument)?),
                "--output",
            )?,
            Some("--project") => set_once(
                &mut project,
                PathBuf::from(next_value(&mut arguments, &argument)?),
                "--project",
            )?,
            Some("--scale") => set_once(
                &mut scale,
                parse_positive_u32(next_value(&mut arguments, &argument)?, "--scale")?,
                "--scale",
            )?,
            Some("--logical-width") => set_once(
                &mut logical_width,
                parse_positive_u32(next_value(&mut arguments, &argument)?, "--logical-width")?,
                "--logical-width",
            )?,
            Some("--logical-height") => set_once(
                &mut logical_height,
                parse_positive_u32(next_value(&mut arguments, &argument)?, "--logical-height")?,
                "--logical-height",
            )?,
            Some("--require-size") => set_once(
                &mut required_size,
                parse_size(next_value(&mut arguments, &argument)?, "--require-size")?,
                "--require-size",
            )?,
            _ => {
                return Err(invalid_capture_request(format!(
                    "unknown capture argument {}",
                    argument.to_string_lossy()
                )));
            }
        }
    }

    let target = required(target, "--target")?;
    let appearance = required(appearance, "--appearance")?;
    let output = required(output, "--output")?;
    let project = match target {
        NativeCaptureTarget::Launcher => None,
        NativeCaptureTarget::Project(_) => {
            Some(required(project, "--project for a project target")?)
        }
    };
    let mut capture = NativeCaptureRequest::new(target, appearance, output)
        .map_err(|error| invalid_capture_request(error.to_string()))?;
    match (logical_width, logical_height) {
        (None, None) => {}
        (Some(width), Some(height)) => capture
            .configure_viewport(
                (width, height),
                scale.unwrap_or(NativeCaptureRequest::SCALE),
            )
            .map_err(|error| invalid_capture_request(error.to_string()))?,
        _ => {
            return Err(invalid_capture_request(
                "--logical-width and --logical-height must be supplied together".to_owned(),
            ));
        }
    }
    if logical_width.is_none() {
        capture
            .configure_viewport(
                NativeCaptureRequest::LOGICAL_SIZE,
                scale.unwrap_or(NativeCaptureRequest::SCALE),
            )
            .map_err(|error| invalid_capture_request(error.to_string()))?;
    }
    capture
        .require_size(required_size)
        .map_err(|error| invalid_capture_request(error.to_string()))?;
    capture.exit_after_capture = !keep_open;
    Ok(ProcessRequest::Capture {
        launch: project.map_or_else(LaunchRequest::launcher, LaunchRequest::open),
        capture,
    })
}

fn parse_target(
    value: std::ffi::OsString,
) -> Result<NativeCaptureTarget, parchmint_desktop::StartupError> {
    let target = match value.to_str() {
        Some("launcher") => NativeCaptureTarget::Launcher,
        Some("editor") => NativeCaptureTarget::Project(RibbonDestination::Editor),
        Some("cards") => NativeCaptureTarget::Project(RibbonDestination::Cards),
        Some("history") => NativeCaptureTarget::Project(RibbonDestination::History),
        Some("recently-deleted") => {
            NativeCaptureTarget::Project(RibbonDestination::RecentlyDeleted)
        }
        Some("export") => NativeCaptureTarget::Project(RibbonDestination::Export),
        Some("settings") => NativeCaptureTarget::Project(RibbonDestination::Settings),
        Some("global-search") => NativeCaptureTarget::Project(RibbonDestination::GlobalSearch),
        _ => {
            return Err(invalid_capture_request(format!(
                "invalid capture target {}",
                value.to_string_lossy()
            )));
        }
    };
    Ok(target)
}

fn parse_appearance(
    value: std::ffi::OsString,
) -> Result<ResolvedAppearance, parchmint_desktop::StartupError> {
    match value.to_str() {
        Some("light") => Ok(ResolvedAppearance::Light),
        Some("dark") => Ok(ResolvedAppearance::Dark),
        _ => Err(invalid_capture_request(format!(
            "invalid appearance {}",
            value.to_string_lossy()
        ))),
    }
}

fn parse_positive_u32(
    value: std::ffi::OsString,
    flag: &str,
) -> Result<u32, parchmint_desktop::StartupError> {
    value
        .to_str()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| invalid_capture_request(format!("{flag} must be a positive integer")))
}

fn parse_size(
    value: std::ffi::OsString,
    flag: &str,
) -> Result<(u32, u32), parchmint_desktop::StartupError> {
    let Some(value) = value.to_str() else {
        return Err(invalid_capture_request(format!(
            "{flag} must use WIDTHxHEIGHT"
        )));
    };
    let Some((width, height)) = value.split_once('x') else {
        return Err(invalid_capture_request(format!(
            "{flag} must use WIDTHxHEIGHT"
        )));
    };
    let width = parse_positive_u32(width.into(), flag)?;
    let height = parse_positive_u32(height.into(), flag)?;
    Ok((width, height))
}

fn next_value(
    arguments: &mut impl Iterator<Item = std::ffi::OsString>,
    flag: &std::ffi::OsString,
) -> Result<std::ffi::OsString, parchmint_desktop::StartupError> {
    arguments.next().ok_or_else(|| {
        invalid_capture_request(format!("missing value for {}", flag.to_string_lossy()))
    })
}

fn set_once<T>(
    slot: &mut Option<T>,
    value: T,
    flag: &str,
) -> Result<(), parchmint_desktop::StartupError> {
    if slot.replace(value).is_some() {
        return Err(invalid_capture_request(format!(
            "{flag} may be provided only once"
        )));
    }
    Ok(())
}

fn required<T>(value: Option<T>, flag: &str) -> Result<T, parchmint_desktop::StartupError> {
    value.ok_or_else(|| invalid_capture_request(format!("missing required {flag}")))
}

fn invalid_capture_request(reason: String) -> parchmint_desktop::StartupError {
    parchmint_desktop::StartupError::Production {
        component: "native capture arguments",
        reason: format!("{reason}\n{CAPTURE_USAGE}"),
    }
}

fn process_exit_code(exit: ExitCode) -> std::process::ExitCode {
    match u8::try_from(exit.value()) {
        Ok(value) => std::process::ExitCode::from(value),
        Err(_) => std::process::ExitCode::FAILURE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_parser_requires_a_project_for_a_project_destination() {
        let output = std::env::temp_dir().join("parchmint-desktop-capture-parser.png");
        let _ = std::fs::remove_file(&output);
        let result = parse_process_request([
            "parchmint".into(),
            "capture".into(),
            "--target".into(),
            "editor".into(),
            "--appearance".into(),
            "light".into(),
            "--output".into(),
            output.into_os_string(),
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn capture_parser_builds_an_exiting_launcher_capture() {
        let output = std::env::temp_dir().join("parchmint-desktop-launcher-capture-parser.png");
        let _ = std::fs::remove_file(&output);
        let request = parse_process_request([
            "parchmint".into(),
            "capture".into(),
            "--target".into(),
            "launcher".into(),
            "--appearance".into(),
            "dark".into(),
            "--output".into(),
            output.clone().into_os_string(),
        ])
        .expect("valid launcher capture arguments");
        let ProcessRequest::Capture { launch, capture } = request else {
            panic!("capture command must parse as a capture request");
        };
        assert_eq!(launch, LaunchRequest::launcher());
        assert_eq!(capture.target, NativeCaptureTarget::Launcher);
        assert_eq!(capture.appearance, ResolvedAppearance::Dark);
        assert_eq!(capture.output_path, output);
        assert!(capture.exit_after_capture);
    }

    #[test]
    fn capture_parser_accepts_scale_and_logical_viewport() {
        let output = std::env::temp_dir().join("parchmint-desktop-scaled-capture-parser.png");
        let _ = std::fs::remove_file(&output);
        let request = parse_process_request([
            "parchmint".into(),
            "capture".into(),
            "--target".into(),
            "launcher".into(),
            "--appearance".into(),
            "light".into(),
            "--output".into(),
            output.into_os_string(),
            "--scale".into(),
            "1".into(),
            "--logical-width".into(),
            "960".into(),
            "--logical-height".into(),
            "540".into(),
            "--require-size".into(),
            "960x540".into(),
        ])
        .expect("valid scaled capture");
        let ProcessRequest::Capture { capture, .. } = request else {
            panic!("capture command must parse as a capture request");
        };
        assert_eq!(capture.logical_size(), (960, 540));
        assert_eq!(capture.scale(), 1);
        assert_eq!(capture.requested_physical_size(), (960, 540));
    }
}
