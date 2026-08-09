//! Requirements-first tests for the framework-neutral desktop UI boundary.

use std::sync::Arc;

use parchmint_application::{GlobalReplacement, ProjectCommandDispatcher};
use parchmint_editor_api::EditorAdapter;
use parchmint_platform_api::{
    ApplicationPathService, ClipboardService, DialogService, ExternalOpenService, MenuService,
    SystemAppearanceService, WindowCapability,
};
use parchmint_preferences::{
    AppearanceService, PreferenceService, ResolvedAppearance, ThemeSnapshot,
};
use parchmint_spellcheck_api::SpellcheckService;
use parchmint_ui_api::{
    ApplicationServices, DesktopUi, ExitCode, PlatformServices, ProjectSessionRegistry, UiError,
    UiPorts, UiStartup, apply_appearance_events,
};
use parchmint_workspace_state::WorkspaceStateStore;

#[test]
fn fake_ui_accepts_the_framework_neutral_startup_and_service_contract() {
    struct FakeUi;

    impl DesktopUi for FakeUi {
        fn run(self: Box<Self>, _: UiStartup, _: UiPorts) -> Result<ExitCode, UiError> {
            Ok(ExitCode::SUCCESS)
        }
    }

    let startup = UiStartup {
        appearance: ThemeSnapshot::new(ResolvedAppearance::Dark, 17),
        sessions: ProjectSessionRegistry::new(),
        initial_project: None,
    };
    assert_eq!(startup.appearance.generation, 17);
    assert!(startup.initial_project.is_none());
    let _: Box<dyn DesktopUi> = Box::new(FakeUi);
    let _: fn(Box<dyn DesktopUi>, UiStartup, UiPorts) -> Result<ExitCode, UiError> = DesktopUi::run;

    fn assert_service<T: Send + Sync + ?Sized>() {}

    assert_service::<dyn ProjectCommandDispatcher>();
    assert_service::<dyn GlobalReplacement>();
    assert_service::<dyn EditorAdapter>();
    assert_service::<dyn SpellcheckService>();
    assert_service::<dyn MenuService>();
    assert_service::<dyn DialogService>();
    assert_service::<dyn ClipboardService>();
    assert_service::<dyn ExternalOpenService>();
    assert_service::<dyn ApplicationPathService>();
    assert_service::<dyn SystemAppearanceService>();
    assert_service::<dyn PreferenceService>();
    assert_service::<dyn AppearanceService>();
    assert_service::<dyn WorkspaceStateStore>();

    fn application_services(
        commands: Arc<dyn ProjectCommandDispatcher>,
        replacements: Arc<dyn GlobalReplacement>,
    ) -> ApplicationServices {
        ApplicationServices::new(commands, replacements)
    }

    fn platform_services(
        menus: Arc<dyn MenuService>,
        dialogs: Arc<dyn DialogService>,
        clipboard: Arc<dyn ClipboardService>,
        external_open: Arc<dyn ExternalOpenService>,
        application_paths: Arc<dyn ApplicationPathService>,
        system_appearance: Arc<dyn SystemAppearanceService>,
    ) -> PlatformServices {
        PlatformServices::new(
            menus,
            dialogs,
            clipboard,
            external_open,
            application_paths,
            system_appearance,
        )
    }

    fn ui_ports(
        application: ApplicationServices,
        editor: Arc<dyn EditorAdapter>,
        spellcheck: Arc<dyn SpellcheckService>,
        platform: PlatformServices,
        preferences: Arc<dyn PreferenceService>,
        appearance: Arc<dyn AppearanceService>,
        workspace_state: Arc<dyn WorkspaceStateStore>,
    ) -> UiPorts {
        UiPorts::new(
            application,
            editor,
            spellcheck,
            platform,
            preferences,
            appearance,
            workspace_state,
        )
    }

    let _ = (application_services, platform_services, ui_ports);
}

#[test]
fn project_sessions_reject_stale_generations_after_recreation() {
    let mut sessions = ProjectSessionRegistry::new();
    let first = sessions.register(12);

    assert_eq!((first.session_id(), first.generation()), (12, 1));
    assert!(sessions.retire(first));
    assert!(!sessions.is_current(first));

    let replacement = sessions.register(12);
    assert_eq!(
        (replacement.session_id(), replacement.generation()),
        (12, 2)
    );
    assert!(sessions.is_current(replacement));
    assert!(!sessions.retire(first));
}

#[test]
fn appearance_events_apply_each_generation_in_window_id_order() {
    let snapshots = [
        ThemeSnapshot::new(ResolvedAppearance::Light, 3),
        ThemeSnapshot::new(ResolvedAppearance::Dark, 4),
    ];
    let mut applied = Vec::new();

    apply_appearance_events(
        &snapshots,
        &[
            WindowCapability::new(9, 5),
            WindowCapability::new(2, 8),
            WindowCapability::new(7, 3),
        ],
        |window, snapshot| {
            applied.push((snapshot.generation, window.window_id(), window.generation()));
        },
    );

    assert_eq!(
        applied,
        [
            (3, 2, 8),
            (3, 7, 3),
            (3, 9, 5),
            (4, 2, 8),
            (4, 7, 3),
            (4, 9, 5),
        ]
    );
}

#[test]
fn ui_api_source_contains_no_widget_or_window_framework_types() {
    let source = [include_str!("../src/lib.rs"), include_str!("../Cargo.toml")]
        .join("\n")
        .to_ascii_lowercase();
    for forbidden in ["iced", "winit", "gtk", "tauri", "raw_window_handle"] {
        assert!(
            !source.contains(forbidden),
            "found forbidden type family: {forbidden}"
        );
    }
}
