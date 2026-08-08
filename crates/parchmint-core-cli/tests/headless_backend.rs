//! End-to-end backend composition through the native CLI, without a GUI runtime.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use parchmint_contracts::generated::CliOutputV1;
use parchmint_test_support::ScopedProject;
use serde_json::Value;

const SUCCESS: i32 = 0;

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
        .expect("headless CLI should start without a GUI runtime")
}

fn machine(output: &Output) -> CliOutputV1 {
    assert!(
        output.stderr.is_empty(),
        "machine diagnostics belong in JSON"
    );
    let value: CliOutputV1 = serde_json::from_slice(&output.stdout)
        .expect("CLI output should use the stable machine contract");
    assert_eq!(value.schema, "parchmint.cli-output/v1");
    value
}

fn ok(args: &[&str]) -> CliOutputV1 {
    let output = cli(args);
    assert_eq!(output.status.code(), Some(SUCCESS), "CLI failed: {args:?}");
    let result = machine(&output);
    assert!(result.ok, "CLI reported failure: {args:?}");
    result
}

fn checkpoint_id(output: &CliOutputV1) -> String {
    fn find(value: &Value) -> Option<String> {
        match value {
            Value::Object(fields) => fields
                .get("checkpoint_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| fields.values().find_map(find)),
            Value::Array(values) => values.iter().find_map(find),
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => None,
        }
    }

    find(
        output
            .data
            .as_ref()
            .expect("checkpoint should return an ID"),
    )
    .expect("checkpoint machine output should contain checkpoint_id")
}

fn numeric_data(output: &CliOutputV1, field: &str) -> u64 {
    output
        .data
        .as_ref()
        .and_then(|data| data.get(field))
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("machine output should contain numeric {field}"))
}

fn assert_redacted(output: &CliOutputV1, sensitive: &str) {
    assert!(
        !serde_json::to_string(output).unwrap().contains(sensitive),
        "machine output must not expose project paths or authored prose"
    );
}

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("interchange destination should be created");
    for entry in fs::read_dir(source).expect("project should be readable") {
        let entry = entry.expect("project entry should be readable");
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry
            .file_type()
            .expect("project entry type should be readable")
            .is_dir()
        {
            copy_tree(&source_path, &destination_path);
        } else {
            fs::copy(source_path, destination_path).expect("project resource should be copied");
        }
    }
}

fn canonical_snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn visit(root: &Path, current: &Path, files: &mut Vec<(PathBuf, Vec<u8>)>) {
        for entry in fs::read_dir(current).expect("project should remain readable") {
            let entry = entry.expect("project entry should be readable");
            let path = entry.path();
            if entry
                .file_type()
                .expect("project entry type should be readable")
                .is_dir()
            {
                visit(root, &path, files);
            } else {
                files.push((
                    path.strip_prefix(root)
                        .expect("resource should be project-relative")
                        .to_path_buf(),
                    fs::read(path).expect("canonical resource should be readable"),
                ));
            }
        }
    }

    let mut files = Vec::new();
    for relative in [
        ".parchmint/format-version",
        "project.toml",
        "styles.css",
        "dictionary.txt",
        "deletions.json",
    ] {
        let path = root.join(relative);
        if path.is_file() {
            files.push((PathBuf::from(relative), fs::read(path).unwrap()));
        }
    }
    for relative in ["manuscript", "research", "annotations"] {
        let directory = root.join(relative);
        if directory.is_dir() {
            visit(root, &directory, &mut files);
        }
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    files
}

