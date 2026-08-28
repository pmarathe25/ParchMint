//! Deterministic presentation state for project-facing workspace views.
//!
//! Service crates remain responsible for project mutation, History, search,
//! export, preferences, and persistence. This module validates UI intent,
//! retains temporary view state, and emits effects for the integration layer.

use std::collections::{BTreeMap, BTreeSet};

use iced::widget::text_editor;
#[cfg(feature = "diagnostics")]
use parchmint_diagnostics::{self as diagnostics, Level as DiagnosticLevel};
use parchmint_domain::{
    MetadataApplicability as DomainMetadataApplicability, MetadataFieldDefinition, MetadataFieldId,
    MetadataTextKind as DomainMetadataTextKind, NodeKind, Project, ProjectExportSetting,
    ProjectExportSettings, ProjectSection, StyleCatalog, StyleDefinition, StyleId, StyleProperties,
    StyleRole, TextAlignment,
};
use parchmint_editor_api::{CanonicalDocumentLoad, SemanticDocument};
use parchmint_editor_core::EditorCoreSession;
use parchmint_preferences::{AppearanceMode, ResolvedAppearance};
use parchmint_ui_api::{
    ExportArtifact, ExportArtifactToken, HistoryMaintenanceStatus, ProjectSnapshot,
};
use parchmint_workspace_state::{
    ExplorerWorkspaceState, OpenTabState, PaneLayout, SavedViewState, WorkspaceMode,
    WorkspaceSnapshot,
};

use crate::{
    EditorFixture, EditorMessage, EditorPane, EditorWorkspace, InspectorContext, Point,
    RibbonDestination, TabSpec,
};

/// Requirement-linked project fixture families.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectFixture {
    Explorer,
    Cards,
    GlobalSearch,
    History,
    RecentlyDeleted,
    SettingsAppearance,
    Export,
    ErrorRecovery,
}

/// The mutually exclusive left-sidebar surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarSurface {
    Explorer,
    GlobalSearch,
}

/// The interaction used to update the shared hierarchy selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionGesture {
    Replace,
    Additive,
    ContiguousRange,
}

/// User-creatable hierarchy node kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HierarchyItemKind {
    Group,
    Document,
}

/// A hierarchy drop target expressed without GUI-framework values.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum DragDestination {
    BeforeSibling(String),
    AfterSibling(String),
    IntoGroup(String),
    EditorPane(EditorPane),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeClipboardKind {
    Copy,
    Cut,
}

#[derive(Debug, Clone)]
struct TreeClipboard {
    session: u64,
    kind: TreeClipboardKind,
    node_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HierarchyPointerDrag {
    source_id: String,
    destination: Option<DragDestination>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HierarchyRename {
    node_id: String,
    title: String,
}

#[derive(Debug, Clone)]
struct PendingHierarchyCreation {
    parent_id: String,
    kind: HierarchyItemKind,
}

/// Deterministic validation for one drag operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragValidity {
    Allowed,
    RejectedCycle,
    RejectedDocumentParent,
    RejectedMissingNode,
    RejectedNoOp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HierarchyNodeKind {
    Root,
    Group,
    Document,
}

/// Public, read-only hierarchy kind used by Explorer and Cards renderers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HierarchyRowKind {
    Root,
    Group,
    Document,
}

#[derive(Debug, Clone)]
struct HierarchyNode {
    id: String,
    title: String,
    section_id: String,
    parent: Option<String>,
    children: Vec<String>,
    kind: HierarchyNodeKind,
    document_id: Option<String>,
    synopsis: String,
}

impl HierarchyNode {
    fn new(
        id: &str,
        title: &str,
        section_id: &str,
        parent: Option<&str>,
        kind: HierarchyNodeKind,
    ) -> Self {
        Self {
            id: id.to_owned(),
            title: title.to_owned(),
            section_id: section_id.to_owned(),
            parent: parent.map(str::to_owned),
            children: Vec::new(),
            kind,
            document_id: (kind == HierarchyNodeKind::Document).then(|| id.to_owned()),
            synopsis: String::new(),
        }
    }
}

/// One ordered Explorer row without exposing the mutable hierarchy internals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorerRow<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub synopsis: &'a str,
    pub section_id: &'a str,
    pub parent_id: Option<&'a str>,
    pub child_ids: Vec<&'a str>,
    pub kind: HierarchyRowKind,
    pub document_id: Option<&'a str>,
    pub expanded: bool,
    pub selected: bool,
    pub cut_pending: bool,
}

/// Shared Explorer/Cards hierarchy and selection presentation.
#[derive(Debug, Clone)]
pub struct ExplorerState {
    nodes: BTreeMap<String, HierarchyNode>,
    roots: Vec<String>,
    expanded: BTreeSet<String>,
    selected: BTreeSet<String>,
    selection_anchor: Option<String>,
    cut_pending: BTreeSet<String>,
}

impl ExplorerState {
    fn fixture() -> Self {
        let mut nodes = BTreeMap::new();
        for node in [
            HierarchyNode::new(
                "manuscript",
                "Manuscript",
                "manuscript",
                None,
                HierarchyNodeKind::Root,
            ),
            HierarchyNode::new(
                "part-one",
                "Part One",
                "manuscript",
                Some("manuscript"),
                HierarchyNodeKind::Group,
            ),
            HierarchyNode::new(
                "chapter-one",
                "Chapter One",
                "manuscript",
                Some("part-one"),
                HierarchyNodeKind::Document,
            ),
            HierarchyNode::new(
                "chapter-two",
                "Chapter Two",
                "manuscript",
                Some("part-one"),
                HierarchyNodeKind::Document,
            ),
            HierarchyNode::new(
                "chapter-three",
                "Chapter Three",
                "manuscript",
                Some("manuscript"),
                HierarchyNodeKind::Document,
            ),
            HierarchyNode::new(
                "research",
                "Research",
                "research",
                None,
                HierarchyNodeKind::Root,
            ),
            HierarchyNode::new(
                "research-notes",
                "Research Notes",
                "research",
                Some("research"),
                HierarchyNodeKind::Document,
            ),
        ] {
            nodes.insert(node.id.clone(), node);
        }
        for (parent, children) in [
            (
                "manuscript",
                vec!["part-one".to_owned(), "chapter-three".to_owned()],
            ),
            (
                "part-one",
                vec!["chapter-one".to_owned(), "chapter-two".to_owned()],
            ),
            ("research", vec!["research-notes".to_owned()]),
        ] {
            nodes
                .get_mut(parent)
                .expect("fixture parent exists")
                .children = children;
        }
        nodes
            .get_mut("chapter-one")
            .expect("fixture document exists")
            .synopsis = "A first-person opening beside the river.".to_owned();
        Self {
            nodes,
            roots: vec!["manuscript".to_owned(), "research".to_owned()],
            expanded: BTreeSet::from([
                "manuscript".to_owned(),
                "part-one".to_owned(),
                "research".to_owned(),
            ]),
            selected: BTreeSet::new(),
            selection_anchor: None,
            cut_pending: BTreeSet::new(),
        }
    }

    fn from_project(project: &Project) -> Self {
        let mut nodes = BTreeMap::new();
        for (id, node) in project.nodes.iter() {
            let id = stable_id_string(id.as_bytes());
            let section = project
                .nodes
                .section(node.id)
                .expect("a validated project node belongs to a fixed section");
            let kind = match node.kind {
                NodeKind::Root(_) => HierarchyNodeKind::Root,
                NodeKind::Group => HierarchyNodeKind::Group,
                NodeKind::Document(_) => HierarchyNodeKind::Document,
            };
            let document_id = match node.kind {
                NodeKind::Document(document_id) => Some(stable_id_string(document_id.as_bytes())),
                NodeKind::Root(_) | NodeKind::Group => None,
            };
            nodes.insert(
                id.clone(),
                HierarchyNode {
                    id,
                    title: node.title.clone(),
                    section_id: stable_id_string(section.root_id().as_bytes()),
                    parent: project
                        .nodes
                        .parent(node.id)
                        .map(|parent| stable_id_string(parent.as_bytes())),
                    children: project
                        .nodes
                        .children(node.id)
                        .iter()
                        .map(|child| stable_id_string(child.as_bytes()))
                        .collect(),
                    kind,
                    document_id,
                    synopsis: node.synopsis.clone(),
                },
            );
        }
        let roots = [ProjectSection::Manuscript, ProjectSection::Research]
            .into_iter()
            .map(|section| stable_id_string(section.root_id().as_bytes()))
            .collect::<Vec<_>>();
        Self {
            nodes,
            expanded: roots.iter().cloned().collect(),
            roots,
            selected: BTreeSet::new(),
            selection_anchor: None,
            cut_pending: BTreeSet::new(),
        }
    }

    fn reconcile_project(&mut self, project: &Project) {
        let mut authoritative = Self::from_project(project);
        authoritative.expanded = self
            .expanded
            .iter()
            .filter(|id| {
                authoritative.nodes.get(*id).is_some_and(|node| {
                    matches!(
                        node.kind,
                        HierarchyNodeKind::Root | HierarchyNodeKind::Group
                    )
                })
            })
            .cloned()
            .collect();
        authoritative.selected = self
            .selected
            .iter()
            .filter(|id| authoritative.nodes.contains_key(*id))
            .cloned()
            .collect();
        authoritative.selection_anchor = self
            .selection_anchor
            .as_ref()
            .filter(|id| authoritative.nodes.contains_key(*id))
            .cloned();
        authoritative.cut_pending = self
            .cut_pending
            .iter()
            .filter(|id| {
                authoritative
                    .nodes
                    .get(*id)
                    .is_some_and(|node| node.kind != HierarchyNodeKind::Root)
            })
            .cloned()
            .collect();
        authoritative.normalize_selection();
        *self = authoritative;
    }

    /// All hierarchy rows in canonical root and child order.
    pub fn rows(&self) -> Vec<ExplorerRow<'_>> {
        self.preorder_ids()
            .into_iter()
            .filter_map(|id| self.row(id))
            .collect()
    }

    /// A single hierarchy row by its serialized typed node ID.
    pub fn row(&self, node_id: &str) -> Option<ExplorerRow<'_>> {
        let node = self.nodes.get(node_id)?;
        Some(ExplorerRow {
            id: &node.id,
            title: &node.title,
            synopsis: &node.synopsis,
            section_id: &node.section_id,
            parent_id: node.parent.as_deref(),
            child_ids: node.children.iter().map(String::as_str).collect(),
            kind: match node.kind {
                HierarchyNodeKind::Root => HierarchyRowKind::Root,
                HierarchyNodeKind::Group => HierarchyRowKind::Group,
                HierarchyNodeKind::Document => HierarchyRowKind::Document,
            },
            document_id: node.document_id.as_deref(),
            expanded: self.expanded.contains(node_id),
            selected: self.selected.contains(node_id),
            cut_pending: self.cut_pending.contains(node_id),
        })
    }

    fn contains_document(&self, document_id: &str) -> bool {
        self.nodes
            .values()
            .any(|node| node.document_id.as_deref() == Some(document_id))
    }

    pub fn title_for_document(&self, document_id: &str) -> Option<&str> {
        self.nodes
            .values()
            .find(|node| node.document_id.as_deref() == Some(document_id))
            .map(|node| node.title.as_str())
    }

    pub fn node_id_for_document(&self, document_id: &str) -> Option<&str> {
        self.nodes
            .values()
            .find(|node| node.document_id.as_deref() == Some(document_id))
            .map(|node| node.id.as_str())
    }

    fn reveal_document(&mut self, document_id: &str) -> bool {
        let Some(node_id) = self.node_id_for_document(document_id).map(str::to_owned) else {
            return false;
        };
        let ancestors = self
            .ancestors(&node_id)
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        self.expanded.extend(ancestors);
        self.select(&node_id, SelectionGesture::Replace);
        true
    }

    /// Selected nodes in deterministic visible hierarchy order.
    pub fn selected_ids(&self) -> Vec<&str> {
        self.preorder_ids()
            .into_iter()
            .filter(|id| self.selected.contains(*id))
            .collect()
    }

    /// Root section IDs in their explicit order.
    pub fn root_ids(&self) -> Vec<&str> {
        self.roots.iter().map(String::as_str).collect()
    }

    pub fn title(&self, node_id: &str) -> Option<&str> {
        self.nodes.get(node_id).map(|node| node.title.as_str())
    }

    pub fn synopsis(&self, node_id: &str) -> Option<&str> {
        self.nodes.get(node_id).map(|node| node.synopsis.as_str())
    }

    pub fn section_id(&self, node_id: &str) -> Option<&str> {
        self.nodes.get(node_id).map(|node| node.section_id.as_str())
    }

    pub fn is_expanded(&self, node_id: &str) -> bool {
        self.expanded.contains(node_id)
    }

    pub fn is_cut_pending(&self, node_id: &str) -> bool {
        self.cut_pending.contains(node_id)
    }

    pub fn drag_validity(&self, source_id: &str, destination: DragDestination) -> DragValidity {
        let Some(source) = self.nodes.get(source_id) else {
            return DragValidity::RejectedMissingNode;
        };
        match destination {
            DragDestination::EditorPane(_) => {
                if source.kind == HierarchyNodeKind::Document {
                    DragValidity::Allowed
                } else {
                    DragValidity::RejectedDocumentParent
                }
            }
            DragDestination::IntoGroup(target_id) => {
                let Some(target) = self.nodes.get(&target_id) else {
                    return DragValidity::RejectedMissingNode;
                };
                if source_id == target_id || self.is_ancestor(source_id, &target_id) {
                    return DragValidity::RejectedCycle;
                }
                if !matches!(
                    target.kind,
                    HierarchyNodeKind::Root | HierarchyNodeKind::Group
                ) {
                    return DragValidity::RejectedDocumentParent;
                }
                if source.parent.as_deref() == Some(target_id.as_str()) {
                    return DragValidity::RejectedNoOp;
                }
                DragValidity::Allowed
            }
            DragDestination::BeforeSibling(target_id)
            | DragDestination::AfterSibling(target_id) => {
                let Some(target) = self.nodes.get(&target_id) else {
                    return DragValidity::RejectedMissingNode;
                };
                if source_id == target_id {
                    return DragValidity::RejectedNoOp;
                }
                if self.is_ancestor(source_id, &target_id) {
                    return DragValidity::RejectedCycle;
                }
                if target.parent.is_none() {
                    return DragValidity::RejectedDocumentParent;
                }
                DragValidity::Allowed
            }
        }
    }

    fn select(&mut self, node_id: &str, gesture: SelectionGesture) {
        if !self.nodes.contains_key(node_id) {
            return;
        }
        match gesture {
            SelectionGesture::Replace => {
                self.selected.clear();
                self.selected.insert(node_id.to_owned());
                self.selection_anchor = Some(node_id.to_owned());
            }
            SelectionGesture::Additive => {
                if !self.selected.remove(node_id) {
                    self.selected.insert(node_id.to_owned());
                }
                self.selection_anchor = Some(node_id.to_owned());
            }
            SelectionGesture::ContiguousRange => {
                let order = self.preorder_ids();
                let Some(target) = order.iter().position(|candidate| *candidate == node_id) else {
                    return;
                };
                let anchor = self
                    .selection_anchor
                    .as_deref()
                    .and_then(|anchor| order.iter().position(|candidate| *candidate == anchor))
                    .unwrap_or(target);
                let (start, end) = if anchor <= target {
                    (anchor, target)
                } else {
                    (target, anchor)
                };
                self.selected = order[start..=end]
                    .iter()
                    .map(|id| (*id).to_owned())
                    .collect();
            }
        }
        self.normalize_selection();
    }

    fn toggle_expanded(&mut self, node_id: &str) {
        let is_container = self.nodes.get(node_id).is_some_and(|node| {
            matches!(
                node.kind,
                HierarchyNodeKind::Root | HierarchyNodeKind::Group
            )
        });
        if !is_container {
            return;
        }
        if !self.expanded.remove(node_id) {
            self.expanded.insert(node_id.to_owned());
        }
    }

    fn rename(&mut self, node_id: &str, title: String) {
        if let Some(node) = self.nodes.get_mut(node_id)
            && !title.trim().is_empty()
        {
            node.title = title;
        }
    }

    fn set_synopsis(&mut self, node_id: &str, synopsis: String) {
        if let Some(node) = self.nodes.get_mut(node_id) {
            node.synopsis = synopsis;
        }
    }

    fn mark_cut(&mut self) -> bool {
        let selected = self.normalized_selected_ids();
        if selected.is_empty()
            || selected.iter().any(|id| {
                self.nodes
                    .get(*id)
                    .is_none_or(|node| node.kind == HierarchyNodeKind::Root)
            })
        {
            return false;
        }
        self.cut_pending = selected.into_iter().map(str::to_owned).collect();
        true
    }

    fn cancel_cut(&mut self) {
        self.cut_pending.clear();
    }

    fn complete_cut(&mut self) {
        self.cut_pending.clear();
    }

    fn normalized_selected_ids(&self) -> Vec<&str> {
        self.selected_ids()
    }

    fn normalize_selection(&mut self) {
        let selected = self.selected.clone();
        self.selected = selected
            .iter()
            .filter(|node_id| {
                !self
                    .ancestors(node_id)
                    .into_iter()
                    .any(|ancestor| selected.contains(ancestor))
            })
            .cloned()
            .collect();
    }

    fn ancestors(&self, node_id: &str) -> Vec<&str> {
        let mut ancestors = Vec::new();
        let mut parent = self
            .nodes
            .get(node_id)
            .and_then(|node| node.parent.as_deref());
        while let Some(parent_id) = parent {
            ancestors.push(parent_id);
            parent = self
                .nodes
                .get(parent_id)
                .and_then(|node| node.parent.as_deref());
        }
        ancestors
    }

    fn is_ancestor(&self, ancestor_id: &str, node_id: &str) -> bool {
        self.ancestors(node_id).contains(&ancestor_id)
    }

    fn ancestors_are_expanded(&self, node_id: &str) -> bool {
        self.ancestors(node_id)
            .into_iter()
            .all(|ancestor| self.expanded.contains(ancestor))
    }

    fn depth(&self, node_id: &str) -> usize {
        self.ancestors(node_id).len()
    }

    fn preorder_ids(&self) -> Vec<&str> {
        let mut order = Vec::new();
        for root in &self.roots {
            self.append_preorder(root, &mut order);
        }
        order
    }

    fn append_preorder<'a>(&'a self, node_id: &'a str, order: &mut Vec<&'a str>) {
        order.push(node_id);
        if let Some(node) = self.nodes.get(node_id) {
            for child in &node.children {
                self.append_preorder(child, order);
            }
        }
    }
}

fn synopsis_editors(explorer: &ExplorerState) -> BTreeMap<String, text_editor::Content> {
    explorer
        .nodes
        .iter()
        .map(|(node_id, node)| {
            (
                node_id.clone(),
                text_editor::Content::with_text(&node.synopsis),
            )
        })
        .collect()
}

/// Cards-specific projection over the shared hierarchy state.
pub struct CardsState<'a> {
    explorer: &'a ExplorerState,
    section_id: &'a str,
    drag_destination: Option<&'a DragDestination>,
    last_activated_document: Option<&'a str>,
    visible_metadata_labels: Vec<&'a str>,
    definitions: &'a BTreeMap<String, MetadataDefinition>,
    field_order: &'a [String],
    values: &'a BTreeMap<(String, String), String>,
}

/// One ordered Cards item with effective visible metadata values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardItem<'a> {
    pub node_id: &'a str,
    pub document_id: Option<&'a str>,
    pub title: &'a str,
    pub synopsis: &'a str,
    pub kind: HierarchyRowKind,
    pub depth: usize,
    pub expanded: bool,
    pub visible: bool,
    pub selected: bool,
    pub metadata: Vec<(&'a str, &'a str, Option<&'a str>)>,
}

impl<'a> CardsState<'a> {
    pub fn section_id(&self) -> &str {
        self.section_id
    }

    pub const fn shows_hierarchy(&self) -> bool {
        true
    }

    pub fn drag_destination(&self) -> Option<&DragDestination> {
        self.drag_destination
    }

    pub fn visible_metadata_labels(&self) -> Vec<&str> {
        self.visible_metadata_labels.clone()
    }

    pub fn selected_ids(&self) -> Vec<&str> {
        self.explorer.selected_ids()
    }

    pub fn last_activated_document(&self) -> Option<&str> {
        self.last_activated_document
    }

    /// Items under the selected section in canonical hierarchy order.
    pub fn items(&self) -> Vec<CardItem<'a>> {
        self.explorer
            .preorder_ids()
            .into_iter()
            .filter(|id| {
                *id != self.section_id
                    && self
                        .explorer
                        .nodes
                        .get(*id)
                        .is_some_and(|node| node.section_id == self.section_id)
            })
            .filter_map(|id| {
                let node = self.explorer.nodes.get(id)?;
                let metadata = self
                    .field_order
                    .iter()
                    .filter_map(|field_id| {
                        let definition = self.definitions.get(field_id)?;
                        (definition.visible_on_cards
                            && definition.applicability.applies_to(node.kind))
                        .then(|| {
                            // Defaults are copied when a node is created. Existing
                            // nodes without a stored value stay empty; a later
                            // definition edit must never rewrite their cards.
                            let value = self
                                .values
                                .get(&(id.to_owned(), field_id.clone()))
                                .map(String::as_str);
                            (field_id.as_str(), definition.label.as_str(), value)
                        })
                    })
                    .collect();
                Some(CardItem {
                    node_id: id,
                    document_id: node.document_id.as_deref(),
                    title: &node.title,
                    synopsis: &node.synopsis,
                    kind: match node.kind {
                        HierarchyNodeKind::Root => HierarchyRowKind::Root,
                        HierarchyNodeKind::Group => HierarchyRowKind::Group,
                        HierarchyNodeKind::Document => HierarchyRowKind::Document,
                    },
                    depth: self.explorer.depth(id).saturating_sub(1),
                    expanded: self.explorer.expanded.contains(id),
                    visible: self.explorer.ancestors_are_expanded(id),
                    selected: self.explorer.selected.contains(id),
                    metadata,
                })
            })
            .collect()
    }
}

/// Which live hierarchy node kinds expose a metadata field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataFieldApplicability {
    Groups,
    Documents,
    GroupsAndDocuments,
}

/// The text editor shape required by a metadata definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataFieldTextKind {
    SingleLine,
    Multiline,
}

/// One editable Settings style property. The UI intentionally names every
/// persisted property so no formatting control silently disappears.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleProperty {
    FontFamily,
    FontSizePoints,
    Weight,
    Italic,
    Alignment,
    FirstLineIndentPoints,
    LeftIndentPoints,
    RightIndentPoints,
    LineSpacing,
    SpaceBeforePoints,
    SpaceAfterPoints,
    KeepWithNext,
    PageBreakBefore,
}

impl StyleProperty {
    pub const fn label(self) -> &'static str {
        match self {
            Self::FontFamily => "Font family",
            Self::FontSizePoints => "Font size (pt)",
            Self::Weight => "Weight",
            Self::Italic => "Italic",
            Self::Alignment => "Alignment",
            Self::FirstLineIndentPoints => "First-line indent (pt)",
            Self::LeftIndentPoints => "Left indent (pt)",
            Self::RightIndentPoints => "Right indent (pt)",
            Self::LineSpacing => "Line spacing",
            Self::SpaceBeforePoints => "Space before (pt)",
            Self::SpaceAfterPoints => "Space after (pt)",
            Self::KeepWithNext => "Keep with next",
            Self::PageBreakBefore => "Page break before",
        }
    }
}

impl MetadataFieldApplicability {
    const fn applies_to(self, kind: HierarchyNodeKind) -> bool {
        matches!(
            (self, kind),
            (Self::Groups, HierarchyNodeKind::Group)
                | (Self::Documents, HierarchyNodeKind::Document)
                | (
                    Self::GroupsAndDocuments,
                    HierarchyNodeKind::Group | HierarchyNodeKind::Document
                )
        )
    }
}

#[derive(Debug, Clone)]
struct MetadataDefinition {
    label: String,
    description: Option<String>,
    applicability: MetadataFieldApplicability,
    text_kind: MetadataFieldTextKind,
    default_value: Option<String>,
    visible_on_cards: bool,
}

/// Inspector-facing Synopsis and metadata projection.
pub struct InspectorState<'a> {
    explorer: &'a ExplorerState,
    definitions: &'a BTreeMap<String, MetadataDefinition>,
    field_order: &'a [String],
    values: &'a BTreeMap<(String, String), String>,
}

/// One Inspector metadata row with both stored and effective values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorMetadataItem<'a> {
    pub field_id: &'a str,
    pub label: &'a str,
    pub description: Option<&'a str>,
    pub stored_value: Option<&'a str>,
    pub effective_value: Option<&'a str>,
    pub applicability: MetadataFieldApplicability,
    pub text_kind: MetadataFieldTextKind,
}

impl<'a> InspectorState<'a> {
    pub const fn synopsis_is_multiline_plain_text(&self) -> bool {
        true
    }

    pub const fn metadata_is_ordered_by_settings(&self) -> bool {
        true
    }

    pub fn metadata_value(&self, node_id: &str, field_id: &str) -> Option<&str> {
        self.metadata_field_is_visible(node_id, field_id)
            .then(|| self.stored_metadata_value(node_id, field_id))
            .flatten()
    }

    pub fn stored_metadata_value(&self, node_id: &str, field_id: &str) -> Option<&str> {
        self.values
            .get(&(node_id.to_owned(), field_id.to_owned()))
            .map(String::as_str)
    }

    pub fn metadata_field_is_visible(&self, node_id: &str, field_id: &str) -> bool {
        let Some(node) = self.explorer.nodes.get(node_id) else {
            return false;
        };
        self.definitions
            .get(field_id)
            .is_some_and(|definition| definition.applicability.applies_to(node.kind))
    }

    pub fn visible_field_ids(&self, node_id: &str) -> Vec<&str> {
        self.field_order
            .iter()
            .filter(|field| self.metadata_field_is_visible(node_id, field))
            .map(String::as_str)
            .collect()
    }

    /// Ordered metadata rows applicable to one selected hierarchy node.
    pub fn metadata_items(&self, node_id: &str) -> Vec<InspectorMetadataItem<'a>> {
        let Some(kind) = self.explorer.nodes.get(node_id).map(|node| node.kind) else {
            return Vec::new();
        };
        let definitions = self.definitions;
        let field_order = self.field_order;
        let values = self.values;
        field_order
            .iter()
            .filter_map(|field_id| {
                let definition = definitions.get(field_id)?;
                if !definition.applicability.applies_to(kind) {
                    return None;
                }
                let stored_value = values
                    .get(&(node_id.to_owned(), field_id.clone()))
                    .map(String::as_str);
                Some(InspectorMetadataItem {
                    field_id,
                    label: &definition.label,
                    description: definition.description.as_deref(),
                    stored_value,
                    // Defaults copy on node creation. They are not a live fallback
                    // for an existing inspector value.
                    effective_value: stored_value,
                    applicability: definition.applicability,
                    text_kind: definition.text_kind,
                })
            })
            .collect()
    }
}

/// Stable settings definition exposed for headless verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataFieldSummary<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub description: Option<&'a str>,
    pub default_value: Option<&'a str>,
    pub visible_on_cards: bool,
    pub applicability: MetadataFieldApplicability,
    pub text_kind: MetadataFieldTextKind,
}

/// A Settings list/detail selection. IDs remain stable while labels change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsDetail {
    MetadataField(String),
    Style(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsCategory {
    General,
    Appearance,
    Styles,
    Metadata,
    Dictionaries,
}

impl SettingsCategory {
    pub const fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Appearance => "Appearance",
            Self::Styles => "Styles",
            Self::Metadata => "Metadata fields",
            Self::Dictionaries => "Dictionaries",
        }
    }
}

