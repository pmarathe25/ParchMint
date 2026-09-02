use std::path::Path;

use parchmint_desktop::{
    DesktopInteractionHarness, EditorPane, HarnessDropPosition, HarnessHierarchySurface,
    HarnessTarget, HarnessWindow, LaunchRequest, RibbonDestination,
};
use parchmint_ui_driver::IsolatedRun;

/// Exercises the high-cardinality authoring paths with deterministic state
/// checks.  In particular, this guards against unbounded preview-tab growth
/// and route/resize handlers that lose the mounted project state.
#[test]
fn authoring_stays_bounded_during_repeated_navigation_and_revision() {
    let run = IsolatedRun::new("adversarial-author-resilience").expect("isolated run");
    let project = run.root().join("adversarial-author-resilience.parchmint");
    let harness = create_project(&run, &project, "Adversarial Author Resilience");
    let _ = harness.take_diagnostics();

    create_group(&harness, "Manuscript", "Scenes");
    create_group(&harness, "Manuscript", "Archive");
    for index in 0..12 {
        create_document(
            &harness,
            "Scenes",
            &format!("Scene {index:02} research marker"),
        );
    }

    let hierarchy = harness.hierarchy_titles().expect("read sizable hierarchy");
    assert_eq!(
        hierarchy
            .iter()
            .filter(|title| title.starts_with("Scene "))
            .count(),
        12,
        "all created documents should remain addressable"
    );

    // New documents begin as permanent tabs. Remove those creation tabs to
    // ensure the high-count loop below genuinely stresses preview reuse.
    for index in 0..12 {
        let node = harness
            .hierarchy_node(&format!("Scene {index:02} research marker"))
            .expect("resolve permanent creation tab");
        harness
            .click_hierarchy_node(HarnessWindow::Project, node)
            .expect("focus permanent creation tab before closing it");
        let document_id = harness
            .active_editor_document_id(EditorPane::Primary)
            .expect("read permanent creation tab identity");
        harness
            .close_editor_tab(HarnessWindow::Project, EditorPane::Primary, document_id)
            .expect("close permanent creation tab");
    }
    let baseline_tabs = harness.tab_titles().expect("read initial tab strip").len();

    // Repeated Explorer selection must reuse one temporary preview tab.
    for index in 0..12 {
        let node = harness
            .hierarchy_node(&format!("Scene {index:02} research marker"))
            .expect("resolve scene preview node");
        harness
            .click_hierarchy_node(HarnessWindow::Project, node)
            .expect("select scene preview");
        assert_eq!(
            harness.tab_titles().expect("read preview tab strip").len(),
            baseline_tabs + 1,
            "preview selection must stay bounded at one temporary tab"
        );
    }

    // Promote one tab, then revisit all scenes again.  The permanent tab must
    // survive while the temporary preview continues to be replaced.
    harness
        .right_click_text(HarnessWindow::Project, "Scene 11 research marker")
        .expect("open final scene context menu");
    harness
        .click_text(HarnessWindow::Project, "Open")
        .expect("promote final scene preview");
    let promoted_tabs = harness.tab_titles().expect("read promoted tab strip").len();
    assert_eq!(promoted_tabs, baseline_tabs + 1);
    for index in 0..12 {
        let node = harness
            .hierarchy_node(&format!("Scene {index:02} research marker"))
            .expect("resolve repeated scene node");
        harness
            .click_hierarchy_node(HarnessWindow::Project, node)
            .expect("reselect scene preview");
        assert!(
            harness.tab_titles().expect("read repeated tab strip").len() <= promoted_tabs + 1,
            "temporary previews must not accumulate"
        );
    }

    // Route through each workspace repeatedly while delivering a range of
    // responsive window sizes.  Every route remains observable afterwards.
    for (width, height) in [(1280.0, 720.0), (1280.0, 900.0), (1440.0, 900.0)] {
        harness
            .resize(HarnessWindow::Project, width, height)
            .expect("resize project window");
        let destinations = &[
            RibbonDestination::Cards,
            RibbonDestination::Editor,
            RibbonDestination::Settings,
            RibbonDestination::Editor,
        ][..];
        for destination in destinations {
            harness
                .click_target(HarnessWindow::Project, HarnessTarget::Ribbon(*destination))
                .expect("route workspace ribbon");
        }
        assert!(
            harness
                .contains_text(HarnessWindow::Project, "Scenes")
                .expect("check routed project surface"),
            "project outline should remain mounted after resize and routing"
        );
    }

    // One large local replacement is cheaper and more deterministic than a
    // clock threshold, while still exercising repeated match handling.
    harness
        .right_click_text(HarnessWindow::Project, "Scene 00 research marker")
        .expect("open source scene menu");
    harness
        .click_text(HarnessWindow::Project, "Open")
        .expect("open source scene");
    harness
        .resize(HarnessWindow::Project, 1440.0, 900.0)
        .expect("restore a full editing layout before local search");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            &"research ".repeat(24),
        )
        .expect("write repeated search corpus");
    harness
        .press_command_key(HarnessWindow::Project, 'f')
        .expect("open local search");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::LocalFind(EditorPane::Primary),
            "research",
        )
        .expect("enter repeated search query");
    assert!(
        harness
            .contains_text(HarnessWindow::Project, "24 matches")
            .expect("read repeated search result")
    );
    harness
        .click_text(HarnessWindow::Project, "Replace")
        .expect("open local replacement");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::LocalReplace(EditorPane::Primary),
            "revised",
        )
        .expect("enter replacement text");
    harness
        .click_text(HarnessWindow::Project, "Replace all")
        .expect("apply repeated replacement");
    assert_eq!(
        harness
            .active_editor_body()
            .expect("read revised corpus")
            .matches("revised")
            .count(),
        24
    );

    // Move several outline items into the same destination and verify the
    // hierarchy remains complete after repeated drag routing.
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Editor),
        )
        .expect("return to Explorer after replacement");
    for index in 0..3 {
        let title = format!("Scene {index:02} research marker");
        let source = harness.hierarchy_node(&title).expect("resolve drag source");
        let destination = harness
            .hierarchy_node("Archive")
            .expect("resolve drag destination");
        harness
            .drag_hierarchy_node(
                HarnessWindow::Project,
                HarnessHierarchySurface::Explorer,
                source,
                destination,
                HarnessDropPosition::Into,
            )
            .expect("move outline item into archive");
    }
    assert_eq!(
        harness
            .hierarchy_titles()
            .expect("read complete post-drag hierarchy")
            .iter()
            .filter(|title| title.starts_with("Scene "))
            .count(),
        12
    );

    let errors = harness
        .take_diagnostics()
        .into_iter()
        .filter(|event| format!("{:?}", event.level) == "Error")
        .collect::<Vec<_>>();
    assert!(
        errors.is_empty(),
        "author flow emitted error diagnostics: {errors:?}"
    );
    assert!(
        harness.trace().expect("read authoring trace").len() > 40,
        "stress flow should exercise a substantial interaction trace"
    );
    close(harness);
}

