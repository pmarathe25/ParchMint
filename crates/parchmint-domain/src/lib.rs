#![allow(missing_docs)] // Variant-field prose is documented in the format/API handoff.
//! Qt-free, validated project graph and command layer.
//!
//! Canonical files are implemented by `parchmint-storage`; this crate owns the
//! graph rules which make those files meaningful.

use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::num::NonZeroU64;
use std::path::{Component, Path};
use thiserror::Error;
use uuid::Uuid;

macro_rules! stable_id {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(
            Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Generates a new random stable identifier.
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
            /// Parses a serialized stable identifier.
            pub fn parse(value: &str) -> Result<Self, uuid::Error> {
                value.parse().map(Self)
            }
            /// Returns the identifier as a UUID.
            pub const fn uuid(self) -> Uuid {
                self.0
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

stable_id!(ProjectId, "Stable project identifier.");
stable_id!(NodeId, "Stable binder-node identifier.");
stable_id!(DocumentId, "Stable Markdown-document identifier.");
stable_id!(StyleId, "Stable style-definition identifier.");
stable_id!(AssetId, "Stable attachment identifier.");
stable_id!(CompilePresetId, "Stable compile-preset identifier.");

/// A project-relative path which has been rejected if it could be lexical traversal.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct RelativeProjectPath(String);

impl RelativeProjectPath {
    /// Validates a portable relative path. Storage additionally rejects symlink escape.
    pub fn new(value: impl Into<String>) -> Result<Self, ProjectError> {
        let value = value.into();
        let path = Path::new(&value);
        if value.is_empty()
            || path.is_absolute()
            || path
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(ProjectError::UnsafePath(value));
        }
        Ok(Self(value.replace('\\', "/")))
    }
    /// Returns the portable slash-separated representation.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for RelativeProjectPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Metadata stored in Markdown front matter rather than `outline.toml`.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DocumentMetadata {
    /// User-facing title.
    pub title: String,
    /// Brief outline/card synopsis.
    #[serde(default)]
    pub summary: String,
    /// Optional workflow status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Ordered user labels.
    #[serde(default)]
    pub labels: Vec<String>,
    /// Ordered user tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Document flags with stable machine keys.
    #[serde(default)]
    pub flags: BTreeMap<String, bool>,
}

/// Markdown document associated with one user node.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DocumentRecord {
    /// Stable document identifier.
    pub id: DocumentId,
    /// Owning binder node.
    pub node_id: NodeId,
    /// Canonical location below the project root.
    pub path: RelativeProjectPath,
    /// Front-matter metadata.
    pub metadata: DocumentMetadata,
}

/// A binder node kind. Root strings are intentionally resolved through helpers.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum NodeKind {
    /// The immutable manuscript root.
    ManuscriptRoot,
    /// The immutable research root.
    ResearchRoot,
    /// A user-defined structural group with a Markdown metadata document.
    Group { document_id: DocumentId },
    /// A normal text/research section with a Markdown document.
    Document { document_id: DocumentId },
}

impl NodeKind {
    /// Returns whether this is one of the two required roots.
    pub const fn is_builtin_root(&self) -> bool {
        matches!(self, Self::ManuscriptRoot | Self::ResearchRoot)
    }
    /// Returns the backing document, if any.
    pub const fn document_id(&self) -> Option<DocumentId> {
        match self {
            Self::Group { document_id } | Self::Document { document_id } => Some(*document_id),
            _ => None,
        }
    }
}

/// A node in deterministic child order.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Node {
    /// Stable node identifier.
    pub id: NodeId,
    /// Node category.
    pub kind: NodeKind,
    /// Parent for active nodes; roots have none.
    pub parent: Option<NodeId>,
    /// Ordered active children.
    #[serde(default)]
    pub children: Vec<NodeId>,
}

/// A reusable named character or paragraph style.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StyleDefinition {
    /// Stable style identifier.
    pub id: StyleId,
    /// Immutable machine key for built-ins, optional for user styles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine_key: Option<String>,
    /// User-displayable name.
    pub name: String,
    /// Optional parent style.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub based_on: Option<StyleId>,
    /// Deterministic appearance/settings map.
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
    /// Built-ins cannot be deleted.
    #[serde(default)]
    pub builtin: bool,
}

