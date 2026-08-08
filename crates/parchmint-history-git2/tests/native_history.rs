use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use parchmint_history_api::{
    CheckpointCategory, HistoryError, HistoryPageQuery, HistoryStore, MaintenanceBudget,
};
use parchmint_project_fs::{
    FsError, NativeProjectFileSystem, ProjectFileSystem, UntrustedProjectPath,
};

mod common;

use common::{LockedProject, ProjectVersion, TEST_DOCUMENT, checkpoint, named_checkpoint};

const HELPER_MODE: &str = "PARCHMINT_HISTORY_GIT2_HELPER_MODE";
const HELPER_PATH: &str = "PARCHMINT_HISTORY_GIT2_HELPER_PATH";

#[test]
fn initialization_creates_private_linear_main_and_reopens_existing_history() {
    let project = LockedProject::new("initialize");
    let first_version = ProjectVersion::named("First draft");
    project.write(&first_version);
    let canonical_before = first_version.bytes();

    let store = project.store();
    let state = store
        .initialize(project.project.clone())
        .expect("new History should initialize");
    assert_eq!(state.project, project.project);
    assert_eq!(state.checkpoint_count, 0);
    assert_eq!(
        fs::read_to_string(project.path.join(".git/HEAD"))
            .expect("embedded repository should have HEAD"),
        "ref: refs/heads/main\n"
    );
    let config = fs::read_to_string(project.path.join(".git/config"))
        .expect("embedded repository should have local configuration");
    assert!(
        !config.contains("[remote "),
        "History must not configure remotes"
    );
    assert!(config.contains("filemode = false"));
    assert!(config.contains("autocrlf = false"));
    assert!(config.contains("symlinks = false"));
    for (path, bytes) in canonical_before {
        assert_eq!(project.read(&path), bytes, "initialize changed {path}");
    }

    let first = checkpoint(&store, 1, &first_version, CheckpointCategory::ExplicitSave)
        .expect("first checkpoint should commit");
    drop(store);

    let reopened = project.store();
    let reopened_state = reopened
        .initialize(project.project.clone())
        .expect("existing History should reopen");
    assert_eq!(reopened_state.checkpoint_count, 1);
    assert_eq!(
        reopened
            .list(HistoryPageQuery::newest_first(10))
            .expect("reopened History should list")
            .checkpoints[0]
            .id,
        first
    );
}