fn create_project(run: &IsolatedRun, project: &Path, title: &str) -> DesktopInteractionHarness {
    let harness = DesktopInteractionHarness::launch(run.root(), LaunchRequest::launcher())
        .expect("launch application");
    harness
        .click_text(HarnessWindow::Launcher, "Create Project")
        .expect("open project creation form");
    harness
        .type_into(HarnessWindow::Launcher, "Project title", title)
        .expect("enter project title");
    harness
        .type_into(
            HarnessWindow::Launcher,
            "Project destination",
            project.to_string_lossy(),
        )
        .expect("enter project destination");
    harness
        .click_text(HarnessWindow::Launcher, "Create and Open")
        .expect("create project");
    harness
}

fn create_group(harness: &DesktopInteractionHarness, parent: &str, title: &str) {
    harness
        .right_click_text(HarnessWindow::Project, parent)
        .expect("open parent context menu");
    harness
        .click_text(HarnessWindow::Project, "Create group")
        .expect("create group");
    harness
        .redraw(HarnessWindow::Project)
        .expect("render group-name field");
    harness
        .type_focused(HarnessWindow::Project, title)
        .expect("name group");
    harness
        .press_key(HarnessWindow::Project, parchmint_desktop::HarnessKey::Enter)
        .expect("commit group name");
}

fn create_document(harness: &DesktopInteractionHarness, parent: &str, title: &str) {
    harness
        .right_click_text(HarnessWindow::Project, parent)
        .expect("open parent context menu");
    harness
        .click_text(HarnessWindow::Project, "Create document")
        .expect("create document");
    harness
        .redraw(HarnessWindow::Project)
        .expect("render document-name field");
    harness
        .type_focused(HarnessWindow::Project, title)
        .expect("name document");
    harness
        .press_key(HarnessWindow::Project, parchmint_desktop::HarnessKey::Enter)
        .expect("commit document name");
}

fn close(harness: DesktopInteractionHarness) {
    harness
        .close(HarnessWindow::Project)
        .expect("close project");
    harness.shutdown().expect("stop application");
}