/// Future-facing compile-preset placeholder with a stable identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompilePreset {
    /// Stable preset identifier.
    pub id: CompilePresetId,
    /// User-facing preset name.
    pub name: String,
    /// Explicit placeholder settings, preserved by storage.
    #[serde(default)]
    pub settings: BTreeMap<String, String>,
}

/// Non-authoritative local workspace reference.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceReference {
    /// Last selected node, if still present.
    pub selected_node: Option<NodeId>,
    /// Optional pane arrangement identifier.
    pub layout: Option<String>,
}

/// Tombstone for a detached subtree. The document files themselves move to `trash/`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TrashTombstone {
    /// Root of the trashed subtree.
    pub node_id: NodeId,
    /// Previous parent.
    pub parent: NodeId,
    /// Previous sibling index.
    pub index: usize,
}

/// Complete canonical in-memory project model.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Project {
    /// Stable project identifier.
    pub id: ProjectId,
    /// Human-facing project name.
    pub name: String,
    /// Required root IDs in deterministic root order.
    pub roots: [NodeId; 2],
    /// Nodes keyed by stable identity.
    pub nodes: BTreeMap<NodeId, Node>,
    /// Documents keyed by stable identity.
    pub documents: BTreeMap<DocumentId, DocumentRecord>,
    /// Styles keyed by stable identity.
    pub styles: BTreeMap<StyleId, StyleDefinition>,
    /// Future compile presets.
    #[serde(default)]
    pub compile_presets: BTreeMap<CompilePresetId, CompilePreset>,
    /// Detached subtree locations.
    #[serde(default)]
    pub trash: BTreeMap<NodeId, TrashTombstone>,
}

impl Project {
    /// Creates an empty project containing exactly the built-in manuscript and research roots/styles.
    pub fn new(name: impl Into<String>) -> Self {
        let manuscript = NodeId::new();
        let research = NodeId::new();
        let nodes = BTreeMap::from([
            (
                manuscript,
                Node {
                    id: manuscript,
                    kind: NodeKind::ManuscriptRoot,
                    parent: None,
                    children: Vec::new(),
                },
            ),
            (
                research,
                Node {
                    id: research,
                    kind: NodeKind::ResearchRoot,
                    parent: None,
                    children: Vec::new(),
                },
            ),
        ]);
        Self {
            id: ProjectId::new(),
            name: name.into(),
            roots: [manuscript, research],
            nodes,
            documents: BTreeMap::new(),
            styles: builtin_styles(),
            compile_presets: BTreeMap::new(),
            trash: BTreeMap::new(),
        }
    }

