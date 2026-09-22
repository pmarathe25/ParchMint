use std::path::Path;

use parchmint_desktop::{
    DesktopInteractionHarness, EditorPane, HarnessKey, HarnessTarget, HarnessWindow, LaunchRequest,
    RibbonDestination,
};
use parchmint_ui_driver::{IsolatedRun, create_document, create_group, create_project};

#[test]
fn save_and_close_wait_for_recovery_before_projecting_again() {
    for close_while_pending in [false, true] {
        for later_edit in [false, true] {
            let run = IsolatedRun::new("recovery-save-overlap").unwrap();
            let project = run.root().join("novel.parchmint");
            let harness = create_project(&run, &project, "Recovery overlap");
            harness
                .type_into_target(
                    HarnessWindow::Project,
                    HarnessTarget::EditorPrimary,
                    "protectedmarker ",
                )
                .unwrap();
            harness.hold_completions().unwrap();
            harness.elapse_recovery_capture().unwrap();
            if later_edit {
                harness
                    .type_focused(HarnessWindow::Project, "latermarker ")
                    .unwrap();
            }
            if close_while_pending {
                harness.close(HarnessWindow::Project).unwrap();
            } else {
                harness
                    .press_command_key(HarnessWindow::Project, 's')
                    .unwrap();
            }
            harness.release_completions(later_edit).unwrap();
            if !close_while_pending {
                harness.close(HarnessWindow::Project).unwrap();
            }
            harness.shutdown().unwrap();
            let reopened =
                DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project))
                    .unwrap();
            let body = reopened.active_editor_body().unwrap();
            assert!(body.contains("protectedmarker"));
            assert_eq!(body.contains("latermarker"), later_edit);
            reopened.close(HarnessWindow::Project).unwrap();
            reopened.shutdown().unwrap();
        }
    }
}

#[test]
fn scrolling_defers_layout_writes_and_close_preserves_the_latest_position() {
    let run = IsolatedRun::new("deferred-scroll-layout").unwrap();
    let project = run.root().join("scroll.parchmint");
    let harness = create_project(&run, &project, "Scroll layout");
    // The default My Writing workspace has the same reserved initial node IDs.
    // A newly created document identifies this project's workspace uniquely.
    create_document(&harness, "Manuscript", "Scroll document");
    harness
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            // Explicit lines overflow every platform's viewport independently
            // of font metrics and the resulting soft-wrap count.
            "The harbor lantern shines through the rain tonight.\n".repeat(80),
        )
        .unwrap();
    harness
        .press_command_key(HarnessWindow::Project, 's')
        .unwrap();
    harness
        .scroll_target_by(
            HarnessWindow::Project,
            HarnessTarget::EditorPrimary,
            100_000.0,
        )
        .unwrap();
    harness.elapse_notifications().unwrap();
    let document = harness
        .active_editor_document_id(EditorPane::Primary)
        .unwrap();
    let manifest = parchmint_project_format::ProjectFormatCodec::default()
        .decode_manifest(&std::fs::read(project.join("project.toml")).unwrap())
        .unwrap();
    let active_node = manifest.value()["parchmint-structure"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node.get("document-id").and_then(|id| id.as_str()) == Some(&document))
        .unwrap()["id"]
        .as_str()
        .unwrap();
    let workspace_paths = std::fs::read_dir(run.root().join("data/workspaces"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
                && serde_json::from_slice::<serde_json::Value>(&std::fs::read(path).unwrap())
                    .unwrap()["views"]
                    .as_array()
                    .is_some_and(|views| views.iter().any(|view| view["node"] == active_node))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        workspace_paths.len(),
        1,
        "identify this project's workspace"
    );
    let workspace_path = workspace_paths.into_iter().next().unwrap();
    let before = std::fs::read(&workspace_path).unwrap();
    for _ in 0..5 {
        harness
            .scroll_target_by(HarnessWindow::Project, HarnessTarget::EditorPrimary, -20.0)
            .unwrap();
    }
    assert_eq!(
        std::fs::read(&workspace_path).unwrap(),
        before,
        "wheel input must not wait for a layout write"
    );
    harness.elapse_notifications().unwrap();
    let settled: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&workspace_path).unwrap()).unwrap();
    let previous: serde_json::Value = serde_json::from_slice(&before).unwrap();
    assert_ne!(settled["views"], previous["views"]);
    harness
        .scroll_target_by(HarnessWindow::Project, HarnessTarget::EditorPrimary, -20.0)
        .unwrap();
    harness.close(HarnessWindow::Project).unwrap();
    harness.shutdown().unwrap();
    let closed: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&workspace_path).unwrap()).unwrap();
    assert_ne!(
        closed["views"], settled["views"],
        "close must flush the final unexpired scroll position"
    );
    let reopened =
        DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project)).unwrap();
    reopened.close(HarnessWindow::Project).unwrap();
    reopened.shutdown().unwrap();
    let restored: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&workspace_path).unwrap()).unwrap();
    assert_eq!(restored["views"], closed["views"]);
}

