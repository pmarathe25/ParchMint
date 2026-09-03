use std::{collections::BTreeMap, fs, path::Path, time::Duration};

use parchmint_desktop::{
    DesktopInteractionHarness, EditorPane, HarnessTarget, HarnessWindow, LaunchRequest,
    RibbonDestination,
};
use parchmint_domain::{
    DocumentId, NodeId, Project, ProjectCommand, ProjectId, apply_project_command,
};
use parchmint_project_format::{CanonicalProjectPathMap, ProjectFormatCodec};
use parchmint_ui_driver::IsolatedRun;

const LARGE_DOCUMENT_WORDS: usize = 250_000;
const LARGE_DOCUMENT_TITLE: &str = "250K Manuscript";

/// Exercises a large document in a two-pane workspace through
/// repeated continuous-writing autosaves, shared-view verification, History
/// loading, and a full application restart. The virtual clock makes the long
/// session deterministic without a wall-clock wait.
#[test]
fn large_manuscript_survives_sustained_two_pane_authoring_and_restart() {
    let run = IsolatedRun::new("large-document-sustained-authoring").expect("isolated run");
    let project = run
        .root()
        .join("large-document-sustained-authoring.parchmint");
    seed_large_document_project(
        &project,
        "Large Document Sustained Authoring",
        &large_body("word"),
    );

    let harness = DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project))
        .expect("open large-document project");
    let _ = harness.take_diagnostics();
    open_in_both_panes(&harness);
    assert!(
        harness
            .editor_panes_share_session()
            .expect("observe shared two-pane editor session"),
        "two views of one manuscript must share a session before authoring"
    );

    harness
        .click_target(HarnessWindow::Project, HarnessTarget::EditorPrimary)
        .expect("focus primary-pane manuscript session");
    for session in 1..=6 {
        let marker = session.to_string();
        harness
            .type_focused(HarnessWindow::Project, marker.clone())
            .expect("continue primary-pane manuscript session");
        harness
            .advance_autosave_clock(Duration::from_secs(301), Duration::from_secs(1))
            .expect("persist continuous-writing autosave boundary");
        assert_eq!(
            harness
                .active_editor_body()
                .expect("read current primary manuscript")
                .matches(&marker)
                .count(),
            1,
            "session {session} marker must remain present after autosave"
        );
    }
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::EditorCompanion)
        .expect("focus companion view of the shared manuscript");
    let companion_body = harness
        .active_editor_body()
        .expect("read companion large-manuscript view");
    for session in 1..=6 {
        assert_eq!(companion_body.matches(&session.to_string()).count(), 1);
    }

    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::History),
        )
        .expect("open History after repeated large-document autosaves");
    assert!(
        harness
            .history_checkpoints()
            .expect("read large-document history")
            .len()
            >= 6,
        "each changed autosave must remain recoverable in History"
    );
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Editor),
        )
        .expect("return to editor after History load");
    assert_no_error_diagnostics(&harness, "sustained two-pane authoring");
    close(harness);

    let reopened = DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project))
        .expect("reopen large-document project");
    let body = reopened
        .active_editor_body()
        .expect("read reopened manuscript");
    assert_eq!(
        body.matches("word").count(),
        LARGE_DOCUMENT_WORDS,
        "reopen must preserve every original word"
    );
    for session in 1..=6 {
        assert_eq!(body.matches(&session.to_string()).count(), 1);
    }
    assert_no_error_diagnostics(&reopened, "reopen after sustained authoring");
    close(reopened);
}