    /// Returns the immutable manuscript root ID.
    pub const fn manuscript_root(&self) -> NodeId {
        self.roots[0]
    }
    /// Returns the immutable research root ID.
    pub const fn research_root(&self) -> NodeId {
        self.roots[1]
    }
    /// Returns the localizable built-in root machine key.
    pub fn builtin_root_key(&self, id: NodeId) -> Option<&'static str> {
        (id == self.manuscript_root())
            .then_some("manuscript")
            .or_else(|| (id == self.research_root()).then_some("research"))
    }
    /// Returns whether this node is detached because it or an ancestor is in trash.
    pub fn is_trashed(&self, id: NodeId) -> bool {
        let mut current = Some(id);
        let mut seen = BTreeSet::new();
        while let Some(node) = current {
            if !seen.insert(node) || self.trash.contains_key(&node) {
                return true;
            }
            current = self.nodes.get(&node).and_then(|entry| entry.parent);
        }
        false
    }
    /// Applies a validated command and returns an explicit event plus structural inverse.
    pub fn execute(&mut self, command: ProjectCommand) -> Result<CommandOutcome, ProjectError> {
        let mut candidate = self.clone();
        let outcome = candidate.apply(command)?;
        candidate.validate()?;
        *self = candidate;
        Ok(outcome)
    }
    #[allow(clippy::too_many_lines)]
    fn apply(&mut self, command: ProjectCommand) -> Result<CommandOutcome, ProjectError> {
        match command {
            ProjectCommand::Create {
                parent,
                node,
                document,
                index,
            } => {
                self.active_parent(parent)?;
                if self.nodes.contains_key(&node.id)
                    || self.documents.contains_key(&document.id)
                    || document.node_id != node.id
                    || node.parent != Some(parent)
                    || node.kind.document_id() != Some(document.id)
                {
                    return Err(ProjectError::InvalidCommand(
                        "create node/document identities do not agree",
                    ));
                }
                self.insert_child(parent, node.id, index)?;
                self.nodes.insert(node.id, node.clone());
                self.documents.insert(document.id, document);
                Ok(CommandOutcome {
                    events: vec![ProjectEvent::NodeCreated(node.id)],
                    undo: StructuralUndo {
                        inverse: ProjectCommand::Trash { node: node.id },
                    },
                })
            }
            ProjectCommand::Rename { node, title } => {
                let document = self.document_for_node(node)?;
                let old = self
                    .documents
                    .get_mut(&document)
                    .ok_or(ProjectError::MissingDocument(document))?;
                let prior = std::mem::replace(&mut old.metadata.title, title);
                Ok(CommandOutcome {
                    events: vec![ProjectEvent::NodeRenamed(node)],
                    undo: StructuralUndo {
                        inverse: ProjectCommand::Rename { node, title: prior },
                    },
                })
            }
            ProjectCommand::Reorder { node, index } => {
                let parent = self
                    .active_node(node)?
                    .parent
                    .ok_or(ProjectError::RootMutation(node))?;
                let old_index = self.remove_child(parent, node)?;
                self.insert_child(parent, node, index)?;
                Ok(CommandOutcome {
                    events: vec![ProjectEvent::NodeReordered(node)],
                    undo: StructuralUndo {
                        inverse: ProjectCommand::Reorder {
                            node,
                            index: old_index,
                        },
                    },
                })
            }
            ProjectCommand::Reparent {
                node,
                parent,
                index,
            } => {
                if node == parent || self.is_descendant(parent, node)? {
                    return Err(ProjectError::Cycle);
                }
                let old_parent = self
                    .active_node(node)?
                    .parent
                    .ok_or(ProjectError::RootMutation(node))?;
                self.active_parent(parent)?;
                let old_index = self.remove_child(old_parent, node)?;
                self.insert_child(parent, node, index)?;
                self.nodes
                    .get_mut(&node)
                    .ok_or(ProjectError::MissingNode(node))?
                    .parent = Some(parent);
                Ok(CommandOutcome {
                    events: vec![ProjectEvent::NodeReparented { node, parent }],
                    undo: StructuralUndo {
                        inverse: ProjectCommand::Reparent {
                            node,
                            parent: old_parent,
                            index: old_index,
                        },
                    },
                })
            }
            ProjectCommand::Duplicate {
                node,
                parent,
                index,
            } => {
                self.active_node(node)?;
                self.active_parent(parent)?;
                let mut copies = Vec::new();
                self.clone_subtree(node, parent, &mut copies)?;
                let copied_root = copies
                    .first()
                    .map(|(node, _)| node.id)
                    .ok_or(ProjectError::InvalidCommand("empty duplicate"))?;
                for (copy, document) in &copies {
                    self.nodes.insert(copy.id, copy.clone());
                    self.documents.insert(document.id, document.clone());
                }
                self.insert_child(parent, copied_root, index)?;
                Ok(CommandOutcome {
                    events: vec![ProjectEvent::NodeDuplicated {
                        source: node,
                        copy: copied_root,
                    }],
                    undo: StructuralUndo {
                        inverse: ProjectCommand::Trash { node: copied_root },
                    },
                })
            }
            ProjectCommand::Trash { node } => {
                let parent = self
                    .active_node(node)?
                    .parent
                    .ok_or(ProjectError::RootMutation(node))?;
                let index = self.remove_child(parent, node)?;
                self.trash.insert(
                    node,
                    TrashTombstone {
                        node_id: node,
                        parent,
                        index,
                    },
                );
                Ok(CommandOutcome {
                    events: vec![ProjectEvent::NodeTrashed(node)],
                    undo: StructuralUndo {
                        inverse: ProjectCommand::Restore {
                            node,
                            parent,
                            index,
                        },
                    },
                })
            }
            ProjectCommand::Restore {
                node,
                parent,
                index,
            } => {
                let tombstone = self
                    .trash
                    .remove(&node)
                    .ok_or(ProjectError::NotTrashed(node))?;
                self.active_parent(parent)?;
                self.insert_child(parent, node, index)?;
                self.nodes
                    .get_mut(&node)
                    .ok_or(ProjectError::MissingNode(node))?
                    .parent = Some(parent);
                Ok(CommandOutcome {
                    events: vec![ProjectEvent::NodeRestored(node)],
                    undo: StructuralUndo {
                        inverse: ProjectCommand::TrashAt {
                            node,
                            parent: tombstone.parent,
                            index: tombstone.index,
                        },
                    },
                })
            }
            ProjectCommand::TrashAt {
                node,
                parent,
                index,
            } => {
                self.active_parent(parent)?;
                let actual = self.remove_child(parent, node)?;
                self.trash.insert(
                    node,
                    TrashTombstone {
                        node_id: node,
                        parent,
                        index,
                    },
                );
                Ok(CommandOutcome {
                    events: vec![ProjectEvent::NodeTrashed(node)],
                    undo: StructuralUndo {
                        inverse: ProjectCommand::Restore {
                            node,
                            parent,
                            index: actual,
                        },
                    },
                })
            }
            ProjectCommand::EditMetadata { document, metadata } => {
                let record = self
                    .documents
                    .get_mut(&document)
                    .ok_or(ProjectError::MissingDocument(document))?;
                let prior = std::mem::replace(&mut record.metadata, metadata);
                Ok(CommandOutcome {
                    events: vec![ProjectEvent::MetadataEdited(document)],
                    undo: StructuralUndo {
                        inverse: ProjectCommand::EditMetadata {
                            document,
                            metadata: prior,
                        },
                    },
                })
            }
            ProjectCommand::MutateStyle { mutation } => self.mutate_style(mutation),
        }
    }
    fn mutate_style(&mut self, mutation: StyleMutation) -> Result<CommandOutcome, ProjectError> {
        match mutation {
            StyleMutation::Create(style) => {
                if self.styles.contains_key(&style.id) {
                    return Err(ProjectError::DuplicateStyle(style.id));
                }
                let id = style.id;
                self.styles.insert(id, style);
                Ok(CommandOutcome {
                    events: vec![ProjectEvent::StyleMutated(id)],
                    undo: StructuralUndo {
                        inverse: ProjectCommand::MutateStyle {
                            mutation: StyleMutation::Delete(id),
                        },
                    },
                })
            }
            StyleMutation::Update(style) => {
                let old = self
                    .styles
                    .insert(style.id, style.clone())
                    .ok_or(ProjectError::MissingStyle(style.id))?;
                Ok(CommandOutcome {
                    events: vec![ProjectEvent::StyleMutated(style.id)],
                    undo: StructuralUndo {
                        inverse: ProjectCommand::MutateStyle {
                            mutation: StyleMutation::Update(old),
                        },
                    },
                })
            }
            StyleMutation::Delete(id) => {
                let old = self
                    .styles
                    .get(&id)
                    .cloned()
                    .ok_or(ProjectError::MissingStyle(id))?;
                if old.builtin {
                    return Err(ProjectError::BuiltinStyle(id));
                }
                self.styles.remove(&id);
                Ok(CommandOutcome {
                    events: vec![ProjectEvent::StyleMutated(id)],
                    undo: StructuralUndo {
                        inverse: ProjectCommand::MutateStyle {
                            mutation: StyleMutation::Create(old),
                        },
                    },
                })
            }
        }
    }
    fn clone_subtree(
        &self,
        source: NodeId,
        parent: NodeId,
        out: &mut Vec<(Node, DocumentRecord)>,
    ) -> Result<(), ProjectError> {
        let source_node = self.active_node(source)?.clone();
        if source_node.kind.is_builtin_root() {
            return Err(ProjectError::RootMutation(source));
        }
        let new_node = NodeId::new();
        let old_document = self
            .documents
            .get(
                &source_node
                    .kind
                    .document_id()
                    .ok_or(ProjectError::MissingDocumentForNode(source))?,
            )
            .ok_or(ProjectError::MissingDocumentForNode(source))?;
        let new_document = DocumentId::new();
        let folder =
            old_document
                .path
                .as_str()
                .split('/')
                .next()
                .ok_or(ProjectError::InvalidInvariant(
                    "document path has no top-level folder",
                ))?;
        let path = RelativeProjectPath::new(format!("{folder}/{new_node}.md"))?;
        let mut node = Node {
            id: new_node,
            kind: match source_node.kind {
                NodeKind::Group { .. } => NodeKind::Group {
                    document_id: new_document,
                },
                NodeKind::Document { .. } => NodeKind::Document {
                    document_id: new_document,
                },
                _ => unreachable!(),
            },
            parent: Some(parent),
            children: Vec::new(),
        };
        let record = DocumentRecord {
            id: new_document,
            node_id: new_node,
            path,
            metadata: old_document.metadata.clone(),
        };
        let child_ids = source_node.children;
        out.push((node.clone(), record));
        for child in child_ids {
            let before = out.len();
            self.clone_subtree(child, new_node, out)?;
            let child_id = out[before].0.id;
            node.children.push(child_id);
        }
        // The parent copy was stored before recursive work; replace it with final children.
        let position = out
            .iter()
            .position(|(candidate, _)| candidate.id == new_node)
            .ok_or(ProjectError::MissingNode(new_node))?;
        out[position].0 = node;
        Ok(())
    }
    fn active_node(&self, id: NodeId) -> Result<&Node, ProjectError> {
        if self.is_trashed(id) {
            Err(ProjectError::TrashedNode(id))
        } else {
            self.nodes.get(&id).ok_or(ProjectError::MissingNode(id))
        }
    }
    fn active_parent(&self, id: NodeId) -> Result<(), ProjectError> {
        self.active_node(id).map(|_| ())
    }
    fn document_for_node(&self, node: NodeId) -> Result<DocumentId, ProjectError> {
        self.active_node(node)?
            .kind
            .document_id()
            .ok_or(ProjectError::RootMutation(node))
    }
    fn insert_child(
        &mut self,
        parent: NodeId,
        child: NodeId,
        index: usize,
    ) -> Result<(), ProjectError> {
        let parent = self
            .nodes
            .get_mut(&parent)
            .ok_or(ProjectError::MissingNode(parent))?;
        if index > parent.children.len() {
            return Err(ProjectError::SiblingIndex(index));
        }
        parent.children.insert(index, child);
        Ok(())
    }
    fn remove_child(&mut self, parent: NodeId, child: NodeId) -> Result<usize, ProjectError> {
        let parent = self
            .nodes
            .get_mut(&parent)
            .ok_or(ProjectError::MissingNode(parent))?;
        let index = parent
            .children
            .iter()
            .position(|id| *id == child)
            .ok_or(ProjectError::InvalidCommand("node is absent from parent"))?;
        parent.children.remove(index);
        Ok(index)
    }
    fn is_descendant(&self, candidate: NodeId, ancestor: NodeId) -> Result<bool, ProjectError> {
        let mut current = Some(candidate);
        while let Some(id) = current {
            if id == ancestor {
                return Ok(true);
            }
            current = self.active_node(id)?.parent;
        }
        Ok(false)
    }
    /// Checks graph, path, document, and style invariants without changing state.
    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.roots[0] == self.roots[1] {
            return Err(ProjectError::InvalidInvariant("roots must differ"));
        }
        for (root, expected) in [(self.roots[0], true), (self.roots[1], false)] {
            let node = self
                .nodes
                .get(&root)
                .ok_or(ProjectError::MissingNode(root))?;
            if node.parent.is_some()
                || !node.kind.is_builtin_root()
                || (expected && !matches!(node.kind, NodeKind::ManuscriptRoot))
                || (!expected && !matches!(node.kind, NodeKind::ResearchRoot))
            {
                return Err(ProjectError::InvalidInvariant(
                    "required roots are malformed",
                ));
            }
        }
        let mut membership = BTreeSet::new();
        for (id, node) in &self.nodes {
            if node.id != *id {
                return Err(ProjectError::InvalidInvariant(
                    "node map key differs from node id",
                ));
            }
            if self.is_trashed(*id) {
                continue;
            }
            if !node.kind.is_builtin_root() && node.parent.is_none() {
                return Err(ProjectError::InvalidInvariant(
                    "active non-root lacks parent",
                ));
            }
            for child in &node.children {
                if self.is_trashed(*child) || !membership.insert(*child) {
                    return Err(ProjectError::InvalidInvariant(
                        "child has duplicate/trashed membership",
                    ));
                }
                let child_node = self
                    .nodes
                    .get(child)
                    .ok_or(ProjectError::MissingNode(*child))?;
                if child_node.parent != Some(*id) {
                    return Err(ProjectError::InvalidInvariant("child parent disagrees"));
                }
            }
            if let Some(doc) = node.kind.document_id() {
                let record = self
                    .documents
                    .get(&doc)
                    .ok_or(ProjectError::MissingDocument(doc))?;
                if record.node_id != *id {
                    return Err(ProjectError::InvalidInvariant("document owner disagrees"));
                }
                let _ = RelativeProjectPath::new(record.path.as_str())?;
            }
        }
        for root in self.roots {
            if membership.contains(&root) {
                return Err(ProjectError::InvalidInvariant("root appears as child"));
            }
            self.walk_active(root, &mut BTreeSet::new())?;
        }
        for tombstone in self.trash.values() {
            if !self.nodes.contains_key(&tombstone.node_id)
                || !self.nodes.contains_key(&tombstone.parent)
            {
                return Err(ProjectError::InvalidInvariant(
                    "tombstone references missing node",
                ));
            }
        }
        self.validate_styles()
    }
    fn walk_active(
        &self,
        node: NodeId,
        visiting: &mut BTreeSet<NodeId>,
    ) -> Result<(), ProjectError> {
        if !visiting.insert(node) {
            return Err(ProjectError::Cycle);
        }
        for child in &self.active_node(node)?.children {
            self.walk_active(*child, visiting)?;
        }
        visiting.remove(&node);
        Ok(())
    }
    fn validate_styles(&self) -> Result<(), ProjectError> {
        for style in self.styles.values() {
            let mut seen = BTreeSet::new();
            let mut current = Some(style.id);
            while let Some(id) = current {
                if !seen.insert(id) {
                    return Err(ProjectError::StyleCycle);
                }
                current = self
                    .styles
                    .get(&id)
                    .ok_or(ProjectError::MissingStyle(id))?
                    .based_on;
            }
        }
        if self.styles.values().filter(|style| style.builtin).count() < 2 {
            return Err(ProjectError::InvalidInvariant("built-in styles missing"));
        }
        Ok(())
    }
}