#[test]
fn empty_tabs_close_without_creating_project_documents() {
    let run = IsolatedRun::new("empty-draft-tabs").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Empty tabs");
    let before = std::fs::read(project.join("project.toml")).unwrap();
    for _ in 0..3 {
        harness
            .click_target(
                HarnessWindow::Project,
                HarnessTarget::NewTab(EditorPane::Primary),
            )
            .unwrap();
        let draft = harness
            .active_editor_document_id(EditorPane::Primary)
            .unwrap();
        harness
            .close_editor_tab(HarnessWindow::Project, EditorPane::Primary, draft)
            .unwrap();
        assert!(
            !harness
                .contains_text(HarnessWindow::Project, "Save before closing?")
                .unwrap()
        );
    }
    assert_eq!(std::fs::read(project.join("project.toml")).unwrap(), before);
    assert!(
        !harness
            .hierarchy_titles()
            .unwrap()
            .contains(&"Unfiled".to_owned())
    );
    harness.close(HarnessWindow::Project).unwrap();
    harness.shutdown().unwrap();
    assert!(canonical_bodies_in(&project.join("unfiled")).is_empty());
}

#[test]
fn closing_a_changed_draft_can_cancel_save_or_discard() {
    let run = IsolatedRun::new("close-draft-decision").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Draft decisions");
    for save in [true, false] {
        harness
            .click_target(
                HarnessWindow::Project,
                HarnessTarget::NewTab(EditorPane::Primary),
            )
            .unwrap();
        harness
            .type_focused(HarnessWindow::Project, "Keep this idea.")
            .unwrap();
        let draft = harness
            .active_editor_document_id(EditorPane::Primary)
            .unwrap();
        harness
            .close_editor_tab(HarnessWindow::Project, EditorPane::Primary, draft.clone())
            .unwrap();
        assert!(
            harness
                .contains_text(HarnessWindow::Project, "Save before closing?")
                .unwrap()
        );
        harness
            .click_target(HarnessWindow::Project, HarnessTarget::ModalCancel)
            .unwrap();
        assert!(
            harness
                .active_editor_body()
                .unwrap()
                .contains("Keep this idea.")
        );
        harness
            .close_editor_tab(HarnessWindow::Project, EditorPane::Primary, draft)
            .unwrap();
        if save {
            harness
                .click_target(HarnessWindow::Project, HarnessTarget::ModalConfirm)
                .unwrap();
            harness
                .type_focused(HarnessWindow::Project, "Opening")
                .unwrap();
            harness
                .click_target(HarnessWindow::Project, HarnessTarget::ModalConfirm)
                .unwrap();
            assert!(
                harness
                    .hierarchy_titles()
                    .unwrap()
                    .contains(&"Opening".to_owned())
            );
        } else {
            harness
                .click_text(HarnessWindow::Project, "Don’t Save")
                .unwrap();
        }
        assert!(
            !harness
                .contains_text(HarnessWindow::Project, "Save before closing?")
                .unwrap()
        );
    }
    harness.close(HarnessWindow::Project).unwrap();
    harness.shutdown().unwrap();
    assert!(
        canonical_bodies_in(&project.join("manuscript"))
            .iter()
            .any(|body| body.contains("Keep this idea."))
    );
    assert!(canonical_bodies_in(&project.join("unfiled")).is_empty());
}