/// One stable Settings navigation item. The selection is always one of these
/// categories, even when its detail surface has no project-backed controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsCategoryItem {
    pub category: SettingsCategory,
    pub label: &'static str,
    pub selected: bool,
}

/// Dictionary storage scopes. Project words are authored project data; global
/// words remain application preferences and are intentionally not mirrored in
/// a project snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictionaryScope {
    Project,
    Global,
}

impl DictionaryScope {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Project => "Project dictionary",
            Self::Global => "Global dictionary",
        }
    }
}

/// One selectable dictionary scope. An unavailable scope has no editable
/// words in this presentation state, rather than a stale copied value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DictionaryScopeItem {
    pub scope: DictionaryScope,
    pub label: &'static str,
    pub available: bool,
    pub selected: bool,
}

/// Settings projection for the fixed v1 spelling language and dictionary data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionarySettingsState {
    project_words: Vec<String>,
    selected_scope: DictionaryScope,
}

impl DictionarySettingsState {
    fn fixture() -> Self {
        Self {
            project_words: Vec::new(),
            selected_scope: DictionaryScope::Project,
        }
    }

    fn from_project(project: &Project) -> Self {
        Self {
            project_words: project.dictionary.iter().map(str::to_owned).collect(),
            selected_scope: DictionaryScope::Project,
        }
    }

    pub const fn language(&self) -> &'static str {
        "en-US"
    }

    pub const fn selected_scope(&self) -> DictionaryScope {
        self.selected_scope
    }

    pub const fn scope_available(&self, scope: DictionaryScope) -> bool {
        matches!(scope, DictionaryScope::Project)
    }

    pub fn scopes(&self) -> [DictionaryScopeItem; 2] {
        [DictionaryScope::Project, DictionaryScope::Global].map(|scope| DictionaryScopeItem {
            scope,
            label: scope.label(),
            available: self.scope_available(scope),
            selected: self.selected_scope == scope,
        })
    }

    pub fn words(&self) -> Option<&[String]> {
        self.scope_available(self.selected_scope)
            .then_some(self.project_words.as_slice())
    }

    fn select_scope(&mut self, scope: DictionaryScope) {
        if self.scope_available(scope) {
            self.selected_scope = scope;
        }
    }
}

/// One style definition projected for Settings list/detail presentation.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleSummary<'a> {
    pub id: &'a str,
    pub display_name: &'a str,
    pub role: StyleRole,
    pub inherits: Option<&'a str>,
    pub properties: &'a StyleProperties,
}

/// Presentation state for project Settings.
#[derive(Debug, Clone)]
pub struct SettingsState {
    appearance: AppearanceMode,
    dictionaries: DictionarySettingsState,
    metadata_definitions: BTreeMap<String, MetadataDefinition>,
    metadata_order: Vec<String>,
    style_definitions: BTreeMap<String, StyleDefinition>,
    style_order: Vec<String>,
    selected_detail: Option<SettingsDetail>,
    selected_category: SettingsCategory,
    metadata_drag_source: Option<String>,
    metadata_drag_target: Option<usize>,
}

impl SettingsState {
    fn fixture() -> Self {
        Self {
            appearance: AppearanceMode::System,
            dictionaries: DictionarySettingsState::fixture(),
            metadata_definitions: BTreeMap::from([
                (
                    "field-17".to_owned(),
                    MetadataDefinition {
                        label: "Point of view".to_owned(),
                        description: Some("Narrative perspective".to_owned()),
                        applicability: MetadataFieldApplicability::GroupsAndDocuments,
                        text_kind: MetadataFieldTextKind::SingleLine,
                        default_value: None,
                        visible_on_cards: true,
                    },
                ),
                (
                    "field-18".to_owned(),
                    MetadataDefinition {
                        label: "Location".to_owned(),
                        description: None,
                        applicability: MetadataFieldApplicability::Documents,
                        text_kind: MetadataFieldTextKind::SingleLine,
                        default_value: Some("Unknown".to_owned()),
                        visible_on_cards: true,
                    },
                ),
            ]),
            metadata_order: vec!["field-17".to_owned(), "field-18".to_owned()],
            style_definitions: StyleCatalog::default()
                .iter()
                .map(|definition| {
                    (
                        stable_id_string(definition.id.as_bytes()),
                        definition.clone(),
                    )
                })
                .collect(),
            style_order: StyleCatalog::default()
                .iter()
                .map(|definition| stable_id_string(definition.id.as_bytes()))
                .collect(),
            selected_detail: None,
            selected_category: SettingsCategory::Appearance,
            metadata_drag_source: None,
            metadata_drag_target: None,
        }
    }

    fn from_project(project: &Project, appearance: AppearanceMode) -> Self {
        let mut metadata_definitions = BTreeMap::new();
        let mut metadata_order = Vec::new();
        for definition in project.metadata.iter() {
            let id = stable_id_string(definition.id.as_bytes());
            metadata_order.push(id.clone());
            metadata_definitions.insert(
                id,
                MetadataDefinition {
                    label: definition.label.clone(),
                    description: definition.description.clone(),
                    applicability: match definition.applicability {
                        DomainMetadataApplicability::Groups => MetadataFieldApplicability::Groups,
                        DomainMetadataApplicability::Documents => {
                            MetadataFieldApplicability::Documents
                        }
                        DomainMetadataApplicability::GroupsAndDocuments => {
                            MetadataFieldApplicability::GroupsAndDocuments
                        }
                    },
                    text_kind: match definition.text_kind {
                        DomainMetadataTextKind::SingleLine => MetadataFieldTextKind::SingleLine,
                        DomainMetadataTextKind::Multiline => MetadataFieldTextKind::Multiline,
                    },
                    default_value: definition.default_value.clone(),
                    visible_on_cards: definition.visible_on_cards,
                },
            );
        }
        let style_order = project
            .styles
            .iter()
            .map(|definition| stable_id_string(definition.id.as_bytes()))
            .collect::<Vec<_>>();
        let style_definitions = project
            .styles
            .iter()
            .map(|definition| {
                (
                    stable_id_string(definition.id.as_bytes()),
                    definition.clone(),
                )
            })
            .collect();
        Self {
            appearance,
            dictionaries: DictionarySettingsState::from_project(project),
            metadata_definitions,
            metadata_order,
            style_definitions,
            style_order,
            selected_detail: None,
            selected_category: SettingsCategory::Appearance,
            metadata_drag_source: None,
            metadata_drag_target: None,
        }
    }

    pub const fn appearance_choices(&self) -> [AppearanceMode; 3] {
        [
            AppearanceMode::System,
            AppearanceMode::Light,
            AppearanceMode::Dark,
        ]
    }

    pub const fn appearance(&self) -> AppearanceMode {
        self.appearance
    }

    pub const fn appearance_is_outside_project_undo_save_and_history(&self) -> bool {
        true
    }

    pub fn categories(&self) -> [SettingsCategoryItem; 5] {
        [
            SettingsCategory::General,
            SettingsCategory::Appearance,
            SettingsCategory::Styles,
            SettingsCategory::Metadata,
            SettingsCategory::Dictionaries,
        ]
        .map(|category| SettingsCategoryItem {
            category,
            label: category.label(),
            selected: self.selected_category == category,
        })
    }

    pub fn dictionaries(&self) -> &DictionarySettingsState {
        &self.dictionaries
    }

    pub fn metadata_fields(&self) -> Vec<MetadataFieldSummary<'_>> {
        self.metadata_order
            .iter()
            .filter_map(|id| {
                self.metadata_definitions
                    .get(id)
                    .map(|field| MetadataFieldSummary {
                        id,
                        label: &field.label,
                        description: field.description.as_deref(),
                        default_value: field.default_value.as_deref(),
                        visible_on_cards: field.visible_on_cards,
                        applicability: field.applicability,
                        text_kind: field.text_kind,
                    })
            })
            .collect()
    }

    pub fn styles(&self) -> Vec<StyleSummary<'_>> {
        self.style_order
            .iter()
            .filter_map(|id| {
                let definition = self.style_definitions.get(id)?;
                Some(StyleSummary {
                    id,
                    display_name: &definition.display_name,
                    role: definition.role,
                    inherits: definition.inherits.as_ref().and_then(|parent| {
                        self.style_order
                            .iter()
                            .find(|candidate| {
                                self.style_definitions
                                    .get(*candidate)
                                    .is_some_and(|style| style.id == *parent)
                            })
                            .map(String::as_str)
                    }),
                    properties: &definition.properties,
                })
            })
            .collect()
    }

    pub fn selected_detail(&self) -> Option<&SettingsDetail> {
        self.selected_detail.as_ref()
    }

    pub const fn selected_category(&self) -> SettingsCategory {
        self.selected_category
    }

    pub fn metadata_drag_source(&self) -> Option<&str> {
        self.metadata_drag_source.as_deref()
    }

    pub const fn metadata_drag_target(&self) -> Option<usize> {
        self.metadata_drag_target
    }

    pub fn metadata_field(&self, id: &str) -> Option<MetadataFieldSummary<'_>> {
        self.metadata_definitions
            .get_key_value(id)
            .map(|(id, field)| MetadataFieldSummary {
                id,
                label: &field.label,
                description: field.description.as_deref(),
                default_value: field.default_value.as_deref(),
                visible_on_cards: field.visible_on_cards,
                applicability: field.applicability,
                text_kind: field.text_kind,
            })
    }

    pub fn style(&self, id: &str) -> Option<StyleSummary<'_>> {
        self.styles().into_iter().find(|style| style.id == id)
    }
}

/// One streamed search result suitable for grouping and navigation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalSearchResult {
    pub document_id: String,
    pub match_id: String,
    pub prefix: String,
    pub matching_text: String,
    pub suffix: String,
    pub indexed_revision: u64,
}

/// Global Search sidebar state.
#[derive(Debug, Clone, Default)]
pub struct GlobalSearchState {
    query: String,
    replacement: String,
    case_sensitive: bool,
    whole_word: bool,
    results: Vec<GlobalSearchResult>,
    query_generation: u64,
    complete: bool,
    error: Option<String>,
    scroll_offset: f32,
}

impl GlobalSearchState {
    pub const fn has_explicit_return_to_explorer(&self) -> bool {
        true
    }

    pub const fn searches_entire_project(&self) -> bool {
        true
    }

    pub const fn has_scope_selector(&self) -> bool {
        false
    }

    pub const fn results_stream(&self) -> bool {
        true
    }

    pub const fn results_are_virtualized(&self) -> bool {
        true
    }

    pub const fn results_are_grouped_by_document(&self) -> bool {
        true
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn replacement(&self) -> &str {
        &self.replacement
    }

    pub const fn case_sensitive(&self) -> bool {
        self.case_sensitive
    }

    pub const fn whole_word(&self) -> bool {
        self.whole_word
    }

    pub fn results(&self) -> &[GlobalSearchResult] {
        &self.results
    }

    pub fn windowed_results(&self) -> impl Iterator<Item = &GlobalSearchResult> {
        let start = self.result_window_start();
        self.results.iter().skip(start).take(80)
    }

    pub fn result_window_start(&self) -> usize {
        (self.scroll_offset.max(0.0) / 44.0) as usize
    }

    pub fn result_window_bottom_padding(&self) -> f32 {
        self.results
            .len()
            .saturating_sub(self.result_window_start().saturating_add(80)) as f32
            * 44.0
    }

    pub const fn query_generation(&self) -> u64 {
        self.query_generation
    }

    pub const fn is_complete(&self) -> bool {
        self.complete
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

/// Three-state hierarchy controls used by replacement review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplacementCheckState {
    Selected,
    Unselected,
    Indeterminate,
}

#[derive(Debug, Clone)]
struct ReplacementNode {
    children: Vec<String>,
    included: bool,
    result: Option<GlobalSearchResult>,
    issue: Option<String>,
}

/// A flattened, accessible row in the replacement-review hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplacementPreviewRowKind {
    AllMatches,
    Document,
    Match,
}

/// Read-only data used to render one replacement-review row.
#[derive(Debug, Clone, Copy)]
pub struct ReplacementPreviewRow<'a> {
    pub node_id: &'a str,
    pub kind: ReplacementPreviewRowKind,
    pub depth: usize,
    pub check_state: ReplacementCheckState,
    pub document_id: Option<&'a str>,
    pub prefix: Option<&'a str>,
    pub matching_text: Option<&'a str>,
    pub suffix: Option<&'a str>,
    pub indexed_revision: Option<u64>,
    pub issue: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ReplacementPreviewValidation {
    Draft,
    Validating,
    Ready,
    Failed(String),
}

/// Hierarchy-shaped global replacement preview.
#[derive(Debug, Clone)]
pub struct ReplacementPreviewState {
    open: bool,
    nodes: BTreeMap<String, ReplacementNode>,
    captured_project_revision: u64,
    captured_query_generation: u64,
    validation: ReplacementPreviewValidation,
}

impl ReplacementPreviewState {
    fn fixture() -> Self {
        Self {
            open: false,
            nodes: BTreeMap::from([
                (
                    "manuscript".to_owned(),
                    ReplacementNode {
                        children: vec!["chapter-one".to_owned(), "chapter-two".to_owned()],
                        included: true,
                        result: None,
                        issue: None,
                    },
                ),
                (
                    "chapter-one".to_owned(),
                    ReplacementNode {
                        children: vec![
                            "chapter-one-match-1".to_owned(),
                            "chapter-one-match-2".to_owned(),
                        ],
                        included: true,
                        result: None,
                        issue: None,
                    },
                ),
                (
                    "chapter-two".to_owned(),
                    ReplacementNode {
                        children: vec!["chapter-two-match-1".to_owned()],
                        included: false,
                        result: None,
                        issue: None,
                    },
                ),
                (
                    "chapter-one-match-1".to_owned(),
                    ReplacementNode {
                        children: Vec::new(),
                        included: true,
                        result: Some(GlobalSearchResult {
                            document_id: "chapter-one".to_owned(),
                            match_id: "chapter-one-match-1".to_owned(),
                            prefix: "beside the ".to_owned(),
                            matching_text: "river".to_owned(),
                            suffix: ", the path".to_owned(),
                            indexed_revision: 1,
                        }),
                        issue: None,
                    },
                ),
                (
                    "chapter-one-match-2".to_owned(),
                    ReplacementNode {
                        children: Vec::new(),
                        included: true,
                        result: Some(GlobalSearchResult {
                            document_id: "chapter-one".to_owned(),
                            match_id: "chapter-one-match-2".to_owned(),
                            prefix: "the ".to_owned(),
                            matching_text: "river".to_owned(),
                            suffix: " turned north".to_owned(),
                            indexed_revision: 1,
                        }),
                        issue: None,
                    },
                ),
                (
                    "chapter-two-match-1".to_owned(),
                    ReplacementNode {
                        children: Vec::new(),
                        included: false,
                        result: Some(GlobalSearchResult {
                            document_id: "chapter-two".to_owned(),
                            match_id: "chapter-two-match-1".to_owned(),
                            prefix: "a ".to_owned(),
                            matching_text: "river".to_owned(),
                            suffix: " below".to_owned(),
                            indexed_revision: 1,
                        }),
                        issue: None,
                    },
                ),
            ]),
            captured_project_revision: 1,
            captured_query_generation: 0,
            validation: ReplacementPreviewValidation::Draft,
        }
    }

    fn prepare(
        &mut self,
        results: &[GlobalSearchResult],
        captured_project_revision: u64,
        captured_query_generation: u64,
    ) {
        let mut documents = BTreeMap::<String, Vec<String>>::new();
        for result in results {
            documents
                .entry(result.document_id.clone())
                .or_default()
                .push(result.match_id.clone());
        }
        self.nodes.clear();
        let document_ids = documents.keys().cloned().collect::<Vec<_>>();
        self.nodes.insert(
            "all-matches".to_owned(),
            ReplacementNode {
                children: document_ids.clone(),
                included: true,
                result: None,
                issue: None,
            },
        );
        for (document, matches) in documents {
            self.nodes.insert(
                document,
                ReplacementNode {
                    children: matches.clone(),
                    included: true,
                    result: None,
                    issue: None,
                },
            );
            for match_id in matches {
                self.nodes.insert(
                    match_id.clone(),
                    ReplacementNode {
                        children: Vec::new(),
                        included: true,
                        result: results
                            .iter()
                            .find(|result| result.match_id == match_id)
                            .cloned(),
                        issue: None,
                    },
                );
            }
        }
        self.captured_project_revision = captured_project_revision;
        self.captured_query_generation = captured_query_generation;
        self.validation = ReplacementPreviewValidation::Validating;
    }

    pub const fn uses_middle_pane(&self) -> bool {
        self.open
    }

    pub fn check_state(&self, node_id: &str) -> ReplacementCheckState {
        let Some(node) = self.nodes.get(node_id) else {
            return ReplacementCheckState::Unselected;
        };
        if node.children.is_empty() {
            return if node.included {
                ReplacementCheckState::Selected
            } else {
                ReplacementCheckState::Unselected
            };
        }
        let states = node
            .children
            .iter()
            .map(|child| self.check_state(child))
            .collect::<Vec<_>>();
        if states
            .iter()
            .all(|state| *state == ReplacementCheckState::Selected)
        {
            ReplacementCheckState::Selected
        } else if states
            .iter()
            .all(|state| *state == ReplacementCheckState::Unselected)
        {
            ReplacementCheckState::Unselected
        } else {
            ReplacementCheckState::Indeterminate
        }
    }

    pub const fn requires_revision_revalidation(&self) -> bool {
        true
    }

    pub const fn captured_project_revision(&self) -> u64 {
        self.captured_project_revision
    }

    pub const fn captured_query_generation(&self) -> u64 {
        self.captured_query_generation
    }

    pub fn validation_error(&self) -> Option<&str> {
        match &self.validation {
            ReplacementPreviewValidation::Failed(error) => Some(error),
            ReplacementPreviewValidation::Draft
            | ReplacementPreviewValidation::Validating
            | ReplacementPreviewValidation::Ready => None,
        }
    }

    pub const fn is_validating(&self) -> bool {
        matches!(self.validation, ReplacementPreviewValidation::Validating)
    }

    pub const fn is_revalidated(&self) -> bool {
        matches!(self.validation, ReplacementPreviewValidation::Ready)
    }

    pub fn can_apply(&self, project_revision: u64) -> bool {
        self.is_revalidated()
            && self.captured_project_revision == project_revision
            && !self.included_match_ids().is_empty()
    }

    pub fn results(&self) -> Vec<GlobalSearchResult> {
        self.nodes
            .values()
            .filter_map(|node| node.result.clone())
            .collect()
    }

    pub fn rows(&self) -> Vec<ReplacementPreviewRow<'_>> {
        let mut rows = Vec::new();
        self.append_rows(
            "all-matches",
            0,
            ReplacementPreviewRowKind::AllMatches,
            &mut rows,
        );
        rows
    }

    fn append_rows<'a>(
        &'a self,
        node_id: &'a str,
        depth: usize,
        kind: ReplacementPreviewRowKind,
        rows: &mut Vec<ReplacementPreviewRow<'a>>,
    ) {
        let Some(node) = self.nodes.get(node_id) else {
            return;
        };
        let result = node.result.as_ref();
        rows.push(ReplacementPreviewRow {
            node_id,
            kind,
            depth,
            check_state: self.check_state(node_id),
            document_id: result.map(|result| result.document_id.as_str()),
            prefix: result.map(|result| result.prefix.as_str()),
            matching_text: result.map(|result| result.matching_text.as_str()),
            suffix: result.map(|result| result.suffix.as_str()),
            indexed_revision: result.map(|result| result.indexed_revision),
            issue: node.issue.as_deref(),
        });
        for child in &node.children {
            self.append_rows(
                child,
                depth + 1,
                if self
                    .nodes
                    .get(child)
                    .is_some_and(|child| child.children.is_empty())
                {
                    ReplacementPreviewRowKind::Match
                } else {
                    ReplacementPreviewRowKind::Document
                },
                rows,
            );
        }
    }

    pub fn included_match_ids(&self) -> Vec<&str> {
        self.nodes
            .iter()
            .filter(|(_, node)| node.children.is_empty() && node.included)
            .map(|(id, _)| id.as_str())
            .collect()
    }

    fn set_included(&mut self, node_id: &str, included: bool) {
        let Some(children) = self.nodes.get(node_id).map(|node| node.children.clone()) else {
            return;
        };
        if children.is_empty() {
            if let Some(node) = self.nodes.get_mut(node_id) {
                node.included = included;
                node.issue = None;
            }
            self.validation = ReplacementPreviewValidation::Draft;
            return;
        }
        for child in children {
            self.set_included(&child, included);
        }
        self.validation = ReplacementPreviewValidation::Draft;
    }

    fn select_all(&mut self, included: bool) {
        self.set_included("all-matches", included);
    }

    fn mark_ready(&mut self, captured_project_revision: u64) {
        self.captured_project_revision = captured_project_revision;
        self.validation = ReplacementPreviewValidation::Ready;
    }

    fn mark_failed(&mut self, error: String) {
        for node in self.nodes.values_mut() {
            if node.children.is_empty() && node.included {
                node.issue = Some(error.clone());
            }
        }
        self.validation = ReplacementPreviewValidation::Failed(error);
    }

    fn close(&mut self) {
        self.open = false;
        self.validation = ReplacementPreviewValidation::Draft;
    }
}

/// Whole-project restore is the only supported History scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HistoryRestoreScope {
    EntireProject,
}

/// A project modal with the context required by its controls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectModal {
    HistoryRestore {
        checkpoint_id: String,
        checkpoint_label: String,
        affected_summary: String,
        scope: HistoryRestoreScope,
    },
    DeleteMetadataField {
        field_id: String,
    },
    DeleteStyle {
        style_id: String,
    },
    ReinitializeHistory,
    /// A user-facing failure. Technical detail remains in the local debug log
    /// so that a backend implementation cannot accidentally expose internals.
    Error {
        title: String,
        detail: String,
    },
}

/// Authoritative category reported for a History checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryCheckpointCategory {
    Autosave,
    ExplicitSave,
    StructuralChange,
    NamedSnapshot,
    Restoration,
}

impl HistoryCheckpointCategory {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Autosave => "Automatic save",
            Self::ExplicitSave => "Saved project",
            Self::StructuralChange => "Project change",
            Self::NamedSnapshot => "Named snapshot",
            Self::Restoration => "Restoration",
        }
    }
}

/// A checkpoint row projected from the History service, without inventing
/// timestamps or document titles the service did not provide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryCheckpointRow {
    pub checkpoint_id: String,
    pub sequence: u64,
    pub category: HistoryCheckpointCategory,
    pub affected_document_ids: Vec<String>,
    pub name: Option<String>,
}

impl HistoryCheckpointRow {
    pub fn label(&self) -> String {
        self.name
            .clone()
            .unwrap_or_else(|| self.category.label().to_owned())
    }

    /// Deterministic display summary for the authoritative affected-document
    /// IDs reported by History.
    pub fn affected_summary(&self) -> String {
        match self.affected_document_ids.len() {
            0 => "No documents".to_owned(),
            1 => "1 document".to_owned(),
            count => format!("{count} documents"),
        }
    }

    /// History has no persisted wall-clock field. The git-backed provider's
    /// synthetic commit time is an identity-stability detail, not display data.
    pub const fn recorded_at_unix_millis(&self) -> Option<u64> {
        None
    }
}

/// The manifest and optional exact document content for a selected checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryPreviewData {
    pub checkpoint: HistoryCheckpointRow,
    pub resource_paths: Vec<String>,
    pub document: Option<HistoryDocumentPreview>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryDocumentPreview {
    pub document_id: String,
    pub canonical_path: String,
    pub semantic: SemanticDocument,
}

/// Current-document facts that can safely be shown beside a checkpoint
/// manifest. The checkpoint body stays unavailable until History exposes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryCurrentDocument {
    pub document_id: String,
    pub title: String,
    pub body: String,
    pub semantic: SemanticDocument,
}

/// How one aligned before/after row changed between a checkpoint and the
/// current canonical semantic document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryComparisonLineKind {
    Unchanged,
    Added,
    Removed,
    Modified,
}

/// How one text span participates in a comparison row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryComparisonSpanKind {
    Unchanged,
    Added,
    Removed,
}

/// One contiguous text span within a comparison line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryComparisonSpan {
    pub kind: HistoryComparisonSpanKind,
    pub text: String,
}

/// One numbered line on either side of a History comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryComparisonTextLine {
    pub line_number: usize,
    pub spans: Vec<HistoryComparisonSpan>,
}

/// One aligned, line-numbered checkpoint/current comparison row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryComparisonLine {
    pub kind: HistoryComparisonLineKind,
    pub before: Option<HistoryComparisonTextLine>,
    pub after: Option<HistoryComparisonTextLine>,
}

/// Deterministic counts derived from the typed comparison rows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HistoryChangeSummary {
    pub added_lines: usize,
    pub removed_lines: usize,
    pub modified_lines: usize,
}

/// Read-only comparison of one exact checkpoint document with its loaded
/// current counterpart. It carries no mutation or persistence semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryComparison {
    pub checkpoint_id: String,
    pub document_id: String,
    pub document_title: String,
    pub lines: Vec<HistoryComparisonLine>,
}

impl HistoryComparison {
    pub fn change_summary(&self) -> HistoryChangeSummary {
        self.lines
            .iter()
            .fold(HistoryChangeSummary::default(), |mut summary, line| {
                match line.kind {
                    HistoryComparisonLineKind::Added => summary.added_lines += 1,
                    HistoryComparisonLineKind::Removed => summary.removed_lines += 1,
                    HistoryComparisonLineKind::Modified => summary.modified_lines += 1,
                    HistoryComparisonLineKind::Unchanged => {}
                }
                summary
            })
    }
}

/// History list/detail presentation facts.
#[derive(Debug, Clone, Default)]
pub struct HistoryState {
    checkpoints: Vec<HistoryCheckpointRow>,
    active_document_filter: Option<String>,
    selected_checkpoint_id: Option<String>,
    preview: Option<HistoryPreviewData>,
    current_document: Option<HistoryCurrentDocument>,
    comparison: Option<HistoryComparison>,
    named_snapshot_draft: String,
    creating_named_snapshot: bool,
    error: Option<String>,
    next_cursor: Option<String>,
    loading_more: bool,
    scroll_offset: f32,
    maintenance: HistoryMaintenanceStatus,
    maintenance_message: Option<String>,
}

impl HistoryState {
    pub const fn is_virtualized(&self) -> bool {
        true
    }

    pub const fn comparison_is_side_by_side(&self) -> bool {
        true
    }

    pub const fn has_separate_preview_button(&self) -> bool {
        false
    }

    pub fn checkpoints(&self) -> &[HistoryCheckpointRow] {
        &self.checkpoints
    }

    pub fn visible_checkpoints(&self) -> impl Iterator<Item = &HistoryCheckpointRow> {
        self.checkpoints.iter().filter(|checkpoint| {
            self.active_document_filter
                .as_ref()
                .is_none_or(|document_id| {
                    checkpoint
                        .affected_document_ids
                        .iter()
                        .any(|affected| affected == document_id)
                })
        })
    }

    pub fn windowed_checkpoints(&self) -> impl Iterator<Item = &HistoryCheckpointRow> {
        let start = (self.scroll_offset.max(0.0) / 72.0) as usize;
        self.visible_checkpoints().skip(start).take(60)
    }

    pub fn checkpoint_window_start(&self) -> usize {
        (self.scroll_offset.max(0.0) / 72.0) as usize
    }

    pub fn checkpoint_window_bottom_padding(&self) -> f32 {
        self.visible_checkpoints()
            .count()
            .saturating_sub(self.checkpoint_window_start().saturating_add(60)) as f32
            * 72.0
    }

