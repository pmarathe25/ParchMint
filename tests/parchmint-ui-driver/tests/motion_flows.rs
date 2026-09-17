use std::{path::PathBuf, time::Duration};

use parchmint_desktop::{
    DesktopInteractionHarness, EditorPane, HarnessDropPosition, HarnessHierarchySurface,
    HarnessKey, HarnessTarget, HarnessWindow, RibbonDestination,
};
use parchmint_ui_driver::{IsolatedRun, create_document, create_group, create_project};

const WINDOW: HarnessWindow = HarnessWindow::Project;

fn capture_root(name: &str) -> Option<PathBuf> {
    let root = std::env::var_os("PARCHMINT_MOTION_FRAMES")?;
    std::env::var("PARCHMINT_MOTION_FILTER")
        .ok()
        .is_none_or(|filter| filter.split(',').any(|part| name.starts_with(part)))
        .then(|| PathBuf::from(root))
}

fn frames(harness: &DesktopInteractionHarness, name: &str) {
    window_frames(harness, WINDOW, name);
}

fn window_frames(harness: &DesktopInteractionHarness, window: HarnessWindow, name: &str) {
    let mut elapsed = 0;
    for delta in [0, 16, 32, 48, 64, 100] {
        elapsed += delta;
        harness
            .advance_motion(window, Duration::from_millis(delta))
            .unwrap();
        if let Some(root) = capture_root(name) {
            harness
                .snapshot(window, root.join(format!("{name}-{elapsed:03}")))
                .unwrap();
        }
    }
}

fn click(harness: &DesktopInteractionHarness, target: HarnessTarget) {
    harness.click_target(WINDOW, target).unwrap();
}

#[test]
fn reduced_motion_shows_the_final_panes_on_the_first_frame() {
    let run = IsolatedRun::new("reduced-motion-frames").unwrap();
    let harness = create_project(&run, &run.root().join("novel.parchmint"), "Reduced motion");
    harness
        .type_into_target(
            WINDOW,
            HarnessTarget::EditorPrimary,
            "The text stays in place.",
        )
        .unwrap();
    click(&harness, HarnessTarget::Ribbon(RibbonDestination::Settings));
    harness.click_text(WINDOW, "Reduce motion").unwrap();
    click(&harness, HarnessTarget::Ribbon(RibbonDestination::Editor));
    harness.advance_motion(WINDOW, Duration::ZERO).unwrap();
    click(&harness, HarnessTarget::ToggleCompanion);
    assert!(harness.editor_panes_share_session().unwrap());
    let output = capture_root("reduced-motion").unwrap_or_else(|| run.root().to_path_buf());
    harness
        .snapshot(WINDOW, output.join("reduced-motion-000"))
        .unwrap();
    harness
        .advance_motion(WINDOW, Duration::from_millis(260))
        .unwrap();
    harness
        .snapshot(WINDOW, output.join("reduced-motion-260"))
        .unwrap();
    assert!(
        std::fs::read(output.join("reduced-motion-000-tiny-skia.png")).unwrap()
            == std::fs::read(output.join("reduced-motion-260-tiny-skia.png")).unwrap(),
        "reduced motion must settle the rendered panes immediately; inspect {}",
        output.display(),
    );
    harness.close(WINDOW).unwrap();
    harness.shutdown().unwrap();
}

#[test]
fn typing_and_undo_survive_interrupted_pane_motion() {
    for appearance in ["Light", "Dark"] {
        let run = IsolatedRun::new("typing-motion").unwrap();
        let harness = create_project(&run, &run.root().join("novel.parchmint"), "Motion input");
        click(&harness, HarnessTarget::Ribbon(RibbonDestination::Settings));
        harness.click_text(WINDOW, appearance).unwrap();
        click(&harness, HarnessTarget::Ribbon(RibbonDestination::Editor));
        harness
            .type_into_target(WINDOW, HarnessTarget::EditorPrimary, "Before motion.")
            .unwrap();
        harness.advance_motion(WINDOW, Duration::ZERO).unwrap();
        click(&harness, HarnessTarget::ToggleCompanion);
        harness
            .advance_motion(WINDOW, Duration::from_millis(48))
            .unwrap();
        assert!(harness.editor_panes_share_session().unwrap());
        harness
            .type_into_target(WINDOW, HarnessTarget::EditorPrimary, " During motion.")
            .unwrap();
        let edited = harness.active_editor_body().unwrap();
        assert!(edited.contains("During motion."));
        harness.press_command_key(WINDOW, 'z').unwrap();
        assert!(
            !harness
                .active_editor_body()
                .unwrap()
                .contains("During motion.")
        );
        if cfg!(target_os = "macos") {
            harness.press_command_shift_key(WINDOW, 'z').unwrap();
        } else {
            harness.press_command_key(WINDOW, 'y').unwrap();
        }
        assert_eq!(harness.active_editor_body().unwrap(), edited);
        click(&harness, HarnessTarget::ToggleCompanion);
        harness
            .advance_motion(WINDOW, Duration::from_millis(16))
            .unwrap();
        click(&harness, HarnessTarget::ToggleCompanion);
        frames(&harness, &format!("typing-interrupted-{appearance}"));
        assert_eq!(harness.active_editor_body().unwrap(), edited);
        assert!(harness.editor_panes_share_session().unwrap());
        harness.elapse_autosave_idle().unwrap();
        harness.close(WINDOW).unwrap();
        harness.shutdown().unwrap();
    }
}