#[test]
fn a_new_tab_can_be_written_before_choosing_its_name_and_group() {
    let run = IsolatedRun::new("unfiled-draft-save").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Write first");
    create_group(&harness, "Manuscript", "Act One");
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::NewTab(EditorPane::Primary),
        )
        .unwrap();
    assert_eq!(harness.active_editor_tab_title().unwrap(), "Untitled");
    harness
        .type_focused(HarnessWindow::Project, "A story starts here.")
        .unwrap();
    assert!(
        harness
            .active_editor_body()
            .unwrap()
            .contains("A story starts here.")
    );
    assert!(
        harness
            .contains_text(HarnessWindow::Project, "Manuscript · 0 words")
            .unwrap()
    );
    harness
        .press_command_key(HarnessWindow::Project, 's')
        .unwrap();
    assert!(
        harness
            .target_is_focused(HarnessWindow::Project, HarnessTarget::DraftTitle)
            .unwrap()
    );
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ModalCancel)
        .unwrap();
    assert!(
        harness
            .active_editor_body()
            .unwrap()
            .contains("A story starts here.")
    );
    harness
        .type_focused(HarnessWindow::Project, " More.")
        .unwrap();
    assert!(harness.active_editor_body().unwrap().contains("More."));
    harness
        .press_command_key(HarnessWindow::Project, 's')
        .unwrap();
    harness
        .type_focused(HarnessWindow::Project, "The beginning")
        .unwrap();
    harness
        .click_text(HarnessWindow::Project, "Manuscript › Act One")
        .unwrap();
    if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
        harness
            .snapshot(HarnessWindow::Project, Path::new(&root).join("save-draft"))
            .unwrap();
    }
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ModalConfirm)
        .unwrap();
    assert_eq!(harness.active_editor_tab_title().unwrap(), "The beginning");
    assert!(
        harness
            .contains_text(HarnessWindow::Project, "Manuscript · 5 words")
            .unwrap()
    );
    assert!(
        !harness
            .hierarchy_titles()
            .unwrap()
            .contains(&"Unfiled".to_owned())
    );
    harness
        .press_command_key(HarnessWindow::Project, 's')
        .unwrap();
    assert!(
        !harness
            .contains_text(HarnessWindow::Project, "Save document")
            .unwrap()
    );
    harness.close(HarnessWindow::Project).unwrap();
    harness.shutdown().unwrap();
    assert!(
        canonical_bodies_in(&project.join("manuscript"))
            .iter()
            .any(|body| body.contains("A story starts here."))
    );
    let reopened =
        DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project)).unwrap();
    assert_eq!(reopened.active_editor_tab_title().unwrap(), "The beginning");
    assert!(
        reopened
            .active_editor_body()
            .unwrap()
            .contains("A story starts here.")
    );
    reopened.close(HarnessWindow::Project).unwrap();
    reopened.shutdown().unwrap();
}

#[test]
fn changed_drafts_remain_recoverable_without_exposing_an_unfiled_section() {
    let run = IsolatedRun::new("unfiled-draft-reopen").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Unfiled writing");
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::NewTab(EditorPane::Primary),
        )
        .unwrap();
    harness
        .type_focused(HarnessWindow::Project, "An unplaced opening.")
        .unwrap();
    let primary = harness
        .active_editor_document_id(EditorPane::Primary)
        .unwrap();
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ToggleCompanion)
        .unwrap();
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::NewTab(EditorPane::Companion),
        )
        .unwrap();
    harness
        .type_focused(HarnessWindow::Project, "A different ending.")
        .unwrap();
    assert_ne!(
        primary,
        harness
            .active_editor_document_id(EditorPane::Companion)
            .unwrap()
    );
    assert!(
        harness
            .contains_text(HarnessWindow::Project, "Manuscript · 0 words")
            .unwrap()
    );
    // Closing the workspace preserves both drafts without filing either one.
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::EditorCompanion)
        .unwrap();
    harness.close(HarnessWindow::Project).unwrap();
    harness.shutdown().unwrap();
    let bodies = canonical_bodies_in(&project.join("unfiled"));
    assert!(
        bodies
            .iter()
            .any(|body| body.contains("An unplaced opening."))
    );
    assert!(
        bodies
            .iter()
            .any(|body| body.contains("A different ending."))
    );
    let reopened =
        DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project)).unwrap();
    assert!(
        !reopened
            .hierarchy_titles()
            .unwrap()
            .contains(&"Unfiled".to_owned())
    );
    assert!(
        reopened
            .active_editor_body()
            .unwrap()
            .contains("A different ending.")
    );
    reopened
        .press_command_key(HarnessWindow::Project, 's')
        .unwrap();
    assert!(
        reopened
            .contains_text(HarnessWindow::Project, "Save document")
            .unwrap()
    );
    reopened
        .click_target(HarnessWindow::Project, HarnessTarget::ModalCancel)
        .unwrap();
    reopened.close(HarnessWindow::Project).unwrap();
    reopened.shutdown().unwrap();
}

