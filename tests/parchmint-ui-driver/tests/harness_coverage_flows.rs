use std::{fs, path::Path, time::Duration};

use parchmint_desktop::{
    DesktopInteractionHarness, EditorPane, HarnessKey, HarnessSelectionGesture, HarnessTarget,
    HarnessWindow, LaunchRequest, ProductionFaultKind, ProductionFaultPoint, ProductionObservation,
    RibbonDestination,
};
use parchmint_ui_driver::IsolatedRun;

#[test]
fn explorer_opening_keeps_editor_shortcuts_and_reselecting_keeps_the_selection() {
    let run = IsolatedRun::new("explorer-open-editor-focus").expect("isolated run");
    let project = run.root().join("explorer-open-editor-focus.parchmint");
    let harness = create_project(&run, &project, "Explorer Editor Focus");

    create_document(&harness, "Manuscript", "Focused Scene");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "Explorerfocus prose.",
        )
        .expect("write scene prose");
    let document_id = harness
        .active_editor_document_id(EditorPane::Primary)
        .expect("read created document identity");
    harness
        .close_editor_tab(HarnessWindow::Project, EditorPane::Primary, document_id)
        .expect("close the created tab before previewing it from Explorer");

    let scene = harness
        .hierarchy_node("Focused Scene")
        .expect("resolve Explorer scene");
    harness
        .click_hierarchy_node(HarnessWindow::Project, scene.clone())
        .expect("open the scene from Explorer");
    harness
        .multi_click_editor_text(
            HarnessWindow::Project,
            EditorPane::Primary,
            "Explorerfocus",
            2,
        )
        .expect("select a word with native double-click");
    harness
        .click_hierarchy_node(HarnessWindow::Project, scene)
        .expect("reselect the already active Explorer document");
    harness
        .press_command_key(HarnessWindow::Project, 'b')
        .expect("format the retained selection immediately after Explorer navigation");
    harness
        .elapse_autosave_idle()
        .expect("persist formatted Explorer prose");

    let bodies = canonical_bodies(&project);
    assert!(
        bodies
            .iter()
            .any(|body| body.contains("<strong>Explorerfocus</strong> prose.")),
        "Explorer selection must not remount or lose editor focus; bodies: {bodies:?}"
    );
    close(harness);
}

#[test]
fn native_triple_click_selects_a_paragraph_for_formatting() {
    let run = IsolatedRun::new("editor-triple-click").expect("isolated run");
    let project = run.root().join("editor-triple-click.parchmint");
    let harness = create_project(&run, &project, "Editor Triple Click");

    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "First paragraph.",
        )
        .expect("write the first paragraph");
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Enter)
        .expect("split the manuscript into a second paragraph");
    harness
        .type_focused(HarnessWindow::Project, "Second paragraph.")
        .expect("write the second paragraph");
    let body = harness.active_editor_body().expect("read split manuscript");
    assert!(body.contains("Second paragraph."), "body was {body:?}");
    harness
        .multi_click_editor_text(HarnessWindow::Project, EditorPane::Primary, "Second", 3)
        .expect("select the second paragraph with native triple-click");
    harness
        .press_command_key(HarnessWindow::Project, 'i')
        .expect("format the triple-click paragraph");
    harness
        .elapse_autosave_idle()
        .expect("persist formatted paragraph");

    let bodies = canonical_bodies(&project);
    assert!(
        bodies
            .iter()
            .any(|body| body.contains("<em>Second paragraph.</em>")),
        "triple-click should select the paragraph, not only the clicked word; bodies: {bodies:?}"
    );
    close(harness);
}

#[test]
fn editor_clipboard_flow_copies_and_sanitizes_external_rich_text() {
    let run = IsolatedRun::new("clipboard-authoring").expect("isolated run");
    let project = run.root().join("clipboard-authoring.parchmint");
    let harness = create_project(&run, &project, "Clipboard Authoring");

    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "Before selected prose after.",
        )
        .expect("write source prose");
    harness
        .select_editor_text(
            HarnessWindow::Project,
            EditorPane::Primary,
            "selected prose",
        )
        .expect("select copied prose");
    harness
        .press_command_key(HarnessWindow::Project, 'c')
        .expect("copy selection through the standard shortcut");
    assert_eq!(
        harness.clipboard_contents().0.as_deref(),
        Some("selected prose"),
        "production copy should publish the selected plain text"
    );

    harness.seed_clipboard(
        Some("Plain fallback"),
        Some("<p><strong>Refined</strong> prose<script>unsafe()</script><img src=\"x\"></p>"),
    );
    harness
        .press_command_key(HarnessWindow::Project, 'v')
        .expect("paste seeded external rich text");
    let body = harness.active_editor_body().expect("read pasted prose");
    assert!(
        body.contains("<strong>Refined</strong> prose"),
        "body was {body:?}"
    );
    assert!(!body.contains("unsafe"), "body was {body:?}");
    assert!(!body.contains("Plain fallback"), "body was {body:?}");
    close(harness);
}

