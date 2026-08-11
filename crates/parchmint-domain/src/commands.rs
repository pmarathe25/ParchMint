use crate::{
    DeletionTombstone, DocumentId, DomainError, MetadataFieldDefinition, MetadataFieldId, NodeId,
    NodeKind, Project, ProjectExportSettings, ProjectNode, ProjectRevision, Resource, ResourceSet,
    StyleDefinition, StyleId,
};

#[derive(Debug, Clone, PartialEq)]
pub enum ProjectCommand {
    CreateGroup {
        id: NodeId,
        parent: NodeId,
        index: usize,
        title: String,
    },
    CreateDocument {
        id: NodeId,
        document_id: DocumentId,
        parent: NodeId,
        index: usize,
        title: String,
    },
    DeleteNode {
        id: NodeId,
        deleted_at_unix_millis: u64,
        restoring_checkpoint: Option<crate::CheckpointId>,
    },
    RestoreDeleted {
        id: NodeId,
    },
    MoveNode {
        id: NodeId,
        parent: NodeId,
        index: usize,
    },
    RenameNode {
        id: NodeId,
        title: String,
    },
    CopyNode {
        id: NodeId,
        parent: NodeId,
        index: usize,
    },
    SetSynopsis {
        id: NodeId,
        synopsis: String,
    },
    SetNodeExportSettings {
        id: NodeId,
        settings: ProjectExportSettings,
    },
    SetMetadataValue {
        id: NodeId,
        field: MetadataFieldId,
        value: Option<String>,
    },
    UpsertMetadataField {
        definition: MetadataFieldDefinition,
    },
    DeleteMetadataField {
        id: MetadataFieldId,
    },
    MoveMetadataField {
        id: MetadataFieldId,
        index: usize,
    },
    UpsertStyle {
        definition: StyleDefinition,
    },
    DeleteStyle {
        id: StyleId,
    },
    AddDictionaryWord {
        word: String,
    },
    RemoveDictionaryWord {
        word: String,
    },
    SetProjectExportSettings {
        settings: ProjectExportSettings,
    },
    RestoreState(Box<Project>),
}

impl ProjectCommand {
    pub fn create_group(
        id: NodeId,
        parent: NodeId,
        index: usize,
        title: impl Into<String>,
    ) -> Self {
        Self::CreateGroup {
            id,
            parent,
            index,
            title: title.into(),
        }
    }

    pub fn create_document(
        id: NodeId,
        document_id: DocumentId,
        parent: NodeId,
        index: usize,
        title: impl Into<String>,
    ) -> Self {
        Self::CreateDocument {
            id,
            document_id,
            parent,
            index,
            title: title.into(),
        }
    }

    pub const fn delete_node(id: NodeId) -> Self {
        Self::DeleteNode {
            id,
            deleted_at_unix_millis: 0,
            restoring_checkpoint: None,
        }
    }

    pub const fn delete_node_at(id: NodeId, deleted_at_unix_millis: u64) -> Self {
        Self::DeleteNode {
            id,
            deleted_at_unix_millis,
            restoring_checkpoint: None,
        }
    }

    pub const fn delete_node_from_checkpoint(
        id: NodeId,
        deleted_at_unix_millis: u64,
        restoring_checkpoint: crate::CheckpointId,
    ) -> Self {
        Self::DeleteNode {
            id,
            deleted_at_unix_millis,
            restoring_checkpoint: Some(restoring_checkpoint),
        }
    }

    pub const fn restore_deleted(id: NodeId) -> Self {
        Self::RestoreDeleted { id }
    }

    pub const fn move_node(id: NodeId, parent: NodeId, index: usize) -> Self {
        Self::MoveNode { id, parent, index }
    }

    pub fn rename_node(id: NodeId, title: impl Into<String>) -> Self {
        Self::RenameNode {
            id,
            title: title.into(),
        }
    }

    pub const fn copy_node(id: NodeId, parent: NodeId, index: usize) -> Self {
        Self::CopyNode { id, parent, index }
    }

    pub fn set_synopsis(id: NodeId, synopsis: impl Into<String>) -> Self {
        Self::SetSynopsis {
            id,
            synopsis: synopsis.into(),
        }
    }

    pub const fn set_node_export_settings(id: NodeId, settings: ProjectExportSettings) -> Self {
        Self::SetNodeExportSettings { id, settings }
    }

    pub fn set_metadata_value(id: NodeId, field: MetadataFieldId, value: Option<String>) -> Self {
        Self::SetMetadataValue { id, field, value }
    }

