use std::{fs, path::PathBuf};

use parchmint_desktop::{
    DesktopInteractionHarness, EditorPane, HarnessDropPosition, HarnessHierarchySurface,
    HarnessKey, HarnessTarget, HarnessWindow, LaunchRequest, RibbonDestination,
};
use parchmint_ui_driver::IsolatedRun;

#[test]
fn novelist_can_plan_and_draft_a_chaptered_manuscript() {
    let run = IsolatedRun::new("chaptered-novel").expect("isolated run");
    let project = run.root().join("glass-harbor.parchmint");
    let draft = "Rain carried the harbor bells across the water.";
    let harness = create_project(&run, &project, "The Glass Harbor");

    create_group(&harness, "Manuscript", "Part One");
    create_document(&harness, "Part One", "Chapter One");
    assert!(visible(&harness, "Part One"));
    assert!(visible(&harness, "Chapter One"));

    harness
        .right_click_text(HarnessWindow::Project, "Chapter One")
        .expect("open chapter context menu");
    harness
        .click_text(HarnessWindow::Project, "Open")
        .expect("open chapter in its primary editor pane");
    harness
        .type_into_target(HarnessWindow::Project, HarnessTarget::EditorPrimary, draft)
        .expect("draft chapter prose");
    assert!(
        harness
            .active_editor_body()
            .expect("read chapter body")
            .contains(draft)
    );
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Cards),
        )
        .expect("switch to Cards");
    assert!(visible(&harness, "Manuscript outline"));
    assert!(visible(&harness, "Part One"));
    assert!(visible(&harness, "Chapter One"));

    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Editor),
        )
        .expect("return to Editor");
    harness
        .close(HarnessWindow::Project)
        .expect("close chaptered project");
    harness.shutdown().expect("stop first application instance");
}

#[test]
fn newly_created_chapter_autosaves_to_canonical_storage() {
    let run = IsolatedRun::new("chapter-autosave").expect("isolated run");
    let project = run.root().join("glass-harbor.parchmint");
    let draft = "Rain carried the harbor bells across the water.";
    let harness = create_project(&run, &project, "The Glass Harbor");

    create_group(&harness, "Manuscript", "Part One");
    create_document(&harness, "Part One", "Chapter One");
    harness
        .right_click_text(HarnessWindow::Project, "Chapter One")
        .expect("open chapter context menu");
    harness
        .click_text(HarnessWindow::Project, "Open")
        .expect("open chapter in its primary editor pane");
    harness
        .type_into_target(HarnessWindow::Project, HarnessTarget::EditorPrimary, draft)
        .expect("draft chapter prose");
    harness.elapse_autosave_idle().expect("autosave the draft");
    assert!(
        canonical_bodies(&project)
            .iter()
            .any(|body| body.contains(draft)),
        "autosave should persist prose for a newly created chapter"
    );
    harness
        .close(HarnessWindow::Project)
        .expect("close chaptered project");
    harness.shutdown().expect("stop chaptered project");
}

#[test]
fn short_story_author_can_draft_save_and_visit_workspace_modes() {
    let run = IsolatedRun::new("short-story").expect("isolated run");
    let project = run.root().join("winter-story.parchmint");
    let draft = "A frost-black raven crossed the silent field.";
    let harness = create_project(&run, &project, "Winter Story");

    harness
        .type_into_target(HarnessWindow::Project, HarnessTarget::EditorPrimary, draft)
        .expect("draft short story");
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Cards),
        )
        .expect("review the story in Cards");
    assert!(visible(&harness, "Untitled Document"));
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Settings),
        )
        .expect("open project settings");
    harness
        .click_text(HarnessWindow::Project, "Dark")
        .expect("choose dark appearance");
    assert!(visible(&harness, "Dark appearance"));
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Export),
        )
        .expect("review manuscript export");
    assert!(visible(&harness, "Export manuscript"));
    assert!(visible(&harness, "Entire Manuscript"));
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Editor),
        )
        .expect("return to drafting");
    harness
        .elapse_autosave_idle()
        .expect("autosave short story");
    assert!(
        canonical_bodies(&project)
            .iter()
            .any(|body| body.contains(draft))
    );
    harness
        .close(HarnessWindow::Project)
        .expect("close saved short story");
    harness.shutdown().expect("stop first application instance");

    let reopened = DesktopInteractionHarness::launch(run.root(), LaunchRequest::launcher())
        .expect("relaunch application");
    reopened
        .click_text(HarnessWindow::Launcher, "Winter Story")
        .expect("open recent short story");
    assert!(
        reopened
            .active_editor_body()
            .expect("read reopened short story")
            .contains(draft)
    );
    reopened
        .close(HarnessWindow::Project)
        .expect("close reopened short story");
    reopened.shutdown().expect("stop reopened application");
}