#[test]
fn organizer_can_multiselect_copy_cut_and_paste_with_standard_shortcuts() {
    let run = IsolatedRun::new("tree-clipboard-authoring").expect("isolated run");
    let project = run.root().join("tree-clipboard-authoring.parchmint");
    let harness = create_project(&run, &project, "Tree Clipboard Authoring");

    create_group(&harness, "Manuscript", "Source");
    create_group(&harness, "Manuscript", "Destination");
    for title in ["One", "Two", "Three"] {
        create_document(&harness, "Source", title);
    }
    let one = harness.hierarchy_node("One").expect("resolve One");
    let three = harness.hierarchy_node("Three").expect("resolve Three");
    let destination = harness
        .hierarchy_node("Destination")
        .expect("resolve destination");

    harness
        .select_hierarchy_node(
            HarnessWindow::Project,
            one.clone(),
            HarnessSelectionGesture::Replace,
        )
        .expect("select range anchor");
    harness
        .select_hierarchy_node(
            HarnessWindow::Project,
            three.clone(),
            HarnessSelectionGesture::ContiguousRange,
        )
        .expect("extend contiguous selection");
    let selected = harness
        .hierarchy()
        .expect("read hierarchy selection")
        .into_iter()
        .filter(|entry| entry.selected)
        .map(|entry| entry.title)
        .collect::<Vec<_>>();
    assert_eq!(selected, ["One", "Two", "Three"]);
    harness
        .press_command_key(HarnessWindow::Project, 'c')
        .expect("copy the selected chapters");
    harness
        .select_hierarchy_node(
            HarnessWindow::Project,
            destination.clone(),
            HarnessSelectionGesture::Replace,
        )
        .expect("select copy destination");
    harness
        .press_command_key(HarnessWindow::Project, 'v')
        .expect("paste copied chapters into selected group");
    let hierarchy = harness.hierarchy().expect("read copied hierarchy");
    let destination_id = hierarchy
        .iter()
        .find(|entry| entry.title == "Destination")
        .expect("destination entry")
        .id
        .clone();
    for title in ["One Copy", "Two Copy", "Three Copy"] {
        assert!(
            hierarchy
                .iter()
                .any(|entry| entry.title == title
                    && entry.parent_id.as_deref() == Some(&destination_id)),
            "copied hierarchy was {hierarchy:?}"
        );
    }

    harness
        .select_hierarchy_node(
            HarnessWindow::Project,
            three.clone(),
            HarnessSelectionGesture::Replace,
        )
        .expect("select source chapter to cut");
    harness
        .press_command_key(HarnessWindow::Project, 'x')
        .expect("cut selected chapter");
    assert!(
        harness
            .hierarchy()
            .expect("read pending cut")
            .into_iter()
            .any(|entry| entry.title == "Three" && entry.cut_pending),
        "cut chapter should remain visible while pending"
    );
    harness
        .press_key(
            HarnessWindow::Project,
            parchmint_desktop::HarnessKey::Escape,
        )
        .expect("cancel pending cut");
    assert!(
        !harness
            .hierarchy()
            .expect("read cancelled cut")
            .into_iter()
            .any(|entry| entry.title == "Three" && entry.cut_pending),
        "Escape should clear pending cut state"
    );
    close(harness);
}

#[test]
fn author_can_drag_a_research_note_onto_the_primary_pane() {
    let run = IsolatedRun::new("pane-drop-authoring").expect("isolated run");
    let project = run.root().join("pane-drop-authoring.parchmint");
    let harness = create_project(&run, &project, "Pane Drop Authoring");

    create_document(&harness, "Research", "Harbor Ledger");
    let note = harness
        .hierarchy_node("Harbor Ledger")
        .expect("resolve research note");
    let note_document_id = harness
        .hierarchy()
        .expect("read research hierarchy")
        .into_iter()
        .find(|entry| entry.title == "Harbor Ledger")
        .and_then(|entry| entry.document_id)
        .expect("research note document identity");
    let before = harness
        .hierarchy_titles()
        .expect("read hierarchy before drop");
    harness
        .drag_hierarchy_node_to_pane(HarnessWindow::Project, note, EditorPane::Primary)
        .expect("drop research note onto primary pane");
    assert_eq!(
        harness
            .active_editor_document_id(EditorPane::Primary)
            .expect("read primary-pane document"),
        note_document_id,
        "dropping a document on a pane should open it there"
    );
    assert_eq!(
        harness
            .hierarchy_titles()
            .expect("read hierarchy after drop"),
        before,
        "opening by pane drop must not rearrange the hierarchy"
    );
    close(harness);
}