    pub const fn upsert_metadata_field(definition: MetadataFieldDefinition) -> Self {
        Self::UpsertMetadataField { definition }
    }

    pub const fn delete_metadata_field(id: MetadataFieldId) -> Self {
        Self::DeleteMetadataField { id }
    }

    pub const fn move_metadata_field(id: MetadataFieldId, index: usize) -> Self {
        Self::MoveMetadataField { id, index }
    }

    pub const fn upsert_style(definition: StyleDefinition) -> Self {
        Self::UpsertStyle { definition }
    }

    pub const fn delete_style(id: StyleId) -> Self {
        Self::DeleteStyle { id }
    }

    pub fn add_dictionary_word(word: impl Into<String>) -> Self {
        Self::AddDictionaryWord { word: word.into() }
    }

    pub fn remove_dictionary_word(word: impl Into<String>) -> Self {
        Self::RemoveDictionaryWord { word: word.into() }
    }

    pub const fn set_project_export_settings(settings: ProjectExportSettings) -> Self {
        Self::SetProjectExportSettings { settings }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppliedProjectCommand {
    pub project: Project,
    pub inverse: ProjectCommand,
    pub changed_resources: ResourceSet,
}

pub fn apply_project_command(
    project: &Project,
    expected: ProjectRevision,
    command: ProjectCommand,
) -> Result<AppliedProjectCommand, DomainError> {
    if expected != project.revision {
        return Err(DomainError::StaleRevision {
            expected,
            actual: project.revision,
        });
    }
    project.validate()?;

    let previous = project.clone();
    let mut draft = project.clone();
    apply_to_draft(&mut draft, command)?;
    draft.validate()?;
    draft.revision = project.revision.next();

    Ok(AppliedProjectCommand {
        changed_resources: changed_resources(project, &draft),
        project: draft,
        inverse: ProjectCommand::RestoreState(Box::new(previous)),
    })
}

fn apply_to_draft(project: &mut Project, command: ProjectCommand) -> Result<(), DomainError> {
    match command {
        ProjectCommand::CreateGroup {
            id,
            parent,
            index,
            title,
        } => {
            ensure_available_node_id(project, id)?;
            project.nodes.try_insert_group(id, parent, index, title)?;
            apply_metadata_defaults(project, id);
        }
        ProjectCommand::CreateDocument {
            id,
            document_id,
            parent,
            index,
            title,
        } => {
            ensure_available_node_id(project, id)?;
            ensure_available_document_id(project, document_id)?;
            project
                .nodes
                .try_insert_document(id, document_id, parent, index, title)?;
            apply_metadata_defaults(project, id);
        }
        ProjectCommand::DeleteNode {
            id,
            deleted_at_unix_millis,
            restoring_checkpoint,
        } => delete_node(project, id, deleted_at_unix_millis, restoring_checkpoint)?,
        ProjectCommand::RestoreDeleted { id } => restore_deleted(project, id)?,
        ProjectCommand::MoveNode { id, parent, index } => {
            project.nodes.move_node(id, parent, index)?;
        }
        ProjectCommand::RenameNode { id, title } => {
            if id.is_fixed_root() {
                return Err(DomainError::InvalidTree {
                    reason: "fixed roots cannot be renamed",
                });
            }
            project
                .nodes
                .get_mut(id)
                .ok_or(DomainError::MissingNode { id })?
                .title = title;
        }
        ProjectCommand::CopyNode { id, parent, index } => {
            copy_document(project, id, parent, index)?;
        }
        ProjectCommand::SetSynopsis { id, synopsis } => {
            let node = editable_node(project, id)?;
            node.synopsis = synopsis;
        }
        ProjectCommand::SetNodeExportSettings { id, settings } => {
            let node = editable_node(project, id)?;
            node.export_settings = settings;
        }
        ProjectCommand::SetMetadataValue { id, field, value } => {
            set_metadata_value(project, id, field, value)?;
        }
        ProjectCommand::UpsertMetadataField { definition } => {
            project.metadata.upsert(definition)?;
        }
        ProjectCommand::DeleteMetadataField { id } => {
            project.metadata.remove(id)?;
            for (_, node) in project.nodes.iter_mut() {
                node.metadata.remove(&id);
            }
        }
        ProjectCommand::MoveMetadataField { id, index } => {
            project.metadata.move_to(id, index)?;
        }
        ProjectCommand::UpsertStyle { definition } => {
            project.styles.upsert(definition)?;
        }
        ProjectCommand::DeleteStyle { id } => {
            project.styles.remove(id)?;
        }
        ProjectCommand::AddDictionaryWord { word } => {
            project.dictionary.insert(word)?;
        }
        ProjectCommand::RemoveDictionaryWord { word } => {
            project.dictionary.remove(&word);
        }
        ProjectCommand::SetProjectExportSettings { settings } => {
            project.export_settings = settings;
        }
        ProjectCommand::RestoreState(snapshot) => {
            if snapshot.id != project.id {
                return Err(DomainError::InvalidInput {
                    field: "inverse project",
                    reason: "project identity does not match",
                });
            }
            *project = *snapshot;
        }
    }
    Ok(())
}

fn editable_node(project: &mut Project, id: NodeId) -> Result<&mut ProjectNode, DomainError> {
    if id.is_fixed_root() {
        return Err(DomainError::InvalidTree {
            reason: "fixed roots cannot be edited",
        });
    }
    project
        .nodes
        .get_mut(id)
        .ok_or(DomainError::MissingNode { id })
}

fn ensure_available_node_id(project: &Project, id: NodeId) -> Result<(), DomainError> {
    let used_by_tombstone = project
        .deleted
        .values()
        .flat_map(|tombstone| &tombstone.subtree)
        .any(|snapshot| snapshot.node.id == id);
    if id.is_fixed_root() || project.nodes.contains(id) || used_by_tombstone {
        Err(DomainError::DuplicateId { field: "node ID" })
    } else {
        Ok(())
    }
}

fn ensure_available_document_id(
    project: &Project,
    document_id: DocumentId,
) -> Result<(), DomainError> {
    let live = project
        .nodes
        .iter()
        .any(|(_, node)| node.kind == NodeKind::Document(document_id));
    let deleted = project
        .deleted
        .values()
        .flat_map(|tombstone| &tombstone.subtree)
        .any(|snapshot| snapshot.node.kind == NodeKind::Document(document_id));
    if live || deleted {
        Err(DomainError::DuplicateId {
            field: "document ID",
        })
    } else {
        Ok(())
    }
}

fn apply_metadata_defaults(project: &mut Project, id: NodeId) {
    let node = project.nodes.get_mut(id).expect("new node exists");
    for definition in project.metadata.iter() {
        if definition.applicability.applies_to(node.kind)
            && let Some(value) = &definition.default_value
        {
            node.metadata.insert(definition.id, value.clone());
        }
    }
}

fn delete_node(
    project: &mut Project,
    id: NodeId,
    deleted_at_unix_millis: u64,
    restoring_checkpoint: Option<crate::CheckpointId>,
) -> Result<(), DomainError> {
    if id.is_fixed_root() {
        return Err(DomainError::InvalidTree {
            reason: "fixed roots cannot be deleted",
        });
    }
    let node = project
        .nodes
        .get(id)
        .ok_or(DomainError::MissingNode { id })?
        .clone();
    let former_parent = project.nodes.parent(id).ok_or(DomainError::InvalidTree {
        reason: "a live non-root node has no parent",
    })?;
    let former_index = project
        .nodes
        .children(former_parent)
        .iter()
        .position(|candidate| *candidate == id)
        .expect("parent contains live child");
    let section = project.nodes.section(id).ok_or(DomainError::InvalidTree {
        reason: "deleted node is not below a fixed root",
    })?;
    let subtree = project.nodes.remove_subtree(id)?;
    project.deleted.insert(
        id,
        DeletionTombstone {
            node_id: id,
            title: node.title,
            kind: node.kind,
            section,
            former_parent,
            former_index,
            deleted_at_unix_millis,
            restoring_checkpoint,
            subtree,
        },
    );
    Ok(())
}

fn restore_deleted(project: &mut Project, id: NodeId) -> Result<(), DomainError> {
    let tombstone = project
        .deleted
        .get(&id)
        .cloned()
        .ok_or(DomainError::MissingItem {
            kind: "deletion tombstone",
        })?;
    let parent = project
        .nodes
        .get(tombstone.former_parent)
        .filter(|node| node.kind.can_have_children())
        .map_or_else(|| tombstone.section.root_id(), |_| tombstone.former_parent);
    let index = if parent == tombstone.former_parent {
        tombstone.former_index
    } else {
        project.nodes.children(parent).len()
    };
    project
        .nodes
        .restore_subtree(&tombstone.subtree, parent, index)?;
    project.deleted.remove(&id);
    Ok(())
}

fn set_metadata_value(
    project: &mut Project,
    id: NodeId,
    field: MetadataFieldId,
    value: Option<String>,
) -> Result<(), DomainError> {
    let definition = project
        .metadata
        .get(field)
        .ok_or(DomainError::MissingItem {
            kind: "metadata field",
        })?
        .clone();
    let node = editable_node(project, id)?;
    if !definition.applicability.applies_to(node.kind) {
        return Err(DomainError::InvalidInput {
            field: "metadata value",
            reason: "field does not apply to this node type",
        });
    }
    if definition.text_kind == crate::MetadataTextKind::SingleLine
        && value
            .as_deref()
            .is_some_and(|candidate| candidate.contains(['\r', '\n']))
    {
        return Err(DomainError::InvalidInput {
            field: "metadata value",
            reason: "single-line fields cannot contain line breaks",
        });
    }
    if let Some(value) = value {
        node.metadata.insert(field, value);
    } else {
        node.metadata.remove(&field);
    }
    Ok(())
}

fn copy_document(
    project: &mut Project,
    id: NodeId,
    parent: NodeId,
    index: usize,
) -> Result<(), DomainError> {
    if id.is_fixed_root() {
        return Err(DomainError::InvalidTree {
            reason: "fixed roots cannot be copied",
        });
    }
    let source = project
        .nodes
        .get(id)
        .ok_or(DomainError::MissingNode { id })?
        .clone();
    let NodeKind::Document(_) = source.kind else {
        return Err(DomainError::InvalidInput {
            field: "copy",
            reason: "group copy is deferred",
        });
    };
    let new_node_id = derive_available_node_id(project, id);
    let new_document_id = derive_available_document_id(project, new_node_id);
    let mut copy = source;
    copy.id = new_node_id;
    copy.kind = NodeKind::Document(new_document_id);
    copy.title.push_str(" Copy");
    project.nodes.insert_node(copy, parent, index)
}

fn derive_available_node_id(project: &Project, source: NodeId) -> NodeId {
    for attempt in 1_u64.. {
        let mut bytes = *source.as_bytes();
        let revision = project.revision.value().wrapping_add(attempt).to_le_bytes();
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte ^= project.id.as_bytes()[15 - index];
            *byte = byte.wrapping_add(revision[index % revision.len()]);
            *byte = byte.rotate_left(((index % 7) + 1) as u32);
        }
        let candidate = NodeId::from_bytes(bytes);
        if ensure_available_node_id(project, candidate).is_ok() {
            return candidate;
        }
    }
    unreachable!("the finite project cannot occupy the complete ID space")
}

fn derive_available_document_id(project: &Project, node_id: NodeId) -> DocumentId {
    for attempt in 1_u8..=u8::MAX {
        let mut bytes = *node_id.as_bytes();
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte ^= 0xa5_u8
                .wrapping_add(attempt)
                .rotate_left((index % 8) as u32);
        }
        let candidate = DocumentId::from_bytes(bytes);
        if ensure_available_document_id(project, candidate).is_ok() {
            return candidate;
        }
    }
    unreachable!("the finite project cannot occupy the complete ID space")
}