#[test]
fn reopening_a_project_restores_cards_context_and_both_writing_panes() {
    let run = IsolatedRun::new("restore-writing-context").expect("isolated run");
    let project = run.root().join("restore-writing-context.parchmint");
    let harness = create_project(&run, &project, "Restore Writing Context");

    create_document(&harness, "Research", "Harbor Notes");
    harness
        .right_click_text(HarnessWindow::Project, "Untitled Document")
        .expect("open manuscript document menu");
    harness
        .click_text(HarnessWindow::Project, "Open")
        .expect("open manuscript document in primary pane");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "The harbor bells were already ringing.",
        )
        .expect("draft manuscript prose");
    harness
        .right_click_text(HarnessWindow::Project, "Harbor Notes")
        .expect("open research document menu");
    harness
        .click_text(HarnessWindow::Project, "Open in companion")
        .expect("open research document in companion pane");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorCompanion,
            "Tide tables put the storm at midnight.",
        )
        .expect("draft companion research note");
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Cards),
        )
        .expect("switch to Cards before closing");
    harness
        .click_text(HarnessWindow::Project, "Research")
        .expect("show the Research Cards section before closing");
    harness
        .close(HarnessWindow::Project)
        .expect("close the project with its writing context");
    harness.shutdown().expect("stop first writing session");

    let reopened = DesktopInteractionHarness::launch(run.root(), LaunchRequest::launcher())
        .expect("relaunch application");
    reopened
        .click_text(HarnessWindow::Launcher, "Restore Writing Context")
        .expect("reopen project");
    assert!(visible(&reopened, "Manuscript outline"));
    assert!(visible(&reopened, "Harbor Notes"));
    assert!(
        reopened
            .active_editor_body()
            .expect("read restored focused companion")
            .contains("Tide tables put the storm at midnight."),
        "the focused companion document should restore with the workspace"
    );
    reopened
        .close(HarnessWindow::Project)
        .expect("close reopened writing context");
    reopened.shutdown().expect("stop reopened writing context");
}

#[test]
fn author_can_configure_metadata_to_appear_on_cards() {
    let run = IsolatedRun::new("settings-metadata").expect("isolated run");
    let project = run.root().join("settings-metadata.parchmint");
    let harness = create_project(&run, &project, "Metadata Settings");

    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Settings),
        )
        .expect("open project settings");
    harness
        .click_text(HarnessWindow::Project, "Metadata fields")
        .expect("open metadata settings");
    harness
        .click_text(HarnessWindow::Project, "+ New field")
        .expect("start metadata field creation");
    assert!(
        harness
            .target_is_focused(HarnessWindow::Project, HarnessTarget::MetadataFieldName)
            .expect("focus the new metadata field name")
    );
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::MetadataFieldName,
            "Point of view",
        )
        .expect("name the metadata field without clearing a placeholder");
    harness
        .click_text(HarnessWindow::Project, "Add field")
        .expect("persist the named metadata field");
    assert!(visible(&harness, "Point of view"));
    harness
        .click_text(HarnessWindow::Project, "Visible on cards")
        .expect("show the metadata field on cards");
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Cards),
        )
        .expect("review the configured card projection");
    assert!(visible(&harness, "Point of view"));
    harness
        .close(HarnessWindow::Project)
        .expect("close settings project");
    harness.shutdown().expect("stop settings project");
}

#[test]
fn author_can_rename_a_chapter_from_the_inspector() {
    let run = IsolatedRun::new("inspector-title").expect("isolated run");
    let project = run.root().join("inspector-title.parchmint");
    let harness = create_project(&run, &project, "Inspector Title");

    create_group(&harness, "Manuscript", "Part One");
    create_document(&harness, "Part One", "Untitled Scene");
    harness
        .replace_target(
            HarnessWindow::Project,
            HarnessTarget::InspectorTitle,
            "The Breakwater",
        )
        .expect("rename the selected chapter in Inspector");
    assert!(visible(&harness, "The Breakwater"));
    assert!(
        harness.hierarchy_node("The Breakwater").is_ok(),
        "Inspector title edits must update the shared Explorer hierarchy"
    );
    harness
        .close(HarnessWindow::Project)
        .expect("close inspector-title project");
    harness.shutdown().expect("stop inspector-title project");
}

#[test]
fn editor_honors_standard_formatting_shortcuts() {
    let run = IsolatedRun::new("editor-formatting-shortcuts").expect("isolated run");
    let project = run.root().join("editor-formatting-shortcuts.parchmint");
    let harness = create_project(&run, &project, "Formatting Shortcuts");

    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "Bold opening",
        )
        .expect("write prose to format");
    harness
        .select_editor_text(HarnessWindow::Project, EditorPane::Primary, "Bold opening")
        .expect("select prose to format");
    harness
        .press_command_key(HarnessWindow::Project, 'b')
        .expect("toggle bold with the standard shortcut");
    harness
        .elapse_autosave_idle()
        .expect("autosave formatted prose");

    assert!(
        canonical_bodies(&project)
            .iter()
            .any(|body| body.contains("<strong>Bold opening</strong>")),
        "the editor shortcut must emit canonical bold markup"
    );
    harness
        .close(HarnessWindow::Project)
        .expect("close formatting project");
    harness.shutdown().expect("stop formatting project");
}

#[test]
fn editor_honors_standard_undo_and_redo_shortcuts() {
    let run = IsolatedRun::new("editor-undo-redo-shortcuts").expect("isolated run");
    let project = run.root().join("editor-undo-redo-shortcuts.parchmint");
    let harness = create_project(&run, &project, "Undo and Redo Shortcuts");

    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "A revisable opening.",
        )
        .expect("write revisable prose");
    harness
        .press_command_key(HarnessWindow::Project, 'z')
        .expect("undo with the standard shortcut");
    assert!(
        !harness
            .active_editor_body()
            .expect("read the undone draft")
            .contains("A revisable opening."),
        "undo must apply to the focused editor"
    );
    #[cfg(target_os = "macos")]
    harness
        .press_command_shift_key(HarnessWindow::Project, 'z')
        .expect("redo with the standard macOS shortcut");
    #[cfg(not(target_os = "macos"))]
    harness
        .press_command_key(HarnessWindow::Project, 'y')
        .expect("redo with the standard shortcut");
    assert!(
        harness
            .active_editor_body()
            .expect("read the redone draft")
            .contains("A revisable opening."),
        "redo must restore the focused editor change"
    );
    harness
        .close(HarnessWindow::Project)
        .expect("close undo-redo project");
    harness.shutdown().expect("stop undo-redo project");
}