/// Runs real project-wide replacement against a large document, then
/// saves a follow-up revision, loads History, and verifies canonical storage
/// after a restart. It catches search/replacement work that loses or corrupts
/// content while operating on a large body.
#[test]
fn large_document_project_wide_revision_preserves_every_match_after_restart() {
    let run = IsolatedRun::new("large-document-global-revision").expect("isolated run");
    let project = run.root().join("large-document-global-revision.parchmint");
    seed_large_document_project(
        &project,
        "Large Document Global Revision",
        &large_body_with_repeated_marker("draftmarker"),
    );

    let harness = DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project))
        .expect("open large-document revision project");
    let _ = harness.take_diagnostics();
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ExplorerSearch)
        .expect("open project-wide search");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::GlobalSearchQuery,
            "draftmarker",
        )
        .expect("search repeated large-document marker");
    harness
        .redraw(HarnessWindow::Project)
        .expect("render global search results");
    let global_search = harness
        .global_search_status()
        .expect("read large-document search status");
    assert!(
        global_search.contains("results=250, documents=1, complete=true"),
        "all repeated markers must be indexed before replacement: {global_search}"
    );
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::GlobalReplacement,
            "finalmarker",
        )
        .expect("enter large-document replacement");
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::GlobalReplacementReview,
        )
        .expect("review project-wide replacement");
    harness
        .click_text(HarnessWindow::Project, "Revalidate selection")
        .expect("revalidate large-document replacement");
    harness
        .click_text(HarnessWindow::Project, "Apply replacement")
        .expect("apply large-document replacement");

    let revised = harness
        .active_editor_body()
        .expect("read revised manuscript");
    assert_eq!(revised.matches("draftmarker").count(), 0);
    assert_eq!(revised.matches("finalmarker").count(), 250);
    harness
        .type_into_target(HarnessWindow::Project, HarnessTarget::EditorPrimary, "7")
        .expect("write a later author revision");
    harness
        .elapse_autosave_idle()
        .expect("persist later author revision");

    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::History),
        )
        .expect("load History after large replacement");
    assert!(
        harness
            .history_checkpoints()
            .expect("read large revision checkpoints")
            .len()
            >= 2,
        "both large revisions must remain available to History"
    );
    assert_no_error_diagnostics(&harness, "project-wide large-document revision");
    close(harness);

    let canonical = canonical_bodies(&project);
    assert_eq!(
        canonical.len(),
        1,
        "canonical project should retain one body"
    );
    assert_eq!(canonical[0].matches("draftmarker").count(), 0);
    assert_eq!(canonical[0].matches("finalmarker").count(), 250);
    assert_eq!(canonical[0].matches('7').count(), 1);

    let reopened = DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project))
        .expect("reopen revised large-document project");
    let body = reopened
        .active_editor_body()
        .expect("read reopened revision");
    assert_eq!(body.matches("finalmarker").count(), 250);
    assert_eq!(body.matches('7').count(), 1);
    assert_no_error_diagnostics(&reopened, "restart after project-wide revision");
    close(reopened);
}

/// Verifies that repeated recovery projections coalesce safely for a
/// large document and that accepting recovery preserves every late
/// author marker after an abandoned session.
#[test]
fn large_document_recovery_replays_repeated_unsaved_authoring_without_loss() {
    let run = IsolatedRun::new("large-document-recovery").expect("isolated run");
    let project = run.root().join("large-document-recovery.parchmint");
    seed_large_document_project(&project, "Large Document Recovery", &large_body("word"));

    let harness = DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project))
        .expect("open large-document recovery project");
    let _ = harness.take_diagnostics();
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::EditorPrimary)
        .expect("focus large manuscript for recovery authoring");
    for capture in 1..=6 {
        harness
            .type_focused(HarnessWindow::Project, capture.to_string())
            .expect("write unsaved large-document revision");
        harness
            .elapse_recovery_capture()
            .expect("coalesce recovery projection");
    }
    assert_no_error_diagnostics(&harness, "recovery projection capture");
    harness
        .abandon()
        .expect("abandon project without a final save");

    let reopened = DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project))
        .expect("reopen abandoned large-document project");
    assert!(
        reopened
            .contains_text(HarnessWindow::Project, "Recovered changes are ready")
            .expect("read recovery affordance"),
        "the unsaved recovery journal must be offered"
    );
    reopened
        .click_text(HarnessWindow::Project, "Recover changes")
        .expect("accept large-document recovery");
    let body = reopened
        .active_editor_body()
        .expect("read recovered manuscript");
    assert_eq!(body.matches("word").count(), LARGE_DOCUMENT_WORDS);
    for capture in 1..=6 {
        assert_eq!(body.matches(&capture.to_string()).count(), 1);
    }
    assert_no_error_diagnostics(&reopened, "accepted large-document recovery");
    close(reopened);
    let canonical = canonical_bodies(&project);
    assert_eq!(canonical.len(), 1);
    for capture in 1..=6 {
        assert_eq!(canonical[0].matches(&capture.to_string()).count(), 1);
    }
}