fn changed_resources(before: &Project, after: &Project) -> ResourceSet {
    let mut changed = ResourceSet::default();
    if before.styles != after.styles {
        changed.insert(Resource::Manifest);
        changed.insert(Resource::Styles);
    }
    if before.dictionary != after.dictionary {
        changed.insert(Resource::Dictionary);
    }
    if before.nodes != after.nodes
        || before.metadata != after.metadata
        || before.deleted != after.deleted
        || before.display_title != after.display_title
        || before.author != after.author
        || before.export_settings != after.export_settings
    {
        changed.insert(Resource::Manifest);
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MetadataApplicability, MetadataTextKind, ProjectId};

    fn make_project() -> Project {
        Project::new(ProjectId::from_bytes([1; 16]))
    }

    #[test]
    fn fixed_roots_cannot_be_changed_or_copied() {
        let project = make_project();
        let before = project.clone();

        for command in [
            ProjectCommand::delete_node(NodeId::manuscript_root()),
            ProjectCommand::move_node(NodeId::manuscript_root(), NodeId::research_root(), 0),
            ProjectCommand::rename_node(NodeId::manuscript_root(), "Draft"),
            ProjectCommand::copy_node(NodeId::manuscript_root(), NodeId::research_root(), 0),
        ] {
            assert!(matches!(
                apply_project_command(&project, project.revision, command),
                Err(DomainError::InvalidTree { .. })
            ));
            assert_eq!(project, before);
        }
    }

    #[test]
    fn moving_a_group_below_its_descendant_is_rejected() {
        let mut project = make_project();
        let group = NodeId::from_bytes([2; 16]);
        let nested = NodeId::from_bytes([3; 16]);
        project
            .nodes
            .insert_group(group, NodeId::manuscript_root(), 0);
        project.nodes.insert_group(nested, group, 0);

        assert!(matches!(
            apply_project_command(
                &project,
                project.revision,
                ProjectCommand::move_node(group, nested, 0),
            ),
            Err(DomainError::CycleDetected { .. })
        ));
    }

    #[test]
    fn command_inverse_restores_the_prior_project_state() {
        let project = make_project();
        let applied = apply_project_command(
            &project,
            project.revision,
            ProjectCommand::create_group(
                NodeId::from_bytes([2; 16]),
                NodeId::manuscript_root(),
                0,
                "Chapter 1",
            ),
        )
        .unwrap();
        let undone =
            apply_project_command(&applied.project, applied.project.revision, applied.inverse)
                .unwrap();

        assert_eq!(undone.project.nodes, project.nodes);
        assert_eq!(undone.project.styles, project.styles);
        assert_eq!(undone.project.metadata, project.metadata);
        assert_eq!(undone.project.deleted, project.deleted);
    }

    #[test]
    fn stale_revisions_are_rejected_without_mutating_the_project() {
        let project = make_project();
        let result = apply_project_command(
            &project,
            ProjectRevision::from(99),
            ProjectCommand::create_group(
                NodeId::from_bytes([2; 16]),
                NodeId::manuscript_root(),
                0,
                "Chapter 1",
            ),
        );

        assert!(matches!(result, Err(DomainError::StaleRevision { .. })));
        assert_eq!(project, make_project());
    }

    #[test]
    fn deleting_and_undoing_a_group_restores_its_complete_subtree() {
        let mut project = make_project();
        let group = NodeId::from_bytes([2; 16]);
        let document = NodeId::from_bytes([3; 16]);
        project
            .nodes
            .try_insert_group(group, NodeId::manuscript_root(), 0, "Chapter")
            .unwrap();
        project
            .nodes
            .try_insert_document(document, DocumentId::from_bytes([4; 16]), group, 0, "Scene")
            .unwrap();
        let before = project.clone();

        let deleted = apply_project_command(
            &project,
            project.revision,
            ProjectCommand::delete_node_at(group, 123),
        )
        .unwrap();
        let tombstone = &deleted.project.deleted[&group];
        assert_eq!(tombstone.deleted_at_unix_millis, 123);
        assert_eq!(tombstone.subtree.len(), 2);
        assert!(!deleted.project.nodes.contains(document));

        let undone =
            apply_project_command(&deleted.project, deleted.project.revision, deleted.inverse)
                .unwrap();
        assert_eq!(undone.project.nodes, before.nodes);
        assert_eq!(undone.project.deleted, before.deleted);
    }

    #[test]
    fn metadata_defaults_are_copied_and_field_deletion_cleans_values() {
        let project = make_project();
        let field = MetadataFieldId::from_bytes([2; 16]);
        let with_field = apply_project_command(
            &project,
            project.revision,
            ProjectCommand::upsert_metadata_field(MetadataFieldDefinition {
                id: field,
                label: "Status".into(),
                description: None,
                applicability: MetadataApplicability::Documents,
                text_kind: MetadataTextKind::SingleLine,
                default_value: Some("Draft".into()),
                visible_on_cards: true,
            }),
        )
        .unwrap()
        .project;
        let document = NodeId::from_bytes([3; 16]);
        let with_document = apply_project_command(
            &with_field,
            with_field.revision,
            ProjectCommand::create_document(
                document,
                DocumentId::from_bytes([4; 16]),
                NodeId::manuscript_root(),
                0,
                "Scene",
            ),
        )
        .unwrap()
        .project;
        assert_eq!(
            with_document.nodes.get(document).unwrap().metadata[&field],
            "Draft"
        );

        let without_field = apply_project_command(
            &with_document,
            with_document.revision,
            ProjectCommand::delete_metadata_field(field),
        )
        .unwrap()
        .project;
        assert!(without_field.metadata.get(field).is_none());
        assert!(
            !without_field
                .nodes
                .get(document)
                .unwrap()
                .metadata
                .contains_key(&field)
        );
    }
}
