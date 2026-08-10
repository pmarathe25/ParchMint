use std::{
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use parchmint_ui_iced::{VisualAppearance, VisualTarget, capture_visual};
use parchmint_ui_verification::{compare, decode_png, diff_image, encode_png, write_report};

const USAGE: &str = "Usage:\n  parchmint-ui-verify capture --target <launcher|project> --appearance <light|dark> --output-stem <PATH>\n  parchmint-ui-verify compare --reference <PNG> --actual <PNG> --diff <PNG> --report <JSON>";

#[derive(Debug)]
enum Command {
    Capture(CaptureArguments),
    Compare(CompareArguments),
}

#[derive(Debug)]
struct CaptureArguments {
    target: VisualTarget,
    appearance: VisualAppearance,
    output_stem: PathBuf,
}

#[derive(Debug)]
struct CompareArguments {
    reference: PathBuf,
    actual: PathBuf,
    diff: PathBuf,
    report: PathBuf,
}

fn main() -> ExitCode {
    match run() {
        Ok(exit_code) => exit_code,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode, String> {
    match parse_arguments(env::args_os().skip(1))? {
        Command::Capture(arguments) => capture(arguments),
        Command::Compare(arguments) => compare_images(arguments),
    }
}

fn capture(arguments: CaptureArguments) -> Result<ExitCode, String> {
    let capture = capture_visual(
        arguments.target,
        arguments.appearance,
        arguments.output_stem,
    )
    .map_err(|error| format!("could not capture visual target: {error}"))?;
    println!("{}", capture.output_path.display());
    Ok(ExitCode::SUCCESS)
}

fn compare_images(arguments: CompareArguments) -> Result<ExitCode, String> {
    reject_input_overwrites(&arguments)?;

    let reference = decode_png(&arguments.reference).map_err(|error| {
        format!(
            "could not decode reference PNG {}: {error}",
            arguments.reference.display()
        )
    })?;
    let actual = decode_png(&arguments.actual).map_err(|error| {
        format!(
            "could not decode actual PNG {}: {error}",
            arguments.actual.display()
        )
    })?;
    let report = compare(&reference, &actual);
    let diff = diff_image(&reference, &actual)
        .map_err(|error| format!("could not build diff: {error}"))?;

    encode_png(&arguments.diff, &diff).map_err(|error| {
        format!(
            "could not write diff PNG {}: {error}",
            arguments.diff.display()
        )
    })?;
    write_report(&arguments.report, &report).map_err(|error| {
        format!(
            "could not write JSON report {}: {error}",
            arguments.report.display()
        )
    })?;
    Ok(if report.matches {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn parse_arguments(
    mut arguments: impl Iterator<Item = std::ffi::OsString>,
) -> Result<Command, String> {
    match arguments
        .next()
        .as_deref()
        .and_then(std::ffi::OsStr::to_str)
    {
        Some("capture") => parse_capture_arguments(arguments).map(Command::Capture),
        Some("compare") => parse_compare_arguments(arguments).map(Command::Compare),
        Some("--help" | "-h") => Err(USAGE.to_owned()),
        Some(command) => Err(format!("unknown command {command}\n{USAGE}")),
        None => Err(USAGE.to_owned()),
    }
}

fn parse_capture_arguments(
    arguments: impl Iterator<Item = std::ffi::OsString>,
) -> Result<CaptureArguments, String> {
    let mut target = None;
    let mut appearance = None;
    let mut output_stem = None;
    let mut arguments = arguments;

    while let Some(argument) = arguments.next() {
        let value = next_value(&mut arguments, &argument)?;
        match argument.to_str() {
            Some("--target") => set_once(&mut target, parse_target(value)?, "--target")?,
            Some("--appearance") => {
                set_once(&mut appearance, parse_appearance(value)?, "--appearance")?
            }
            Some("--output-stem") => {
                set_once(&mut output_stem, PathBuf::from(value), "--output-stem")?
            }
            _ => return Err(unknown_argument(&argument)),
        }
    }

    Ok(CaptureArguments {
        target: required(target, "--target")?,
        appearance: required(appearance, "--appearance")?,
        output_stem: required(output_stem, "--output-stem")?,
    })
}

fn parse_compare_arguments(
    arguments: impl Iterator<Item = std::ffi::OsString>,
) -> Result<CompareArguments, String> {
    let mut reference = None;
    let mut actual = None;
    let mut diff = None;
    let mut report = None;
    let mut arguments = arguments;

    while let Some(argument) = arguments.next() {
        let value = next_value(&mut arguments, &argument)?;
        match argument.to_str() {
            Some("--reference") => set_once(&mut reference, PathBuf::from(value), "--reference")?,
            Some("--actual") => set_once(&mut actual, PathBuf::from(value), "--actual")?,
            Some("--diff") => set_once(&mut diff, PathBuf::from(value), "--diff")?,
            Some("--report") => set_once(&mut report, PathBuf::from(value), "--report")?,
            _ => return Err(unknown_argument(&argument)),
        }
    }

    Ok(CompareArguments {
        reference: required(reference, "--reference")?,
        actual: required(actual, "--actual")?,
        diff: required(diff, "--diff")?,
        report: required(report, "--report")?,
    })
}

fn next_value(
    arguments: &mut impl Iterator<Item = std::ffi::OsString>,
    flag: &std::ffi::OsString,
) -> Result<std::ffi::OsString, String> {
    arguments
        .next()
        .ok_or_else(|| format!("missing value for {}\n{USAGE}", flag.to_string_lossy()))
}

fn parse_target(value: std::ffi::OsString) -> Result<VisualTarget, String> {
    match value.to_str() {
        Some("launcher") => Ok(VisualTarget::Launcher),
        Some("project") => Ok(VisualTarget::Project),
        _ => Err(format!(
            "invalid --target {}; expected launcher or project\n{USAGE}",
            value.to_string_lossy()
        )),
    }
}

fn parse_appearance(value: std::ffi::OsString) -> Result<VisualAppearance, String> {
    match value.to_str() {
        Some("light") => Ok(VisualAppearance::Light),
        Some("dark") => Ok(VisualAppearance::Dark),
        _ => Err(format!(
            "invalid --appearance {}; expected light or dark\n{USAGE}",
            value.to_string_lossy()
        )),
    }
}

fn set_once<T>(slot: &mut Option<T>, value: T, flag: &str) -> Result<(), String> {
    if slot.replace(value).is_some() {
        return Err(format!("{flag} may be provided only once\n{USAGE}"));
    }
    Ok(())
}

fn required<T>(value: Option<T>, flag: &str) -> Result<T, String> {
    value.ok_or_else(|| format!("missing required {flag}\n{USAGE}"))
}

fn unknown_argument(argument: &std::ffi::OsString) -> String {
    format!("unknown argument {}\n{USAGE}", argument.to_string_lossy())
}

fn reject_input_overwrites(arguments: &CompareArguments) -> Result<(), String> {
    for output in [&arguments.diff, &arguments.report] {
        for input in [&arguments.reference, &arguments.actual] {
            if same_existing_path(output, input) {
                return Err(format!(
                    "refusing to overwrite input image {} with output {}",
                    input.display(),
                    output.display()
                ));
            }
        }
    }
    if arguments.diff == arguments.report {
        return Err("--diff and --report must use different paths".to_owned());
    }
    Ok(())
}

fn same_existing_path(left: &Path, right: &Path) -> bool {
    left == right
        || matches!(
            (fs::canonicalize(left), fs::canonicalize(right)),
            (Ok(left), Ok(right)) if left == right
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_parser_requires_every_named_value() {
        assert!(
            parse_arguments(
                ["capture", "--target", "launcher"]
                    .into_iter()
                    .map(Into::into)
            )
            .is_err()
        );
    }

    #[test]
    fn compare_parser_rejects_duplicate_flags() {
        let result = parse_arguments(
            [
                "compare",
                "--reference",
                "a.png",
                "--reference",
                "b.png",
                "--actual",
                "c.png",
                "--diff",
                "d.png",
                "--report",
                "e.json",
            ]
            .into_iter()
            .map(Into::into),
        );
        assert!(result.unwrap_err().contains("only once"));
    }
}
