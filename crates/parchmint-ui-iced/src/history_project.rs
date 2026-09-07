//! Project-wide History comparison. Runs entirely on the service worker.
use parchmint_domain::{NodeId, NodeKind, Project, ProjectExportSetting, ProjectSection};

use super::*;
use parchmint_editor_api::{CanonicalCommentAnchor, CanonicalProjection};
use parchmint_project_format::{CanonicalCodec, ProjectFormatCodec};

pub(super) fn compare(
    ports: &dyn ServiceFeedPorts,
    checkpoint: CheckpointId,
    preview: &SnapshotResourcePaths,
    mut current: ProjectSnapshot,
    drafts: Vec<CanonicalProjection>,
) -> Result<Vec<HistoryComparison>, ServiceFeedError> {
    let codec = ProjectFormatCodec::default();
    let checkpoint_id = encode_hex(checkpoint.as_bytes());
    let manifest = resource_text(ports, checkpoint, preview, "project.toml")?;
    let before_project = codec
        .decode_manifest(manifest.as_bytes())
        .and_then(|manifest| codec.decode_domain_project(&manifest, current.project.id))
        .map_err(|error| service_error(ServiceKind::History, error))?;
    let before_documents: BTreeMap<DocumentId, String> = match &before_project {
        Some((project, _)) => document_titles(project),
        None => preview
            .resource_paths
            .iter()
            .filter(|path| path.as_str().ends_with(".html"))
            .map(|path| {
                (
                    parchmint_project_format::legacy_document_id(path),
                    path.as_str().to_owned(),
                )
            })
            .collect(),
    };
    let after_documents = document_titles(&current.project);
    for draft in drafts {
        if let Some(document) = current
            .documents
            .iter_mut()
            .find(|document| document.document_id == draft.document_id())
        {
            document.body = draft.body().to_owned();
            document.comments = draft.comments().to_vec();
        }
    }
    let mut changes = Vec::new();
    let mut add = |title: &str, before: &str, after: &str| {
        if before != after
            || title.starts_with("Added document ·")
            || title.starts_with("Deleted document ·")
        {
            changes.push(crate::project_workspace::compare_history_text(
                &checkpoint_id,
                title,
                before,
                after,
            ));
        }
    };
    let documents: BTreeSet<_> = before_documents
        .keys()
        .chain(after_documents.keys())
        .copied()
        .collect();
    for id in documents {
        let before = load_checkpoint_document(ports, checkpoint, preview, id)?;
        let after = current
            .documents
            .iter()
            .find(|document| document.document_id == id && after_documents.contains_key(&id));
        let title = after_documents
            .get(&id)
            .or_else(|| before_documents.get(&id))
            .expect("union member");
        let label = match (
            before_documents.contains_key(&id),
            after_documents.contains_key(&id),
        ) {
            (false, true) => format!("Added document · {title}"),
            (true, false) => format!("Deleted document · {title}"),
            _ => format!("Document · {title}"),
        };
        let before_text = before
            .as_ref()
            .map(|document| document.semantic.plain_text())
            .unwrap_or_default();
        let after_semantic = after
            .map(|document| {
                EditorCoreSession::open(CanonicalDocumentLoad::new(id, document.body.clone()))
                    .map(|session| session.canonical_projection().semantic().clone())
                    .map_err(|error| service_error(ServiceKind::ProjectQuery, error))
            })
            .transpose()?;
        let after_text = after_semantic
            .as_ref()
            .map(|semantic| semantic.plain_text())
            .unwrap_or_default();
        add(&label, &before_text, &after_text);
        // Text-equivalent formatting changes still need visible evidence.
        if before.is_some()
            && after_semantic.is_some()
            && before_text == after_text
            && before.as_ref().map(|document| &document.semantic) != after_semantic.as_ref()
        {
            let before_html = before
                .as_ref()
                .map(|document| resource_text(ports, checkpoint, preview, &document.canonical_path))
                .transpose()?
                .unwrap_or_default();
            add(
                &format!("Formatting · {title} (HTML)"),
                &before_html,
                after
                    .map(|document| document.body.as_str())
                    .unwrap_or_default(),
            );
        }
        let annotation_path = format!("annotations/{}.json", encode_hex(id.as_bytes()));
        let annotations = resource_text(ports, checkpoint, preview, &annotation_path)?;
        let before_comments = if annotations.is_empty() {
            Vec::new()
        } else {
            codec
                .decode_annotations(annotations.as_bytes())
                .and_then(|annotations| annotations.typed_threads())
                .map_err(|error| service_error(ServiceKind::History, error))?
                .into_iter()
                .map(parchmint_editor_api::CanonicalComment::from)
                .collect()
        };
        add(
            &format!("Comments · {title}"),
            &comments_text(&before_comments),
            &comments_text(
                after
                    .map(|document| document.comments.as_slice())
                    .unwrap_or_default(),
            ),
        );
    }
    if let Some((before, _)) = &before_project {
        add(
            "Project outline and settings",
            &outline(before),
            &outline(&current.project),
        );
    }
    add(
        "Project dictionary",
        &resource_text(ports, checkpoint, preview, "dictionary.txt")?,
        &current
            .project
            .dictionary
            .iter()
            .map(|word| format!("{word}\n"))
            .collect::<String>(),
    );
    add(
        "Project styles (CSS)",
        &resource_text(ports, checkpoint, preview, "styles.css")?,
        &current.styles_css,
    );
    changes.sort_by(|left, right| left.document_title.cmp(&right.document_title));
    Ok(changes)
}