#[test]
fn explorer_focus_routes_undo_and_redo_to_project_history() {
    let run = IsolatedRun::new("project-undo-shortcuts").expect("isolated run");
    let project = run.root().join("project-undo-shortcuts.parchmint");
    let harness = create_project(&run, &project, "Project Undo Shortcuts");

    create_group(&harness, "Manuscript", "Part One");
    assert!(visible(&harness, "Part One"));

    for _ in 0..3 {
        harness
            .press_key(HarnessWindow::Project, HarnessKey::F6)
            .expect("move focus to Explorer");
    }
    harness
        .press_command_key(HarnessWindow::Project, 'z')
        .expect("undo Explorer project change");
    assert!(
        !visible(&harness, "Part One"),
        "project undo should remove the Explorer-created group"
    );

    harness
        .press_command_key(HarnessWindow::Project, 'y')
        .expect("redo Explorer project change");
    assert!(visible(&harness, "Part One"));
    harness
        .close(HarnessWindow::Project)
        .expect("close project undo shortcut project");
    harness
        .shutdown()
        .expect("stop project undo shortcut project");
}

#[test]
fn explorer_creation_replaces_the_selected_default_title() {
    let run = IsolatedRun::new("explorer-rename-shortcut").expect("isolated run");
    let project = run.root().join("explorer-rename-shortcut.parchmint");
    let harness = create_project(&run, &project, "Explorer Rename Shortcut");

    harness
        .right_click_text(HarnessWindow::Project, "Manuscript")
        .expect("open Manuscript context menu");
    harness
        .click_text(HarnessWindow::Project, "Create group")
        .expect("create a group");
    harness
        .redraw(HarnessWindow::Project)
        .expect("render the created group-name field");
    assert!(
        harness
            .target_is_visible(HarnessWindow::Project, HarnessTarget::ExplorerRename)
            .expect("inspect created group-name field"),
        "the created group must render an inline name field"
    );
    assert!(
        harness
            .target_is_focused(HarnessWindow::Project, HarnessTarget::ExplorerRename)
            .expect("inspect initial group-name focus"),
        "creation must immediately focus the inline name field"
    );
    harness
        .type_focused(HarnessWindow::Project, "Part One")
        .expect("replace the selected default group name");
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Enter)
        .expect("commit the initial group name");
    let titles = harness
        .hierarchy_titles()
        .expect("read hierarchy after direct creation naming");
    assert!(
        titles.iter().any(|title| title == "Part One"),
        "the created group must have its requested title: {titles:?}"
    );
    assert!(!titles.iter().any(|title| title == "New Group"));

    harness
        .close(HarnessWindow::Project)
        .expect("close Explorer rename project");
    harness.shutdown().expect("stop Explorer rename project");
}

#[test]
fn explorer_add_menu_creates_a_group_and_document_in_the_current_context() {
    let run = IsolatedRun::new("explorer-add-menu").expect("isolated run");
    let project = run.root().join("explorer-add-menu.parchmint");
    let harness = create_project(&run, &project, "Explorer Add Menu");

    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ExplorerAdd)
        .expect("open the Explorer-local creation menu");
    harness
        .redraw(HarnessWindow::Project)
        .expect("render the Explorer creation menu");
    assert!(visible(&harness, "Add to Manuscript"));
    harness
        .click_text(HarnessWindow::Project, "Group")
        .expect("create a group from the Explorer menu");
    harness
        .redraw(HarnessWindow::Project)
        .expect("render the created group-name field");
    let group_name_is_focused = harness
        .target_is_focused(HarnessWindow::Project, HarnessTarget::ExplorerRename)
        .expect("focus the new group title");
    let titles_after_group = harness
        .hierarchy_titles()
        .expect("read hierarchy after group creation");
    assert!(
        group_name_is_focused,
        "the Explorer menu must select the default title for replacement; hierarchy: {titles_after_group:?}"
    );
    harness
        .type_focused(HarnessWindow::Project, "Part One")
        .expect("replace the new group title");
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Enter)
        .expect("commit the group title");

    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ExplorerAdd)
        .expect("open the creation menu for the selected group");
    assert!(visible(&harness, "Add to Part One"));
    harness
        .click_text(HarnessWindow::Project, "Document")
        .expect("create a document within the selected group");
    harness
        .redraw(HarnessWindow::Project)
        .expect("render the created document-name field");
    assert!(
        harness
            .target_is_focused(HarnessWindow::Project, HarnessTarget::ExplorerRename)
            .expect("focus the new document title"),
        "the Explorer menu must select the document default title"
    );
    harness
        .type_focused(HarnessWindow::Project, "Chapter One")
        .expect("replace the new document title");
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Enter)
        .expect("commit the document title");

    let titles = harness
        .hierarchy_titles()
        .expect("read hierarchy after Explorer-local creation");
    assert!(titles.iter().any(|title| title == "Part One"), "{titles:?}");
    assert!(
        titles.iter().any(|title| title == "Chapter One"),
        "{titles:?}"
    );

    harness
        .close(HarnessWindow::Project)
        .expect("close Explorer menu project");
    harness.shutdown().expect("stop Explorer menu project");
}