#[test]
fn workspace_transitions_keep_writing_and_controls_available() {
    let run = IsolatedRun::new("workspace-motion").unwrap();
    let harness = create_project(
        &run,
        &run.root().join("novel.parchmint"),
        "The Glass Harbor",
    );
    harness.type_into_target(WINDOW, HarnessTarget::EditorPrimary,
        "Mara arrived at the harbor before dawn. The water held the last stars, and the houses along the quay were still asleep.\nShe carried a letter that no one was meant to read.").unwrap();
    harness.advance_motion(WINDOW, Duration::ZERO).unwrap();
    frames(&harness, "editor");
    click(&harness, HarnessTarget::ToggleCompanion);
    frames(&harness, "split-open");
    for pane in [EditorPane::Primary, EditorPane::Companion] {
        click(&harness, HarnessTarget::PaneFocus(pane));
        frames(&harness, &format!("focus-{pane:?}"));
        click(&harness, HarnessTarget::PaneFocus(pane));
        frames(&harness, &format!("restore-{pane:?}"));
    }
    click(&harness, HarnessTarget::ToggleCompanion);
    harness
        .advance_motion(WINDOW, Duration::from_millis(48))
        .unwrap();
    click(&harness, HarnessTarget::ToggleCompanion);
    frames(&harness, "split-interrupted");
    click(&harness, HarnessTarget::ToggleCompanion);
    frames(&harness, "split-close");
    for target in [
        HarnessTarget::ToggleExplorer,
        HarnessTarget::ToggleInspector,
    ] {
        click(&harness, target);
        frames(&harness, &format!("{target:?}-toggle"));
        click(&harness, target);
        frames(&harness, &format!("{target:?}-restore"));
    }
    click(&harness, HarnessTarget::ExplorerSearch);
    harness.hold_completions().unwrap();
    harness
        .type_into_target(WINDOW, HarnessTarget::GlobalSearchQuery, "harbor")
        .unwrap();
    frames(&harness, "search-pending");
    assert!(harness.text_is_visible(WINDOW, "Searching…").unwrap());
    harness.release_completions(false).unwrap();
    frames(&harness, "search-results");
    harness.click_text(WINDOW, "←  Search").unwrap();
    harness
        .select_editor_text(WINDOW, EditorPane::Primary, "letter")
        .unwrap();
    click(&harness, HarnessTarget::AddComment);
    frames(&harness, "comment-open");
    harness
        .type_into_target(
            WINDOW,
            HarnessTarget::CommentDraft,
            "Reveal the letter's author later.",
        )
        .unwrap();
    harness.click_text(WINDOW, "Add comment").unwrap();
    frames(&harness, "comment-saved");
    harness.press_command_key(WINDOW, 'f').unwrap();
    frames(&harness, "find-open");
    harness
        .type_into_target(
            WINDOW,
            HarnessTarget::LocalFind(EditorPane::Primary),
            "harbor",
        )
        .unwrap();
    harness.click_text(WINDOW, "Replace…").unwrap();
    frames(&harness, "replace-open");
    harness.press_key(WINDOW, HarnessKey::Escape).unwrap();
    frames(&harness, "find-close");
    click(&harness, HarnessTarget::ListMenu);
    frames(&harness, "list-menu");
    harness.press_key(WINDOW, HarnessKey::Escape).unwrap();
    harness.elapse_autosave_idle().unwrap();
    harness.click_text(WINDOW, "The Glass Harbor").unwrap();
    frames(&harness, "project-menu");
    harness.press_key(WINDOW, HarnessKey::Escape).unwrap();
    for destination in [
        RibbonDestination::Cards,
        RibbonDestination::History,
        RibbonDestination::RecentlyDeleted,
        RibbonDestination::Export,
        RibbonDestination::Settings,
    ] {
        click(&harness, HarnessTarget::Ribbon(destination));
        frames(&harness, &format!("screen-{destination:?}"));
        if destination == RibbonDestination::History {
            harness.hold_completions().unwrap();
            harness.click_history_checkpoint(WINDOW, 0).unwrap();
            frames(&harness, "history-pending");
            harness.release_completions(false).unwrap();
            frames(&harness, "history-comparison");
        }
    }
    for category in ["Styles", "Metadata fields", "Dictionaries", "Appearance"] {
        harness.click_text(WINDOW, category).unwrap();
        frames(&harness, &format!("settings-{category}"));
    }
    harness.click_text(WINDOW, "Dark").unwrap();
    click(&harness, HarnessTarget::Ribbon(RibbonDestination::Editor));
    frames(&harness, "dark-editor");
    click(&harness, HarnessTarget::ToggleCompanion);
    frames(&harness, "dark-split");
    assert!(
        harness
            .active_editor_body()
            .unwrap()
            .contains("The water held the last stars")
    );
    let document = harness
        .active_editor_document_id(EditorPane::Primary)
        .unwrap();
    harness
        .begin_tab_drag(WINDOW, EditorPane::Primary, document.clone())
        .unwrap();
    for (index, point) in [(0.15, 0.1), (0.6, 0.25), (0.8, 0.6)]
        .into_iter()
        .enumerate()
    {
        harness
            .move_pointer_to_target(WINDOW, HarnessTarget::EditorCompanion, point)
            .unwrap();
        frames(&harness, &format!("tab-drag-{index}"));
    }
    harness.release_hierarchy_drag(WINDOW).unwrap();
    frames(&harness, "tab-drop");
    assert!(
        harness
            .editor_tab_is_visible(WINDOW, EditorPane::Companion, document.clone())
            .unwrap()
    );
    assert!(
        !harness
            .editor_tab_is_visible(WINDOW, EditorPane::Primary, document)
            .unwrap()
    );
    harness.close(WINDOW).unwrap();
    harness.shutdown().unwrap();
}

