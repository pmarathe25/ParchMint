use parchmint_desktop::{
    DesktopInteractionHarness, EditorPane, HarnessDropPosition, HarnessHierarchySurface,
    HarnessKey, HarnessTarget, HarnessWindow, LaunchRequest, RibbonDestination,
};
use parchmint_ui_driver::{IsolatedRun, create_document, create_group, create_project};
const WINDOW: HarnessWindow = HarnessWindow::Project;

fn open_formatting(harness: &DesktopInteractionHarness) {
    if harness
        .target_is_visible(WINDOW, HarnessTarget::FormattingMenu)
        .unwrap()
    {
        harness
            .click_target(WINDOW, HarnessTarget::FormattingMenu)
            .unwrap();
    }
}

fn capture(harness: &DesktopInteractionHarness, name: &str) {
    if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
        harness
            .snapshot(WINDOW, std::path::PathBuf::from(root).join(name))
            .unwrap();
    }
}

#[test]
fn first_launch_is_a_writing_workspace_and_companion_starts_empty() {
    let run = IsolatedRun::new("workspace-startup").unwrap();
    let harness = DesktopInteractionHarness::launch(run.root(), LaunchRequest::launcher()).unwrap();
    assert!(harness.has_window(WINDOW).unwrap());
    assert!(!harness.has_window(HarnessWindow::Launcher).unwrap());
    assert!(
        !harness
            .text_is_visible(WINDOW, "No comments in this document")
            .unwrap()
    );
    harness
        .type_into_target(WINDOW, HarnessTarget::EditorPrimary, "The first line.")
        .unwrap();
    harness
        .click_target(WINDOW, HarnessTarget::ToggleCompanion)
        .unwrap();
    assert_eq!(harness.active_editor_tab_title().unwrap(), "Untitled");
    assert!(!harness.editor_panes_share_session().unwrap_or(false));
    capture(&harness, "empty-companion");
    open_formatting(&harness);
    assert!(harness.text_is_visible(WINDOW, "15").unwrap());
    harness.press_key(WINDOW, HarnessKey::Escape).unwrap();
    harness
        .click_target(WINDOW, HarnessTarget::ParagraphStyle)
        .unwrap();
    harness.click_text(WINDOW, "Heading 1").unwrap();
    open_formatting(&harness);
    assert!(harness.text_is_visible(WINDOW, "24").unwrap());
    harness
        .type_into_target(WINDOW, HarnessTarget::EditorCompanion, "A separate note.")
        .unwrap();
    assert!(
        !harness
            .active_editor_body()
            .unwrap()
            .contains("The first line.")
    );
    assert!(
        harness
            .active_editor_body()
            .unwrap()
            .contains("data-style-id=\"heading-1\"")
    );
    harness
        .click_target(WINDOW, HarnessTarget::NewTab(EditorPane::Companion))
        .unwrap();
    open_formatting(&harness);
    assert!(harness.text_is_visible(WINDOW, "15").unwrap());
    harness.press_key(WINDOW, HarnessKey::Escape).unwrap();
    assert!(!harness.text_is_visible(WINDOW, "24").unwrap());
    harness
        .type_into_target(
            WINDOW,
            HarnessTarget::EditorCompanion,
            "A fresh body paragraph.",
        )
        .unwrap();
    open_formatting(&harness);
    assert!(harness.text_is_visible(WINDOW, "15").unwrap());
    harness.press_key(WINDOW, HarnessKey::Escape).unwrap();
    harness
        .click_target(WINDOW, HarnessTarget::EditorPrimary)
        .unwrap();
    assert!(
        harness
            .active_editor_body()
            .unwrap()
            .contains("The first line.")
    );
    harness.close(WINDOW).unwrap();
    harness.shutdown().unwrap();
}

#[test]
fn link_toolbar_opens_after_promoting_an_empty_tab() {
    let run = IsolatedRun::new("scratch-link").unwrap();
    let harness = DesktopInteractionHarness::launch(run.root(), LaunchRequest::launcher()).unwrap();
    harness
        .click_target(WINDOW, HarnessTarget::NewTab(EditorPane::Primary))
        .unwrap();
    harness.click_target(WINDOW, HarnessTarget::Link).unwrap();
    assert!(harness.text_is_visible(WINDOW, "Link destination").unwrap());
    harness.click_text(WINDOW, "Cancel").unwrap();
    harness.close(WINDOW).unwrap();
    harness.shutdown().unwrap();
}

