use std::path::PathBuf;

use parchmint_desktop::{
    DesktopInteractionHarness, EditorPane, FocusTarget, HarnessTarget, HarnessWindow,
    LaunchRequest, RibbonDestination,
};
use parchmint_ui_driver::IsolatedRun;

#[test]
fn authoring_refinements_remain_coherent_across_cards_editor_search_comments_and_history() {
    let run = IsolatedRun::new("authoring-refinements").expect("isolated run");
    let project = run.root().join("authoring-refinements.parchmint");
    let harness = create_project(&run, &project, "Harbor Archive");

    create_group(&harness, "Manuscript", "Act One");
    create_document(&harness, "Act One", "Opening Watch");
    create_group(&harness, "Research", "Sources");
    create_document(&harness, "Sources", "Tide Ledger");

    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Settings),
        )
        .expect("open settings to configure card metadata");
    harness
        .click_text(HarnessWindow::Project, "Metadata fields")
        .expect("open metadata settings");
    harness
        .click_text(HarnessWindow::Project, "+ New field")
        .expect("start metadata-field creation");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::MetadataFieldName,
            "Point of view",
        )
        .expect("name metadata field");
    harness
        .click_text(HarnessWindow::Project, "Add field")
        .expect("add metadata field");
    harness
        .click_text(HarnessWindow::Project, "Visible on cards")
        .expect("show generic metadata field on cards");

    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Cards),
        )
        .expect("open manuscript Cards");
    let act_one = harness.hierarchy_node("Act One").expect("resolve Act One");
    let opening_watch = harness
        .hierarchy_node("Opening Watch")
        .expect("resolve Opening Watch");
    assert!(
        harness
            .cards_node_is_visible(HarnessWindow::Project, act_one.clone())
            .expect("observe mounted group card")
    );
    harness
        .click_cards_node(HarnessWindow::Project, act_one.clone())
        .expect("collapse group from its Cards row");
    assert!(
        !harness
            .cards_node_is_visible(HarnessWindow::Project, opening_watch.clone())
            .expect("observe collapsed group card"),
        "a Cards group click must hide its children"
    );
    harness
        .click_cards_node(HarnessWindow::Project, act_one)
        .expect("expand group from its Cards row");
    assert!(
        harness
            .cards_node_is_visible(HarnessWindow::Project, opening_watch.clone())
            .expect("observe expanded document card")
    );
    assert!(
        visible(&harness, "Point of view: —"),
        "Cards must expose metadata with labelled generic chip text"
    );
    let previously_active_tab = harness
        .active_editor_tab_title()
        .expect("read tab active before selecting a document card");
    harness
        .click_cards_node(HarnessWindow::Project, opening_watch.clone())
        .expect("select document from Cards");
    assert_eq!(
        harness
            .active_editor_tab_title()
            .expect("read selected document tab"),
        previously_active_tab,
        "a single document-card click must select without activating a tab"
    );
    harness
        .double_click_cards_node(HarnessWindow::Project, opening_watch)
        .expect("activate document from Cards");
    assert!(
        harness
            .target_is_visible(HarnessWindow::Project, HarnessTarget::EditorPrimary)
            .expect("observe Editor after card activation"),
        "double-clicking a document card must visibly switch to Editor"
    );
    assert_eq!(
        harness
            .active_editor_tab_title()
            .expect("read activated document tab"),
        "Opening Watch"
    );

    assert!(visible(&harness, "Manuscript › Act One › Opening Watch"));
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "North wind gathers.",
        )
        .expect("write first history revision");
    harness
        .multi_click_editor_text(HarnessWindow::Project, EditorPane::Primary, "North", 2)
        .expect("select a word through actual editor pointer events");
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::Bold)
        .expect("format the selected word from the toolbar");
    harness
        .type_focused(HarnessWindow::Project, "Storm")
        .expect("continue drafting immediately after toolbar formatting");
    assert!(
        harness
            .active_editor_body()
            .expect("read toolbar-formatted revision")
            .contains("Storm wind gathers."),
        "toolbar formatting must return input focus to the editor"
    );
    harness
        .elapse_autosave_idle()
        .expect("create first automatic checkpoint");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            " Tides turn now.",
        )
        .expect("write later history revision");
    harness
        .elapse_autosave_idle()
        .expect("create later automatic checkpoint");
    assert!(
        !visible(&harness, "Couldn't update the editor"),
        "ordinary drafting and its deferred editor work must never surface an editor failure"
    );
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::History),
        )
        .expect("open project history");
    let checkpoints = harness
        .history_checkpoints()
        .expect("read loaded history checkpoints");
    assert!(
        checkpoints.len() >= 2,
        "two distinct automatic saves should be retained: {checkpoints:?}"
    );
    harness
        .click_history_checkpoint(HarnessWindow::Project, 1)
        .expect("load the earlier automatic checkpoint");
    let history = harness
        .history_status()
        .expect("read loaded history status");
    assert!(
        history.contains("comparison=true"),
        "history status: {history}"
    );
    assert!(
        history.contains("comparison_delta=Some("),
        "history must expose a loaded word-count delta for its selected checkpoint: {history}"
    );
    assert!(visible(&harness, "Checkpoint"));
    assert!(visible(&harness, "Current"));
    assert!(
        !visible(&harness, "Unified"),
        "History must keep its comparison in side-by-side mode"
    );
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Editor),
        )
        .expect("return from history to the manuscript editor");
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::SceneBreak)
        .expect("insert scene break through its semantic toolbar control");
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::PageBreak)
        .expect("insert page break through its semantic toolbar control");
    let body_with_breaks = harness
        .active_editor_body()
        .expect("read manuscript after inserting breaks");
    assert!(body_with_breaks.contains("data-kind=\"scene-break\""));
    assert!(body_with_breaks.contains("data-kind=\"page-break\""));

    harness
        .right_click_text(HarnessWindow::Project, "Tide Ledger")
        .expect("open research document context menu");
    harness
        .click_text(HarnessWindow::Project, "Open in companion")
        .expect("open research note alongside manuscript");
    assert!(
        harness
            .target_is_visible(HarnessWindow::Project, HarnessTarget::EditorCompanion)
            .expect("observe companion editor")
    );
    assert!(visible(&harness, "Research › Sources › Tide Ledger"));

    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Cards),
        )
        .expect("return to Cards before project search");
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ExplorerSearch)
        .expect("open global search from the Explorer-local action");
    assert!(
        harness
            .target_is_visible(HarnessWindow::Project, HarnessTarget::GlobalSearchQuery)
            .expect("observe global-search query field")
    );
    assert!(
        !harness
            .target_is_visible(HarnessWindow::Project, HarnessTarget::EditorPrimary)
            .expect("observe editor omission in Global Search"),
        "Global Search must not retain a responsive editor surface behind its results"
    );
    assert!(
        !visible(&harness, "Manuscript › Act One › Opening Watch"),
        "Global Search must omit editor breadcrumbs with the editor surface"
    );
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Editor),
        )
        .expect("return from global search to Editor");
    assert!(visible(&harness, "Manuscript › Act One › Opening Watch"));

    let primary_document = harness
        .active_editor_document_id(EditorPane::Primary)
        .expect("read focused manuscript document");
    harness
        .select_editor_text(HarnessWindow::Project, EditorPane::Primary, "Storm wind")
        .expect("select anchored comment text");
    harness
        .right_click_target_at(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            (0.5, 0.5),
        )
        .expect("open selected-text actions");
    harness
        .click_text(HarnessWindow::Project, "Add Comment")
        .expect("create a comment draft");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::CommentDraft,
            "Verify this weather detail.",
        )
        .expect("write comment draft");
    harness
        .click_text(HarnessWindow::Project, "Add comment")
        .expect("attach the comment");
    harness
        .move_pointer_to_comment_anchor(HarnessWindow::Project, EditorPane::Primary)
        .expect("hover the comment anchor");
    assert!(visible(&harness, "Attached comment"));
    harness
        .move_pointer_to_editor_text(HarnessWindow::Project, EditorPane::Primary, "Storm wind")
        .expect("keep the popover anchored while hovering its selection");
    assert!(visible(&harness, "Attached comment"));
    harness
        .move_pointer_outside(HarnessWindow::Project)
        .expect("leave the comment anchor before choosing its Inspector entry");
    harness
        .click_text(HarnessWindow::Project, "Verify this weather detail.")
        .expect("select the Inspector's read-only comment entry");
    assert!(visible(&harness, "Verify this weather detail."));
    assert_eq!(
        harness
            .focus_target(HarnessWindow::Project)
            .expect("read focus after selecting hover comment"),
        FocusTarget::EditorDocument(primary_document),
        "selecting the Inspector's comment index should reveal its editor anchor without stealing focus"
    );

    harness
        .close(HarnessWindow::Project)
        .expect("close authoring-refinement project");
    harness
        .shutdown()
        .expect("stop authoring-refinement application");
}

