//! Project-wide History comparison. Runs entirely on the service worker.
use parchmint_domain::{NodeId, NodeKind, Project, ProjectExportSetting};

use super::*;
use parchmint_editor_api::{CanonicalCommentAnchor, CanonicalProjection};
use parchmint_project_format::{CanonicalCodec, ProjectFormatCodec};

pub(super) fn compare(
    ports: &dyn ServiceFeedPorts,
    checkpoint: CheckpointId,
    preview: &SnapshotResourcePaths,
    current: ProjectSnapshot,
    drafts: Vec<CanonicalProjection>,
) -> Result<Vec<HistoryComparison>, ServiceFeedError> {
    compare_scope(ports, checkpoint, preview, current, drafts, None)
}

pub(super) fn compare_document(
    ports: &dyn ServiceFeedPorts,
    checkpoint: CheckpointId,
    preview: &SnapshotResourcePaths,
    current: ProjectSnapshot,
    drafts: Vec<CanonicalProjection>,
    document: DocumentId,
) -> Result<Vec<HistoryComparison>, ServiceFeedError> {
    compare_scope(ports, checkpoint, preview, current, drafts, Some(document))
}

fn compare_scope(
    ports: &dyn ServiceFeedPorts,
    checkpoint: CheckpointId,
    preview: &SnapshotResourcePaths,
    mut current: ProjectSnapshot,
    drafts: Vec<CanonicalProjection>,
    document_scope: Option<DocumentId>,
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
    let mut add = |title: &str,
                   before: &str,
                   after: &str,
                   path: Vec<crate::project_workspace::HistoryHeading>| {
        if before != after
            || title.starts_with("Added document ·")
            || title.starts_with("Deleted document ·")
            || title.starts_with("Structure ·")
        {
            let mut comparison = crate::project_workspace::compare_history_text(
                &checkpoint_id,
                title,
                before,
                after,
            );
            comparison.path = path;
            changes.push(comparison);
        }
    };
    let documents: BTreeSet<_> = before_documents
        .keys()
        .chain(after_documents.keys())
        .copied()
        .collect();
    for id in documents {
        if document_scope.is_some_and(|selected| selected != id) {
            continue;
        }
        let before_tree = before_project.as_ref().map(|(project, _)| project);
        let node_id = current
            .project
            .nodes
            .iter()
            .find_map(|(node_id, node)| (node.kind == NodeKind::Document(id)).then_some(*node_id))
            .or_else(|| {
                before_tree.and_then(|p| {
                    p.nodes.iter().find_map(|(node_id, node)| {
                        (node.kind == NodeKind::Document(id)).then_some(*node_id)
                    })
                })
            });
        let path = node_id
            .map(|node| history_path(before_tree, &current.project, node))
            .unwrap_or_default();
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
        add(&label, &before_text, &after_text, path.clone());
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
                path.clone(),
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
            &format!("Notes · {title}"),
            &comments_text(&before_comments),
            &comments_text(
                after
                    .map(|document| document.comments.as_slice())
                    .unwrap_or_default(),
            ),
            path,
        );
    }
    if let Some((before, _)) = &before_project {
        let node_ids: BTreeSet<_> = before
            .nodes
            .iter()
            .chain(current.project.nodes.iter())
            .map(|(id, _)| *id)
            .collect();
        for id in node_ids {
            let old = before.nodes.get(id);
            let new = current.project.nodes.get(id);
            if document_scope.is_some_and(|doc| {
                old.or(new)
                    .is_none_or(|n| n.kind != NodeKind::Document(doc))
            }) {
                continue;
            }
            let path = history_path(Some(before), &current.project, id);
            if path.is_empty() {
                continue;
            }
            let title = new.or(old).map(|n| n.title.as_str()).unwrap_or_default();
            if old.map(|n| (&n.title, before.nodes.parent(id)))
                != new.map(|n| (&n.title, current.project.nodes.parent(id)))
            {
                let before_parent = old
                    .and_then(|_| before.nodes.parent(id))
                    .and_then(|id| before.nodes.get(id))
                    .map(|n| n.title.as_str())
                    .unwrap_or_default();
                let after_parent = new
                    .and_then(|_| current.project.nodes.parent(id))
                    .and_then(|id| current.project.nodes.get(id))
                    .map(|n| n.title.as_str())
                    .unwrap_or_default();
                let moved = old.is_some()
                    && new.is_some()
                    && before.nodes.parent(id) != current.project.nodes.parent(id);
                add(
                    &format!("Structure · {title}"),
                    if moved { before_parent } else { "" },
                    if moved { after_parent } else { "" },
                    path.clone(),
                );
            }
            add(
                &format!("Synopsis · {title}"),
                old.map(|n| n.synopsis.as_str()).unwrap_or_default(),
                new.map(|n| n.synopsis.as_str()).unwrap_or_default(),
                path.clone(),
            );
            let metadata =
                |node: Option<&parchmint_domain::ProjectNode>, project: &Project| -> String {
                    node.map(|node| {
                        node.metadata
                            .iter()
                            .map(|(id, value)| {
                                format!(
                                    "{}: {value}",
                                    project
                                        .metadata
                                        .get(*id)
                                        .map(|f| f.label.as_str())
                                        .unwrap_or("Field")
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    })
                    .unwrap_or_default()
                };
            add(
                &format!("Metadata · {title}"),
                &metadata(old, before),
                &metadata(new, &current.project),
                path.clone(),
            );
            add(
                &format!("Export · {title}"),
                &export_description(old.map(|n| n.export_settings).unwrap_or_default()),
                &export_description(new.map(|n| n.export_settings).unwrap_or_default()),
                path,
            );
        }
    }
    if document_scope.is_none() {
        if let Some((before, _)) = &before_project {
            add(
                "Project outline and settings",
                &outline(before),
                &outline(&current.project),
                Vec::new(),
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
            Vec::new(),
        );
        add(
            "Project styles (CSS)",
            &resource_text(ports, checkpoint, preview, "styles.css")?,
            &current.styles_css,
            Vec::new(),
        );
    }
    changes.sort_by(|left, right| {
        left.path
            .iter()
            .map(|p| p.order)
            .collect::<Vec<_>>()
            .cmp(&right.path.iter().map(|p| p.order).collect::<Vec<_>>())
            .then(left.document_title.cmp(&right.document_title))
    });
    Ok(changes)
}

fn history_path(
    before: Option<&Project>,
    after: &Project,
    mut id: NodeId,
) -> Vec<crate::project_workspace::HistoryHeading> {
    let mut path = Vec::new();
    loop {
        let old = before.and_then(|p| p.nodes.get(id));
        let new = after.nodes.get(id);
        let Some(node) = new.or(old) else {
            break;
        };
        let owner = if new.is_some() {
            after
        } else {
            before.unwrap()
        };
        path.push(crate::project_workspace::HistoryHeading {
            id: encode_hex(id.as_bytes()),
            before_title: old.map(|n| n.title.clone()),
            title: new.map(|n| n.title.clone()),
            group: node.kind.can_have_children(),
            order: owner
                .nodes
                .parent(id)
                .map(|parent| {
                    owner
                        .nodes
                        .children(parent)
                        .iter()
                        .position(|child| *child == id)
                        .unwrap_or(0)
                })
                .unwrap_or(0),
        });
        let Some(parent) = owner.nodes.parent(id) else {
            break;
        };
        id = parent;
    }
    path.reverse();
    path
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
    let mut output = format!(
        "Title: {}\nAuthor: {}\nExport: {}\n",
        project.display_title,
        project.author.as_deref().unwrap_or_default(),
        export_description(project.export_settings)
    );
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
        for message in &thread.messages {
            output.push_str(&format!(
                "Note on {quote}{}\n",
                if orphaned {
                    " · anchor unavailable"
                } else {
                    ""
                }
            ));
            output.push_str(&message.body);
            output.push('\n');
        }
    }
    output
}