#[test]
fn explorer_f2_rename_replaces_the_selected_title() {
    let run = IsolatedRun::new("explorer-f2-rename").expect("isolated run");
    let project = run.root().join("explorer-f2-rename.parchmint");
    let harness = create_project(&run, &project, "Explorer F2 Rename");

    for _ in 0..3 {
        harness
            .press_key(HarnessWindow::Project, HarnessKey::F6)
            .expect("move keyboard focus to Explorer");
    }
    harness
        .click_text(HarnessWindow::Project, "Untitled Document")
        .expect("select the document to rename");
    harness
        .press_key(HarnessWindow::Project, HarnessKey::F2)
        .expect("start standard Explorer rename");
    assert!(
        harness
            .target_is_focused(HarnessWindow::Project, HarnessTarget::ExplorerRename)
            .expect("inspect F2 rename focus"),
        "F2 must focus the selected Explorer title"
    );
    harness
        .type_focused(HarnessWindow::Project, "Chapter One")
        .expect("replace the selected title through F2");
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Enter)
        .expect("commit standard Explorer rename");
    let titles = harness
        .hierarchy_titles()
        .expect("read hierarchy after F2 rename");
    assert!(
        titles.iter().any(|title| title == "Chapter One"),
        "{titles:?}"
    );
    assert!(
        !titles.iter().any(|title| title == "Untitled Document"),
        "F2 must replace the selected title: {titles:?}"
    );

    harness
        .close(HarnessWindow::Project)
        .expect("close Explorer F2 rename project");
    harness.shutdown().expect("stop Explorer F2 rename project");
}

#[test]
fn keyboard_focus_can_confirm_a_settings_modal_across_commands() {
    let run = IsolatedRun::new("settings-keyboard-focus").expect("isolated run");
    let project = run.root().join("settings-keyboard-focus.parchmint");
    let harness = create_project(&run, &project, "Settings Keyboard Focus");

    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Settings),
        )
        .expect("open project settings");
    harness
        .click_text(HarnessWindow::Project, "Metadata fields")
        .expect("open metadata settings");
    harness
        .click_text(HarnessWindow::Project, "+ New field")
        .expect("start metadata field creation");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::MetadataFieldName,
            "Draft status",
        )
        .expect("name the metadata field");
    harness
        .click_text(HarnessWindow::Project, "Add field")
        .expect("persist metadata field before deletion");
    harness
        .click_text(HarnessWindow::Project, "Delete metadata field")
        .expect("request field deletion");
    assert!(visible(&harness, "Delete metadata field"));
    assert!(
        harness
            .target_is_focused(HarnessWindow::Project, HarnessTarget::ModalCancel)
            .expect("read initial modal focus")
    );
    harness
        .press_key(HarnessWindow::Project, HarnessKey::F6)
        .expect("move the modal keyboard focus from Cancel to Confirm");
    assert!(
        harness
            .target_is_focused(HarnessWindow::Project, HarnessTarget::ModalConfirm)
            .expect("read retained confirm focus")
    );
    harness
        .click_text(HarnessWindow::Project, "Confirm")
        .expect("confirm deletion after keyboard focus traversal");
    assert!(!visible(&harness, "New field"));
    harness
        .close(HarnessWindow::Project)
        .expect("close settings keyboard-focus project");
    harness
        .shutdown()
        .expect("stop settings keyboard-focus project");
}

#[test]
fn author_can_compare_and_restore_an_automatic_history_checkpoint() {
    let run = IsolatedRun::new("history-restore").expect("isolated run");
    let project = run.root().join("history-restore.parchmint");
    let harness = create_project(&run, &project, "History Restore");

    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "The first chart marked the safe channel.",
        )
        .expect("write the first checkpoint revision");
    harness
        .elapse_autosave_idle()
        .expect("create the first automatic checkpoint");
    harness
        .type_focused(
            HarnessWindow::Project,
            " The second chart changed the route.",
        )
        .expect("write a later revision through retained editor focus");
    harness
        .elapse_autosave_idle()
        .expect("create the later automatic checkpoint");
    let later_revision = harness
        .active_editor_body()
        .expect("read later revision before opening history");
    assert!(later_revision.contains("The second chart changed the route."));
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::History),
        )
        .expect("open project history");
    assert!(visible(&harness, "Writing timeline"));
    harness
        .click_history_checkpoint(HarnessWindow::Project, 1)
        .expect("compare the earlier automatic checkpoint");
    assert!(
        visible(&harness, "Checkpoint"),
        "history status: {}",
        harness.history_status().expect("read history status")
    );
    harness
        .click_text(HarnessWindow::Project, "Restore “Automatic save”")
        .expect("request checkpoint restoration");
    assert!(visible(&harness, "Restore project history"));
    harness
        .click_text(HarnessWindow::Project, "Confirm")
        .expect("restore the selected checkpoint");
    assert!(visible(&harness, "Writing timeline"));
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Editor),
        )
        .expect("return to the restored draft");
    let restored_revision = harness
        .active_editor_body()
        .expect("read the restored draft");
    assert!(
        restored_revision.contains("The first chart marked the safe channel."),
        "restore must retain the earlier checkpoint text: {restored_revision:?}"
    );
    assert!(
        !restored_revision.contains("The second chart changed the route."),
        "restore must discard the later revision: {restored_revision:?}"
    );
    harness
        .close(HarnessWindow::Project)
        .expect("close history project");
    harness.shutdown().expect("stop history project");
}