    pub fn active_document_filter(&self) -> Option<&str> {
        self.active_document_filter.as_deref()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn selected_checkpoint_id(&self) -> Option<&str> {
        self.selected_checkpoint_id.as_deref()
    }

    pub fn preview(&self) -> Option<&HistoryPreviewData> {
        self.preview.as_ref()
    }

    pub fn current_document(&self) -> Option<&HistoryCurrentDocument> {
        self.current_document.as_ref()
    }

    /// Builds a typed comparison only when the selected checkpoint preview and
    /// current presentation facts refer to the same loaded document.
    pub fn comparison(&self) -> Option<&HistoryComparison> {
        self.comparison.as_ref()
    }

    fn refresh_comparison(&mut self) {
        let Some(preview) = self.preview.as_ref() else {
            self.comparison = None;
            return;
        };
        let Some(before) = preview.document.as_ref() else {
            self.comparison = None;
            return;
        };
        let Some(after) = self.current_document.as_ref() else {
            self.comparison = None;
            return;
        };
        if before.document_id != after.document_id {
            self.comparison = None;
            return;
        }
        self.comparison = Some(compare_history_documents(
            &preview.checkpoint.checkpoint_id,
            before,
            after,
        ));
    }

    pub fn named_snapshot_draft(&self) -> &str {
        &self.named_snapshot_draft
    }

    pub const fn is_creating_named_snapshot(&self) -> bool {
        self.creating_named_snapshot
    }

    pub fn next_cursor(&self) -> Option<&str> {
        self.next_cursor.as_deref()
    }

    pub const fn is_loading_more(&self) -> bool {
        self.loading_more
    }

    pub fn maintenance(&self) -> &HistoryMaintenanceStatus {
        &self.maintenance
    }

    pub fn maintenance_message(&self) -> Option<&str> {
        self.maintenance_message.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HistoryLineEdit {
    Unchanged(String),
    Added(String),
    Removed(String),
}

fn compare_history_documents(
    checkpoint_id: &str,
    before: &HistoryDocumentPreview,
    after: &HistoryCurrentDocument,
) -> HistoryComparison {
    let before_text = before.semantic.plain_text();
    let after_text = after.semantic.plain_text();
    let before_lines = semantic_lines(&before.semantic, &before_text);
    let after_lines = semantic_lines(&after.semantic, &after_text);
    let edits = history_line_edits(&before_lines, &after_lines);
    HistoryComparison {
        checkpoint_id: checkpoint_id.to_owned(),
        document_id: before.document_id.clone(),
        document_title: after.title.clone(),
        lines: comparison_rows(edits),
    }
}

fn semantic_lines<'a>(semantic: &SemanticDocument, text: &'a str) -> Vec<&'a str> {
    if semantic.blocks().is_empty() {
        Vec::new()
    } else {
        text.split('\n').collect()
    }
}

fn history_line_edits(before: &[&str], after: &[&str]) -> Vec<HistoryLineEdit> {
    const MAX_LCS_CELLS: usize = 100_000;
    let rows = before.len().saturating_add(1);
    let columns = after.len().saturating_add(1);
    if rows
        .checked_mul(columns)
        .is_none_or(|cells| cells > MAX_LCS_CELLS)
    {
        return bounded_history_line_edits(before, after);
    }

    let mut lengths = vec![0_usize; rows * columns];
    for before_index in (0..before.len()).rev() {
        for after_index in (0..after.len()).rev() {
            let index = before_index * columns + after_index;
            lengths[index] = if before[before_index] == after[after_index] {
                lengths[(before_index + 1) * columns + after_index + 1] + 1
            } else {
                lengths[(before_index + 1) * columns + after_index]
                    .max(lengths[before_index * columns + after_index + 1])
            };
        }
    }

    let mut edits = Vec::with_capacity(before.len().max(after.len()));
    let (mut before_index, mut after_index) = (0, 0);
    while before_index < before.len() && after_index < after.len() {
        if before[before_index] == after[after_index] {
            edits.push(HistoryLineEdit::Unchanged(before[before_index].to_owned()));
            before_index += 1;
            after_index += 1;
        } else if lengths[(before_index + 1) * columns + after_index]
            >= lengths[before_index * columns + after_index + 1]
        {
            edits.push(HistoryLineEdit::Removed(before[before_index].to_owned()));
            before_index += 1;
        } else {
            edits.push(HistoryLineEdit::Added(after[after_index].to_owned()));
            after_index += 1;
        }
    }
    edits.extend(
        before[before_index..]
            .iter()
            .map(|line| HistoryLineEdit::Removed((*line).to_owned())),
    );
    edits.extend(
        after[after_index..]
            .iter()
            .map(|line| HistoryLineEdit::Added((*line).to_owned())),
    );
    edits
}

/// Keeps memory bounded for unusually large documents while preserving exact
/// shared prefix/suffix lines and honestly marking the entire middle changed.
fn bounded_history_line_edits(before: &[&str], after: &[&str]) -> Vec<HistoryLineEdit> {
    let prefix = before
        .iter()
        .zip(after)
        .take_while(|(before, after)| before == after)
        .count();
    let suffix = before[prefix..]
        .iter()
        .rev()
        .zip(after[prefix..].iter().rev())
        .take_while(|(before, after)| before == after)
        .count();
    let mut edits = before[..prefix]
        .iter()
        .map(|line| HistoryLineEdit::Unchanged((*line).to_owned()))
        .collect::<Vec<_>>();
    edits.extend(
        before[prefix..before.len() - suffix]
            .iter()
            .map(|line| HistoryLineEdit::Removed((*line).to_owned())),
    );
    edits.extend(
        after[prefix..after.len() - suffix]
            .iter()
            .map(|line| HistoryLineEdit::Added((*line).to_owned())),
    );
    edits.extend(
        before[before.len() - suffix..]
            .iter()
            .map(|line| HistoryLineEdit::Unchanged((*line).to_owned())),
    );
    edits
}

fn comparison_rows(edits: Vec<HistoryLineEdit>) -> Vec<HistoryComparisonLine> {
    let mut rows = Vec::with_capacity(edits.len());
    let (mut before_line_number, mut after_line_number) = (1, 1);
    let mut index = 0;
    while index < edits.len() {
        if let HistoryLineEdit::Unchanged(text) = &edits[index] {
            rows.push(HistoryComparisonLine {
                kind: HistoryComparisonLineKind::Unchanged,
                before: Some(comparison_text_line(
                    before_line_number,
                    text,
                    HistoryComparisonSpanKind::Unchanged,
                )),
                after: Some(comparison_text_line(
                    after_line_number,
                    text,
                    HistoryComparisonSpanKind::Unchanged,
                )),
            });
            before_line_number += 1;
            after_line_number += 1;
            index += 1;
            continue;
        }

        let chunk_start = index;
        while index < edits.len() && !matches!(&edits[index], HistoryLineEdit::Unchanged(_)) {
            index += 1;
        }
        let removed = edits[chunk_start..index]
            .iter()
            .filter_map(|edit| match edit {
                HistoryLineEdit::Removed(text) => Some(text.as_str()),
                HistoryLineEdit::Added(_) | HistoryLineEdit::Unchanged(_) => None,
            })
            .collect::<Vec<_>>();
        let added = edits[chunk_start..index]
            .iter()
            .filter_map(|edit| match edit {
                HistoryLineEdit::Added(text) => Some(text.as_str()),
                HistoryLineEdit::Removed(_) | HistoryLineEdit::Unchanged(_) => None,
            })
            .collect::<Vec<_>>();
        for pair_index in 0..removed.len().max(added.len()) {
            let before_text = removed.get(pair_index).copied();
            let after_text = added.get(pair_index).copied();
            let (kind, before_spans, after_spans) = match (before_text, after_text) {
                (Some(before), Some(after)) => {
                    let (before_spans, after_spans) = modified_history_spans(before, after);
                    (
                        HistoryComparisonLineKind::Modified,
                        Some(before_spans),
                        Some(after_spans),
                    )
                }
                (Some(before), None) => (
                    HistoryComparisonLineKind::Removed,
                    Some(vec![history_span(
                        HistoryComparisonSpanKind::Removed,
                        before,
                    )]),
                    None,
                ),
                (None, Some(after)) => (
                    HistoryComparisonLineKind::Added,
                    None,
                    Some(vec![history_span(HistoryComparisonSpanKind::Added, after)]),
                ),
                (None, None) => unreachable!("comparison chunk must contain a line"),
            };
            let before = before_spans.map(|spans| {
                let line = HistoryComparisonTextLine {
                    line_number: before_line_number,
                    spans,
                };
                before_line_number += 1;
                line
            });
            let after = after_spans.map(|spans| {
                let line = HistoryComparisonTextLine {
                    line_number: after_line_number,
                    spans,
                };
                after_line_number += 1;
                line
            });
            rows.push(HistoryComparisonLine {
                kind,
                before,
                after,
            });
        }
    }
    rows
}

fn comparison_text_line(
    line_number: usize,
    text: &str,
    kind: HistoryComparisonSpanKind,
) -> HistoryComparisonTextLine {
    HistoryComparisonTextLine {
        line_number,
        spans: vec![history_span(kind, text)],
    }
}

fn modified_history_spans(
    before: &str,
    after: &str,
) -> (Vec<HistoryComparisonSpan>, Vec<HistoryComparisonSpan>) {
    let prefix_bytes = before
        .chars()
        .zip(after.chars())
        .take_while(|(before, after)| before == after)
        .map(|(character, _)| character.len_utf8())
        .sum::<usize>();
    let prefix_bytes = history_word_start(before, prefix_bytes);
    let before_rest = &before[prefix_bytes..];
    let after_rest = &after[prefix_bytes..];
    let suffix_bytes = before_rest
        .chars()
        .rev()
        .zip(after_rest.chars().rev())
        .take_while(|(before, after)| before == after)
        .map(|(character, _)| character.len_utf8())
        .sum::<usize>();
    let before_changed_end = history_word_end(before, before.len() - suffix_bytes);
    let after_changed_end = history_word_end(after, after.len() - suffix_bytes);
    (
        history_change_spans(
            before,
            prefix_bytes,
            before_changed_end,
            HistoryComparisonSpanKind::Removed,
        ),
        history_change_spans(
            after,
            prefix_bytes,
            after_changed_end,
            HistoryComparisonSpanKind::Added,
        ),
    )
}

fn history_word_start(text: &str, mut boundary: usize) -> usize {
    while boundary > 0 && boundary < text.len() {
        let before = text[..boundary].chars().next_back();
        let after = text[boundary..].chars().next();
        if !before.is_some_and(history_word_character) || !after.is_some_and(history_word_character)
        {
            break;
        }
        boundary = text[..boundary]
            .char_indices()
            .next_back()
            .map_or(0, |(index, _)| index);
    }
    boundary
}

fn history_word_end(text: &str, mut boundary: usize) -> usize {
    while boundary > 0 && boundary < text.len() {
        let before = text[..boundary].chars().next_back();
        let after = text[boundary..].chars().next();
        if !before.is_some_and(history_word_character) || !after.is_some_and(history_word_character)
        {
            break;
        }
        boundary += after
            .expect("word boundary includes a following character")
            .len_utf8();
    }
    boundary
}

fn history_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn history_change_spans(
    text: &str,
    prefix_end: usize,
    changed_end: usize,
    changed_kind: HistoryComparisonSpanKind,
) -> Vec<HistoryComparisonSpan> {
    let mut spans = Vec::with_capacity(3);
    if prefix_end > 0 {
        spans.push(history_span(
            HistoryComparisonSpanKind::Unchanged,
            &text[..prefix_end],
        ));
    }
    if changed_end > prefix_end {
        spans.push(history_span(changed_kind, &text[prefix_end..changed_end]));
    }
    if changed_end < text.len() {
        spans.push(history_span(
            HistoryComparisonSpanKind::Unchanged,
            &text[changed_end..],
        ));
    }
    spans
}

fn history_span(kind: HistoryComparisonSpanKind, text: &str) -> HistoryComparisonSpan {
    HistoryComparisonSpan {
        kind,
        text: text.to_owned(),
    }
}

/// Where a deleted subtree can be restored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreLocation {
    FormerParent(String),
    SectionRoot(String),
}

#[derive(Debug, Clone)]
struct DeletedItem {
    title: String,
    section_id: String,
    kind: HierarchyRowKind,
    former: RestoreLocation,
    former_index: usize,
    location: RestoreLocation,
    fallback: RestoreLocation,
    deleted_at_unix_millis: u64,
    restoring_checkpoint_id: Option<String>,
    preview_document_id: Option<String>,
    preview_document_title: Option<String>,
    preview: Option<DeletedDocumentPreview>,
}

#[derive(Debug, Clone)]
struct DeletedDocumentPreview {
    document_id: String,
    title: String,
    semantic: SemanticDocument,
}

/// One Recently Deleted item in deterministic tombstone-ID order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentlyDeletedItem<'a> {
    pub node_id: &'a str,
    pub title: &'a str,
    pub section_id: &'a str,
    pub kind: HierarchyRowKind,
    pub former_location: &'a RestoreLocation,
    pub former_index: usize,
    pub restore_location: &'a RestoreLocation,
    pub fallback_location: &'a RestoreLocation,
    pub deleted_at_unix_millis: u64,
    pub restoring_checkpoint_id: Option<&'a str>,
    pub preview_document_id: Option<&'a str>,
    pub formatted_preview_available: bool,
}

/// Canonical, read-only content for the selected Recently Deleted item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentlyDeletedPreview<'a> {
    pub document_id: &'a str,
    pub title: &'a str,
    pub semantic: &'a SemanticDocument,
}

/// Recently Deleted list/detail presentation state.
#[derive(Debug, Clone)]
pub struct RecentlyDeletedState {
    items: BTreeMap<String, DeletedItem>,
    default_root: String,
    selected_item_id: Option<String>,
}

impl RecentlyDeletedState {
    fn fixture() -> Self {
        Self {
            items: BTreeMap::from([(
                "deleted-part".to_owned(),
                DeletedItem {
                    title: "Deleted Part".to_owned(),
                    section_id: "manuscript".to_owned(),
                    kind: HierarchyRowKind::Group,
                    former: RestoreLocation::FormerParent("part-one".to_owned()),
                    former_index: 0,
                    location: RestoreLocation::FormerParent("part-one".to_owned()),
                    fallback: RestoreLocation::SectionRoot("manuscript".to_owned()),
                    deleted_at_unix_millis: 0,
                    restoring_checkpoint_id: None,
                    preview_document_id: Some("deleted-part".to_owned()),
                    preview_document_title: Some("Deleted Part".to_owned()),
                    preview: Some(DeletedDocumentPreview {
                        document_id: "deleted-part".to_owned(),
                        title: "Deleted Part".to_owned(),
                        semantic: SemanticDocument::default(),
                    }),
                },
            )]),
            default_root: "manuscript".to_owned(),
            selected_item_id: Some("deleted-part".to_owned()),
        }
    }

    fn from_snapshot(snapshot: &ProjectSnapshot) -> Self {
        let project = &snapshot.project;
        let documents = snapshot
            .documents
            .iter()
            .map(|document| (document.document_id, document))
            .collect::<BTreeMap<_, _>>();
        let mut items = BTreeMap::new();
        for (node_id, tombstone) in &project.deleted {
            let id = stable_id_string(node_id.as_bytes());
            let former_parent = stable_id_string(tombstone.former_parent.as_bytes());
            let section_id = stable_id_string(tombstone.section.root_id().as_bytes());
            let fallback = RestoreLocation::SectionRoot(section_id.clone());
            let former = RestoreLocation::FormerParent(former_parent.clone());
            let former_is_live_container = project
                .nodes
                .get(tombstone.former_parent)
                .is_some_and(|node| node.kind.can_have_children());
            let location = if former_is_live_container {
                former.clone()
            } else {
                fallback.clone()
            };
            let preview_candidate = tombstone.subtree.iter().find_map(|deleted| {
                let NodeKind::Document(document_id) = deleted.node.kind else {
                    return None;
                };
                Some((document_id, deleted.node.title.clone()))
            });
            let preview = preview_candidate.as_ref().and_then(|(document_id, title)| {
                let document = documents.get(document_id)?;
                let semantic = EditorCoreSession::open(CanonicalDocumentLoad::new(
                    document.document_id,
                    document.body.clone(),
                ))
                .ok()?
                .canonical_projection()
                .semantic()
                .clone();
                Some(DeletedDocumentPreview {
                    document_id: stable_id_string(document_id.as_bytes()),
                    title: title.clone(),
                    semantic,
                })
            });
            let preview_document_id = preview_candidate
                .as_ref()
                .map(|(document, _)| stable_id_string(document.as_bytes()));
            let preview_document_title = preview_candidate.map(|(_, title)| title);
            items.insert(
                id,
                DeletedItem {
                    title: tombstone.title.clone(),
                    section_id,
                    kind: match tombstone.kind {
                        NodeKind::Root(_) => HierarchyRowKind::Root,
                        NodeKind::Group => HierarchyRowKind::Group,
                        NodeKind::Document(_) => HierarchyRowKind::Document,
                    },
                    former,
                    former_index: tombstone.former_index,
                    location,
                    fallback,
                    deleted_at_unix_millis: tombstone.deleted_at_unix_millis,
                    restoring_checkpoint_id: tombstone
                        .restoring_checkpoint
                        .map(|id| stable_id_string(id.as_bytes())),
                    preview_document_id,
                    preview_document_title,
                    preview,
                },
            );
        }
        let selected_item_id = selected_item_id(&items);
        Self {
            items,
            default_root: stable_id_string(ProjectSection::Manuscript.root_id().as_bytes()),
            selected_item_id,
        }
    }

    fn reconcile_snapshot(&mut self, snapshot: &ProjectSnapshot) {
        let mut authoritative = Self::from_snapshot(snapshot);
        for (id, item) in &mut authoritative.items {
            if let Some(previous) = self.items.get(id)
                && previous.location == previous.fallback
            {
                item.location = item.fallback.clone();
            }
        }
        if self
            .selected_item_id
            .as_ref()
            .is_some_and(|id| authoritative.items.contains_key(id))
        {
            authoritative.selected_item_id = self.selected_item_id.clone();
        }
        *self = authoritative;
    }

    pub fn has_formatted_preview(&self) -> bool {
        self.items.values().any(|item| item.preview.is_some())
    }

    pub fn restore_location(&self, node_id: &str) -> RestoreLocation {
        self.items
            .get(node_id)
            .map(|item| item.location.clone())
            .unwrap_or_else(|| RestoreLocation::SectionRoot(self.default_root.clone()))
    }

    pub fn items(&self) -> Vec<RecentlyDeletedItem<'_>> {
        let mut items = self
            .items
            .iter()
            .map(|(id, item)| RecentlyDeletedItem {
                node_id: id,
                title: &item.title,
                section_id: &item.section_id,
                kind: item.kind,
                former_location: &item.former,
                former_index: item.former_index,
                restore_location: &item.location,
                fallback_location: &item.fallback,
                deleted_at_unix_millis: item.deleted_at_unix_millis,
                restoring_checkpoint_id: item.restoring_checkpoint_id.as_deref(),
                preview_document_id: item.preview_document_id.as_deref(),
                formatted_preview_available: item.preview.is_some(),
            })
            .collect::<Vec<_>>();
        items.sort_by(|left, right| {
            right
                .deleted_at_unix_millis
                .cmp(&left.deleted_at_unix_millis)
                .then_with(|| left.node_id.cmp(right.node_id))
        });
        items
    }

    pub fn formatted_preview_available(&self, node_id: &str) -> bool {
        self.items
            .get(node_id)
            .is_some_and(|item| item.preview.is_some())
    }

    pub fn selected_item_id(&self) -> Option<&str> {
        self.selected_item_id.as_deref()
    }

    pub fn selected_preview(&self) -> Option<RecentlyDeletedPreview<'_>> {
        let item = self.items.get(self.selected_item_id.as_deref()?)?;
        let preview = item.preview.as_ref()?;
        Some(RecentlyDeletedPreview {
            document_id: &preview.document_id,
            title: &preview.title,
            semantic: &preview.semantic,
        })
    }

    pub const fn has_purge_action(&self) -> bool {
        false
    }

    fn use_fallback(&mut self, node_id: &str) {
        if let Some(item) = self.items.get_mut(node_id) {
            item.location = item.fallback.clone();
        }
    }

    fn select(&mut self, node_id: String) {
        if self.items.contains_key(&node_id) {
            self.selected_item_id = Some(node_id);
        }
    }

    fn set_preview(
        &mut self,
        node_id: &str,
        document_id: &str,
        semantic: SemanticDocument,
    ) -> bool {
        let Some(item) = self.items.get_mut(node_id) else {
            return false;
        };
        if item.preview_document_id.as_deref() != Some(document_id) {
            return false;
        }
        let Some(title) = item.preview_document_title.clone() else {
            return false;
        };
        item.preview = Some(DeletedDocumentPreview {
            document_id: document_id.to_owned(),
            title,
            semantic,
        });
        true
    }
}

fn selected_item_id(items: &BTreeMap<String, DeletedItem>) -> Option<String> {
    items
        .iter()
        .max_by(|(left_id, left), (right_id, right)| {
            left.deleted_at_unix_millis
                .cmp(&right.deleted_at_unix_millis)
                .then_with(|| right_id.cmp(left_id))
        })
        .map(|(id, _)| id.clone())
}

/// Export progress and terminal presentation states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportState {
    Ready,
    ChoosingDestination,
    Planning,
    Exporting { completed: u64, total: u64 },
    Committing,
    Cancelling,
    Succeeded { artifact: ExportArtifact },
    Cancelled,
    Failed(String),
}

/// Fixed whole-manuscript export controls and feedback.
#[derive(Debug, Clone)]
pub struct ExportViewState {
    state: ExportState,
    output_name: String,
    numbering_documents: bool,
    project_settings: ProjectExportSettings,
    node_settings: BTreeMap<String, ProjectExportSettings>,
    destination: Option<String>,
}

impl Default for ExportViewState {
    fn default() -> Self {
        Self {
            state: ExportState::Ready,
            output_name: "manuscript.html".to_owned(),
            numbering_documents: false,
            project_settings: ProjectExportSettings::default(),
            node_settings: BTreeMap::new(),
            destination: None,
        }
    }
}

impl ExportViewState {
    pub const fn is_entire_manuscript_only(&self) -> bool {
        true
    }

    pub const fn has_partial_inclusion_controls(&self) -> bool {
        false
    }

    pub fn state(&self) -> ExportState {
        self.state.clone()
    }

    pub fn output_name(&self) -> &str {
        &self.output_name
    }

    pub fn destination(&self) -> Option<&str> {
        self.destination.as_deref()
    }

    pub const fn numbers_documents(&self) -> bool {
        self.numbering_documents
    }

    pub const fn can_open_result(&self) -> bool {
        matches!(self.state, ExportState::Succeeded { .. })
    }

    pub const fn can_cancel(&self) -> bool {
        matches!(
            self.state,
            ExportState::Planning | ExportState::Exporting { .. } | ExportState::Committing
        )
    }

    pub const fn can_start(&self) -> bool {
        self.destination.is_some()
            && matches!(
                self.state,
                ExportState::Ready
                    | ExportState::Succeeded { .. }
                    | ExportState::Cancelled
                    | ExportState::Failed(_)
            )
    }

    pub const fn can_reveal_result(&self) -> bool {
        self.can_open_result()
    }

    pub const fn project_settings(&self) -> ProjectExportSettings {
        self.project_settings
    }

    pub fn node_settings(&self, node_id: &str) -> Option<ProjectExportSettings> {
        self.node_settings.get(node_id).copied()
    }

    fn reconcile_project(&mut self, project: &Project) {
        self.project_settings = project.export_settings;
        self.node_settings = project
            .nodes
            .iter()
            .map(|(id, node)| (stable_id_string(id.as_bytes()), node.export_settings))
            .collect();
    }
}

/// Save frontier shown in the project status bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveState {
    SavedThrough(u64),
    Dirty { current_revision: u64 },
    Saving { through_revision: u64 },
    Error(String),
}

/// Nonblocking save, recovery, and close presentation state.
#[derive(Debug, Clone)]
pub struct SaveViewState {
    state: SaveState,
    recovery_intact: bool,
    close_waiting: bool,
}

impl SaveViewState {
    fn fixture() -> Self {
        Self {
            state: SaveState::SavedThrough(1),
            recovery_intact: true,
            close_waiting: false,
        }
    }

    pub fn state(&self) -> SaveState {
        self.state.clone()
    }

    pub const fn editing_remains_available(&self) -> bool {
        true
    }

    pub const fn recovery_remains_intact(&self) -> bool {
        self.recovery_intact
    }

    pub const fn claims_saved(&self) -> bool {
        matches!(self.state, SaveState::SavedThrough(_))
    }

    pub const fn close_is_waiting_for_retry_or_cancel(&self) -> bool {
        self.close_waiting
    }
}

/// Full-area content states that retain their surrounding workspace context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentState {
    Ready,
    Empty,
    Loading,
    Error(String),
    Recovery,
}

impl ContentState {
    pub const fn uses_full_available_content_area(&self) -> bool {
        true
    }
}

/// Recovery-choice presentation state.
#[derive(Debug, Clone)]
pub struct RecoveryState {
    accepted: bool,
    durable_save_completed: bool,
    accepted_records: usize,
    affected_documents: Vec<(String, u64)>,
    isolation: Option<String>,
    error: Option<String>,
    resolving: bool,
}

/// One document affected by recovery. Recovery reconciliation reports a
/// document revision, but it does not report recovered word counts or edit
/// timestamps, so those values remain unavailable instead of being inferred.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryDocumentSummary<'a> {
    pub document_id: &'a str,
    pub display_title: Option<&'a str>,
    pub recovered_word_count: Option<usize>,
    pub last_edit: Option<&'a str>,
    pub revision: u64,
}

/// Whether recovery can honestly confirm the History note shown by a surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryHistoryPreservation {
    Unavailable,
}

impl RecoveryState {
    pub const fn is_disposable_after_durable_save(&self) -> bool {
        self.accepted && self.durable_save_completed
    }

    pub const fn durable_save_completed(&self) -> bool {
        self.durable_save_completed
    }

    pub const fn accepted_records(&self) -> usize {
        self.accepted_records
    }

    pub fn affected_documents(&self) -> &[(String, u64)] {
        &self.affected_documents
    }

    pub fn isolation(&self) -> Option<&str> {
        self.isolation.as_deref()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub const fn is_resolving(&self) -> bool {
        self.resolving
    }

    pub const fn history_preservation(&self) -> RecoveryHistoryPreservation {
        RecoveryHistoryPreservation::Unavailable
    }
}

/// Project operations that can complete asynchronously.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProjectTask {
    GlobalSearch {
        generation: u64,
    },
    ReplacementPreview,
    ApplyReplacement,
    LoadHistory,
    PreviewHistory {
        checkpoint_id: String,
    },
    PreviewDeleted {
        node_id: String,
        checkpoint_id: String,
        document_id: String,
    },
    RestoreHistory {
        checkpoint_id: String,
    },
    RestoreDeleted {
        node_id: String,
    },
    Save {
        through_revision: u64,
    },
    Export {
        source_revision: u64,
    },
    ReconcileRecovery,
    AcceptRecovery,
    DiscardRecovery,
    PersistWorkspace,
}

