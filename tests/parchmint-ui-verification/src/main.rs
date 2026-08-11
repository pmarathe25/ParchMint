use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, ExitCode},
};

use parchmint_ui_iced::{VisualAppearance, VisualTarget, capture_visual};
use parchmint_ui_verification::{
    CatalogCaseReport, compare, decode_png, diff_image, encode_png, passes_acceptance,
    write_catalog_case, write_catalog_index, write_report,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const USAGE: &str = "Usage:\n  parchmint-ui-verify list\n  parchmint-ui-verify capture --target <fixture-id> --appearance <light|dark> --output-stem <PATH>\n  parchmint-ui-verify verify-catalog --references <DIR> --output <DIR>\n  parchmint-ui-verify native-capture --desktop <PARCHMINT-BIN> --target <fixture-id> --appearance <light|dark> --output <ABSOLUTE-PNG> [--project <PROJECT>] [--scale <1|2>] [--logical-width <PIXELS> --logical-height <PIXELS>] [--require-size <WIDTHxHEIGHT>] [--reference <PNG> --diff <PNG> --report <JSON>]\n  parchmint-ui-verify compare --reference <PNG> --actual <PNG> --diff <PNG> --report <JSON>";

#[derive(Debug)]
enum Command {
    List,
    Capture(CaptureArguments),
    VerifyCatalog(CatalogArguments),
    CatalogCase(CatalogCaseArguments),
    NativeCapture(NativeCaptureArguments),
    Compare(CompareArguments),
}

#[derive(Debug)]
struct CatalogArguments {
    references: PathBuf,
    output: PathBuf,
}

#[derive(Debug)]
struct CatalogCaseArguments {
    target: VisualTarget,
    appearance: VisualAppearance,
    reference: PathBuf,
    output: PathBuf,
    output_stem: PathBuf,
}

#[derive(Debug, Deserialize)]
struct ReferenceSet {
    schema: String,
    logical_width: u32,
    logical_height: u32,
    export_scale: u32,
    physical_width: u32,
    physical_height: u32,
    screens: Vec<ReferenceScreen>,
}

#[derive(Debug, Deserialize)]
struct ReferenceScreen {
    fixture_id: String,
    light: String,
    dark: String,
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

#[derive(Debug)]
struct NativeCaptureArguments {
    desktop: PathBuf,
    target: VisualTarget,
    appearance: VisualAppearance,
    output: PathBuf,
    project: Option<PathBuf>,
    scale: Option<u32>,
    logical_width: Option<u32>,
    logical_height: Option<u32>,
    required_size: Option<(u32, u32)>,
    comparison: Option<NativeComparisonOutputs>,
}

#[derive(Debug)]
struct NativeComparisonOutputs {
    reference: PathBuf,
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
        Command::List => list_targets(),
        Command::Capture(arguments) => capture(arguments),
        Command::VerifyCatalog(arguments) => verify_catalog(arguments),
        Command::CatalogCase(arguments) => capture_catalog_case(arguments),
        Command::NativeCapture(arguments) => capture_native(arguments),
        Command::Compare(arguments) => compare_images(arguments),
    }
}

fn list_targets() -> Result<ExitCode, String> {
    for target in VisualTarget::ALL {
        for appearance in VisualAppearance::ALL {
            println!(
                "{}\t{}\t{}",
                target.name(),
                appearance.name(),
                target.reference_id(appearance)
            );
        }
    }
    Ok(ExitCode::SUCCESS)
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

fn verify_catalog(arguments: CatalogArguments) -> Result<ExitCode, String> {
    let plan = validated_catalog_plan(&arguments.references)?;
    fs::create_dir_all(arguments.output.join(".staging"))
        .map_err(|error| format!("could not create catalog staging directory: {error}"))?;
    let mut cases = Vec::new();
    for (target, appearance, reference) in plan {
        let output_stem = arguments
            .output
            .join(".staging")
            .join(appearance.name())
            .join(target.reference_id(appearance));
        if let Some(parent) = output_stem.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "could not create catalog capture directory {}: {error}",
                    parent.display()
                )
            })?;
        }
        cases.push(run_catalog_case(
            target,
            appearance,
            reference,
            &arguments.output,
            &output_stem,
        )?);
    }
    let index = write_catalog_index(&arguments.output, &cases)
        .map_err(|error| format!("could not write catalog index: {error}"))?;
    let accepted = cases.iter().filter(|case| case.acceptance_passed).count();
    println!(
        "catalog index: {} ({accepted}/{} accepted)",
        index.display(),
        cases.len()
    );
    Ok(if accepted == cases.len() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

/// Runs the whole capture/compare/write case in a fresh process. This bounds
/// full-resolution renderer and PNG allocations for the 20-board catalog.
fn run_catalog_case(
    target: VisualTarget,
    appearance: VisualAppearance,
    reference: PathBuf,
    output: &Path,
    output_stem: &Path,
) -> Result<CatalogCaseReport, String> {
    let output = ProcessCommand::new(
        env::current_exe()
            .map_err(|error| format!("could not locate verifier executable: {error}"))?,
    )
    .args([
        "catalog-case",
        "--target",
        target.name(),
        "--appearance",
        appearance.name(),
        "--reference",
    ])
    .arg(reference)
    .args(["--output"])
    .arg(output)
    .args(["--output-stem"])
    .arg(output_stem)
    .output()
    .map_err(|error| {
        format!(
            "could not start catalog case for {} {}: {error}",
            target.name(),
            appearance.name()
        )
    })?;
    if !output.status.success() {
        return Err(format!(
            "catalog case for {} {} exited with {:?}: {}",
            target.name(),
            appearance.name(),
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let stdout = String::from_utf8(output.stdout).map_err(|error| {
        format!(
            "catalog case for {} {} returned non-UTF-8 output: {error}",
            target.name(),
            appearance.name()
        )
    })?;
    let report = stdout.trim();
    if report.is_empty() {
        return Err(format!(
            "catalog case for {} {} returned no report",
            target.name(),
            appearance.name()
        ));
    }
    serde_json::from_str(report).map_err(|error| {
        format!(
            "catalog case for {} {} returned invalid JSON: {error}",
            target.name(),
            appearance.name()
        )
    })
}

fn capture_catalog_case(arguments: CatalogCaseArguments) -> Result<ExitCode, String> {
    let capture_path = capture_catalog_render(
        arguments.target,
        arguments.appearance,
        &arguments.output_stem,
    )?;
    let actual = decode_png(&capture_path).map_err(|error| {
        format!(
            "could not decode catalog actual {}: {error}",
            capture_path.display()
        )
    })?;
    if (actual.width(), actual.height()) != (2880, 1800) {
        return Err(format!(
            "catalog target {} {} rendered {}x{}; expected 2880x1800",
            arguments.target.name(),
            arguments.appearance.name(),
            actual.width(),
            actual.height()
        ));
    }
    let report = write_catalog_case(
        &arguments.output,
        arguments.target.reference_id(arguments.appearance),
        arguments.appearance.name(),
        arguments.reference,
        &actual,
    )
    .map_err(|error| format!("could not write catalog case: {error}"))?;
    fs::remove_file(&capture_path).map_err(|error| {
        format!(
            "could not remove catalog staging output {}: {error}",
            capture_path.display()
        )
    })?;
    println!(
        "{}",
        serde_json::to_string(&report).expect("catalog report is serializable")
    );
    Ok(ExitCode::SUCCESS)
}

fn capture_catalog_render(
    target: VisualTarget,
    appearance: VisualAppearance,
    output_stem: &Path,
) -> Result<PathBuf, String> {
    let output = ProcessCommand::new(
        env::current_exe()
            .map_err(|error| format!("could not locate verifier executable: {error}"))?,
    )
    .args([
        "capture",
        "--target",
        target.name(),
        "--appearance",
        appearance.name(),
        "--output-stem",
    ])
    .arg(output_stem)
    .output()
    .map_err(|error| {
        format!(
            "could not start catalog render for {} {}: {error}",
            target.name(),
            appearance.name()
        )
    })?;
    if !output.status.success() {
        return Err(format!(
            "catalog render for {} {} exited with {:?}: {}",
            target.name(),
            appearance.name(),
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let stdout = String::from_utf8(output.stdout).map_err(|error| {
        format!(
            "catalog render for {} {} returned non-UTF-8 output: {error}",
            target.name(),
            appearance.name()
        )
    })?;
    let path = stdout.trim();
    if path.is_empty() {
        return Err(format!(
            "catalog render for {} {} returned no output path",
            target.name(),
            appearance.name()
        ));
    }
    Ok(PathBuf::from(path))
}

#[cfg(test)]
fn catalog_plan(references: &Path) -> Vec<(VisualTarget, VisualAppearance, PathBuf)> {
    VisualTarget::ALL
        .into_iter()
        .flat_map(|target| {
            VisualAppearance::ALL.into_iter().map(move |appearance| {
                let reference = references
                    .join(appearance.name())
                    .join(format!("{}.png", target.reference_id(appearance)));
                (target, appearance, reference)
            })
        })
        .collect()
}

fn validated_catalog_plan(
    references: &Path,
) -> Result<Vec<(VisualTarget, VisualAppearance, PathBuf)>, String> {
    let manifest_path = references.join("reference-set.toml");
    let manifest: ReferenceSet = toml::from_str(
        &fs::read_to_string(&manifest_path)
            .map_err(|error| format!("could not read {}: {error}", manifest_path.display()))?,
    )
    .map_err(|error| format!("could not parse {}: {error}", manifest_path.display()))?;
    if manifest.schema != "parchmint.penpot-reference-set/v1"
        || (
            manifest.logical_width,
            manifest.logical_height,
            manifest.export_scale,
        ) != (1440, 900, 2)
        || (manifest.physical_width, manifest.physical_height) != (2880, 1800)
    {
        return Err(format!(
            "reference manifest {} does not describe the required 1440x900 @2x catalog",
            manifest_path.display()
        ));
    }
    if manifest.screens.len() != VisualTarget::ALL.len() {
        return Err(format!(
            "reference manifest {} has {} screens; expected {}",
            manifest_path.display(),
            manifest.screens.len(),
            VisualTarget::ALL.len()
        ));
    }
    let checksums = validated_checksums(references)?;
    let mut screens = BTreeMap::new();
    for screen in manifest.screens {
        if screens.insert(screen.fixture_id.clone(), screen).is_some() {
            return Err("reference manifest contains a duplicate fixture_id".to_owned());
        }
    }
    let mut plan = Vec::with_capacity(VisualTarget::ALL.len() * VisualAppearance::ALL.len());
    let mut expected_paths = BTreeSet::new();
    for target in VisualTarget::ALL {
        let screen = screens
            .remove(target.name())
            .ok_or_else(|| format!("reference manifest is missing {}", target.name()))?;
        for appearance in VisualAppearance::ALL {
            let relative = match appearance {
                VisualAppearance::Light => &screen.light,
                VisualAppearance::Dark => &screen.dark,
            };
            let expected = format!(
                "{}/{}.png",
                appearance.name(),
                target.reference_id(appearance)
            );
            if relative != &expected {
                return Err(format!(
                    "reference manifest maps {} {} to {}; expected {expected}",
                    target.name(),
                    appearance.name(),
                    relative
                ));
            }
            let path = safe_reference_path(references, relative)?;
            if !path.is_file() {
                return Err(format!("reference image is missing: {}", path.display()));
            }
            if !checksums.contains_key(relative) {
                return Err(format!("reference image is not checksummed: {relative}"));
            }
            expected_paths.insert(relative.clone());
            plan.push((target, appearance, path));
        }
    }
    if !screens.is_empty()
        || checksums.keys().collect::<BTreeSet<_>>() != expected_paths.iter().collect()
    {
        return Err("reference manifest/checksum catalog has unmapped entries".to_owned());
    }
    Ok(plan)
}

fn validated_checksums(references: &Path) -> Result<BTreeMap<String, String>, String> {
    let checksum_path = references.join("SHA256SUMS");
    let content = fs::read_to_string(&checksum_path)
        .map_err(|error| format!("could not read {}: {error}", checksum_path.display()))?;
    let mut checksums = BTreeMap::new();
    for line in content.lines() {
        let (expected, relative) = line
            .split_once("  ")
            .ok_or_else(|| format!("invalid checksum line in {}", checksum_path.display()))?;
        if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!("invalid SHA-256 digest for {relative}"));
        }
        let path = safe_reference_path(references, relative)?;
        let digest = Sha256::digest(
            fs::read(&path)
                .map_err(|error| format!("could not read {}: {error}", path.display()))?,
        );
        let mut actual = String::with_capacity(digest.len() * 2);
        for byte in digest {
            write!(&mut actual, "{byte:02x}").expect("writing to String cannot fail");
        }
        if actual != expected {
            return Err(format!("reference checksum mismatch for {relative}"));
        }
        if checksums
            .insert(relative.to_owned(), expected.to_owned())
            .is_some()
        {
            return Err(format!("duplicate checksum entry for {relative}"));
        }
    }
    Ok(checksums)
}

fn safe_reference_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative = Path::new(relative);
    if relative.is_absolute()
        || !relative
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
    {
        return Err(format!("unsafe reference path {}", relative.display()));
    }
    Ok(root.join(relative))
}

fn capture_native(arguments: NativeCaptureArguments) -> Result<ExitCode, String> {
    if arguments.output.exists() {
        return Err(format!(
            "native capture output already exists: {}",
            arguments.output.display()
        ));
    }
    let target = native_target(arguments.target);
    if target != "launcher" && arguments.project.is_none() {
        return Err(format!(
            "--project is required for the {} native target\n{USAGE}",
            arguments.target.name()
        ));
    }
    let mut command = ProcessCommand::new(&arguments.desktop);
    command.args([
        "capture",
        "--target",
        target,
        "--appearance",
        arguments.appearance.name(),
        "--output",
    ]);
    command.arg(&arguments.output);
    if let Some(project) = &arguments.project {
        command.args(["--project"]);
        command.arg(project);
    }
    if let Some(scale) = arguments.scale {
        command.args(["--scale", &scale.to_string()]);
    }
    if let Some(width) = arguments.logical_width {
        command.args(["--logical-width", &width.to_string()]);
    }
    if let Some(height) = arguments.logical_height {
        command.args(["--logical-height", &height.to_string()]);
    }
    if let Some((width, height)) = arguments.required_size {
        command.args(["--require-size", &format!("{width}x{height}")]);
    }
    let status = command.status().map_err(|error| {
        format!(
            "could not run production desktop {}: {error}",
            arguments.desktop.display()
        )
    })?;
    if !status.success() {
        return Err(format!("production native capture exited with {status}"));
    }
    if !arguments.output.is_file() {
        return Err(format!(
            "production native capture reported success but did not write {}",
            arguments.output.display()
        ));
    }
    println!("{}", arguments.output.display());
    match arguments.comparison {
        Some(outputs) => compare_images(CompareArguments {
            reference: outputs.reference,
            actual: arguments.output,
            diff: outputs.diff,
            report: outputs.report,
        }),
        None => Ok(ExitCode::SUCCESS),
    }
}

fn native_target(target: VisualTarget) -> &'static str {
    match target {
        VisualTarget::Launcher => "launcher",
        VisualTarget::EditorSingle | VisualTarget::EditorDual | VisualTarget::ErrorRecovery => {
            "editor"
        }
        VisualTarget::Cards => "cards",
        VisualTarget::GlobalSearch => "global-search",
        VisualTarget::History => "history",
        VisualTarget::SettingsAppearance => "settings",
        VisualTarget::Export => "export",
        VisualTarget::RecentlyDeleted => "recently-deleted",
    }
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
    Ok(if passes_acceptance(&report) {
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
        Some("verify-catalog") => parse_catalog_arguments(arguments).map(Command::VerifyCatalog),
        Some("catalog-case") => parse_catalog_case_arguments(arguments).map(Command::CatalogCase),
        Some("native-capture") => {
            parse_native_capture_arguments(arguments).map(Command::NativeCapture)
        }
        Some("compare") => parse_compare_arguments(arguments).map(Command::Compare),
        Some("list") => {
            if arguments.next().is_some() {
                Err(format!("list accepts no arguments\n{USAGE}"))
            } else {
                Ok(Command::List)
            }
        }
        Some("--help" | "-h") => Err(USAGE.to_owned()),
        Some(command) => Err(format!("unknown command {command}\n{USAGE}")),
        None => Err(USAGE.to_owned()),
    }
}

fn parse_catalog_arguments(
    arguments: impl Iterator<Item = std::ffi::OsString>,
) -> Result<CatalogArguments, String> {
    let mut references = None;
    let mut output = None;
    let mut arguments = arguments;
    while let Some(argument) = arguments.next() {
        let value = next_value(&mut arguments, &argument)?;
        match argument.to_str() {
            Some("--references") => {
                set_once(&mut references, PathBuf::from(value), "--references")?
            }
            Some("--output") => set_once(&mut output, PathBuf::from(value), "--output")?,
            _ => return Err(unknown_argument(&argument)),
        }
    }
    Ok(CatalogArguments {
        references: required(references, "--references")?,
        output: required(output, "--output")?,
    })
}

fn parse_catalog_case_arguments(
    arguments: impl Iterator<Item = std::ffi::OsString>,
) -> Result<CatalogCaseArguments, String> {
    let mut target = None;
    let mut appearance = None;
    let mut reference = None;
    let mut output = None;
    let mut output_stem = None;
    let mut arguments = arguments;
    while let Some(argument) = arguments.next() {
        let value = next_value(&mut arguments, &argument)?;
        match argument.to_str() {
            Some("--target") => set_once(&mut target, parse_target(value)?, "--target")?,
            Some("--appearance") => {
                set_once(&mut appearance, parse_appearance(value)?, "--appearance")?
            }
            Some("--reference") => set_once(&mut reference, PathBuf::from(value), "--reference")?,
            Some("--output") => set_once(&mut output, PathBuf::from(value), "--output")?,
            Some("--output-stem") => {
                set_once(&mut output_stem, PathBuf::from(value), "--output-stem")?
            }
            _ => return Err(unknown_argument(&argument)),
        }
    }
    Ok(CatalogCaseArguments {
        target: required(target, "--target")?,
        appearance: required(appearance, "--appearance")?,
        reference: required(reference, "--reference")?,
        output: required(output, "--output")?,
        output_stem: required(output_stem, "--output-stem")?,
    })
}

fn parse_native_capture_arguments(
    arguments: impl Iterator<Item = std::ffi::OsString>,
) -> Result<NativeCaptureArguments, String> {
    let mut desktop = None;
    let mut target = None;
    let mut appearance = None;
    let mut output = None;
    let mut project = None;
    let mut scale = None;
    let mut logical_width = None;
    let mut logical_height = None;
    let mut required_size = None;
    let mut reference = None;
    let mut diff = None;
    let mut report = None;
    let mut arguments = arguments;

    while let Some(argument) = arguments.next() {
        let value = next_value(&mut arguments, &argument)?;
        match argument.to_str() {
            Some("--desktop") => set_once(&mut desktop, PathBuf::from(value), "--desktop")?,
            Some("--target") => set_once(&mut target, parse_target(value)?, "--target")?,
            Some("--appearance") => {
                set_once(&mut appearance, parse_appearance(value)?, "--appearance")?
            }
            Some("--output") => set_once(&mut output, PathBuf::from(value), "--output")?,
            Some("--project") => set_once(&mut project, PathBuf::from(value), "--project")?,
            Some("--scale") => {
                set_once(&mut scale, parse_positive_u32(value, "--scale")?, "--scale")?
            }
            Some("--logical-width") => set_once(
                &mut logical_width,
                parse_positive_u32(value, "--logical-width")?,
                "--logical-width",
            )?,
            Some("--logical-height") => set_once(
                &mut logical_height,
                parse_positive_u32(value, "--logical-height")?,
                "--logical-height",
            )?,
            Some("--require-size") => set_once(
                &mut required_size,
                parse_size(value, "--require-size")?,
                "--require-size",
            )?,
            Some("--reference") => set_once(&mut reference, PathBuf::from(value), "--reference")?,
            Some("--diff") => set_once(&mut diff, PathBuf::from(value), "--diff")?,
            Some("--report") => set_once(&mut report, PathBuf::from(value), "--report")?,
            _ => return Err(unknown_argument(&argument)),
        }
    }
    let comparison = match (reference, diff, report) {
        (None, None, None) => None,
        (Some(reference), Some(diff), Some(report)) => Some(NativeComparisonOutputs {
            reference,
            diff,
            report,
        }),
        _ => {
            return Err(format!(
                "--reference, --diff, and --report must be supplied together\n{USAGE}"
            ));
        }
    };
    if logical_width.is_some() != logical_height.is_some() {
        return Err(format!(
            "--logical-width and --logical-height must be supplied together\n{USAGE}"
        ));
    }
    Ok(NativeCaptureArguments {
        desktop: required(desktop, "--desktop")?,
        target: required(target, "--target")?,
        appearance: required(appearance, "--appearance")?,
        output: required(output, "--output")?,
        project,
        scale,
        logical_width,
        logical_height,
        required_size,
        comparison,
    })
}

fn parse_positive_u32(value: std::ffi::OsString, flag: &str) -> Result<u32, String> {
    value
        .to_str()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("{flag} must be a positive integer\n{USAGE}"))
}

fn parse_size(value: std::ffi::OsString, flag: &str) -> Result<(u32, u32), String> {
    let Some(value) = value.to_str() else {
        return Err(format!("{flag} must use WIDTHxHEIGHT\n{USAGE}"));
    };
    let Some((width, height)) = value.split_once('x') else {
        return Err(format!("{flag} must use WIDTHxHEIGHT\n{USAGE}"));
    };
    Ok((
        parse_positive_u32(width.into(), flag)?,
        parse_positive_u32(height.into(), flag)?,
    ))
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
    VisualTarget::ALL
        .into_iter()
        .find(|target| value.to_str() == Some(target.name()))
        .ok_or_else(|| {
            format!(
                "invalid --target {}; use `list` for fixture IDs\n{USAGE}",
                value.to_string_lossy()
            )
        })
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

    #[test]
    fn list_parser_accepts_no_arguments_and_rejects_extra_arguments() {
        assert!(matches!(
            parse_arguments(["list"].into_iter().map(Into::into)),
            Ok(Command::List)
        ));
        assert!(parse_arguments(["list", "extra"].into_iter().map(Into::into)).is_err());
    }

    #[test]
    fn catalog_parser_and_plan_cover_every_penpot_target() {
        assert!(
            parse_arguments(
                ["verify-catalog", "--references", "references"]
                    .into_iter()
                    .map(Into::into)
            )
            .is_err()
        );
        assert!(matches!(
            parse_arguments(
                [
                    "verify-catalog",
                    "--references",
                    "references",
                    "--output",
                    "artifacts",
                ]
                .into_iter()
                .map(Into::into)
            ),
            Ok(Command::VerifyCatalog(_))
        ));

        let plan = catalog_plan(Path::new("references"));
        assert_eq!(plan.len(), 20);
        let unique_references = plan
            .iter()
            .map(|(_, _, reference)| reference)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique_references.len(), 20);
        assert!(plan.iter().all(|(_, appearance, reference)| {
            reference
                .parent()
                .is_some_and(|parent| parent.ends_with(appearance.name()))
        }));
    }

    #[test]
    fn checked_in_catalog_manifest_and_checksums_match_the_capture_mapping() {
        let references = Path::new(env!("CARGO_MANIFEST_DIR")).join("references/penpot");
        let plan = validated_catalog_plan(&references).unwrap();
        assert_eq!(plan.len(), 20);
        assert!(plan.iter().all(|(_, _, reference)| reference.is_file()));
    }
}