#[test]
fn author_can_focus_a_two_pane_comparison_and_restore_its_sidebars() {
    let run = IsolatedRun::new("focused-two-pane-authoring").expect("isolated run");
    let project = run.root().join("focused-two-pane-authoring.parchmint");
    let harness = create_project(&run, &project, "Focused Two Pane Authoring");

    create_document(&harness, "Research", "Harbor Ledger");
    let note = harness
        .hierarchy_node("Harbor Ledger")
        .expect("resolve companion note");
    harness
        .drag_hierarchy_node_to_pane(HarnessWindow::Project, note, EditorPane::Companion)
        .expect("open the research note beside the manuscript");
    assert!(
        harness
            .target_is_visible(HarnessWindow::Project, HarnessTarget::ExplorerAdd)
            .expect("inspect Explorer visibility before focusing")
    );
    harness
        .click_text(HarnessWindow::Project, "Focus pane")
        .expect("focus the two-pane comparison");
    assert!(
        !harness
            .target_is_visible(HarnessWindow::Project, HarnessTarget::ExplorerAdd)
            .expect("Explorer should be hidden while panes are focused")
    );
    harness
        .click_text(HarnessWindow::Project, "Restore panes")
        .expect("restore authoring sidebars");
    assert!(
        harness
            .target_is_visible(HarnessWindow::Project, HarnessTarget::ExplorerAdd)
            .expect("Explorer should return to its prior visibility")
    );
    close(harness);
}

#[test]
fn author_opens_existing_global_search_with_the_standard_project_shortcut() {
    let run = IsolatedRun::new("global-search-shortcut-authoring").expect("isolated run");
    let project = run
        .root()
        .join("global-search-shortcut-authoring.parchmint");
    let harness = create_project(&run, &project, "Global Search Shortcut");

    harness
        .press_command_shift_key(HarnessWindow::Project, 'f')
        .expect("open existing Global Search with the standard project shortcut");
    assert!(
        harness
            .target_is_visible(HarnessWindow::Project, HarnessTarget::GlobalSearchQuery)
            .expect("Global Search query should receive the shortcut destination")
    );
    close(harness);
}