#[test]
fn paragraph_controls_format_selection_and_future_typing_and_reopen() {
    let run = IsolatedRun::new("paragraph-toolbar").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Paragraph controls");
    harness
        .type_into_target(WINDOW, HarnessTarget::EditorPrimary, "The harbor at dawn.")
        .unwrap();
    open_formatting(&harness);
    assert!(harness.text_is_visible(WINDOW, "15").unwrap());
    harness.press_key(WINDOW, HarnessKey::Escape).unwrap();
    harness
        .select_editor_text(WINDOW, EditorPane::Primary, "harbor")
        .unwrap();
    harness
        .click_target(WINDOW, HarnessTarget::ParagraphStyle)
        .unwrap();
    harness.click_text(WINDOW, "Heading 1").unwrap();
    assert!(
        harness
            .active_editor_body()
            .unwrap()
            .contains("data-style-id=\"heading-1\"")
    );
    open_formatting(&harness);
    assert!(harness.text_is_visible(WINDOW, "24").unwrap());
    harness
        .click_target(WINDOW, HarnessTarget::Alignment)
        .unwrap();
    for _ in 0..2 {
        harness.press_key(WINDOW, HarnessKey::ArrowDown).unwrap();
    }
    harness.press_key(WINDOW, HarnessKey::Enter).unwrap();
    harness
        .click_target(WINDOW, HarnessTarget::LineSpacing)
        .unwrap();
    for _ in 0..4 {
        harness.press_key(WINDOW, HarnessKey::ArrowDown).unwrap();
    }
    harness.press_key(WINDOW, HarnessKey::Enter).unwrap();
    let body = harness.active_editor_body().unwrap();
    assert!(body.contains("data-alignment=\"center\""), "{body}");
    assert!(body.contains("data-line-spacing=\"200\""), "{body}");
    harness.press_key(WINDOW, HarnessKey::ArrowRight).unwrap();
    harness.type_focused(WINDOW, "MARKER").unwrap();
    capture(&harness, "paragraph-formatting");
    harness
        .click_target(WINDOW, HarnessTarget::ParagraphStyle)
        .unwrap();
    harness
        .click_target(WINDOW, HarnessTarget::ManageStyles)
        .unwrap();
    capture(&harness, "styles-manager-light");
    harness.click_text(WINDOW, "Done").unwrap();
    let body = harness.active_editor_body().unwrap();
    assert!(body.contains("harborMARKER"), "{body}");
    harness.close(WINDOW).unwrap();
    harness.shutdown().unwrap();
    let reopened =
        DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project)).unwrap();
    assert_eq!(reopened.active_editor_body().unwrap(), body);
    open_formatting(&reopened);
    assert!(reopened.text_is_visible(WINDOW, "24").unwrap());
    reopened.close(WINDOW).unwrap();
    reopened.shutdown().unwrap();
}

