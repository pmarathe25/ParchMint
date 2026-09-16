use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use parchmint_domain::{NodeId, ProjectId, ViewId};
use parchmint_workspace_state::{
    ExplorerWorkspaceState, FileWorkspaceStateStore, OpenTabState, PaneLayout, ProjectIdentity,
    SavedViewState, WorkspaceMode, WorkspaceSnapshot, WorkspaceStateStore, WorkspaceWarning,
};

struct TemporaryDirectory {
    path: PathBuf,
}

static TEMPORARY_DIRECTORY_SEQUENCE: AtomicUsize = AtomicUsize::new(1);

impl TemporaryDirectory {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "parchmint-workspace-state-{label}-{}-{}",
            std::process::id(),
            TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir_all(&path).expect("workspace-state test directory should be created");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn project(value: u8) -> ProjectIdentity {
    ProjectIdentity::new(ProjectId::from_bytes([value; 16]))
}

fn node(value: u8) -> NodeId {
    NodeId::from_bytes([value; 16])
}

fn view(value: u8) -> ViewId {
    ViewId::from_bytes([value; 16])
}

fn snapshot() -> WorkspaceSnapshot {
    let open_view = view(3);
    WorkspaceSnapshot {
        layout: PaneLayout {
            explorer_width: 280,
            inspector_width: 360,
            split_ratio: 0.62,
            explorer_collapsed: false,
            inspector_collapsed: true,
            companion_open: true,
        },
        explorer: ExplorerWorkspaceState {
            expanded_sections: BTreeSet::from([node(1), node(2)]),
            selected_nodes: BTreeSet::from([node(2)]),
        },
        tabs: vec![OpenTabState {
            view: open_view,
            node: node(1),
        }],
        active_view: Some(open_view),
        views: BTreeMap::from([(
            open_view,
            SavedViewState {
                node: node(1),
                scroll_offset: 418,
            },
        )]),
        mode: WorkspaceMode::Cards,
        cards_section: Some(node(2)),
    }
}

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    match future.as_mut().poll(&mut context) {
        std::task::Poll::Ready(value) => value,
        std::task::Poll::Pending => panic!("file store operations complete synchronously"),
    }
}

#[test]
fn versioned_workspace_files_round_trip_all_application_only_state_per_project() {
    let directory = TemporaryDirectory::new("round-trip");
    let store = FileWorkspaceStateStore::new(directory.path());
    let saved = snapshot();

    let revision = block_on(store.save(project(1), &saved)).expect("workspace save should succeed");
    assert_eq!(revision.value(), 1);
    let second_project = WorkspaceSnapshot {
        mode: WorkspaceMode::Editor,
        ..WorkspaceSnapshot::default()
    };
    let second_revision = block_on(store.save(project(2), &second_project))
        .expect("second project workspace save should succeed");
    assert_eq!(second_revision.value(), 1);

    let restored = block_on(store.load(project(1)))
        .expect("workspace load should succeed")
        .expect("saved workspace should be present");
    assert_eq!(restored, saved);
    let second_restored = block_on(store.load(project(2)))
        .expect("second project workspace load should succeed")
        .expect("second project workspace should be present");
    assert_eq!(second_restored, second_project);

    let files = fs::read_dir(directory.path())
        .expect("application-data directory should be readable")
        .map(|entry| entry.expect("workspace entry should be readable").path())
        .collect::<Vec<_>>();
    assert_eq!(files.len(), 2, "each project must have one workspace file");
    let raw =
        fs::read_to_string(store.path_for(project(1))).expect("workspace file should be readable");
    assert!(raw.contains("\"version\":1"), "workspace data is versioned");
}

