use std::path::Path;

use parchmint_desktop::{
    DesktopInteractionHarness, EditorPane, HarnessTarget, HarnessWindow, LaunchRequest,
    RibbonDestination,
};
use parchmint_ui_driver::IsolatedRun;

#[test]
fn visible_explorer_buttons_create_and_open_a_named_chapter() {
    let run = IsolatedRun::new("visible-creation-actions").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Visible creation");
    harness
        .click_text(HarnessWindow::Project, "New group")
        .unwrap();
    harness
        .replace_text_and_submit(HarnessWindow::Project, "New Group", "Part One")
        .unwrap();
    harness
        .click_text(HarnessWindow::Project, "New document")
        .unwrap();
    harness
        .replace_text_and_submit(HarnessWindow::Project, "Untitled", "Chapter One")
        .unwrap();
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "The chapter begins.",
        )
        .unwrap();
    assert!(
        harness
            .active_editor_body()
            .unwrap()
            .contains("The chapter begins.")
    );
    harness.close(HarnessWindow::Project).unwrap();
    harness.shutdown().unwrap();
    assert!(
        canonical_bodies(&project)
            .iter()
            .any(|body| body.contains("The chapter begins."))
    );
}

#[test]
fn finishing_research_creation_by_clicking_the_editor_never_duplicates_input() {
    let run = IsolatedRun::new("research-pointer-routing").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Independent panes");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "Manuscript must stay unchanged.",
        )
        .unwrap();
    harness
        .right_click_text(HarnessWindow::Project, "Research")
        .unwrap();
    harness
        .click_text(HarnessWindow::Project, "Create document")
        .unwrap();
    // The Research tab is opened by committing the name on blur. This click
    // initially lands in the still-full-width manuscript editor.
    harness
        .type_at(HarnessWindow::Project, (760.0, 300.0), "One owner.")
        .unwrap();
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorCompanion,
            "Research only.",
        )
        .unwrap();
    harness.elapse_autosave_idle().unwrap();
    harness.close(HarnessWindow::Project).unwrap();
    harness.shutdown().unwrap();
    let bodies = canonical_bodies(&project);
    assert!(
        bodies
            .iter()
            .any(|body| body.contains("<p>Manuscript must stay unchanged.One owner.</p>")),
        "{bodies:?}"
    );
    assert_eq!(
        bodies
            .iter()
            .filter(|body| body.contains("One owner."))
            .count(),
        1,
        "{bodies:?}"
    );
    assert_eq!(
        bodies
            .iter()
            .filter(|body| body.contains("Research only."))
            .count(),
        1,
        "{bodies:?}"
    );
}

#[test]
fn toolbar_typing_marks_and_history_compare_the_live_unsaved_draft() {
    let run = IsolatedRun::new("typing-format-history").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Typing and History");
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::EditorPrimary)
        .unwrap();
    harness.click_text(HarnessWindow::Project, "B").unwrap();
    harness
        .type_focused(HarnessWindow::Project, "Bold words")
        .unwrap();
    harness.click_text(HarnessWindow::Project, "B").unwrap();
    harness
        .type_focused(HarnessWindow::Project, " plain words")
        .unwrap();
    harness.elapse_autosave_idle().unwrap();
    let body = harness.active_editor_body().unwrap();
    assert!(
        body.contains("<strong>Bold words</strong> plain words"),
        "{body}"
    );
    harness
        .type_focused(HarnessWindow::Project, " unsavedmarker")
        .unwrap();
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::History),
        )
        .unwrap();
    harness
        .click_history_checkpoint(HarnessWindow::Project, 0)
        .unwrap();
    assert!(
        harness
            .contains_text(HarnessWindow::Project, " unsavedmarker")
            .unwrap(),
        "History's Current side must include the live draft"
    );
    harness.close(HarnessWindow::Project).unwrap();
    harness.shutdown().unwrap();
    assert!(
        canonical_bodies(&project)
            .iter()
            .any(|body| body.contains("unsavedmarker"))
    );
}