#[test]
fn unfiled_writing_can_be_recovered_before_its_first_explicit_save() {
    let run = IsolatedRun::new("unfiled-draft-recovery").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Recover unfiled writing");
    harness
        .press_command_key(HarnessWindow::Project, 't')
        .unwrap();
    harness
        .type_focused(HarnessWindow::Project, "An idea worth keeping.")
        .unwrap();
    harness.elapse_recovery_capture().unwrap();
    harness.abandon().unwrap();
    let reopened =
        DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project)).unwrap();
    assert!(
        reopened
            .contains_text(HarnessWindow::Project, "Unsaved changes found")
            .unwrap()
    );
    reopened
        .click_text(HarnessWindow::Project, "Recover changes")
        .unwrap();

    assert!(
        reopened
            .active_editor_body()
            .unwrap()
            .contains("An idea worth keeping.")
    );
    assert!(
        reopened
            .contains_text(HarnessWindow::Project, "Manuscript · 0 words")
            .unwrap()
    );
    reopened.close(HarnessWindow::Project).unwrap();
    reopened.shutdown().unwrap();
    assert!(
        canonical_bodies_in(&project.join("unfiled"))
            .iter()
            .any(|body| body.contains("An idea worth keeping."))
    );
}

#[test]
fn creation_and_typing_survive_delayed_recovery_completions_in_either_order() {
    for newest_first in [false, true] {
        let run = IsolatedRun::new("delayed-recovery-creation").unwrap();
        let project = run.root().join("novel.parchmint");
        let harness = create_project(&run, &project, "Delayed recovery");
        harness
            .type_into_target(
                HarnessWindow::Project,
                HarnessTarget::EditorPrimary,
                "Draft before recovery. ",
            )
            .unwrap();
        harness.hold_completions().unwrap();
        harness.elapse_recovery_capture().unwrap();
        harness
            .right_click_text(HarnessWindow::Project, "Manuscript")
            .unwrap();
        harness
            .click_text(HarnessWindow::Project, "New document")
            .unwrap();
        harness
            .type_into_target(
                HarnessWindow::Project,
                HarnessTarget::EditorPrimary,
                "Typed while creation was pending.",
            )
            .unwrap();
        harness.release_completions(newest_first).unwrap();
        harness
            .replace_text_and_submit(HarnessWindow::Project, "Untitled", "Next chapter")
            .unwrap();
        harness
            .type_into_target(
                HarnessWindow::Project,
                HarnessTarget::EditorPrimary,
                "A newly created chapter.",
            )
            .unwrap();
        harness.close(HarnessWindow::Project).unwrap();
        harness.shutdown().unwrap();
        let bodies = canonical_bodies(&project);
        assert!(
            bodies
                .iter()
                .any(|body| body
                    .contains("Draft before recovery. Typed while creation was pending.")),
            "{bodies:?}"
        );
        assert!(
            bodies
                .iter()
                .any(|body| body.contains("A newly created chapter.")),
            "{bodies:?}"
        );
        let reopened =
            DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project)).unwrap();
        assert!(
            reopened
                .hierarchy_titles()
                .unwrap()
                .contains(&"Next chapter".to_owned())
        );
        assert!(
            reopened
                .active_editor_body()
                .unwrap()
                .contains("A newly created chapter.")
        );
        reopened.close(HarnessWindow::Project).unwrap();
        reopened.shutdown().unwrap();
    }
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
        .click_text(HarnessWindow::Project, "New document")
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
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::Bold)
        .unwrap();
    harness
        .type_focused(HarnessWindow::Project, "Bold words")
        .unwrap();
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::Bold)
        .unwrap();
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
            .contains_text(
                HarnessWindow::Project,
                "Bold words plain words unsavedmarker"
            )
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
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::Bold)
        .unwrap();
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::AddComment)
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
    assert!(
        checkpoints.len() >= 2,
        "comment activity must retain prior checkpoints"
    );
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
        .unwrap();
    harness
        .click_text(HarnessWindow::Project, "Open in companion")
        .expect("open the same source beside the primary pane");
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
fn editor_toolbar_menus_format_lists_and_insert_breaks() {
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
    for (target, expected) in [
        (HarnessTarget::ListBulleted, "<ul>"),
        (HarnessTarget::ListNumbered, "<ol>"),
    ] {
        if target == HarnessTarget::ListNumbered {
            harness
                .click_target(HarnessWindow::Project, HarnessTarget::ListMenu)
                .unwrap();
            if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
                harness
                    .snapshot(HarnessWindow::Project, Path::new(&root).join("list-menu"))
                    .unwrap();
            }
        }
        harness
            .click_target(HarnessWindow::Project, target)
            .unwrap();
        let body = harness.active_editor_body().unwrap();
        assert!(body.contains(expected), "{body}");
    }
    harness
        .type_focused(HarnessWindow::Project, " Still writing.")
        .unwrap();
    assert!(
        harness
            .active_editor_body()
            .unwrap()
            .contains("Opening scene. Still writing.")
    );
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::BreakMenu)
        .expect("open break menu");
    harness
        .press_key(HarnessWindow::Project, HarnessKey::ArrowUp)
        .expect("select scene break");
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Enter)
        .expect("insert scene break");
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::BreakMenu)
        .expect("reopen break menu");
    harness
        .press_key(HarnessWindow::Project, HarnessKey::ArrowDown)
        .expect("select page break");
    harness
        .press_key(HarnessWindow::Project, HarnessKey::Enter)
        .expect("insert page break");
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

