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
    let mut future = Box::pin(future);
    let waker = std::task::Waker::noop();
    let mut context = std::task::Context::from_waker(waker);
    loop {
        match future.as_mut().poll(&mut context) {
            std::task::Poll::Ready(value) => return value,
            std::task::Poll::Pending => std::thread::yield_now(),
        }
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
    assert!(
        !raw.contains("authored"),
        "workspace data stays application-only"
    );
}

#[test]
fn restoring_a_project_prunes_deleted_node_references() {
    let directory = TemporaryDirectory::new("prune");
    let store = FileWorkspaceStateStore::new(directory.path());
    let saved = snapshot();
    block_on(store.save(project(1), &saved)).expect("workspace save should succeed");

    let restored = block_on(store.load_or_default(project(1), &BTreeSet::from([node(1)])))
        .expect("workspace restore should succeed")
        .snapshot;
    assert_eq!(restored.tabs.len(), 1);
    assert!(restored.views.contains_key(&view(3)));
    assert!(!restored.explorer.expanded_sections.contains(&node(2)));

    let mut deleted = saved;
    deleted.tabs.push(OpenTabState {
        view: view(4),
        node: node(99),
    });
    deleted.views.insert(
        view(4),
        SavedViewState {
            node: node(99),
            scroll_offset: 7,
        },
    );
    block_on(store.save(project(1), &deleted)).expect("updated workspace save should succeed");
    let restored = block_on(store.load_or_default(project(1), &BTreeSet::from([node(1)])))
        .expect("workspace restore should succeed")
        .snapshot;
    assert!(restored.tabs.iter().all(|tab| tab.node == node(1)));
    assert!(!restored.views.contains_key(&view(4)));
    assert_eq!(restored.active_view, Some(view(3)));
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
    fs::write(&file, b"not valid workspace data").expect("invalid workspace fixture should write");
    let before = fs::read(&file).expect("invalid workspace fixture should remain readable");

    let invalid = block_on(store.load_or_default(project(1), &BTreeSet::new()))
        .expect("invalid workspace should use defaults");
    assert_eq!(invalid.snapshot, default);
    assert!(matches!(
        invalid.warning,
        Some(WorkspaceWarning::InvalidFile { path, .. }) if path == file
    ));
    assert_eq!(
        fs::read(file).expect("invalid workspace should be preserved"),
        before
    );
}

#[test]
fn workspace_save_failure_leaves_project_and_history_data_untouched() {
    let directory = TemporaryDirectory::new("failure");
    let blocker = directory.path().join("not-a-directory");
    fs::write(&blocker, b"project data").expect("blocking file should be created");
    let project_data = directory.path().join("project.json");
    let history_data = directory.path().join("history.json");
    fs::write(&project_data, b"canonical project").expect("project fixture should be created");
    fs::write(&history_data, b"project history").expect("history fixture should be created");
    let store = FileWorkspaceStateStore::new(&blocker);

    assert!(
        block_on(store.save(project(1), &snapshot())).is_err(),
        "workspace failure should be observable"
    );
    assert_eq!(
        fs::read(&blocker).expect("project data should be preserved"),
        b"project data"
    );
    assert_eq!(
        fs::read(project_data).expect("project data should be preserved"),
        b"canonical project"
    );
    assert_eq!(
        fs::read(history_data).expect("history data should be preserved"),
        b"project history"
    );
}
