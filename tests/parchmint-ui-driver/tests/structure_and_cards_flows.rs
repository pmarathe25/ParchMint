use std::{collections::BTreeMap, fs, path::Path};

use parchmint_desktop::{
    DesktopInteractionHarness, EditorPane, FocusTarget, HarnessDropPosition,
    HarnessHierarchySurface, HarnessKey, HarnessSelectionGesture, HarnessTarget, HarnessWindow,
    LaunchRequest, RibbonDestination,
};
use parchmint_domain::{
    DocumentId, NodeId, Project, ProjectCommand, ProjectId, apply_project_command,
};
use parchmint_project_format::{CanonicalProjectPathMap, ProjectFormatCodec};
use parchmint_ui_driver::IsolatedRun;

#[test]
fn explorer_document_click_reuses_a_temporary_preview_until_opened_permanently() {
    let run = IsolatedRun::new("preview-replacement").expect("isolated run");
    let project = run.root().join("preview-replacement.parchmint");
    let harness = create_project(&run, &project, "Preview Replacement");
    create_document(&harness, "Manuscript", "First Note");
    create_document(&harness, "Manuscript", "Second Note");
    let first = harness
        .hierarchy_node("First Note")
        .expect("resolve the first Explorer note");
    let second = harness
        .hierarchy_node("Second Note")
        .expect("resolve the second Explorer note");

    harness
        .click_hierarchy_node(HarnessWindow::Project, first.clone())
        .expect("activate the first permanent creation tab");
    let first_id = harness
        .active_editor_document_id(EditorPane::Primary)
        .expect("read first document id");
    harness
        .close_editor_tab(HarnessWindow::Project, EditorPane::Primary, first_id)
        .expect("close the first permanent creation tab");
    harness
        .click_hierarchy_node(HarnessWindow::Project, second.clone())
        .expect("activate the second permanent creation tab");
    let second_id = harness
        .active_editor_document_id(EditorPane::Primary)
        .expect("read second document id");
    harness
        .close_editor_tab(HarnessWindow::Project, EditorPane::Primary, second_id)
        .expect("close the second permanent creation tab");

    let baseline_tab_count = harness.tab_titles().expect("read baseline tab strip").len();

    harness
        .click_hierarchy_node(HarnessWindow::Project, first.clone())
        .expect("preview the first note");
    assert_eq!(
        harness
            .tab_titles()
            .expect("read first preview tab strip")
            .len(),
        baseline_tab_count + 1
    );

    harness
        .click_hierarchy_node(HarnessWindow::Project, second.clone())
        .expect("replace the temporary preview");
    assert_eq!(
        harness
            .tab_titles()
            .expect("read replaced preview tab strip")
            .len(),
        baseline_tab_count + 1,
        "a new preview must replace the old one"
    );

    harness
        .right_click_hierarchy_node(HarnessWindow::Project, second)
        .expect("open the second note context menu");
    harness
        .click_text(HarnessWindow::Project, "Open")
        .expect("promote the preview to a permanent tab");
    harness
        .click_hierarchy_node(HarnessWindow::Project, first)
        .expect("preview the first note again");
    assert_eq!(
        harness.tab_titles().expect("read promoted tab strip").len(),
        baseline_tab_count + 2,
        "a promoted tab must survive a later preview"
    );

    close(harness);
}