fn canonical_bodies(project: &Path) -> Vec<String> {
    ["manuscript", "research", "unfiled"]
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

#[test]
fn a_draft_keeps_typing_while_its_creation_completion_is_delayed() {
    for newest_first in [false, true] {
        let run = IsolatedRun::new("buffered-draft").unwrap();
        let project = run.root().join("novel.parchmint");
        let harness = create_project(&run, &project, "Buffered draft");
        harness
            .click_target(
                HarnessWindow::Project,
                HarnessTarget::NewTab(EditorPane::Primary),
            )
            .unwrap();
        harness.hold_completions().unwrap();
        harness
            .type_focused(HarnessWindow::Project, "The first sentence. ")
            .unwrap();
        harness
            .type_focused(HarnessWindow::Project, "And the next one.")
            .unwrap();
        harness.release_completions(newest_first).unwrap();
        assert!(
            harness
                .active_editor_body()
                .unwrap()
                .contains("The first sentence. And the next one.")
        );
        harness.elapse_recovery_capture().unwrap();
        harness.close(HarnessWindow::Project).unwrap();
        harness.shutdown().unwrap();
        assert!(
            canonical_bodies_in(&project.join("unfiled"))
                .iter()
                .any(|body| body.contains("The first sentence. And the next one."))
        );
    }
}

#[test]
fn tabs_move_between_panes_and_explorer_menus_dismiss_in_the_editor() {
    let run = IsolatedRun::new("pane-tab-transfers").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Pane transfers");
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ToggleCompanion)
        .unwrap();
    let initial = harness
        .active_editor_document_id(EditorPane::Primary)
        .unwrap();
    harness
        .close_editor_tab(HarnessWindow::Project, EditorPane::Primary, initial)
        .unwrap();
    create_document(&harness, "Manuscript", "Opening");
    harness
        .type_focused(HarnessWindow::Project, "A moving chapter.")
        .unwrap();
    let opening = harness
        .active_editor_document_id(EditorPane::Primary)
        .unwrap();
    harness
        .right_click_text(HarnessWindow::Project, "Manuscript")
        .unwrap();
    assert!(
        harness
            .contains_text(HarnessWindow::Project, "New group")
            .unwrap()
    );
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::EditorCompanion)
        .unwrap();
    assert!(
        !harness
            .contains_text(HarnessWindow::Project, "New group")
            .unwrap()
    );
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ToggleExplorer)
        .unwrap();
    harness
        .drag_text_to_text(HarnessWindow::Project, "Opening", "Untitled")
        .unwrap();
    assert!(
        !harness
            .editor_tab_is_visible(HarnessWindow::Project, EditorPane::Primary, opening.clone())
            .unwrap()
    );
    assert!(
        harness
            .editor_tab_is_visible(HarnessWindow::Project, EditorPane::Companion, opening)
            .unwrap()
    );
    assert!(
        harness
            .active_editor_body()
            .unwrap()
            .contains("A moving chapter.")
    );
    if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
        harness
            .snapshot(HarnessWindow::Project, Path::new(&root).join("pane-tabs"))
            .unwrap();
        harness
            .resize(HarnessWindow::Project, 1280.0, 720.0)
            .unwrap();
        harness
            .snapshot(
                HarnessWindow::Project,
                Path::new(&root).join("editor-compact"),
            )
            .unwrap();
    }
    harness.close(HarnessWindow::Project).unwrap();
    harness.shutdown().unwrap();
}

