use std::{
    fs,
    future::Future,
    path::{Path, PathBuf},
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    time::{SystemTime, UNIX_EPOCH},
};

use parchmint_preferences::{
    AppearanceController, AppearanceMode, ApplicationPreferences, FilePreferenceStore,
    PreferenceCommand, PreferenceCoordinator, PreferenceError, PreferenceRevision,
    PreferenceService, PreferenceStore, RecentProject, ResolvedAppearance, ThemeSnapshot,
};

struct TemporaryFile {
    path: PathBuf,
}

impl TemporaryFile {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock should be after the Unix epoch")
            .as_nanos();
        Self {
            path: std::env::temp_dir().join(format!("parchmint-preferences-{label}-{nonce}.json")),
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn block_on<T>(future: impl Future<Output = T>) -> T {
    let parker = Arc::new(ThreadParker {
        thread: std::thread::current(),
    });
    let waker = Waker::from(parker);
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::park(),
        }
    }
}

struct ThreadParker {
    thread: std::thread::Thread,
}

impl Wake for ThreadParker {
    fn wake(self: Arc<Self>) {
        self.thread.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.thread.unpark();
    }
}

#[test]
fn preference_file_is_versioned_and_round_trips_recent_projects_dictionary_and_appearance() {
    let file = TemporaryFile::new("versioned");
    let store = FilePreferenceStore::new(file.path());
    let values = ApplicationPreferences {
        appearance: AppearanceMode::Dark,
        recent_projects: vec![
            RecentProject::new("Second", "/work/second.parchment", 20),
            RecentProject::new("First", "/work/first.parchment", 10),
        ],
        global_dictionary: ["ParchMint", "reflow"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
    };

    let saved = store.compare_and_save(PreferenceRevision::from(0), &values);
    let saved = block_on(saved).expect("first preference save should succeed");
    assert_eq!(saved.revision, PreferenceRevision::from(1));
    assert_eq!(saved.values, values);

    let raw = fs::read_to_string(file.path()).expect("preference file should exist");
    assert!(
        raw.contains("\"version\":2"),
        "storage must carry its schema version"
    );
    assert_eq!(
        block_on(store.load()).expect("versioned preferences should load"),
        saved
    );
}

#[test]
fn version_one_recent_paths_migrate_to_typed_records_and_newest_open_wins() {
    let file = TemporaryFile::new("recent-migration");
    fs::write(
        file.path(),
        br#"{"version":1,"revision":4,"preferences":{"appearance":"Dark","recent_projects":["/work/old-book","/work/other"],"global_dictionary":["ParchMint"]}}"#,
    )
    .expect("legacy preferences should be written");
    let store: Arc<dyn PreferenceStore> = Arc::new(FilePreferenceStore::new(file.path()));
    let service = PreferenceCoordinator::new(store);

    let migrated = block_on(service.load()).expect("version one preferences should migrate");
    assert_eq!(migrated.values.appearance, AppearanceMode::Dark);
    assert_eq!(migrated.values.global_dictionary, ["ParchMint"]);
    assert_eq!(
        migrated.values.recent_projects,
        [
            RecentProject::new("old-book", "/work/old-book", 0),
            RecentProject::new("other", "/work/other", 0),
        ]
    );

    let updated = block_on(service.update(
        migrated.revision,
        PreferenceCommand::AddRecentProject(RecentProject::new("Old Book", "/work/old-book", 99)),
    ))
    .expect("migrated recent project should update in place");
    assert_eq!(
        updated.values.recent_projects,
        [
            RecentProject::new("Old Book", "/work/old-book", 99),
            RecentProject::new("other", "/work/other", 0),
        ]
    );
    assert_eq!(updated.values.appearance, AppearanceMode::Dark);
    assert_eq!(updated.values.global_dictionary, ["ParchMint"]);
}

#[test]
fn stale_revision_is_rejected_without_overwriting_the_newer_preferences() {
    let file = TemporaryFile::new("stale");
    let store = FilePreferenceStore::new(file.path());
    let first = ApplicationPreferences {
        appearance: AppearanceMode::Light,
        ..ApplicationPreferences::default()
    };
    let current = store.compare_and_save(PreferenceRevision::from(0), &first);
    let current = block_on(current).expect("initial save should succeed");

    let mut stale = first.clone();
    stale.appearance = AppearanceMode::Dark;
    assert!(matches!(
        block_on(store.compare_and_save(PreferenceRevision::from(0), &stale)),
        Err(PreferenceError::StaleRevision { expected, actual })
            if expected == PreferenceRevision::from(0) && actual == current.revision
    ));
    assert_eq!(
        block_on(store.load()).expect("newer preferences should remain"),
        current
    );
}

#[test]
fn unreadable_preference_file_is_preserved_for_diagnosis() {
    let file = TemporaryFile::new("unreadable");
    fs::write(file.path(), b"{not valid preference data").expect("fixture should be written");
    let before = fs::read(file.path()).expect("fixture should be readable by the test");
    let store = FilePreferenceStore::new(file.path());

    assert!(matches!(
        block_on(store.load()),
        Err(PreferenceError::UnreadableFile { .. })
    ));
    assert!(matches!(
        block_on(store.compare_and_save(
            PreferenceRevision::from(0),
            &ApplicationPreferences::default(),
        )),
        Err(PreferenceError::UnreadableFile { .. })
    ));
    assert_eq!(
        fs::read(file.path()).expect("unreadable fixture must be preserved"),
        before
    );
}

#[test]
fn coordinator_applies_recent_projects_and_global_dictionary_as_application_changes() {
    let file = TemporaryFile::new("coordinator");
    let store: Arc<dyn PreferenceStore> = Arc::new(FilePreferenceStore::new(file.path()));
    let service = PreferenceCoordinator::new(store);
    let mut changes = service.changes();

    let initial = block_on(service.load()).expect("coordinator should load defaults");
    let recent = block_on(service.update(
        initial.revision,
        PreferenceCommand::AddRecentProject(RecentProject::new("Book", "/work/book.parchment", 17)),
    ))
    .expect("recent project update should succeed");
    let dictionary = block_on(service.update(
        recent.revision,
        PreferenceCommand::AddGlobalDictionaryWord("ParchMint".into()),
    ))
    .expect("global dictionary update should succeed");

    assert_eq!(
        dictionary.values.recent_projects,
        [RecentProject::new("Book", "/work/book.parchment", 17)]
    );
    assert_eq!(dictionary.values.global_dictionary, ["ParchMint"]);
    assert!(dictionary.revision > recent.revision);
    assert_eq!(
        changes
            .next()
            .expect("recent-project update should publish"),
        parchmint_preferences::PreferenceChange { snapshot: recent }
    );
    assert_eq!(
        changes.next().expect("dictionary update should publish"),
        parchmint_preferences::PreferenceChange {
            snapshot: dictionary
        }
    );
}

#[test]
fn appearance_modes_resolve_and_system_events_publish_snapshots() {
    let file = TemporaryFile::new("appearance");
    let store: Arc<dyn PreferenceStore> = Arc::new(FilePreferenceStore::new(file.path()));
    let service: Arc<dyn PreferenceService> = Arc::new(PreferenceCoordinator::new(store));
    let controller = AppearanceController::new(Arc::clone(&service));
    let snapshot = controller
        .initialize(
            &block_on(service.load()).expect("defaults should load"),
            ResolvedAppearance::Dark,
        )
        .expect("System should resolve from the OS appearance");
    assert_eq!(snapshot.appearance, ResolvedAppearance::Dark);
    let mut changes = controller.changes();

    let light = block_on(controller.set_mode(PreferenceRevision::from(0), AppearanceMode::Light))
        .expect("explicit Light should be accepted");
    assert_eq!(light.appearance, ResolvedAppearance::Light);
    let dark = block_on(controller.set_mode(PreferenceRevision::from(1), AppearanceMode::Dark))
        .expect("explicit Dark should be accepted");
    assert_eq!(dark.appearance, ResolvedAppearance::Dark);
    assert_eq!(changes.next().expect("Light update should publish"), light);
    assert_eq!(changes.next().expect("Dark update should publish"), dark);
    assert!(
        controller
            .system_appearance_changed(ResolvedAppearance::Light)
            .expect("event should be handled")
            .is_none()
    );

    let system = block_on(controller.set_mode(PreferenceRevision::from(2), AppearanceMode::System))
        .expect("System should be accepted");
    assert_eq!(system.appearance, ResolvedAppearance::Light);
    assert_eq!(
        changes.next().expect("System update should publish"),
        system
    );
    let event = controller
        .system_appearance_changed(ResolvedAppearance::Dark)
        .expect("System event should resolve")
        .expect("System event should publish a snapshot");
    assert_eq!(event.appearance, ResolvedAppearance::Dark);
    assert_eq!(event.generation, 5);
    assert_eq!(
        changes
            .next()
            .expect("System event should reach subscribers"),
        event
    );
}

#[test]
fn theme_snapshots_are_numbered_and_applied_to_all_windows_in_stable_id_order() {
    let snapshots = [
        ThemeSnapshot::new(ResolvedAppearance::Light, 4),
        ThemeSnapshot::new(ResolvedAppearance::Dark, 5),
    ];
    let mut applied = Vec::new();
    let windows = [9_u64, 2, 7];
    ThemeSnapshot::apply_to_windows(&snapshots, &windows, |window, snapshot| {
        applied.push((window, snapshot.generation));
    });
    assert_eq!(applied, [(2, 4), (7, 4), (9, 4), (2, 5), (7, 5), (9, 5)]);
}

#[test]
fn application_preferences_do_not_modify_project_state() {
    let file = TemporaryFile::new("application-only");
    let project = TemporaryFile::new("project-state");
    let project_state = b"canonical project state";
    fs::write(project.path(), project_state).expect("project fixture should be written");
    let store = FilePreferenceStore::new(file.path());
    let values = ApplicationPreferences {
        appearance: AppearanceMode::Dark,
        recent_projects: vec![RecentProject::new(
            "Project",
            project.path().display().to_string(),
            1,
        )],
        global_dictionary: vec!["ParchMint".into()],
    };
    block_on(store.compare_and_save(PreferenceRevision::from(0), &values))
        .expect("application preferences should save");

    assert_eq!(
        fs::read(project.path()).expect("preference save must not rewrite a project"),
        project_state
    );
}
