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
        let value = value.into().replace('\\', "/");
        let path = Path::new(&value);
        if value.is_empty()
            || path.is_absolute()
            || path
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(ProjectError::UnsafePath(value));
        }
        Ok(Self(value))
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
    /// Immutable attachment represented by this research document, if any.
    /// The attachment bytes and display metadata live in the project asset
    /// catalog; this is only a stable reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment: Option<AssetId>,
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
    /// Whether the style applies to whole paragraphs or character runs.
    #[serde(default)]
    pub kind: StyleKind,
    /// Style automatically selected for the paragraph created after this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_style: Option<StyleId>,
    /// Deterministic appearance/settings map.
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
    /// Built-ins cannot be deleted.
    #[serde(default)]
    pub builtin: bool,
}

/// Semantic scope of a named style.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StyleKind {
    /// Applies to a whole paragraph/block.
    #[default]
    Paragraph,
    /// Applies to an inline character run.
    Character,
}

/// Fully inherited, Qt-independent style consumed by exporters and editor projections.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComputedStyle {
    pub id: StyleId,
    pub kind: StyleKind,
    pub properties: BTreeMap<String, String>,
    pub next_style: Option<StyleId>,
}

/// Rules controlling which binder documents a compile preset can include.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchInclusion {
    /// Research is excluded unless a research node/root is explicitly selected.
    #[default]
    SelectedRoots,
    /// Research is never included, even when a broad selection would include it.
    Exclude,
    /// Include the complete research root after manuscript content.
    All,
}

/// Inclusion behavior stored with a compile preset.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompileInclusionRules {
    /// Research selection policy. The default keeps research out of normal manuscript exports.
    #[serde(default)]
    pub research: ResearchInclusion,
    /// Honor an explicit `include-in-compile = false` document flag.
    #[serde(default = "default_true")]
    pub respect_include_flag: bool,
    /// Whether documents with no body blocks still receive their semantic title.
    #[serde(default = "default_true")]
    pub include_empty_documents: bool,
}

impl Default for CompileInclusionRules {
    fn default() -> Self {
        Self {
            research: ResearchInclusion::SelectedRoots,
            respect_include_flag: true,
            include_empty_documents: true,
        }
    }
}

/// Whether a generated project title is inserted before compiled content.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectTitleBehavior {
    /// Do not synthesize a project title.
    None,
    /// Insert the preset metadata title (or project name) as a semantic title block.
    #[default]
    Heading,
}

/// Whether each compiled source document receives its metadata title.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentTitleBehavior {
    /// Do not synthesize per-document titles.
    None,
    /// Insert each non-empty document title as a heading.
    #[default]
    Heading,
}

/// Semantic title insertion settings.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompileTitleBehavior {
    #[serde(default)]
    pub project_title: ProjectTitleBehavior,
    #[serde(default)]
    pub document_titles: DocumentTitleBehavior,
    /// Heading level used for generated document titles (1 through 6).
    #[serde(default = "default_document_heading_level")]
    pub document_heading_level: u8,
}

impl Default for CompileTitleBehavior {
    fn default() -> Self {
        Self {
            project_title: ProjectTitleBehavior::Heading,
            document_titles: DocumentTitleBehavior::Heading,
            document_heading_level: default_document_heading_level(),
        }
    }
}

/// A semantic boundary inserted between compiled source documents.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompileSeparator {
    /// Do not insert a boundary.
    None,
    /// Insert a scene/thematic break.
    #[default]
    SceneBreak,
    /// Insert a semantic page break.
    PageBreak,
}

/// Project metadata embedded in formats that support it.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompileMetadata {
    /// Optional export title. An empty value falls back to the project name.
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub keywords: Vec<String>,
}

/// Format-neutral override applied after Rust resolves a named style's inheritance.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompileStyleMapping {
    /// Optional stable CSS/OOXML class name. Exporters sanitize it before emitting it.
    #[serde(default)]
    pub class_name: String,
    /// Per-preset style properties which override the computed project style.
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
}

/// Physical page settings consumed by PDF and DOCX export plans.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PageSettings {
    /// Human-readable paper label, e.g. `A4` or `Letter`.
    #[serde(default = "default_page_name")]
    pub paper: String,
    /// Paper width in whole micrometres; avoiding floats keeps presets deterministic.
    #[serde(default = "default_page_width_micrometres")]
    pub width_micrometres: u32,
    /// Paper height in whole micrometres.
    #[serde(default = "default_page_height_micrometres")]
    pub height_micrometres: u32,
    #[serde(default = "default_margin_micrometres")]
    pub margin_top_micrometres: u32,
    #[serde(default = "default_margin_micrometres")]
    pub margin_right_micrometres: u32,
    #[serde(default = "default_margin_micrometres")]
    pub margin_bottom_micrometres: u32,
    #[serde(default = "default_margin_micrometres")]
    pub margin_left_micrometres: u32,
    #[serde(default)]
    pub header: String,
    #[serde(default)]
    pub footer: String,
}