#[test]
fn overview_groups_collapse_and_managers_are_contextual() {
    let run = IsolatedRun::new("overview-disclosures").unwrap();
    let harness = create_project(&run, &run.root().join("novel.parchmint"), "Overview review");
    create_group(&harness, "Manuscript", "Act One");
    for title in [
        "Arrival",
        "Letter",
        "Discovery",
        "Departure",
        "Aftermath",
        "Home",
    ] {
        create_document(&harness, "Act One", title);
    }
    let original_order = harness.hierarchy_titles().unwrap();
    harness
        .preview_hierarchy_move(
            WINDOW,
            HarnessHierarchySurface::Explorer,
            harness.hierarchy_node("Arrival").unwrap(),
            harness.hierarchy_node("Departure").unwrap(),
            HarnessDropPosition::After,
        )
        .unwrap();
    capture(&harness, "explorer-drag-ghost");
    harness.press_key(WINDOW, HarnessKey::Escape).unwrap();
    harness.release_hierarchy_drag(WINDOW).unwrap();
    assert_eq!(harness.hierarchy_titles().unwrap(), original_order);
    harness
        .click_target(WINDOW, HarnessTarget::Ribbon(RibbonDestination::Cards))
        .unwrap();
    let group = harness.hierarchy_node("Act One").unwrap();
    let arrival = harness.hierarchy_node("Arrival").unwrap();
    assert!(
        harness
            .cards_node_is_visible(WINDOW, arrival.clone())
            .unwrap()
    );
    harness.toggle_cards_group(WINDOW, group.clone()).unwrap();
    assert!(
        !harness
            .cards_node_is_visible(WINDOW, arrival.clone())
            .unwrap()
    );
    capture(&harness, "overview-collapsed");
    harness.toggle_cards_group(WINDOW, group).unwrap();
    assert!(harness.cards_node_is_visible(WINDOW, arrival).unwrap());
    capture(&harness, "overview-expanded");
    let mut preview_order = None;
    for (name, destination, position) in [
        (
            "overview-slot-after-row",
            "Departure",
            HarnessDropPosition::After,
        ),
        (
            "overview-slot-before-row",
            "Aftermath",
            HarnessDropPosition::Before,
        ),
    ] {
        harness
            .preview_hierarchy_move(
                WINDOW,
                HarnessHierarchySurface::Cards,
                harness.hierarchy_node("Arrival").unwrap(),
                harness.hierarchy_node(destination).unwrap(),
                position,
            )
            .unwrap();
        let order = harness.preview_hierarchy_titles().unwrap();
        if let Some(previous) = &preview_order {
            assert_eq!(
                &order, previous,
                "both row edges address the same insertion slot"
            );
        }
        preview_order = Some(order);
        harness
            .advance_motion(WINDOW, std::time::Duration::from_millis(260))
            .unwrap();
        capture(&harness, name);
        harness.press_key(WINDOW, HarnessKey::Escape).unwrap();
        harness.release_hierarchy_drag(WINDOW).unwrap();
        harness
            .advance_motion(WINDOW, std::time::Duration::from_millis(260))
            .unwrap();
        assert_eq!(harness.hierarchy_titles().unwrap(), original_order);
    }
    harness
        .click_target(WINDOW, HarnessTarget::ManageMetadata)
        .unwrap();
    harness.click_text(WINDOW, "+ New field").unwrap();
    harness
        .type_into_target(WINDOW, HarnessTarget::MetadataFieldName, "Viewpoint")
        .unwrap();
    harness.click_text(WINDOW, "Add field").unwrap();
    capture(&harness, "metadata-manager-light");
    harness.click_text(WINDOW, "Done").unwrap();
    harness
        .click_target(WINDOW, HarnessTarget::Ribbon(RibbonDestination::Settings))
        .unwrap();
    assert!(!harness.contains_text(WINDOW, "Styles").unwrap());
    assert!(!harness.contains_text(WINDOW, "Metadata fields").unwrap());
    harness.close(WINDOW).unwrap();
    harness.shutdown().unwrap();
}

#[test]
fn navigation_destinations_share_the_vertical_rail() {
    let run = IsolatedRun::new("application-navigation").unwrap();
    let project = run.root().join("navigation.parchmint");
    let harness = create_project(&run, &project, "Navigation");
    let export = HarnessTarget::Ribbon(RibbonDestination::Export);
    assert!(harness.target_is_visible(WINDOW, export).unwrap());
    for destination in [
        RibbonDestination::Export,
        RibbonDestination::History,
        RibbonDestination::Settings,
    ] {
        assert!(
            harness
                .target_is_visible(WINDOW, HarnessTarget::Ribbon(destination))
                .unwrap()
        );
    }
    capture(&harness, "application-menu");
    harness.click_target(WINDOW, export).unwrap();
    assert!(
        harness
            .text_is_visible(WINDOW, "Export manuscript")
            .unwrap()
    );
    harness
        .click_target(WINDOW, HarnessTarget::Ribbon(RibbonDestination::Cards))
        .unwrap();
    harness
        .click_target(WINDOW, HarnessTarget::Ribbon(RibbonDestination::Editor))
        .unwrap();
    assert!(
        harness
            .target_is_visible(WINDOW, HarnessTarget::EditorPrimary)
            .unwrap()
    );
    assert!(harness.target_is_visible(WINDOW, export).unwrap());
    harness.close(WINDOW).unwrap();
    harness.shutdown().unwrap();
}