#[test]
fn overview_dragging_and_disclosures_remain_stable_between_frames() {
    let run = IsolatedRun::new("overview-motion").unwrap();
    let harness = create_project(
        &run,
        &run.root().join("novel.parchmint"),
        "The Glass Harbor",
    );
    for group in ["Act One", "Act Two"] {
        create_group(&harness, "Manuscript", group);
    }
    for title in ["Arrival", "The letter", "Discovery", "Departure"] {
        create_document(&harness, "Act One", title);
    }
    click(&harness, HarnessTarget::Ribbon(RibbonDestination::Cards));
    let act_one = harness.hierarchy_node("Act One").unwrap();
    let arrival = harness.hierarchy_node("Arrival").unwrap();
    let departure = harness.hierarchy_node("Departure").unwrap();
    let act_two = harness.hierarchy_node("Act Two").unwrap();
    harness.advance_motion(WINDOW, Duration::ZERO).unwrap();
    frames(&harness, "overview");
    for (name, destination, position) in [
        ("card-reorder", departure, HarnessDropPosition::After),
        ("card-move-group", act_two, HarnessDropPosition::Into),
    ] {
        let original = harness.hierarchy_titles().unwrap();
        harness
            .preview_hierarchy_move(
                WINDOW,
                HarnessHierarchySurface::Cards,
                arrival.clone(),
                destination,
                position,
            )
            .unwrap();
        let preview = harness.preview_hierarchy_titles().unwrap();
        frames(&harness, name);
        assert_eq!(harness.preview_hierarchy_titles().unwrap(), preview);
        harness.press_key(WINDOW, HarnessKey::Escape).unwrap();
        frames(&harness, &format!("{name}-cancel"));
        harness.release_hierarchy_drag(WINDOW).unwrap();
        assert_eq!(harness.hierarchy_titles().unwrap(), original);
    }
    harness.toggle_cards_group(WINDOW, act_one.clone()).unwrap();
    frames(&harness, "group-collapse");
    harness.toggle_cards_group(WINDOW, act_one).unwrap();
    frames(&harness, "group-expand");
    harness.click_text(WINDOW, "What happens here?").unwrap();
    frames(&harness, "synopsis-edit");
    harness.type_focused(WINDOW, "A sealed letter arrives at the harbor. Mara must decide whether to open it before the tide carries the sender away.").unwrap();
    frames(&harness, "synopsis-typed");
    harness.press_key(WINDOW, HarnessKey::Escape).unwrap();
    frames(&harness, "synopsis-display");
    harness.right_click_cards_node(WINDOW, arrival).unwrap();
    frames(&harness, "card-context");
    harness.press_key(WINDOW, HarnessKey::Escape).unwrap();
    harness.close(WINDOW).unwrap();
    harness.shutdown().unwrap();
}

