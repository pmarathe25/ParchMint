use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use parchmint_contracts::generated::CliOutputV1;
use parchmint_project_fs::{NativeProjectFileSystem, ProjectFileSystem, UntrustedProjectPath};
use parchmint_test_support::ScopedProject;
use serde_json::Value;

const SUCCESS: i32 = 0;
const USAGE_ERROR: i32 = 2;
const UNSAFE_INPUT: i32 = 3;
const LOCKED: i32 = 4;
const INVALID_PROJECT: i32 = 5;
const CANCELLED: i32 = 6;

struct Fixture {
    project: ScopedProject,
}

impl Fixture {
    fn new() -> Self {
        Self {
            project: ScopedProject::from_fixture("canonical/minimal-project")
                .expect("canonical fixture should be available"),
        }
    }

    fn path(&self) -> &Path {
        self.project.root.as_path()
    }
}

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_parchmint-core-cli"))
        .args(args)
        .output()
        .expect("core CLI should start")
}

fn status(output: &Output) -> i32 {
    output
        .status
        .code()
        .expect("CLI should exit with a stable numeric status")
}

fn machine(output: &Output) -> CliOutputV1 {
    assert!(
        output.stderr.is_empty(),
        "machine diagnostics belong in JSON"
    );
    serde_json::from_slice(&output.stdout)
        .expect("machine output should match the stable CLI output contract")
}

fn assert_machine_schema(output: &Output, expected_ok: bool) -> CliOutputV1 {
    let value = machine(output);
    assert_eq!(value.schema, "parchmint.cli-output/v1");
    assert_eq!(value.ok, expected_ok);
    value
}

fn assert_no_path(value: &CliOutputV1, path: &Path) {
    assert_no_text(value, &path.to_string_lossy());
}

fn assert_no_text(value: &CliOutputV1, text: &str) {
    assert!(
        !value.message.as_deref().unwrap_or_default().contains(text),
        "sensitive text leaked in machine output"
    );
    if let Some(data) = &value.data {
        assert_no_value_text(data, text);
    }
}

fn assert_no_value_text(value: &Value, text: &str) {
    match value {
        Value::String(value) => assert!(
            !value.contains(text),
            "sensitive text leaked in machine output: {value}"
        ),
        Value::Array(values) => values
            .iter()
            .for_each(|value| assert_no_value_text(value, text)),
        Value::Object(values) => values
            .values()
            .for_each(|value| assert_no_value_text(value, text)),
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

#[test]
fn create_open_validate_and_inspect_have_stable_machine_contracts() {
    let parent = Fixture::new();
    let project = parent.path().join("created-project");

    let created = cli(&["--machine", "create", project.to_str().unwrap()]);
    assert_eq!(status(&created), SUCCESS);
    assert_machine_schema(&created, true);
    assert!(
        project.is_dir(),
        "create must cross the real filesystem boundary"
    );

    for command in ["open", "validate", "inspect"] {
        let output = cli(&["--machine", command, project.to_str().unwrap()]);
        assert_eq!(status(&output), SUCCESS, "{command} should succeed");
        assert_machine_schema(&output, true);
    }
}

#[test]
fn migrate_is_explicit_and_machine_output_is_deterministic() {
    let fixture = Fixture::new();
    fs::write(
        fixture.path().join("project.toml"),
        "# migrate this project\n[project]\n",
    )
    .expect("fixture should be mutable");
    let first = cli(&["--machine", "migrate", fixture.path().to_str().unwrap()]);
    let second = cli(&["--machine", "migrate", fixture.path().to_str().unwrap()]);

    assert_eq!(status(&first), SUCCESS);
    assert_eq!(status(&second), SUCCESS);
    assert_eq!(machine(&first), machine(&second));
    assert_machine_schema(&first, true);
    assert_eq!(
        fs::read_to_string(fixture.path().join("project.toml"))
            .expect("migrate should rewrite the canonical manifest"),
        "[project]\n"
    );
}

#[test]
fn machine_mode_is_stable_regardless_of_flag_placement() {
    let fixture = Fixture::new();
    let output = cli(&["open", fixture.path().to_str().unwrap(), "--machine"]);

    assert_eq!(status(&output), SUCCESS);
    assert_machine_schema(&output, true);
}

#[test]
fn command_save_recover_history_search_rebuild_and_export_use_real_project_state() {
    let fixture = Fixture::new();
    let path = fixture.path().to_str().unwrap();
    let export = fixture.path().join("out.html");

    for args in [
        vec!["--machine", "command", path, "noop"],
        vec!["--machine", "save", path],
        vec!["--machine", "recover", path],
        vec!["--machine", "history", path],
        vec!["--machine", "search", path, "draft"],
        vec!["--machine", "rebuild", path],
        vec!["--machine", "export", path, export.to_str().unwrap()],
    ] {
        let output = cli(&args);
        assert_eq!(status(&output), SUCCESS, "command should succeed: {args:?}");
        assert_machine_schema(&output, true);
    }
    assert!(
        export.is_file(),
        "export must write through the native service boundary"
    );
}

#[test]
fn usage_invalid_project_and_unsafe_path_have_distinct_exit_codes() {
    let fixture = Fixture::new();
    let missing = fixture.path().join("missing-project");
    let unsafe_path = fixture.path().join("../outside-secret/../../etc/passwd");

    let usage = cli(&["--machine", "open"]);
    assert_eq!(status(&usage), USAGE_ERROR);
    assert_machine_schema(&usage, false);

    let invalid = cli(&["--machine", "open", missing.to_str().unwrap()]);
    assert_eq!(status(&invalid), INVALID_PROJECT);
    let invalid_json = assert_machine_schema(&invalid, false);
    assert_no_path(&invalid_json, &missing);

    let unsafe_output = cli(&["--machine", "open", unsafe_path.to_str().unwrap()]);
    assert_eq!(status(&unsafe_output), UNSAFE_INPUT);
    assert_no_path(&machine(&unsafe_output), &unsafe_path);
}

#[test]
fn human_diagnostics_redact_project_paths_and_user_prose() {
    let fixture = Fixture::new();
    fs::write(
        fixture.path().join("project.toml"),
        "private prose must not be echoed",
    )
    .expect("fixture should be mutable");

    let output = cli(&["open", fixture.path().to_str().unwrap()]);
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    assert_eq!(status(&output), INVALID_PROJECT);
    assert!(!diagnostics.contains(fixture.path().to_string_lossy().as_ref()));
    assert!(!diagnostics.contains("private prose must not be echoed"));
}

#[test]
fn a_locked_project_maps_to_locked_without_leaking_the_lock_path() {
    let fixture = Fixture::new();
    let path = fixture.path().to_path_buf();
    let (_root, lease) = NativeProjectFileSystem::new()
        .acquire(UntrustedProjectPath::new(path.clone()))
        .expect("test should hold the native project lock");

    let output = cli(&["--machine", "save", path.to_str().unwrap()]);
    assert_eq!(status(&output), LOCKED);
    let value = assert_machine_schema(&output, false);
    assert_no_path(&value, &path);
    drop(lease);
}

#[test]
fn cancellation_is_reported_without_claiming_success_or_partial_machine_output() {
    let fixture = Fixture::new();
    let output = cli(&[
        "--machine",
        "--cancel",
        "rebuild",
        fixture.path().to_str().unwrap(),
    ]);
    assert_eq!(status(&output), CANCELLED);
    let value = assert_machine_schema(&output, false);
    assert!(!value.ok);
}