fn project_task_name(task: &ProjectTask) -> &'static str {
    match task {
        ProjectTask::GlobalSearch { .. } => "search",
        ProjectTask::ReplacementPreview | ProjectTask::ApplyReplacement => "replacement",
        ProjectTask::LoadHistory
        | ProjectTask::PreviewHistory { .. }
        | ProjectTask::RestoreHistory { .. } => "history",
        ProjectTask::PreviewDeleted { .. } | ProjectTask::RestoreDeleted { .. } => {
            "recently-deleted"
        }
        ProjectTask::Save { .. } => "save",
        ProjectTask::Export { .. } => "export",
        ProjectTask::ReconcileRecovery
        | ProjectTask::AcceptRecovery
        | ProjectTask::DiscardRecovery => "recovery",
        ProjectTask::PersistWorkspace => "workspace",
    }
}

fn project_error_title(operation: &str) -> &'static str {
    match operation {
        "save" => "Couldn't save changes",
        "export" => "Couldn't export your project",
        "history" => "Couldn't complete the History request",
        "recovery" => "Couldn't recover unsaved changes",
        "search" => "Couldn't search this project",
        "replacement" => "Couldn't replace project text",
        "recently-deleted" => "Couldn't restore that item",
        "editor" => "Couldn't update the editor",
        _ => "Couldn't complete that action",
    }
}

fn project_error_detail(operation: &str) -> &'static str {
    match operation {
        "save" => "Please try again. Your project remains open and your recovery data is intact.",
        "recovery" => {
            "Please try again. ParchMint has kept the project open so you can choose how to proceed."
        }
        _ => "Please try again. ParchMint wrote technical details to its local debug log.",
    }
}

/// Exact identity for delayed work in one live project session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTaskTicket {
    session: u64,
    task: ProjectTask,
    request: u64,
    captured_project_revision: u64,
}

impl ProjectTaskTicket {
    pub const fn session(&self) -> u64 {
        self.session
    }

    pub fn task(&self) -> &ProjectTask {
        &self.task
    }

    pub const fn request(&self) -> u64 {
        self.request
    }

    pub const fn captured_project_revision(&self) -> u64 {
        self.captured_project_revision
    }
}

/// Typed delayed payloads accepted only for their matching task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectTaskPayload {
    SearchBatch {
        results: Vec<GlobalSearchResult>,
        finished: bool,
    },
    ReplacementPreviewReady,
    ReplacementApplied {
        revision: u64,
    },
    HistoryLoaded {
        checkpoints: Vec<HistoryCheckpointRow>,
    },
    HistoryPreviewReady {
        preview: HistoryPreviewData,
    },
    DeletedPreviewReady {
        node_id: String,
        checkpoint_id: String,
        document_id: String,
        semantic: SemanticDocument,
    },
    HistoryRestored {
        revision: u64,
    },
    DeletedRestored {
        revision: u64,
    },
    SavedThrough(u64),
    ExportProgress {
        completed: u64,
        total: u64,
    },
    ExportSucceeded {
        artifact: ExportArtifact,
    },
    ExportPlanning,
    ExportCommitting,
    ExportCancelled,
    RecoveryAccepted {
        revision: u64,
    },
    RecoveryCanceled,
    RecoveryUnavailable,
    RecoveryAvailable {
        accepted_records: usize,
        affected_documents: Vec<(String, u64)>,
        isolation: Option<String>,
    },
    RecoveryDiscarded {
        revision: u64,
    },
    WorkspacePersisted,
    Failed(String),
}

/// A delayed project completion carrying its full request identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTaskCompletion {
    ticket: ProjectTaskTicket,
    payload: ProjectTaskPayload,
}

impl ProjectTaskCompletion {
    pub fn for_ticket(ticket: ProjectTaskTicket, payload: ProjectTaskPayload) -> Self {
        Self { ticket, payload }
    }

    pub fn ticket(&self) -> &ProjectTaskTicket {
        &self.ticket
    }

    pub fn payload(&self) -> &ProjectTaskPayload {
        &self.payload
    }
}

/// Widget messages at the project-workspace boundary.
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectMessage {
    ShowExplorer,
    ShowGlobalSearch,
    SelectHierarchy {
        node_id: String,
        gesture: SelectionGesture,
    },
    ToggleHierarchyExpanded(String),
    RequestCreateHierarchy {
        parent_id: String,
        kind: HierarchyItemKind,
    },
    DeleteSelection,
    PreviewHierarchyNode(String),
    OpenHierarchyNode(String),
    OpenHierarchyNodeInCompanion(String),
    RenameNode {
        node_id: String,
        title: String,
    },
    BeginHierarchyRename(String),
    SetHierarchyRenameDraft(String),
    CommitHierarchyRename,
    CancelHierarchyRename,
    SetSynopsis {
        node_id: String,
        synopsis: String,
    },
    /// Applies one local multiline-editor action before emitting the existing
    /// synopsis persistence effect.
    EditSynopsis {
        node_id: String,
        action: text_editor::Action,
    },
    SetMetadataValue {
        node_id: String,
        field_id: String,
        value: String,
    },
    /// Compatibility intent for the two applicability controls. This cannot
    /// produce a zero-target definition: clearing the final target is ignored.
    SetMetadataApplicability {
        field_id: String,
        applies_to_documents: bool,
    },
    SelectMetadataField(String),
    CreateMetadataField,
    UpdateMetadataField {
        field_id: String,
        label: String,
        description: Option<String>,
        applicability: MetadataFieldApplicability,
        text_kind: MetadataFieldTextKind,
        default_value: Option<String>,
        visible_on_cards: bool,
    },
    ReorderMetadataField {
        field_id: String,
        target_index: usize,
    },
    BeginMetadataFieldDrag(String),
    SetMetadataFieldDragTarget(usize),
    CommitMetadataFieldDrag,
    CancelMetadataFieldDrag,
    RequestDeleteMetadataField(String),
    ConfirmDeleteMetadataField,
    SelectStyle(String),
    CreateStyle,
    RenameStyle {
        style_id: String,
        display_name: String,
    },
    SetStyleInheritance {
        style_id: String,
        inherits: Option<String>,
    },
    SetStyleProperties {
        style_id: String,
        properties: StyleProperties,
    },
    SetStyleProperty {
        style_id: String,
        property: StyleProperty,
        value: String,
    },
    RequestDeleteStyle(String),
    ConfirmDeleteStyle,
    ActivateCard(String),
    SetCardsSection(String),
    BeginHierarchyDrag {
        source_id: String,
        gesture: SelectionGesture,
    },
    SetDragDestination(Option<DragDestination>),
    ClearDragDestination(DragDestination),
    CommitHierarchyDrag,
    CancelHierarchyDrag,
    OpenHierarchyContextMenu {
        node_id: String,
        point: Point,
    },
    CloseHierarchyContextMenu,
    DropHierarchy {
        source_id: String,
        destination: DragDestination,
    },
    CopySelection,
    CutSelection,
    CancelCut,
    UndoProject,
    RedoProject,
    PasteSelection {
        destination: DragDestination,
    },
    SetGlobalSearchQuery(String),
    SetGlobalReplacement(String),
    SetGlobalSearchOptions {
        case_sensitive: bool,
        whole_word: bool,
    },
    SetGlobalSearchScroll(f32),
    NavigateGlobalSearchResult(String),
    OpenReplacementPreview,
    SetReplacementIncluded {
        node_id: String,
        included: bool,
    },
    SelectAllReplacementMatches,
    SelectNoReplacementMatches,
    CloseReplacementPreview,
    ApplyReplacement,
    SetHistoryDocumentFilter(Option<String>),
    SetHistoryScroll(f32),
    SelectHistoryCheckpoint(String),
    SetNamedSnapshotDraft(String),
    RequestNamedSnapshot(String),
    RequestHistoryRestore {
        checkpoint_id: String,
    },
    ConfirmHistoryRestore,
    RequestHistoryReinitialize,
    ConfirmHistoryReinitialize,
    HistoryMaintenanceLoaded(HistoryMaintenanceStatus),
    HistoryReinitialized(String),
    DismissModal,
    SelectRecentlyDeleted(String),
    RestoreDeleted(String),
    UseRestoreFallback(String),
    SetAppearance(AppearanceMode),
    SelectSettingsCategory(SettingsCategory),
    SelectDictionaryScope(DictionaryScope),
    SetExportOutputName(String),
    BrowseExportDestination,
    SetExportDestination(Option<String>),
    SetExportNumbering(bool),
    SetExportTitleSetting(ProjectExportSetting),
    SetExportPageBreak(bool),
    StartExport,
    CancelExport,
    OpenExportResult,
    RevealExportResult,
    ExportProgress {
        completed: u64,
        total: u64,
    },
    ExportSucceeded(ExportArtifact),
    ExportPlanning,
    ExportCommitting,
    ExportCancelled,
    ExportFailed(String),
    /// A mounted editor changed without changing the project-tree revision.
    MarkEditorDirty,
    MarkDirty(u64),
    StartSave(u64),
    SaveCompleted(u64),
    SaveFailed(String),
    RequestClose,
    RetryCloseSave,
    CancelClose,
    SetContentState(ContentState),
    AcceptRecovery,
    DiscardRecovery,
    RetryRecovery,
    RecoveryDurablySaved,
}

/// Integration effects translated into application/service calls.
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectEffect {
    OpenDocumentInPrimary(String),
    OpenDocumentInCompanion(String),
    CreateHierarchy {
        parent_id: String,
        kind: HierarchyItemKind,
    },
    DeleteHierarchy(Vec<String>),
    MoveHierarchy {
        node_ids: Vec<String>,
        destination: DragDestination,
    },
    PasteCopiedSubtrees {
        node_ids: Vec<String>,
        destination: DragDestination,
    },
    PasteCutSubtrees {
        node_ids: Vec<String>,
        destination: DragDestination,
    },
    UndoProject,
    RedoProject,
    CommitNodeTitle {
        node_id: String,
        title: String,
    },
    CommitSynopsis {
        node_id: String,
        synopsis: String,
    },
    CommitMetadataValue {
        node_id: String,
        field_id: String,
        value: String,
    },
    UpsertMetadataField(MetadataFieldDefinition),
    ReorderMetadataField {
        field_id: String,
        target_index: usize,
    },
    DeleteMetadataField(String),
    UpsertStyle(StyleDefinition),
    DeleteStyle(String),
    SearchProject {
        query: String,
        case_sensitive: bool,
        whole_word: bool,
        generation: u64,
    },
    NavigateSearchResult {
        match_id: String,
        revalidate_revision: bool,
    },
    BuildReplacementPreview {
        query_generation: u64,
        captured_project_revision: u64,
        replacement: String,
    },
    ApplyGlobalReplacement {
        captured_project_revision: u64,
        included_match_ids: Vec<String>,
        replacement: String,
    },
    CreateNamedSnapshot(String),
    PreviewHistory(String),
    PreviewDeleted {
        node_id: String,
        checkpoint_id: String,
        document_id: String,
    },
    RestoreHistory {
        checkpoint_id: String,
        scope: HistoryRestoreScope,
    },
    ReinitializeHistory,
    RestoreDeletedSubtree {
        node_id: String,
        location: RestoreLocation,
    },
    ApplyAppearanceToAllWindows(AppearanceMode),
    SetProjectExportSettings(ProjectExportSettings),
    ExportEntireManuscript {
        output_name: String,
        number_documents: bool,
        source_revision: u64,
    },
    ChooseExportDestination {
        output_name: String,
    },
    CancelExport,
    OpenExportResult(ExportArtifactToken),
    RevealExportResult(ExportArtifactToken),
    SaveThroughRevision(u64),
    FocusRecoveredEditor,
    ReconcileRecovery,
    DiscardRecovery,
}

/// Project-facing presentation model integrated with the mounted editor model.
#[derive(Debug, Clone)]
pub struct ProjectWorkspace {
    source: ProjectWorkspaceSource,
    session: u64,
    project_revision: u64,
    sidebar: SidebarSurface,
    explorer: ExplorerState,
    tree_clipboard: Option<TreeClipboard>,
    cards_section: String,
    cards_drag_destination: Option<DragDestination>,
    pointer_drag: Option<HierarchyPointerDrag>,
    hierarchy_context_menu: Option<String>,
    hierarchy_context_point: Point,
    hierarchy_rename: Option<HierarchyRename>,
    pending_hierarchy_creation: Option<PendingHierarchyCreation>,
    last_activated_document: Option<String>,
    synopsis_editors: BTreeMap<String, text_editor::Content>,
    /// Locally authored Synopsis text which has not yet been observed in an
    /// authoritative snapshot. This keeps an earlier asynchronous completion
    /// from replacing a newer editor draft.
    synopsis_drafts: BTreeMap<String, String>,
    metadata_values: BTreeMap<(String, String), String>,
    settings: SettingsState,
    global_search: GlobalSearchState,
    replacement_preview: ReplacementPreviewState,
    history: HistoryState,
    recently_deleted: RecentlyDeletedState,
    export: ExportViewState,
    save: SaveViewState,
    content_state: ContentState,
    recovery: RecoveryState,
    modal: Option<ProjectModal>,
    editor: EditorWorkspace,
    pending: BTreeMap<ProjectTask, ProjectTaskTicket>,
    next_request: u64,
}

#[derive(Debug, Clone, Copy)]
enum ProjectWorkspaceSource {
    Fixture(ProjectFixture),
    Production,
}

impl ProjectWorkspace {
    pub fn from_fixture(fixture: ProjectFixture) -> Self {
        let sidebar = if fixture == ProjectFixture::GlobalSearch {
            SidebarSurface::GlobalSearch
        } else {
            SidebarSurface::Explorer
        };
        let content_state = if fixture == ProjectFixture::ErrorRecovery {
            ContentState::Recovery
        } else {
            ContentState::Ready
        };
        let mut global_search = GlobalSearchState::default();
        if fixture == ProjectFixture::GlobalSearch {
            global_search.query = "river".to_owned();
            global_search.results = vec![GlobalSearchResult {
                document_id: "chapter-one".to_owned(),
                match_id: "chapter-one-match-1".to_owned(),
                prefix: "beside the ".to_owned(),
                matching_text: "river".to_owned(),
                suffix: ", the path".to_owned(),
                indexed_revision: 1,
            }];
            global_search.complete = true;
        }
        let history = HistoryState {
            checkpoints: vec![
                HistoryCheckpointRow {
                    checkpoint_id: "snapshot-draft-two".to_owned(),
                    sequence: 2,
                    category: HistoryCheckpointCategory::NamedSnapshot,
                    affected_document_ids: vec!["chapter-one".to_owned()],
                    name: Some("Draft Two".to_owned()),
                },
                HistoryCheckpointRow {
                    checkpoint_id: "autosave-17".to_owned(),
                    sequence: 1,
                    category: HistoryCheckpointCategory::Autosave,
                    affected_document_ids: vec!["chapter-one".to_owned()],
                    name: None,
                },
            ],
            ..HistoryState::default()
        };
        let explorer = ExplorerState::fixture();
        let synopsis_editors = synopsis_editors(&explorer);
        Self {
            source: ProjectWorkspaceSource::Fixture(fixture),
            session: 37,
            project_revision: 1,
            sidebar,
            explorer,
            tree_clipboard: None,
            cards_section: "manuscript".to_owned(),
            cards_drag_destination: Some(DragDestination::BeforeSibling(
                "chapter-three".to_owned(),
            )),
            pointer_drag: None,
            hierarchy_context_menu: None,
            hierarchy_context_point: Point::default(),
            hierarchy_rename: None,
            pending_hierarchy_creation: None,
            last_activated_document: None,
            synopsis_editors,
            synopsis_drafts: BTreeMap::new(),
            metadata_values: BTreeMap::from([(
                ("chapter-one".to_owned(), "field-17".to_owned()),
                "first person".to_owned(),
            )]),
            settings: SettingsState::fixture(),
            global_search,
            replacement_preview: ReplacementPreviewState::fixture(),
            history,
            recently_deleted: RecentlyDeletedState::fixture(),
            export: ExportViewState::default(),
            save: SaveViewState::fixture(),
            content_state,
            recovery: RecoveryState {
                accepted: false,
                durable_save_completed: false,
                accepted_records: 1,
                affected_documents: Vec::new(),
                isolation: None,
                error: None,
                resolving: false,
            },
            modal: None,
            editor: EditorWorkspace::from_fixture(EditorFixture::DualPane),
            pending: BTreeMap::new(),
            next_request: 0,
        }
    }

    /// Hydrates a production workspace from one authoritative project snapshot.
    pub fn from_snapshot(snapshot: &ProjectSnapshot) -> Self {
        let explorer = ExplorerState::from_project(&snapshot.project);
        let synopsis_editors = synopsis_editors(&explorer);
        let settings = SettingsState::from_project(&snapshot.project, AppearanceMode::System);
        let metadata_values = metadata_values_from_project(&snapshot.project);
        let recently_deleted = RecentlyDeletedState::from_snapshot(snapshot);
        let mut export = ExportViewState::default();
        export.reconcile_project(&snapshot.project);
        let has_live_documents = explorer
            .nodes
            .values()
            .any(|node| node.kind == HierarchyNodeKind::Document);
        Self {
            source: ProjectWorkspaceSource::Production,
            session: 0,
            project_revision: snapshot.project.revision.value(),
            sidebar: SidebarSurface::Explorer,
            explorer,
            tree_clipboard: None,
            cards_section: stable_id_string(ProjectSection::Manuscript.root_id().as_bytes()),
            cards_drag_destination: None,
            pointer_drag: None,
            hierarchy_context_menu: None,
            hierarchy_context_point: Point::default(),
            hierarchy_rename: None,
            pending_hierarchy_creation: None,
            last_activated_document: None,
            synopsis_editors,
            synopsis_drafts: BTreeMap::new(),
            metadata_values,
            settings,
            global_search: GlobalSearchState::default(),
            replacement_preview: ReplacementPreviewState {
                open: false,
                nodes: BTreeMap::new(),
                captured_project_revision: snapshot.project.revision.value(),
                captured_query_generation: 0,
                validation: ReplacementPreviewValidation::Draft,
            },
            history: HistoryState::default(),
            recently_deleted,
            export,
            save: SaveViewState {
                state: SaveState::SavedThrough(snapshot.project.revision.value()),
                recovery_intact: true,
                close_waiting: false,
            },
            content_state: if has_live_documents {
                ContentState::Ready
            } else {
                ContentState::Empty
            },
            recovery: RecoveryState {
                accepted: false,
                durable_save_completed: false,
                accepted_records: 0,
                affected_documents: Vec::new(),
                isolation: None,
                error: None,
                resolving: false,
            },
            modal: None,
            editor: EditorWorkspace::from_snapshot(snapshot),
            pending: BTreeMap::new(),
            next_request: 0,
        }
    }

    /// Reconciles authoritative project/document data while retaining live UI state.
    pub fn reconcile_snapshot(&mut self, snapshot: &ProjectSnapshot) {
        let prior_node_ids = self.explorer.nodes.keys().cloned().collect::<BTreeSet<_>>();
        self.project_revision = snapshot.project.revision.value();
        self.explorer.reconcile_project(&snapshot.project);
        self.begin_rename_for_created_hierarchy(&prior_node_ids);
        self.reconcile_synopsis_editors();
        self.metadata_values = metadata_values_from_project(&snapshot.project);
        let selected_category = self.settings.selected_category;
        let selected_detail = self.settings.selected_detail.clone();
        self.settings = SettingsState::from_project(&snapshot.project, self.settings.appearance);
        self.settings.selected_category = selected_category;
        self.settings.selected_detail = selected_detail.filter(|detail| match detail {
            SettingsDetail::MetadataField(id) => {
                self.settings.metadata_definitions.contains_key(id)
            }
            SettingsDetail::Style(id) => self.settings.style_definitions.contains_key(id),
        });
        self.recently_deleted.reconcile_snapshot(snapshot);
        self.export.reconcile_project(&snapshot.project);
        self.editor.reconcile_snapshot(snapshot);

        if !self.explorer.nodes.contains_key(&self.cards_section) {
            self.cards_section = stable_id_string(ProjectSection::Manuscript.root_id().as_bytes());
        }
        self.cards_drag_destination =
            self.cards_drag_destination
                .take()
                .filter(|destination| match destination {
                    DragDestination::BeforeSibling(id)
                    | DragDestination::AfterSibling(id)
                    | DragDestination::IntoGroup(id) => self.explorer.nodes.contains_key(id),
                    DragDestination::EditorPane(_) => true,
                });
        self.pointer_drag = self.pointer_drag.take().filter(|drag| {
            self.explorer.nodes.contains_key(&drag.source_id)
                && drag
                    .destination
                    .as_ref()
                    .is_none_or(|destination| match destination {
                        DragDestination::BeforeSibling(id)
                        | DragDestination::AfterSibling(id)
                        | DragDestination::IntoGroup(id) => self.explorer.nodes.contains_key(id),
                        DragDestination::EditorPane(_) => true,
                    })
        });
        self.hierarchy_context_menu = self
            .hierarchy_context_menu
            .take()
            .filter(|node| self.explorer.nodes.contains_key(node));
        self.hierarchy_rename = self
            .hierarchy_rename
            .take()
            .filter(|rename| self.explorer.nodes.contains_key(&rename.node_id));
        if self
            .last_activated_document
            .as_deref()
            .is_some_and(|id| !self.explorer.contains_document(id))
        {
            self.last_activated_document = None;
        }
        self.global_search
            .results
            .retain(|result| self.explorer.contains_document(&result.document_id));
        if self.replacement_preview.open
            && self.replacement_preview.captured_project_revision != self.project_revision
        {
            self.replacement_preview.mark_failed(
                "The project changed after this replacement preview was captured. Revalidate the selected matches before applying."
                    .to_owned(),
            );
        }
        if self
            .history
            .active_document_filter
            .as_deref()
            .is_some_and(|id| !self.explorer.contains_document(id))
        {
            self.history.active_document_filter = None;
        }
        if matches!(
            &self.modal,
            Some(ProjectModal::DeleteMetadataField { field_id })
                if !self.settings.metadata_definitions.contains_key(field_id)
        ) {
            self.modal = None;
        }
        if matches!(
            &self.modal,
            Some(ProjectModal::DeleteStyle { style_id })
                if !self.settings.style_definitions.contains_key(style_id)
        ) {
            self.modal = None;
        }
        self.pending
            .retain(|_, ticket| ticket.captured_project_revision == self.project_revision);
    }