#[test]
fn author_can_edit_a_document_selected_from_tab_overflow() {
    let run = IsolatedRun::new("tab-overflow-editing").expect("isolated run");
    let project = run.root().join("tab-overflow-editing.parchmint");
    let harness = create_project(&run, &project, "Tab Overflow Editing");

    create_group(&harness, "Manuscript", "Chapters");
    for title in [
        "Chapter One",
        "Chapter Two",
        "Chapter Three",
        "Chapter Four",
        "Chapter Five",
        "Chapter Six",
        "Chapter Seven",
        "Chapter Eight",
        "Chapter Nine",
        "Chapter Ten",
    ] {
        create_document(&harness, "Chapters", title);
        harness
            .right_click_text(HarnessWindow::Project, title)
            .expect("open chapter context menu");
        harness
            .click_text(HarnessWindow::Project, "Open")
            .expect("open chapter as a permanent tab");
    }
    let tab_titles = harness.tab_titles().expect("read the open tab order");
    assert!(
        tab_titles.len() >= 10,
        "opening each chapter must retain its tab before exercising overflow: {tab_titles:?}"
    );
    harness
        .resize(HarnessWindow::Project, 960.0, 720.0)
        .expect("resize the writing workspace to exercise responsive tab overflow");

    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::TabOverflow(EditorPane::Primary),
        )
        .expect("open the real tab-overflow menu");
    harness
        .press_key(HarnessWindow::Project, HarnessKey::ArrowDown)
        .expect("move to the first hidden tab");
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Enter)
        .expect("activate the hidden tab from overflow");
    let overflow_selected_title = harness
        .active_editor_tab_title()
        .expect("identify the document selected from overflow");
    let overflow_selected_id = harness
        .active_editor_document_id(EditorPane::Primary)
        .expect("identify the overflow-selected document");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "Overflow sentinel.",
        )
        .expect("edit the tab selected through overflow");
    assert!(
        harness
            .active_editor_body()
            .expect("read the tab selected from overflow")
            .contains("Overflow sentinel."),
        "the mounted editor must match the tab activated through overflow"
    );
    assert!(
        harness
            .editor_tab_is_visible(
                HarnessWindow::Project,
                EditorPane::Primary,
                overflow_selected_id,
            )
            .expect("inspect the rendered tab strip"),
        "selecting a hidden tab must rotate the tab strip to reveal it"
    );

    let other_title = ["Chapter Ten", "Chapter Nine", "Chapter Eight"]
        .into_iter()
        .find(|title| *title != overflow_selected_title.as_str())
        .expect("a visible chapter distinct from the overflow selection");
    harness
        .right_click_text(HarnessWindow::Project, other_title)
        .expect("open another chapter after the overflow edit");
    harness
        .click_text(HarnessWindow::Project, "Open")
        .expect("switch to another chapter");
    assert!(
        !harness
            .active_editor_body()
            .expect("read a different chapter")
            .contains("Overflow sentinel."),
        "editing a hidden tab must not write into whichever document happened to be mounted"
    );
    harness
        .right_click_text(HarnessWindow::Project, overflow_selected_title.clone())
        .expect("return to the overflow-selected document");
    harness
        .click_text(HarnessWindow::Project, "Open")
        .expect("open the overflow-selected document again");
    assert!(
        harness
            .active_editor_body()
            .expect("read the overflow-selected document again")
            .contains("Overflow sentinel."),
        "the overflow edit must remain attached to its selected document"
    );

    harness
        .close(HarnessWindow::Project)
        .expect("close tab-overflow project");
    harness.shutdown().expect("stop tab-overflow project");
}

#[test]
fn retained_focus_supports_cross_command_local_find_and_replace() {
    let run = IsolatedRun::new("retained-local-search").expect("isolated run");
    let project = run.root().join("retained-local-search.parchmint");
    let harness = create_project(&run, &project, "Retained Local Search");

    harness
        .click_target(HarnessWindow::Project, HarnessTarget::EditorPrimary)
        .expect("focus editor");
    harness
        .type_focused(HarnessWindow::Project, "river river")
        .expect("type through retained editor focus");
    harness
        .press_command_key(HarnessWindow::Project, 'f')
        .expect("open local Find with the focused editor shortcut");
    assert!(visible(&harness, "Find"));
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::LocalFind(EditorPane::Primary),
            "river",
        )
        .expect("enter local search query");
    assert!(visible(&harness, "2 matches"));
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Enter)
        .expect("navigate from a retained Find focus");
    harness
        .click_text(HarnessWindow::Project, "Replace")
        .expect("show local replacement controls");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::LocalReplace(EditorPane::Primary),
            "water",
        )
        .expect("enter local replacement");
    harness
        .click_text(HarnessWindow::Project, "Replace all")
        .expect("replace all local matches");
    assert!(
        harness
            .active_editor_body()
            .expect("read replaced editor body")
            .contains("water water")
    );
    harness
        .close(HarnessWindow::Project)
        .expect("close local-search project");
    harness.shutdown().expect("stop local-search project");
}

#[test]
fn author_can_reorder_chapters_from_explorer_and_cards() {
    let run = IsolatedRun::new("hierarchy-drag").expect("isolated run");
    let project = run.root().join("hierarchy-drag.parchmint");
    let harness = create_project(&run, &project, "Hierarchy Drag");

    create_group(&harness, "Manuscript", "Part One");
    create_document(&harness, "Part One", "Chapter One");
    create_document(&harness, "Part One", "Chapter Two");
    let chapter_one = harness
        .hierarchy_node("Chapter One")
        .expect("resolve first chapter");
    let chapter_two = harness
        .hierarchy_node("Chapter Two")
        .expect("resolve second chapter");
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Cards),
        )
        .expect("open Cards for direct manipulation");
    harness
        .drag_hierarchy_node(
            HarnessWindow::Project,
            HarnessHierarchySurface::Cards,
            chapter_one.clone(),
            chapter_two.clone(),
            HarnessDropPosition::After,
        )
        .expect("drag the first card after the second");
    assert_order(
        &harness,
        &["Chapter Two", "Chapter One"],
        "Cards drag should reorder sibling chapters",
    );
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Editor),
        )
        .expect("return to Explorer");
    harness
        .drag_hierarchy_node(
            HarnessWindow::Project,
            HarnessHierarchySurface::Explorer,
            chapter_two,
            chapter_one,
            HarnessDropPosition::After,
        )
        .expect("drag the first Explorer chapter after the second");
    assert_order(
        &harness,
        &["Chapter One", "Chapter Two"],
        "Explorer drag should reorder sibling chapters",
    );
    harness
        .close(HarnessWindow::Project)
        .expect("close hierarchy-drag project");
    harness.shutdown().expect("stop hierarchy-drag project");
}

