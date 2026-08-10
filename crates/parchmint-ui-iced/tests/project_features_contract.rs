//! Requirements-first contracts for project-facing Iced views.
//!
//! The Stage 37 implementation supplies the presentation types exercised here.
//! These tests deliberately stay at the UI boundary: they use deterministic
//! fixture state and presentation effects, never a display server or a service
//! implementation.

use parchmint_preferences::{AppearanceMode, ResolvedAppearance};
use parchmint_ui_iced::{
    ContentState, DragDestination, DragValidity, ExportState, HistoryRestoreScope, ProjectEffect,
    ProjectFixture, ProjectMessage, ProjectModal, ProjectWorkspace, ReplacementCheckState,
    RestoreLocation, SaveState, SelectionGesture, SidebarSurface,
};

#[test]
fn project_fixtures_have_requirement_linked_light_dark_references_for_every_view() {
    let references = [
        (
            ProjectFixture::Explorer,
            "editor-single-light",
            "editor-single-dark",
        ),
        (ProjectFixture::Cards, "cards-light", "cards-dark"),
        (
            ProjectFixture::GlobalSearch,
            "global-search-light",
            "global-search-dark",
        ),
        (ProjectFixture::History, "history-light", "history-dark"),
        (
            ProjectFixture::RecentlyDeleted,
            "recently-deleted-light",
            "recently-deleted-dark",
        ),
        (
            ProjectFixture::SettingsAppearance,
            "settings-appearance-light",
            "settings-appearance-dark",
        ),
        (
            ProjectFixture::Export,
            "export-project-output-controls-light",
            "export-project-output-controls-dark",
        ),
        (
            ProjectFixture::ErrorRecovery,
            "error-recovery-light",
            "error-recovery-dark",
        ),
    ];

    for (fixture, light, dark) in references {
        let workspace = ProjectWorkspace::from_fixture(fixture);
        assert_eq!(
            workspace.fixture_reference(ResolvedAppearance::Light),
            light,
            "each project view has a deterministic Light fixture"
        );
        assert_eq!(
            workspace.fixture_reference(ResolvedAppearance::Dark),
            dark,
            "each project view has a deterministic Dark fixture"
        );
    }
}

#[test]
fn explorer_selection_normalizes_descendants_and_dragging_preserves_hierarchy_rules() {
    // TREE-004, TREE-005, TREE-006, TREE-007, TREE-008, TREE-009, TREE-017, TREE-018.
    let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);

    workspace.update(ProjectMessage::SelectHierarchy {
        node_id: "chapter-one".into(),
        gesture: SelectionGesture::Replace,
    });
    workspace.update(ProjectMessage::SelectHierarchy {
        node_id: "chapter-three".into(),
        gesture: SelectionGesture::ContiguousRange,
    });
    assert_eq!(
        workspace.explorer().selected_ids(),
        ["chapter-one", "chapter-two", "chapter-three"],
        "Shift selection follows deterministic sibling order"
    );

    workspace.update(ProjectMessage::SelectHierarchy {
        node_id: "research-notes".into(),
        gesture: SelectionGesture::Additive,
    });
    assert_eq!(
        workspace.explorer().selected_ids(),
        [
            "chapter-one",
            "chapter-two",
            "chapter-three",
            "research-notes"
        ],
        "the platform additive gesture keeps a noncontiguous selection"
    );

    workspace.update(ProjectMessage::SelectHierarchy {
        node_id: "part-one".into(),
        gesture: SelectionGesture::Additive,
    });
    assert_eq!(
        workspace.explorer().selected_ids(),
        ["part-one", "chapter-three", "research-notes"],
        "a selected ancestor subsumes its selected descendants for batch operations"
    );

    assert_eq!(
        workspace
            .explorer()
            .drag_validity("part-one", DragDestination::IntoGroup("chapter-two".into()),),
        DragValidity::RejectedCycle,
        "a group cannot be moved into its own subtree"
    );
    assert_eq!(
        workspace.explorer().drag_validity(
            "chapter-one",
            DragDestination::BeforeSibling("chapter-two".into()),
        ),
        DragValidity::Allowed,
        "a document can deterministically reorder among siblings"
    );
    assert_eq!(
        workspace
            .explorer()
            .drag_validity("chapter-one", DragDestination::IntoGroup("research".into()),),
        DragValidity::Allowed,
        "a node can move between Manuscript and Research through a group destination"
    );
}