    pub fn fixture_reference(&self, appearance: ResolvedAppearance) -> &'static str {
        match (self.source, appearance) {
            (ProjectWorkspaceSource::Production, _) => {
                panic!("production workspaces do not have fixture references")
            }
            (
                ProjectWorkspaceSource::Fixture(ProjectFixture::Explorer),
                ResolvedAppearance::Light,
            ) => "editor-single-light",
            (
                ProjectWorkspaceSource::Fixture(ProjectFixture::Explorer),
                ResolvedAppearance::Dark,
            ) => "editor-single-dark",
            (ProjectWorkspaceSource::Fixture(ProjectFixture::Cards), ResolvedAppearance::Light) => {
                "cards-light"
            }
            (ProjectWorkspaceSource::Fixture(ProjectFixture::Cards), ResolvedAppearance::Dark) => {
                "cards-dark"
            }
            (
                ProjectWorkspaceSource::Fixture(ProjectFixture::GlobalSearch),
                ResolvedAppearance::Light,
            ) => "global-search-light",
            (
                ProjectWorkspaceSource::Fixture(ProjectFixture::GlobalSearch),
                ResolvedAppearance::Dark,
            ) => "global-search-dark",
            (
                ProjectWorkspaceSource::Fixture(ProjectFixture::History),
                ResolvedAppearance::Light,
            ) => "history-light",
            (
                ProjectWorkspaceSource::Fixture(ProjectFixture::History),
                ResolvedAppearance::Dark,
            ) => "history-dark",
            (
                ProjectWorkspaceSource::Fixture(ProjectFixture::RecentlyDeleted),
                ResolvedAppearance::Light,
            ) => "recently-deleted-light",
            (
                ProjectWorkspaceSource::Fixture(ProjectFixture::RecentlyDeleted),
                ResolvedAppearance::Dark,
            ) => "recently-deleted-dark",
            (
                ProjectWorkspaceSource::Fixture(ProjectFixture::SettingsAppearance),
                ResolvedAppearance::Light,
            ) => "settings-appearance-light",
            (
                ProjectWorkspaceSource::Fixture(ProjectFixture::SettingsAppearance),
                ResolvedAppearance::Dark,
            ) => "settings-appearance-dark",
            (
                ProjectWorkspaceSource::Fixture(ProjectFixture::Export),
                ResolvedAppearance::Light,
            ) => "export-project-output-controls-light",
            (ProjectWorkspaceSource::Fixture(ProjectFixture::Export), ResolvedAppearance::Dark) => {
                "export-project-output-controls-dark"
            }
            (
                ProjectWorkspaceSource::Fixture(ProjectFixture::ErrorRecovery),
                ResolvedAppearance::Light,
            ) => "error-recovery-light",
            (
                ProjectWorkspaceSource::Fixture(ProjectFixture::ErrorRecovery),
                ResolvedAppearance::Dark,
            ) => "error-recovery-dark",
        }
    }

    pub const fn sidebar_surface(&self) -> SidebarSurface {
        self.sidebar
    }

    pub fn explorer(&self) -> &ExplorerState {
        &self.explorer
    }

    pub fn tree_clipboard_kind(&self) -> Option<TreeClipboardKind> {
        self.tree_clipboard
            .as_ref()
            .filter(|clipboard| clipboard.session == self.session)
            .map(|clipboard| clipboard.kind)
    }

    pub fn can_copy_or_cut_selection(&self) -> bool {
        let selected = self.explorer.normalized_selected_ids();
        !selected.is_empty()
            && selected.iter().all(|id| {
                self.explorer
                    .nodes
                    .get(*id)
                    .is_some_and(|node| node.kind != HierarchyNodeKind::Root)
            })
    }

    pub fn hierarchy_drag_source(&self) -> Option<&str> {
        self.pointer_drag
            .as_ref()
            .map(|drag| drag.source_id.as_str())
    }

    pub fn hierarchy_drag_destination(&self) -> Option<&DragDestination> {
        self.pointer_drag
            .as_ref()
            .and_then(|drag| drag.destination.as_ref())
    }

    pub fn hierarchy_context_menu(&self) -> Option<&str> {
        self.hierarchy_context_menu.as_deref()
    }

    pub const fn hierarchy_context_point(&self) -> Point {
        self.hierarchy_context_point
    }

    pub fn hierarchy_rename(&self) -> Option<(&str, &str)> {
        self.hierarchy_rename
            .as_ref()
            .map(|rename| (rename.node_id.as_str(), rename.title.as_str()))
    }

    /// Clears a cut payload only after the runtime reports a durable move and
    /// refreshed authoritative snapshot. Copy payloads remain reusable.
    pub fn complete_tree_paste(&mut self, kind: TreeClipboardKind) {
        if kind == TreeClipboardKind::Cut {
            self.explorer.complete_cut();
            self.tree_clipboard = None;
        }
    }

    pub fn select_tree_roots(&mut self, node_ids: &[String]) {
        self.explorer.selected = node_ids
            .iter()
            .filter(|node_id| {
                self.explorer
                    .nodes
                    .get(*node_id)
                    .is_some_and(|node| node.kind != HierarchyNodeKind::Root)
            })
            .cloned()
            .collect();
        self.explorer.selection_anchor = node_ids.last().cloned();
        self.explorer.normalize_selection();
    }

    pub fn cards(&self) -> CardsState<'_> {
        let labels = self
            .settings
            .metadata_order
            .iter()
            .filter_map(|id| self.settings.metadata_definitions.get(id))
            .filter(|field| field.visible_on_cards)
            .map(|field| field.label.as_str())
            .collect();
        CardsState {
            explorer: &self.explorer,
            section_id: &self.cards_section,
            drag_destination: self.cards_drag_destination.as_ref(),
            last_activated_document: self.last_activated_document.as_deref(),
            visible_metadata_labels: labels,
            definitions: &self.settings.metadata_definitions,
            field_order: &self.settings.metadata_order,
            values: &self.metadata_values,
        }
    }

    pub fn explorer_active_panes(&self, node_id: &str) -> (bool, bool) {
        let Some(document_id) = self
            .explorer
            .nodes
            .get(node_id)
            .and_then(|node| node.document_id.as_deref())
        else {
            return (false, false);
        };
        (
            self.editor.pane(EditorPane::Primary).active_document() == Some(document_id),
            self.editor.pane(EditorPane::Companion).active_document() == Some(document_id),
        )
    }

    pub fn inspector(&self) -> InspectorState<'_> {
        InspectorState {
            explorer: &self.explorer,
            definitions: &self.settings.metadata_definitions,
            field_order: &self.settings.metadata_order,
            values: &self.metadata_values,
        }
    }

    pub(crate) fn synopsis_editor(&self, node_id: &str) -> Option<&text_editor::Content> {
        self.synopsis_editors.get(node_id)
    }

    fn replace_synopsis_editor(&mut self, node_id: &str, synopsis: &str) {
        if let Some(editor) = self.synopsis_editors.get_mut(node_id) {
            *editor = text_editor::Content::with_text(synopsis);
        }
    }

    /// Reconciles canonical synopsis text without replacing an editor whose
    /// text already matches. `text_editor::Content` owns the selection and
    /// cursor, so recreating it after an acknowledged edit moves the caret
    /// back to the start and reverses subsequent English typing.
    fn reconcile_synopsis_editors(&mut self) {
        self.synopsis_editors
            .retain(|node_id, _| self.explorer.nodes.contains_key(node_id));
        self.synopsis_drafts
            .retain(|node_id, _| self.explorer.nodes.contains_key(node_id));
        for (node_id, node) in &self.explorer.nodes {
            if self
                .synopsis_drafts
                .get(node_id)
                .is_some_and(|draft| draft == &node.synopsis)
            {
                self.synopsis_drafts.remove(node_id);
            }
            if self.synopsis_drafts.contains_key(node_id) {
                continue;
            }
            match self.synopsis_editors.get_mut(node_id) {
                Some(editor) if editor.text() == node.synopsis => {}
                Some(editor) => *editor = text_editor::Content::with_text(&node.synopsis),
                None => {
                    self.synopsis_editors.insert(
                        node_id.clone(),
                        text_editor::Content::with_text(&node.synopsis),
                    );
                }
            }
        }
    }

    /// The Inspector follows the most recently focused editor or Explorer row.
    /// Explorer selection is also the deterministic fallback when neither has
    /// an applicable context.
    pub fn inspector_node_id(&self) -> Option<&str> {
        match self.editor.inspector_context() {
            InspectorContext::Document { document_id } => {
                self.explorer.node_id_for_document(document_id)
            }
            InspectorContext::Group { group_id } => Some(group_id.as_str()),
            InspectorContext::None => None,
        }
        .or_else(|| self.explorer.selected_ids().into_iter().next())
    }

    pub fn settings(&self) -> &SettingsState {
        &self.settings
    }

    pub fn global_search(&self) -> &GlobalSearchState {
        &self.global_search
    }

    pub fn replacement_preview(&self) -> &ReplacementPreviewState {
        &self.replacement_preview
    }

    pub fn history(&self) -> &HistoryState {
        &self.history
    }

    /// The History filter follows the active tab in the focused editor pane.
    pub fn focused_history_document(&self) -> Option<&str> {
        self.editor
            .pane(self.editor.focused_pane())
            .active_document()
    }

    /// Native integration supplies the authoritative current document only
    /// while a History preview is being shown.
    pub fn set_history_current_document(&mut self, document: Option<HistoryCurrentDocument>) {
        self.history.current_document = document;
        self.history.refresh_comparison();
    }

    pub fn complete_history_workflow(&mut self) {
        self.history.creating_named_snapshot = false;
        self.history.named_snapshot_draft.clear();
        self.history.error = None;
    }

    pub fn begin_history_load_more(&mut self) {
        self.history.loading_more = self.history.next_cursor.is_some();
        self.history.error = None;
    }

    pub fn finish_history_page(&mut self, next_cursor: Option<String>) {
        self.history.next_cursor = next_cursor;
        self.history.loading_more = false;
    }

    pub fn fail_history_workflow(&mut self, error: String) {
        self.history.creating_named_snapshot = false;
        self.history.error = Some(error.clone());
        self.report_error("history", error);
    }

    /// Records the technical cause locally and opens the shared modal language
    /// with safe, actionable copy for the author.
    pub fn report_error(&mut self, operation: &'static str, error: impl Into<String>) {
        #[cfg(feature = "diagnostics")]
        let error = error.into();
        #[cfg(not(feature = "diagnostics"))]
        let _ = error;
        #[cfg(feature = "diagnostics")]
        let session = self.session.to_string();
        #[cfg(feature = "diagnostics")]
        let revision = self.project_revision.to_string();
        #[cfg(feature = "diagnostics")]
        diagnostics::event(
            DiagnosticLevel::Error,
            "ui.user-error",
            "presented",
            &[
                ("operation", operation),
                ("session", &session),
                ("project_revision", &revision),
                ("error", &error),
            ],
        );
        self.modal = Some(ProjectModal::Error {
            title: project_error_title(operation).to_owned(),
            detail: project_error_detail(operation).to_owned(),
        });
    }

    pub fn recently_deleted(&self) -> &RecentlyDeletedState {
        &self.recently_deleted
    }

    pub fn selected_deleted_preview_effect(&self) -> Option<ProjectEffect> {
        let node_id = self.recently_deleted.selected_item_id.as_ref()?;
        let item = self.recently_deleted.items.get(node_id)?;
        if item.preview.is_some() {
            return None;
        }
        Some(ProjectEffect::PreviewDeleted {
            node_id: node_id.clone(),
            checkpoint_id: item.restoring_checkpoint_id.clone()?,
            document_id: item.preview_document_id.clone()?,
        })
    }

    pub fn export(&self) -> &ExportViewState {
        &self.export
    }

    pub fn save(&self) -> &SaveViewState {
        &self.save
    }

    pub fn content_state(&self) -> &ContentState {
        &self.content_state
    }

    pub fn recovery(&self) -> &RecoveryState {
        &self.recovery
    }

    /// Resolves recovery document IDs against the current project hierarchy.
    /// The recovery service supplies no recovered word count or last-edit time.
    pub fn recovery_summary(&self) -> Vec<RecoveryDocumentSummary<'_>> {
        self.recovery
            .affected_documents()
            .iter()
            .map(|(document_id, revision)| RecoveryDocumentSummary {
                document_id,
                display_title: self.explorer.title_for_document(document_id),
                recovered_word_count: None,
                last_edit: None,
                revision: *revision,
            })
            .collect()
    }

    pub fn modal(&self) -> Option<ProjectModal> {
        self.modal.clone()
    }

    pub fn editor(&self) -> &EditorWorkspace {
        &self.editor
    }

    pub fn editor_mut(&mut self) -> &mut EditorWorkspace {
        &mut self.editor
    }

    pub const fn project_session(&self) -> u64 {
        self.session
    }

    pub const fn project_revision(&self) -> u64 {
        self.project_revision
    }

    /// Accepts the revision from a completed persistence refresh while keeping
    /// local editor and Inspector drafts intact. Full snapshot reconciliation
    /// is intentionally reserved for structural workflows.
    pub(crate) fn accept_persisted_revision(&mut self, revision: u64) {
        self.project_revision = self.project_revision.max(revision);
    }

    pub const fn shell_context_is_retained(&self) -> bool {
        true
    }

    pub fn workspace_snapshot(
        &self,
        layout: &crate::ShellLayout,
        destination: RibbonDestination,
    ) -> WorkspaceSnapshot {
        let mut tabs = Vec::new();
        let mut views = BTreeMap::new();
        for pane in [EditorPane::Primary, EditorPane::Companion] {
            let pane_state = self.editor.pane(pane);
            for tab in pane_state.tabs() {
                let Some(node_id) = self.explorer.node_id_for_document(tab.id()) else {
                    continue;
                };
                let Some(node) = stable_id_bytes(node_id).map(parchmint_domain::NodeId::from_bytes)
                else {
                    continue;
                };
                tabs.push(OpenTabState {
                    view: pane_state.view(),
                    node,
                });
            }
            if let Some(document) = pane_state.active_document()
                && let Some(node_id) = self.explorer.node_id_for_document(document)
                && let Some(node) =
                    stable_id_bytes(node_id).map(parchmint_domain::NodeId::from_bytes)
            {
                views.insert(
                    pane_state.view(),
                    SavedViewState {
                        node,
                        scroll_offset: pane_state.scroll_offset().max(0.0).round() as u64,
                    },
                );
            }
        }
        WorkspaceSnapshot {
            layout: PaneLayout {
                explorer_width: layout.explorer_width(),
                inspector_width: layout.inspector_width(),
                split_ratio: self.editor.split_ratio(),
                explorer_collapsed: !layout.explorer_is_visible(),
                inspector_collapsed: !layout.inspector_is_visible(),
                companion_open: self.editor.pane(EditorPane::Companion).is_populated(),
            },
            explorer: ExplorerWorkspaceState {
                expanded_sections: self
                    .explorer
                    .expanded
                    .iter()
                    .filter_map(|id| stable_id_bytes(id))
                    .map(parchmint_domain::NodeId::from_bytes)
                    .collect(),
                selected_nodes: self
                    .explorer
                    .selected
                    .iter()
                    .filter_map(|id| stable_id_bytes(id))
                    .map(parchmint_domain::NodeId::from_bytes)
                    .collect(),
            },
            tabs,
            active_view: Some(self.editor.pane(self.editor.focused_pane()).view()),
            views,
            mode: if destination == RibbonDestination::Cards {
                WorkspaceMode::Cards
            } else {
                WorkspaceMode::Editor
            },
            cards_section: stable_id_bytes(&self.cards_section)
                .map(parchmint_domain::NodeId::from_bytes),
        }
    }

    pub fn apply_workspace_snapshot(&mut self, snapshot: &WorkspaceSnapshot) -> RibbonDestination {
        self.explorer.expanded = snapshot
            .explorer
            .expanded_sections
            .iter()
            .map(|node| stable_id_string(node.as_bytes()))
            .filter(|node| self.explorer.nodes.contains_key(node))
            .collect();
        self.explorer.selected = snapshot
            .explorer
            .selected_nodes
            .iter()
            .map(|node| stable_id_string(node.as_bytes()))
            .filter(|node| self.explorer.nodes.contains_key(node))
            .collect();
        self.explorer.normalize_selection();
        self.explorer.selection_anchor = self
            .explorer
            .selected_ids()
            .last()
            .map(|id| (*id).to_owned());
        if let Some(section) = snapshot.cards_section {
            let section = stable_id_string(section.as_bytes());
            if self
                .explorer
                .nodes
                .get(&section)
                .is_some_and(|node| node.kind == HierarchyNodeKind::Root)
            {
                self.cards_section = section;
            }
        }
        self.sync_inspector_context_from_selection();
        let tabs = snapshot
            .tabs
            .iter()
            .filter_map(|tab| {
                let node_id = stable_id_string(tab.node.as_bytes());
                let row = self.explorer.row(&node_id)?;
                Some((
                    tab.view,
                    TabSpec::new(row.document_id?.to_owned(), row.title.to_owned()),
                ))
            })
            .collect();
        let scroll_offsets = snapshot
            .views
            .iter()
            .map(|(view, state)| (*view, state.scroll_offset as f32))
            .collect();
        let active_documents = snapshot
            .views
            .iter()
            .filter_map(|(view, state)| {
                let node_id = stable_id_string(state.node.as_bytes());
                self.explorer
                    .row(&node_id)?
                    .document_id
                    .map(|document| (*view, document.to_owned()))
            })
            .collect();
        self.editor.restore_workspace_views(
            tabs,
            snapshot.active_view,
            &scroll_offsets,
            &active_documents,
        );
        self.editor.set_split_ratio(snapshot.layout.split_ratio);
        match snapshot.mode {
            WorkspaceMode::Editor => RibbonDestination::Editor,
            WorkspaceMode::Cards => RibbonDestination::Cards,
        }
    }

    fn sync_inspector_context_from_selection(&mut self) {
        let Some(node_id) = self
            .explorer
            .selected_ids()
            .first()
            .map(|id| (*id).to_owned())
        else {
            return;
        };
        let Some(node) = self.explorer.nodes.get(&node_id) else {
            return;
        };
        let context = match node.kind {
            HierarchyNodeKind::Document => InspectorContext::Document {
                document_id: node
                    .document_id
                    .clone()
                    .expect("document hierarchy nodes have document ids"),
            },
            HierarchyNodeKind::Root | HierarchyNodeKind::Group => {
                InspectorContext::Group { group_id: node_id }
            }
        };
        self.editor
            .update(EditorMessage::SetInspectorContext(context));
    }

    /// Reveals the active document from the focused editor pane in Explorer.
    /// The containing hierarchy remains the shared selection, so Inspector and
    /// Cards stay synchronized with editor navigation.
    pub fn reveal_focused_editor_document(&mut self) {
        let document_id = self
            .editor
            .pane(self.editor.focused_pane())
            .active_document()
            .map(str::to_owned);
        if document_id.is_some_and(|document_id| self.explorer.reveal_document(&document_id)) {
            self.sync_inspector_context_from_selection();
        }
    }

    pub fn begin_session(&mut self, session: u64, project_revision: u64) {
        self.session = session;
        self.project_revision = project_revision;
        self.pending.clear();
        self.next_request = 0;
        self.tree_clipboard = None;
        self.explorer.cancel_cut();
        #[cfg(feature = "diagnostics")]
        let session = session.to_string();
        #[cfg(feature = "diagnostics")]
        let revision = project_revision.to_string();
        #[cfg(feature = "diagnostics")]
        diagnostics::event(
            DiagnosticLevel::Info,
            "ui.project-session",
            "started",
            &[("session", &session), ("project_revision", &revision)],
        );
    }

    /// Blocks editable presentation while the startup recovery journal is
    /// reconciled for this exact project session.
    pub fn begin_recovery_reconciliation(&mut self) -> ProjectTaskTicket {
        self.content_state = ContentState::Loading;
        self.recovery.resolving = true;
        self.recovery.error = None;
        self.begin_task(ProjectTask::ReconcileRecovery)
    }

    /// Starts one task and invalidates an older request for the same task key.
    pub fn begin_task(&mut self, task: ProjectTask) -> ProjectTaskTicket {
        self.next_request = self.next_request.saturating_add(1);
        self.pending
            .retain(|pending, _| !same_task_family(pending, &task));
        let ticket = ProjectTaskTicket {
            session: self.session,
            task: task.clone(),
            request: self.next_request,
            captured_project_revision: self.project_revision,
        };
        self.pending.insert(task, ticket.clone());
        ticket
    }

    /// Accepts only the exact live session/task/request and a matching payload.
    pub fn accept_completion(&mut self, completion: ProjectTaskCompletion) -> bool {
        let ticket = completion.ticket();
        if ticket.session != self.session
            || self.pending.get(&ticket.task) != Some(ticket)
            || !project_payload_matches(&ticket.task, completion.payload())
            || !project_payload_claim_is_exact(&ticket.task, completion.payload())
            || !self.ticket_revision_is_live(ticket)
        {
            #[cfg(feature = "diagnostics")]
            diagnostics::event(
                DiagnosticLevel::Warn,
                "ui.project-task",
                "ignored stale or invalid completion",
                &[("task", project_task_name(&ticket.task))],
            );
            return false;
        }
        let keep_streaming = matches!(
            completion.payload(),
            ProjectTaskPayload::SearchBatch {
                finished: false,
                ..
            } | ProjectTaskPayload::ExportPlanning
                | ProjectTaskPayload::ExportProgress { .. }
                | ProjectTaskPayload::ExportCommitting
        );
        let task = ticket.task.clone();
        let failure = match completion.payload() {
            ProjectTaskPayload::Failed(error) => Some((project_task_name(&task), error.clone())),
            _ => None,
        };
        let accepted = self.apply_completion(completion);
        if accepted && !keep_streaming {
            self.pending.remove(&task);
        }
        if accepted && let Some((operation, error)) = failure {
            self.report_error(operation, error);
        }
        accepted
    }

    pub fn update(&mut self, message: ProjectMessage) -> Vec<ProjectEffect> {
        if !matches!(
            &message,
            ProjectMessage::OpenHierarchyContextMenu { .. }
                | ProjectMessage::CloseHierarchyContextMenu
                | ProjectMessage::RenameNode { .. }
        ) {
            self.hierarchy_context_menu = None;
        }
        match message {
            ProjectMessage::ShowExplorer => {
                self.sidebar = SidebarSurface::Explorer;
                Vec::new()
            }
            ProjectMessage::ShowGlobalSearch => {
                self.sidebar = SidebarSurface::GlobalSearch;
                Vec::new()
            }
            ProjectMessage::SelectHierarchy { node_id, gesture } => {
                self.explorer.select(&node_id, gesture);
                self.sync_inspector_context_from_selection();
                Vec::new()
            }
            ProjectMessage::ToggleHierarchyExpanded(node_id) => {
                self.explorer.toggle_expanded(&node_id);
                Vec::new()
            }
            ProjectMessage::RequestCreateHierarchy { parent_id, kind } => {
                let can_contain_children =
                    self.explorer.nodes.get(&parent_id).is_some_and(|node| {
                        matches!(
                            node.kind,
                            HierarchyNodeKind::Root | HierarchyNodeKind::Group
                        )
                    });
                if !can_contain_children {
                    return Vec::new();
                }
                self.pending_hierarchy_creation = Some(PendingHierarchyCreation {
                    parent_id: parent_id.clone(),
                    kind,
                });
                vec![ProjectEffect::CreateHierarchy { parent_id, kind }]
            }
            ProjectMessage::DeleteSelection => {
                let selected = self.explorer.normalized_selected_ids();
                let node_ids = selected
                    .into_iter()
                    .filter(|node_id| {
                        self.explorer
                            .nodes
                            .get(*node_id)
                            .is_some_and(|node| node.kind != HierarchyNodeKind::Root)
                    })
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                (!node_ids.is_empty())
                    .then_some(ProjectEffect::DeleteHierarchy(node_ids))
                    .into_iter()
                    .collect()
            }
            ProjectMessage::PreviewHierarchyNode(node_id) => {
                self.open_hierarchy_node(node_id, None, true)
            }
            ProjectMessage::OpenHierarchyNode(node_id) => {
                self.open_hierarchy_node(node_id, None, false)
            }
            ProjectMessage::OpenHierarchyNodeInCompanion(node_id) => {
                self.open_hierarchy_node(node_id, Some(EditorPane::Companion), false)
            }
            ProjectMessage::RenameNode { node_id, title } => {
                self.explorer.rename(&node_id, title.clone());
                vec![ProjectEffect::CommitNodeTitle { node_id, title }]
            }
            ProjectMessage::BeginHierarchyRename(node_id) => {
                let Some(node) = self.explorer.nodes.get(&node_id) else {
                    return Vec::new();
                };
                if node.kind == HierarchyNodeKind::Root {
                    return Vec::new();
                }
                self.hierarchy_context_menu = None;
                self.hierarchy_rename = Some(HierarchyRename {
                    node_id,
                    title: node.title.clone(),
                });
                Vec::new()
            }
            ProjectMessage::SetHierarchyRenameDraft(title) => {
                if let Some(rename) = self.hierarchy_rename.as_mut() {
                    rename.title = title;
                }
                Vec::new()
            }
            ProjectMessage::CommitHierarchyRename => {
                let Some(rename) = self.hierarchy_rename.take() else {
                    return Vec::new();
                };
                let title = rename.title.trim().to_owned();
                if title.is_empty()
                    || self
                        .explorer
                        .nodes
                        .get(&rename.node_id)
                        .is_none_or(|node| node.title == title)
                {
                    return Vec::new();
                }
                self.explorer.rename(&rename.node_id, title.clone());
                vec![ProjectEffect::CommitNodeTitle {
                    node_id: rename.node_id,
                    title,
                }]
            }
            ProjectMessage::CancelHierarchyRename => {
                self.hierarchy_rename = None;
                Vec::new()
            }
            ProjectMessage::SetSynopsis { node_id, synopsis } => {
                self.explorer.set_synopsis(&node_id, synopsis.clone());
                self.replace_synopsis_editor(&node_id, &synopsis);
                self.synopsis_drafts
                    .insert(node_id.clone(), synopsis.clone());
                vec![ProjectEffect::CommitSynopsis { node_id, synopsis }]
            }
            ProjectMessage::EditSynopsis { node_id, action } => {
                let is_edit = action.is_edit();
                let Some(editor) = self.synopsis_editors.get_mut(&node_id) else {
                    return Vec::new();
                };
                editor.perform(action);
                if !is_edit {
                    return Vec::new();
                }
                let synopsis = editor.text();
                self.explorer.set_synopsis(&node_id, synopsis.clone());
                self.synopsis_drafts
                    .insert(node_id.clone(), synopsis.clone());
                vec![ProjectEffect::CommitSynopsis { node_id, synopsis }]
            }
            ProjectMessage::SetMetadataValue {
                node_id,
                field_id,
                value,
            } => {
                if !self
                    .inspector()
                    .metadata_field_is_visible(&node_id, &field_id)
                {
                    return Vec::new();
                }
                self.metadata_values
                    .insert((node_id.clone(), field_id.clone()), value.clone());
                vec![ProjectEffect::CommitMetadataValue {
                    node_id,
                    field_id,
                    value,
                }]
            }
            ProjectMessage::SetMetadataApplicability {
                field_id,
                applies_to_documents,
            } => {
                let Some(field) = self.settings.metadata_definitions.get_mut(&field_id) else {
                    return Vec::new();
                };
                let previous = field.applicability;
                field.applicability = match (field.applicability, applies_to_documents) {
                    (MetadataFieldApplicability::GroupsAndDocuments, false) => {
                        MetadataFieldApplicability::Groups
                    }
                    (MetadataFieldApplicability::Groups, true)
                    | (MetadataFieldApplicability::Documents, true) => {
                        MetadataFieldApplicability::GroupsAndDocuments
                    }
                    // A definition must target Groups, Documents, or both.
                    (MetadataFieldApplicability::Documents, false) => field.applicability,
                    (current, _) => current,
                };
                (field.applicability != previous)
                    .then(|| self.metadata_effect(&field_id))
                    .flatten()
                    .into_iter()
                    .collect()
            }
            ProjectMessage::SelectMetadataField(field_id) => {
                if self.settings.metadata_definitions.contains_key(&field_id) {
                    self.settings.selected_category = SettingsCategory::Metadata;
                    self.settings.selected_detail = Some(SettingsDetail::MetadataField(field_id));
                }
                Vec::new()
            }
            ProjectMessage::CreateMetadataField => {
                let id = self.new_metadata_field_id();
                self.settings.metadata_order.push(id.clone());
                self.settings.metadata_definitions.insert(
                    id.clone(),
                    MetadataDefinition {
                        label: "New field".to_owned(),
                        description: None,
                        applicability: MetadataFieldApplicability::Documents,
                        text_kind: MetadataFieldTextKind::SingleLine,
                        default_value: None,
                        visible_on_cards: false,
                    },
                );
                self.settings.selected_detail = Some(SettingsDetail::MetadataField(id.clone()));
                self.metadata_effect(&id).into_iter().collect()
            }
            ProjectMessage::UpdateMetadataField {
                field_id,
                label,
                description,
                applicability,
                text_kind,
                default_value,
                visible_on_cards,
            } => {
                if label.trim().is_empty()
                    || (text_kind == MetadataFieldTextKind::SingleLine
                        && default_value
                            .as_deref()
                            .is_some_and(|value| value.contains(['\r', '\n'])))
                {
                    return Vec::new();
                }
                let Some(field) = self.settings.metadata_definitions.get_mut(&field_id) else {
                    return Vec::new();
                };
                field.label = label;
                field.description = description.filter(|value| !value.trim().is_empty());
                field.applicability = applicability;
                field.text_kind = text_kind;
                field.default_value = default_value;
                field.visible_on_cards = visible_on_cards;
                self.metadata_effect(&field_id).into_iter().collect()
            }
            ProjectMessage::ReorderMetadataField {
                field_id,
                target_index,
            } => {
                let Some(index) = self
                    .settings
                    .metadata_order
                    .iter()
                    .position(|candidate| *candidate == field_id)
                else {
                    return Vec::new();
                };
                let field = self.settings.metadata_order.remove(index);
                let target = target_index.min(self.settings.metadata_order.len());
                if index == target {
                    self.settings.metadata_order.insert(index, field);
                    return Vec::new();
                }
                self.settings.metadata_order.insert(target, field);
                vec![ProjectEffect::ReorderMetadataField {
                    field_id,
                    target_index,
                }]
            }
            ProjectMessage::BeginMetadataFieldDrag(field_id) => {
                if self.settings.metadata_definitions.contains_key(&field_id) {
                    self.settings.metadata_drag_source = Some(field_id);
                    self.settings.metadata_drag_target = None;
                }
                Vec::new()
            }
            ProjectMessage::SetMetadataFieldDragTarget(target_index) => {
                if self.settings.metadata_drag_source.is_some() {
                    self.settings.metadata_drag_target =
                        Some(target_index.min(self.settings.metadata_order.len()));
                }
                Vec::new()
            }
            ProjectMessage::CommitMetadataFieldDrag => {
                let source = self.settings.metadata_drag_source.take();
                let target = self.settings.metadata_drag_target.take();
                match (source, target) {
                    (Some(field_id), Some(target_index))
                        if self.settings.metadata_definitions.contains_key(&field_id) =>
                    {
                        self.update(ProjectMessage::ReorderMetadataField {
                            field_id,
                            target_index,
                        })
                    }
                    _ => Vec::new(),
                }
            }
            ProjectMessage::CancelMetadataFieldDrag => {
                self.settings.metadata_drag_source = None;
                self.settings.metadata_drag_target = None;
                Vec::new()
            }
            ProjectMessage::RequestDeleteMetadataField(field_id) => {
                if self.settings.metadata_definitions.contains_key(&field_id) {
                    self.modal = Some(ProjectModal::DeleteMetadataField { field_id });
                }
                Vec::new()
            }
            ProjectMessage::ConfirmDeleteMetadataField => {
                let Some(ProjectModal::DeleteMetadataField { field_id }) = self.modal.take() else {
                    return Vec::new();
                };
                self.settings.metadata_definitions.remove(&field_id);
                self.settings
                    .metadata_order
                    .retain(|candidate| candidate != &field_id);
                self.metadata_values
                    .retain(|(_, candidate), _| candidate != &field_id);
                vec![ProjectEffect::DeleteMetadataField(field_id)]
            }
            ProjectMessage::SelectStyle(style_id) => {
                if self.settings.style_definitions.contains_key(&style_id) {
                    self.settings.selected_category = SettingsCategory::Styles;
                    self.settings.selected_detail = Some(SettingsDetail::Style(style_id));
                }
                Vec::new()
            }
            ProjectMessage::CreateStyle => {
                let (id, domain_id) = self.new_style_id();
                let definition = StyleDefinition::custom(domain_id, "New style");
                self.settings.style_order.push(id.clone());
                self.settings
                    .style_definitions
                    .insert(id.clone(), definition.clone());
                self.settings.selected_detail = Some(SettingsDetail::Style(id));
                vec![ProjectEffect::UpsertStyle(definition)]
            }
            ProjectMessage::RenameStyle {
                style_id,
                display_name,
            } => {
                if display_name.trim().is_empty() {
                    return Vec::new();
                }
                let Some(definition) = self.settings.style_definitions.get_mut(&style_id) else {
                    return Vec::new();
                };
                definition.display_name = display_name;
                vec![ProjectEffect::UpsertStyle(definition.clone())]
            }
            ProjectMessage::SetStyleInheritance { style_id, inherits } => {
                let parent = inherits
                    .as_ref()
                    .and_then(|id| self.settings.style_definitions.get(id));
                let Some(definition) = self.settings.style_definitions.get(&style_id).cloned()
                else {
                    return Vec::new();
                };
                if inherits.as_ref().is_some_and(|id| id == &style_id)
                    || (inherits.is_some() && parent.is_none())
                    || self.style_would_cycle(&style_id, inherits.as_deref())
                {
                    return Vec::new();
                }
                let mut definition = definition;
                definition.inherits = parent.map(|style| style.id);
                self.settings
                    .style_definitions
                    .insert(style_id, definition.clone());
                vec![ProjectEffect::UpsertStyle(definition)]
            }
            ProjectMessage::SetStyleProperties {
                style_id,
                properties,
            } => {
                if !style_properties_are_finite(&properties) {
                    return Vec::new();
                }
                let Some(definition) = self.settings.style_definitions.get_mut(&style_id) else {
                    return Vec::new();
                };
                definition.properties = properties;
                vec![ProjectEffect::UpsertStyle(definition.clone())]
            }
            ProjectMessage::SetStyleProperty {
                style_id,
                property,
                value,
            } => {
                let Some(definition) = self.settings.style_definitions.get_mut(&style_id) else {
                    return Vec::new();
                };
                let previous = definition.properties.clone();
                if !set_style_property(&mut definition.properties, property, &value) {
                    definition.properties = previous;
                    return Vec::new();
                }
                vec![ProjectEffect::UpsertStyle(definition.clone())]
            }
            ProjectMessage::RequestDeleteStyle(style_id) => {
                if self
                    .settings
                    .style_definitions
                    .get(&style_id)
                    .is_some_and(|style| !style.role.is_reserved())
                {
                    self.modal = Some(ProjectModal::DeleteStyle { style_id });
                }
                Vec::new()
            }
            ProjectMessage::ConfirmDeleteStyle => {
                let Some(ProjectModal::DeleteStyle { style_id }) = self.modal.take() else {
                    return Vec::new();
                };
                let can_delete =
                    self.settings
                        .style_definitions
                        .get(&style_id)
                        .is_some_and(|style| {
                            !style.role.is_reserved()
                                && !self
                                    .settings
                                    .style_definitions
                                    .values()
                                    .any(|candidate| candidate.inherits == Some(style.id))
                        });
                if !can_delete {
                    return Vec::new();
                }
                self.settings.style_definitions.remove(&style_id);
                self.settings.style_order.retain(|id| id != &style_id);
                if self.settings.selected_detail == Some(SettingsDetail::Style(style_id.clone())) {
                    self.settings.selected_detail = None;
                }
                vec![ProjectEffect::DeleteStyle(style_id)]
            }
            ProjectMessage::ActivateCard(document_id) => self.activate_card(document_id),
            ProjectMessage::SetCardsSection(section) => {
                if self
                    .explorer
                    .nodes
                    .get(&section)
                    .is_some_and(|node| node.kind == HierarchyNodeKind::Root)
                {
                    self.cards_section = section;
                }
                Vec::new()
            }
            ProjectMessage::BeginHierarchyDrag { source_id, gesture } => {
                if !self.explorer.nodes.contains_key(&source_id) {
                    return Vec::new();
                }
                if !self.explorer.selected.contains(&source_id)
                    || gesture != SelectionGesture::Replace
                {
                    self.explorer.select(&source_id, gesture);
                }
                if self
                    .explorer
                    .nodes
                    .get(&source_id)
                    .is_some_and(|node| node.kind != HierarchyNodeKind::Root)
                {
                    self.pointer_drag = Some(HierarchyPointerDrag {
                        source_id,
                        destination: None,
                    });
                    self.cards_drag_destination = None;
                    self.hierarchy_context_menu = None;
                }
                Vec::new()
            }
            ProjectMessage::SetDragDestination(destination) => {
                if let Some(drag) = self.pointer_drag.as_mut()
                    && self.explorer.nodes.contains_key(&drag.source_id)
                {
                    self.cards_drag_destination = destination.clone();
                    drag.destination = destination;
                }
                Vec::new()
            }
            ProjectMessage::ClearDragDestination(destination) => {
                if let Some(drag) = self.pointer_drag.as_mut()
                    && drag.destination.as_ref() == Some(&destination)
                {
                    self.cards_drag_destination = None;
                    drag.destination = None;
                }
                Vec::new()
            }
            ProjectMessage::CommitHierarchyDrag => {
                let drag = self.pointer_drag.take();
                self.cards_drag_destination = None;
                let Some(HierarchyPointerDrag {
                    source_id,
                    destination: Some(destination),
                }) = drag
                else {
                    return Vec::new();
                };
                self.drop_hierarchy(source_id, destination)
            }
            ProjectMessage::CancelHierarchyDrag => {
                self.pointer_drag = None;
                self.cards_drag_destination = None;
                Vec::new()
            }
            ProjectMessage::OpenHierarchyContextMenu { node_id, point } => {
                if self.explorer.nodes.contains_key(&node_id) {
                    self.explorer.select(&node_id, SelectionGesture::Replace);
                    self.hierarchy_context_menu = Some(node_id);
                    self.hierarchy_context_point = point;
                    self.pointer_drag = None;
                    self.cards_drag_destination = None;
                }
                Vec::new()
            }
            ProjectMessage::CloseHierarchyContextMenu => {
                self.hierarchy_context_menu = None;
                Vec::new()
            }
            ProjectMessage::DropHierarchy {
                source_id,
                destination,
            } => self.drop_hierarchy(source_id, destination),
            ProjectMessage::CopySelection => {
                let selected = self.explorer.normalized_selected_ids();
                if selected.iter().any(|id| {
                    self.explorer
                        .nodes
                        .get(*id)
                        .is_none_or(|node| node.kind == HierarchyNodeKind::Root)
                }) {
                    return Vec::new();
                }
                let node_ids = selected.into_iter().map(str::to_owned).collect::<Vec<_>>();
                if !node_ids.is_empty() {
                    self.explorer.cancel_cut();
                    self.tree_clipboard = Some(TreeClipboard {
                        session: self.session,
                        kind: TreeClipboardKind::Copy,
                        node_ids,
                    });
                }
                Vec::new()
            }
            ProjectMessage::CutSelection => {
                if self.explorer.mark_cut() {
                    self.tree_clipboard = Some(TreeClipboard {
                        session: self.session,
                        kind: TreeClipboardKind::Cut,
                        node_ids: self
                            .explorer
                            .preorder_ids()
                            .into_iter()
                            .filter(|id| self.explorer.cut_pending.contains(*id))
                            .map(str::to_owned)
                            .collect(),
                    });
                }
                Vec::new()
            }
            ProjectMessage::CancelCut => {
                self.explorer.cancel_cut();
                if self.tree_clipboard_kind() == Some(TreeClipboardKind::Cut) {
                    self.tree_clipboard = None;
                }
                Vec::new()
            }
            ProjectMessage::UndoProject => vec![ProjectEffect::UndoProject],
            ProjectMessage::RedoProject => vec![ProjectEffect::RedoProject],
            ProjectMessage::PasteSelection { destination } => {
                let Some(clipboard) = self
                    .tree_clipboard
                    .as_ref()
                    .filter(|clipboard| clipboard.session == self.session)
                else {
                    return Vec::new();
                };
                match clipboard.kind {
                    TreeClipboardKind::Copy => vec![ProjectEffect::PasteCopiedSubtrees {
                        node_ids: clipboard.node_ids.clone(),
                        destination,
                    }],
                    TreeClipboardKind::Cut => vec![ProjectEffect::PasteCutSubtrees {
                        node_ids: clipboard.node_ids.clone(),
                        destination,
                    }],
                }
            }
            ProjectMessage::SetGlobalSearchQuery(query) => {
                self.global_search.query = query;
                self.global_search.query_generation =
                    self.global_search.query_generation.saturating_add(1);
                self.global_search.results.clear();
                self.global_search.scroll_offset = 0.0;
                self.global_search.complete = false;
                self.global_search.error = None;
                self.replacement_preview.close();
                vec![self.search_effect()]
            }
            ProjectMessage::SetGlobalSearchScroll(offset) => {
                if offset.is_finite() {
                    self.global_search.scroll_offset = offset.max(0.0);
                }
                Vec::new()
            }
            ProjectMessage::SetGlobalReplacement(replacement) => {
                self.global_search.replacement = replacement;
                if self.replacement_preview.open {
                    self.replacement_preview.validation = ReplacementPreviewValidation::Draft;
                }
                Vec::new()
            }
            ProjectMessage::SetGlobalSearchOptions {
                case_sensitive,
                whole_word,
            } => {
                self.global_search.case_sensitive = case_sensitive;
                self.global_search.whole_word = whole_word;
                self.global_search.query_generation =
                    self.global_search.query_generation.saturating_add(1);
                self.global_search.results.clear();
                self.global_search.scroll_offset = 0.0;
                self.global_search.complete = false;
                self.global_search.error = None;
                self.replacement_preview.close();
                vec![self.search_effect()]
            }
            ProjectMessage::NavigateGlobalSearchResult(match_id) => {
                vec![ProjectEffect::NavigateSearchResult {
                    match_id,
                    revalidate_revision: true,
                }]
            }
            ProjectMessage::OpenReplacementPreview => {
                if !self.global_search.complete || self.global_search.results.is_empty() {
                    self.replacement_preview.open = true;
                    self.replacement_preview.mark_failed(
                        "Wait for the project search to finish and return at least one match."
                            .to_owned(),
                    );
                    return Vec::new();
                }
                self.replacement_preview.open = true;
                if !self.replacement_preview.nodes.is_empty()
                    && self.replacement_preview.captured_query_generation
                        == self.global_search.query_generation
                    && self.replacement_preview.captured_project_revision == self.project_revision
                {
                    self.replacement_preview.validation = ReplacementPreviewValidation::Validating;
                } else {
                    self.replacement_preview.prepare(
                        &self.global_search.results,
                        self.project_revision,
                        self.global_search.query_generation,
                    );
                }
                vec![ProjectEffect::BuildReplacementPreview {
                    query_generation: self.global_search.query_generation,
                    captured_project_revision: self.project_revision,
                    replacement: self.global_search.replacement.clone(),
                }]
            }
            ProjectMessage::SetReplacementIncluded { node_id, included } => {
                self.replacement_preview.set_included(&node_id, included);
                Vec::new()
            }
            ProjectMessage::SelectAllReplacementMatches => {
                self.replacement_preview.select_all(true);
                Vec::new()
            }
            ProjectMessage::SelectNoReplacementMatches => {
                self.replacement_preview.select_all(false);
                Vec::new()
            }
            ProjectMessage::CloseReplacementPreview => {
                self.replacement_preview.close();
                Vec::new()
            }
            ProjectMessage::ApplyReplacement => {
                if !self.replacement_preview.can_apply(self.project_revision) {
                    return Vec::new();
                }
                vec![ProjectEffect::ApplyGlobalReplacement {
                    captured_project_revision: self.replacement_preview.captured_project_revision,
                    included_match_ids: self
                        .replacement_preview
                        .included_match_ids()
                        .into_iter()
                        .map(str::to_owned)
                        .collect(),
                    replacement: self.global_search.replacement.clone(),
                }]
            }
            ProjectMessage::SetHistoryDocumentFilter(document_id) => {
                self.history.active_document_filter = document_id;
                self.history.checkpoints.clear();
                self.history.selected_checkpoint_id = None;
                self.history.preview = None;
                self.history.current_document = None;
                self.history.comparison = None;
                self.history.next_cursor = None;
                self.history.loading_more = false;
                self.history.error = None;
                self.history.scroll_offset = 0.0;
                Vec::new()
            }
            ProjectMessage::SetHistoryScroll(offset) => {
                if offset.is_finite() {
                    self.history.scroll_offset = offset.max(0.0);
                }
                Vec::new()
            }
            ProjectMessage::SelectHistoryCheckpoint(checkpoint_id) => {
                if !self
                    .history
                    .visible_checkpoints()
                    .any(|checkpoint| checkpoint.checkpoint_id == checkpoint_id)
                {
                    return Vec::new();
                }
                self.history.selected_checkpoint_id = Some(checkpoint_id.clone());
                self.history.preview = None;
                self.history.comparison = None;
                self.history.error = None;
                vec![ProjectEffect::PreviewHistory(checkpoint_id)]
            }
            ProjectMessage::SetNamedSnapshotDraft(value) => {
                self.history.named_snapshot_draft = value;
                self.history.error = None;
                Vec::new()
            }
            ProjectMessage::RequestNamedSnapshot(name) => {
                let name = name.trim().to_owned();
                if name.is_empty() {
                    self.history.error = Some("A snapshot name is required.".to_owned());
                    return Vec::new();
                }
                self.history.named_snapshot_draft = name.clone();
                self.history.creating_named_snapshot = true;
                self.history.error = None;
                vec![ProjectEffect::CreateNamedSnapshot(name)]
            }
            ProjectMessage::RequestHistoryRestore { checkpoint_id } => {
                let (checkpoint_label, affected_summary) = self
                    .history
                    .checkpoints
                    .iter()
                    .find(|checkpoint| checkpoint.checkpoint_id == checkpoint_id)
                    .map(|checkpoint| (checkpoint.label(), checkpoint.affected_summary()))
                    .unwrap_or_else(|| {
                        (
                            "Selected checkpoint".to_owned(),
                            "Unknown changes".to_owned(),
                        )
                    });
                self.modal = Some(ProjectModal::HistoryRestore {
                    checkpoint_id,
                    checkpoint_label,
                    affected_summary,
                    scope: HistoryRestoreScope::EntireProject,
                });
                Vec::new()
            }
            ProjectMessage::ConfirmHistoryRestore => {
                let Some(ProjectModal::HistoryRestore {
                    checkpoint_id,
                    scope,
                    ..
                }) = self.modal.take()
                else {
                    return Vec::new();
                };
                vec![ProjectEffect::RestoreHistory {
                    checkpoint_id,
                    scope,
                }]
            }
            ProjectMessage::RequestHistoryReinitialize => {
                if matches!(
                    self.history.maintenance,
                    HistoryMaintenanceStatus::Reinitializable { .. }
                ) {
                    self.modal = Some(ProjectModal::ReinitializeHistory);
                }
                Vec::new()
            }
            ProjectMessage::ConfirmHistoryReinitialize => {
                if self.modal.take() == Some(ProjectModal::ReinitializeHistory) {
                    vec![ProjectEffect::ReinitializeHistory]
                } else {
                    Vec::new()
                }
            }
            ProjectMessage::HistoryMaintenanceLoaded(status) => {
                self.history.maintenance = status;
                Vec::new()
            }
            ProjectMessage::HistoryReinitialized(message) => {
                self.history.maintenance = HistoryMaintenanceStatus::Available;
                self.history.maintenance_message = Some(message);
                self.history.error = None;
                Vec::new()
            }
            ProjectMessage::DismissModal => {
                self.modal = None;
                Vec::new()
            }
            ProjectMessage::SelectRecentlyDeleted(node_id) => {
                self.recently_deleted.select(node_id);
                self.selected_deleted_preview_effect().into_iter().collect()
            }
            ProjectMessage::RestoreDeleted(node_id) => {
                if !self.recently_deleted.items.contains_key(&node_id) {
                    return Vec::new();
                }
                let location = self.recently_deleted.restore_location(&node_id);
                vec![ProjectEffect::RestoreDeletedSubtree { node_id, location }]
            }
            ProjectMessage::UseRestoreFallback(node_id) => {
                self.recently_deleted.use_fallback(&node_id);
                Vec::new()
            }
            ProjectMessage::SetAppearance(appearance) => {
                self.settings.appearance = appearance;
                vec![ProjectEffect::ApplyAppearanceToAllWindows(appearance)]
            }
            ProjectMessage::SelectSettingsCategory(category) => {
                self.settings.selected_category = category;
                Vec::new()
            }
            ProjectMessage::SelectDictionaryScope(scope) => {
                self.settings.dictionaries.select_scope(scope);
                Vec::new()
            }
            ProjectMessage::SetExportOutputName(output_name) => {
                if !output_name.trim().is_empty() {
                    self.export.output_name = output_name;
                }
                Vec::new()
            }
            ProjectMessage::BrowseExportDestination => {
                self.export.state = ExportState::ChoosingDestination;
                vec![ProjectEffect::ChooseExportDestination {
                    output_name: self.export.output_name.clone(),
                }]
            }
            ProjectMessage::SetExportDestination(destination) => {
                self.export.destination = destination;
                self.export.state = ExportState::Ready;
                Vec::new()
            }
            ProjectMessage::SetExportNumbering(number_documents) => {
                self.export.numbering_documents = number_documents;
                Vec::new()
            }
            ProjectMessage::SetExportTitleSetting(setting) => {
                self.export.project_settings.emit_titles = setting;
                vec![ProjectEffect::SetProjectExportSettings(
                    self.export.project_settings,
                )]
            }
            ProjectMessage::SetExportPageBreak(starts_new_page) => {
                self.export.project_settings.starts_new_page = starts_new_page;
                vec![ProjectEffect::SetProjectExportSettings(
                    self.export.project_settings,
                )]
            }
            ProjectMessage::StartExport => {
                if self.export.destination.is_none() {
                    return Vec::new();
                }
                self.export.state = ExportState::Planning;
                vec![ProjectEffect::ExportEntireManuscript {
                    output_name: self.export.output_name.clone(),
                    number_documents: self.export.numbering_documents,
                    source_revision: self.project_revision,
                }]
            }
            ProjectMessage::CancelExport => {
                if self.export.can_cancel() {
                    self.export.state = ExportState::Cancelling;
                    vec![ProjectEffect::CancelExport]
                } else {
                    Vec::new()
                }
            }
            ProjectMessage::OpenExportResult => match &self.export.state {
                ExportState::Succeeded { artifact } => {
                    vec![ProjectEffect::OpenExportResult(artifact.token)]
                }
                _ => Vec::new(),
            },
            ProjectMessage::RevealExportResult => match &self.export.state {
                ExportState::Succeeded { artifact } => {
                    vec![ProjectEffect::RevealExportResult(artifact.token)]
                }
                _ => Vec::new(),
            },
            ProjectMessage::ExportPlanning => {
                if !matches!(self.export.state, ExportState::Cancelling) {
                    self.export.state = ExportState::Planning;
                }
                Vec::new()
            }
            ProjectMessage::ExportProgress { completed, total } => {
                if !matches!(self.export.state, ExportState::Cancelling) {
                    self.export.state = ExportState::Exporting {
                        completed: completed.min(total),
                        total,
                    };
                }
                Vec::new()
            }
            ProjectMessage::ExportCommitting => {
                if !matches!(self.export.state, ExportState::Cancelling) {
                    self.export.state = ExportState::Committing;
                }
                Vec::new()
            }
            ProjectMessage::ExportSucceeded(artifact) => {
                self.export.state = ExportState::Succeeded { artifact };
                Vec::new()
            }
            ProjectMessage::ExportCancelled => {
                self.export.state = ExportState::Cancelled;
                Vec::new()
            }
            ProjectMessage::ExportFailed(error) => {
                self.export.state = ExportState::Failed(error.clone());
                self.report_error("export", error);
                Vec::new()
            }
            ProjectMessage::MarkEditorDirty => {
                self.save.state = SaveState::Dirty {
                    current_revision: self.project_revision,
                };
                Vec::new()
            }
            ProjectMessage::MarkDirty(revision) => {
                self.project_revision = self.project_revision.max(revision);
                self.save.state = SaveState::Dirty {
                    current_revision: self.project_revision,
                };
                Vec::new()
            }
            ProjectMessage::StartSave(through_revision) => {
                self.save.state = SaveState::Saving { through_revision };
                vec![ProjectEffect::SaveThroughRevision(through_revision)]
            }
            ProjectMessage::SaveCompleted(revision) => {
                self.finish_save(revision);
                Vec::new()
            }
            ProjectMessage::SaveFailed(error) => {
                self.save.state = SaveState::Error(error.clone());
                self.save.recovery_intact = true;
                self.report_error("save", error);
                Vec::new()
            }
            ProjectMessage::RequestClose => {
                self.save.close_waiting = !self.save.claims_saved();
                Vec::new()
            }
            ProjectMessage::RetryCloseSave => {
                self.save.close_waiting = true;
                vec![ProjectEffect::SaveThroughRevision(self.project_revision)]
            }
            ProjectMessage::CancelClose => {
                self.save.close_waiting = false;
                Vec::new()
            }
            ProjectMessage::SetContentState(state) => {
                if let ContentState::Error(error) = &state {
                    self.report_error("project", error.clone());
                }
                self.content_state = state;
                Vec::new()
            }
            ProjectMessage::AcceptRecovery => {
                if self.content_state != ContentState::Recovery || self.recovery.resolving {
                    return Vec::new();
                }
                self.recovery.resolving = true;
                self.recovery.error = None;
                vec![ProjectEffect::FocusRecoveredEditor]
            }
            ProjectMessage::DiscardRecovery => {
                if self.content_state != ContentState::Recovery || self.recovery.resolving {
                    return Vec::new();
                }
                self.recovery.resolving = true;
                self.recovery.error = None;
                vec![ProjectEffect::DiscardRecovery]
            }
            ProjectMessage::RetryRecovery => {
                if self.content_state != ContentState::Recovery || self.recovery.resolving {
                    return Vec::new();
                }
                self.recovery.resolving = true;
                self.recovery.error = None;
                vec![ProjectEffect::ReconcileRecovery]
            }
            ProjectMessage::RecoveryDurablySaved => {
                self.recovery.durable_save_completed = true;
                self.save.recovery_intact = false;
                Vec::new()
            }
        }
    }

    fn metadata_effect(&self, field_id: &str) -> Option<ProjectEffect> {
        let definition = self.settings.metadata_definitions.get(field_id)?;
        let id = metadata_id_from_stable(field_id)?;
        Some(ProjectEffect::UpsertMetadataField(
            MetadataFieldDefinition {
                id,
                label: definition.label.clone(),
                description: definition.description.clone(),
                applicability: match definition.applicability {
                    MetadataFieldApplicability::Groups => DomainMetadataApplicability::Groups,
                    MetadataFieldApplicability::Documents => DomainMetadataApplicability::Documents,
                    MetadataFieldApplicability::GroupsAndDocuments => {
                        DomainMetadataApplicability::GroupsAndDocuments
                    }
                },
                text_kind: match definition.text_kind {
                    MetadataFieldTextKind::SingleLine => DomainMetadataTextKind::SingleLine,
                    MetadataFieldTextKind::Multiline => DomainMetadataTextKind::Multiline,
                },
                default_value: definition.default_value.clone(),
                visible_on_cards: definition.visible_on_cards,
            },
        ))
    }

    fn new_metadata_field_id(&self) -> String {
        for candidate in 1_u64.. {
            let mut bytes = *b"PMMETAFIELD00000";
            bytes[8..].copy_from_slice(&candidate.to_be_bytes());
            let id = stable_id_string(&bytes);
            if !self.settings.metadata_definitions.contains_key(&id) {
                return id;
            }
        }
        unreachable!("u64 ID space is not exhausted")
    }

    fn new_style_id(&self) -> (String, StyleId) {
        for candidate in 1_u64.. {
            let mut bytes = *b"PMCUSTSTYLE00000";
            bytes[8..].copy_from_slice(&candidate.to_be_bytes());
            let domain_id = StyleId::from_bytes(bytes);
            let id = stable_id_string(domain_id.as_bytes());
            if !self.settings.style_definitions.contains_key(&id) {
                return (id, domain_id);
            }
        }
        unreachable!("u64 ID space is not exhausted")
    }

    fn style_would_cycle(&self, style_id: &str, proposed_parent: Option<&str>) -> bool {
        let mut cursor = proposed_parent;
        while let Some(id) = cursor {
            if id == style_id {
                return true;
            }
            cursor = self.settings.style_definitions.get(id).and_then(|style| {
                style.inherits.as_ref().and_then(|parent| {
                    self.settings
                        .style_definitions
                        .iter()
                        .find_map(|(id, candidate)| {
                            (candidate.id == *parent).then_some(id.as_str())
                        })
                })
            });
        }
        false
    }

    pub(crate) const fn fixture(&self) -> ProjectFixture {
        match self.source {
            ProjectWorkspaceSource::Fixture(fixture) => fixture,
            ProjectWorkspaceSource::Production => {
                panic!("production workspace has no visual fixture")
            }
        }
    }

    fn activate_card(&mut self, node_id: String) -> Vec<ProjectEffect> {
        let Some((section_id, document_id, title)) =
            self.explorer.nodes.get(&node_id).and_then(|node| {
                (node.kind == HierarchyNodeKind::Document).then(|| {
                    (
                        node.section_id.clone(),
                        node.document_id
                            .clone()
                            .expect("a document hierarchy node has a document ID"),
                        node.title.clone(),
                    )
                })
            })
        else {
            return Vec::new();
        };
        let pane = if is_research_section(&section_id) {
            EditorPane::Companion
        } else {
            EditorPane::Primary
        };
        self.last_activated_document = Some(document_id.clone());
        let _ = self.editor.update(EditorMessage::OpenTab {
            pane,
            tab: TabSpec::new(document_id.clone(), title),
        });
        match pane {
            EditorPane::Primary => vec![ProjectEffect::OpenDocumentInPrimary(document_id)],
            EditorPane::Companion => vec![ProjectEffect::OpenDocumentInCompanion(document_id)],
        }
    }

    fn open_hierarchy_node(
        &mut self,
        node_id: String,
        requested_pane: Option<EditorPane>,
        preview: bool,
    ) -> Vec<ProjectEffect> {
        let Some((section_id, document_id, title)) =
            self.explorer.nodes.get(&node_id).and_then(|node| {
                (node.kind == HierarchyNodeKind::Document).then(|| {
                    (
                        node.section_id.clone(),
                        node.document_id
                            .clone()
                            .expect("a document hierarchy node has a document ID"),
                        node.title.clone(),
                    )
                })
            })
        else {
            return Vec::new();
        };
        self.explorer.select(&node_id, SelectionGesture::Replace);
        self.sync_inspector_context_from_selection();
        let pane = requested_pane.unwrap_or_else(|| {
            if is_research_section(&section_id) {
                EditorPane::Companion
            } else {
                EditorPane::Primary
            }
        });
        let tab = TabSpec::new(document_id.clone(), title);
        let _ = self.editor.update(if preview {
            EditorMessage::OpenPreviewTab { pane, tab }
        } else {
            EditorMessage::OpenTab { pane, tab }
        });
        match pane {
            EditorPane::Primary => vec![ProjectEffect::OpenDocumentInPrimary(document_id)],
            EditorPane::Companion => vec![ProjectEffect::OpenDocumentInCompanion(document_id)],
        }
    }

    fn begin_rename_for_created_hierarchy(&mut self, prior_node_ids: &BTreeSet<String>) {
        let Some(pending) = self.pending_hierarchy_creation.take() else {
            return;
        };
        let expected_kind = match pending.kind {
            HierarchyItemKind::Group => HierarchyNodeKind::Group,
            HierarchyItemKind::Document => HierarchyNodeKind::Document,
        };
        let mut created = self.explorer.nodes.values().filter(|node| {
            !prior_node_ids.contains(&node.id)
                && node.parent.as_deref() == Some(pending.parent_id.as_str())
                && node.kind == expected_kind
        });
        let Some(node) = created.next() else {
            // A different asynchronous refresh can arrive before this create
            // workflow completes. Keep waiting for the authoritative node.
            self.pending_hierarchy_creation = Some(pending);
            return;
        };
        if created.next().is_some() {
            // More than one matching node means this completion cannot be
            // identified safely. Do not start editing an unrelated item.
            return;
        }
        let node_id = node.id.clone();
        let title = node.title.clone();
        self.explorer.select(&node_id, SelectionGesture::Replace);
        self.explorer.expanded.insert(pending.parent_id);
        self.hierarchy_rename = Some(HierarchyRename { node_id, title });
    }

    fn drop_hierarchy(
        &mut self,
        source_id: String,
        destination: DragDestination,
    ) -> Vec<ProjectEffect> {
        if self.explorer.drag_validity(&source_id, destination.clone()) != DragValidity::Allowed {
            return Vec::new();
        }
        if let DragDestination::EditorPane(pane) = destination {
            let Some(node) = self.explorer.nodes.get(&source_id) else {
                return Vec::new();
            };
            let Some(document_id) = node.document_id.clone() else {
                return Vec::new();
            };
            let title = node.title.clone();
            let _ = self.editor.update(EditorMessage::OpenTab {
                pane,
                tab: TabSpec::new(document_id.clone(), title),
            });
            return match pane {
                EditorPane::Primary => vec![ProjectEffect::OpenDocumentInPrimary(document_id)],
                EditorPane::Companion => vec![ProjectEffect::OpenDocumentInCompanion(document_id)],
            };
        }
        let node_ids = if self.explorer.selected.contains(&source_id) {
            self.explorer
                .normalized_selected_ids()
                .into_iter()
                .map(str::to_owned)
                .collect()
        } else {
            vec![source_id]
        };
        vec![ProjectEffect::MoveHierarchy {
            node_ids,
            destination,
        }]
    }

    fn search_effect(&self) -> ProjectEffect {
        ProjectEffect::SearchProject {
            query: self.global_search.query.clone(),
            case_sensitive: self.global_search.case_sensitive,
            whole_word: self.global_search.whole_word,
            generation: self.global_search.query_generation,
        }
    }

    fn ticket_revision_is_live(&self, ticket: &ProjectTaskTicket) -> bool {
        match ticket.task {
            ProjectTask::ReplacementPreview
            | ProjectTask::ApplyReplacement
            | ProjectTask::RestoreHistory { .. }
            | ProjectTask::RestoreDeleted { .. }
            | ProjectTask::PreviewDeleted { .. }
            | ProjectTask::ReconcileRecovery
            | ProjectTask::AcceptRecovery
            | ProjectTask::DiscardRecovery => {
                ticket.captured_project_revision == self.project_revision
            }
            ProjectTask::GlobalSearch { generation } => {
                generation == self.global_search.query_generation
            }
            ProjectTask::LoadHistory
            | ProjectTask::PreviewHistory { .. }
            | ProjectTask::Save { .. }
            | ProjectTask::Export { .. }
            | ProjectTask::PersistWorkspace => true,
        }
    }

    fn apply_completion(&mut self, completion: ProjectTaskCompletion) -> bool {
        match completion.payload {
            ProjectTaskPayload::SearchBatch { results, finished } => {
                self.global_search.results.extend(results);
                self.global_search.complete = finished;
                self.global_search.error = None;
                true
            }
            ProjectTaskPayload::ReplacementPreviewReady => {
                self.replacement_preview.open = true;
                self.replacement_preview
                    .mark_ready(completion.ticket.captured_project_revision);
                true
            }
            ProjectTaskPayload::ReplacementApplied { revision }
            | ProjectTaskPayload::HistoryRestored { revision }
            | ProjectTaskPayload::DeletedRestored { revision } => {
                self.project_revision = revision;
                self.save.state = SaveState::Dirty {
                    current_revision: revision,
                };
                if matches!(completion.ticket.task, ProjectTask::ApplyReplacement) {
                    self.replacement_preview.close();
                }
                true
            }
            ProjectTaskPayload::RecoveryAccepted { revision } => {
                self.project_revision = revision;
                self.save.state = SaveState::SavedThrough(revision);
                self.recovery.accepted = true;
                self.recovery.durable_save_completed = true;
                self.recovery.resolving = false;
                self.recovery.error = None;
                self.content_state = self.ready_content_state();
                true
            }
            ProjectTaskPayload::RecoveryCanceled => {
                self.recovery.resolving = false;
                self.recovery.error = None;
                self.content_state = ContentState::Recovery;
                true
            }
            ProjectTaskPayload::RecoveryDiscarded { revision } => {
                self.project_revision = revision;
                self.save.state = SaveState::SavedThrough(revision);
                self.recovery.accepted = false;
                self.recovery.durable_save_completed = false;
                self.recovery.resolving = false;
                self.recovery.error = None;
                self.content_state = self.ready_content_state();
                true
            }
            ProjectTaskPayload::RecoveryUnavailable => {
                self.recovery = RecoveryState {
                    accepted: false,
                    durable_save_completed: false,
                    accepted_records: 0,
                    affected_documents: Vec::new(),
                    isolation: None,
                    error: None,
                    resolving: false,
                };
                self.content_state = self.ready_content_state();
                true
            }
            ProjectTaskPayload::RecoveryAvailable {
                accepted_records,
                affected_documents,
                isolation,
            } => {
                self.recovery.accepted_records = accepted_records;
                self.recovery.affected_documents = affected_documents;
                self.recovery.isolation = isolation;
                self.recovery.error = None;
                self.recovery.resolving = false;
                self.content_state = ContentState::Recovery;
                true
            }
            ProjectTaskPayload::HistoryLoaded { checkpoints } => {
                self.history.checkpoints = checkpoints;
                self.history.loading_more = false;
                if self
                    .history
                    .selected_checkpoint_id
                    .as_ref()
                    .is_some_and(|selected| {
                        !self
                            .history
                            .checkpoints
                            .iter()
                            .any(|checkpoint| &checkpoint.checkpoint_id == selected)
                    })
                {
                    self.history.selected_checkpoint_id = None;
                    self.history.preview = None;
                    self.history.comparison = None;
                }
                self.history.error = None;
                true
            }
            ProjectTaskPayload::HistoryPreviewReady { preview } => {
                self.history.preview = Some(preview);
                self.history.refresh_comparison();
                self.history.error = None;
                true
            }
            ProjectTaskPayload::DeletedPreviewReady {
                node_id,
                checkpoint_id: _,
                document_id,
                semantic,
            } => self
                .recently_deleted
                .set_preview(&node_id, &document_id, semantic),
            ProjectTaskPayload::SavedThrough(revision) => {
                self.finish_save(revision);
                true
            }
            ProjectTaskPayload::ExportPlanning => {
                if !matches!(self.export.state, ExportState::Cancelling) {
                    self.export.state = ExportState::Planning;
                }
                true
            }
            ProjectTaskPayload::ExportProgress { completed, total } => {
                if !matches!(self.export.state, ExportState::Cancelling) {
                    self.export.state = ExportState::Exporting {
                        completed: completed.min(total),
                        total,
                    };
                }
                true
            }
            ProjectTaskPayload::ExportCommitting => {
                if !matches!(self.export.state, ExportState::Cancelling) {
                    self.export.state = ExportState::Committing;
                }
                true
            }
            ProjectTaskPayload::ExportSucceeded { artifact } => {
                self.export.state = ExportState::Succeeded { artifact };
                true
            }
            ProjectTaskPayload::ExportCancelled => {
                self.export.state = ExportState::Cancelled;
                true
            }
            ProjectTaskPayload::WorkspacePersisted => true,
            ProjectTaskPayload::Failed(error) => {
                match completion.ticket.task {
                    ProjectTask::GlobalSearch { .. } => self.global_search.error = Some(error),
                    ProjectTask::LoadHistory
                    | ProjectTask::PreviewHistory { .. }
                    | ProjectTask::RestoreHistory { .. } => {
                        self.history.loading_more = false;
                        self.history.error = Some(error);
                    }
                    ProjectTask::PreviewDeleted { .. } => {}
                    ProjectTask::Export { .. } => self.export.state = ExportState::Failed(error),
                    ProjectTask::Save { .. } => {
                        self.save.state = SaveState::Error(error);
                        self.save.recovery_intact = true;
                    }
                    ProjectTask::ReconcileRecovery
                    | ProjectTask::AcceptRecovery
                    | ProjectTask::DiscardRecovery => {
                        self.recovery.error = Some(error);
                        self.recovery.resolving = false;
                        self.content_state = ContentState::Recovery;
                        self.save.recovery_intact = true;
                    }
                    ProjectTask::ReplacementPreview | ProjectTask::ApplyReplacement => {
                        self.replacement_preview.mark_failed(error);
                    }
                    ProjectTask::RestoreDeleted { .. } | ProjectTask::PersistWorkspace => {
                        self.content_state = ContentState::Error(error)
                    }
                }
                true
            }
        }
    }

    fn ready_content_state(&self) -> ContentState {
        if self
            .explorer
            .nodes
            .values()
            .any(|node| node.kind == HierarchyNodeKind::Document)
        {
            ContentState::Ready
        } else {
            ContentState::Empty
        }
    }

    fn finish_save(&mut self, revision: u64) {
        if revision < self.project_revision {
            self.save.state = SaveState::Dirty {
                current_revision: self.project_revision,
            };
        } else {
            self.save.state = SaveState::SavedThrough(revision);
            self.save.close_waiting = false;
        }
    }
}