#[test]
fn novelist_can_turn_inspector_synopses_into_a_cards_outline() {
    let run = IsolatedRun::new("inspector-outline").expect("isolated run");
    let project = run.root().join("inspector-outline.parchmint");
    let harness = create_project(&run, &project, "Inspector Outline");

    create_group(&harness, "Manuscript", "Act One");
    create_document(&harness, "Act One", "Opening Image");
    let opening_image = harness
        .hierarchy_node("Opening Image")
        .expect("resolve the first chapter");
    harness
        .click_hierarchy_node(HarnessWindow::Project, opening_image)
        .expect("select the first chapter for Inspector editing");
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::InspectorSynopsis)
        .expect("focus Inspector synopsis");
    assert!(
        harness
            .target_is_focused(HarnessWindow::Project, HarnessTarget::InspectorSynopsis)
            .expect("inspect synopsis focus")
    );
    harness
        .type_focused(
            HarnessWindow::Project,
            "A storm puts the harbor under glass.",
        )
        .expect("write a chapter synopsis in Inspector");
    create_document(&harness, "Act One", "The First Choice");
    let first_choice = harness
        .hierarchy_node("The First Choice")
        .expect("resolve the second chapter");
    harness
        .click_hierarchy_node(HarnessWindow::Project, first_choice)
        .expect("select the second chapter for Inspector editing");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::InspectorSynopsis,
            "The protagonist chooses the impossible crossing.",
        )
        .expect("write the second chapter synopsis in Inspector");
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Cards),
        )
        .expect("review the outline in Cards");
    assert!(visible(&harness, "A storm puts the harbor under glass."));
    assert!(visible(
        &harness,
        "The protagonist chooses the impossible crossing."
    ));
    harness
        .close(HarnessWindow::Project)
        .expect("close inspector-outline project");
    harness.shutdown().expect("stop inspector-outline project");
}

#[test]
fn editor_selection_popover_can_create_reply_resolve_and_delete_a_comment() {
    let run = IsolatedRun::new("comment-popover").expect("isolated run");
    let project = run.root().join("comment-popover.parchmint");
    let harness = create_project(&run, &project, "Comment Popover");
    let draft = "The lighthouse keeper waits through the storm.";

    harness
        .type_into_target(HarnessWindow::Project, HarnessTarget::EditorPrimary, draft)
        .expect("draft a commentable sentence");
    harness
        .select_editor_text(
            HarnessWindow::Project,
            EditorPane::Primary,
            "lighthouse keeper",
        )
        .expect("select prose through its stable document anchor");
    harness
        .right_click_target_at(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            (0.5, 0.5),
        )
        .expect("open the selected-text popover");
    assert!(visible(&harness, "Add Comment"));
    harness
        .click_text(HarnessWindow::Project, "Add Comment")
        .expect("begin a comment from the popover");
    assert!(
        visible(&harness, "Write a comment for the selected anchor."),
        "comment feedback: {}",
        harness
            .comment_feedback()
            .expect("read comment composer feedback")
    );
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::CommentDraft)
        .expect("focus the comment draft");
    assert!(
        harness
            .target_is_focused(HarnessWindow::Project, HarnessTarget::CommentDraft)
            .expect("inspect comment-draft focus"),
        "the comment draft must receive keyboard focus"
    );
    harness
        .type_focused(HarnessWindow::Project, "Verify the weather detail.")
        .expect("write a comment draft");
    harness
        .click_text(HarnessWindow::Project, "Add at selection")
        .expect("attach the comment to the selection");
    assert!(visible(&harness, "Unresolved"));
    assert!(
        visible(&harness, "Verify the weather detail."),
        "comment feedback after creation: {}",
        harness
            .comment_feedback()
            .expect("read comment creation feedback")
    );
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::CommentReply,
            "Confirmed against the log.",
        )
        .expect("reply to the comment thread");
    harness
        .click_text(HarnessWindow::Project, "Reply")
        .expect("submit the comment reply");
    assert!(visible(&harness, "Reply: Confirmed against the log."));
    harness
        .click_text(HarnessWindow::Project, "Resolve")
        .expect("resolve the comment thread");
    assert!(visible(&harness, "Resolved"));
    assert!(visible(&harness, "Verify the weather detail."));
    harness
        .click_text(HarnessWindow::Project, "Delete thread")
        .expect("request comment deletion");
    harness
        .click_text(HarnessWindow::Project, "Confirm delete")
        .expect("delete the comment thread");
    assert!(visible(&harness, "No comments"));
    harness
        .close(HarnessWindow::Project)
        .expect("close comment project");
    harness.shutdown().expect("stop comment project");
}

#[test]
fn revision_author_can_search_and_replace_a_draft_phrase() {
    let run = IsolatedRun::new("revision-search").expect("isolated run");
    let project = run.root().join("revision.parchmint");
    let before = "The river kept its counsel.";
    let after = "The sea kept its counsel.";
    let harness = create_project(&run, &project, "Revision Notes");

    harness
        .type_into_target(HarnessWindow::Project, HarnessTarget::EditorPrimary, before)
        .expect("draft revision sentence");
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ExplorerSearch)
        .expect("open global search");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::GlobalSearchQuery,
            "river",
        )
        .expect("search draft phrase");
    assert!(visible(&harness, "1 match in 1 document"));
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::GlobalReplacement,
            "sea",
        )
        .expect("enter replacement");
    harness
        .click_text(HarnessWindow::Project, "Replace")
        .expect("review replacement");
    assert!(visible(&harness, "Replace Preview"));
    harness
        .click_text(HarnessWindow::Project, "Revalidate selection")
        .expect("revalidate replacement");
    harness
        .click_text(HarnessWindow::Project, "Apply replacement")
        .expect("apply replacement");
    let revised = harness.active_editor_body().expect("read revised draft");
    assert!(revised.contains(after), "actual draft: {revised:?}");
    harness
        .close(HarnessWindow::Project)
        .expect("close revised project");
    harness.shutdown().expect("stop revised project");
}

