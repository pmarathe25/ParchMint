use std::collections::BTreeMap;

use super::*;

fn document_id(value: u8) -> DocumentId {
    DocumentId::from_bytes([value; 16])
}

fn document(
    id: u8,
    title: &str,
    revision: u64,
    body: &str,
) -> (ExportNode, (DocumentId, ExportSource)) {
    let id = document_id(id);
    (
        ExportNode::document(id, title, ExportSettings::default()),
        (
            id,
            ExportSource {
                revision: revision.into(),
                body: body.into(),
            },
        ),
    )
}

fn snapshot(
    manuscript: Vec<ExportNode>,
    sources: BTreeMap<DocumentId, ExportSource>,
) -> ProjectSnapshot {
    let mut project = ProjectSnapshot::new(
        ExportStyleCatalog::new("p { color: black; }"),
        ExportDefaults::default(),
        manuscript,
        sources,
    );
    project.research = vec![ExportNode::document(
        document_id(99),
        "Research",
        ExportSettings::default(),
    )];
    project
        .comments
        .insert(document_id(1), vec!["comment".into()]);
    project.metadata.insert("author".into(), "private".into());
    project
}

fn plan(project: &ProjectSnapshot) -> ExportPlan {
    ExportPlan::build(
        ExportRequest::new("manuscript.html", ExportRunOptions::default()),
        project,
    )
    .expect("fixture creates a valid plan")
}

#[test]
fn whole_manuscript_is_ordered_and_excludes_research_comments_and_metadata() {
    let (first, source_one) = document(1, "First", 4, "one");
    let (second, source_two) = document(2, "Second", 4, "two");
    let mut project = snapshot(
        vec![ExportNode::group(
            "Chapter",
            ExportSettings::default(),
            vec![first, second],
        )],
        BTreeMap::from([source_one, source_two]),
    );

    let result = plan(&project);
    project
        .sources
        .get_mut(&document_id(1))
        .expect("source")
        .body = "later body".into();
    project
        .sources
        .get_mut(&document_id(1))
        .expect("source")
        .revision = 5.into();
    assert_eq!(
        result.items(),
        &[
            SemanticExportItem::GroupHeading(ExportHeading {
                title: "Chapter".into(),
                settings: EffectiveExportSettings {
                    emit_titles: true,
                    start_new_page: false,
                },
            }),
            SemanticExportItem::Document(ExportDocument {
                id: document_id(1),
                title: "First".into(),
                body: "one".into(),
                settings: EffectiveExportSettings {
                    emit_titles: true,
                    start_new_page: false,
                },
                source_revision: 4.into(),
            }),
            SemanticExportItem::Document(ExportDocument {
                id: document_id(2),
                title: "Second".into(),
                body: "two".into(),
                settings: EffectiveExportSettings {
                    emit_titles: true,
                    start_new_page: false,
                },
                source_revision: 4.into(),
            }),
        ]
    );
    assert_eq!(
        result.source_revisions(),
        &BTreeMap::from([(document_id(1), 4.into()), (document_id(2), 4.into())])
    );
}

#[test]
fn inherited_settings_resolve_project_group_and_document_levels() {
    let (inherited, source_one) = document(1, "Inherited", 3, "one");
    let explicit = ExportNode::document(
        document_id(2),
        "Explicit",
        ExportSettings {
            emit_titles: InheritedSetting::Disabled,
            start_new_page: InheritedSetting::Enabled,
        },
    );
    let project = snapshot(
        vec![ExportNode::group(
            "Group",
            ExportSettings {
                emit_titles: InheritedSetting::Disabled,
                start_new_page: InheritedSetting::Enabled,
            },
            vec![inherited, explicit],
        )],
        BTreeMap::from([
            source_one,
            (
                document_id(2),
                ExportSource {
                    revision: 3.into(),
                    body: "two".into(),
                },
            ),
        ]),
    );

    let result = plan(&project);
    assert!(matches!(
        &result.items()[0],
        SemanticExportItem::Document(ExportDocument { settings, .. })
            if !settings.emit_titles && settings.start_new_page
    ));
    assert!(matches!(result.items()[1], SemanticExportItem::PageBreak));
    assert!(matches!(
        &result.items()[2],
        SemanticExportItem::Document(ExportDocument { settings, .. })
            if !settings.emit_titles && settings.start_new_page
    ));
}