#[test]
fn cards_project_the_same_hierarchy_selection_and_document_activation_without_implicit_status() {
    // CARD-001 through CARD-010 and TREE-009.
    let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Cards);

    assert_eq!(workspace.cards().section_id(), "manuscript");
    assert!(workspace.cards().shows_hierarchy());
    assert!(workspace.cards().drag_destination().is_some());
    assert!(workspace.cards().title_is_editable("chapter-one"));
    assert!(workspace.cards().synopsis_is_editable("chapter-one"));
    assert!(workspace.cards().metadata_is_read_only());
    assert!(
        !workspace
            .cards()
            .visible_metadata_labels()
            .contains(&"Status"),
        "Cards must not invent a Status: Draft field when the project has none"
    );

    workspace.update(ProjectMessage::SelectHierarchy {
        node_id: "chapter-one".into(),
        gesture: SelectionGesture::Replace,
    });
    assert_eq!(
        workspace.cards().selected_ids(),
        ["chapter-one"],
        "Cards exposes the shared hierarchy selection"
    );
    assert_eq!(
        workspace.cards().selected_ids(),
        workspace.explorer().selected_ids()
    );

    let effects = workspace.update(ProjectMessage::ActivateCard("chapter-one".into()));
    assert_eq!(
        effects,
        [ProjectEffect::OpenDocumentInPrimary("chapter-one".into())]
    );
    assert!(workspace.cards().last_activated_document().is_some());
}

#[test]
fn inspector_synopsis_and_metadata_follow_settings_order_without_destroying_hidden_values() {
    // META-001 through META-009 and Inspector guidance.
    let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);

    assert!(workspace.inspector().synopsis_is_multiline_plain_text());
    assert!(workspace.inspector().metadata_is_ordered_by_settings());
    assert_eq!(
        workspace
            .inspector()
            .metadata_value("chapter-one", "field-17"),
        Some("first person")
    );

    workspace.update(ProjectMessage::SetMetadataApplicability {
        field_id: "field-17".into(),
        applies_to_documents: false,
    });
    assert!(
        !workspace
            .inspector()
            .metadata_field_is_visible("chapter-one", "field-17")
    );
    assert_eq!(
        workspace
            .inspector()
            .stored_metadata_value("chapter-one", "field-17"),
        Some("first person"),
        "changing applicability hides existing values instead of deleting them"
    );

    workspace.update(ProjectMessage::RenameMetadataField {
        field_id: "field-17".into(),
        label: "Narration".into(),
    });
    assert_eq!(
        workspace
            .inspector()
            .stored_metadata_value("chapter-one", "field-17"),
        Some("first person"),
        "a stable field ID preserves values through a label change"
    );
}

#[test]
fn global_search_replaces_explorer_and_replacement_preview_propagates_indeterminate_selection() {
    // SEARCH-006 through SEARCH-014.
    let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::GlobalSearch);

    assert_eq!(workspace.sidebar_surface(), SidebarSurface::GlobalSearch);
    assert!(workspace.global_search().has_explicit_return_to_explorer());
    assert!(workspace.global_search().searches_entire_project());
    assert!(!workspace.global_search().has_scope_selector());
    assert!(workspace.global_search().results_stream());
    assert!(workspace.global_search().results_are_virtualized());
    assert!(workspace.global_search().results_are_grouped_by_document());

    workspace.update(ProjectMessage::OpenReplacementPreview);
    assert!(workspace.replacement_preview().uses_middle_pane());
    assert_eq!(
        workspace.replacement_preview().check_state("manuscript"),
        ReplacementCheckState::Indeterminate,
        "a parent with selected and excluded descendants is visibly indeterminate"
    );

    workspace.update(ProjectMessage::SetReplacementIncluded {
        node_id: "chapter-one-match-1".into(),
        included: false,
    });
    assert_eq!(
        workspace.replacement_preview().check_state("chapter-one"),
        ReplacementCheckState::Indeterminate,
        "document and group controls reflect match-level exclusions"
    );
    assert!(
        workspace
            .replacement_preview()
            .requires_revision_revalidation()
    );
}

#[test]
fn history_restore_requires_a_whole_project_confirmation_before_emitting_work() {
    // HIST-007 and HIST-008.
    let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::History);

    let effects = workspace.update(ProjectMessage::RequestHistoryRestore {
        checkpoint_id: "snapshot-draft-two".into(),
    });
    assert!(effects.is_empty());
    assert_eq!(
        workspace.modal(),
        Some(ProjectModal::HistoryRestore {
            checkpoint_id: "snapshot-draft-two".into(),
            scope: HistoryRestoreScope::EntireProject,
        })
    );

    let effects = workspace.update(ProjectMessage::ConfirmHistoryRestore);
    assert_eq!(
        effects,
        [ProjectEffect::RestoreHistory {
            checkpoint_id: "snapshot-draft-two".into(),
            scope: HistoryRestoreScope::EntireProject,
        }]
    );
}