#[test]
fn headless_cli_composes_create_edit_recovery_history_search_and_interchange() {
    let fixture = Fixture::new();
    let project = fixture.path().join("headless-project");
    let project_text = project.to_string_lossy().into_owned();
    let project_arg = project_text.as_str();

    ok(&["--machine", "create", project_arg]);
    ok(&["--machine", "open", project_arg]);

    // The edit remains in recovery until the explicit save completes.
    let edit = ok(&[
        "--machine",
        "edit",
        project_arg,
        "manuscript/chapter-1.html",
        "hello from the headless backend",
    ]);
    assert_redacted(&edit, project_arg);
    assert_redacted(&edit, "hello from the headless backend");
    let chapter = project.join("manuscript/chapter-1.html");
    assert!(
        !chapter.exists(),
        "an unsaved edit must remain outside canonical resources"
    );
    ok(&["--machine", "terminate", project_arg]);
    assert!(
        !chapter.exists(),
        "forced termination must not promote recovery bytes to canonical state"
    );
    ok(&["--machine", "recover", project_arg]);
    assert!(
        fs::read_to_string(&chapter)
            .unwrap()
            .contains("headless backend"),
        "recovery should replay the unsaved edit"
    );
    ok(&["--machine", "save", project_arg]);

    let checkpoint = ok(&["--machine", "checkpoint", project_arg, "after-edit"]);
    let checkpoint = checkpoint_id(&checkpoint);
    let checkpoint_bytes = canonical_snapshot(&project);
    ok(&[
        "--machine",
        "edit",
        project_arg,
        "manuscript/chapter-1.html",
        "changed after the checkpoint",
    ]);
    ok(&["--machine", "terminate", project_arg]);
    ok(&["--machine", "recover", project_arg]);
    ok(&["--machine", "save", project_arg]);
    assert!(
        fs::read_to_string(&chapter)
            .unwrap()
            .contains("changed after the checkpoint")
    );
    let obsolete = project.join("research/obsolete.html");
    fs::create_dir_all(obsolete.parent().unwrap()).unwrap();
    fs::write(&obsolete, b"<p data-block-id=\"obsolete\">obsolete</p>\n").unwrap();
    let history_before_restore = numeric_data(
        &ok(&["--machine", "history", project_arg]),
        "checkpoint_count",
    );
    ok(&["--machine", "restore", project_arg, checkpoint.as_str()]);
    assert_eq!(canonical_snapshot(&project), checkpoint_bytes);
    assert!(
        !obsolete.exists(),
        "restore must delete post-checkpoint files"
    );
    assert_eq!(
        numeric_data(
            &ok(&["--machine", "history", project_arg]),
            "checkpoint_count"
        ),
        history_before_restore + 1,
        "restore must append History without rewinding it"
    );

    ok(&["--machine", "index", project_arg]);
    let query = ok(&["--machine", "query", project_arg, "headless"]);
    assert_eq!(numeric_data(&query, "hit_count"), 1);
    ok(&["--machine", "rebuild", project_arg]);
    ok(&["--machine", "close", project_arg]);
    ok(&["--machine", "open", project_arg]);

    let canonical_before_corruption = canonical_snapshot(&project);
    let derived = project.join(".parchmint/cache/search.sqlite");
    fs::write(&derived, b"corrupt derived state")
        .expect("the test must be able to corrupt disposable search state");
    ok(&["--machine", "open", project_arg]);
    assert_eq!(canonical_snapshot(&project), canonical_before_corruption);
    ok(&["--machine", "rebuild", project_arg]);
    assert_eq!(canonical_snapshot(&project), canonical_before_corruption);
    let rebuilt_query = ok(&["--machine", "query", project_arg, "headless"]);
    assert_eq!(numeric_data(&rebuilt_query, "hit_count"), 1);

    // A copied project is the interchange contract for Windows, macOS, and Linux.
    // The same canonical bytes and History must reopen without a GUI or installed Git.
    let imported = fixture.path().join("interchanged-project");
    copy_tree(&project, &imported);
    let imported_text = imported.to_string_lossy().into_owned();
    ok(&["--machine", "close", project_arg]);
    ok(&["--machine", "open", imported_text.as_str()]);
    assert_eq!(canonical_snapshot(&imported), canonical_snapshot(&project));
    let imported_history = ok(&["--machine", "history", imported_text.as_str()]);
    assert!(numeric_data(&imported_history, "checkpoint_count") >= 3);
    let imported_query = ok(&["--machine", "query", imported_text.as_str(), "headless"]);
    assert_eq!(numeric_data(&imported_query, "hit_count"), 1);
}

#[cfg(target_os = "linux")]
#[test]
fn deletion_failure_is_retried_before_the_project_reopens() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new();
    let project = fixture.path().join("restore-retry");
    let project_text = project.to_string_lossy().into_owned();
    let project_arg = project_text.as_str();
    ok(&["--machine", "create", project_arg]);

    let checkpoint = checkpoint_id(&ok(&[
        "--machine",
        "checkpoint",
        project_arg,
        "before-deletion",
    ]));
    let checkpoint_bytes = canonical_snapshot(&project);
    fs::write(project.join("styles.css"), b"/* later canonical state */\n").unwrap();
    let blocked = project.join("research/blocked");
    fs::create_dir_all(&blocked).unwrap();
    fs::write(
        blocked.join("obsolete.html"),
        b"<p data-block-id=\"obsolete\">obsolete</p>\n",
    )
    .unwrap();
    let before_failure = canonical_snapshot(&project);

    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o555)).unwrap();
    let failed = cli(&["--machine", "restore", project_arg, checkpoint.as_str()]);
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(failed.status.code(), Some(1));
    let failed = machine(&failed);
    assert!(!failed.ok);
    assert_redacted(&failed, project_arg);
    assert_redacted(&failed, "obsolete");
    assert_eq!(canonical_snapshot(&project), before_failure);

    ok(&["--machine", "open", project_arg]);
    assert_eq!(canonical_snapshot(&project), checkpoint_bytes);
}