#[test]
fn validation_rejects_missing_sources_and_unsafe_targets_but_keeps_per_document_revisions() {
    let (first, source_one) = document(1, "First", 1, "one");
    let missing = ExportNode::document(document_id(2), "Missing", ExportSettings::default());
    let project = snapshot(vec![first, missing], BTreeMap::from([source_one]));
    let unsafe_target = ExportPlan::build(
        ExportRequest::new("../outside.html", ExportRunOptions::default()),
        &project,
    )
    .expect_err("unsafe target must fail");
    assert!(
        unsafe_target
            .issues()
            .iter()
            .any(|issue| matches!(issue, ExportValidationIssue::UnsafeOutputTarget { .. }))
    );
    assert!(unsafe_target.issues().iter().any(|issue| matches!(
        issue,
        ExportValidationIssue::MissingSource { document } if *document == document_id(2)
    )));

    let (second, source_two) = document(2, "Second", 2, "two");
    let mixed = snapshot(
        vec![
            ExportNode::document(document_id(1), "First", ExportSettings::default()),
            second,
        ],
        BTreeMap::from([
            (
                document_id(1),
                ExportSource {
                    revision: 1.into(),
                    body: "one".into(),
                },
            ),
            source_two,
        ]),
    );
    let plan = ExportPlan::build(
        ExportRequest::new("safe.html", ExportRunOptions::default()),
        &mixed,
    )
    .expect("each document revision should be captured independently");
    assert!(
        plan.source_revisions()
            .values()
            .copied()
            .eq([1.into(), 2.into()]),
        "the immutable plan must retain each document's captured revision"
    );
}

#[derive(Default)]
struct Sink {
    temporary: Option<Vec<u8>>,
    completed: Option<Vec<u8>>,
    fail_start: bool,
}

impl ExportSink for Sink {
    fn start(&mut self, _: &ExportTargetCapability) -> Result<(), ExportError> {
        self.temporary = Some(Vec::new());
        if self.fail_start {
            Err(ExportError::Sink {
                operation: "start",
                reason: "injected failure".into(),
            })
        } else {
            Ok(())
        }
    }

    fn write_chunk(&mut self, bytes: &[u8]) -> Result<(), ExportError> {
        self.temporary
            .as_mut()
            .ok_or(ExportError::InvalidState)?
            .extend_from_slice(bytes);
        Ok(())
    }

    fn finish(&mut self) -> Result<(), ExportError> {
        self.completed = self.temporary.take();
        Ok(())
    }

    fn abort(&mut self) {
        self.temporary = None;
    }
}

#[test]
fn cancelled_export_cleans_temporary_output_and_cannot_complete() {
    let (first, source_one) = document(1, "First", 1, "one");
    let (second, source_two) = document(2, "Second", 1, "two");
    let result = plan(&snapshot(
        vec![first, second],
        BTreeMap::from([source_one, source_two]),
    ));
    let handle = ExportHandle::new();
    let mut sink = Sink::default();
    let mut output = handle
        .begin_temporary(&mut sink, result.target())
        .expect("temporary output starts");
    output.write_chunk(b"first").expect("first chunk writes");
    assert_eq!(handle.cancel(), CancelOutcome::Cancelled);
    assert_eq!(output.write_chunk(b"second"), Err(ExportError::Cancelled));
    assert_eq!(output.finish(), Err(ExportError::Cancelled));
    assert_eq!(handle.status(), ExportStatus::Cancelled);
    assert!(sink.temporary.is_none() && sink.completed.is_none());
}

#[test]
fn temporary_output_completes_only_after_finish_and_is_cleaned_up_on_start_failure() {
    let (node, source) = document(1, "Only", 1, "body");
    let result = plan(&snapshot(vec![node], BTreeMap::from([source])));
    let handle = ExportHandle::new();
    let mut sink = Sink::default();
    let mut output = handle
        .begin_temporary(&mut sink, result.target())
        .expect("temporary output starts");
    output.write_chunk(b"body").expect("chunk writes");
    assert_eq!(handle.status(), ExportStatus::Running);
    let completion = output.finish().expect("temporary output completes");
    assert_eq!(completion.target, result.target().clone());
    assert_eq!(handle.status(), ExportStatus::Completed);
    assert!(sink.temporary.is_none() && sink.completed.is_some());

    let failed_handle = ExportHandle::new();
    let mut failed_sink = Sink {
        fail_start: true,
        ..Sink::default()
    };
    assert!(matches!(
        failed_handle.begin_temporary(&mut failed_sink, result.target()),
        Err(ExportError::Sink {
            operation: "start",
            reason,
        }) if reason == "injected failure"
    ));
    assert_eq!(failed_handle.status(), ExportStatus::Failed);
    assert!(failed_sink.temporary.is_none() && failed_sink.completed.is_none());
}