#[test]
fn pruning_deleted_nodes_clears_selection_tabs_views_and_cards_context() {
    let mut saved = snapshot();
    saved.remove_missing_nodes(&BTreeSet::from([node(2)]));
    assert_eq!(saved.explorer.expanded_sections, BTreeSet::from([node(2)]));
    assert_eq!(saved.explorer.selected_nodes, BTreeSet::from([node(2)]));
    assert!(saved.tabs.is_empty());
    assert!(saved.views.is_empty());
    assert_eq!(saved.active_view, None);
    assert_eq!(saved.cards_section, Some(node(2)));

    saved.remove_missing_nodes(&BTreeSet::new());
    assert!(saved.explorer.expanded_sections.is_empty());
    assert!(saved.explorer.selected_nodes.is_empty());
    assert_eq!(saved.cards_section, None);
}

#[test]
fn missing_or_invalid_workspace_file_uses_defaults_and_reports_invalid_data() {
    let directory = TemporaryDirectory::new("fallback");
    let store = FileWorkspaceStateStore::new(directory.path());
    let default = WorkspaceSnapshot::default();

    let missing = block_on(store.load_or_default(project(1), &BTreeSet::new()))
        .expect("missing workspace should use defaults");
    assert_eq!(missing.snapshot, default);
    assert!(missing.warning.is_none());

    block_on(store.save(project(1), &snapshot())).expect("workspace save should succeed");
    let file = store.path_for(project(1));
    let mut malformed: serde_json::Value =
        serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    // A 32-byte identifier with a multibyte scalar crossing a hex-pair boundary.
    malformed["active_view"] = serde_json::Value::String(format!("a€{}", "0".repeat(28)));
    for bytes in [
        b"not valid workspace data".to_vec(),
        serde_json::to_vec(&malformed).unwrap(),
    ] {
        fs::write(&file, &bytes).expect("invalid workspace fixture should write");
        let invalid = block_on(store.load_or_default(project(1), &BTreeSet::new()))
            .expect("invalid workspace should use defaults");
        assert_eq!(invalid.snapshot, default);
        assert!(matches!(
            invalid.warning,
            Some(WorkspaceWarning::InvalidFile { path, .. }) if path == file
        ));
        assert_eq!(
            fs::read(&file).expect("invalid workspace should be preserved"),
            bytes
        );
    }
}

#[test]
fn invalid_layout_cannot_replace_a_readable_workspace() {
    let directory = TemporaryDirectory::new("invalid-layout");
    let store = FileWorkspaceStateStore::new(directory.path());
    let mut saved = snapshot();
    block_on(store.save(project(1), &saved)).unwrap();
    let before = fs::read(store.path_for(project(1))).unwrap();

    for ratio in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        saved.layout.split_ratio = ratio;
        assert!(block_on(store.save(project(1), &saved)).is_err());
        assert_eq!(fs::read(store.path_for(project(1))).unwrap(), before);
        assert_eq!(block_on(store.load(project(1))).unwrap(), Some(snapshot()));
    }
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn removing_a_workspace_is_idempotent_and_resets_its_revision() {
    let directory = TemporaryDirectory::new("remove");
    let store = FileWorkspaceStateStore::new(directory.path());
    block_on(store.save(project(1), &snapshot())).unwrap();
    for _ in 0..2 {
        block_on(store.remove(project(1))).unwrap();
        assert_eq!(block_on(store.load(project(1))).unwrap(), None);
    }
    assert_eq!(
        block_on(store.save(project(1), &snapshot()))
            .unwrap()
            .value(),
        1
    );
}

#[test]
fn workspace_directory_failure_preserves_the_blocking_file() {
    let directory = TemporaryDirectory::new("failure");
    let blocker = directory.path().join("not-a-directory");
    fs::write(&blocker, b"existing data").unwrap();
    let store = FileWorkspaceStateStore::new(&blocker);
    assert!(matches!(
        block_on(store.save(project(1), &snapshot())),
        Err(parchmint_workspace_state::WorkspaceError::Storage {
            operation: "create application-data directory",
            ..
        })
    ));
    assert_eq!(fs::read(&blocker).unwrap(), b"existing data");
}