#[test]
fn explorer_keyboard_navigation_reaches_a_document_and_group_click_collapses_it() {
    let run = IsolatedRun::new("explorer-keyboard-structure").expect("isolated run");
    let project = run.root().join("explorer-keyboard-structure.parchmint");
    let harness = create_project(&run, &project, "Explorer Keyboard Structure");
    create_group(&harness, "Manuscript", "Act One");
    create_document(&harness, "Act One", "Opening Scene");
    let act_one = harness
        .hierarchy_node("Act One")
        .expect("resolve the Act One group");
    let opening_scene = harness
        .hierarchy_node("Opening Scene")
        .expect("resolve the opening-scene document");

    assert!(
        harness
            .hierarchy_node_is_visible(HarnessWindow::Project, opening_scene.clone())
            .expect("inspect expanded Explorer group")
    );
    harness
        .click_hierarchy_node(HarnessWindow::Project, act_one.clone())
        .expect("collapse the act with one click");
    assert!(
        !harness
            .hierarchy_node_is_visible(HarnessWindow::Project, opening_scene)
            .expect("inspect collapsed Explorer group")
    );
    harness
        .click_hierarchy_node(HarnessWindow::Project, act_one)
        .expect("reopen the act");
    assert!(
        harness
            .hierarchy_node_is_visible(
                HarnessWindow::Project,
                harness
                    .hierarchy_node("Opening Scene")
                    .expect("resolve reopened opening scene"),
            )
            .expect("inspect reopened Explorer group")
    );
    harness
        .select_hierarchy_node(
            HarnessWindow::Project,
            harness
                .hierarchy_node("Act One")
                .expect("resolve reopened act"),
            HarnessSelectionGesture::Replace,
        )
        .expect("focus the act before keyboard navigation");
    assert_eq!(
        harness
            .focus_target(HarnessWindow::Project)
            .expect("inspect Explorer keyboard focus"),
        FocusTarget::Explorer
    );
    harness
        .press_key(HarnessWindow::Project, HarnessKey::ArrowDown)
        .expect("navigate from the act to its document");
    assert_eq!(
        harness
            .focus_target(HarnessWindow::Project)
            .expect("inspect focus after Explorer navigation"),
        FocusTarget::Explorer
    );
    assert_eq!(
        harness
            .hierarchy()
            .expect("inspect keyboard Explorer selection")
            .into_iter()
            .find(|entry| entry.selected)
            .map(|entry| entry.title),
        Some("Opening Scene".to_owned())
    );
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Enter)
        .expect("open the keyboard-selected document");
    harness
        .redraw(HarnessWindow::Project)
        .expect("complete the keyboard-requested document mount");
    assert_eq!(
        harness
            .active_editor_tab_title()
            .expect("read keyboard-opened tab"),
        "Opening Scene"
    );

    close(harness);
}

#[test]
fn explorer_add_menu_builds_a_multi_level_outline_in_the_selected_context() {
    let run = IsolatedRun::new("nested-explorer-add").expect("isolated run");
    let project = run.root().join("nested-explorer-add.parchmint");
    let harness = create_project(&run, &project, "Nested Explorer Add");

    add_group(&harness, "Manuscript", "Act One");
    add_group(&harness, "Act One", "Scene One");
    add_document(&harness, "Scene One", "Beat One");
    add_document(&harness, "Act One", "Interlude");

    let titles = harness.hierarchy_titles().expect("read nested hierarchy");
    assert_order(&titles, &["Act One", "Scene One", "Beat One", "Interlude"]);

    close(harness);
}

#[test]
fn cards_selection_can_move_an_outline_item_into_another_group() {
    let run = IsolatedRun::new("cards-cross-group").expect("isolated run");
    let project = run.root().join("cards-cross-group.parchmint");
    let harness = create_project(&run, &project, "Cards Cross Group");
    create_group(&harness, "Manuscript", "Act One");
    create_group(&harness, "Manuscript", "Act Two");
    create_document(&harness, "Act One", "Opening");
    create_document(&harness, "Act Two", "Closing");

    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Cards),
        )
        .expect("open the Cards outline");
    harness
        .click_cards_node(
            HarnessWindow::Project,
            harness
                .hierarchy_node("Opening")
                .expect("resolve source card"),
        )
        .expect("select an outline card before dragging it");
    let opening = harness
        .hierarchy_node("Opening")
        .expect("resolve source card");
    assert!(
        harness
            .cards_node_is_visible(HarnessWindow::Project, opening.clone())
            .expect("inspect selected source card"),
        "selecting a Cards document must leave it mounted and draggable"
    );
    let act_two = harness
        .hierarchy_node("Act Two")
        .expect("resolve destination card");
    assert!(
        harness
            .cards_node_is_visible(HarnessWindow::Project, act_two.clone())
            .expect("inspect destination group card"),
        "a Cards destination group must remain mounted before a drag"
    );
    harness
        .drag_hierarchy_node(
            HarnessWindow::Project,
            HarnessHierarchySurface::Cards,
            opening,
            act_two,
            HarnessDropPosition::Into,
        )
        .expect("move the selected card into Act Two");
    let titles = harness.hierarchy_titles().expect("read moved outline");
    assert_order(&titles, &["Act One", "Act Two", "Closing", "Opening"]);

    close(harness);
}