#[test]
fn checkpoints_are_idempotent_and_named_unchanged_snapshots_are_committed() {
    let project = LockedProject::new("checkpoint");
    let version = ProjectVersion::named("Stable draft");
    project.write(&version);
    let excluded: [(&str, &[u8]); 5] = [
        (".parchmint/recovery/pending.bin", b"recovery"),
        (".parchmint/cache/search.sqlite", b"cache"),
        (".parchmint/workspace-layout.json", b"workspace"),
        (".parchmint/appearance.json", b"appearance"),
        (".parchmint/global-dictionary.txt", b"global"),
    ];
    for (path, bytes) in excluded {
        let target = project.path.join(path);
        fs::create_dir_all(target.parent().expect("excluded file should have a parent"))
            .expect("excluded file parent should be created");
        fs::write(target, bytes).expect("excluded fixture should be written");
    }
    let store = project.initialize();

    let mut forbidden_resources = version.hashes();
    let existing_hash = *forbidden_resources
        .values()
        .next()
        .expect("canonical fixture should not be empty");
    forbidden_resources.insert(
        parchmint_project_format::CanonicalRelativePath::parse(".parchmint/recovery/pending.bin")
            .expect("recovery fixture path should be syntactically canonical"),
        existing_hash,
    );
    assert!(matches!(
        store.checkpoint(parchmint_history_api::CheckpointInput {
            intent_hash: parchmint_history_api::CheckpointIntentHash::from_bytes([0; 32]),
            resources: forbidden_resources,
            category: CheckpointCategory::ExplicitSave,
            affected_documents: vec![TEST_DOCUMENT],
            name: None,
        }),
        Err(HistoryError::InvalidInput { .. })
    ));

    let omitted = project.path.join("manuscript/omitted.html");
    fs::write(&omitted, b"<p>omitted canonical resource</p>\n")
        .expect("omitted canonical fixture should be written");
    assert!(matches!(
        checkpoint(&store, 0, &version, CheckpointCategory::ExplicitSave),
        Err(HistoryError::InvalidInput { .. })
    ));
    fs::remove_file(omitted).expect("omitted canonical fixture should be removed");

    let save = checkpoint(&store, 1, &version, CheckpointCategory::ExplicitSave)
        .expect("checkpoint should commit");
    assert_eq!(
        checkpoint(&store, 1, &version, CheckpointCategory::ExplicitSave)
            .expect("identical checkpoint intent should be idempotent"),
        save
    );
    let named = named_checkpoint(&store, 2, &version, "Before restructuring")
        .expect("an unchanged named snapshot should create an empty commit");
    assert_ne!(named, save);

    let page = store
        .list(HistoryPageQuery::newest_first(10))
        .expect("History should list");
    assert_eq!(page.checkpoints.len(), 2);
    assert_eq!(page.checkpoints[0].id, named);
    assert_eq!(page.checkpoints[0].sequence, 2);
    assert_eq!(
        page.checkpoints[0]
            .name
            .as_ref()
            .expect("named checkpoint should retain its name")
            .as_str(),
        "Before restructuring"
    );
    assert_eq!(page.checkpoints[1].id, save);
    assert_eq!(page.checkpoints[1].sequence, 1);
    let preview = store.preview(save).expect("checkpoint should preview");
    assert_eq!(preview.resources, version.hashes());
    let restore = store.restore(save).expect("checkpoint should restore");
    let restore_paths: Vec<_> = restore
        .writes()
        .writes
        .iter()
        .map(|write| write.path.as_str())
        .collect();
    assert!(restore_paths.iter().all(|path| {
        !path.starts_with(".parchmint/recovery/")
            && !path.starts_with(".parchmint/cache/")
            && !path.contains("workspace")
            && !path.contains("appearance")
            && !path.contains("global-dictionary")
    }));
}

#[test]
fn checkpoint_retries_return_the_original_after_the_project_advances() {
    let project = LockedProject::new("checkpoint-retry");
    let first = ProjectVersion::named("First draft");
    let second = ProjectVersion::named("Second draft");
    project.write(&first);
    let store = project.initialize();

    let original = checkpoint(&store, 1, &first, CheckpointCategory::ExplicitSave)
        .expect("first checkpoint should commit");
    project.write(&second);
    checkpoint(&store, 2, &second, CheckpointCategory::ExplicitSave)
        .expect("later checkpoint should commit");

    assert_eq!(
        checkpoint(&store, 1, &first, CheckpointCategory::ExplicitSave)
            .expect("a completed intent retry should be idempotent"),
        original
    );
}