#[test]
fn manuscript_and_research_keep_independent_edits_comments_and_saved_history() {
    let run = IsolatedRun::new("manuscript-research-integrity").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Research beside the novel");
    let manuscript = harness
        .active_editor_document_id(EditorPane::Primary)
        .unwrap();
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "Mara returns to the harbor.",
        )
        .unwrap();
    create_group(&harness, "Research", "Characters");
    create_document(&harness, "Characters", "Mara's background");
    harness
        .right_click_text(HarnessWindow::Project, "Mara's background")
        .unwrap();
    harness
        .click_text(HarnessWindow::Project, "Open in companion")
        .unwrap();
    let research = harness
        .active_editor_document_id(EditorPane::Companion)
        .unwrap();
    assert_ne!(manuscript, research);
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorCompanion,
            "Mara grew up inland and fears deep water.",
        )
        .unwrap();
    assert!(
        harness
            .contains_text(HarnessWindow::Project, "Document · 8 words")
            .unwrap()
    );
    assert!(
        harness
            .contains_text(HarnessWindow::Project, "Manuscript · 5 words")
            .unwrap()
    );
    harness
        .select_editor_text(HarnessWindow::Project, EditorPane::Primary, "harbor")
        .unwrap();
    harness
        .scroll_target_by(HarnessWindow::Project, HarnessTarget::EditorCompanion, 80.0)
        .unwrap();
    harness.click_text(HarnessWindow::Project, "B").unwrap();
    harness
        .click_text(HarnessWindow::Project, "Comment")
        .unwrap();
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::CommentDraft,
            "Check this against Mara's background.",
        )
        .unwrap();
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::EditorCompanion)
        .unwrap();
    harness
        .click_text(HarnessWindow::Project, "Add comment")
        .unwrap();
    harness.elapse_autosave_idle().unwrap();

    let bodies = canonical_bodies(&project);
    assert!(
        bodies
            .iter()
            .any(|body| body.contains("<strong>harbor</strong>"))
    );
    let research_body = bodies
        .iter()
        .find(|body| body.contains("grew up inland"))
        .unwrap();
    assert!(!research_body.contains("<strong>"));
    let annotations = std::fs::read_to_string(
        project
            .join("annotations")
            .join(format!("{manuscript}.json")),
    )
    .unwrap();
    assert!(annotations.contains("Check this against Mara's background."));
    assert!(annotations.contains("harbor"));
    let research_annotations = project.join("annotations").join(format!("{research}.json"));
    assert!(
        !std::fs::read_to_string(research_annotations)
            .unwrap_or_default()
            .contains("Check this")
    );

    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::History),
        )
        .unwrap();
    let checkpoints = harness.history_checkpoints().unwrap();
    assert!(!checkpoints.is_empty());
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Editor),
        )
        .unwrap();
    harness
        .press_command_key(HarnessWindow::Project, 's')
        .unwrap();
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::History),
        )
        .unwrap();
    assert_eq!(
        harness.history_checkpoints().unwrap().len(),
        checkpoints.len(),
        "saving unchanged writing must not add an empty checkpoint"
    );
    harness.close(HarnessWindow::Project).unwrap();
    harness.shutdown().unwrap();

    let reopened =
        DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project)).unwrap();
    assert_eq!(canonical_bodies(&project), bodies);
    assert_eq!(
        std::fs::read_to_string(
            project
                .join("annotations")
                .join(format!("{manuscript}.json"))
        )
        .unwrap(),
        annotations
    );
    assert_eq!(
        reopened
            .active_editor_document_id(EditorPane::Primary)
            .unwrap(),
        manuscript
    );
    assert_eq!(
        reopened
            .active_editor_document_id(EditorPane::Companion)
            .unwrap(),
        research
    );
    reopened.close(HarnessWindow::Project).unwrap();
    reopened.shutdown().unwrap();
}

#[test]
fn editor_can_research_and_revise_the_same_document_from_both_panes() {
    let run = IsolatedRun::new("same-document-research").expect("isolated run");
    let project = run.root().join("same-document-research.parchmint");
    let harness = create_project(&run, &project, "Same Document Research");

    create_group(&harness, "Manuscript", "Field Notes");
    create_document(&harness, "Field Notes", "Tide Journal");
    harness
        .right_click_text(HarnessWindow::Project, "Tide Journal")
        .expect("open research document menu");
    harness
        .click_text(HarnessWindow::Project, "Open")
        .expect("open research document in primary pane");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "The tide turns at dusk.",
        )
        .expect("write source note in primary pane");
    let document_id = harness
        .active_editor_document_id(EditorPane::Primary)
        .expect("read primary document identity");

    harness
        .right_click_text(HarnessWindow::Project, "Tide Journal")
        .expect("reopen research document menu");
    harness
        .click_text(HarnessWindow::Project, "Open in companion")
        .expect("open the same source in companion pane");
    assert_eq!(
        document_id,
        harness
            .active_editor_document_id(EditorPane::Companion)
            .expect("read companion document identity")
    );
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorCompanion,
            " The lantern stays lit.",
        )
        .expect("append research detail through the companion pane");
    assert!(
        harness
            .active_editor_body()
            .expect("read companion body")
            .contains("lantern")
    );

    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            " Verified by the log.",
        )
        .expect("revise from the primary pane after a companion edit");
    assert!(
        harness
            .active_editor_body()
            .expect("read primary body after focus transfer")
            .contains("Verified by the log")
    );
    harness
        .close(HarnessWindow::Project)
        .expect("close project");
    harness.shutdown().expect("stop application");
}