fn builtin_styles() -> BTreeMap<StyleId, StyleDefinition> {
    [
        (
            StyleId(Uuid::from_u128(0x018f_0be2_a8ea_7d2d_89ea_45aa_6637_08d4)),
            "body",
            "Body",
        ),
        (
            StyleId(Uuid::from_u128(0x018f_0be2_a8ea_7d2d_89ea_45aa_6637_08d5)),
            "heading",
            "Heading",
        ),
    ]
    .into_iter()
    .map(|(id, key, name)| {
        (
            id,
            StyleDefinition {
                id,
                machine_key: Some(key.into()),
                name: name.into(),
                based_on: None,
                properties: BTreeMap::new(),
                builtin: true,
            },
        )
    })
    .collect()
}

/// A validated mutation accepted by the Rust command layer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ProjectCommand {
    /// Adds a document-backed node at an exact sibling index.
    Create {
        parent: NodeId,
        node: Node,
        document: DocumentRecord,
        index: usize,
    },
    /// Changes a document title (which is persisted in front matter).
    Rename { node: NodeId, title: String },
    /// Changes sibling position.
    Reorder { node: NodeId, index: usize },
    /// Moves a subtree to another active parent.
    Reparent {
        node: NodeId,
        parent: NodeId,
        index: usize,
    },
    /// Deep-copies a document-backed subtree with newly generated stable IDs.
    Duplicate {
        node: NodeId,
        parent: NodeId,
        index: usize,
    },
    /// Detaches a subtree into canonical trash.
    Trash { node: NodeId },
    /// Restores a trashed subtree.
    Restore {
        node: NodeId,
        parent: NodeId,
        index: usize,
    },
    /// Internal lossless inverse for restore.
    TrashAt {
        node: NodeId,
        parent: NodeId,
        index: usize,
    },
    /// Replaces document front-matter metadata.
    EditMetadata {
        document: DocumentId,
        metadata: DocumentMetadata,
    },
    /// Creates, updates, or deletes a style definition.
    MutateStyle { mutation: StyleMutation },
}