fn project_payload_matches(task: &ProjectTask, payload: &ProjectTaskPayload) -> bool {
    matches!(
        (task, payload),
        (
            ProjectTask::GlobalSearch { .. },
            ProjectTaskPayload::SearchBatch { .. } | ProjectTaskPayload::Failed(_)
        ) | (
            ProjectTask::ReplacementPreview,
            ProjectTaskPayload::ReplacementPreviewReady | ProjectTaskPayload::Failed(_)
        ) | (
            ProjectTask::ApplyReplacement,
            ProjectTaskPayload::ReplacementApplied { .. } | ProjectTaskPayload::Failed(_)
        ) | (
            ProjectTask::LoadHistory,
            ProjectTaskPayload::HistoryLoaded { .. } | ProjectTaskPayload::Failed(_)
        ) | (
            ProjectTask::PreviewHistory { .. },
            ProjectTaskPayload::HistoryPreviewReady { .. } | ProjectTaskPayload::Failed(_)
        ) | (
            ProjectTask::PreviewDeleted { .. },
            ProjectTaskPayload::DeletedPreviewReady { .. } | ProjectTaskPayload::Failed(_)
        ) | (
            ProjectTask::RestoreHistory { .. },
            ProjectTaskPayload::HistoryRestored { .. } | ProjectTaskPayload::Failed(_)
        ) | (
            ProjectTask::RestoreDeleted { .. },
            ProjectTaskPayload::DeletedRestored { .. } | ProjectTaskPayload::Failed(_)
        ) | (
            ProjectTask::Save { .. },
            ProjectTaskPayload::SavedThrough(_) | ProjectTaskPayload::Failed(_)
        ) | (
            ProjectTask::Export { .. },
            ProjectTaskPayload::ExportPlanning
                | ProjectTaskPayload::ExportProgress { .. }
                | ProjectTaskPayload::ExportCommitting
                | ProjectTaskPayload::ExportSucceeded { .. }
                | ProjectTaskPayload::ExportCancelled
                | ProjectTaskPayload::Failed(_)
        ) | (
            ProjectTask::ReconcileRecovery,
            ProjectTaskPayload::RecoveryUnavailable
                | ProjectTaskPayload::RecoveryAvailable { .. }
                | ProjectTaskPayload::Failed(_)
        ) | (
            ProjectTask::AcceptRecovery,
            ProjectTaskPayload::RecoveryAccepted { .. }
                | ProjectTaskPayload::RecoveryCanceled
                | ProjectTaskPayload::Failed(_)
        ) | (
            ProjectTask::DiscardRecovery,
            ProjectTaskPayload::RecoveryDiscarded { .. }
                | ProjectTaskPayload::RecoveryCanceled
                | ProjectTaskPayload::Failed(_)
        ) | (
            ProjectTask::PersistWorkspace,
            ProjectTaskPayload::WorkspacePersisted | ProjectTaskPayload::Failed(_)
        )
    )
}