#[test]
fn newest_first_order_and_cursor_continuation_stay_stable_after_an_append() {
    let project = LockedProject::new("ordering");
    let store = project.initialize();
    let versions = [
        ProjectVersion::named("One"),
        ProjectVersion::named("Two"),
        ProjectVersion::named("Three"),
        ProjectVersion::named("Four"),
    ];
    let mut ids = Vec::new();
    for (index, version) in versions[..3].iter().enumerate() {
        project.write(version);
        ids.push(
            checkpoint(
                &store,
                index as u8 + 1,
                version,
                CheckpointCategory::Autosave,
            )
            .expect("ordered checkpoint should commit"),
        );
    }

    let first_page = store
        .list(HistoryPageQuery::newest_first(2))
        .expect("first page should list");
    assert_eq!(
        first_page
            .checkpoints
            .iter()
            .map(|checkpoint| checkpoint.id)
            .collect::<Vec<_>>(),
        vec![ids[2], ids[1]]
    );
    assert_eq!(
        first_page
            .checkpoints
            .iter()
            .map(|checkpoint| checkpoint.sequence)
            .collect::<Vec<_>>(),
        vec![3, 2]
    );

    project.write(&versions[3]);
    let fourth = checkpoint(&store, 4, &versions[3], CheckpointCategory::Autosave)
        .expect("new checkpoint should append");
    let continuation = store
        .list(HistoryPageQuery {
            cursor: first_page.next_cursor,
            limit: 2,
            affected_document: None,
        })
        .expect("an issued cursor should keep its original continuation");
    assert_eq!(
        continuation
            .checkpoints
            .iter()
            .map(|checkpoint| checkpoint.id)
            .collect::<Vec<_>>(),
        vec![ids[0]]
    );
    assert!(continuation.next_cursor.is_none());

    let all = store
        .list(HistoryPageQuery::newest_first(10))
        .expect("complete History should list");
    assert_eq!(
        all.checkpoints
            .iter()
            .map(|checkpoint| checkpoint.id)
            .collect::<Vec<_>>(),
        vec![fourth, ids[2], ids[1], ids[0]]
    );
    assert!(
        all.checkpoints
            .iter()
            .all(|summary| summary.affected_documents == [TEST_DOCUMENT])
    );
}

#[test]
fn restore_returns_a_complete_plan_without_mutating_files_or_rewinding_history() {
    let project = LockedProject::new("restore");
    let first_version = ProjectVersion::named("First draft");
    let second_version = ProjectVersion::named("Second draft");
    let current_version = ProjectVersion::named("Unsaved current draft");
    project.write(&first_version);
    let store = project.initialize();
    let source = checkpoint(&store, 1, &first_version, CheckpointCategory::ExplicitSave)
        .expect("source checkpoint should commit");
    project.write(&second_version);
    let second = checkpoint(&store, 2, &second_version, CheckpointCategory::ExplicitSave)
        .expect("second checkpoint should commit");
    project.write(&current_version);
    let obsolete = project.path.join("manuscript/obsolete.html");
    fs::write(&obsolete, b"<p>obsolete</p>\n").expect("obsolete canonical file should be written");
    let before_restore = store
        .list(HistoryPageQuery::newest_first(10))
        .expect("History should list before restore");

    let plan = store
        .restore(source)
        .expect("whole-project restore plan should load");
    assert_eq!(plan.source(), source);
    assert_eq!(plan.resources(), &first_version.hashes());
    let writes: BTreeMap<_, _> = plan
        .writes()
        .writes
        .iter()
        .map(|write| (write.path.clone(), write.bytes.clone()))
        .collect();
    let expected_writes: BTreeMap<_, _> = first_version
        .bytes()
        .into_iter()
        .map(|(path, bytes)| (path.as_str().to_owned(), bytes))
        .collect();
    assert_eq!(writes, expected_writes);
    assert_eq!(
        plan.deletions(),
        &[
            parchmint_project_format::CanonicalRelativePath::parse("manuscript/obsolete.html")
                .expect("obsolete path should be canonical")
        ]
    );
    for (path, bytes) in current_version.bytes() {
        assert_eq!(project.read(&path), bytes, "restore changed current {path}");
    }
    assert_eq!(
        store
            .list(HistoryPageQuery::newest_first(10))
            .expect("History should remain listable"),
        before_restore
    );

    for write in &plan.writes().writes {
        fs::write(project.path.join(&write.path), &write.bytes)
            .expect("normal save path should apply restore write");
    }
    for deletion in plan.deletions() {
        fs::remove_file(project.path.join(deletion.as_str()))
            .expect("normal save path should apply restore deletion");
    }
    let restoration = checkpoint(&store, 3, &first_version, CheckpointCategory::Restoration)
        .expect("applied restore should append a restoration checkpoint");
    let after_restore = store
        .list(HistoryPageQuery::newest_first(10))
        .expect("History should list after restoration checkpoint");
    assert_eq!(after_restore.checkpoints[0].id, restoration);
    assert_eq!(after_restore.checkpoints[1].id, second);
    assert_eq!(after_restore.checkpoints[2].id, source);
    assert_eq!(
        store
            .preview(source)
            .expect("source checkpoint should remain reachable")
            .resources,
        first_version.hashes()
    );
}