/// Style-definition mutation payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum StyleMutation {
    Create(StyleDefinition),
    Update(StyleDefinition),
    Delete(StyleId),
}
/// An undo record deliberately independent of Qt's text undo stack.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StructuralUndo {
    /// Command which reverses the acknowledged operation.
    pub inverse: ProjectCommand,
}
/// Command execution result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommandOutcome {
    /// Events emitted in deterministic order.
    pub events: Vec<ProjectEvent>,
    /// Inverse structural operation.
    pub undo: StructuralUndo,
}
/// Explicit events consumed by application/UI snapshots.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ProjectEvent {
    NodeCreated(NodeId),
    NodeRenamed(NodeId),
    NodeReordered(NodeId),
    NodeReparented { node: NodeId, parent: NodeId },
    NodeDuplicated { source: NodeId, copy: NodeId },
    NodeTrashed(NodeId),
    NodeRestored(NodeId),
    MetadataEdited(DocumentId),
    StyleMutated(StyleId),
}

/// Domain invariant or command failure.
#[derive(Debug, Error)]
pub enum ProjectError {
    /// A persisted path could escape the project lexically.
    #[error("unsafe project-relative path: {0}")]
    UnsafePath(String),
    /// Node ID does not exist.
    #[error("node does not exist: {0}")]
    MissingNode(NodeId),
    /// Document ID does not exist.
    #[error("document does not exist: {0}")]
    MissingDocument(DocumentId),
    /// A node requiring a document has none.
    #[error("node has no associated document: {0}")]
    MissingDocumentForNode(NodeId),
    /// Style ID does not exist.
    #[error("style does not exist: {0}")]
    MissingStyle(StyleId),
    /// A style is already present.
    #[error("style already exists: {0}")]
    DuplicateStyle(StyleId),
    /// Root nodes cannot be moved, renamed, or trashed.
    #[error("built-in root cannot be structurally changed: {0}")]
    RootMutation(NodeId),
    /// A node is detached in project trash.
    #[error("node is in trash: {0}")]
    TrashedNode(NodeId),
    /// Node does not have a tombstone.
    #[error("node is not in trash: {0}")]
    NotTrashed(NodeId),
    /// Sibling insertion index is invalid.
    #[error("invalid sibling index: {0}")]
    SiblingIndex(usize),
    /// A parent relation would create a cycle.
    #[error("node hierarchy contains a cycle")]
    Cycle,
    /// Style inheritance has a cycle.
    #[error("style inheritance contains a cycle")]
    StyleCycle,
    /// A built-in style cannot be deleted.
    #[error("built-in style cannot be deleted: {0}")]
    BuiltinStyle(StyleId),
    /// Command payload is internally inconsistent.
    #[error("invalid command: {0}")]
    InvalidCommand(&'static str),
    /// A persisted model violates a structural contract.
    #[error("project invariant violated: {0}")]
    InvalidInvariant(&'static str),
}

/// Identifies one open-project incarnation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ProjectGeneration(NonZeroU64);
impl ProjectGeneration {
    /// Creates a non-zero generation.
    pub fn new(value: u64) -> Result<Self, RevisionError> {
        NonZeroU64::new(value).map(Self).ok_or(RevisionError::Zero)
    }
    /// Wire value.
    pub fn get(self) -> u64 {
        self.0.get()
    }
}
/// Monotonic resource revision.
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
pub struct Revision(u64);
impl Revision {
    /// Initial revision.
    pub const INITIAL: Self = Self(0);
    /// Creates a revision.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
    /// Wire value.
    pub const fn get(self) -> u64 {
        self.0
    }
    /// Advances without wrapping.
    pub fn next(self) -> Result<Self, RevisionError> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(RevisionError::Overflow)
    }
}
/// Correlates asynchronous work with project/resource state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct WorkStamp {
    /// Open-project incarnation.
    pub generation: ProjectGeneration,
    /// Resource revision.
    pub revision: Revision,
}
impl WorkStamp {
    /// Whether a completion targets this current state.
    pub fn is_current(self, generation: ProjectGeneration, revision: Revision) -> bool {
        self.generation == generation && self.revision == revision
    }
}
impl fmt::Display for WorkStamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.generation.get(), self.revision.get())
    }
}
/// Errors from monotonic identifiers.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum RevisionError {
    /// Zero cannot identify an open project.
    #[error("project generation must be non-zero")]
    Zero,
    /// Counter overflow.
    #[error("revision counter overflowed")]
    Overflow,
}

