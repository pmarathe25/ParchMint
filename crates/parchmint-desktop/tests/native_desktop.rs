//! Native desktop orchestration contracts through the injected bootstrap seam.

mod fixtures;

use std::path::Path;
use std::sync::atomic::Ordering;

use fixtures::desktop::{
    Event, FakeProjectFilesystem, block_on, final_save_request, fixture, fixture_with_filesystem,
};
use parchmint_desktop::{
    DesktopError, FinalSaveResolution, LaunchIntent, LaunchRequest, NewProjectRequest,
    OpenProjectResult, ProjectFilesystemError, StartupError,
};
use parchmint_platform_api::SystemAppearance;
use parchmint_preferences::{AppearanceMode, ResolvedAppearance};

#[test]
fn launch_request_parses_the_optional_first_project_argument() {
    assert_eq!(
        LaunchRequest::from_args(["parchmint"]),
        LaunchRequest::launcher()
    );
    assert_eq!(
        LaunchRequest::from_args(["parchmint", "/tmp/novel.parchmint"]),
        LaunchRequest::open("/tmp/novel.parchmint")
    );
    assert_eq!(
        LaunchRequest::from_args([
            "parchmint",
            "/tmp/first.parchmint",
            "/tmp/ignored.parchmint",
        ]),
        LaunchRequest::open("/tmp/first.parchmint")
    );
}

#[test]
fn run_completes_startup_and_project_routing_before_entering_the_ui_driver() {
    let project = "/tmp/run-wiring.parchmint";
    let desktop = fixture(AppearanceMode::System, SystemAppearance::Dark);

    assert_eq!(
        desktop.bootstrap.run(LaunchRequest::open(project)),
        Ok(parchmint_desktop::ExitCode::SUCCESS)
    );
    let events = desktop.ui.events();
    let opened = events
        .iter()
        .position(|event| matches!(event, Event::Opened { .. }))
        .expect("project should open before the native driver");
    let ran = events
        .iter()
        .position(|event| *event == Event::Ran)
        .expect("native driver should run");
    assert!(opened < ran);
}

#[test]
fn startup_injects_services_resolves_initial_appearance_and_routes_launch_intent() {
    let project = "/tmp/launch-intent.parchmint";
    let desktop = fixture(AppearanceMode::Dark, SystemAppearance::Light);
    let runtime = block_on(desktop.bootstrap.start(LaunchRequest::open(project))).unwrap();

    assert!(desktop.ui.received_application_services());
    assert!(desktop.ui.events().contains(&Event::Started {
        appearance: parchmint_preferences::ThemeSnapshot::new(ResolvedAppearance::Dark, 1),
        intent: LaunchIntent::Open(parchmint_desktop::RequestedProjectPath::new(project)),
    }));
    assert!(desktop.ui.events().iter().any(|event| matches!(
        event,
        Event::Opened { project: path, .. } if path == Path::new(project)
    )));
    assert!(desktop.filesystem.is_locked(Path::new(project)));
    assert_eq!(desktop.system_appearance_reads.load(Ordering::Relaxed), 0);
    assert!(
        runtime
            .route_launch(LaunchRequest::launcher())
            .unwrap()
            .is_none()
    );
}

#[test]
fn system_startup_reads_the_current_appearance_once() {
    let desktop = fixture(AppearanceMode::System, SystemAppearance::Dark);

    let _runtime = block_on(desktop.bootstrap.start(LaunchRequest::launcher())).unwrap();

    assert_eq!(desktop.system_appearance_reads.load(Ordering::Relaxed), 1);
}

#[test]
fn startup_failure_cleans_up_partial_session_and_window_registration() {
    let project = Path::new("/tmp/partial.parchmint");
    let desktop = fixture(AppearanceMode::System, SystemAppearance::Light);
    desktop.ui.fail_next_project_open();

    assert!(matches!(
        block_on(desktop.bootstrap.start(LaunchRequest::open(project))),
        Err(StartupError::Ui(_))
    ));
    assert!(!desktop.filesystem.is_locked(project));
    assert!(
        desktop
            .ui
            .events()
            .iter()
            .all(|event| !matches!(event, Event::Closed(_)))
    );
}

#[test]
fn repeated_open_focuses_existing_window_without_registering_another_session() {
    let project = "/tmp/repeated.parchmint";
    let desktop = fixture(AppearanceMode::System, SystemAppearance::Light);
    let runtime = block_on(desktop.bootstrap.start(LaunchRequest::launcher())).unwrap();
    let first = runtime.open_project(project).unwrap();
    let second = runtime.open_project(project).unwrap();

    assert!(matches!(first, OpenProjectResult::Opened { .. }));
    assert!(matches!(second, OpenProjectResult::Focused(_)));
    assert_eq!(
        desktop
            .ui
            .events()
            .iter()
            .filter(|event| matches!(event, Event::Opened { .. }))
            .count(),
        1
    );
}