#[test]
fn cards_group_click_expands_and_collapses_its_children() {
    let run = IsolatedRun::new("cards-group-disclosure").expect("isolated run");
    let project = run.root().join("cards-group-disclosure.parchmint");
    let harness = create_project(&run, &project, "Cards Group Disclosure");
    create_group(&harness, "Manuscript", "Part One");
    create_document(&harness, "Part One", "Opening Scene");

    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Cards),
        )
        .expect("open the Cards outline");
    let opening = harness
        .hierarchy_node("Opening Scene")
        .expect("resolve child Card");
    assert!(
        harness
            .cards_node_is_visible(HarnessWindow::Project, opening.clone())
            .expect("inspect the expanded group")
    );

    harness
        .click_cards_node(
            HarnessWindow::Project,
            harness
                .hierarchy_node("Part One")
                .expect("resolve group Card"),
        )
        .expect("collapse the group from its Card");
    assert_eq!(
        harness
            .hierarchy()
            .expect("inspect shared Cards selection")
            .into_iter()
            .find(|entry| entry.selected)
            .map(|entry| entry.title),
        Some("Part One".to_owned())
    );
    assert!(
        !harness
            .cards_node_is_visible(HarnessWindow::Project, opening.clone())
            .expect("inspect the collapsed group")
    );

    harness
        .click_cards_node(
            HarnessWindow::Project,
            harness
                .hierarchy_node("Part One")
                .expect("resolve group Card"),
        )
        .expect("expand the group from its Card");
    assert!(
        harness
            .cards_node_is_visible(HarnessWindow::Project, opening)
            .expect("inspect the re-expanded group")
    );

    close(harness);
}

#[test]
fn cards_document_click_selects_and_double_click_opens_the_document() {
    let run = IsolatedRun::new("cards-document-activation").expect("isolated run");
    let project = run.root().join("cards-document-activation.parchmint");
    let harness = create_project(&run, &project, "Cards Document Activation");
    create_document(&harness, "Manuscript", "Chapter One");

    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Cards),
        )
        .expect("open the Cards outline");
    let chapter = harness
        .hierarchy_node("Chapter One")
        .expect("resolve document Card");
    harness
        .click_cards_node(HarnessWindow::Project, chapter.clone())
        .expect("select the document Card");
    assert_eq!(
        harness
            .hierarchy()
            .expect("inspect shared Cards selection")
            .into_iter()
            .find(|entry| entry.selected)
            .map(|entry| entry.title),
        Some("Chapter One".to_owned())
    );

    harness
        .double_click_cards_node(HarnessWindow::Project, chapter)
        .expect("activate the document Card");
    assert_eq!(
        harness
            .active_editor_tab_title()
            .expect("inspect activated document tab"),
        "Chapter One"
    );
    assert!(
        !harness
            .target_is_visible(HarnessWindow::Project, HarnessTarget::CardsList)
            .expect("inspect Cards route after activation"),
        "double-clicking a document Card must visibly leave Cards for Editor"
    );
    assert!(
        harness
            .target_is_visible(HarnessWindow::Project, HarnessTarget::EditorPrimary)
            .expect("inspect activated Editor route")
    );

    close(harness);
}