impl Default for PageSettings {
    fn default() -> Self {
        Self {
            paper: default_page_name(),
            width_micrometres: default_page_width_micrometres(),
            height_micrometres: default_page_height_micrometres(),
            margin_top_micrometres: default_margin_micrometres(),
            margin_right_micrometres: default_margin_micrometres(),
            margin_bottom_micrometres: default_margin_micrometres(),
            margin_left_micrometres: default_margin_micrometres(),
            header: String::new(),
            footer: String::new(),
        }
    }
}

/// Persisted compile/export recipe. All IDs are stable and all collections have
/// deterministic TOML ordering through their `BTreeMap` representation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompilePreset {
    /// Stable preset identifier.
    pub id: CompilePresetId,
    /// User-facing preset name.
    pub name: String,
    /// Roots selected for compile. An empty vector means the manuscript root.
    #[serde(default)]
    pub selected_roots: Vec<NodeId>,
    #[serde(default)]
    pub inclusion: CompileInclusionRules,
    #[serde(default)]
    pub titles: CompileTitleBehavior,
    #[serde(default)]
    pub separator: CompileSeparator,
    #[serde(default)]
    pub metadata: CompileMetadata,
    /// Export-only property overrides keyed by immutable style identity.
    #[serde(default)]
    pub style_mapping: BTreeMap<StyleId, CompileStyleMapping>,
    #[serde(default)]
    pub page: PageSettings,
    /// Namespaced exporter options. Unknown keys remain readable and are never
    /// interpreted as executable commands.
    #[serde(default)]
    pub exporter_settings: BTreeMap<String, BTreeMap<String, String>>,
    /// Deprecated Stage 02 placeholder settings, retained for opening early
    /// project-format-1 files without losing user data.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub settings: BTreeMap<String, String>,
}

impl CompilePreset {
    /// Creates the deterministic default manuscript preset.
    pub fn manuscript(name: impl Into<String>) -> Self {
        Self {
            id: CompilePresetId::new(),
            name: name.into(),
            selected_roots: Vec::new(),
            inclusion: CompileInclusionRules::default(),
            titles: CompileTitleBehavior::default(),
            separator: CompileSeparator::SceneBreak,
            metadata: CompileMetadata::default(),
            style_mapping: BTreeMap::new(),
            page: PageSettings::default(),
            exporter_settings: BTreeMap::new(),
            settings: BTreeMap::new(),
        }
    }
}