#[test]
fn create_validates_input_and_records_recent_only_after_the_window_opens() {
    let desktop = fixture(AppearanceMode::System, SystemAppearance::Light);
    let runtime = block_on(desktop.bootstrap.start(LaunchRequest::launcher())).unwrap();

    assert!(matches!(
        runtime.create_project(NewProjectRequest::new("  ", "/tmp/blank-title", None)),
        Err(DesktopError::InvalidNewProject(_))
    ));
    assert!(matches!(
        runtime.create_project(NewProjectRequest::new("Book", "relative/project", None)),
        Err(DesktopError::InvalidNewProject(_))
    ));

    desktop.ui.fail_next_project_open();
    assert!(matches!(
        runtime.create_project(NewProjectRequest::new(
            "Failed Book",
            "/tmp/failed-create-window",
            None,
        )),
        Err(DesktopError::Ui(_))
    ));
    assert!(
        desktop
            .preferences
            .snapshot()
            .values
            .recent_projects
            .is_empty()
    );

    assert!(matches!(
        runtime.create_project(NewProjectRequest::new(
            "New Book",
            "/tmp/new-book",
            Some("Writer".to_owned()),
        )),
        Ok(OpenProjectResult::Opened { .. })
    ));
    let recent = desktop.preferences.snapshot().values.recent_projects;
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].name, "New Book");
    assert_eq!(recent[0].path, "/tmp/new-book");
    assert!(recent[0].last_opened_unix_seconds > 0);
}

#[test]
fn a_second_process_gets_safe_locked_project_result() {
    let project = "/tmp/cross-process-lock.parchmint";
    let (first_filesystem, second_filesystem) = FakeProjectFilesystem::shared();
    let first = fixture_with_filesystem(
        first_filesystem,
        AppearanceMode::System,
        SystemAppearance::Light,
    );
    let first_runtime = block_on(first.bootstrap.start(LaunchRequest::launcher())).unwrap();
    first_runtime.open_project(project).unwrap();
    let second = fixture_with_filesystem(
        second_filesystem,
        AppearanceMode::System,
        SystemAppearance::Light,
    );
    let second_runtime = block_on(second.bootstrap.start(LaunchRequest::launcher())).unwrap();

    assert_eq!(
        second_runtime.open_project(project),
        Ok(OpenProjectResult::Locked)
    );
    assert!(second.ui.events().contains(&Event::Locked(project.into())));
}

#[test]
fn window_and_session_generations_filter_results_after_close_and_reopen() {
    let project = "/tmp/generation.parchmint";
    let desktop = fixture(AppearanceMode::System, SystemAppearance::Light);
    let runtime = block_on(desktop.bootstrap.start(LaunchRequest::launcher())).unwrap();
    let OpenProjectResult::Opened { window, session } = runtime.open_project(project).unwrap()
    else {
        panic!("first open must create a project");
    };
    let first_save = final_save_request(&runtime, Path::new(project));
    assert!(runtime.is_current_window(window));
    assert!(runtime.is_current_session(session));
    assert_eq!(
        runtime.resolve_final_save(first_save, Ok(())),
        Ok(FinalSaveResolution::Closed(window))
    );
    assert!(!runtime.is_current_window(window));
    assert!(!runtime.is_current_session(session));
    let OpenProjectResult::Opened {
        window: replacement_window,
        session: replacement_session,
    } = runtime.open_project(project).unwrap()
    else {
        panic!("reopen must create a replacement");
    };
    assert_eq!(window.window_id(), replacement_window.window_id());
    assert_ne!(window.generation(), replacement_window.generation());
    assert_eq!(session.session_id(), replacement_session.session_id());
    assert_ne!(session.generation(), replacement_session.generation());
    assert!(!runtime.accepts(window, session));
    assert_eq!(
        runtime.resolve_final_save(first_save, Ok(())),
        Ok(FinalSaveResolution::IgnoredStale)
    );
}

#[test]
fn final_save_failure_retains_the_window_and_reports_the_error() {
    let project = "/tmp/final-save.parchmint";
    let desktop = fixture(AppearanceMode::System, SystemAppearance::Light);
    let runtime = block_on(desktop.bootstrap.start(LaunchRequest::launcher())).unwrap();
    let OpenProjectResult::Opened { window, .. } = runtime.open_project(project).unwrap() else {
        panic!("open must create a project");
    };
    let final_save = final_save_request(&runtime, Path::new(project));

    assert!(runtime.is_current_window(window));
    assert!(desktop.ui.events().contains(&Event::Retained(window)));
    assert_eq!(
        desktop.filesystem.final_save_requests(),
        vec![std::path::PathBuf::from(project)]
    );
    assert_eq!(
        runtime.resolve_final_save(
            final_save,
            Err(ProjectFilesystemError::failed("save", "disk unavailable"))
        ),
        Ok(FinalSaveResolution::SaveFailed)
    );
    assert!(runtime.is_current_window(window));
    assert!(desktop.ui.events().contains(&Event::SaveFailed(window)));
}