fn open_in_both_panes(harness: &DesktopInteractionHarness) {
    harness
        .right_click_text(HarnessWindow::Project, LARGE_DOCUMENT_TITLE)
        .expect("open large-manuscript context menu");
    harness
        .click_text(HarnessWindow::Project, "Open")
        .expect("open large manuscript in primary pane");
    let document_id = harness
        .active_editor_document_id(EditorPane::Primary)
        .expect("read primary large-manuscript identity");
    harness
        .right_click_text(HarnessWindow::Project, LARGE_DOCUMENT_TITLE)
        .expect("reopen large-manuscript context menu");
    harness
        .click_text(HarnessWindow::Project, "Open in companion")
        .expect("open large manuscript in companion pane");
    assert_eq!(
        harness
            .active_editor_document_id(EditorPane::Companion)
            .expect("read companion large-manuscript identity"),
        document_id,
        "both panes must bind the same large document"
    );
}

fn seed_large_document_project(path: &Path, title: &str, body: &str) {
    fs::create_dir_all(path.join(".parchmint")).expect("create canonical control directory");
    fs::write(path.join(".parchmint/root-id"), "0000000000000001\n")
        .expect("write canonical project identity");
    let document_id = DocumentId::from_bytes([0xB1; 16]);
    let mut project = Project::new(ProjectId::from_bytes([0x91; 16]));
    project.display_title = title.to_owned();
    let revision = project.revision;
    project = apply_project_command(
        &project,
        revision,
        ProjectCommand::create_document(
            NodeId::from_bytes([0xA1; 16]),
            document_id,
            NodeId::manuscript_root(),
            0,
            LARGE_DOCUMENT_TITLE,
        ),
    )
    .expect("build canonical large manuscript")
    .project;
    let encoding = ProjectFormatCodec::default()
        .encode_domain_project(
            &project,
            &BTreeMap::from([(document_id, body.to_owned())]),
            &BTreeMap::new(),
            &CanonicalProjectPathMap::default(),
        )
        .expect("encode canonical large manuscript");
    for resource in encoding.resources.into_values() {
        let destination = path.join(resource.path.as_str());
        fs::create_dir_all(destination.parent().expect("canonical resource parent"))
            .expect("create canonical resource parent");
        fs::write(destination, resource.bytes).expect("write canonical resource");
    }
}

fn large_body(word: &str) -> String {
    // The initial blank lines give the headless author a stable writing area
    // above the large-document corpus. That keeps real pointer input from
    // splitting a source word merely because the harness targets its center.
    format!(
        "<p>{}{}</p>",
        "\n".repeat(64),
        format!("{word} ").repeat(LARGE_DOCUMENT_WORDS)
    )
}

fn large_body_with_repeated_marker(marker: &str) -> String {
    const MARKER_OCCURRENCES: usize = 250;
    const FILLER_PER_MARKER: usize = (LARGE_DOCUMENT_WORDS / MARKER_OCCURRENCES) - 1;
    let chunk = format!("{}{} ", "word ".repeat(FILLER_PER_MARKER), marker);
    format!(
        "<p>{}{}</p>",
        "\n".repeat(64),
        chunk.repeat(MARKER_OCCURRENCES)
    )
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

fn assert_no_error_diagnostics(harness: &DesktopInteractionHarness, flow: &str) {
    let errors = harness
        .take_diagnostics()
        .into_iter()
        .filter(|event| format!("{:?}", event.level) == "Error")
        .collect::<Vec<_>>();
    assert!(
        errors.is_empty(),
        "{flow} emitted error diagnostics: {errors:?}"
    );
}

fn close(harness: DesktopInteractionHarness) {
    harness
        .close(HarnessWindow::Project)
        .expect("close project");
    harness.shutdown().expect("stop application");
}