#[test]
fn a_corrupt_git_object_is_reported_without_touching_current_files_or_other_projects() {
    let damaged_project = LockedProject::new("corrupt");
    let saved = ProjectVersion::named("Saved draft");
    let current = ProjectVersion::named("Current readable draft");
    damaged_project.write(&saved);
    let damaged_store = damaged_project.initialize();
    let checkpoint_id = checkpoint(&damaged_store, 1, &saved, CheckpointCategory::ExplicitSave)
        .expect("checkpoint should commit before corruption");
    damaged_project.write(&current);

    let healthy_project = LockedProject::new("corrupt-isolation");
    let healthy_version = ProjectVersion::named("Independent project");
    healthy_project.write(&healthy_version);
    let healthy_store = healthy_project.initialize();
    let healthy_id = checkpoint(
        &healthy_store,
        1,
        &healthy_version,
        CheckpointCategory::ExplicitSave,
    )
    .expect("independent checkpoint should commit");

    corrupt_head_object(&damaged_project.path);
    assert!(matches!(
        damaged_store.preview(checkpoint_id),
        Err(HistoryError::CorruptHistory { .. })
    ));
    assert!(matches!(
        damaged_store.verify(),
        Err(HistoryError::CorruptHistory { .. })
    ));
    for (path, bytes) in current.bytes() {
        assert_eq!(
            damaged_project.read(&path),
            bytes,
            "corrupt History changed current {path}"
        );
    }
    assert_eq!(
        healthy_store
            .preview(healthy_id)
            .expect("another project's History should remain readable")
            .resources,
        healthy_version.hashes()
    );
}

#[test]
fn stale_git_lock_recovery_requires_the_current_project_lock_owner() {
    let mut project = LockedProject::new("stale-lock");
    let first_version = ProjectVersion::named("Before interruption");
    let second_version = ProjectVersion::named("After recovery");
    project.write(&first_version);
    let store = project.initialize();
    checkpoint(&store, 1, &first_version, CheckpointCategory::ExplicitSave)
        .expect("first checkpoint should commit");

    assert!(matches!(
        NativeProjectFileSystem::new().acquire(UntrustedProjectPath::new(project.path.clone())),
        Err(FsError::Locked { .. })
    ));
    let git_lock = project.path.join(".git/index.lock");
    fs::write(&git_lock, b"interrupted owner").expect("stale Git lock should be simulated");
    project.write(&second_version);
    project.release_lock();

    assert!(
        store
            .checkpoint(parchmint_history_api::CheckpointInput {
                intent_hash: parchmint_history_api::CheckpointIntentHash::from_bytes([2; 32]),
                resources: second_version.hashes(),
                category: CheckpointCategory::ExplicitSave,
                affected_documents: vec![TEST_DOCUMENT],
                name: None,
            })
            .is_err()
    );
    assert!(
        git_lock.exists(),
        "a former owner must not remove the Git lock"
    );

    project.reacquire();
    let recovered = project.store();
    let state = recovered
        .initialize(project.project.clone())
        .expect("the current project owner should recover stale Git locks");
    assert_eq!(state.checkpoint_count, 1);
    let resumed = checkpoint(
        &recovered,
        2,
        &second_version,
        CheckpointCategory::ExplicitSave,
    )
    .expect("checkpointing should continue after stale-lock recovery");
    assert!(!git_lock.exists(), "recovered Git lock should be removed");
    assert_eq!(
        recovered
            .list(HistoryPageQuery::newest_first(10))
            .expect("recovered History should list")
            .checkpoints[0]
            .id,
        resumed
    );
}