/// Stable editor-boundary representation; it intentionally contains no Qt types.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EditorSnapshot {
    /// Stable document identifier.
    pub document_id: String,
    /// Revision represented.
    pub revision: Revision,
    /// Blocks in order.
    pub blocks: Vec<EditorBlock>,
}
/// Minimal semantic Stage 01 editor blocks.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum EditorBlock {
    /// Paragraph.
    Paragraph {
        /// Style ID.
        style_id: String,
        /// Text.
        text: String,
    },
    /// Page break.
    PageBreak,
    /// Protected opaque source.
    Opaque {
        /// Raw source.
        source: String,
        /// Display reason.
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    fn node(parent: NodeId, _root: NodeId, title: &str) -> (Node, DocumentRecord) {
        let id = NodeId::new();
        let document_id = DocumentId::new();
        let path = RelativeProjectPath::new(format!("manuscript/{id}.md")).unwrap();
        (
            Node {
                id,
                kind: NodeKind::Document { document_id },
                parent: Some(parent),
                children: vec![],
            },
            DocumentRecord {
                id: document_id,
                node_id: id,
                path,
                metadata: DocumentMetadata {
                    title: title.into(),
                    ..Default::default()
                },
            },
        )
    }
    fn create(project: &mut Project, parent: NodeId, title: &str) -> NodeId {
        let (node, document) = node(parent, project.manuscript_root(), title);
        let id = node.id;
        project
            .execute(ProjectCommand::Create {
                parent,
                node,
                document,
                index: project.nodes[&parent].children.len(),
            })
            .unwrap();
        id
    }
    #[test]
    fn commands_preserve_graph_and_undo() {
        let mut project = Project::new("Novel");
        let root = project.manuscript_root();
        let one = create(&mut project, root, "One");
        let two = create(&mut project, root, "Two");
        project
            .execute(ProjectCommand::Reorder {
                node: two,
                index: 0,
            })
            .unwrap();
        project
            .execute(ProjectCommand::Reparent {
                node: one,
                parent: two,
                index: 0,
            })
            .unwrap();
        assert!(
            project
                .execute(ProjectCommand::Reparent {
                    node: two,
                    parent: one,
                    index: 0
                })
                .is_err()
        );
        let undo = project
            .execute(ProjectCommand::Trash { node: two })
            .unwrap()
            .undo;
        project.execute(undo.inverse).unwrap();
        project.validate().unwrap();
    }
    #[test]
    fn style_cycles_and_path_traversal_are_rejected() {
        assert!(RelativeProjectPath::new("../outside").is_err());
        let mut project = Project::new("Novel");
        let a = StyleDefinition {
            id: StyleId::new(),
            machine_key: None,
            name: "A".into(),
            based_on: None,
            properties: BTreeMap::new(),
            builtin: false,
        };
        let b = StyleDefinition {
            id: StyleId::new(),
            machine_key: None,
            name: "B".into(),
            based_on: Some(a.id),
            properties: BTreeMap::new(),
            builtin: false,
        };
        project
            .execute(ProjectCommand::MutateStyle {
                mutation: StyleMutation::Create(a.clone()),
            })
            .unwrap();
        project
            .execute(ProjectCommand::MutateStyle {
                mutation: StyleMutation::Create(b.clone()),
            })
            .unwrap();
        let mut cycle = a;
        cycle.based_on = Some(b.id);
        assert!(
            project
                .execute(ProjectCommand::MutateStyle {
                    mutation: StyleMutation::Update(cycle)
                })
                .is_err()
        );
    }
    #[test]
    fn generated_structural_sequences_remain_valid() {
        let mut project = Project::new("Stress");
        let root = project.manuscript_root();
        let mut ids = Vec::new();
        for number in 0..100 {
            let id = create(&mut project, root, &number.to_string());
            ids.push(id);
            if number % 3 == 0 {
                let _ = project.execute(ProjectCommand::Reorder { node: id, index: 0 });
            }
            if number % 7 == 0 {
                let undo = project
                    .execute(ProjectCommand::Trash { node: id })
                    .unwrap()
                    .undo;
                project.execute(undo.inverse).unwrap();
            }
            project.validate().unwrap();
        }
    }
}