#[test]
fn revision_author_can_replace_a_phrase_after_the_draft_is_saved() {
    let run = IsolatedRun::new("saved-revision-search").expect("isolated run");
    let project = run.root().join("saved-revision.parchmint");
    let before = "The river kept its counsel.";
    let after = "The sea kept its counsel.";
    let harness = create_project(&run, &project, "Saved Revision Notes");

    harness
        .type_into_target(HarnessWindow::Project, HarnessTarget::EditorPrimary, before)
        .expect("draft revision sentence");
    harness
        .elapse_autosave_idle()
        .expect("save the draft before project-wide search");
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ExplorerSearch)
        .expect("open global search");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::GlobalSearchQuery,
            "river",
        )
        .expect("search saved draft phrase");
    assert!(visible(&harness, "1 match in 1 document"));
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::GlobalReplacement,
            "sea",
        )
        .expect("enter project-wide replacement");
    harness
        .click_text(HarnessWindow::Project, "Replace")
        .expect("review replacement");
    assert!(visible(&harness, "Replace Preview"));
    harness
        .click_text(HarnessWindow::Project, "Revalidate selection")
        .expect("revalidate the saved result");
    assert!(
        visible(
            &harness,
            "Selected matches are revalidated and ready to apply atomically.",
        ),
        "replacement selection was not ready to apply: {}",
        harness
            .replacement_status()
            .expect("read replacement status")
    );
    harness
        .click_text(HarnessWindow::Project, "Apply replacement")
        .expect("apply the global replacement");
    assert!(
        !visible(&harness, "Replace Preview"),
        "replacement did not complete; needs attention={}",
        visible(&harness, "Preview needs attention")
    );
    let revised = harness.active_editor_body().expect("read revised draft");
    assert!(revised.contains(after), "actual draft: {revised:?}");
    harness
        .close(HarnessWindow::Project)
        .expect("close saved revision project");
    harness.shutdown().expect("stop saved revision project");
}

#[test]
fn anthology_editor_can_build_a_collection_outline() {
    let run = IsolatedRun::new("anthology-outline").expect("isolated run");
    let project = run.root().join("lantern-collection.parchmint");
    let harness = create_project(&run, &project, "Lantern Collection");

    create_group(&harness, "Manuscript", "Stories");
    create_document(&harness, "Stories", "The First Lantern");
    create_document(&harness, "Stories", "The Ash Orchard");
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Cards),
        )
        .expect("review collection cards");
    assert!(visible(&harness, "Stories"));
    assert!(visible(&harness, "The First Lantern"));
    assert!(visible(&harness, "The Ash Orchard"));
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Editor),
        )
        .expect("return to collection editor");
    harness
        .close(HarnessWindow::Project)
        .expect("close collection outline");
    harness.shutdown().expect("stop collection outline");
}

#[test]
fn historical_novelist_can_keep_research_notes_separate_from_the_manuscript() {
    let run = IsolatedRun::new("historical-research").expect("isolated run");
    let project = run.root().join("harbor-history.parchmint");
    let harness = create_project(&run, &project, "Harbor History");

    create_group(&harness, "Research", "Harbor Records");
    create_document(&harness, "Harbor Records", "Lighthouse Log");
    harness
        .right_click_text(HarnessWindow::Project, "Lighthouse Log")
        .expect("open research-note context menu");
    harness
        .click_text(HarnessWindow::Project, "Open in companion")
        .expect("open the research note in the companion pane");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorCompanion,
            "The light failed at midnight on 14 November.",
        )
        .expect("draft research note in the companion pane");
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Cards),
        )
        .expect("review research notes in Cards");
    assert!(visible(&harness, "Harbor Records"));
    assert!(visible(&harness, "Lighthouse Log"));
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Export),
        )
        .expect("review manuscript-only export");
    assert!(visible(
        &harness,
        "Excludes Synopsis, metadata, comments, and Research."
    ));
    harness
        .close(HarnessWindow::Project)
        .expect("close historical research project");
    harness
        .shutdown()
        .expect("stop historical research project");
}