#[test]
fn delayed_actions_and_notifications_leave_the_workspace_usable() {
    use parchmint_desktop::{ProductionFaultKind, ProductionFaultPoint};
    let run = IsolatedRun::new("feedback-motion").unwrap();
    let harness = create_project(&run, &run.root().join("novel.parchmint"), "Feedback review");
    harness
        .type_into_target(WINDOW, HarnessTarget::EditorPrimary, "The harbor at dawn.")
        .unwrap();
    harness.advance_motion(WINDOW, Duration::ZERO).unwrap();
    click(&harness, HarnessTarget::Ribbon(RibbonDestination::History));
    harness
        .advance_motion(WINDOW, Duration::from_millis(260))
        .unwrap();
    harness
        .type_into(WINDOW, "Milestone name", "First draft")
        .unwrap();
    harness.fail_next(ProductionFaultPoint::History, ProductionFaultKind::Io);
    let error = harness.click_text(WINDOW, "Create milestone").unwrap_err();
    assert!(error.to_string().contains("History"));
    frames(&harness, "error-dialog");
    click(&harness, HarnessTarget::ModalCancel);
    frames(&harness, "error-toast");
    harness.click_text(WINDOW, "Notifications 1").unwrap();
    frames(&harness, "notification-drawer");
    harness.click_text(WINDOW, "Close").unwrap();
    harness.elapse_notifications().unwrap();
    frames(&harness, "toast-expire");
    harness.click_text(WINDOW, "Create milestone").unwrap();
    assert!(
        harness
            .history_checkpoints()
            .unwrap()
            .iter()
            .any(|checkpoint| checkpoint.label == "First draft")
    );
    click(&harness, HarnessTarget::Ribbon(RibbonDestination::Export));
    harness
        .advance_motion(WINDOW, Duration::from_millis(260))
        .unwrap();
    let output = run.root().join("manuscript.html");
    harness.set_next_path_selection(&output);
    click(&harness, HarnessTarget::ExportBrowse);
    harness.hold_completions().unwrap();
    click(&harness, HarnessTarget::ExportStart);
    frames(&harness, "export-pending");
    harness.release_completions(false).unwrap();
    frames(&harness, "export-complete");
    assert!(
        std::fs::read_to_string(output)
            .unwrap()
            .contains("The harbor at dawn.")
    );
    harness.close(WINDOW).unwrap();
    harness.shutdown().unwrap();
}

#[test]
fn project_creation_and_recovery_show_their_pending_states() {
    use parchmint_desktop::LaunchRequest;
    let run = IsolatedRun::new("startup-motion").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = DesktopInteractionHarness::launch(run.root(), LaunchRequest::launcher()).unwrap();
    let launcher = HarnessWindow::Launcher;
    harness.advance_motion(launcher, Duration::ZERO).unwrap();
    window_frames(&harness, launcher, "launcher");
    harness.click_text(launcher, "Create Project").unwrap();
    window_frames(&harness, launcher, "project-form");
    harness
        .type_into(launcher, "Project title", "A new beginning")
        .unwrap();
    harness
        .type_into(launcher, "Project destination", project.to_string_lossy())
        .unwrap();
    harness.hold_completions().unwrap();
    harness.click_text(launcher, "Create and Open").unwrap();
    window_frames(&harness, launcher, "project-create-pending");
    harness.release_completions(false).unwrap();
    frames(&harness, "project-opened");
    harness
        .type_into_target(
            WINDOW,
            HarnessTarget::EditorPrimary,
            "A sentence protected by recovery.",
        )
        .unwrap();
    harness.elapse_recovery_capture().unwrap();
    harness.abandon().unwrap();
    let reopened =
        DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project)).unwrap();
    reopened.advance_motion(WINDOW, Duration::ZERO).unwrap();
    assert!(
        reopened
            .text_is_visible(WINDOW, "Unsaved changes found")
            .unwrap()
    );
    frames(&reopened, "recovery");
    reopened.hold_completions().unwrap();
    reopened.click_text(WINDOW, "Recover changes").unwrap();
    frames(&reopened, "recovery-pending");
    reopened.release_completions(false).unwrap();
    frames(&reopened, "recovery-complete");
    assert!(
        reopened
            .active_editor_body()
            .unwrap()
            .contains("A sentence protected by recovery.")
    );
    reopened.close(WINDOW).unwrap();
    reopened.shutdown().unwrap();
}