#[test]
fn embedded_history_identity_must_match_the_project_root() {
    let project = LockedProject::new("history-identity");
    let version = ProjectVersion::named("Bound draft");
    project.write(&version);
    let store = project.initialize();
    checkpoint(&store, 1, &version, CheckpointCategory::ExplicitSave)
        .expect("checkpoint should commit before identity damage");

    let config_path = project.path.join(".git/config");
    let config = fs::read_to_string(&config_path).expect("History config should be readable");
    let history_id = fs::read_to_string(project.path.join(".parchmint/root-id"))
        .expect("project identity should be readable");
    let damaged = config.replace(history_id.trim(), "ffffffffffffffff");
    assert_ne!(
        damaged, config,
        "History identity should be present in config"
    );
    fs::write(config_path, damaged).expect("History identity should be damaged");

    assert!(matches!(
        project.store().initialize(project.project.clone()),
        Err(HistoryError::CorruptHistory { .. })
    ));
}

#[test]
fn line_endings_are_normalized_without_changing_checkpoint_identity() {
    let project = LockedProject::new("line-endings");
    let version = ProjectVersion::named("Portable draft");
    project.write_crlf(&version);
    let store = project.initialize();

    let crlf_checkpoint = checkpoint(&store, 1, &version, CheckpointCategory::ExplicitSave)
        .expect("CRLF working files should match canonical hashes");
    let preview = store
        .preview(crlf_checkpoint)
        .expect("portable checkpoint should preview");
    assert_eq!(preview.resources, version.hashes());
    let restore = store
        .restore(crlf_checkpoint)
        .expect("portable checkpoint should restore");
    for write in &restore.writes().writes {
        if write.path != ".parchmint/format-version" {
            assert!(
                !write.bytes.windows(2).any(|window| window == b"\r\n"),
                "restored canonical bytes should use LF: {}",
                write.path
            );
        }
    }

    project.write(&version);
    let named = named_checkpoint(&store, 2, &version, "Portable marker")
        .expect("LF files should produce the same tree and allow a named empty commit");
    let page = store
        .list(HistoryPageQuery::newest_first(10))
        .expect("portable History should list");
    assert_eq!(page.checkpoints[0].id, named);
    assert_eq!(page.checkpoints[1].id, crlf_checkpoint);
}

#[test]
fn maintenance_honors_its_object_budget_and_retains_every_checkpoint() {
    let project = LockedProject::new("maintenance");
    let store = project.initialize();
    for index in 1..=3 {
        let version = ProjectVersion::named(&format!("Draft {index}"));
        project.write(&version);
        checkpoint(&store, index, &version, CheckpointCategory::Autosave)
            .expect("maintenance fixture checkpoint should commit");
    }

    let no_work = store
        .maintain(MaintenanceBudget::new(0))
        .expect("zero-budget maintenance should be a no-op");
    assert_eq!(no_work.checked_objects, 0);
    assert_eq!(no_work.retained_checkpoints, 3);
    let loose_before = loose_object_count(&project.path);

    let bounded = store
        .maintain(MaintenanceBudget::new(1))
        .expect("bounded maintenance should complete");
    assert_eq!(bounded.checked_objects, 1);
    assert_eq!(bounded.retained_checkpoints, 3);
    assert_eq!(loose_object_count(&project.path) + 1, loose_before);
    let pack_directory = project.path.join(".git/objects/pack");
    let pack_files: Vec<_> = fs::read_dir(pack_directory)
        .expect("maintenance pack directory should exist")
        .map(|entry| {
            entry
                .expect("maintenance pack entry should be readable")
                .path()
        })
        .collect();
    assert!(pack_files.iter().any(|path| {
        path.extension()
            .is_some_and(|extension| extension == "pack")
    }));
    assert!(
        pack_files
            .iter()
            .any(|path| path.extension().is_some_and(|extension| extension == "idx"))
    );
    assert_eq!(
        store
            .list(HistoryPageQuery::newest_first(10))
            .expect("maintenance must retain History")
            .checkpoints
            .len(),
        3
    );
}