const fn default_true() -> bool {
    true
}
const fn default_document_heading_level() -> u8 {
    1
}
fn default_page_name() -> String {
    "A4".into()
}
const fn default_page_width_micrometres() -> u32 {
    210_000
}
const fn default_page_height_micrometres() -> u32 {
    297_000
}
const fn default_margin_micrometres() -> u32 {
    25_400
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
    /// Adds built-in styles introduced by this format version without replacing
    /// overrides of an existing built-in stable identity.
    pub fn ensure_required_builtin_styles(&mut self) {
        for (id, style) in builtin_styles() {
            self.styles.entry(id).or_insert(style);
        }
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
        self.validate_command(&command)?;
        let outcome = self.apply(command)?;
        if let Err(error) = self.validate_events(&outcome.events) {
            // Command-local rollback is intentionally exact.  The user-facing
            // undo for create/duplicate is "move to trash", while a failed
            // transaction must remove the newly allocated records entirely.
            self.rollback(&outcome)?;
            return Err(error);
        }
        Ok(outcome)
    }

    /// Reverts an unacknowledged command without creating a user-visible trash
    /// entry. Persistence uses this when a canonical transaction cannot commit.
    pub fn rollback(&mut self, outcome: &CommandOutcome) -> Result<(), ProjectError> {
        match outcome.events.as_slice() {
            [ProjectEvent::NodeCreated(node)] => self.remove_unacknowledged_subtree(*node),
            [ProjectEvent::NodeDuplicated { copy, .. }] => {
                self.remove_unacknowledged_subtree(*copy)
            }
            _ => self.apply(outcome.undo.inverse.clone()).map(|_| ()),
        }
    }

    /// Resolves inherited properties without Qt or mutable display names.
    pub fn computed_style(&self, id: StyleId) -> Result<ComputedStyle, ProjectError> {
        let leaf = self.styles.get(&id).ok_or(ProjectError::MissingStyle(id))?;
        let mut chain = Vec::new();
        let mut current = Some(id);
        let mut seen = BTreeSet::new();
        while let Some(style_id) = current {
            if !seen.insert(style_id) {
                return Err(ProjectError::StyleCycle);
            }
            let style = self
                .styles
                .get(&style_id)
                .ok_or(ProjectError::MissingStyle(style_id))?;
            if style.kind != leaf.kind {
                return Err(ProjectError::StyleKindMismatch);
            }
            chain.push(style);
            current = style.based_on;
        }
        let mut properties = BTreeMap::new();
        for style in chain.into_iter().rev() {
            properties.extend(style.properties.clone());
        }
        Ok(ComputedStyle {
            id,
            kind: leaf.kind,
            properties,
            next_style: leaf.next_style,
        })
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
                self.trash
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
                            parent,
                            index,
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
            ProjectCommand::UpsertCompilePreset { preset } => {
                let id = preset.id;
                let previous = self.compile_presets.insert(id, preset);
                Ok(CommandOutcome {
                    events: vec![ProjectEvent::CompilePresetSaved(id)],
                    undo: StructuralUndo {
                        inverse: previous
                            .map_or(ProjectCommand::RemoveCompilePreset { id }, |preset| {
                                ProjectCommand::UpsertCompilePreset { preset }
                            }),
                    },
                })
            }
            ProjectCommand::RemoveCompilePreset { id } => {
                let preset = self
                    .compile_presets
                    .remove(&id)
                    .ok_or(ProjectError::MissingCompilePreset(id))?;
                Ok(CommandOutcome {
                    events: vec![ProjectEvent::CompilePresetRemoved(id)],
                    undo: StructuralUndo {
                        inverse: ProjectCommand::UpsertCompilePreset { preset },
                    },
                })
            }
        }
    }
    #[allow(clippy::too_many_lines)]
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
                if !self.styles.contains_key(&style.id) {
                    return Err(ProjectError::MissingStyle(style.id));
                }
                let old = self
                    .styles
                    .insert(style.id, style.clone())
                    .expect("style existence was checked before mutation");
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
                if self.styles.values().any(|style| {
                    style.id != id && (style.based_on == Some(id) || style.next_style == Some(id))
                }) {
                    return Err(ProjectError::StyleInUse(id));
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
            StyleMutation::DeleteAndReplace { id, replacement } => {
                let old = self
                    .styles
                    .get(&id)
                    .cloned()
                    .ok_or(ProjectError::MissingStyle(id))?;
                if old.builtin {
                    return Err(ProjectError::BuiltinStyle(id));
                }
                let replacement_style = self
                    .styles
                    .get(&replacement)
                    .ok_or(ProjectError::MissingStyle(replacement))?;
                if old.kind != replacement_style.kind || id == replacement {
                    return Err(ProjectError::StyleKindMismatch);
                }
                let affected = self
                    .styles
                    .values()
                    .filter(|style| {
                        style.id != id
                            && (style.based_on == Some(id) || style.next_style == Some(id))
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                for style in self.styles.values_mut() {
                    if style.based_on == Some(id) {
                        style.based_on = Some(replacement);
                    }
                    if style.next_style == Some(id) {
                        style.next_style = Some(replacement);
                    }
                }
                self.styles.remove(&id);
                Ok(CommandOutcome {
                    events: vec![ProjectEvent::StyleReplaced { id, replacement }],
                    undo: StructuralUndo {
                        inverse: ProjectCommand::MutateStyle {
                            mutation: StyleMutation::RestoreDeleted {
                                style: old,
                                affected,
                                replacement,
                            },
                        },
                    },
                })
            }
            StyleMutation::RestoreDeleted {
                style,
                affected,
                replacement,
            } => {
                let id = style.id;
                if self.styles.contains_key(&id) {
                    return Err(ProjectError::DuplicateStyle(id));
                }
                self.styles.insert(id, style);
                for prior in affected {
                    if self.styles.contains_key(&prior.id) {
                        self.styles.insert(prior.id, prior);
                    }
                }
                Ok(CommandOutcome {
                    events: vec![ProjectEvent::StyleMutated(id)],
                    undo: StructuralUndo {
                        inverse: ProjectCommand::MutateStyle {
                            mutation: StyleMutation::DeleteAndReplace { id, replacement },
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
        let source_node = self.active_node(source)?;
        if source_node.kind.is_builtin_root() {
            return Err(ProjectError::RootMutation(source));
        }
        // `(source, copied parent, copied-parent output index)`. Children are
        // pushed in reverse so output remains canonical preorder without using
        // user-controlled depth on the Rust stack.
        let mut pending: Vec<(NodeId, NodeId, Option<usize>)> = vec![(source, parent, None)];
        while let Some((source_id, copied_parent, parent_output)) = pending.pop() {
            let source_node = self
                .nodes
                .get(&source_id)
                .ok_or(ProjectError::MissingNode(source_id))?;
            let old_document = self
                .documents
                .get(
                    &source_node
                        .kind
                        .document_id()
                        .ok_or(ProjectError::MissingDocumentForNode(source_id))?,
                )
                .ok_or(ProjectError::MissingDocumentForNode(source_id))?;
            let new_node = NodeId::new();
            let new_document = DocumentId::new();
            let folder = old_document.path.as_str().split('/').next().ok_or(
                ProjectError::InvalidInvariant("document path has no top-level folder"),
            )?;
            let path = RelativeProjectPath::new(format!("{folder}/{new_node}.md"))?;
            let node = Node {
                id: new_node,
                kind: match source_node.kind {
                    NodeKind::Group { .. } => NodeKind::Group {
                        document_id: new_document,
                    },
                    NodeKind::Document { .. } => NodeKind::Document {
                        document_id: new_document,
                    },
                    _ => return Err(ProjectError::RootMutation(source_id)),
                },
                parent: Some(copied_parent),
                children: Vec::new(),
            };
            let record = DocumentRecord {
                id: new_document,
                node_id: new_node,
                path,
                metadata: old_document.metadata.clone(),
            };
            let output_index = out.len();
            out.push((node, record));
            if let Some(parent_output) = parent_output {
                out[parent_output].0.children.push(new_node);
            }
            pending.extend(
                source_node
                    .children
                    .iter()
                    .rev()
                    .map(|child| (*child, new_node, Some(output_index))),
            );
        }
        Ok(())
    }
    #[allow(clippy::too_many_lines)]
    fn validate_command(&self, command: &ProjectCommand) -> Result<(), ProjectError> {
        match command {
            ProjectCommand::Create {
                parent,
                node,
                document,
                index,
            } => {
                self.active_parent(*parent)?;
                if *index > self.nodes[parent].children.len() {
                    return Err(ProjectError::SiblingIndex(*index));
                }
                if self.nodes.contains_key(&node.id)
                    || self.documents.contains_key(&document.id)
                    || document.node_id != node.id
                    || node.parent != Some(*parent)
                    || node.kind.document_id() != Some(document.id)
                    || node.kind.is_builtin_root()
                    || !node.children.is_empty()
                {
                    return Err(ProjectError::InvalidCommand(
                        "create node/document identities do not agree",
                    ));
                }
                let _ = RelativeProjectPath::new(document.path.as_str())?;
            }
            ProjectCommand::Rename { node, .. } => {
                self.document_for_node(*node)?;
            }
            ProjectCommand::Reorder { node, index } => {
                let parent = self
                    .active_node(*node)?
                    .parent
                    .ok_or(ProjectError::RootMutation(*node))?;
                let siblings = &self.nodes[&parent].children;
                if !siblings.contains(node) {
                    return Err(ProjectError::InvalidCommand("node is absent from parent"));
                }
                if *index >= siblings.len() {
                    return Err(ProjectError::SiblingIndex(*index));
                }
            }
            ProjectCommand::Reparent {
                node,
                parent,
                index,
            } => {
                if node == parent || self.is_descendant(*parent, *node)? {
                    return Err(ProjectError::Cycle);
                }
                let old_parent = self
                    .active_node(*node)?
                    .parent
                    .ok_or(ProjectError::RootMutation(*node))?;
                self.active_parent(*parent)?;
                let available =
                    self.nodes[parent].children.len() - usize::from(old_parent == *parent);
                if *index > available {
                    return Err(ProjectError::SiblingIndex(*index));
                }
            }
            ProjectCommand::Duplicate {
                node,
                parent,
                index,
            } => {
                let source = self.active_node(*node)?;
                if source.kind.is_builtin_root() {
                    return Err(ProjectError::RootMutation(*node));
                }
                self.active_parent(*parent)?;
                if *index > self.nodes[parent].children.len() {
                    return Err(ProjectError::SiblingIndex(*index));
                }
            }
            ProjectCommand::Trash { node } => {
                if self.active_node(*node)?.parent.is_none() {
                    return Err(ProjectError::RootMutation(*node));
                }
            }
            ProjectCommand::Restore {
                node,
                parent,
                index,
            } => {
                if !self.trash.contains_key(node) {
                    return Err(ProjectError::NotTrashed(*node));
                }
                self.active_parent(*parent)?;
                if !self.nodes.contains_key(node) {
                    return Err(ProjectError::MissingNode(*node));
                }
                if *index > self.nodes[parent].children.len() {
                    return Err(ProjectError::SiblingIndex(*index));
                }
            }
            ProjectCommand::TrashAt { node, parent, .. } => {
                self.active_parent(*parent)?;
                if !self.nodes[parent].children.contains(node) {
                    return Err(ProjectError::InvalidCommand("node is absent from parent"));
                }
            }
            ProjectCommand::EditMetadata { document, .. } => {
                if !self.documents.contains_key(document) {
                    return Err(ProjectError::MissingDocument(*document));
                }
            }
            ProjectCommand::MutateStyle { mutation } => match mutation {
                StyleMutation::Create(style) | StyleMutation::RestoreDeleted { style, .. } => {
                    if self.styles.contains_key(&style.id) {
                        return Err(ProjectError::DuplicateStyle(style.id));
                    }
                }
                StyleMutation::Update(style) => {
                    if !self.styles.contains_key(&style.id) {
                        return Err(ProjectError::MissingStyle(style.id));
                    }
                }
                StyleMutation::Delete(id) | StyleMutation::DeleteAndReplace { id, .. } => {
                    if !self.styles.contains_key(id) {
                        return Err(ProjectError::MissingStyle(*id));
                    }
                }
            },
            ProjectCommand::UpsertCompilePreset { .. } => {}
            ProjectCommand::RemoveCompilePreset { id } => {
                if !self.compile_presets.contains_key(id) {
                    return Err(ProjectError::MissingCompilePreset(*id));
                }
            }
        }
        Ok(())
    }
    fn validate_events(&self, events: &[ProjectEvent]) -> Result<(), ProjectError> {
        for event in events {
            match *event {
                ProjectEvent::NodeCreated(node)
                | ProjectEvent::NodeRenamed(node)
                | ProjectEvent::NodeReordered(node)
                | ProjectEvent::NodeReparented { node, .. }
                | ProjectEvent::NodeRestored(node) => self.validate_node_record(node)?,
                ProjectEvent::NodeDuplicated { copy, .. } => self.validate_subtree(copy)?,
                ProjectEvent::NodeTrashed(node) => {
                    let tombstone = self
                        .trash
                        .get(&node)
                        .ok_or(ProjectError::NotTrashed(node))?;
                    if self.nodes[&tombstone.parent].children.contains(&node) {
                        return Err(ProjectError::InvalidInvariant(
                            "trashed root remains in active parent",
                        ));
                    }
                }
                ProjectEvent::MetadataEdited(document) => {
                    let record = self
                        .documents
                        .get(&document)
                        .ok_or(ProjectError::MissingDocument(document))?;
                    if self
                        .nodes
                        .get(&record.node_id)
                        .and_then(|node| node.kind.document_id())
                        != Some(document)
                    {
                        return Err(ProjectError::InvalidInvariant("document owner disagrees"));
                    }
                    let _ = RelativeProjectPath::new(record.path.as_str())?;
                }
                ProjectEvent::StyleMutated(_) | ProjectEvent::StyleReplaced { .. } => {
                    self.validate_styles()?;
                }
                ProjectEvent::CompilePresetSaved(_) | ProjectEvent::CompilePresetRemoved(_) => {
                    self.validate_compile_presets()?;
                }
            }
        }
        Ok(())
    }
    fn validate_node_record(&self, id: NodeId) -> Result<(), ProjectError> {
        let node = self.nodes.get(&id).ok_or(ProjectError::MissingNode(id))?;
        if node.id != id {
            return Err(ProjectError::InvalidInvariant(
                "node map key differs from node id",
            ));
        }
        if let Some(parent) = node.parent {
            let parent = self
                .nodes
                .get(&parent)
                .ok_or(ProjectError::MissingNode(parent))?;
            if !self.trash.contains_key(&id) && !parent.children.contains(&id) {
                return Err(ProjectError::InvalidInvariant(
                    "node is absent from its active parent",
                ));
            }
        }
        if let Some(document) = node.kind.document_id() {
            let record = self
                .documents
                .get(&document)
                .ok_or(ProjectError::MissingDocument(document))?;
            if record.node_id != id {
                return Err(ProjectError::InvalidInvariant("document owner disagrees"));
            }
            let _ = RelativeProjectPath::new(record.path.as_str())?;
        }
        Ok(())
    }
    fn validate_subtree(&self, root: NodeId) -> Result<(), ProjectError> {
        let mut pending = vec![root];
        let mut seen = BTreeSet::new();
        while let Some(id) = pending.pop() {
            if !seen.insert(id) {
                return Err(ProjectError::Cycle);
            }
            self.validate_node_record(id)?;
            let node = self.nodes.get(&id).ok_or(ProjectError::MissingNode(id))?;
            for child in &node.children {
                let child_node = self
                    .nodes
                    .get(child)
                    .ok_or(ProjectError::MissingNode(*child))?;
                if child_node.parent != Some(id) {
                    return Err(ProjectError::InvalidInvariant("child parent disagrees"));
                }
                pending.push(*child);
            }
        }
        // A parent-chain walk is bounded by user depth but never consumes the
        // call stack. It catches a reparent cycle without scanning the project.
        let mut ancestry = BTreeSet::new();
        let mut current = Some(root);
        while let Some(id) = current {
            if !ancestry.insert(id) {
                return Err(ProjectError::Cycle);
            }
            current = self.nodes.get(&id).and_then(|node| node.parent);
        }
        Ok(())
    }
    fn remove_unacknowledged_subtree(&mut self, root: NodeId) -> Result<(), ProjectError> {
        let parent = self
            .nodes
            .get(&root)
            .ok_or(ProjectError::MissingNode(root))?
            .parent
            .ok_or(ProjectError::RootMutation(root))?;
        self.remove_child(parent, root)?;
        let mut pending = vec![root];
        while let Some(id) = pending.pop() {
            let node = self
                .nodes
                .remove(&id)
                .ok_or(ProjectError::MissingNode(id))?;
            pending.extend(node.children);
            if let Some(document) = node.kind.document_id() {
                self.documents.remove(&document);
            }
            self.trash.remove(&id);
        }
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
    #[allow(clippy::too_many_lines)]
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
        let mut trashed = BTreeSet::new();
        for (id, tombstone) in &self.trash {
            if tombstone.node_id != *id
                || !self.nodes.contains_key(id)
                || !self.nodes.contains_key(&tombstone.parent)
            {
                return Err(ProjectError::InvalidInvariant(
                    "tombstone references missing node",
                ));
            }
            if self.nodes[&tombstone.parent].children.contains(id) {
                return Err(ProjectError::InvalidInvariant(
                    "trashed root remains in active parent",
                ));
            }
            let mut pending = vec![*id];
            while let Some(node_id) = pending.pop() {
                if !trashed.insert(node_id) {
                    return Err(ProjectError::InvalidInvariant(
                        "trashed subtrees overlap or contain a cycle",
                    ));
                }
                let node = self
                    .nodes
                    .get(&node_id)
                    .ok_or(ProjectError::MissingNode(node_id))?;
                for child in &node.children {
                    let child_node = self
                        .nodes
                        .get(child)
                        .ok_or(ProjectError::MissingNode(*child))?;
                    if child_node.parent != Some(node_id) {
                        return Err(ProjectError::InvalidInvariant("child parent disagrees"));
                    }
                    pending.push(*child);
                }
            }
        }
        if self.roots.iter().any(|root| trashed.contains(root)) {
            return Err(ProjectError::InvalidInvariant("required root is trashed"));
        }
        let mut referenced_documents = BTreeSet::new();
        for (id, node) in &self.nodes {
            if node.id != *id {
                return Err(ProjectError::InvalidInvariant(
                    "node map key differs from node id",
                ));
            }
            if !node.kind.is_builtin_root() && node.parent.is_none() {
                return Err(ProjectError::InvalidInvariant("non-root lacks parent"));
            }
            if let Some(doc) = node.kind.document_id() {
                if !referenced_documents.insert(doc) {
                    return Err(ProjectError::InvalidInvariant(
                        "document is referenced by more than one node",
                    ));
                }
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
        if referenced_documents.len() != self.documents.len() {
            return Err(ProjectError::InvalidInvariant(
                "document record is not referenced exactly once",
            ));
        }
        let mut active = BTreeSet::new();
        let mut pending = self.roots.to_vec();
        while let Some(id) = pending.pop() {
            if trashed.contains(&id) || !active.insert(id) {
                return Err(ProjectError::Cycle);
            }
            let node = self.nodes.get(&id).ok_or(ProjectError::MissingNode(id))?;
            for child in &node.children {
                if trashed.contains(child) {
                    return Err(ProjectError::InvalidInvariant(
                        "active parent contains trashed child",
                    ));
                }
                let child_node = self
                    .nodes
                    .get(child)
                    .ok_or(ProjectError::MissingNode(*child))?;
                if child_node.parent != Some(id) {
                    return Err(ProjectError::InvalidInvariant("child parent disagrees"));
                }
                pending.push(*child);
            }
        }
        if active.len().saturating_add(trashed.len()) != self.nodes.len() {
            return Err(ProjectError::InvalidInvariant(
                "node is unreachable from active or trash roots",
            ));
        }
        self.validate_styles()?;
        self.validate_compile_presets()
    }
    fn validate_styles(&self) -> Result<(), ProjectError> {
        let mut resolved = BTreeSet::new();
        for style in self.styles.values() {
            if style.name.trim().is_empty() || style.name.chars().count() > 128 {
                return Err(ProjectError::InvalidStyle(
                    "style name must contain 1–128 characters",
                ));
            }
            validate_style_properties(&style.properties)?;
            let mut path = Vec::new();
            let mut seen = BTreeSet::new();
            let mut current = Some(style.id);
            while let Some(id) = current {
                if resolved.contains(&id) {
                    break;
                }
                if !seen.insert(id) {
                    return Err(ProjectError::StyleCycle);
                }
                let parent = self.styles.get(&id).ok_or(ProjectError::MissingStyle(id))?;
                if parent.kind != style.kind {
                    return Err(ProjectError::StyleKindMismatch);
                }
                path.push(id);
                current = parent.based_on;
            }
            resolved.extend(path);
            if let Some(next) = style.next_style {
                let next = self
                    .styles
                    .get(&next)
                    .ok_or(ProjectError::MissingStyle(next))?;
                if style.kind != StyleKind::Paragraph || next.kind != StyleKind::Paragraph {
                    return Err(ProjectError::StyleKindMismatch);
                }
            }
        }
        let required = ["body", "heading", "emphasis"];
        if required.iter().any(|key| {
            !self
                .styles
                .values()
                .any(|style| style.builtin && style.machine_key.as_deref() == Some(*key))
        }) {
            return Err(ProjectError::InvalidInvariant("built-in styles missing"));
        }
        Ok(())
    }
    fn validate_compile_presets(&self) -> Result<(), ProjectError> {
        for preset in self.compile_presets.values() {
            if preset.name.trim().is_empty() || preset.name.chars().count() > 128 {
                return Err(ProjectError::InvalidCompilePreset(
                    "preset name must contain 1–128 characters",
                ));
            }
            if !(1..=6).contains(&preset.titles.document_heading_level) {
                return Err(ProjectError::InvalidCompilePreset(
                    "document title heading level must be 1 through 6",
                ));
            }
            if preset.page.width_micrometres == 0
                || preset.page.height_micrometres == 0
                || preset.page.margin_left_micrometres + preset.page.margin_right_micrometres
                    >= preset.page.width_micrometres
                || preset.page.margin_top_micrometres + preset.page.margin_bottom_micrometres
                    >= preset.page.height_micrometres
            {
                return Err(ProjectError::InvalidCompilePreset(
                    "page dimensions or margins are invalid",
                ));
            }
            let mut roots = BTreeSet::new();
            if preset
                .selected_roots
                .iter()
                .any(|root| !roots.insert(*root))
            {
                return Err(ProjectError::InvalidCompilePreset(
                    "selected roots must be unique stable node IDs",
                ));
            }
            if preset.exporter_settings.iter().any(|(format, values)| {
                format.len() > 64
                    || values.iter().any(|(key, value)| {
                        key.len() > 128
                            || value.len() > 4_096
                            || value.chars().any(char::is_control)
                    })
            }) {
                return Err(ProjectError::InvalidCompilePreset(
                    "exporter settings exceed the bounded text vocabulary",
                ));
            }
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
            StyleKind::Paragraph,
        ),
        (
            StyleId(Uuid::from_u128(0x018f_0be2_a8ea_7d2d_89ea_45aa_6637_08d5)),
            "heading",
            "Heading",
            StyleKind::Paragraph,
        ),
        (
            StyleId(Uuid::from_u128(0x018f_0be2_a8ea_7d2d_89ea_45aa_6637_08d6)),
            "emphasis",
            "Emphasis",
            StyleKind::Character,
        ),
    ]
    .into_iter()
    .map(|(id, key, name, kind)| {
        (
            id,
            StyleDefinition {
                id,
                machine_key: Some(key.into()),
                name: name.into(),
                based_on: None,
                kind,
                next_style: None,
                properties: BTreeMap::new(),
                builtin: true,
            },
        )
    })
    .collect()
}

fn validate_style_properties(properties: &BTreeMap<String, String>) -> Result<(), ProjectError> {
    const KEYS: &[&str] = &[
        "alignment",
        "background",
        "first-line-indent",
        "font-family",
        "font-size",
        "font-style",
        "font-weight",
        "foreground",
        "keep-with-next",
        "left-indent",
        "line-height",
        "page-break-before",
        "right-indent",
        "space-after",
        "space-before",
        "text-decoration",
    ];
    for (key, value) in properties {
        if (!KEYS.contains(&key.as_str()) && !key.starts_with("x-"))
            || key.len() > 64
            || value.len() > 512
            || value.chars().any(char::is_control)
        {
            return Err(ProjectError::InvalidStyle(
                "property key or value is outside the supported bounded vocabulary",
            ));
        }
    }
    Ok(())
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
    /// Creates or replaces a complete compile preset atomically.
    UpsertCompilePreset { preset: CompilePreset },
    /// Removes one compile preset without affecting canonical manuscript data.
    RemoveCompilePreset { id: CompilePresetId },
}

/// Style-definition mutation payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum StyleMutation {
    Create(StyleDefinition),
    Update(StyleDefinition),
    Delete(StyleId),
    /// Deletes a user style and rewires definition references to a same-kind replacement.
    DeleteAndReplace {
        id: StyleId,
        replacement: StyleId,
    },
    /// Internal lossless inverse for [`StyleMutation::DeleteAndReplace`].
    RestoreDeleted {
        style: StyleDefinition,
        affected: Vec<StyleDefinition>,
        replacement: StyleId,
    },
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
    StyleReplaced { id: StyleId, replacement: StyleId },
    CompilePresetSaved(CompilePresetId),
    CompilePresetRemoved(CompilePresetId),
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
    /// Compile preset ID does not exist.
    #[error("compile preset does not exist: {0}")]
    MissingCompilePreset(CompilePresetId),
    /// Compile preset violates bounded persistent settings rules.
    #[error("invalid compile preset: {0}")]
    InvalidCompilePreset(&'static str),
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
    /// A character style cannot inherit from or replace a paragraph style, and vice versa.
    #[error("style kinds are incompatible")]
    StyleKindMismatch,
    /// Style is referenced and requires an explicit replacement.
    #[error("style is still referenced and requires a replacement: {0}")]
    StyleInUse(StyleId),
    /// Style definition contains an invalid value.
    #[error("invalid style: {0}")]
    InvalidStyle(&'static str),
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
        assert!(RelativeProjectPath::new(r"..\..\outside.md").is_err());
        let mut project = Project::new("Novel");
        let a = StyleDefinition {
            id: StyleId::new(),
            machine_key: None,
            name: "A".into(),
            based_on: None,
            kind: StyleKind::Paragraph,
            next_style: None,
            properties: BTreeMap::new(),
            builtin: false,
        };
        let b = StyleDefinition {
            id: StyleId::new(),
            machine_key: None,
            name: "B".into(),
            based_on: Some(a.id),
            kind: StyleKind::Paragraph,
            next_style: None,
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
    fn restore_to_a_new_parent_can_be_undone_and_redone() {
        let mut project = Project::new("Novel");
        let manuscript = project.manuscript_root();
        let research = project.research_root();
        let node = create(&mut project, manuscript, "Scene");

        project.execute(ProjectCommand::Trash { node }).unwrap();
        let undo_restore = project
            .execute(ProjectCommand::Restore {
                node,
                parent: research,
                index: 0,
            })
            .unwrap()
            .undo;
        let redo_restore = project.execute(undo_restore.inverse).unwrap().undo;
        assert!(project.is_trashed(node));

        project.execute(redo_restore.inverse).unwrap();
        assert_eq!(project.nodes[&node].parent, Some(research));
        assert_eq!(project.nodes[&research].children, [node]);
        project.validate().unwrap();
    }
    #[test]
    fn randomized_style_rename_inheritance_and_replacement_remain_resolvable() {
        let mut project = Project::new("Styles");
        let body = project
            .styles
            .values()
            .find(|style| style.machine_key.as_deref() == Some("body"))
            .unwrap()
            .id;
        let mut ids = Vec::new();
        for index in 0..24 {
            let id = StyleId::new();
            project
                .execute(ProjectCommand::MutateStyle {
                    mutation: StyleMutation::Create(StyleDefinition {
                        id,
                        machine_key: None,
                        name: format!("User {index}"),
                        based_on: ids.last().copied().or(Some(body)),
                        kind: StyleKind::Paragraph,
                        next_style: Some(body),
                        properties: BTreeMap::from([(
                            "space-after".into(),
                            format!("{}pt", index % 9),
                        )]),
                        builtin: false,
                    }),
                })
                .unwrap();
            ids.push(id);
        }
        let mut state = 0x5eed_u64;
        for step in 0..500 {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let index = usize::try_from(state % ids.len() as u64).unwrap();
            let mut style = project.styles[&ids[index]].clone();
            style.name = format!("Renamed {step}-{index}");
            style.based_on = if index == 0 {
                Some(body)
            } else {
                Some(ids[usize::try_from(state % index as u64).unwrap()])
            };
            style.properties.insert(
                "line-height".into(),
                format!("{}.{}", 1 + step % 2, step % 10),
            );
            project
                .execute(ProjectCommand::MutateStyle {
                    mutation: StyleMutation::Update(style),
                })
                .unwrap();
            project.computed_style(ids[index]).unwrap();
        }
        let deleted = ids[12];
        let replacement = ids[3];
        let outcome = project
            .execute(ProjectCommand::MutateStyle {
                mutation: StyleMutation::DeleteAndReplace {
                    id: deleted,
                    replacement,
                },
            })
            .unwrap();
        assert!(!project.styles.contains_key(&deleted));
        project.execute(outcome.undo.inverse).unwrap();
        assert!(project.styles.contains_key(&deleted));
        project.validate().unwrap();
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

    #[test]
    fn deep_graph_validation_and_duplicate_are_iterative() {
        let mut project = Project::new("Deep");
        let root = project.manuscript_root();
        let mut parent = root;
        let mut first = None;
        for depth in 0..20_000 {
            let node_id = NodeId::new();
            let document_id = DocumentId::new();
            project
                .nodes
                .get_mut(&parent)
                .unwrap()
                .children
                .push(node_id);
            project.nodes.insert(
                node_id,
                Node {
                    id: node_id,
                    kind: NodeKind::Group { document_id },
                    parent: Some(parent),
                    children: Vec::new(),
                },
            );
            project.documents.insert(
                document_id,
                DocumentRecord {
                    id: document_id,
                    node_id,
                    path: RelativeProjectPath::new(format!("manuscript/{node_id}.md")).unwrap(),
                    metadata: DocumentMetadata {
                        title: format!("Depth {depth}"),
                        ..DocumentMetadata::default()
                    },
                },
            );
            first.get_or_insert(node_id);
            parent = node_id;
        }
        project.validate().unwrap();

        let outcome = project
            .execute(ProjectCommand::Duplicate {
                node: first.unwrap(),
                parent: root,
                index: 1,
            })
            .unwrap();
        assert!(matches!(
            outcome.events.as_slice(),
            [ProjectEvent::NodeDuplicated { .. }]
        ));
        project.validate().unwrap();
    }

    #[test]
    fn deep_invalid_graph_fails_without_recursive_stack_use() {
        let mut project = Project::new("Deep invalid");
        let root = project.manuscript_root();
        let mut parent = root;
        let mut first = None;
        for _ in 0..30_000 {
            let node_id = NodeId::new();
            let document_id = DocumentId::new();
            project
                .nodes
                .get_mut(&parent)
                .unwrap()
                .children
                .push(node_id);
            project.nodes.insert(
                node_id,
                Node {
                    id: node_id,
                    kind: NodeKind::Document { document_id },
                    parent: Some(parent),
                    children: Vec::new(),
                },
            );
            project.documents.insert(
                document_id,
                DocumentRecord {
                    id: document_id,
                    node_id,
                    path: RelativeProjectPath::new(format!("manuscript/{node_id}.md")).unwrap(),
                    metadata: DocumentMetadata::default(),
                },
            );
            first.get_or_insert(node_id);
            parent = node_id;
        }
        project
            .nodes
            .get_mut(&parent)
            .unwrap()
            .children
            .push(first.unwrap());
        assert!(project.validate().is_err());
    }
}