#[test]
fn recently_deleted_restores_complete_subtrees_at_the_old_location_or_section_root() {
    // DEL-003 through DEL-007.
    let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::RecentlyDeleted);

    assert!(workspace.recently_deleted().has_formatted_preview());
    assert_eq!(
        workspace
            .recently_deleted()
            .restore_location("deleted-part"),
        RestoreLocation::FormerParent("part-one".into())
    );

    let effects = workspace.update(ProjectMessage::RestoreDeleted("deleted-part".into()));
    assert_eq!(
        effects,
        [ProjectEffect::RestoreDeletedSubtree {
            node_id: "deleted-part".into(),
            location: RestoreLocation::FormerParent("part-one".into()),
        }]
    );

    workspace.update(ProjectMessage::UseRestoreFallback("deleted-part".into()));
    assert_eq!(
        workspace
            .recently_deleted()
            .restore_location("deleted-part"),
        RestoreLocation::SectionRoot("manuscript".into())
    );
    assert!(!workspace.recently_deleted().has_purge_action());
}

#[test]
fn settings_exposes_only_supported_appearance_choices_and_emits_application_scoped_propagation() {
    // APPR-001 through APPR-009.
    let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::SettingsAppearance);

    assert_eq!(
        workspace.settings().appearance_choices(),
        [
            AppearanceMode::System,
            AppearanceMode::Light,
            AppearanceMode::Dark
        ]
    );
    assert_eq!(workspace.settings().appearance(), AppearanceMode::System);

    let effects = workspace.update(ProjectMessage::SetAppearance(AppearanceMode::Dark));
    assert_eq!(workspace.settings().appearance(), AppearanceMode::Dark);
    assert_eq!(
        effects,
        [ProjectEffect::ApplyAppearanceToAllWindows(
            AppearanceMode::Dark
        )]
    );
    assert!(
        workspace
            .settings()
            .appearance_is_outside_project_undo_save_and_history()
    );
}

#[test]
fn export_and_save_error_states_remain_actionable_without_false_success() {
    // EXP-001, EXP-002, EXP-008, EXP-009 and SAVE-005 through SAVE-014.
    let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Export);

    assert!(workspace.export().is_entire_manuscript_only());
    assert!(!workspace.export().has_partial_inclusion_controls());
    workspace.update(ProjectMessage::ExportFailed(
        "destination is not writable".into(),
    ));
    assert_eq!(
        workspace.export().state(),
        ExportState::Failed("destination is not writable".into())
    );
    assert!(!workspace.export().can_open_result());
    assert!(!workspace.export().can_reveal_result());

    workspace.update(ProjectMessage::SaveFailed("disk full".into()));
    assert_eq!(
        workspace.save().state(),
        SaveState::Error("disk full".into())
    );
    assert!(workspace.save().editing_remains_available());
    assert!(workspace.save().recovery_remains_intact());
    assert!(!workspace.save().claims_saved());

    workspace.update(ProjectMessage::RequestClose);
    assert!(workspace.save().close_is_waiting_for_retry_or_cancel());
}

#[test]
fn empty_loading_error_and_recovery_states_keep_shell_context_and_recovery_returns_focus_after_acceptance()
 {
    // Empty/loading/error/recovery design guidance and SAVE-011 through SAVE-014.
    let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::ErrorRecovery);

    for state in [
        ContentState::Empty,
        ContentState::Loading,
        ContentState::Error("history unavailable".into()),
        ContentState::Recovery,
    ] {
        workspace.update(ProjectMessage::SetContentState(state));
        assert!(workspace.shell_context_is_retained());
        assert!(workspace.content_state().uses_full_available_content_area());
    }

    let effects = workspace.update(ProjectMessage::AcceptRecovery);
    assert_eq!(effects, [ProjectEffect::FocusRecoveredEditor]);
    assert!(
        !workspace.recovery().is_disposable_after_durable_save(),
        "accepting recovery retains it until a durable save completes"
    );
    workspace.update(ProjectMessage::RecoveryDurablySaved);
    assert!(workspace.recovery().is_disposable_after_durable_save());
}
