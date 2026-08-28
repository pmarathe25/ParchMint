//! Requirements-first contracts for project-facing Iced views.
//!
//! The Stage 37 implementation supplies the presentation types exercised here.
//! These tests deliberately stay at the UI boundary: they use deterministic
//! fixture state and presentation effects, never a display server or a service
//! implementation.

use parchmint_application::{DocumentSnapshot, DocumentVisibility, EditorRevision};
use parchmint_domain::{
    DocumentId, MetadataApplicability, MetadataFieldDefinition, MetadataFieldId, MetadataTextKind,
    NodeId, Project, ProjectCommand, ProjectExportSetting, ProjectExportSettings, ProjectId,
    apply_project_command,
};
use parchmint_preferences::{AppearanceMode, ResolvedAppearance};
use parchmint_ui_api::{
    DocumentSummary, DocumentWordCount, ExportArtifact, ExportArtifactToken, ProjectSnapshot,
};
use parchmint_ui_iced::{
    ContentState, DragDestination, DragValidity, EditorMessage, EditorPane, ExportState,
    GlobalSearchResult, HierarchyRowKind, HistoryRestoreScope, ProjectEffect, ProjectFixture,
    ProjectMessage, ProjectModal, ProjectTask, ProjectTaskCompletion, ProjectTaskPayload,
    ProjectWorkspace, ReplacementCheckState, ReplacementPreviewRowKind, RestoreLocation,
    RibbonDestination, SaveState, SelectionGesture, ShellLayout, SidebarSurface, StatusCount,
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
fn inspector_follows_the_most_recent_explorer_or_editor_focus() {
    let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
    workspace.update(ProjectMessage::SelectHierarchy {
        node_id: "research-notes".into(),
        gesture: SelectionGesture::Replace,
    });

    assert_eq!(workspace.inspector_node_id(), Some("research-notes"));

    workspace
        .editor_mut()
        .update(EditorMessage::FocusPane(EditorPane::Primary));
    assert_eq!(workspace.inspector_node_id(), Some("chapter-one"));
}

#[test]
fn cards_disclosure_hides_descendants_without_narrowing_the_projection() {
    let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Cards);
    assert!(
        workspace
            .cards()
            .items()
            .iter()
            .any(|item| item.node_id == "chapter-one" && item.visible)
    );

    workspace.update(ProjectMessage::ToggleHierarchyExpanded("part-one".into()));
    let cards = workspace.cards();
    assert!(cards.items().iter().any(|item| item.node_id == "part-one"));
    assert!(
        cards
            .items()
            .iter()
            .any(|item| item.node_id == "chapter-one" && !item.visible)
    );
}