#[test]
fn embedded_history_needs_neither_a_git_executable_nor_network_transport() {
    let mut project = LockedProject::new("offline");
    let version = ProjectVersion::named("Offline draft");
    project.write(&version);
    project.release_lock();

    let missing_tools = project.path.join("missing-tools");
    let status = Command::new(env::current_exe().expect("test executable should be available"))
        .args(["--exact", "embedded_history_helper", "--nocapture"])
        .env(HELPER_MODE, "checkpoint")
        .env(HELPER_PATH, &project.path)
        .env("PATH", &missing_tools)
        .env("GIT_EXEC_PATH", &missing_tools)
        .env("GIT_PROXY_COMMAND", &missing_tools)
        .status()
        .expect("embedded History helper should start");
    assert!(
        status.success(),
        "History should work with Git and network helpers unavailable"
    );

    let config = fs::read_to_string(project.path.join(".git/config"))
        .expect("embedded repository config should be readable");
    assert!(!config.contains("[remote "));
    assert!(!project.path.join(".gitmodules").exists());
}

#[test]
fn git2_is_pinned_to_the_vendored_transport_free_selection() {
    let manifest = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
        .expect("History Git2 manifest should be readable");
    assert!(manifest.contains(
        "git2 = { version = \"=0.21.0\", default-features = false, features = [\"vendored-libgit2\"] }"
    ));
    assert!(manifest.contains(
        "libz-sys = { version = \"=1.1.29\", default-features = false, features = [\"libc\", \"static\"] }"
    ));
}

#[test]
fn embedded_history_helper() {
    if env::var_os(HELPER_MODE).as_deref() != Some(std::ffi::OsStr::new("checkpoint")) {
        return;
    }
    let path = PathBuf::from(env::var_os(HELPER_PATH).expect("helper path should be provided"));
    let (root, _lease) = NativeProjectFileSystem::new()
        .acquire(UntrustedProjectPath::new(path))
        .expect("helper should acquire the project lock");
    let store = parchmint_history_git2::Git2HistoryStore::new(root);
    store
        .initialize(parchmint_project_repository::ProjectRootCapability::new(
            0x09,
        ))
        .expect("helper should initialize embedded History");
    checkpoint(
        &store,
        1,
        &ProjectVersion::named("Offline draft"),
        CheckpointCategory::ExplicitSave,
    )
    .expect("helper should create a checkpoint without external Git");
}

fn corrupt_head_object(project: &Path) {
    let head = fs::read_to_string(project.join(".git/HEAD")).expect("HEAD should be readable");
    let reference = head
        .strip_prefix("ref: ")
        .expect("History HEAD should be symbolic")
        .trim();
    let object_id = fs::read_to_string(project.join(".git").join(reference))
        .expect("main reference should be readable");
    let object_id = object_id.trim();
    assert_eq!(object_id.len(), 40, "Git object ID should be SHA-1");
    let object_path = project
        .join(".git/objects")
        .join(&object_id[..2])
        .join(&object_id[2..]);
    make_writable(&object_path);
    fs::write(object_path, b"corrupt object").expect("HEAD object should be corrupted");
}

fn loose_object_count(project: &Path) -> usize {
    fs::read_dir(project.join(".git/objects"))
        .expect("Git object directory should be readable")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().len() == 2)
        .map(|entry| {
            fs::read_dir(entry.path())
                .expect("loose object bucket should be readable")
                .filter_map(Result::ok)
                .filter(|object| object.path().is_file())
                .count()
        })
        .sum()
}

#[cfg(unix)]
fn make_writable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .expect("Git object metadata should be readable")
        .permissions();
    permissions.set_mode(permissions.mode() | 0o200);
    fs::set_permissions(path, permissions).expect("Git object should be made writable");
}

#[cfg(windows)]
fn make_writable(path: &Path) {
    let mut permissions = fs::metadata(path)
        .expect("Git object metadata should be readable")
        .permissions();
    permissions.set_readonly(false);
    fs::set_permissions(path, permissions).expect("Git object should be made writable");
}