#[test]
fn history_flow_preserves_meaningful_checkpoints_and_records_restoration() {
    let run = IsolatedRun::new("history-quality-authoring").expect("isolated run");
    let project = run.root().join("history-quality-authoring.parchmint");
    let harness = create_project(&run, &project, "History Quality Authoring");

    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "The first revision stays available.",
        )
        .expect("write first revision");
    harness.elapse_autosave_idle().expect("save first revision");
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::History),
        )
        .expect("open writing timeline");
    let initial = harness.history_checkpoints().expect("read first timeline");
    assert!(
        initial
            .iter()
            .all(|checkpoint| checkpoint.recorded_at_unix_millis.is_some()),
        "new History checkpoints must retain their real display time: {initial:?}"
    );
    assert!(
        initial.iter().any(|checkpoint| checkpoint
            .timeline_heading
            .as_deref()
            .is_some_and(|heading| { heading.contains("UTC · Writing session") })),
        "the timeline should group checkpoints into dated writing sessions: {initial:?}"
    );
    let first = initial
        .iter()
        .find(|checkpoint| checkpoint.category == "Automatic save")
        .expect("automatic checkpoint for first revision")
        .clone();

    harness
        .press_command_key(HarnessWindow::Project, 's')
        .expect("save an unchanged project");
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::History),
        )
        .expect("refresh writing timeline after no-op save");
    assert_eq!(
        harness
            .history_checkpoints()
            .expect("read no-op timeline")
            .len(),
        initial.len(),
        "an unchanged explicit save must reuse its existing checkpoint"
    );

    harness
        .type_into(
            HarnessWindow::Project,
            "Milestone name",
            "First complete draft",
        )
        .expect("name a milestone");
    harness
        .press_key(HarnessWindow::Project, parchmint_desktop::HarnessKey::Enter)
        .expect("create named milestone");
    assert!(
        harness
            .history_checkpoints()
            .expect("read named timeline")
            .iter()
            .any(|checkpoint| {
                checkpoint.category == "Named snapshot"
                    && checkpoint.label == "First complete draft"
            }),
        "named milestone should remain distinct from save checkpoints"
    );

    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Editor),
        )
        .expect("return to draft");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            " The later revision is restored away.",
        )
        .expect("write later revision");
    harness.elapse_autosave_idle().expect("save later revision");
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::History),
        )
        .expect("return to timeline");
    let before_restore = harness
        .history_checkpoints()
        .expect("read timeline before restore");
    assert!(
        before_restore
            .iter()
            .any(|checkpoint| checkpoint.id == first.id),
        "the older checkpoint must remain available before restoration"
    );
    harness
        .click_history_checkpoint_by_id(HarnessWindow::Project, first.id.clone())
        .expect("select the exact first checkpoint");
    harness
        .click_text(HarnessWindow::Project, "Restore “Automatic save”")
        .expect("request restoration");
    harness
        .click_text(HarnessWindow::Project, "Confirm")
        .expect("confirm restoration");
    let after_restore = harness
        .history_checkpoints()
        .expect("read restored timeline");
    assert!(
        after_restore
            .iter()
            .any(|checkpoint| checkpoint.id == first.id)
            && after_restore
                .iter()
                .any(|checkpoint| checkpoint.category == "Restoration"),
        "restoration must append a new event without erasing the selected checkpoint: {after_restore:?}"
    );
    assert!(
        after_restore.len() > before_restore.len(),
        "restoration must add a checkpoint rather than rewrite history"
    );
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Editor),
        )
        .expect("return to restored document");
    let restored = harness.active_editor_body().expect("read restored prose");
    assert!(restored.contains("The first revision stays available."));
    assert!(!restored.contains("The later revision is restored away."));
    close(harness);
}

#[test]
fn continuous_autosave_and_retry_after_final_save_failure_keep_authoring_safe() {
    let run = IsolatedRun::new("save-resilience-authoring").expect("isolated run");
    let project = run.root().join("save-resilience-authoring.parchmint");
    let harness = create_project(&run, &project, "Save Resilience Authoring");
    let continuous = "Continuous drafting must save after five minutes.";

    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            continuous,
        )
        .expect("begin continuous drafting");
    harness
        .advance_autosave_clock(Duration::from_secs(301), Duration::from_secs(1))
        .expect("cross continuous-writing autosave boundary");
    assert!(
        canonical_bodies(&project)
            .iter()
            .any(|body| body.contains(continuous)),
        "continuous-edit autosave should persist the draft"
    );

    harness
        .type_focused(
            HarnessWindow::Project,
            " A final unsaved sentence remains editable.",
        )
        .expect("continue editing after autosave");
    harness.fail_next(ProductionFaultPoint::FinalSave, ProductionFaultKind::Io);
    harness
        .close(HarnessWindow::Project)
        .expect("request a close whose final save fails");
    assert!(
        harness
            .has_window(HarnessWindow::Project)
            .expect("read retained project window"),
        "a failed final save must retain the project window"
    );
    assert!(
        harness.observations().iter().any(|observation| matches!(
            observation,
            ProductionObservation::FinalSaveFailed { .. }
        )),
        "the production boundary should record the final-save failure"
    );
    assert!(
        harness
            .active_editor_body()
            .expect("read draft after failed close")
            .contains("A final unsaved sentence remains editable."),
        "failed close must leave the latest prose available"
    );
    harness
        .click_text(HarnessWindow::Project, "Try again")
        .expect("retry final save and close");
    assert!(
        !harness
            .has_window(HarnessWindow::Project)
            .expect("read closed project window"),
        "a successful retry should complete close"
    );
    harness.shutdown().expect("stop application");
}