#[test]
fn history_reloads_prior_checkpoints_after_comment_activity() {
    let run = IsolatedRun::new("history-after-comment").expect("isolated run");
    let project = run.root().join("history-after-comment.parchmint");
    let harness = create_project(&run, &project, "History After Comment");

    create_group(&harness, "Research", "Sources");
    create_document(&harness, "Sources", "Tide Ledger");

    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "Lantern light crosses the harbor.",
        )
        .expect("write first revision");
    harness
        .elapse_autosave_idle()
        .expect("create first automatic checkpoint");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            " The tide turns before dawn.",
        )
        .expect("write second revision");
    harness
        .elapse_autosave_idle()
        .expect("create second automatic checkpoint");
    harness
        .right_click_text(HarnessWindow::Project, "Tide Ledger")
        .expect("open research context menu");
    harness
        .click_text(HarnessWindow::Project, "Open in companion")
        .expect("open a companion document");
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Cards),
        )
        .expect("visit Cards before searching");
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ExplorerSearch)
        .expect("open global search from Explorer");
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Editor),
        )
        .expect("return to editor after searching");
    harness
        .select_editor_text(HarnessWindow::Project, EditorPane::Primary, "Lantern light")
        .expect("select comment anchor");
    harness
        .right_click_target_at(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            (0.5, 0.5),
        )
        .expect("open comment actions");
    harness
        .click_text(HarnessWindow::Project, "Add Comment")
        .expect("begin comment");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::CommentDraft,
            "Confirm the lantern image.",
        )
        .expect("write comment");
    harness
        .click_text(HarnessWindow::Project, "Add comment")
        .expect("persist comment");

    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::History),
        )
        .expect("open history after comment activity");
    let checkpoints = harness
        .history_checkpoints()
        .expect("read history after comment activity");
    assert!(
        checkpoints.len() >= 2,
        "comment activity must not empty the loaded history timeline: {checkpoints:?}"
    );

    harness
        .close(HarnessWindow::Project)
        .expect("close history-after-comment project");
    harness
        .shutdown()
        .expect("stop history-after-comment application");
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