fn project_payload_claim_is_exact(task: &ProjectTask, payload: &ProjectTaskPayload) -> bool {
    match (task, payload) {
        (
            ProjectTask::Save { through_revision },
            ProjectTaskPayload::SavedThrough(saved_revision),
        ) => through_revision == saved_revision,
        (
            ProjectTask::PreviewHistory { checkpoint_id },
            ProjectTaskPayload::HistoryPreviewReady { preview },
        ) => checkpoint_id == &preview.checkpoint.checkpoint_id,
        (
            ProjectTask::PreviewDeleted {
                node_id,
                checkpoint_id,
                document_id,
            },
            ProjectTaskPayload::DeletedPreviewReady {
                node_id: result_node,
                checkpoint_id: result_checkpoint,
                document_id: result_document,
                ..
            },
        ) => {
            node_id == result_node
                && checkpoint_id == result_checkpoint
                && document_id == result_document
        }
        _ => true,
    }
}

fn same_task_family(left: &ProjectTask, right: &ProjectTask) -> bool {
    matches!(
        (left, right),
        (
            ProjectTask::GlobalSearch { .. },
            ProjectTask::GlobalSearch { .. }
        ) | (
            ProjectTask::ReplacementPreview,
            ProjectTask::ReplacementPreview
        ) | (ProjectTask::ApplyReplacement, ProjectTask::ApplyReplacement)
            | (ProjectTask::LoadHistory, ProjectTask::LoadHistory)
            | (
                ProjectTask::PreviewHistory { .. },
                ProjectTask::PreviewHistory { .. }
            )
            | (
                ProjectTask::PreviewDeleted { .. },
                ProjectTask::PreviewDeleted { .. }
            )
            | (
                ProjectTask::RestoreHistory { .. },
                ProjectTask::RestoreHistory { .. }
            )
            | (
                ProjectTask::RestoreDeleted { .. },
                ProjectTask::RestoreDeleted { .. }
            )
            | (ProjectTask::Save { .. }, ProjectTask::Save { .. })
            | (ProjectTask::Export { .. }, ProjectTask::Export { .. })
            | (ProjectTask::AcceptRecovery, ProjectTask::AcceptRecovery)
            | (
                ProjectTask::ReconcileRecovery,
                ProjectTask::ReconcileRecovery
            )
            | (ProjectTask::DiscardRecovery, ProjectTask::DiscardRecovery)
            | (ProjectTask::PersistWorkspace, ProjectTask::PersistWorkspace)
    )
}

fn stable_id_string(bytes: &[u8; 16]) -> String {
    use std::fmt::Write as _;

    let mut serialized = String::with_capacity(32);
    for byte in bytes {
        write!(&mut serialized, "{byte:02x}").expect("writing to a String cannot fail");
    }
    serialized
}

fn stable_id_bytes(value: &str) -> Option<[u8; 16]> {
    if value.len() != 32 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut bytes = [0_u8; 16];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(bytes)
}

fn metadata_id_from_stable(value: &str) -> Option<MetadataFieldId> {
    stable_id_bytes(value).map(MetadataFieldId::from_bytes)
}

fn set_style_property(
    properties: &mut StyleProperties,
    property: StyleProperty,
    value: &str,
) -> bool {
    match property {
        StyleProperty::FontFamily => {
            properties.font_family = optional_trimmed(value).map(str::to_owned)
        }
        StyleProperty::FontSizePoints => {
            match optional_trimmed(value).map(str::parse).transpose() {
                Ok(value) => properties.font_size_points = value,
                Err(_) => return false,
            }
        }
        StyleProperty::Weight => match optional_trimmed(value).map(str::parse).transpose() {
            Ok(value) => properties.weight = value,
            Err(_) => return false,
        },
        StyleProperty::Italic => match optional_trimmed(value).map(str::parse).transpose() {
            Ok(value) => properties.italic = value,
            Err(_) => return false,
        },
        StyleProperty::Alignment => {
            properties.alignment = match optional_trimmed(value) {
                None => None,
                Some("Start") => Some(TextAlignment::Start),
                Some("Center") => Some(TextAlignment::Center),
                Some("End") => Some(TextAlignment::End),
                Some("Justify") => Some(TextAlignment::Justify),
                Some(_) => return false,
            };
        }
        StyleProperty::FirstLineIndentPoints => {
            match optional_trimmed(value).map(str::parse).transpose() {
                Ok(value) => properties.first_line_indent_points = value,
                Err(_) => return false,
            }
        }
        StyleProperty::LeftIndentPoints => {
            match optional_trimmed(value).map(str::parse).transpose() {
                Ok(value) => properties.left_indent_points = value,
                Err(_) => return false,
            }
        }
        StyleProperty::RightIndentPoints => {
            match optional_trimmed(value).map(str::parse).transpose() {
                Ok(value) => properties.right_indent_points = value,
                Err(_) => return false,
            }
        }
        StyleProperty::LineSpacing => match optional_trimmed(value).map(str::parse).transpose() {
            Ok(value) => properties.line_spacing = value,
            Err(_) => return false,
        },
        StyleProperty::SpaceBeforePoints => {
            match optional_trimmed(value).map(str::parse).transpose() {
                Ok(value) => properties.space_before_points = value,
                Err(_) => return false,
            }
        }
        StyleProperty::SpaceAfterPoints => {
            match optional_trimmed(value).map(str::parse).transpose() {
                Ok(value) => properties.space_after_points = value,
                Err(_) => return false,
            }
        }
        StyleProperty::KeepWithNext => match optional_trimmed(value).map(str::parse).transpose() {
            Ok(value) => properties.keep_with_next = value,
            Err(_) => return false,
        },
        StyleProperty::PageBreakBefore => match optional_trimmed(value).map(str::parse).transpose()
        {
            Ok(value) => properties.page_break_before = value,
            Err(_) => return false,
        },
    }
    style_properties_are_finite(properties)
}