#[test]
fn dragging_an_expanded_cards_group_does_not_collapse_it() {
    let run = IsolatedRun::new("cards-group-drag-disclosure").expect("isolated run");
    let project = run.root().join("cards-group-drag-disclosure.parchmint");
    let harness = create_project(&run, &project, "Cards Group Drag Disclosure");
    create_group(&harness, "Manuscript", "Part One");
    create_group(&harness, "Manuscript", "Part Two");
    create_document(&harness, "Part One", "Opening Scene");

    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Cards),
        )
        .expect("open the Cards outline");
    let part_one = harness
        .hierarchy_node("Part One")
        .expect("resolve source group");
    let part_two = harness
        .hierarchy_node("Part Two")
        .expect("resolve destination group");
    let opening = harness
        .hierarchy_node("Opening Scene")
        .expect("resolve expanded child Card");

    harness
        .drag_hierarchy_node(
            HarnessWindow::Project,
            HarnessHierarchySurface::Cards,
            part_one,
            part_two,
            HarnessDropPosition::After,
        )
        .expect("reorder an expanded group in Cards");
    assert!(
        harness
            .cards_node_is_visible(HarnessWindow::Project, opening)
            .expect("the moved group's child remains visible")
    );

    close(harness);
}

#[test]
fn cards_virtual_window_keeps_a_long_outline_navigable_and_draggable() {
    let run = IsolatedRun::new("cards-virtual-window").expect("isolated run");
    let project = run.root().join("cards-virtual-window.parchmint");
    seed_large_cards_project(&project, 305);
    assert!(project.is_dir(), "canonical project root must exist");
    assert!(
        project.join(".parchmint/root-id").is_file(),
        "canonical project identity must exist"
    );
    assert!(
        project.join("project.toml").is_file(),
        "canonical manifest must exist"
    );
    let harness = DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project))
        .expect("open the canonical long-outline project");

    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Cards),
        )
        .expect("open the long Cards outline");
    let first = harness
        .hierarchy_node("Bulk Card 000")
        .expect("resolve the first long-outline card");
    let earlier = harness
        .hierarchy_node("Bulk Card 302")
        .expect("resolve a later mounted card");
    let last = harness
        .hierarchy_node("Bulk Card 304")
        .expect("resolve the final long-outline card");
    assert!(
        harness
            .cards_node_is_visible(HarnessWindow::Project, first)
            .expect("inspect the initial Cards window")
    );
    assert!(
        !harness
            .cards_node_is_visible(HarnessWindow::Project, last.clone())
            .expect("verify unmounted Cards rows")
    );

    harness
        .scroll_target_by(HarnessWindow::Project, HarnessTarget::CardsList, -50_000.0)
        .expect("scroll the semantic Cards list to its final window");
    assert!(
        harness
            .cards_node_is_visible(HarnessWindow::Project, last.clone())
            .expect("mount the final card after scrolling")
    );
    assert!(
        harness
            .cards_node_is_visible(HarnessWindow::Project, earlier.clone())
            .expect("mount the nearby Cards drag target after scrolling")
    );
    harness
        .drag_hierarchy_node(
            HarnessWindow::Project,
            HarnessHierarchySurface::Cards,
            last,
            earlier,
            HarnessDropPosition::After,
        )
        .expect("reorder mounted Cards rows after virtual scrolling");
    assert_order(
        &harness.hierarchy_titles().expect("read reordered outline"),
        &["Bulk Card 302", "Bulk Card 304", "Bulk Card 303"],
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
        .expect("render the new group-name field");
    harness
        .type_focused(HarnessWindow::Project, title)
        .expect("replace the selected group name");
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Enter)
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
        .expect("render the new document-name field");
    harness
        .type_focused(HarnessWindow::Project, title)
        .expect("replace the selected document name");
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Enter)
        .expect("commit document name");
}

