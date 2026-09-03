//! Public contracts for the framework-neutral desktop shell.

use parchmint_platform_api::WindowCapability;
use parchmint_preferences::{AppearanceMode, ResolvedAppearance};
use parchmint_ui_iced::{
    DialogKind, F6Region, FocusTarget, InspectorSection, LauncherState, MenuKind, PaneGeometry,
    RibbonDestination, SUPPORTED_APPEARANCES, Shell, ShellLayout, ShellMessage, ShellTask,
    TaskCompletion,
};

#[test]
fn window_messages_and_completions_require_the_exact_live_request() {
    let live = WindowCapability::new(12, 4);
    let stale = WindowCapability::new(12, 3);
    let mut shell = Shell::new(live);

    assert!(shell.accept(ShellMessage::OpenMenu {
        window: live,
        menu: MenuKind::Application,
    }));
    assert_eq!(shell.open_menu(), Some(MenuKind::Application));
    assert!(!shell.accept(ShellMessage::CloseMenu { window: stale }));
    assert_eq!(shell.open_menu(), Some(MenuKind::Application));

    assert!(!shell.accept_completion(TaskCompletion::new(
        stale,
        ShellTask::LoadRecentProjects,
        Ok(()),
    )));
    assert!(!shell.accept_completion(TaskCompletion::new(live, ShellTask::CreateProject, Ok(()),)));
    assert!(shell.accept_completion(TaskCompletion::new(
        live,
        ShellTask::LoadRecentProjects,
        Ok(()),
    )));
}

#[test]
fn launcher_starts_empty_and_keeps_recent_project_metadata() {
    let mut launcher = LauncherState::default();

    assert!(launcher.is_visible());
    assert!(launcher.recent_projects().is_empty());
    assert_eq!(launcher.new_project().language(), "en-US");
    assert_eq!(launcher.new_project().focus(), FocusTarget::ProjectName);

    launcher.add_recent_project("Novel", "/work/novel.parchment", "2026-08-08T10:00:00Z");
    let project = launcher.recent_projects().first().expect("recent project");
    assert_eq!(project.name(), "Novel");
    assert_eq!(project.path(), "/work/novel.parchment");
    assert_eq!(project.last_opened(), "2026-08-08T10:00:00Z");
}

#[test]
fn geometry_clamps_sidebars_and_scales_without_clipping() {
    assert_eq!(ShellLayout::MIN_WINDOW_SIZE, (1280, 720));

    let mut layout = ShellLayout::for_window(1920, 1080);
    layout.resize_explorer(400);
    layout.resize_inspector(500);
    assert_eq!(layout.ribbon().height(), ShellLayout::RIBBON_HEIGHT);
    assert_eq!(layout.status_bar().height(), ShellLayout::STATUS_BAR_HEIGHT);
    assert_eq!(layout.inspector().width(), PaneGeometry::MAX_SIDEBAR_WIDTH);
    assert!(layout.explorer().width() >= PaneGeometry::MIN_SIDEBAR_WIDTH);

    let baseline = ShellLayout::for_window_at_scale(1440, 900, 1.0);
    for scale in [1.25, 1.5, 2.0] {
        let scaled = ShellLayout::for_window_at_scale(1440, 900, scale);
        assert!(scaled.hit_target_minimum() >= baseline.hit_target_minimum());
        assert!(scaled.ribbon().height() >= baseline.ribbon().height());
        assert!(scaled.status_bar().height() >= baseline.status_bar().height());
        assert!(scaled.has_no_clipped_controls());
    }
}

#[test]
fn pane_visibility_and_restored_widths_remain_explicit_workspace_state() {
    let mut layout = ShellLayout::for_window(1440, 900);
    layout.restore_panes(336, 412, false, true);

    assert_eq!(layout.explorer_width(), 336);
    assert_eq!(layout.inspector_width(), 412);
    assert!(!layout.explorer_is_visible());
    assert!(layout.inspector_is_visible());
    assert_eq!(layout.explorer().width(), 0);
    assert_eq!(layout.inspector().width(), 412);

    layout.resize_window(1280, 720);
    layout.set_explorer_visible(true);
    assert!(layout.has_no_clipped_controls());
}

