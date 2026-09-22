use std::{collections::BTreeMap, fs, path::Path};

use parchmint_desktop::{
    DesktopInteractionHarness, FocusTarget, HarnessDropPosition, HarnessHierarchySurface,
    HarnessKey, HarnessSelectionGesture, HarnessTarget, HarnessWindow, LaunchRequest,
    RibbonDestination,
};
use parchmint_domain::{
    DocumentId, NodeId, Project, ProjectCommand, ProjectId, apply_project_command,
};
use parchmint_project_format::{CanonicalProjectPathMap, ProjectFormatCodec};
use parchmint_ui_driver::{IsolatedRun, create_project};

#[test]
fn hierarchy_drag_previews_reflow_cancel_and_commit_on_both_surfaces() {
    for surface in [
        HarnessHierarchySurface::Cards,
        HarnessHierarchySurface::Explorer,
    ] {
        let run = IsolatedRun::new("hierarchy-reflow").unwrap();
        let project = run.root().join("reflow.parchmint");
        let harness = create_project(&run, &project, "Live Outline");
        create_group(&harness, "Manuscript", "Act One");
        create_group(&harness, "Manuscript", "Act Two");
        for title in ["Arrival", "Discovery", "Departure"] {
            create_document(&harness, "Act One", title);
        }
        harness
            .click_target(
                HarnessWindow::Project,
                HarnessTarget::Ribbon(match surface {
                    HarnessHierarchySurface::Cards => RibbonDestination::Cards,
                    HarnessHierarchySurface::Explorer => RibbonDestination::Editor,
                }),
            )
            .unwrap();
        let arrival = harness.hierarchy_node("Arrival").unwrap();
        let departure = harness.hierarchy_node("Departure").unwrap();
        let act_two = harness.hierarchy_node("Act Two").unwrap();
        let original = harness.hierarchy_titles().unwrap();
        let saved = fs::read(project.join("project.toml")).unwrap();

        for escape in [true, false] {
            harness
                .preview_hierarchy_move(
                    HarnessWindow::Project,
                    surface,
                    arrival.clone(),
                    departure.clone(),
                    HarnessDropPosition::After,
                )
                .unwrap();
            if surface == HarnessHierarchySurface::Cards {
                assert_order(
                    &harness.preview_hierarchy_titles().unwrap(),
                    &["Discovery", "Departure", "Arrival"],
                );
            } else {
                assert_eq!(harness.preview_hierarchy_titles().unwrap(), original);
            }
            assert_eq!(harness.hierarchy_titles().unwrap(), original);
            assert_eq!(fs::read(project.join("project.toml")).unwrap(), saved);
            harness.redraw(HarnessWindow::Project).unwrap();
            if surface == HarnessHierarchySurface::Cards {
                assert_order(
                    &harness.preview_hierarchy_titles().unwrap(),
                    &["Discovery", "Departure", "Arrival"],
                );
            } else {
                assert_eq!(harness.preview_hierarchy_titles().unwrap(), original);
            }
            if escape {
                harness
                    .press_key(HarnessWindow::Project, HarnessKey::Escape)
                    .unwrap();
            } else {
                harness
                    .move_pointer_outside(HarnessWindow::Project)
                    .unwrap();
            }
            harness
                .release_hierarchy_drag(HarnessWindow::Project)
                .unwrap();
            assert_eq!(harness.preview_hierarchy_titles().unwrap(), original);
            assert_eq!(harness.hierarchy_titles().unwrap(), original);
            assert_eq!(fs::read(project.join("project.toml")).unwrap(), saved);
        }

        harness
            .drag_hierarchy_node(
                HarnessWindow::Project,
                surface,
                arrival.clone(),
                departure,
                HarnessDropPosition::After,
            )
            .unwrap();
        let reordered = harness.hierarchy_titles().unwrap();
        assert_order(
            &reordered,
            &["Discovery", "Departure", "Arrival", "Act Two"],
        );

        if surface == HarnessHierarchySurface::Cards {
            harness
                .toggle_cards_group(HarnessWindow::Project, act_two.clone())
                .unwrap();
        }
        harness
            .preview_hierarchy_move(
                HarnessWindow::Project,
                surface,
                arrival.clone(),
                act_two.clone(),
                HarnessDropPosition::Into,
            )
            .unwrap();
        if surface == HarnessHierarchySurface::Cards {
            assert_order(
                &harness.preview_hierarchy_titles().unwrap(),
                &["Discovery", "Departure", "Act Two", "Arrival"],
            );
        } else {
            assert_eq!(harness.preview_hierarchy_titles().unwrap(), reordered);
        }
        assert_eq!(harness.hierarchy_titles().unwrap(), reordered);
        if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
            harness
                .snapshot(
                    HarnessWindow::Project,
                    Path::new(&root).join(format!("reflow-{surface:?}")),
                )
                .unwrap();
        }
        harness
            .press_key(HarnessWindow::Project, HarnessKey::Escape)
            .unwrap();
        harness
            .release_hierarchy_drag(HarnessWindow::Project)
            .unwrap();
        assert_eq!(harness.preview_hierarchy_titles().unwrap(), reordered);
        harness
            .drag_hierarchy_node(
                HarnessWindow::Project,
                surface,
                arrival.clone(),
                act_two,
                HarnessDropPosition::Into,
            )
            .unwrap();
        if surface == HarnessHierarchySurface::Cards {
            assert!(
                harness
                    .cards_node_is_visible(HarnessWindow::Project, arrival)
                    .unwrap(),
                "the destination must stay expanded after the move completes"
            );
        }
        let committed = harness.hierarchy_titles().unwrap();
        assert_order(
            &committed,
            &["Discovery", "Departure", "Act Two", "Arrival"],
        );
        let discovery = harness.hierarchy_node("Discovery").unwrap();
        let act_one = harness.hierarchy_node("Act One").unwrap();
        harness
            .drag_hierarchy_node(
                HarnessWindow::Project,
                surface,
                discovery,
                act_one,
                HarnessDropPosition::Before,
            )
            .unwrap();
        let committed = harness.hierarchy_titles().unwrap();
        assert_order(
            &committed,
            &["Discovery", "Act One", "Departure", "Act Two", "Arrival"],
        );
        close(harness);
        let reopened =
            DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project)).unwrap();
        assert_eq!(reopened.hierarchy_titles().unwrap(), committed);
        close(reopened);
    }
}