#[test]
fn research_heavy_novelist_can_plan_cards_and_draft_beside_source_notes() {
    let run = IsolatedRun::new("research-heavy-novel").expect("isolated run");
    let project = run.root().join("research-heavy-novel.parchmint");
    let harness = create_project(&run, &project, "The Salt Archive");

    create_group(&harness, "Research", "Harbor Records");
    create_document(&harness, "Harbor Records", "Lighthouse Log");
    harness
        .right_click_text(HarnessWindow::Project, "Lighthouse Log")
        .expect("open first research-note context menu");
    harness
        .click_text(HarnessWindow::Project, "Open in companion")
        .expect("open the first research note in the companion pane");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorCompanion,
            "The light failed at midnight on 14 November.",
        )
        .expect("write the lighthouse research note in the companion pane");
    create_document(&harness, "Harbor Records", "Pilot Interview");
    harness
        .right_click_text(HarnessWindow::Project, "Pilot Interview")
        .expect("open second research-note context menu");
    harness
        .click_text(HarnessWindow::Project, "Open in companion")
        .expect("open the second research note in the companion pane");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorCompanion,
            "Pilots refused the north channel after the glass tide.",
        )
        .expect("write a second research note in the companion pane");

    create_group(&harness, "Manuscript", "Act One");
    create_document(&harness, "Act One", "Opening Image");
    let opening_image = harness
        .hierarchy_node("Opening Image")
        .expect("resolve the opening chapter");
    harness
        .click_hierarchy_node(HarnessWindow::Project, opening_image)
        .expect("select the opening chapter for Inspector editing");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::InspectorSynopsis,
            "A storm seals the harbor and strands the cartographer.",
        )
        .expect("outline the opening chapter in Inspector");
    create_document(&harness, "Act One", "The Crossing");
    let crossing = harness
        .hierarchy_node("The Crossing")
        .expect("resolve the crossing chapter");
    harness
        .click_hierarchy_node(HarnessWindow::Project, crossing)
        .expect("select the crossing chapter for Inspector editing");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::InspectorSynopsis,
            "The cartographer chooses the forbidden channel.",
        )
        .expect("outline the second chapter in Inspector");
    create_group(&harness, "Manuscript", "Act Two");
    create_document(&harness, "Act Two", "The Archive");
    let archive = harness
        .hierarchy_node("The Archive")
        .expect("resolve the archive chapter");
    harness
        .click_hierarchy_node(HarnessWindow::Project, archive)
        .expect("select the archive chapter for Inspector editing");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::InspectorSynopsis,
            "An old pilot log reveals why the tide turns to glass.",
        )
        .expect("outline the later chapter in Inspector");

    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Cards),
        )
        .expect("review the novel outline in Cards");
    assert!(visible(&harness, "Opening Image"));
    assert!(visible(&harness, "The Crossing"));
    assert!(visible(&harness, "The Archive"));
    assert!(visible(
        &harness,
        "A storm seals the harbor and strands the cartographer."
    ));
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Editor),
        )
        .expect("return to drafting beside research");
    harness
        .right_click_text(HarnessWindow::Project, "Opening Image")
        .expect("open the opening chapter");
    harness
        .click_text(HarnessWindow::Project, "Open")
        .expect("open the manuscript chapter in the primary pane");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "Rain carried the harbor bells across the water.",
        )
        .expect("draft prose while research remains available in the companion pane");
    assert!(
        harness
            .active_editor_body()
            .expect("read current manuscript chapter")
            .contains("Rain carried the harbor bells across the water.")
    );
    harness
        .close(HarnessWindow::Project)
        .expect("close research-heavy novel");
    harness.shutdown().expect("stop research-heavy novel");
}

#[test]
fn collection_editor_can_restore_a_deleted_story() {
    let run = IsolatedRun::new("collection-restore").expect("isolated run");
    let project = run.root().join("lantern-collection.parchmint");
    let harness = create_project(&run, &project, "Lantern Collection");

    create_group(&harness, "Manuscript", "Stories");
    create_document(&harness, "Stories", "The First Lantern");
    let first_lantern = harness
        .hierarchy_node("The First Lantern")
        .expect("resolve story to delete");
    harness
        .click_hierarchy_node(HarnessWindow::Project, first_lantern)
        .expect("select story before deleting it");
    harness
        .right_click_text(HarnessWindow::Project, "The First Lantern")
        .expect("open story context menu");
    harness
        .click_text(HarnessWindow::Project, "Delete")
        .expect("delete story from collection");
    let hierarchy_after_deletion = harness
        .hierarchy_titles()
        .expect("read hierarchy after deletion");
    assert!(
        !hierarchy_after_deletion
            .iter()
            .any(|title| title == "The First Lantern"),
        "deleted story must leave the Explorer; actual hierarchy: {hierarchy_after_deletion:?}"
    );
    assert!(
        !visible(&harness, "The First Lantern"),
        "deleted story must not remain in an editor tab or pane"
    );

    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::RecentlyDeleted),
        )
        .expect("open Recently Deleted");
    harness
        .click_text(HarnessWindow::Project, "The First Lantern")
        .expect("select deleted story");
    harness
        .click_text(HarnessWindow::Project, "Restore item")
        .expect("restore deleted story");
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Editor),
        )
        .expect("return to collection editor");
    assert!(visible(&harness, "Stories"));
    assert!(visible(&harness, "The First Lantern"));
    harness
        .close(HarnessWindow::Project)
        .expect("close collection");
    harness.shutdown().expect("stop collection application");
}

fn create_project(run: &IsolatedRun, project: &PathBuf, title: &str) -> DesktopInteractionHarness {
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
        .replace_text_and_submit(HarnessWindow::Project, "New Group", title)
        .expect("name group");
}

fn create_document(harness: &DesktopInteractionHarness, parent: &str, title: &str) {
    harness
        .right_click_text(HarnessWindow::Project, parent)
        .expect("open parent context menu");
    harness
        .click_text(HarnessWindow::Project, "Create document")
        .expect("create document");
    harness
        .replace_text_and_submit(HarnessWindow::Project, "Untitled", title)
        .expect("name document");
}

fn visible(harness: &DesktopInteractionHarness, text: &str) -> bool {
    harness
        .contains_text(HarnessWindow::Project, text)
        .expect("query project surface")
}

fn assert_order(harness: &DesktopInteractionHarness, expected: &[&str], message: &str) {
    let titles = harness.hierarchy_titles().expect("read hierarchy order");
    let positions = expected
        .iter()
        .map(|title| {
            titles
                .iter()
                .position(|candidate| candidate == title)
                .expect("expected hierarchy title")
        })
        .collect::<Vec<_>>();
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "{message}; actual hierarchy: {titles:?}"
    );
}

fn canonical_bodies(project: &std::path::Path) -> Vec<String> {
    ["manuscript", "research"]
        .into_iter()
        .flat_map(|directory| canonical_bodies_in(&project.join(directory)))
        .collect()
}

fn canonical_bodies_in(directory: &std::path::Path) -> Vec<String> {
    fs::read_dir(directory)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .flat_map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                canonical_bodies_in(&path)
            } else if path.extension().is_some_and(|value| value == "html") {
                vec![fs::read_to_string(path).expect("read canonical document")]
            } else {
                Vec::new()
            }
        })
        .collect()
}