#[test]
fn navigation_focus_dialog_and_menu_states_are_consistent() {
    let window = WindowCapability::new(3, 1);
    let mut shell = Shell::new(window);

    assert!(shell.inspector_section_is_expanded(InspectorSection::Synopsis));
    assert!(shell.inspector_section_is_expanded(InspectorSection::Metadata));
    assert!(shell.inspector_section_is_expanded(InspectorSection::Comments));

    assert!(shell.accept(ShellMessage::OpenMenu {
        window,
        menu: MenuKind::Project,
    }));
    assert_eq!(shell.open_menu(), Some(MenuKind::Project));
    assert!(shell.accept(ShellMessage::CloseMenu { window }));
    assert_eq!(shell.open_menu(), None);

    shell.select_destination(RibbonDestination::Cards);
    assert_eq!(shell.destination(), RibbonDestination::Cards);
    shell.open_global_search();
    assert!(shell.global_search_is_open());
    shell.select_destination(RibbonDestination::GlobalSearch);
    assert_eq!(shell.destination(), RibbonDestination::Cards);

    shell.toggle_inspector_section(InspectorSection::Synopsis);
    shell.focus(FocusTarget::EditorDocument("chapter-1".into()));
    assert!(!shell.inspector_section_is_expanded(InspectorSection::Synopsis));
    assert!(shell.comments_are_available());

    shell.toggle_inspector_section(InspectorSection::Comments);
    assert!(!shell.inspector_section_is_expanded(InspectorSection::Comments));
    shell.expand_inspector_section(InspectorSection::Comments);
    assert!(shell.inspector_section_is_expanded(InspectorSection::Comments));
    shell.expand_inspector_section(InspectorSection::Comments);
    assert!(shell.inspector_section_is_expanded(InspectorSection::Comments));

    shell.focus_next_region();
    assert_eq!(shell.focus_region(), F6Region::Inspector);
    assert!(shell.has_visible_focus());

    shell.focus(FocusTarget::NewProjectAction);
    shell.open_dialog(DialogKind::CreateProject);
    assert_eq!(shell.dialog_kind(), Some(DialogKind::CreateProject));
    assert!(shell.focus_is_trapped());
    assert_eq!(shell.focus_target(), FocusTarget::ProjectName);
    shell.dismiss_dialog();
    assert_eq!(shell.focus_target(), FocusTarget::NewProjectAction);
}

#[test]
fn appearance_changes_propagate_to_every_open_window() {
    assert_eq!(
        SUPPORTED_APPEARANCES,
        &[
            AppearanceMode::System,
            AppearanceMode::Light,
            AppearanceMode::Dark,
        ]
    );

    let mut windows = Shell::windows();
    for window in [WindowCapability::new(9, 1), WindowCapability::new(2, 1)] {
        windows.insert(window, Shell::new(window));
    }
    windows.set_system_appearance(ResolvedAppearance::Dark);
    assert!(
        windows
            .values()
            .all(|shell| shell.resolved_appearance() == ResolvedAppearance::Dark)
    );

    windows.set_appearance(AppearanceMode::Light);
    assert_eq!(windows.appearance(), AppearanceMode::Light);
    assert!(
        windows
            .values()
            .all(|shell| shell.resolved_appearance() == ResolvedAppearance::Light)
    );
}

#[test]
fn issuing_a_task_does_not_wait_and_only_the_latest_completion_applies() {
    let mut shell = Shell::new(WindowCapability::new(7, 1));
    let first = shell.begin_task(ShellTask::OpenProject);
    let second = shell.begin_task(ShellTask::OpenProject);

    assert_eq!(first.window(), shell.window());
    assert_eq!(first.task(), ShellTask::OpenProject);
    assert!(second.request() > first.request());
    assert!(!shell.accept_completion(TaskCompletion::for_ticket(first, Ok(()))));
    assert!(shell.accept_completion(TaskCompletion::for_ticket(second, Ok(()))));
}