#[test]
fn outline_creation_flows_from_name_to_synopsis_and_preserves_actual_word_counts() {
    let run = IsolatedRun::new("outline-planning").unwrap();
    let project = run.root().join("planning.parchmint");
    let harness = create_project(&run, &project, "Outline Planning");
    create_group(&harness, "Manuscript", "Act One");
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Cards),
        )
        .unwrap();
    harness
        .right_click_text(HarnessWindow::Project, "Act One")
        .unwrap();
    harness
        .click_text(HarnessWindow::Project, "New document")
        .unwrap();
    harness
        .type_focused(HarnessWindow::Project, "The arrival")
        .unwrap();
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Enter)
        .unwrap();
    assert!(
        harness
            .target_is_visible(HarnessWindow::Project, HarnessTarget::CardsList)
            .unwrap()
    );
    assert!(
        harness
            .target_is_focused(HarnessWindow::Project, HarnessTarget::InspectorSynopsis)
            .unwrap()
    );
    harness
        .type_focused(HarnessWindow::Project, "Mara arrives at the harbor.")
        .unwrap();
    harness
        .press_key(HarnessWindow::Project, HarnessKey::PrimaryEnter)
        .unwrap();
    assert!(
        harness
            .target_is_focused(HarnessWindow::Project, HarnessTarget::ExplorerRename)
            .unwrap()
    );
    harness
        .type_focused(HarnessWindow::Project, "The departure")
        .unwrap();
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Enter)
        .unwrap();
    assert!(
        harness
            .target_is_visible(HarnessWindow::Project, HarnessTarget::CardsList)
            .unwrap()
    );
    assert!(
        harness
            .target_is_focused(HarnessWindow::Project, HarnessTarget::InspectorSynopsis)
            .unwrap()
    );
    harness
        .type_focused(HarnessWindow::Project, "She sets sail at dawn.")
        .unwrap();
    assert!(
        harness
            .contains_text(HarnessWindow::Project, "Manuscript · 0 words")
            .unwrap()
    );
    assert!(
        harness
            .contains_text(HarnessWindow::Project, "Mara arrives at the harbor.")
            .unwrap()
    );
    let arrival = harness.hierarchy_node("The arrival").unwrap();
    let departure = harness.hierarchy_node("The departure").unwrap();
    harness
        .drag_hierarchy_node(
            HarnessWindow::Project,
            HarnessHierarchySurface::Cards,
            departure,
            arrival.clone(),
            HarnessDropPosition::Before,
        )
        .unwrap();
    assert_order(
        &harness.hierarchy_titles().unwrap(),
        &["The departure", "The arrival"],
    );
    harness
        .double_click_cards_node(HarnessWindow::Project, arrival)
        .unwrap();
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "The harbor was silent.",
        )
        .unwrap();
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Cards),
        )
        .unwrap();
    harness
        .click_cards_node(
            HarnessWindow::Project,
            harness.hierarchy_node("Act One").unwrap(),
        )
        .unwrap();
    assert!(
        harness
            .contains_text(HarnessWindow::Project, "Selected · 4 words")
            .unwrap()
    );
    assert!(
        harness
            .contains_text(HarnessWindow::Project, "Manuscript · 4 words")
            .unwrap()
    );
    if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
        harness
            .snapshot(
                HarnessWindow::Project,
                Path::new(&root).join("outline-planning"),
            )
            .unwrap();
    }
    close(harness);
    let reopened =
        DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project)).unwrap();
    reopened
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Cards),
        )
        .unwrap();
    assert_order(
        &reopened.hierarchy_titles().unwrap(),
        &["The departure", "The arrival"],
    );
    assert!(
        reopened
            .contains_text(HarnessWindow::Project, "Mara arrives at the harbor.")
            .unwrap()
    );
    assert!(
        reopened
            .contains_text(HarnessWindow::Project, "She sets sail at dawn.")
            .unwrap()
    );
    assert!(
        reopened
            .contains_text(HarnessWindow::Project, "Manuscript · 4 words")
            .unwrap()
    );
    close(reopened);
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
fn explorer_context_menu_builds_a_multi_level_outline() {
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
fn cards_group_heading_and_chevron_toggle_the_same_disclosure() {
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
            harness.hierarchy_node("Part One").unwrap(),
        )
        .unwrap();
    assert!(
        !harness
            .cards_node_is_visible(HarnessWindow::Project, opening.clone())
            .unwrap()
    );

    harness
        .toggle_cards_group(
            HarnessWindow::Project,
            harness
                .hierarchy_node("Part One")
                .expect("resolve group Card"),
        )
        .expect("expand the group from its Card");
    assert!(
        harness
            .cards_node_is_visible(HarnessWindow::Project, opening)
            .unwrap()
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
        .double_click_cards_node(HarnessWindow::Project, chapter.clone())
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

    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Cards),
        )
        .expect("return to Outline");
    harness
        .right_click_cards_node(HarnessWindow::Project, chapter)
        .expect("open document menu");
    harness
        .click_text(HarnessWindow::Project, "Open")
        .expect("open the already active document");
    assert!(
        harness
            .target_is_visible(HarnessWindow::Project, HarnessTarget::EditorPrimary)
            .expect(
                "context-menu Open returns to writing even when the document is already mounted"
            )
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
            .cards_node_is_visible(HarnessWindow::Project, first.clone())
            .expect("inspect the initial Cards window")
    );
    assert!(
        !harness
            .cards_node_is_visible(HarnessWindow::Project, last.clone())
            .expect("verify unmounted Cards rows")
    );

    let original = harness.hierarchy_titles().unwrap();
    harness
        .preview_hierarchy_move(
            HarnessWindow::Project,
            HarnessHierarchySurface::Cards,
            first,
            harness.hierarchy_node("Bulk Card 001").unwrap(),
            HarnessDropPosition::After,
        )
        .unwrap();
    harness
        .scroll_target_by(HarnessWindow::Project, HarnessTarget::CardsList, -50_000.0)
        .expect("scroll the semantic Cards list to its final window");
    harness.redraw(HarnessWindow::Project).unwrap();
    let preview = harness.preview_hierarchy_titles().unwrap();
    assert_eq!(harness.hierarchy_titles().unwrap(), original);
    assert!(
        preview
            .iter()
            .position(|title| title == "Bulk Card 000")
            .unwrap()
            > 250
    );
    harness
        .release_hierarchy_drag(HarnessWindow::Project)
        .unwrap();
    assert_eq!(
        harness.hierarchy_titles().unwrap(),
        preview,
        "release commits the visible preview after scrolling unmounts the original source"
    );
    if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
        harness
            .snapshot(
                HarnessWindow::Project,
                std::path::Path::new(&root).join("long-outline-after-drag"),
            )
            .unwrap();
    }
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

    assert!(
        harness
            .target_is_visible(HarnessWindow::Project, HarnessTarget::CardsList)
            .unwrap(),
        "reordering must keep Outline visible"
    );
    assert!(
        !harness
            .contains_text(HarnessWindow::Project, "Reorganized project")
            .unwrap()
    );
    let final_card = harness
        .hierarchy_node("Bulk Card 303")
        .expect("resolve final card");
    harness
        .right_click_cards_node(HarnessWindow::Project, final_card)
        .expect("open the last card's menu");
    harness
        .click_text(HarnessWindow::Project, "Rename")
        .expect("rename the card");
    assert!(
        harness
            .target_is_visible(HarnessWindow::Project, HarnessTarget::ExplorerRename)
            .expect("reveal the offscreen Explorer name field")
    );
    assert!(
        harness
            .target_is_focused(HarnessWindow::Project, HarnessTarget::ExplorerRename)
            .expect("focus the revealed name field")
    );
    harness
        .replace_target(
            HarnessWindow::Project,
            HarnessTarget::ExplorerRename,
            "Final scene",
        )
        .expect("name the selected card");
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Enter)
        .expect("commit the title");
    assert!(harness.hierarchy_node("Final scene").is_ok());
    harness
        .scroll_target_by(
            HarnessWindow::Project,
            HarnessTarget::CardsList,
            -1_000_000.0,
        )
        .unwrap();
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::OverviewAdd)
        .unwrap();
    harness
        .type_focused(HarnessWindow::Project, "A new ending")
        .unwrap();
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Enter)
        .unwrap();
    assert!(
        harness
            .cards_node_is_visible(
                HarnessWindow::Project,
                harness.hierarchy_node("A new ending").unwrap()
            )
            .unwrap(),
        "creating from the top must reveal the new card at the end of a long outline"
    );
    assert!(
        harness
            .target_is_focused(HarnessWindow::Project, HarnessTarget::InspectorSynopsis)
            .unwrap()
    );

    close(harness);
}

fn create_group(harness: &DesktopInteractionHarness, parent: &str, title: &str) {
    harness
        .right_click_text(HarnessWindow::Project, parent)
        .expect("open parent context menu");
    harness
        .click_text(HarnessWindow::Project, "New group")
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
        .click_text(HarnessWindow::Project, "New document")
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
    create_group(harness, parent, title);
}
fn add_document(harness: &DesktopInteractionHarness, parent: &str, title: &str) {
    create_document(harness, parent, title);
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