fn resource_text(
    ports: &dyn ServiceFeedPorts,
    checkpoint: CheckpointId,
    preview: &SnapshotResourcePaths,
    path: &str,
) -> Result<String, ServiceFeedError> {
    let path = CanonicalRelativePath::parse(path)
        .map_err(|error| service_error(ServiceKind::History, error))?;
    if !preview.resource_paths.contains(&path) {
        return Ok(String::new());
    }
    String::from_utf8(ports.history_resource(checkpoint, &path)?.bytes)
        .map_err(|error| service_error(ServiceKind::History, error))
}

fn document_titles(project: &Project) -> BTreeMap<DocumentId, String> {
    project
        .nodes
        .iter()
        .filter_map(|(_, node)| match node.kind {
            NodeKind::Document(id) => Some((id, node.title.clone())),
            _ => None,
        })
        .collect()
}

fn outline(project: &Project) -> String {
    fn visit(project: &Project, parent: NodeId, path: &str, output: &mut String) {
        for id in project.nodes.children(parent) {
            let node = project.nodes.get(*id).expect("validated tree");
            let path = format!("{path} / {}", node.title);
            output.push_str(&format!(
                "{path} ({})\n",
                if node.kind.can_have_children() {
                    "group"
                } else {
                    "document"
                }
            ));
            if !node.synopsis.is_empty() {
                output.push_str(&format!("Synopsis: {}\n", node.synopsis));
            }
            for (field, value) in &node.metadata {
                let label = project
                    .metadata
                    .get(*field)
                    .map(|field| field.label.as_str())
                    .unwrap_or("Metadata");
                output.push_str(&format!("{label}: {value}\n"));
            }
            if node.export_settings != Default::default() {
                output.push_str(&format!(
                    "Export: {}\n",
                    export_description(node.export_settings)
                ));
            }
            visit(project, *id, &path, output);
        }
    }
    let mut output = format!(
        "Title: {}\nAuthor: {}\nExport: {}\n",
        project.display_title,
        project.author.as_deref().unwrap_or_default(),
        export_description(project.export_settings)
    );
    for section in [ProjectSection::Manuscript, ProjectSection::Research] {
        visit(
            project,
            section.root_id(),
            match section {
                ProjectSection::Manuscript => "Manuscript",
                ProjectSection::Research => "Research",
            },
            &mut output,
        );
    }
    for field in project.metadata.iter() {
        output.push_str(&format!(
            "Metadata field: {} · {:?} · {:?} · default {} · Cards {}\n",
            field.label,
            field.applicability,
            field.text_kind,
            field.default_value.as_deref().unwrap_or_default(),
            field.visible_on_cards
        ));
        if let Some(description) = &field.description {
            output.push_str(&format!("Description: {description}\n"));
        }
    }
    for style in project.styles.iter() {
        output.push_str(&format!(
            "Style: {} · {:?}\n",
            style.display_name, style.role
        ));
    }
    output
}

fn export_description(settings: parchmint_domain::ProjectExportSettings) -> String {
    format!(
        "{}; titles {}; {}",
        if settings.excluded {
            "excluded"
        } else {
            "included"
        },
        match settings.emit_titles {
            ProjectExportSetting::Inherit => "inherited",
            ProjectExportSetting::Enabled => "shown",
            ProjectExportSetting::Disabled => "hidden",
        },
        if settings.starts_new_page {
            "start on a new page"
        } else {
            "continue on the page"
        }
    )
}

fn comments_text(comments: &[parchmint_editor_api::CanonicalComment]) -> String {
    let mut output = String::new();
    for thread in comments {
        let (quote, orphaned) = match &thread.anchor {
            CanonicalCommentAnchor::Text {
                quote, orphaned, ..
            } => (quote.as_str(), *orphaned),
            CanonicalCommentAnchor::Document { .. } => ("document", false),
        };
        output.push_str(&format!(
            "Comment on {quote} · {}{}\n",
            if thread.resolved {
                "Resolved"
            } else {
                "Unresolved"
            },
            if orphaned {
                " · anchor unavailable"
            } else {
                ""
            }
        ));
        for message in &thread.messages {
            output.push_str(&message.body);
            output.push('\n');
        }
    }
    output
}