#[test]
fn cards_project_the_same_hierarchy_selection_and_document_activation_without_implicit_status() {
    // CARD-001 through CARD-010 and TREE-009.
    let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Cards);

    assert_eq!(workspace.cards().section_id(), "manuscript");
    assert!(workspace.cards().shows_hierarchy());
    assert!(workspace.cards().drag_destination().is_some());
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

    workspace.update(ProjectMessage::UpdateMetadataField {
        field_id: "field-17".into(),
        label: "Narration".into(),
        description: Some("Narrative perspective".into()),
        applicability: parchmint_ui_iced::MetadataFieldApplicability::Groups,
        text_kind: parchmint_ui_iced::MetadataFieldTextKind::SingleLine,
        default_value: None,
        visible_on_cards: true,
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
fn settings_metadata_and_style_edits_keep_stable_ids_and_emit_only_valid_effects() {
    let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::SettingsAppearance);

    let existing = workspace.settings().metadata_fields()[1].id.to_owned();
    assert!(
        workspace
            .update(ProjectMessage::SetMetadataApplicability {
                field_id: existing.clone(),
                applies_to_documents: false,
            })
            .is_empty(),
        "Documents cannot transition to a zero-target field"
    );
    assert_eq!(
        workspace
            .settings()
            .metadata_field(&existing)
            .unwrap()
            .applicability,
        parchmint_ui_iced::MetadataFieldApplicability::Documents
    );

    let created = workspace.update(ProjectMessage::CreateMetadataField);
    let field = workspace
        .settings()
        .metadata_fields()
        .last()
        .unwrap()
        .id
        .to_owned();
    assert!(
        matches!(created.as_slice(), [ProjectEffect::UpsertMetadataField(definition)] if definition.label == "New field")
    );
    workspace.update(ProjectMessage::ReorderMetadataField {
        field_id: field.clone(),
        target_index: 0,
    });
    assert_eq!(workspace.settings().metadata_fields()[0].id, field);
    workspace.update(ProjectMessage::RequestDeleteMetadataField(field.clone()));
    assert!(matches!(
        workspace.modal(),
        Some(ProjectModal::DeleteMetadataField { .. })
    ));
    assert_eq!(
        workspace.update(ProjectMessage::ConfirmDeleteMetadataField),
        [ProjectEffect::DeleteMetadataField(field)]
    );

    assert_eq!(
        workspace.settings().styles().len(),
        7,
        "the seven reserved styles are listed"
    );
    let reserved = workspace.settings().styles()[0].id.to_owned();
    assert!(
        workspace
            .update(ProjectMessage::RequestDeleteStyle(reserved))
            .is_empty()
    );
    let effects = workspace.update(ProjectMessage::CreateStyle);
    let custom = workspace.settings().styles().last().unwrap().id.to_owned();
    assert!(
        matches!(effects.as_slice(), [ProjectEffect::UpsertStyle(definition)] if !definition.role.is_reserved())
    );
    let properties = parchmint_domain::StyleProperties {
        font_family: Some("Literata".into()),
        font_size_points: Some(12.0),
        weight: Some(500),
        italic: Some(true),
        alignment: Some(parchmint_domain::TextAlignment::Justify),
        first_line_indent_points: Some(18.0),
        left_indent_points: Some(2.0),
        right_indent_points: Some(3.0),
        line_spacing: Some(1.3),
        space_before_points: Some(4.0),
        space_after_points: Some(5.0),
        keep_with_next: Some(true),
        page_break_before: Some(false),
    };
    assert!(
        matches!(workspace.update(ProjectMessage::SetStyleProperties { style_id: custom.clone(), properties: properties.clone() }).as_slice(), [ProjectEffect::UpsertStyle(definition)] if definition.properties == properties)
    );
    workspace.update(ProjectMessage::RequestDeleteStyle(custom.clone()));
    assert!(matches!(
        workspace.modal(),
        Some(ProjectModal::DeleteStyle { .. })
    ));
    assert_eq!(
        workspace.update(ProjectMessage::ConfirmDeleteStyle),
        [ProjectEffect::DeleteStyle(custom)]
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
fn production_replacement_preview_uses_streamed_matches_and_requires_revalidation() {
    let fixture = production_snapshot();
    let mut workspace = ProjectWorkspace::from_snapshot(&fixture.snapshot);
    let manuscript = id_string(fixture.manuscript_document.as_bytes());
    let research = id_string(fixture.research_document.as_bytes());
    let results = vec![
        streamed_result(
            &manuscript,
            "match-opening",
            "before ",
            "river",
            " after",
            4,
        ),
        streamed_result(&research, "match-research", "near ", "river", " bank", 5),
    ];
    let search = workspace.begin_task(ProjectTask::GlobalSearch { generation: 0 });
    assert!(
        workspace.accept_completion(ProjectTaskCompletion::for_ticket(
            search,
            ProjectTaskPayload::SearchBatch {
                results,
                finished: true,
            },
        ))
    );

    let effects = workspace.update(ProjectMessage::OpenReplacementPreview);
    assert!(matches!(
        effects.as_slice(),
        [ProjectEffect::BuildReplacementPreview { .. }]
    ));
    let rows = workspace.replacement_preview().rows();
    assert_eq!(
        rows.len(),
        5,
        "root, two documents, and two streamed matches"
    );
    assert_eq!(rows[0].kind, ReplacementPreviewRowKind::AllMatches);
    assert_eq!(rows[2].kind, ReplacementPreviewRowKind::Match);
    assert_eq!(rows[2].prefix, Some("before "));
    assert_eq!(rows[2].matching_text, Some("river"));
    assert_eq!(rows[2].suffix, Some(" after"));
    assert_eq!(rows[2].indexed_revision, Some(4));

    workspace.update(ProjectMessage::SetReplacementIncluded {
        node_id: "match-opening".into(),
        included: false,
    });
    assert_eq!(
        workspace.replacement_preview().check_state("all-matches"),
        ReplacementCheckState::Indeterminate
    );
    assert!(
        !workspace
            .replacement_preview()
            .can_apply(workspace.project_revision())
    );

    workspace.update(ProjectMessage::SelectNoReplacementMatches);
    assert!(
        workspace
            .update(ProjectMessage::ApplyReplacement)
            .is_empty()
    );
    workspace.update(ProjectMessage::SelectAllReplacementMatches);
    let effects = workspace.update(ProjectMessage::OpenReplacementPreview);
    assert!(matches!(
        effects.as_slice(),
        [ProjectEffect::BuildReplacementPreview { .. }]
    ));
    let validation = workspace.begin_task(ProjectTask::ReplacementPreview);
    assert!(
        workspace.accept_completion(ProjectTaskCompletion::for_ticket(
            validation,
            ProjectTaskPayload::ReplacementPreviewReady,
        ))
    );

    let effects = workspace.update(ProjectMessage::ApplyReplacement);
    let [
        ProjectEffect::ApplyGlobalReplacement {
            included_match_ids, ..
        },
    ] = effects.as_slice()
    else {
        panic!("revalidated selections must reach the runtime exactly");
    };
    assert_eq!(included_match_ids, &["match-opening", "match-research"]);

    let refreshed_project = apply_project_command(
        &fixture.snapshot.project,
        fixture.snapshot.project.revision,
        ProjectCommand::rename_node(fixture.manuscript_node, "Opening refreshed"),
    )
    .expect("authoritative replacement refresh advances the project")
    .project;
    let refreshed_snapshot = ProjectSnapshot {
        project: refreshed_project,
        document_summaries: fixture.snapshot.document_summaries.clone(),
        documents: fixture.snapshot.documents.clone(),
        styles_css: fixture.snapshot.styles_css.clone(),
    };
    let apply = workspace.begin_task(ProjectTask::ApplyReplacement);
    assert!(
        workspace.accept_completion(ProjectTaskCompletion::for_ticket(
            apply,
            ProjectTaskPayload::ReplacementApplied {
                revision: refreshed_snapshot.project.revision.value(),
            },
        ))
    );
    workspace.reconcile_snapshot(&refreshed_snapshot);
    assert_eq!(
        workspace.project_revision(),
        refreshed_snapshot.project.revision.value()
    );
    assert!(
        !workspace.replacement_preview().uses_middle_pane(),
        "an authoritative replacement refresh closes the completed preview"
    );
}

#[test]
fn replacement_revalidation_failure_keeps_selected_matches_visible_and_skipped() {
    let fixture = production_snapshot();
    let mut workspace = ProjectWorkspace::from_snapshot(&fixture.snapshot);
    let document = id_string(fixture.manuscript_document.as_bytes());
    let search = workspace.begin_task(ProjectTask::GlobalSearch { generation: 0 });
    assert!(
        workspace.accept_completion(ProjectTaskCompletion::for_ticket(
            search,
            ProjectTaskPayload::SearchBatch {
                results: vec![streamed_result(
                    &document,
                    "stale-match",
                    "before ",
                    "river",
                    " after",
                    4,
                )],
                finished: true,
            },
        ))
    );
    workspace.update(ProjectMessage::OpenReplacementPreview);
    let validation = workspace.begin_task(ProjectTask::ReplacementPreview);
    assert!(
        workspace.accept_completion(ProjectTaskCompletion::for_ticket(
            validation,
            ProjectTaskPayload::Failed(
                "replacement source changed after the search completed".into()
            ),
        ))
    );

    assert_eq!(
        workspace.replacement_preview().included_match_ids(),
        ["stale-match"]
    );
    assert!(
        workspace
            .replacement_preview()
            .validation_error()
            .is_some_and(|error| error.contains("source changed"))
    );
    let stale = workspace
        .replacement_preview()
        .rows()
        .into_iter()
        .find(|row| row.node_id == "stale-match")
        .expect("the selected stale match remains visible");
    assert!(stale.issue.is_some());
    assert!(
        workspace
            .update(ProjectMessage::ApplyReplacement)
            .is_empty()
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
            checkpoint_label: "Draft Two".into(),
            affected_summary: "1 document".into(),
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
fn recently_deleted_preview_uses_the_selected_canonical_snapshot_without_exposing_html_source() {
    let mut fixture = production_snapshot();
    let deleted_document = id_string(fixture.deleted_document.as_bytes());
    fixture
        .snapshot
        .documents
        .iter_mut()
        .find(|document| document.document_id == fixture.deleted_document)
        .expect("deleted document remains in the authoritative session snapshot")
        .body = "<h1>Recovered heading</h1><p><strong>Formatted</strong> preview body.</p>".into();
    let mut workspace = ProjectWorkspace::from_snapshot(&fixture.snapshot);
    let deleted_node = id_string(fixture.deleted_group.as_bytes());

    assert_eq!(
        workspace.recently_deleted().selected_item_id(),
        Some(deleted_node.as_str())
    );
    let preview = workspace
        .recently_deleted()
        .selected_preview()
        .expect("the selected tombstone has an authoritative document snapshot");
    assert_eq!(preview.document_id, deleted_document);
    assert_eq!(preview.title, "Discarded Scene");
    assert_eq!(
        preview.semantic.plain_text(),
        "Recovered heading\nFormatted preview body."
    );

    workspace.update(ProjectMessage::SelectRecentlyDeleted(
        "unknown-deleted-node".into(),
    ));
    assert_eq!(
        workspace.recently_deleted().selected_item_id(),
        Some(deleted_node.as_str())
    );
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
fn export_controls_progress_cancel_retry_and_artifact_actions_are_typed() {
    let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Export);
    assert_eq!(
        workspace.update(ProjectMessage::SetExportTitleSetting(
            ProjectExportSetting::Disabled,
        )),
        [ProjectEffect::SetProjectExportSettings(
            ProjectExportSettings {
                excluded: false,
                emit_titles: ProjectExportSetting::Disabled,
                starts_new_page: false,
            }
        )]
    );
    assert_eq!(
        workspace.update(ProjectMessage::SetExportPageBreak(true)),
        [ProjectEffect::SetProjectExportSettings(
            ProjectExportSettings {
                excluded: false,
                emit_titles: ProjectExportSetting::Disabled,
                starts_new_page: true,
            }
        )]
    );

    assert_eq!(
        workspace.update(ProjectMessage::BrowseExportDestination),
        [ProjectEffect::ChooseExportDestination {
            output_name: "manuscript.html".into(),
        }]
    );
    assert_eq!(workspace.export().state(), ExportState::ChoosingDestination);
    workspace.update(ProjectMessage::SetExportDestination(Some(
        "manuscript.html".into(),
    )));
    assert_eq!(workspace.export().state(), ExportState::Ready);
    assert_eq!(
        workspace.update(ProjectMessage::StartExport),
        [ProjectEffect::ExportEntireManuscript {
            output_name: "manuscript.html".into(),
            number_documents: false,
            source_revision: 1,
        }]
    );
    assert_eq!(workspace.export().state(), ExportState::Planning);
    assert_eq!(
        workspace.update(ProjectMessage::ExportProgress {
            completed: 1,
            total: 3,
        }),
        []
    );
    assert_eq!(
        workspace.export().state(),
        ExportState::Exporting {
            completed: 1,
            total: 3,
        }
    );
    assert_eq!(
        workspace.update(ProjectMessage::CancelExport),
        [ProjectEffect::CancelExport]
    );
    workspace.update(ProjectMessage::ExportCancelled);
    assert_eq!(workspace.export().state(), ExportState::Cancelled);

    let first = workspace.begin_task(ProjectTask::Export { source_revision: 1 });
    let second = workspace.begin_task(ProjectTask::Export { source_revision: 1 });
    assert!(
        !workspace.accept_completion(ProjectTaskCompletion::for_ticket(
            first,
            ProjectTaskPayload::ExportProgress {
                completed: 2,
                total: 3,
            },
        ))
    );
    let artifact = ExportArtifact {
        token: ExportArtifactToken::from_raw(22),
        display_name: "manuscript.html".into(),
    };
    assert!(
        workspace.accept_completion(ProjectTaskCompletion::for_ticket(
            second,
            ProjectTaskPayload::ExportSucceeded {
                artifact: artifact.clone(),
            },
        ))
    );
    assert_eq!(
        workspace.update(ProjectMessage::OpenExportResult),
        [ProjectEffect::OpenExportResult(artifact.token)]
    );
    assert_eq!(
        workspace.update(ProjectMessage::RevealExportResult),
        [ProjectEffect::RevealExportResult(artifact.token)]
    );

    let same_name_new_token = ExportArtifact {
        token: ExportArtifactToken::from_raw(23),
        display_name: artifact.display_name,
    };
    workspace.update(ProjectMessage::ExportSucceeded(same_name_new_token.clone()));
    assert_eq!(
        workspace.update(ProjectMessage::OpenExportResult),
        [ProjectEffect::OpenExportResult(same_name_new_token.token)]
    );
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
    assert_eq!(workspace.content_state(), &ContentState::Recovery);
    assert!(
        !workspace.recovery().is_disposable_after_durable_save(),
        "accepting recovery retains it until a durable save completes"
    );
    let ticket = workspace.begin_task(ProjectTask::AcceptRecovery);
    assert!(
        workspace.accept_completion(ProjectTaskCompletion::for_ticket(
            ticket,
            ProjectTaskPayload::RecoveryAccepted { revision: 1 },
        ))
    );
    assert!(workspace.recovery().is_disposable_after_durable_save());
    assert_eq!(workspace.content_state(), &ContentState::Ready);
}

#[test]
fn recovery_failure_stays_blocking_and_retry_reconciles_a_new_exact_request() {
    let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::ErrorRecovery);
    assert_eq!(
        workspace.update(ProjectMessage::AcceptRecovery),
        [ProjectEffect::FocusRecoveredEditor]
    );
    let failed = workspace.begin_task(ProjectTask::AcceptRecovery);
    assert!(
        workspace.accept_completion(ProjectTaskCompletion::for_ticket(
            failed,
            ProjectTaskPayload::Failed("disk full".into()),
        ))
    );
    assert_eq!(workspace.content_state(), &ContentState::Recovery);
    assert_eq!(workspace.recovery().error(), Some("disk full"));

    assert_eq!(
        workspace.update(ProjectMessage::RetryRecovery),
        [ProjectEffect::ReconcileRecovery]
    );
    let retry = workspace.begin_task(ProjectTask::ReconcileRecovery);
    assert!(
        workspace.accept_completion(ProjectTaskCompletion::for_ticket(
            retry,
            ProjectTaskPayload::RecoveryAvailable {
                accepted_records: 2,
                affected_documents: vec![("document-a".into(), 7)],
                isolation: None,
            },
        ))
    );
    assert_eq!(workspace.recovery().error(), None);
    assert_eq!(
        workspace.recovery().affected_documents(),
        &[("document-a".into(), 7)]
    );
}

#[test]
fn recovery_completion_from_a_closed_project_generation_is_ignored() {
    let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::ErrorRecovery);
    let stale = workspace.begin_task(ProjectTask::ReconcileRecovery);
    workspace.begin_session(38, 1);

    assert!(
        !workspace.accept_completion(ProjectTaskCompletion::for_ticket(
            stale,
            ProjectTaskPayload::RecoveryUnavailable,
        ))
    );
    assert_eq!(workspace.content_state(), &ContentState::Recovery);
}

#[test]
fn discarding_recovery_keeps_the_current_snapshot_ready_without_claiming_recovery_acceptance() {
    let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::ErrorRecovery);
    assert_eq!(
        workspace.update(ProjectMessage::DiscardRecovery),
        [ProjectEffect::DiscardRecovery]
    );
    let ticket = workspace.begin_task(ProjectTask::DiscardRecovery);
    assert!(
        workspace.accept_completion(ProjectTaskCompletion::for_ticket(
            ticket,
            ProjectTaskPayload::RecoveryDiscarded { revision: 1 },
        ))
    );
    assert_eq!(workspace.content_state(), &ContentState::Ready);
    assert!(!workspace.recovery().is_disposable_after_durable_save());
}

#[test]
fn production_snapshot_hydrates_ordered_hierarchy_metadata_deleted_items_and_editor_counts() {
    let fixture = production_snapshot();
    let workspace = ProjectWorkspace::from_snapshot(&fixture.snapshot);
    let rows = workspace.explorer().rows();

    assert_eq!(
        rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        [
            id_string(NodeId::manuscript_root().as_bytes()),
            id_string(fixture.group.as_bytes()),
            id_string(fixture.manuscript_node.as_bytes()),
            id_string(NodeId::research_root().as_bytes()),
            id_string(fixture.research_node.as_bytes()),
        ]
    );
    let manuscript = workspace
        .explorer()
        .row(&id_string(fixture.manuscript_node.as_bytes()))
        .unwrap();
    assert_eq!(manuscript.kind, HierarchyRowKind::Document);
    assert_eq!(
        manuscript.parent_id,
        Some(id_string(fixture.group.as_bytes()).as_str())
    );
    assert_eq!(manuscript.title, "Opening Scene");
    assert_eq!(manuscript.synopsis, "The river meeting.");
    assert_eq!(
        manuscript.document_id,
        Some(id_string(fixture.manuscript_document.as_bytes()).as_str())
    );

    let field_id = id_string(fixture.field.as_bytes());
    let node_id = id_string(fixture.manuscript_node.as_bytes());
    let inspector_item = workspace.inspector().metadata_items(&node_id)[0].clone();
    assert_eq!(inspector_item.field_id, field_id);
    assert_eq!(inspector_item.stored_value, Some("Final"));
    assert_eq!(inspector_item.effective_value, Some("Final"));
    assert!(workspace.settings().metadata_fields()[0].visible_on_cards);
    let manuscript_root = id_string(NodeId::manuscript_root().as_bytes());
    assert_eq!(workspace.cards().section_id(), manuscript_root);
    assert_eq!(
        workspace.cards().items()[1].metadata,
        [(field_id.as_str(), "Status", Some("Final"))]
    );

    let deleted_id = id_string(fixture.deleted_group.as_bytes());
    let deleted = &workspace.recently_deleted().items()[0];
    assert_eq!(deleted.node_id, deleted_id);
    assert_eq!(deleted.title, "Discarded Part");
    assert_eq!(deleted.kind, HierarchyRowKind::Group);
    assert_eq!(deleted.deleted_at_unix_millis, 123);
    assert_eq!(
        deleted.former_location,
        &RestoreLocation::FormerParent(manuscript_root.clone())
    );
    assert_eq!(
        deleted.restore_location,
        &RestoreLocation::FormerParent(manuscript_root.clone())
    );
    assert_eq!(
        deleted.fallback_location,
        &RestoreLocation::SectionRoot(manuscript_root)
    );
    assert!(deleted.formatted_preview_available);

    assert_eq!(
        workspace.project_revision(),
        fixture.snapshot.project.revision.value()
    );
    assert_eq!(
        workspace.export().project_settings(),
        ProjectExportSettings {
            excluded: false,
            emit_titles: Default::default(),
            starts_new_page: true,
        }
    );
    assert_eq!(
        workspace
            .export()
            .node_settings(&id_string(fixture.manuscript_node.as_bytes())),
        Some(ProjectExportSettings {
            excluded: true,
            emit_titles: Default::default(),
            starts_new_page: false,
        })
    );
    assert_eq!(
        workspace.save().state(),
        SaveState::SavedThrough(fixture.snapshot.project.revision.value())
    );
    assert_eq!(
        workspace
            .editor()
            .pane(EditorPane::Primary)
            .active_document(),
        Some(id_string(fixture.manuscript_document.as_bytes()).as_str())
    );
    assert_eq!(
        workspace
            .editor()
            .pane(EditorPane::Primary)
            .active_document(),
        Some("05050505050505050505050505050505")
    );
    assert!(
        !workspace
            .editor()
            .pane(EditorPane::Companion)
            .is_populated()
    );
    assert_eq!(
        workspace.editor().status_bar().current_count(),
        StatusCount::ActiveDocument(4)
    );
    assert_eq!(workspace.editor().status_bar().manuscript_total(), 4);
    assert_eq!(
        workspace
            .editor()
            .document_revision(&id_string(fixture.research_document.as_bytes())),
        Some(5)
    );
}

#[test]
fn snapshot_refresh_does_not_reopen_a_tab_the_author_closed() {
    let fixture = production_snapshot();
    let research_document = id_string(fixture.research_document.as_bytes());
    let mut workspace = ProjectWorkspace::from_snapshot(&fixture.snapshot);

    workspace.editor_mut().update(EditorMessage::CloseTab {
        pane: EditorPane::Primary,
        document_id: research_document.clone(),
    });
    workspace.reconcile_snapshot(&fixture.snapshot);

    assert!(
        workspace
            .editor()
            .pane(EditorPane::Primary)
            .tabs()
            .iter()
            .all(|tab| tab.id() != research_document)
    );
}

#[test]
fn production_snapshot_stays_loading_until_startup_recovery_reports_no_records() {
    let fixture = production_snapshot();
    let mut workspace = ProjectWorkspace::from_snapshot(&fixture.snapshot);
    workspace.begin_session(4, fixture.snapshot.project.revision.value());
    let ticket = workspace.begin_recovery_reconciliation();
    assert_eq!(workspace.content_state(), &ContentState::Loading);
    assert!(
        workspace.accept_completion(ProjectTaskCompletion::for_ticket(
            ticket,
            ProjectTaskPayload::RecoveryUnavailable,
        ))
    );
    assert_eq!(workspace.content_state(), &ContentState::Ready);
}

#[test]
fn snapshot_reconciliation_preserves_surviving_ui_state_and_removes_stale_references() {
    let fixture = production_snapshot();
    let mut workspace = ProjectWorkspace::from_snapshot(&fixture.snapshot);
    let group_id = id_string(fixture.group.as_bytes());
    let manuscript_node_id = id_string(fixture.manuscript_node.as_bytes());
    let research_node_id = id_string(fixture.research_node.as_bytes());
    let primary_view = workspace.editor().pane(EditorPane::Primary).view();

    workspace.update(ProjectMessage::SelectHierarchy {
        node_id: manuscript_node_id.clone(),
        gesture: SelectionGesture::Replace,
    });
    workspace.update(ProjectMessage::ToggleHierarchyExpanded(group_id.clone()));
    workspace.editor_mut().update(EditorMessage::OpenLocalFind);
    workspace
        .editor_mut()
        .update(EditorMessage::SetFindQuery("river".into()));
    workspace.update(ProjectMessage::ActivateCard(research_node_id.clone()));

    let mut renamed_project = fixture.snapshot.project.clone();
    renamed_project.nodes.get_mut(fixture.group).unwrap().title = "Part I".into();
    renamed_project
        .nodes
        .get_mut(fixture.manuscript_node)
        .unwrap()
        .title = "Opening Revised".into();
    let renamed_project = apply_project_command(
        &renamed_project,
        renamed_project.revision,
        ProjectCommand::delete_node(fixture.research_node),
    )
    .unwrap()
    .project;
    let renamed_snapshot = ProjectSnapshot {
        project: renamed_project,
        document_summaries: fixture
            .snapshot
            .document_summaries
            .iter()
            .filter(|document| document.document_id != fixture.research_document)
            .cloned()
            .collect(),
        documents: fixture
            .snapshot
            .documents
            .iter()
            .filter(|document| document.document_id != fixture.research_document)
            .cloned()
            .collect(),
        styles_css: fixture.snapshot.styles_css.clone(),
    };
    workspace.reconcile_snapshot(&renamed_snapshot);

    assert_eq!(workspace.explorer().selected_ids(), [manuscript_node_id]);
    assert!(workspace.explorer().is_expanded(&group_id));
    assert_eq!(workspace.explorer().title(&group_id), Some("Part I"));
    assert_eq!(
        workspace.editor().local_search(primary_view).query(),
        "river"
    );
    assert!(
        !workspace
            .editor()
            .pane(EditorPane::Companion)
            .is_populated()
    );
    assert!(workspace.cards().last_activated_document().is_none());
    assert_eq!(
        workspace.editor().pane(EditorPane::Primary).tabs()[0].title(),
        "Opening Revised"
    );

    let removed_project = apply_project_command(
        &renamed_snapshot.project,
        renamed_snapshot.project.revision,
        ProjectCommand::delete_node(fixture.manuscript_node),
    )
    .unwrap()
    .project;
    workspace.reconcile_snapshot(&ProjectSnapshot {
        project: removed_project,
        document_summaries: renamed_snapshot.document_summaries,
        documents: renamed_snapshot.documents,
        styles_css: renamed_snapshot.styles_css,
    });
    assert!(workspace.explorer().selected_ids().is_empty());
    assert!(!workspace.editor().pane(EditorPane::Primary).is_populated());
    assert!(!workspace.editor().local_search(primary_view).is_open());
}

#[test]
fn workspace_snapshot_restores_panes_tabs_views_split_scroll_and_mode() {
    let fixture = production_snapshot();
    let mut workspace = ProjectWorkspace::from_snapshot(&fixture.snapshot);
    workspace.editor_mut().set_split_ratio(0.62);
    workspace.editor_mut().update(EditorMessage::OpenTab {
        pane: EditorPane::Primary,
        tab: parchmint_ui_iced::TabSpec::new(
            id_string(fixture.research_document.as_bytes()),
            "River Notes",
        ),
    });
    workspace
        .editor_mut()
        .set_scroll_offset(EditorPane::Primary, 247.0);
    let research_node = id_string(fixture.research_node.as_bytes());
    workspace.update(ProjectMessage::SelectHierarchy {
        node_id: research_node.clone(),
        gesture: SelectionGesture::Replace,
    });
    workspace.update(ProjectMessage::SetCardsSection(id_string(
        NodeId::research_root().as_bytes(),
    )));
    let mut layout = ShellLayout::for_window(1440, 900);
    layout.restore_panes(334, 418, true, false);

    let saved = workspace.workspace_snapshot(&layout, RibbonDestination::Cards);
    assert_eq!(saved.layout.explorer_width, 334);
    assert_eq!(saved.layout.inspector_width, 418);
    assert!(saved.layout.inspector_collapsed);
    assert_eq!(saved.layout.split_ratio, 0.62);
    assert!(!saved.tabs.is_empty());
    assert!(saved.active_view.is_some());
    assert!(!saved.views.is_empty());
    assert_eq!(saved.explorer.selected_nodes.len(), 1);
    assert_eq!(saved.cards_section, Some(NodeId::research_root()));

    let mut restored = ProjectWorkspace::from_snapshot(&fixture.snapshot);
    assert_eq!(
        restored.apply_workspace_snapshot(&saved),
        RibbonDestination::Cards
    );
    assert_eq!(restored.editor().split_ratio(), 0.62);
    assert_eq!(
        restored.editor().pane(EditorPane::Primary).tabs().len(),
        saved.tabs.len()
    );
    assert_eq!(
        restored
            .editor()
            .pane(EditorPane::Primary)
            .active_document(),
        Some(id_string(fixture.research_document.as_bytes()).as_str())
    );
    assert_eq!(
        restored.editor().pane(EditorPane::Primary).scroll_offset(),
        247.0
    );
    assert_eq!(restored.explorer().selected_ids(), [research_node]);
    assert_eq!(
        restored.cards().section_id(),
        id_string(NodeId::research_root().as_bytes())
    );
}

#[test]
fn history_load_more_state_finishes_for_success_and_failure() {
    let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::History);
    workspace.finish_history_page(Some("next-page".into()));
    assert_eq!(workspace.history().next_cursor(), Some("next-page"));
    workspace.begin_history_load_more();
    assert!(workspace.history().is_loading_more());

    let ticket = workspace.begin_task(ProjectTask::LoadHistory);
    assert!(
        workspace.accept_completion(ProjectTaskCompletion::for_ticket(
            ticket,
            ProjectTaskPayload::Failed("history unavailable".into()),
        ))
    );
    assert!(!workspace.history().is_loading_more());
    assert_eq!(workspace.history().error(), Some("history unavailable"));
}

struct ProductionSnapshotFixture {
    snapshot: ProjectSnapshot,
    group: NodeId,
    manuscript_node: NodeId,
    manuscript_document: DocumentId,
    research_node: NodeId,
    research_document: DocumentId,
    deleted_group: NodeId,
    deleted_document: DocumentId,
    field: MetadataFieldId,
}

fn streamed_result(
    document_id: &str,
    match_id: &str,
    prefix: &str,
    matching_text: &str,
    suffix: &str,
    indexed_revision: u64,
) -> GlobalSearchResult {
    GlobalSearchResult {
        document_id: document_id.to_owned(),
        match_id: match_id.to_owned(),
        prefix: prefix.to_owned(),
        matching_text: matching_text.to_owned(),
        suffix: suffix.to_owned(),
        indexed_revision,
    }
}

fn production_snapshot() -> ProductionSnapshotFixture {
    let group = NodeId::from_bytes([3; 16]);
    let manuscript_node = NodeId::from_bytes([4; 16]);
    let manuscript_document = DocumentId::from_bytes([5; 16]);
    let research_node = NodeId::from_bytes([6; 16]);
    let research_document = DocumentId::from_bytes([7; 16]);
    let deleted_group = NodeId::from_bytes([8; 16]);
    let deleted_node = NodeId::from_bytes([9; 16]);
    let deleted_document = DocumentId::from_bytes([10; 16]);
    let field = MetadataFieldId::from_bytes([11; 16]);
    let mut project = Project::new(ProjectId::from_bytes([1; 16]));
    project
        .metadata
        .upsert(MetadataFieldDefinition {
            id: field,
            label: "Status".into(),
            description: Some("Draft state".into()),
            applicability: MetadataApplicability::Documents,
            text_kind: MetadataTextKind::SingleLine,
            default_value: Some("Draft".into()),
            visible_on_cards: true,
        })
        .unwrap();
    project
        .nodes
        .try_insert_group(group, NodeId::manuscript_root(), 0, "Part One")
        .unwrap();
    project
        .nodes
        .try_insert_document(
            manuscript_node,
            manuscript_document,
            group,
            0,
            "Opening Scene",
        )
        .unwrap();
    project
        .nodes
        .try_insert_document(
            research_node,
            research_document,
            NodeId::research_root(),
            0,
            "River Notes",
        )
        .unwrap();
    project
        .nodes
        .try_insert_group(
            deleted_group,
            NodeId::manuscript_root(),
            1,
            "Discarded Part",
        )
        .unwrap();
    project
        .nodes
        .try_insert_document(
            deleted_node,
            deleted_document,
            deleted_group,
            0,
            "Discarded Scene",
        )
        .unwrap();
    let manuscript = project.nodes.get_mut(manuscript_node).unwrap();
    manuscript.synopsis = "The river meeting.".into();
    manuscript.metadata.insert(field, "Final".into());
    manuscript.export_settings.excluded = true;
    project.export_settings.starts_new_page = true;
    project.revision = 7_u64.into();
    let project = apply_project_command(
        &project,
        project.revision,
        ProjectCommand::delete_node_at(deleted_group, 123),
    )
    .unwrap()
    .project;
    let documents = vec![
        DocumentSnapshot {
            comments: Vec::new(),
            document_id: manuscript_document,
            body: "one two three four".into(),
            revision: EditorRevision::from(3),
            visibility: DocumentVisibility::Closed,
        },
        DocumentSnapshot {
            comments: Vec::new(),
            document_id: research_document,
            body: "five six".into(),
            revision: EditorRevision::from(5),
            visibility: DocumentVisibility::Open,
        },
        DocumentSnapshot {
            comments: Vec::new(),
            document_id: deleted_document,
            body: "formatted deleted preview".into(),
            revision: EditorRevision::from(2),
            visibility: DocumentVisibility::Closed,
        },
    ];
    let document_summaries = documents
        .iter()
        .map(|document| DocumentSummary {
            document_id: document.document_id,
            revision: document.revision,
            visibility: document.visibility,
            content_hash: None,
            word_count: DocumentWordCount::Known(document.body.split_whitespace().count()),
        })
        .collect();
    ProductionSnapshotFixture {
        snapshot: ProjectSnapshot {
            project,
            document_summaries,
            documents,
            styles_css: String::new(),
        },
        group,
        manuscript_node,
        manuscript_document,
        research_node,
        research_document,
        deleted_group,
        deleted_document,
        field,
    }
}

fn id_string(bytes: &[u8; 16]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