#[test]
fn editor_toolbar_inserts_semantic_scene_and_page_breaks() {
    let run = IsolatedRun::new("semantic-breaks").expect("isolated run");
    let project = run.root().join("semantic-breaks.parchmint");
    let harness = create_project(&run, &project, "Semantic Breaks");

    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "Opening scene.",
        )
        .expect("draft opening prose");
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::SceneBreak)
        .expect("insert a scene break");
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::PageBreak)
        .expect("insert a page break");
    let body = harness
        .active_editor_body()
        .expect("read document with semantic breaks");
    assert!(body.contains("data-kind=\"scene-break\""));
    assert!(body.contains("data-kind=\"page-break\""));
    harness
        .close(HarnessWindow::Project)
        .expect("close semantic-break project");
    harness.shutdown().expect("stop application");
}

#[test]
fn editor_selection_formatting_can_be_undone_and_redone_with_keyboard_focus() {
    let run = IsolatedRun::new("format-history-flow").expect("isolated run");
    let project = run.root().join("format-history-flow.parchmint");
    let harness = create_project(&run, &project, "Format History Flow");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            "A decisive opening.",
        )
        .expect("draft opening");
    harness
        .select_editor_text(HarnessWindow::Project, EditorPane::Primary, "decisive")
        .expect("select adjective");
    harness
        .press_command_key(HarnessWindow::Project, 'b')
        .expect("bold selected adjective");
    harness
        .press_command_key(HarnessWindow::Project, 'z')
        .expect("undo formatting");
    assert!(
        !harness
            .active_editor_body()
            .expect("read undone prose")
            .contains("<strong>")
    );
    #[cfg(target_os = "macos")]
    harness
        .press_command_shift_key(HarnessWindow::Project, 'z')
        .expect("redo formatting on macOS");
    #[cfg(not(target_os = "macos"))]
    harness
        .press_command_key(HarnessWindow::Project, 'y')
        .expect("redo formatting");
    harness
        .elapse_autosave_idle()
        .expect("save formatted revision");
    assert!(
        canonical_bodies(&project)
            .iter()
            .any(|body| body.contains("<strong>decisive</strong>")),
        "redo should restore canonical formatting"
    );
    harness
        .close(HarnessWindow::Project)
        .expect("close project");
    harness.shutdown().expect("stop application");
}

fn create_project(run: &IsolatedRun, project: &Path, title: &str) -> DesktopInteractionHarness {
    let harness = DesktopInteractionHarness::launch(run.root(), LaunchRequest::launcher())
        .expect("launch application");
    harness
        .click_text(HarnessWindow::Launcher, "Create Project")
        .expect("open project form");
    harness
        .type_into(HarnessWindow::Launcher, "Project title", title)
        .expect("enter title");
    harness
        .type_into(
            HarnessWindow::Launcher,
            "Project destination",
            project.to_string_lossy(),
        )
        .expect("enter destination");
    harness
        .click_text(HarnessWindow::Launcher, "Create and Open")
        .expect("create project");
    harness
}

fn create_group(harness: &DesktopInteractionHarness, parent: &str, title: &str) {
    harness
        .right_click_text(HarnessWindow::Project, parent)
        .expect("open parent menu");
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
        .expect("open parent menu");
    harness
        .click_text(HarnessWindow::Project, "Create document")
        .expect("create document");
    harness
        .replace_text_and_submit(HarnessWindow::Project, "Untitled", title)
        .expect("name document");
}

fn canonical_bodies(project: &Path) -> Vec<String> {
    ["manuscript", "research"]
        .into_iter()
        .flat_map(|directory| canonical_bodies_in(&project.join(directory)))
        .collect()
}

fn canonical_bodies_in(directory: &Path) -> Vec<String> {
    std::fs::read_dir(directory)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .flat_map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                canonical_bodies_in(&path)
            } else if path.extension().is_some_and(|value| value == "html") {
                vec![std::fs::read_to_string(path).expect("read canonical document")]
            } else {
                Vec::new()
            }
        })
        .collect()
}