#[test]
fn closing_during_draft_creation_preserves_input_and_honors_the_close_request() {
    for close_window in [false, true] {
        let run = IsolatedRun::new("pending-draft-close").unwrap();
        let project = run.root().join("novel.parchmint");
        let harness = create_project(&run, &project, "Pending draft");
        harness
            .click_target(
                HarnessWindow::Project,
                HarnessTarget::NewTab(EditorPane::Primary),
            )
            .unwrap();
        let draft = harness
            .active_editor_document_id(EditorPane::Primary)
            .unwrap();
        harness.hold_completions().unwrap();
        harness
            .type_focused(HarnessWindow::Project, "An idea worth keeping.")
            .unwrap();
        if close_window {
            harness.close(HarnessWindow::Project).unwrap();
        } else {
            harness
                .close_editor_tab(HarnessWindow::Project, EditorPane::Primary, draft)
                .unwrap();
        }
        harness.release_completions(true).unwrap();
        if close_window {
            assert!(!harness.has_window(HarnessWindow::Project).unwrap());
        } else {
            assert!(
                harness
                    .contains_text(HarnessWindow::Project, "Save before closing?")
                    .unwrap()
            );
            harness
                .click_target(HarnessWindow::Project, HarnessTarget::ModalCancel)
                .unwrap();
            assert!(
                harness
                    .active_editor_body()
                    .unwrap()
                    .contains("An idea worth keeping.")
            );
            harness.close(HarnessWindow::Project).unwrap();
        }
        harness.shutdown().unwrap();
        assert!(
            canonical_bodies_in(&project.join("unfiled"))
                .iter()
                .any(|body| body.contains("An idea worth keeping."))
        );
    }
}

#[test]
fn explicitly_saving_a_blank_tab_creates_only_the_chosen_document() {
    let run = IsolatedRun::new("save-blank-tab").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Blank document");
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::NewTab(EditorPane::Primary),
        )
        .unwrap();
    harness
        .press_command_key(HarnessWindow::Project, 's')
        .unwrap();
    harness
        .type_focused(HarnessWindow::Project, "Chapter title")
        .unwrap();
    harness
        .click_target(HarnessWindow::Project, HarnessTarget::ModalConfirm)
        .unwrap();
    assert_eq!(harness.active_editor_tab_title().unwrap(), "Chapter title");
    harness
        .type_focused(HarnessWindow::Project, "The first words.")
        .unwrap();
    harness.close(HarnessWindow::Project).unwrap();
    harness.shutdown().unwrap();
    assert!(canonical_bodies_in(&project.join("unfiled")).is_empty());
    assert!(
        canonical_bodies_in(&project.join("manuscript"))
            .iter()
            .any(|body| body.contains("The first words."))
    );
}
