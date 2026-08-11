use std::collections::{BTreeMap, BTreeSet};

use crate::{
    CheckpointId, DocumentId, DomainError, MetadataCatalog, MetadataFieldId, NodeId,
    ProjectDictionary, ProjectId, ProjectRevision, StyleCatalog,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProjectSection {
    Manuscript,
    Research,
}

impl ProjectSection {
    pub const fn root_id(self) -> NodeId {
        match self {
            Self::Manuscript => NodeId::manuscript_root(),
            Self::Research => NodeId::research_root(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Root(ProjectSection),
    Group,
    Document(DocumentId),
}

impl NodeKind {
    pub const fn can_have_children(self) -> bool {
        matches!(self, Self::Root(_) | Self::Group)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ProjectExportSetting {
    #[default]
    Inherit,
    Enabled,
    Disabled,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProjectExportSettings {
    pub excluded: bool,
    pub emit_titles: ProjectExportSetting,
    pub starts_new_page: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectNode {
    pub id: NodeId,
    pub kind: NodeKind,
    pub title: String,
    pub synopsis: String,
    pub metadata: BTreeMap<MetadataFieldId, String>,
    pub export_settings: ProjectExportSettings,
}

impl ProjectNode {
    pub fn group(id: NodeId, title: impl Into<String>) -> Self {
        Self {
            id,
            kind: NodeKind::Group,
            title: title.into(),
            synopsis: String::new(),
            metadata: BTreeMap::new(),
            export_settings: ProjectExportSettings::default(),
        }
    }

    pub fn document(id: NodeId, document_id: DocumentId, title: impl Into<String>) -> Self {
        Self {
            id,
            kind: NodeKind::Document(document_id),
            title: title.into(),
            synopsis: String::new(),
            metadata: BTreeMap::new(),
            export_settings: ProjectExportSettings::default(),
        }
    }

    fn root(section: ProjectSection) -> Self {
        let (id, title) = match section {
            ProjectSection::Manuscript => (NodeId::manuscript_root(), "Manuscript"),
            ProjectSection::Research => (NodeId::research_root(), "Research"),
        };
        Self {
            id,
            kind: NodeKind::Root(section),
            title: title.into(),
            synopsis: String::new(),
            metadata: BTreeMap::new(),
            export_settings: ProjectExportSettings::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderedTree<K, V>
where
    K: Ord,
{
    nodes: BTreeMap<K, V>,
    parents: BTreeMap<K, K>,
    children: BTreeMap<K, Vec<K>>,
}

impl<K, V> Default for OrderedTree<K, V>
where
    K: Ord,
{
    fn default() -> Self {
        Self {
            nodes: BTreeMap::new(),
            parents: BTreeMap::new(),
            children: BTreeMap::new(),
        }
    }
}

impl OrderedTree<NodeId, ProjectNode> {
    pub fn new_project() -> Self {
        let mut tree = Self::default();
        for section in [ProjectSection::Manuscript, ProjectSection::Research] {
            let node = ProjectNode::root(section);
            tree.children.insert(node.id, Vec::new());
            tree.nodes.insert(node.id, node);
        }
        tree
    }

    pub fn get(&self, id: NodeId) -> Option<&ProjectNode> {
        self.nodes.get(&id)
    }

    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut ProjectNode> {
        self.nodes.get_mut(&id)
    }

    pub fn contains(&self, id: NodeId) -> bool {
        self.nodes.contains_key(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&NodeId, &ProjectNode)> {
        self.nodes.iter()
    }

    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = (&NodeId, &mut ProjectNode)> {
        self.nodes.iter_mut()
    }

    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.parents.get(&id).copied()
    }

    pub fn children(&self, id: NodeId) -> &[NodeId] {
        self.children.get(&id).map_or(&[], Vec::as_slice)
    }

    pub fn section(&self, id: NodeId) -> Option<ProjectSection> {
        let mut cursor = id;
        let mut seen = BTreeSet::new();
        loop {
            if !seen.insert(cursor) {
                return None;
            }
            match self.nodes.get(&cursor)?.kind {
                NodeKind::Root(section) => return Some(section),
                NodeKind::Group | NodeKind::Document(_) => {
                    cursor = *self.parents.get(&cursor)?;
                }
            }
        }
    }

    pub fn insert_group(&mut self, id: NodeId, parent: NodeId, index: usize) {
        self.insert_node(ProjectNode::group(id, String::new()), parent, index)
            .expect("insert_group requires a unique ID and a valid parent");
    }

    pub fn try_insert_group(
        &mut self,
        id: NodeId,
        parent: NodeId,
        index: usize,
        title: impl Into<String>,
    ) -> Result<(), DomainError> {
        self.insert_node(ProjectNode::group(id, title), parent, index)
    }

    pub fn try_insert_document(
        &mut self,
        id: NodeId,
        document_id: DocumentId,
        parent: NodeId,
        index: usize,
        title: impl Into<String>,
    ) -> Result<(), DomainError> {
        self.insert_node(ProjectNode::document(id, document_id, title), parent, index)
    }

    pub(crate) fn insert_node(
        &mut self,
        node: ProjectNode,
        parent: NodeId,
        index: usize,
    ) -> Result<(), DomainError> {
        if node.id.is_fixed_root() || self.nodes.contains_key(&node.id) {
            return Err(DomainError::DuplicateId { field: "node ID" });
        }
        let parent_node = self
            .nodes
            .get(&parent)
            .ok_or(DomainError::MissingNode { id: parent })?;
        if !parent_node.kind.can_have_children() {
            return Err(DomainError::InvalidTree {
                reason: "documents cannot contain children",
            });
        }
        let siblings = self
            .children
            .get_mut(&parent)
            .expect("every container has a child list");
        let destination = index.min(siblings.len());
        siblings.insert(destination, node.id);
        self.parents.insert(node.id, parent);
        self.children.insert(node.id, Vec::new());
        self.nodes.insert(node.id, node);
        Ok(())
    }

    pub(crate) fn move_node(
        &mut self,
        id: NodeId,
        new_parent: NodeId,
        index: usize,
    ) -> Result<(), DomainError> {
        if id.is_fixed_root() {
            return Err(DomainError::InvalidTree {
                reason: "fixed roots cannot be moved",
            });
        }
        if !self.nodes.contains_key(&id) {
            return Err(DomainError::MissingNode { id });
        }
        let parent_node = self
            .nodes
            .get(&new_parent)
            .ok_or(DomainError::MissingNode { id: new_parent })?;
        if !parent_node.kind.can_have_children() {
            return Err(DomainError::InvalidTree {
                reason: "documents cannot contain children",
            });
        }
        let mut cursor = Some(new_parent);
        while let Some(ancestor) = cursor {
            if ancestor == id {
                return Err(DomainError::CycleDetected {
                    node: id,
                    parent: new_parent,
                });
            }
            cursor = self.parents.get(&ancestor).copied();
        }

        let old_parent = self.parents[&id];
        let old_siblings = self
            .children
            .get_mut(&old_parent)
            .expect("a non-root node has a parent child list");
        let old_index = old_siblings
            .iter()
            .position(|candidate| *candidate == id)
            .expect("a parent contains its child");
        old_siblings.remove(old_index);

        let siblings = self
            .children
            .get_mut(&new_parent)
            .expect("every container has a child list");
        let destination = index.min(siblings.len());
        siblings.insert(destination, id);
        self.parents.insert(id, new_parent);
        Ok(())
    }

    pub(crate) fn remove_subtree(
        &mut self,
        id: NodeId,
    ) -> Result<Vec<DeletedNodeSnapshot>, DomainError> {
        if id.is_fixed_root() {
            return Err(DomainError::InvalidTree {
                reason: "fixed roots cannot be deleted",
            });
        }
        if !self.nodes.contains_key(&id) {
            return Err(DomainError::MissingNode { id });
        }
        let mut ids = Vec::new();
        self.collect_preorder(id, &mut ids);
        let snapshots = ids
            .iter()
            .map(|node_id| DeletedNodeSnapshot {
                node: self.nodes[node_id].clone(),
                parent: self.parents.get(node_id).copied(),
                children: self.children[node_id].clone(),
            })
            .collect::<Vec<_>>();

        let parent = self.parents[&id];
        self.children
            .get_mut(&parent)
            .expect("deleted node parent exists")
            .retain(|candidate| *candidate != id);
        for node_id in ids {
            self.nodes.remove(&node_id);
            self.parents.remove(&node_id);
            self.children.remove(&node_id);
        }
        Ok(snapshots)
    }

    pub(crate) fn restore_subtree(
        &mut self,
        snapshots: &[DeletedNodeSnapshot],
        parent: NodeId,
        index: usize,
    ) -> Result<(), DomainError> {
        let root = snapshots.first().ok_or(DomainError::InvalidInput {
            field: "deletion tombstone",
            reason: "deleted subtree is empty",
        })?;
        if snapshots
            .iter()
            .any(|snapshot| self.nodes.contains_key(&snapshot.node.id))
        {
            return Err(DomainError::DuplicateId {
                field: "restored node ID",
            });
        }
        let parent_node = self
            .nodes
            .get(&parent)
            .ok_or(DomainError::MissingNode { id: parent })?;
        if !parent_node.kind.can_have_children() {
            return Err(DomainError::InvalidTree {
                reason: "documents cannot contain restored children",
            });
        }

        for snapshot in snapshots {
            self.nodes.insert(snapshot.node.id, snapshot.node.clone());
            self.children
                .insert(snapshot.node.id, snapshot.children.clone());
            if snapshot.node.id != root.node.id
                && let Some(snapshot_parent) = snapshot.parent
            {
                self.parents.insert(snapshot.node.id, snapshot_parent);
            }
        }
        self.parents.insert(root.node.id, parent);
        let siblings = self
            .children
            .get_mut(&parent)
            .expect("restore parent has a child list");
        let destination = index.min(siblings.len());
        siblings.insert(destination, root.node.id);
        Ok(())
    }

    fn collect_preorder(&self, id: NodeId, output: &mut Vec<NodeId>) {
        output.push(id);
        for child in self.children(id) {
            self.collect_preorder(*child, output);
        }
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        self.validate_fixed_root(ProjectSection::Manuscript, "Manuscript")?;
        self.validate_fixed_root(ProjectSection::Research, "Research")?;
        if self
            .nodes
            .values()
            .filter(|node| matches!(node.kind, NodeKind::Root(_)))
            .count()
            != 2
        {
            return Err(DomainError::InvalidTree {
                reason: "the tree must have exactly two fixed roots",
            });
        }

        let mut listed_children = BTreeSet::new();
        for (parent, children) in &self.children {
            let parent_node = self.nodes.get(parent).ok_or(DomainError::InvalidTree {
                reason: "a child list has no matching node",
            })?;
            if !parent_node.kind.can_have_children() && !children.is_empty() {
                return Err(DomainError::InvalidTree {
                    reason: "documents cannot contain children",
                });
            }
            let mut siblings = BTreeSet::new();
            for child in children {
                if !siblings.insert(*child) || !listed_children.insert(*child) {
                    return Err(DomainError::DuplicateId {
                        field: "ordered child list",
                    });
                }
                if !self.nodes.contains_key(child) {
                    return Err(DomainError::InvalidTree {
                        reason: "a child list references a missing node",
                    });
                }
                if self.parents.get(child) != Some(parent) {
                    return Err(DomainError::InvalidTree {
                        reason: "parent and child indexes disagree",
                    });
                }
            }
        }
        for id in self.nodes.keys() {
            if id.is_fixed_root() {
                if self.parents.contains_key(id) || listed_children.contains(id) {
                    return Err(DomainError::InvalidTree {
                        reason: "fixed roots cannot have parents",
                    });
                }
            } else if !self.parents.contains_key(id) || !listed_children.contains(id) {
                return Err(DomainError::InvalidTree {
                    reason: "every non-root node must have exactly one parent",
                });
            }
            if !self.children.contains_key(id) {
                return Err(DomainError::InvalidTree {
                    reason: "every node must have an ordered child list",
                });
            }
        }

        let mut visited = BTreeSet::new();
        let mut active = BTreeSet::new();
        self.visit(NodeId::manuscript_root(), &mut visited, &mut active)?;
        self.visit(NodeId::research_root(), &mut visited, &mut active)?;
        if visited.len() != self.nodes.len() {
            return Err(DomainError::InvalidTree {
                reason: "all nodes must be reachable from one fixed root",
            });
        }

        let mut document_ids = BTreeSet::new();
        for node in self.nodes.values() {
            if let NodeKind::Document(document_id) = node.kind
                && !document_ids.insert(document_id)
            {
                return Err(DomainError::DuplicateId {
                    field: "document ID",
                });
            }
        }
        Ok(())
    }

    fn visit(
        &self,
        id: NodeId,
        visited: &mut BTreeSet<NodeId>,
        active: &mut BTreeSet<NodeId>,
    ) -> Result<(), DomainError> {
        if !active.insert(id) {
            return Err(DomainError::CycleDetected {
                node: id,
                parent: id,
            });
        }
        if !visited.insert(id) {
            return Err(DomainError::DuplicateId {
                field: "tree traversal",
            });
        }
        for child in self.children(id) {
            self.visit(*child, visited, active)?;
        }
        active.remove(&id);
        Ok(())
    }

    fn validate_fixed_root(
        &self,
        section: ProjectSection,
        title: &'static str,
    ) -> Result<(), DomainError> {
        let id = section.root_id();
        let node = self.nodes.get(&id).ok_or(DomainError::InvalidTree {
            reason: "a fixed root is missing",
        })?;
        if node.kind != NodeKind::Root(section) || node.title != title {
            return Err(DomainError::InvalidTree {
                reason: "fixed root identity, kind, and title cannot change",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletedNodeSnapshot {
    pub node: ProjectNode,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletionTombstone {
    pub node_id: NodeId,
    pub title: String,
    pub kind: NodeKind,
    pub section: ProjectSection,
    pub former_parent: NodeId,
    pub former_index: usize,
    pub deleted_at_unix_millis: u64,
    pub restoring_checkpoint: Option<CheckpointId>,
    pub subtree: Vec<DeletedNodeSnapshot>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpellcheckLanguage {
    EnUs,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    pub id: ProjectId,
    pub revision: ProjectRevision,
    pub display_title: String,
    pub author: Option<String>,
    pub spellcheck_language: SpellcheckLanguage,
    pub nodes: OrderedTree<NodeId, ProjectNode>,
    pub styles: StyleCatalog,
    pub metadata: MetadataCatalog,
    pub dictionary: ProjectDictionary,
    pub export_settings: ProjectExportSettings,
    pub deleted: BTreeMap<NodeId, DeletionTombstone>,
}

impl Project {
    pub fn new(id: ProjectId) -> Self {
        Self {
            id,
            revision: ProjectRevision::default(),
            display_title: String::new(),
            author: None,
            spellcheck_language: SpellcheckLanguage::EnUs,
            nodes: OrderedTree::new_project(),
            styles: StyleCatalog::default(),
            metadata: MetadataCatalog::default(),
            dictionary: ProjectDictionary::default(),
            export_settings: ProjectExportSettings::default(),
            deleted: BTreeMap::new(),
        }
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        self.nodes.validate()?;
        self.styles.validate()?;
        for id in self.deleted.keys() {
            if id.is_fixed_root() || self.nodes.contains(*id) {
                return Err(DomainError::InvalidTree {
                    reason: "deleted IDs cannot be fixed roots or live nodes",
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Resource {
    Manifest,
    Styles,
    Dictionary,
    Document(DocumentId),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResourceSet(BTreeSet<Resource>);

impl ResourceSet {
    pub fn insert(&mut self, resource: Resource) -> bool {
        self.0.insert(resource)
    }

    pub fn contains(&self, resource: Resource) -> bool {
        self.0.contains(&resource)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Resource> {
        self.0.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordered_tree_preserves_order_and_rejects_duplicate_children() {
        let mut tree = OrderedTree::new_project();
        let first = NodeId::from_bytes([2; 16]);
        let second = NodeId::from_bytes([3; 16]);
        tree.insert_group(first, NodeId::manuscript_root(), 0);
        tree.insert_group(second, NodeId::manuscript_root(), 1);

        assert_eq!(tree.children(NodeId::manuscript_root()), &[first, second]);

        tree.children
            .get_mut(&NodeId::manuscript_root())
            .expect("fixed roots have child lists")
            .push(first);
        assert!(matches!(
            tree.validate(),
            Err(DomainError::DuplicateId { .. })
        ));
    }
}