fn optional_trimmed(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn style_properties_are_finite(properties: &StyleProperties) -> bool {
    [
        properties.font_size_points,
        properties.first_line_indent_points,
        properties.left_indent_points,
        properties.right_indent_points,
        properties.line_spacing,
        properties.space_before_points,
        properties.space_after_points,
    ]
    .into_iter()
    .flatten()
    .all(f32::is_finite)
}

fn metadata_values_from_project(project: &Project) -> BTreeMap<(String, String), String> {
    project
        .nodes
        .iter()
        .flat_map(|(node_id, node)| {
            let node_id = stable_id_string(node_id.as_bytes());
            node.metadata.iter().map(move |(field_id, value)| {
                (
                    (node_id.clone(), stable_id_string(field_id.as_bytes())),
                    value.clone(),
                )
            })
        })
        .collect()
}

fn is_research_section(section_id: &str) -> bool {
    section_id == "research"
        || section_id == stable_id_string(ProjectSection::Research.root_id().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistence_revision_advances_without_reconciling_live_presentation() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
        let initial = workspace.project_revision();

        workspace.accept_persisted_revision(initial + 2);
        workspace.accept_persisted_revision(initial + 1);

        assert_eq!(workspace.project_revision(), initial + 2);
    }

    #[test]
    fn editor_dirty_state_does_not_repurpose_the_project_revision() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
        let initial = workspace.project_revision();

        workspace.update(ProjectMessage::MarkEditorDirty);

        assert_eq!(workspace.project_revision(), initial);
        assert_eq!(
            workspace.save().state(),
            SaveState::Dirty {
                current_revision: initial
            }
        );
    }

    fn history_row(
        checkpoint_id: &str,
        category: HistoryCheckpointCategory,
        name: Option<&str>,
        affected_document_ids: Vec<&str>,
    ) -> HistoryCheckpointRow {
        HistoryCheckpointRow {
            checkpoint_id: checkpoint_id.to_owned(),
            sequence: 7,
            category,
            affected_document_ids: affected_document_ids
                .into_iter()
                .map(str::to_owned)
                .collect(),
            name: name.map(str::to_owned),
        }
    }

    #[test]
    fn history_projects_category_name_and_document_summary_without_ids() {
        let workspace = ProjectWorkspace::from_fixture(ProjectFixture::History);
        let row = &workspace.history().checkpoints()[0];

        assert_eq!(row.label(), "Draft Two");
        assert_eq!(row.affected_summary(), "1 document");
        assert_eq!(row.recorded_at_unix_millis(), None);
        assert_eq!(row.category, HistoryCheckpointCategory::NamedSnapshot);
        assert_eq!(row.affected_document_ids.len(), 1);
        assert_eq!(
            HistoryCheckpointCategory::Restoration.label(),
            "Restoration"
        );
    }

    #[test]
    fn history_comparison_projects_numbered_rows_and_changed_spans_from_loaded_semantics() {
        fn semantic(lines: &[&str]) -> SemanticDocument {
            SemanticDocument::new(
                lines
                    .iter()
                    .enumerate()
                    .map(|(index, text)| {
                        parchmint_editor_api::SemanticBlock::new(
                            parchmint_editor_api::BlockId::from_bytes([index as u8; 16]),
                            parchmint_editor_api::SemanticBlockKind::Paragraph,
                            None,
                            *text,
                            Vec::new(),
                        )
                    })
                    .collect(),
            )
        }

        let mut history = HistoryState {
            preview: Some(HistoryPreviewData {
                checkpoint: history_row(
                    "checkpoint-7",
                    HistoryCheckpointCategory::Autosave,
                    None,
                    vec!["chapter-one"],
                ),
                resource_paths: vec!["documents/chapter-one.html".to_owned()],
                document: Some(HistoryDocumentPreview {
                    document_id: "chapter-one".to_owned(),
                    canonical_path: "documents/chapter-one.html".to_owned(),
                    semantic: semantic(&["The blue house", "Keep", "Remove me"]),
                }),
            }),
            current_document: Some(HistoryCurrentDocument {
                document_id: "chapter-one".to_owned(),
                title: "Chapter One".to_owned(),
                body: "<p>The green house</p><p>Keep</p><p>Added one</p><p>Added two</p>"
                    .to_owned(),
                semantic: semantic(&["The green house", "Keep", "Added one", "Added two"]),
            }),
            ..HistoryState::default()
        };
        history.refresh_comparison();

        let comparison = history.comparison().expect("same document is comparable");
        assert_eq!(comparison.checkpoint_id, "checkpoint-7");
        assert_eq!(comparison.document_title, "Chapter One");
        assert_eq!(
            comparison.change_summary(),
            HistoryChangeSummary {
                added_lines: 1,
                removed_lines: 0,
                modified_lines: 2,
            }
        );
        assert_eq!(
            comparison.lines[0].kind,
            HistoryComparisonLineKind::Modified
        );
        assert_eq!(
            comparison.lines[0]
                .before
                .as_ref()
                .map(|line| line.line_number),
            Some(1)
        );
        assert_eq!(
            comparison.lines[0]
                .after
                .as_ref()
                .map(|line| line.line_number),
            Some(1)
        );
        assert_eq!(
            comparison.lines[0]
                .before
                .as_ref()
                .expect("modified before line")
                .spans,
            [
                HistoryComparisonSpan {
                    kind: HistoryComparisonSpanKind::Unchanged,
                    text: "The ".to_owned(),
                },
                HistoryComparisonSpan {
                    kind: HistoryComparisonSpanKind::Removed,
                    text: "blue".to_owned(),
                },
                HistoryComparisonSpan {
                    kind: HistoryComparisonSpanKind::Unchanged,
                    text: " house".to_owned(),
                },
            ]
        );
        assert_eq!(
            comparison.lines[1].kind,
            HistoryComparisonLineKind::Unchanged
        );
        assert_eq!(
            comparison.lines[2].kind,
            HistoryComparisonLineKind::Modified
        );
        assert_eq!(comparison.lines[3].kind, HistoryComparisonLineKind::Added);
        assert!(comparison.lines[3].before.is_none());
        assert_eq!(
            comparison.lines[3]
                .after
                .as_ref()
                .map(|line| line.line_number),
            Some(4)
        );
    }

    #[test]
    fn history_comparison_refuses_unrelated_loaded_documents() {
        let mut history = HistoryState {
            preview: Some(HistoryPreviewData {
                checkpoint: history_row(
                    "checkpoint-7",
                    HistoryCheckpointCategory::Autosave,
                    None,
                    vec!["chapter-one"],
                ),
                resource_paths: Vec::new(),
                document: Some(HistoryDocumentPreview {
                    document_id: "chapter-one".to_owned(),
                    canonical_path: "documents/chapter-one.html".to_owned(),
                    semantic: SemanticDocument::default(),
                }),
            }),
            current_document: Some(HistoryCurrentDocument {
                document_id: "chapter-two".to_owned(),
                title: "Chapter Two".to_owned(),
                body: String::new(),
                semantic: SemanticDocument::default(),
            }),
            ..HistoryState::default()
        };
        history.refresh_comparison();

        assert_eq!(history.comparison(), None);
    }

    #[test]
    fn settings_navigation_keeps_all_design_categories_and_refuses_unavailable_global_words() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::SettingsAppearance);
        assert_eq!(
            workspace
                .settings()
                .categories()
                .map(|category| category.label),
            [
                "General",
                "Appearance",
                "Styles",
                "Metadata fields",
                "Dictionaries",
            ]
        );

        workspace.update(ProjectMessage::SelectSettingsCategory(
            SettingsCategory::Dictionaries,
        ));
        assert_eq!(
            workspace.settings().selected_category(),
            SettingsCategory::Dictionaries
        );

        let dictionaries = workspace.settings().dictionaries();
        assert_eq!(dictionaries.language(), "en-US");
        assert_eq!(dictionaries.selected_scope(), DictionaryScope::Project);
        assert_eq!(
            dictionaries
                .scopes()
                .map(|scope| (scope.scope, scope.available, scope.selected)),
            [
                (DictionaryScope::Project, true, true),
                (DictionaryScope::Global, false, false),
            ]
        );

        workspace.update(ProjectMessage::SelectDictionaryScope(
            DictionaryScope::Global,
        ));
        assert_eq!(
            workspace.settings().dictionaries().selected_scope(),
            DictionaryScope::Project
        );
    }

    #[test]
    fn dictionary_settings_reads_project_words_without_copying_global_preferences() {
        let project_id = parchmint_domain::ProjectId::from_bytes([0x47; 16]);
        let mut project = Project::new(project_id);
        project.dictionary.insert("harbor").unwrap();
        let workspace = ProjectWorkspace::from_snapshot(&ProjectSnapshot {
            project,
            document_summaries: Vec::new(),
            documents: Vec::new(),
            styles_css: String::new(),
        });

        assert_eq!(
            workspace.settings().dictionaries().words(),
            Some(["harbor".to_owned()].as_slice())
        );
        assert!(
            !workspace
                .settings()
                .dictionaries()
                .scope_available(DictionaryScope::Global)
        );
    }

    #[test]
    fn recovery_summary_resolves_titles_without_inventing_missing_display_data() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::ErrorRecovery);
        let ticket = workspace.begin_task(ProjectTask::ReconcileRecovery);
        assert!(
            workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                ticket,
                ProjectTaskPayload::RecoveryAvailable {
                    accepted_records: 2,
                    affected_documents: vec![("chapter-one".to_owned(), 8)],
                    isolation: None,
                },
            ))
        );

        assert_eq!(workspace.recovery().accepted_records(), 2);
        assert_eq!(
            workspace.recovery_summary(),
            [RecoveryDocumentSummary {
                document_id: "chapter-one",
                display_title: Some("Chapter One"),
                recovered_word_count: None,
                last_edit: None,
                revision: 8,
            }]
        );
        assert_eq!(
            workspace.recovery().history_preservation(),
            RecoveryHistoryPreservation::Unavailable
        );
    }

    #[test]
    fn named_snapshot_trims_name_and_exposes_success_or_error_state() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::History);
        assert!(
            workspace
                .update(ProjectMessage::RequestNamedSnapshot("   ".to_owned()))
                .is_empty()
        );
        assert_eq!(
            workspace.history().error(),
            Some("A snapshot name is required.")
        );

        let effects = workspace.update(ProjectMessage::RequestNamedSnapshot(
            " Before launch ".to_owned(),
        ));
        assert_eq!(
            effects,
            [ProjectEffect::CreateNamedSnapshot(
                "Before launch".to_owned()
            )]
        );
        assert!(workspace.history().is_creating_named_snapshot());
        workspace.complete_history_workflow();
        assert!(!workspace.history().is_creating_named_snapshot());
        assert_eq!(workspace.history().named_snapshot_draft(), "");

        workspace.fail_history_workflow("duplicate snapshot name".to_owned());
        assert_eq!(workspace.history().error(), Some("duplicate snapshot name"));
    }

    #[test]
    fn history_active_document_filter_and_preview_reject_stale_selection() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::History);
        workspace.finish_history_page(Some("old-page".to_owned()));
        workspace.update(ProjectMessage::SetHistoryDocumentFilter(Some(
            "chapter-one".to_owned(),
        )));
        assert_eq!(
            workspace.history().active_document_filter(),
            Some("chapter-one")
        );
        assert_eq!(workspace.history().visible_checkpoints().count(), 0);
        assert_eq!(workspace.history().next_cursor(), None);

        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::History);
        let first = workspace.begin_task(ProjectTask::PreviewHistory {
            checkpoint_id: "snapshot-draft-two".to_owned(),
        });
        let second = workspace.begin_task(ProjectTask::PreviewHistory {
            checkpoint_id: "autosave-17".to_owned(),
        });
        let first_preview = HistoryPreviewData {
            checkpoint: history_row(
                "snapshot-draft-two",
                HistoryCheckpointCategory::NamedSnapshot,
                Some("Draft Two"),
                vec!["chapter-one"],
            ),
            resource_paths: vec!["documents/chapter-one.json".to_owned()],
            document: None,
        };
        assert!(
            !workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                first,
                ProjectTaskPayload::HistoryPreviewReady {
                    preview: first_preview,
                },
            ))
        );

        let preview = HistoryPreviewData {
            checkpoint: history_row(
                "autosave-17",
                HistoryCheckpointCategory::Autosave,
                None,
                vec!["chapter-one"],
            ),
            resource_paths: vec!["documents/chapter-one.json".to_owned()],
            document: None,
        };
        assert!(
            workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                second,
                ProjectTaskPayload::HistoryPreviewReady {
                    preview: preview.clone(),
                },
            ))
        );
        assert_eq!(workspace.history().preview(), Some(&preview));

        workspace.set_history_current_document(Some(HistoryCurrentDocument {
            document_id: "chapter-one".to_owned(),
            title: "Chapter One".to_owned(),
            body: "<p>Current text</p>".to_owned(),
            semantic: SemanticDocument::default(),
        }));
        assert_eq!(
            workspace
                .history()
                .current_document()
                .map(|document| document.body.as_str()),
            Some("<p>Current text</p>")
        );
    }

    #[test]
    fn reopened_deleted_tombstone_requests_and_accepts_only_its_checkpoint_content() {
        let node = parchmint_domain::NodeId::from_bytes([0x31; 16]);
        let document = parchmint_domain::DocumentId::from_bytes([0x32; 16]);
        let checkpoint = parchmint_domain::CheckpointId::from_bytes([0x33; 16]);
        let mut project = Project::new(parchmint_domain::ProjectId::from_bytes([0x34; 16]));
        project = parchmint_domain::apply_project_command(
            &project,
            project.revision,
            parchmint_domain::ProjectCommand::create_document(
                node,
                document,
                parchmint_domain::NodeId::manuscript_root(),
                0,
                "Deleted chapter",
            ),
        )
        .unwrap()
        .project;
        project = parchmint_domain::apply_project_command(
            &project,
            project.revision,
            parchmint_domain::ProjectCommand::delete_node_from_checkpoint(node, 99, checkpoint),
        )
        .unwrap()
        .project;
        let mut workspace = ProjectWorkspace::from_snapshot(&ProjectSnapshot {
            project,
            document_summaries: Vec::new(),
            documents: Vec::new(),
            styles_css: String::new(),
        });
        let effect = workspace
            .selected_deleted_preview_effect()
            .expect("reopened tombstone should request History content");
        let ProjectEffect::PreviewDeleted {
            node_id,
            checkpoint_id,
            document_id,
        } = effect
        else {
            panic!("unexpected deleted preview effect")
        };
        let ticket = workspace.begin_task(ProjectTask::PreviewDeleted {
            node_id: node_id.clone(),
            checkpoint_id: checkpoint_id.clone(),
            document_id: document_id.clone(),
        });
        assert!(
            !workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                ticket.clone(),
                ProjectTaskPayload::DeletedPreviewReady {
                    node_id: node_id.clone(),
                    checkpoint_id: "stale".into(),
                    document_id: document_id.clone(),
                    semantic: SemanticDocument::default(),
                },
            ))
        );
        assert!(
            workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                ticket,
                ProjectTaskPayload::DeletedPreviewReady {
                    node_id,
                    checkpoint_id,
                    document_id,
                    semantic: SemanticDocument::default(),
                },
            ))
        );
        assert!(workspace.recently_deleted().selected_preview().is_some());
    }

    #[test]
    fn stale_session_request_payload_and_revision_are_rejected() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::GlobalSearch);
        let first = workspace.begin_task(ProjectTask::ReplacementPreview);
        let second = workspace.begin_task(ProjectTask::ReplacementPreview);
        assert!(
            !workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                first,
                ProjectTaskPayload::ReplacementPreviewReady,
            ))
        );
        assert!(
            !workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                second.clone(),
                ProjectTaskPayload::HistoryPreviewReady {
                    preview: HistoryPreviewData {
                        checkpoint: HistoryCheckpointRow {
                            checkpoint_id: "checkpoint".to_owned(),
                            sequence: 1,
                            category: HistoryCheckpointCategory::Autosave,
                            affected_document_ids: Vec::new(),
                            name: None,
                        },
                        resource_paths: Vec::new(),
                        document: None,
                    },
                },
            ))
        );

        workspace.update(ProjectMessage::MarkDirty(2));
        assert!(
            !workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                second,
                ProjectTaskPayload::ReplacementPreviewReady,
            ))
        );

        let old_session = workspace.begin_task(ProjectTask::LoadHistory);
        workspace.begin_session(38, 2);
        assert!(
            !workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                old_session,
                ProjectTaskPayload::HistoryLoaded {
                    checkpoints: Vec::new(),
                },
            ))
        );
    }

    #[test]
    fn streaming_search_keeps_only_its_exact_live_request() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::GlobalSearch);
        let ticket = workspace.begin_task(ProjectTask::GlobalSearch { generation: 0 });
        assert!(
            workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                ticket.clone(),
                ProjectTaskPayload::SearchBatch {
                    results: Vec::new(),
                    finished: false,
                },
            ))
        );
        assert!(
            workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                ticket,
                ProjectTaskPayload::SearchBatch {
                    results: Vec::new(),
                    finished: true,
                },
            ))
        );
    }

    #[test]
    fn a_new_query_invalidates_the_older_search_family_ticket() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::GlobalSearch);
        workspace.update(ProjectMessage::SetGlobalSearchQuery("river".to_owned()));
        let first = workspace.begin_task(ProjectTask::GlobalSearch { generation: 1 });
        workspace.update(ProjectMessage::SetGlobalSearchQuery("bridge".to_owned()));
        let second = workspace.begin_task(ProjectTask::GlobalSearch { generation: 2 });

        assert!(
            !workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                first,
                ProjectTaskPayload::SearchBatch {
                    results: Vec::new(),
                    finished: true,
                },
            ))
        );
        assert!(
            workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                second,
                ProjectTaskPayload::SearchBatch {
                    results: Vec::new(),
                    finished: true,
                },
            ))
        );
    }

    #[test]
    fn a_save_payload_cannot_claim_a_different_frontier_than_its_ticket() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
        let ticket = workspace.begin_task(ProjectTask::Save {
            through_revision: 1,
        });
        assert!(
            !workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                ticket,
                ProjectTaskPayload::SavedThrough(2),
            ))
        );
    }

    #[test]
    fn pointer_drag_commits_only_a_live_validated_source_and_target() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
        workspace.update(ProjectMessage::BeginHierarchyDrag {
            source_id: "chapter-one".to_owned(),
            gesture: SelectionGesture::Replace,
        });
        assert_eq!(workspace.hierarchy_drag_source(), Some("chapter-one"));
        let destination = DragDestination::AfterSibling("chapter-two".to_owned());
        workspace.update(ProjectMessage::SetDragDestination(Some(
            destination.clone(),
        )));
        assert_eq!(workspace.hierarchy_drag_destination(), Some(&destination));
        assert!(matches!(
            workspace.update(ProjectMessage::CommitHierarchyDrag).as_slice(),
            [ProjectEffect::MoveHierarchy { destination: actual, .. }] if actual == &destination
        ));
        assert_eq!(workspace.hierarchy_drag_source(), None);

        workspace.update(ProjectMessage::BeginHierarchyDrag {
            source_id: "chapter-one".to_owned(),
            gesture: SelectionGesture::Replace,
        });
        workspace.update(ProjectMessage::SetDragDestination(Some(
            DragDestination::AfterSibling("chapter-two".to_owned()),
        )));
        workspace.explorer.nodes.remove("chapter-one");
        assert!(
            workspace
                .update(ProjectMessage::CommitHierarchyDrag)
                .is_empty()
        );
        assert_eq!(workspace.hierarchy_drag_source(), None);
    }

    #[test]
    fn pointer_drag_preserves_an_existing_multi_selection() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
        workspace.update(ProjectMessage::SelectHierarchy {
            node_id: "chapter-one".to_owned(),
            gesture: SelectionGesture::Replace,
        });
        workspace.update(ProjectMessage::SelectHierarchy {
            node_id: "chapter-two".to_owned(),
            gesture: SelectionGesture::Additive,
        });
        workspace.update(ProjectMessage::BeginHierarchyDrag {
            source_id: "chapter-one".to_owned(),
            gesture: SelectionGesture::Replace,
        });
        workspace.update(ProjectMessage::SetDragDestination(Some(
            DragDestination::AfterSibling("chapter-three".to_owned()),
        )));
        assert!(matches!(
            workspace.update(ProjectMessage::CommitHierarchyDrag).as_slice(),
            [ProjectEffect::MoveHierarchy { node_ids, .. }]
                if node_ids == &vec!["chapter-one".to_owned(), "chapter-two".to_owned()]
        ));
    }

    #[test]
    fn export_requires_an_explicit_destination_before_starting() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Export);
        assert!(!workspace.export().can_start());
        assert_eq!(
            workspace.update(ProjectMessage::BrowseExportDestination),
            vec![ProjectEffect::ChooseExportDestination {
                output_name: "manuscript.html".to_owned(),
            }]
        );
        workspace.update(ProjectMessage::SetExportDestination(Some(
            "/tmp/manuscript.html".to_owned(),
        )));
        assert!(workspace.export().can_start());
        assert!(matches!(
            workspace.update(ProjectMessage::StartExport).as_slice(),
            [ProjectEffect::ExportEntireManuscript { .. }]
        ));
    }

    #[test]
    fn history_reinitialize_requires_availability_and_confirmation() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::History);
        workspace.update(ProjectMessage::HistoryMaintenanceLoaded(
            HistoryMaintenanceStatus::Reinitializable {
                problem: "damaged store".to_owned(),
            },
        ));
        assert!(
            workspace
                .update(ProjectMessage::RequestHistoryReinitialize)
                .is_empty()
        );
        assert_eq!(workspace.modal(), Some(ProjectModal::ReinitializeHistory));
        assert_eq!(
            workspace.update(ProjectMessage::ConfirmHistoryReinitialize),
            vec![ProjectEffect::ReinitializeHistory]
        );
    }

    #[test]
    fn search_and_history_windows_follow_scroll_offsets_without_folding_all_rows() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::GlobalSearch);
        workspace.global_search.results = (0..200)
            .map(|index| GlobalSearchResult {
                document_id: "chapter-one".to_owned(),
                match_id: format!("match-{index}"),
                prefix: String::new(),
                matching_text: "x".to_owned(),
                suffix: String::new(),
                indexed_revision: 1,
            })
            .collect();
        workspace.update(ProjectMessage::SetGlobalSearchScroll(4_400.0));
        assert_eq!(workspace.global_search().result_window_start(), 100);
        assert_eq!(workspace.global_search().windowed_results().count(), 80);

        workspace.history.checkpoints = (0..200)
            .map(|index| HistoryCheckpointRow {
                checkpoint_id: format!("checkpoint-{index}"),
                sequence: index,
                category: HistoryCheckpointCategory::Autosave,
                affected_document_ids: Vec::new(),
                name: None,
            })
            .collect();
        workspace.update(ProjectMessage::SetHistoryScroll(7_200.0));
        assert_eq!(workspace.history().checkpoint_window_start(), 100);
        assert_eq!(workspace.history().windowed_checkpoints().count(), 60);
    }

    #[test]
    fn pointer_drag_and_context_menu_cancel_without_dispatching_a_move() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
        workspace.update(ProjectMessage::BeginHierarchyDrag {
            source_id: "chapter-one".to_owned(),
            gesture: SelectionGesture::Replace,
        });
        workspace.update(ProjectMessage::SetDragDestination(Some(
            DragDestination::BeforeSibling("chapter-two".to_owned()),
        )));
        assert!(
            workspace
                .update(ProjectMessage::CancelHierarchyDrag)
                .is_empty()
        );
        assert_eq!(workspace.hierarchy_drag_source(), None);

        workspace.update(ProjectMessage::OpenHierarchyContextMenu {
            node_id: "chapter-two".to_owned(),
            point: Point::new(84.0, 12.0),
        });
        assert_eq!(workspace.hierarchy_context_menu(), Some("chapter-two"));
        assert_eq!(workspace.hierarchy_context_point(), Point::new(84.0, 12.0));
        assert_eq!(workspace.explorer().selected_ids(), ["chapter-two"]);
        workspace.update(ProjectMessage::CopySelection);
        assert_eq!(workspace.hierarchy_context_menu(), None);
    }

    #[test]
    fn hierarchy_rename_edits_inline_and_commits_only_when_submitted() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);

        assert!(
            workspace
                .update(ProjectMessage::BeginHierarchyRename(
                    "chapter-one".to_owned()
                ))
                .is_empty()
        );
        assert_eq!(
            workspace.hierarchy_rename(),
            Some(("chapter-one", "Chapter One"))
        );
        workspace.update(ProjectMessage::SetHierarchyRenameDraft(
            "Opening Scene".to_owned(),
        ));
        assert_eq!(
            workspace
                .explorer()
                .row("chapter-one")
                .map(|node| node.title),
            Some("Chapter One")
        );

        assert_eq!(
            workspace.update(ProjectMessage::CommitHierarchyRename),
            vec![ProjectEffect::CommitNodeTitle {
                node_id: "chapter-one".to_owned(),
                title: "Opening Scene".to_owned(),
            }]
        );
        assert_eq!(workspace.hierarchy_rename(), None);
        assert_eq!(
            workspace
                .explorer()
                .row("chapter-one")
                .map(|node| node.title),
            Some("Opening Scene")
        );
    }

    #[test]
    fn created_hierarchy_enters_inline_rename_after_its_authoritative_snapshot_arrives() {
        let parent = parchmint_domain::NodeId::manuscript_root();
        let parent_id = stable_id_string(parent.as_bytes());
        let project = Project::new(parchmint_domain::ProjectId::from_bytes([0x71; 16]));
        let initial = ProjectSnapshot {
            project: project.clone(),
            document_summaries: Vec::new(),
            documents: Vec::new(),
            styles_css: String::new(),
        };
        let mut workspace = ProjectWorkspace::from_snapshot(&initial);

        assert_eq!(
            workspace.update(ProjectMessage::RequestCreateHierarchy {
                parent_id: parent_id.clone(),
                kind: HierarchyItemKind::Group,
            }),
            [ProjectEffect::CreateHierarchy {
                parent_id: parent_id.clone(),
                kind: HierarchyItemKind::Group,
            }]
        );
        // An unrelated refresh before the create workflow completes must not
        // consume the pending inline-rename request.
        workspace.reconcile_snapshot(&initial);
        assert_eq!(workspace.hierarchy_rename(), None);

        let created_node = parchmint_domain::NodeId::from_bytes([0x72; 16]);
        let created = parchmint_domain::apply_project_command(
            &project,
            project.revision,
            parchmint_domain::ProjectCommand::create_group(created_node, parent, 0, "New Group"),
        )
        .expect("create group snapshot")
        .project;
        let created_id = stable_id_string(created_node.as_bytes());
        workspace.reconcile_snapshot(&ProjectSnapshot {
            project: created,
            document_summaries: Vec::new(),
            documents: Vec::new(),
            styles_css: String::new(),
        });

        assert_eq!(
            workspace.hierarchy_rename(),
            Some((created_id.as_str(), "New Group"))
        );
        assert_eq!(workspace.explorer().selected_ids(), [created_id.as_str()]);
        assert!(workspace.explorer().is_expanded(&parent_id));
    }

    #[test]
    fn created_document_enters_inline_rename_after_its_authoritative_snapshot_arrives() {
        let parent = parchmint_domain::NodeId::manuscript_root();
        let parent_id = stable_id_string(parent.as_bytes());
        let project = Project::new(parchmint_domain::ProjectId::from_bytes([0x73; 16]));
        let mut workspace = ProjectWorkspace::from_snapshot(&ProjectSnapshot {
            project: project.clone(),
            document_summaries: Vec::new(),
            documents: Vec::new(),
            styles_css: String::new(),
        });
        workspace.update(ProjectMessage::RequestCreateHierarchy {
            parent_id: parent_id.clone(),
            kind: HierarchyItemKind::Document,
        });

        let created_node = parchmint_domain::NodeId::from_bytes([0x74; 16]);
        let created = parchmint_domain::apply_project_command(
            &project,
            project.revision,
            parchmint_domain::ProjectCommand::create_document(
                created_node,
                parchmint_domain::DocumentId::from_bytes([0x75; 16]),
                parent,
                0,
                "Untitled",
            ),
        )
        .expect("create document snapshot")
        .project;
        let created_id = stable_id_string(created_node.as_bytes());
        workspace.reconcile_snapshot(&ProjectSnapshot {
            project: created,
            document_summaries: Vec::new(),
            documents: Vec::new(),
            styles_css: String::new(),
        });

        assert_eq!(
            workspace.hierarchy_rename(),
            Some((created_id.as_str(), "Untitled"))
        );
        assert_eq!(workspace.explorer().selected_ids(), [created_id.as_str()]);
    }

    #[test]
    fn project_undo_and_redo_are_explicit_project_effects() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);

        assert_eq!(
            workspace.update(ProjectMessage::UndoProject),
            [ProjectEffect::UndoProject]
        );
        assert_eq!(
            workspace.update(ProjectMessage::RedoProject),
            [ProjectEffect::RedoProject]
        );
    }

    #[test]
    fn group_clipboard_survives_navigation_and_cut_clears_only_on_completion() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
        workspace.update(ProjectMessage::SelectHierarchy {
            node_id: "part-one".to_owned(),
            gesture: SelectionGesture::Replace,
        });
        workspace.update(ProjectMessage::SelectHierarchy {
            node_id: "chapter-three".to_owned(),
            gesture: SelectionGesture::Additive,
        });
        assert!(workspace.update(ProjectMessage::CopySelection).is_empty());
        assert_eq!(
            workspace.tree_clipboard_kind(),
            Some(TreeClipboardKind::Copy)
        );
        workspace.update(ProjectMessage::ShowGlobalSearch);
        workspace.update(ProjectMessage::ShowExplorer);
        assert_eq!(
            workspace.update(ProjectMessage::PasteSelection {
                destination: DragDestination::IntoGroup("research".to_owned()),
            }),
            [ProjectEffect::PasteCopiedSubtrees {
                node_ids: vec!["part-one".to_owned(), "chapter-three".to_owned()],
                destination: DragDestination::IntoGroup("research".to_owned()),
            }]
        );
        assert!(workspace.update(ProjectMessage::CutSelection).is_empty());
        assert!(workspace.explorer().is_cut_pending("part-one"));
        assert!(workspace.explorer().is_cut_pending("chapter-three"));
        assert_eq!(
            workspace.tree_clipboard_kind(),
            Some(TreeClipboardKind::Cut)
        );
        assert_eq!(
            workspace.update(ProjectMessage::PasteSelection {
                destination: DragDestination::IntoGroup("research".to_owned()),
            }),
            [ProjectEffect::PasteCutSubtrees {
                node_ids: vec!["part-one".to_owned(), "chapter-three".to_owned()],
                destination: DragDestination::IntoGroup("research".to_owned()),
            }]
        );
        assert!(workspace.explorer().is_cut_pending("part-one"));
        workspace.complete_tree_paste(TreeClipboardKind::Cut);
        assert!(!workspace.explorer().is_cut_pending("part-one"));
        assert_eq!(workspace.tree_clipboard_kind(), None);
    }

    #[test]
    fn new_project_session_rejects_the_previous_tree_clipboard_payload() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
        workspace.update(ProjectMessage::SelectHierarchy {
            node_id: "chapter-one".to_owned(),
            gesture: SelectionGesture::Replace,
        });
        workspace.update(ProjectMessage::CopySelection);
        workspace.begin_session(38, 1);

        assert_eq!(workspace.tree_clipboard_kind(), None);
        assert!(
            workspace
                .update(ProjectMessage::PasteSelection {
                    destination: DragDestination::IntoGroup("research".to_owned()),
                })
                .is_empty()
        );
    }

    #[test]
    fn copied_roots_become_the_selection_without_consuming_the_copy_payload() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
        workspace.update(ProjectMessage::SelectHierarchy {
            node_id: "chapter-one".to_owned(),
            gesture: SelectionGesture::Replace,
        });
        workspace.update(ProjectMessage::CopySelection);

        workspace.select_tree_roots(&["chapter-three".to_owned(), "research-notes".to_owned()]);
        workspace.complete_tree_paste(TreeClipboardKind::Copy);

        assert_eq!(
            workspace.explorer().selected_ids(),
            ["chapter-three", "research-notes"]
        );
        assert_eq!(
            workspace.tree_clipboard_kind(),
            Some(TreeClipboardKind::Copy)
        );
    }

    #[test]
    fn research_documents_default_to_the_companion_editor_pane() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
        assert_eq!(
            workspace.update(ProjectMessage::OpenHierarchyNode(
                "research-notes".to_owned()
            )),
            [ProjectEffect::OpenDocumentInCompanion(
                "research-notes".to_owned()
            )]
        );
        assert_eq!(
            workspace
                .editor()
                .pane(EditorPane::Companion)
                .active_document(),
            Some("research-notes")
        );
    }

    #[test]
    fn explorer_previews_replace_only_the_unpinned_primary_tab() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);

        workspace.update(ProjectMessage::PreviewHierarchyNode(
            "chapter-three".to_owned(),
        ));
        workspace.update(ProjectMessage::PreviewHierarchyNode(
            "chapter-two".to_owned(),
        ));

        let primary = workspace.editor().pane(EditorPane::Primary);
        assert_eq!(primary.tabs().len(), 2);
        assert_eq!(primary.active_document(), Some("chapter-two"));
        assert!(primary.tabs()[1].is_preview());

        workspace.update(ProjectMessage::OpenHierarchyNode("chapter-two".to_owned()));
        workspace.update(ProjectMessage::PreviewHierarchyNode(
            "chapter-three".to_owned(),
        ));

        let primary = workspace.editor().pane(EditorPane::Primary);
        assert_eq!(
            primary.tabs().iter().map(TabSpec::id).collect::<Vec<_>>(),
            ["chapter-one", "chapter-two", "chapter-three"]
        );
        assert!(!primary.tabs()[1].is_preview());
        assert!(primary.tabs()[2].is_preview());
    }

    #[test]
    fn save_completion_for_an_older_captured_revision_leaves_later_edits_dirty() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
        let ticket = workspace.begin_task(ProjectTask::Save {
            through_revision: 1,
        });
        workspace.update(ProjectMessage::MarkDirty(2));
        assert!(
            workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                ticket,
                ProjectTaskPayload::SavedThrough(1),
            ))
        );
        assert_eq!(
            workspace.save().state(),
            SaveState::Dirty {
                current_revision: 2
            }
        );
    }

    #[test]
    fn failed_async_work_uses_the_shared_error_modal_without_exposing_technical_detail() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
        let ticket = workspace.begin_task(ProjectTask::Export { source_revision: 1 });

        assert!(
            workspace.accept_completion(ProjectTaskCompletion::for_ticket(
                ticket,
                ProjectTaskPayload::Failed(
                    "export writer rejected /private/author-notes".to_owned()
                ),
            ))
        );
        assert!(matches!(
            workspace.modal(),
            Some(ProjectModal::Error { title, detail })
                if title == "Couldn't export your project"
                    && !detail.contains("author-notes")
        ));
    }

    #[test]
    fn direct_save_failures_use_the_shared_error_modal() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
        workspace.update(ProjectMessage::SaveFailed(
            "filesystem unavailable".to_owned(),
        ));

        assert!(matches!(
            workspace.modal(),
            Some(ProjectModal::Error { title, .. }) if title == "Couldn't save changes"
        ));
    }

    #[test]
    fn card_activation_mounts_the_document_in_the_stage_36_editor_workspace() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Cards);
        workspace.update(ProjectMessage::ActivateCard("chapter-three".to_owned()));
        assert_eq!(
            workspace
                .editor()
                .pane(EditorPane::Primary)
                .active_document(),
            Some("chapter-three")
        );
    }

    #[test]
    fn cards_selection_is_shared_with_the_inspector_context() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Cards);
        workspace.update(ProjectMessage::SelectHierarchy {
            node_id: "part-one".to_owned(),
            gesture: SelectionGesture::Replace,
        });
        assert!(
            workspace
                .cards()
                .items()
                .iter()
                .any(|item| item.node_id == "part-one" && item.selected)
        );
        assert_eq!(workspace.explorer().selected_ids(), ["part-one"]);
    }

    #[test]
    fn focusing_an_editor_document_reveals_its_collapsed_explorer_ancestors() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
        workspace.update(ProjectMessage::ToggleHierarchyExpanded(
            "part-one".to_owned(),
        ));
        workspace
            .editor_mut()
            .update(EditorMessage::FocusPane(EditorPane::Companion));

        workspace.reveal_focused_editor_document();

        assert_eq!(workspace.explorer().selected_ids(), ["chapter-two"]);
        assert!(workspace.explorer().is_expanded("part-one"));
    }

    #[test]
    fn synopsis_editor_preserves_multiline_text_and_commits_only_edits() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);

        assert!(
            workspace
                .update(ProjectMessage::EditSynopsis {
                    node_id: "chapter-one".to_owned(),
                    action: text_editor::Action::Move(text_editor::Motion::End),
                })
                .is_empty()
        );

        let effects = workspace.update(ProjectMessage::EditSynopsis {
            node_id: "chapter-one".to_owned(),
            action: text_editor::Action::Edit(text_editor::Edit::Enter),
        });
        let [ProjectEffect::CommitSynopsis { node_id, synopsis }] = effects.as_slice() else {
            panic!("a multiline synopsis edit must use the existing persistence effect");
        };
        assert_eq!(node_id, "chapter-one");
        assert!(synopsis.contains('\n'));
        assert_eq!(
            workspace.explorer().synopsis("chapter-one"),
            Some(synopsis.as_str())
        );
    }
}