#[test]
fn author_can_export_the_manuscript_to_a_controlled_html_artifact() {
    let run = IsolatedRun::new("export-artifact-authoring").expect("isolated run");
    let project = run.root().join("export-artifact-authoring.parchmint");
    let export = run.root().join("manuscript.html");
    let harness = create_project(&run, &project, "Export Artifact Authoring");

    create_document(&harness, "Manuscript", "Chapter One");
    harness
        .right_click_text(HarnessWindow::Project, "Chapter One")
        .expect("open chapter menu");
    harness
        .click_text(HarnessWindow::Project, "Open")
        .expect("open manuscript chapter");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "Only this manuscript scene belongs in the export.",
        )
        .expect("write manuscript body");
    harness
        .replace_target(
            HarnessWindow::Project,
            HarnessTarget::InspectorSynopsis,
            "Private outline material",
        )
        .expect("set non-exported synopsis");
    create_document(&harness, "Research", "Harbor Archive");
    harness
        .right_click_text(HarnessWindow::Project, "Harbor Archive")
        .expect("open research menu");
    harness
        .click_text(HarnessWindow::Project, "Open in companion")
        .expect("open research note");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorCompanion,
            "Private research must not be exported.",
        )
        .expect("write research body");

    harness.set_next_path_selection(export.clone());
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Export),
        )
        .expect("open export workspace");
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ExportBrowse)
        .expect("select controlled export path");
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ExportStart)
        .expect("export manuscript");
    let html = fs::read_to_string(&export).unwrap_or_else(|error| {
        panic!(
            "read generated export: {error}; export state: {}",
            harness.export_status().expect("read export state")
        )
    });
    assert!(html.starts_with("<!doctype html>"), "export was {html:?}");
    assert!(html.contains("Only this manuscript scene belongs in the export."));
    assert!(html.contains("Chapter One"));
    assert!(!html.contains("Private research must not be exported."));
    assert!(!html.contains("Private outline material"));
    close(harness);
}

#[test]
fn abandoned_session_replays_recovery_journal_without_a_final_close() {
    let run = IsolatedRun::new("recovery-journal-authoring").expect("isolated run");
    let project = run.root().join("recovery-journal-authoring.parchmint");
    let baseline = "The durable opening remains.";
    let recovered = " The recovered ending returns.";
    let harness = create_project(&run, &project, "Recovery Journal Authoring");

    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            baseline,
        )
        .expect("write durable opening");
    harness
        .elapse_autosave_idle()
        .expect("persist durable opening");
    harness
        .type_focused(HarnessWindow::Project, recovered)
        .expect("write recovery-only ending");
    harness
        .elapse_recovery_capture()
        .expect("record the high-frequency recovery projection");
    harness
        .abandon()
        .expect("abandon session without a final close");

    let reopened = DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project))
        .expect("relaunch abandoned project directly");
    assert!(
        contains(&reopened, "Recovered changes are ready"),
        "recovery surface should explain the available journal replay"
    );
    reopened
        .click_text(HarnessWindow::Project, "Recover changes")
        .expect("accept recovered changes");
    let body = reopened.active_editor_body().expect("read recovered draft");
    assert!(
        body.contains(baseline) && body.contains(recovered),
        "body was {body:?}"
    );
    close(reopened);
}

fn create_project(run: &IsolatedRun, project: &Path, title: &str) -> DesktopInteractionHarness {
    let harness = DesktopInteractionHarness::launch(run.root(), LaunchRequest::launcher())
        .expect("launch application");
    harness
        .click_text(HarnessWindow::Launcher, "Create Project")
        .expect("open project creation");
    harness
        .type_into(HarnessWindow::Launcher, "Project title", title)
        .expect("set project title");
    harness
        .type_into(
            HarnessWindow::Launcher,
            "Project destination",
            project.display().to_string(),
        )
        .expect("set project destination");
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
    harness
        .redraw(HarnessWindow::Project)
        .expect("render group");
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
    harness
        .redraw(HarnessWindow::Project)
        .expect("render document");
}

fn canonical_bodies(project: &Path) -> Vec<String> {
    ["manuscript", "research"]
        .into_iter()
        .flat_map(|directory| canonical_bodies_in(&project.join(directory)))
        .collect()
}

fn canonical_bodies_in(directory: &Path) -> Vec<String> {
    fs::read_dir(directory)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .flat_map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                canonical_bodies_in(&path)
            } else if path
                .extension()
                .is_some_and(|extension| extension == "html")
            {
                vec![fs::read_to_string(path).expect("read canonical document")]
            } else {
                Vec::new()
            }
        })
        .collect()
}

fn contains(harness: &DesktopInteractionHarness, text: &str) -> bool {
    harness
        .contains_text(HarnessWindow::Project, text)
        .expect("query visible text")
}

fn close(harness: DesktopInteractionHarness) {
    harness
        .close(HarnessWindow::Project)
        .expect("close project window");
    harness.shutdown().expect("stop application");
}