fn add_group(harness: &DesktopInteractionHarness, parent: &str, title: &str) {
    harness
        .click_text(HarnessWindow::Project, parent)
        .expect("select the contextual add parent");
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ExplorerAdd)
        .expect("open contextual Explorer add");
    harness
        .click_text(HarnessWindow::Project, "Group")
        .expect("choose nested group");
    harness
        .redraw(HarnessWindow::Project)
        .expect("render the nested group-name field");
    harness
        .type_focused(HarnessWindow::Project, title)
        .expect("replace the selected nested group name");
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Enter)
        .expect("commit nested group name");
}

fn add_document(harness: &DesktopInteractionHarness, parent: &str, title: &str) {
    harness
        .click_text(HarnessWindow::Project, parent)
        .expect("select the contextual document parent");
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ExplorerAdd)
        .expect("open contextual Explorer add");
    harness
        .click_text(HarnessWindow::Project, "Document")
        .expect("choose nested document");
    harness
        .redraw(HarnessWindow::Project)
        .expect("render the nested document-name field");
    harness
        .type_focused(HarnessWindow::Project, title)
        .unwrap_or_else(|error| {
            panic!("replace the selected nested document name {title:?}: {error}")
        });
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Enter)
        .expect("commit nested document name");
}

fn seed_large_cards_project(path: &Path, documents: usize) {
    fs::create_dir_all(path.join(".parchmint")).expect("create canonical control directory");
    fs::write(path.join(".parchmint/root-id"), "0000000000000001\n")
        .expect("write canonical project identity");
    let mut project = Project::new(ProjectId::from_bytes([0x91; 16]));
    project.display_title = "Cards Virtual Window".to_owned();
    let mut bodies = BTreeMap::new();
    for index in 0..documents {
        let node_id = seeded_id(0xA1, index);
        let document_id = seeded_document_id(0xB1, index);
        let revision = project.revision;
        project = apply_project_command(
            &project,
            revision,
            ProjectCommand::create_document(
                node_id,
                document_id,
                NodeId::manuscript_root(),
                index,
                format!("Bulk Card {index:03}"),
            ),
        )
        .expect("build canonical long outline")
        .project;
        bodies.insert(document_id, "<p>Bulk prose.</p>".to_owned());
    }
    let encoding = ProjectFormatCodec::default()
        .encode_domain_project(
            &project,
            &bodies,
            &BTreeMap::new(),
            &CanonicalProjectPathMap::default(),
        )
        .expect("encode canonical long outline");
    for resource in encoding.resources.into_values() {
        let destination = path.join(resource.path.as_str());
        fs::create_dir_all(destination.parent().expect("resource parent"))
            .expect("create canonical resource parent");
        fs::write(destination, resource.bytes).expect("write canonical resource");
    }
}

fn seeded_id(prefix: u8, index: usize) -> NodeId {
    let mut bytes = [prefix; 16];
    bytes[..8].copy_from_slice(&(index as u64).to_le_bytes());
    NodeId::from_bytes(bytes)
}

fn seeded_document_id(prefix: u8, index: usize) -> DocumentId {
    let mut bytes = [prefix; 16];
    bytes[..8].copy_from_slice(&(index as u64).to_le_bytes());
    DocumentId::from_bytes(bytes)
}

fn assert_order(titles: &[String], expected: &[&str]) {
    let positions = expected
        .iter()
        .map(|title| {
            titles
                .iter()
                .position(|candidate| candidate == title)
                .expect("expected title")
        })
        .collect::<Vec<_>>();
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "unexpected hierarchy order: {titles:?}"
    );
}

fn close(harness: DesktopInteractionHarness) {
    harness
        .close(HarnessWindow::Project)
        .expect("close project");
    harness.shutdown().expect("stop application");
}
