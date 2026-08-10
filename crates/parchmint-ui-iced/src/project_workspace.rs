//! Deterministic presentation state for project-facing workspace views.
//!
//! Service crates remain responsible for project mutation, History, search,
//! export, preferences, and persistence. This module validates UI intent,
//! retains temporary view state, and emits effects for the integration layer.

use std::collections::{BTreeMap, BTreeSet};

use parchmint_preferences::{AppearanceMode, ResolvedAppearance};

use crate::{EditorFixture, EditorMessage, EditorPane, EditorWorkspace, TabSpec};

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
    IntoGroup(String),
    EditorPane(EditorPane),
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

#[derive(Debug, Clone)]
struct HierarchyNode {
    id: String,
    title: String,
    section_id: String,
    parent: Option<String>,
    children: Vec<String>,
    kind: HierarchyNodeKind,
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
            synopsis: String::new(),
        }
    }
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
            DragDestination::BeforeSibling(target_id) => {
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
                    .is_none_or(|node| node.kind != HierarchyNodeKind::Document)
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

/// Cards-specific projection over the shared hierarchy state.
pub struct CardsState<'a> {
    explorer: &'a ExplorerState,
    section_id: &'a str,
    drag_destination: Option<&'a DragDestination>,
    last_activated_document: Option<&'a str>,
    visible_metadata_labels: Vec<&'a str>,
}

impl CardsState<'_> {
    pub fn section_id(&self) -> &str {
        self.section_id
    }

    pub const fn shows_hierarchy(&self) -> bool {
        true
    }

    pub fn drag_destination(&self) -> Option<&DragDestination> {
        self.drag_destination
    }

    pub fn title_is_editable(&self, node_id: &str) -> bool {
        self.explorer.nodes.contains_key(node_id)
    }

    pub fn synopsis_is_editable(&self, node_id: &str) -> bool {
        self.explorer.nodes.contains_key(node_id)
    }

    pub const fn metadata_is_read_only(&self) -> bool {
        true
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MetadataApplicability {
    Groups,
    Documents,
    GroupsAndDocuments,
    None,
}

impl MetadataApplicability {
    const fn applies_to(self, kind: HierarchyNodeKind) -> bool {
        matches!(
            (self, kind),
            (
                Self::Groups,
                HierarchyNodeKind::Group | HierarchyNodeKind::Root
            ) | (Self::Documents, HierarchyNodeKind::Document)
                | (
                    Self::GroupsAndDocuments,
                    HierarchyNodeKind::Root
                        | HierarchyNodeKind::Group
                        | HierarchyNodeKind::Document
                )
        )
    }
}

#[derive(Debug, Clone)]
struct MetadataDefinition {
    label: String,
    description: Option<String>,
    applicability: MetadataApplicability,
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

impl InspectorState<'_> {
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
}

/// Stable settings definition exposed for headless verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataFieldSummary<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub description: Option<&'a str>,
    pub default_value: Option<&'a str>,
    pub visible_on_cards: bool,
}

/// Presentation state for project Settings.
#[derive(Debug, Clone)]
pub struct SettingsState {
    appearance: AppearanceMode,
    metadata_definitions: BTreeMap<String, MetadataDefinition>,
    metadata_order: Vec<String>,
}

impl SettingsState {
    fn fixture() -> Self {
        Self {
            appearance: AppearanceMode::System,
            metadata_definitions: BTreeMap::from([
                (
                    "field-17".to_owned(),
                    MetadataDefinition {
                        label: "Point of view".to_owned(),
                        description: Some("Narrative perspective".to_owned()),
                        applicability: MetadataApplicability::GroupsAndDocuments,
                        default_value: None,
                        visible_on_cards: true,
                    },
                ),
                (
                    "field-18".to_owned(),
                    MetadataDefinition {
                        label: "Location".to_owned(),
                        description: None,
                        applicability: MetadataApplicability::Documents,
                        default_value: Some("Unknown".to_owned()),
                        visible_on_cards: true,
                    },
                ),
            ]),
            metadata_order: vec!["field-17".to_owned(), "field-18".to_owned()],
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
                    })
            })
            .collect()
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
    case_sensitive: bool,
    whole_word: bool,
    results: Vec<GlobalSearchResult>,
    query_generation: u64,
    complete: bool,
    error: Option<String>,
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

    pub const fn case_sensitive(&self) -> bool {
        self.case_sensitive
    }

    pub const fn whole_word(&self) -> bool {
        self.whole_word
    }

    pub fn results(&self) -> &[GlobalSearchResult] {
        &self.results
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
}

/// Hierarchy-shaped global replacement preview.
#[derive(Debug, Clone)]
pub struct ReplacementPreviewState {
    open: bool,
    nodes: BTreeMap<String, ReplacementNode>,
    captured_project_revision: u64,
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
                    },
                ),
                (
                    "chapter-two".to_owned(),
                    ReplacementNode {
                        children: vec!["chapter-two-match-1".to_owned()],
                        included: false,
                    },
                ),
                (
                    "chapter-one-match-1".to_owned(),
                    ReplacementNode {
                        children: Vec::new(),
                        included: true,
                    },
                ),
                (
                    "chapter-one-match-2".to_owned(),
                    ReplacementNode {
                        children: Vec::new(),
                        included: true,
                    },
                ),
                (
                    "chapter-two-match-1".to_owned(),
                    ReplacementNode {
                        children: Vec::new(),
                        included: false,
                    },
                ),
            ]),
            captured_project_revision: 1,
        }
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
            }
            return;
        }
        for child in children {
            self.set_included(&child, included);
        }
    }
}

/// Whole-project restore is the only supported History scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HistoryRestoreScope {
    EntireProject,
}

/// A project modal with complete, explicit destructive scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectModal {
    HistoryRestore {
        checkpoint_id: String,
        scope: HistoryRestoreScope,
    },
    DeleteMetadataField {
        field_id: String,
    },
}

/// History list/detail presentation facts.
#[derive(Debug, Clone, Default)]
pub struct HistoryState {
    checkpoints: Vec<String>,
    active_document_filter: Option<String>,
    error: Option<String>,
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

    pub fn checkpoints(&self) -> &[String] {
        &self.checkpoints
    }

    pub fn active_document_filter(&self) -> Option<&str> {
        self.active_document_filter.as_deref()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
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
    location: RestoreLocation,
    fallback: RestoreLocation,
}

/// Recently Deleted list/detail presentation state.
#[derive(Debug, Clone)]
pub struct RecentlyDeletedState {
    items: BTreeMap<String, DeletedItem>,
}

impl RecentlyDeletedState {
    fn fixture() -> Self {
        Self {
            items: BTreeMap::from([(
                "deleted-part".to_owned(),
                DeletedItem {
                    location: RestoreLocation::FormerParent("part-one".to_owned()),
                    fallback: RestoreLocation::SectionRoot("manuscript".to_owned()),
                },
            )]),
        }
    }

    pub const fn has_formatted_preview(&self) -> bool {
        true
    }

    pub fn restore_location(&self, node_id: &str) -> RestoreLocation {
        self.items
            .get(node_id)
            .map(|item| item.location.clone())
            .unwrap_or_else(|| RestoreLocation::SectionRoot("manuscript".to_owned()))
    }

    pub const fn has_purge_action(&self) -> bool {
        false
    }

    fn use_fallback(&mut self, node_id: &str) {
        if let Some(item) = self.items.get_mut(node_id) {
            item.location = item.fallback.clone();
        }
    }
}

/// Export progress and terminal presentation states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportState {
    Ready,
    Exporting { completed: u64, total: u64 },
    Succeeded { output_name: String },
    Failed(String),
}

/// Fixed whole-manuscript export controls and feedback.
#[derive(Debug, Clone)]
pub struct ExportViewState {
    state: ExportState,
    output_name: String,
    numbering_documents: bool,
}

impl Default for ExportViewState {
    fn default() -> Self {
        Self {
            state: ExportState::Ready,
            output_name: "manuscript.html".to_owned(),
            numbering_documents: false,
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

    pub const fn numbers_documents(&self) -> bool {
        self.numbering_documents
    }

    pub const fn can_open_result(&self) -> bool {
        matches!(self.state, ExportState::Succeeded { .. })
    }

    pub const fn can_reveal_result(&self) -> bool {
        self.can_open_result()
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
}

impl RecoveryState {
    pub const fn is_disposable_after_durable_save(&self) -> bool {
        self.accepted && self.durable_save_completed
    }

    pub const fn durable_save_completed(&self) -> bool {
        self.durable_save_completed
    }
}

/// Project operations that can complete asynchronously.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProjectTask {
    GlobalSearch { generation: u64 },
    ReplacementPreview,
    ApplyReplacement,
    LoadHistory,
    PreviewHistory { checkpoint_id: String },
    RestoreHistory { checkpoint_id: String },
    RestoreDeleted { node_id: String },
    Save { through_revision: u64 },
    Export { source_revision: u64 },
    AcceptRecovery,
    PersistWorkspace,
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
        checkpoints: Vec<String>,
    },
    HistoryPreviewReady,
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
        output_name: String,
    },
    RecoveryAccepted {
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
    OpenHierarchyNode(String),
    OpenHierarchyNodeInCompanion(String),
    RenameNode {
        node_id: String,
        title: String,
    },
    SetSynopsis {
        node_id: String,
        synopsis: String,
    },
    SetMetadataValue {
        node_id: String,
        field_id: String,
        value: String,
    },
    SetMetadataApplicability {
        field_id: String,
        applies_to_documents: bool,
    },
    RenameMetadataField {
        field_id: String,
        label: String,
    },
    ReorderMetadataField {
        field_id: String,
        target_index: usize,
    },
    RequestDeleteMetadataField(String),
    ConfirmDeleteMetadataField,
    ActivateCard(String),
    SetCardsSection(String),
    SetDragDestination(Option<DragDestination>),
    DropHierarchy {
        source_id: String,
        destination: DragDestination,
    },
    CopySelection,
    CutSelection,
    CancelCut,
    PasteSelection {
        destination: DragDestination,
    },
    SetGlobalSearchQuery(String),
    SetGlobalSearchOptions {
        case_sensitive: bool,
        whole_word: bool,
    },
    NavigateGlobalSearchResult(String),
    OpenReplacementPreview,
    SetReplacementIncluded {
        node_id: String,
        included: bool,
    },
    ApplyReplacement,
    SetHistoryDocumentFilter(Option<String>),
    RequestNamedSnapshot(String),
    RequestHistoryRestore {
        checkpoint_id: String,
    },
    ConfirmHistoryRestore,
    DismissModal,
    RestoreDeleted(String),
    UseRestoreFallback(String),
    SetAppearance(AppearanceMode),
    SetExportOutputName(String),
    SetExportNumbering(bool),
    StartExport,
    OpenExportResult,
    RevealExportResult,
    ExportProgress {
        completed: u64,
        total: u64,
    },
    ExportSucceeded(String),
    ExportFailed(String),
    MarkDirty(u64),
    StartSave(u64),
    SaveCompleted(u64),
    SaveFailed(String),
    RequestClose,
    RetryCloseSave,
    CancelClose,
    SetContentState(ContentState),
    AcceptRecovery,
    RecoveryDurablySaved,
}

/// Integration effects translated into application/service calls.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    CopyDocuments(Vec<String>),
    PasteCopiedDocuments {
        destination: DragDestination,
    },
    PasteCutDocuments {
        node_ids: Vec<String>,
        destination: DragDestination,
    },
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
    SetMetadataApplicability {
        field_id: String,
        applies_to_documents: bool,
    },
    RenameMetadataField {
        field_id: String,
        label: String,
    },
    ReorderMetadataField {
        field_id: String,
        target_index: usize,
    },
    DeleteMetadataField(String),
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
    },
    ApplyGlobalReplacement {
        captured_project_revision: u64,
        included_match_ids: Vec<String>,
    },
    CreateNamedSnapshot(String),
    RestoreHistory {
        checkpoint_id: String,
        scope: HistoryRestoreScope,
    },
    RestoreDeletedSubtree {
        node_id: String,
        location: RestoreLocation,
    },
    ApplyAppearanceToAllWindows(AppearanceMode),
    ExportEntireManuscript {
        output_name: String,
        number_documents: bool,
        source_revision: u64,
    },
    OpenExportResult(String),
    RevealExportResult(String),
    SaveThroughRevision(u64),
    FocusRecoveredEditor,
}

/// Project-facing presentation model integrated with the mounted editor model.
#[derive(Debug, Clone)]
pub struct ProjectWorkspace {
    fixture: ProjectFixture,
    session: u64,
    project_revision: u64,
    sidebar: SidebarSurface,
    explorer: ExplorerState,
    cards_section: String,
    cards_drag_destination: Option<DragDestination>,
    last_activated_document: Option<String>,
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
        }
        let history = HistoryState {
            checkpoints: vec!["snapshot-draft-two".to_owned(), "autosave-17".to_owned()],
            ..HistoryState::default()
        };
        Self {
            fixture,
            session: 37,
            project_revision: 1,
            sidebar,
            explorer: ExplorerState::fixture(),
            cards_section: "manuscript".to_owned(),
            cards_drag_destination: Some(DragDestination::BeforeSibling(
                "chapter-three".to_owned(),
            )),
            last_activated_document: None,
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
            },
            modal: None,
            editor: EditorWorkspace::from_fixture(EditorFixture::DualPane),
            pending: BTreeMap::new(),
            next_request: 0,
        }
    }

    pub fn fixture_reference(&self, appearance: ResolvedAppearance) -> &'static str {
        match (self.fixture, appearance) {
            (ProjectFixture::Explorer, ResolvedAppearance::Light) => "editor-single-light",
            (ProjectFixture::Explorer, ResolvedAppearance::Dark) => "editor-single-dark",
            (ProjectFixture::Cards, ResolvedAppearance::Light) => "cards-light",
            (ProjectFixture::Cards, ResolvedAppearance::Dark) => "cards-dark",
            (ProjectFixture::GlobalSearch, ResolvedAppearance::Light) => "global-search-light",
            (ProjectFixture::GlobalSearch, ResolvedAppearance::Dark) => "global-search-dark",
            (ProjectFixture::History, ResolvedAppearance::Light) => "history-light",
            (ProjectFixture::History, ResolvedAppearance::Dark) => "history-dark",
            (ProjectFixture::RecentlyDeleted, ResolvedAppearance::Light) => {
                "recently-deleted-light"
            }
            (ProjectFixture::RecentlyDeleted, ResolvedAppearance::Dark) => "recently-deleted-dark",
            (ProjectFixture::SettingsAppearance, ResolvedAppearance::Light) => {
                "settings-appearance-light"
            }
            (ProjectFixture::SettingsAppearance, ResolvedAppearance::Dark) => {
                "settings-appearance-dark"
            }
            (ProjectFixture::Export, ResolvedAppearance::Light) => {
                "export-project-output-controls-light"
            }
            (ProjectFixture::Export, ResolvedAppearance::Dark) => {
                "export-project-output-controls-dark"
            }
            (ProjectFixture::ErrorRecovery, ResolvedAppearance::Light) => "error-recovery-light",
            (ProjectFixture::ErrorRecovery, ResolvedAppearance::Dark) => "error-recovery-dark",
        }
    }

    pub const fn sidebar_surface(&self) -> SidebarSurface {
        self.sidebar
    }

    pub fn explorer(&self) -> &ExplorerState {
        &self.explorer
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
        }
    }

    pub fn inspector(&self) -> InspectorState<'_> {
        InspectorState {
            explorer: &self.explorer,
            definitions: &self.settings.metadata_definitions,
            field_order: &self.settings.metadata_order,
            values: &self.metadata_values,
        }
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

    pub fn recently_deleted(&self) -> &RecentlyDeletedState {
        &self.recently_deleted
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

    pub const fn shell_context_is_retained(&self) -> bool {
        true
    }

    pub fn begin_session(&mut self, session: u64, project_revision: u64) {
        self.session = session;
        self.project_revision = project_revision;
        self.pending.clear();
        self.next_request = 0;
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
            return false;
        }
        let keep_streaming = matches!(
            completion.payload(),
            ProjectTaskPayload::SearchBatch {
                finished: false,
                ..
            } | ProjectTaskPayload::ExportProgress { .. }
        );
        let task = ticket.task.clone();
        let accepted = self.apply_completion(completion);
        if accepted && !keep_streaming {
            self.pending.remove(&task);
        }
        accepted
    }

    pub fn update(&mut self, message: ProjectMessage) -> Vec<ProjectEffect> {
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
                can_contain_children
                    .then_some(ProjectEffect::CreateHierarchy { parent_id, kind })
                    .into_iter()
                    .collect()
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
            ProjectMessage::OpenHierarchyNode(node_id) => self.open_hierarchy_node(node_id, None),
            ProjectMessage::OpenHierarchyNodeInCompanion(node_id) => {
                self.open_hierarchy_node(node_id, Some(EditorPane::Companion))
            }
            ProjectMessage::RenameNode { node_id, title } => {
                self.explorer.rename(&node_id, title.clone());
                vec![ProjectEffect::CommitNodeTitle { node_id, title }]
            }
            ProjectMessage::SetSynopsis { node_id, synopsis } => {
                self.explorer.set_synopsis(&node_id, synopsis.clone());
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
                field.applicability = match (field.applicability, applies_to_documents) {
                    (MetadataApplicability::GroupsAndDocuments, false) => {
                        MetadataApplicability::Groups
                    }
                    (MetadataApplicability::Groups, true)
                    | (MetadataApplicability::Documents, true)
                    | (MetadataApplicability::None, true) => {
                        MetadataApplicability::GroupsAndDocuments
                    }
                    (MetadataApplicability::Documents, false) => MetadataApplicability::None,
                    (current, _) => current,
                };
                vec![ProjectEffect::SetMetadataApplicability {
                    field_id,
                    applies_to_documents,
                }]
            }
            ProjectMessage::RenameMetadataField { field_id, label } => {
                let Some(field) = self.settings.metadata_definitions.get_mut(&field_id) else {
                    return Vec::new();
                };
                if label.trim().is_empty() {
                    return Vec::new();
                }
                field.label = label.clone();
                vec![ProjectEffect::RenameMetadataField { field_id, label }]
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
                self.settings.metadata_order.insert(target, field);
                vec![ProjectEffect::ReorderMetadataField {
                    field_id,
                    target_index,
                }]
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
            ProjectMessage::ActivateCard(document_id) => self.activate_card(document_id),
            ProjectMessage::SetCardsSection(section) => {
                if matches!(section.as_str(), "manuscript" | "research") {
                    self.cards_section = section;
                }
                Vec::new()
            }
            ProjectMessage::SetDragDestination(destination) => {
                self.cards_drag_destination = destination;
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
                        .is_none_or(|node| node.kind != HierarchyNodeKind::Document)
                }) {
                    return Vec::new();
                }
                let documents = selected.into_iter().map(str::to_owned).collect::<Vec<_>>();
                (!documents.is_empty())
                    .then_some(ProjectEffect::CopyDocuments(documents))
                    .into_iter()
                    .collect()
            }
            ProjectMessage::CutSelection => {
                self.explorer.mark_cut();
                Vec::new()
            }
            ProjectMessage::CancelCut => {
                self.explorer.cancel_cut();
                Vec::new()
            }
            ProjectMessage::PasteSelection { destination } => {
                if self.explorer.cut_pending.is_empty() {
                    vec![ProjectEffect::PasteCopiedDocuments { destination }]
                } else {
                    let node_ids = self
                        .explorer
                        .preorder_ids()
                        .into_iter()
                        .filter(|id| self.explorer.cut_pending.contains(*id))
                        .map(str::to_owned)
                        .collect();
                    self.explorer.complete_cut();
                    vec![ProjectEffect::PasteCutDocuments {
                        node_ids,
                        destination,
                    }]
                }
            }
            ProjectMessage::SetGlobalSearchQuery(query) => {
                self.global_search.query = query;
                self.global_search.query_generation =
                    self.global_search.query_generation.saturating_add(1);
                self.global_search.results.clear();
                self.global_search.complete = false;
                self.global_search.error = None;
                vec![self.search_effect()]
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
                self.global_search.complete = false;
                self.global_search.error = None;
                vec![self.search_effect()]
            }
            ProjectMessage::NavigateGlobalSearchResult(match_id) => {
                vec![ProjectEffect::NavigateSearchResult {
                    match_id,
                    revalidate_revision: true,
                }]
            }
            ProjectMessage::OpenReplacementPreview => {
                self.replacement_preview.open = true;
                self.replacement_preview.captured_project_revision = self.project_revision;
                vec![ProjectEffect::BuildReplacementPreview {
                    query_generation: self.global_search.query_generation,
                    captured_project_revision: self.project_revision,
                }]
            }
            ProjectMessage::SetReplacementIncluded { node_id, included } => {
                self.replacement_preview.set_included(&node_id, included);
                Vec::new()
            }
            ProjectMessage::ApplyReplacement => {
                vec![ProjectEffect::ApplyGlobalReplacement {
                    captured_project_revision: self.replacement_preview.captured_project_revision,
                    included_match_ids: self
                        .replacement_preview
                        .included_match_ids()
                        .into_iter()
                        .map(str::to_owned)
                        .collect(),
                }]
            }
            ProjectMessage::SetHistoryDocumentFilter(document_id) => {
                self.history.active_document_filter = document_id;
                Vec::new()
            }
            ProjectMessage::RequestNamedSnapshot(name) => {
                let name = name.trim().to_owned();
                (!name.is_empty())
                    .then_some(ProjectEffect::CreateNamedSnapshot(name))
                    .into_iter()
                    .collect()
            }
            ProjectMessage::RequestHistoryRestore { checkpoint_id } => {
                self.modal = Some(ProjectModal::HistoryRestore {
                    checkpoint_id,
                    scope: HistoryRestoreScope::EntireProject,
                });
                Vec::new()
            }
            ProjectMessage::ConfirmHistoryRestore => {
                let Some(ProjectModal::HistoryRestore {
                    checkpoint_id,
                    scope,
                }) = self.modal.take()
                else {
                    return Vec::new();
                };
                vec![ProjectEffect::RestoreHistory {
                    checkpoint_id,
                    scope,
                }]
            }
            ProjectMessage::DismissModal => {
                self.modal = None;
                Vec::new()
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
            ProjectMessage::SetExportOutputName(output_name) => {
                if !output_name.trim().is_empty() {
                    self.export.output_name = output_name;
                }
                Vec::new()
            }
            ProjectMessage::SetExportNumbering(number_documents) => {
                self.export.numbering_documents = number_documents;
                Vec::new()
            }
            ProjectMessage::StartExport => {
                self.export.state = ExportState::Exporting {
                    completed: 0,
                    total: 0,
                };
                vec![ProjectEffect::ExportEntireManuscript {
                    output_name: self.export.output_name.clone(),
                    number_documents: self.export.numbering_documents,
                    source_revision: self.project_revision,
                }]
            }
            ProjectMessage::OpenExportResult => match &self.export.state {
                ExportState::Succeeded { output_name } => {
                    vec![ProjectEffect::OpenExportResult(output_name.clone())]
                }
                ExportState::Ready | ExportState::Exporting { .. } | ExportState::Failed(_) => {
                    Vec::new()
                }
            },
            ProjectMessage::RevealExportResult => match &self.export.state {
                ExportState::Succeeded { output_name } => {
                    vec![ProjectEffect::RevealExportResult(output_name.clone())]
                }
                ExportState::Ready | ExportState::Exporting { .. } | ExportState::Failed(_) => {
                    Vec::new()
                }
            },
            ProjectMessage::ExportProgress { completed, total } => {
                if matches!(self.export.state, ExportState::Exporting { .. }) {
                    self.export.state = ExportState::Exporting {
                        completed: completed.min(total),
                        total,
                    };
                }
                Vec::new()
            }
            ProjectMessage::ExportSucceeded(output_name) => {
                self.export.state = ExportState::Succeeded { output_name };
                Vec::new()
            }
            ProjectMessage::ExportFailed(error) => {
                self.export.state = ExportState::Failed(error);
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
                self.save.state = SaveState::Error(error);
                self.save.recovery_intact = true;
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
                self.content_state = state;
                Vec::new()
            }
            ProjectMessage::AcceptRecovery => {
                self.recovery.accepted = true;
                self.content_state = ContentState::Ready;
                vec![ProjectEffect::FocusRecoveredEditor]
            }
            ProjectMessage::RecoveryDurablySaved => {
                self.recovery.durable_save_completed = true;
                self.save.recovery_intact = false;
                Vec::new()
            }
        }
    }

    pub(crate) const fn fixture(&self) -> ProjectFixture {
        self.fixture
    }

    fn activate_card(&mut self, document_id: String) -> Vec<ProjectEffect> {
        let Some(node) = self.explorer.nodes.get(&document_id) else {
            return Vec::new();
        };
        if node.kind != HierarchyNodeKind::Document {
            self.explorer.toggle_expanded(&document_id);
            return Vec::new();
        }
        let pane = if node.section_id == "research" {
            EditorPane::Companion
        } else {
            EditorPane::Primary
        };
        self.last_activated_document = Some(document_id.clone());
        let _ = self.editor.update(EditorMessage::OpenTab {
            pane,
            tab: TabSpec::new(document_id.clone(), node.title.clone()),
        });
        match pane {
            EditorPane::Primary => vec![ProjectEffect::OpenDocumentInPrimary(document_id)],
            EditorPane::Companion => vec![ProjectEffect::OpenDocumentInCompanion(document_id)],
        }
    }

    fn open_hierarchy_node(
        &mut self,
        document_id: String,
        requested_pane: Option<EditorPane>,
    ) -> Vec<ProjectEffect> {
        let Some(node) = self.explorer.nodes.get(&document_id) else {
            return Vec::new();
        };
        if node.kind != HierarchyNodeKind::Document {
            return Vec::new();
        }
        let pane = requested_pane.unwrap_or_else(|| {
            if node.section_id == "research" {
                EditorPane::Companion
            } else {
                EditorPane::Primary
            }
        });
        let title = node.title.clone();
        let _ = self.editor.update(EditorMessage::OpenTab {
            pane,
            tab: TabSpec::new(document_id.clone(), title),
        });
        match pane {
            EditorPane::Primary => vec![ProjectEffect::OpenDocumentInPrimary(document_id)],
            EditorPane::Companion => vec![ProjectEffect::OpenDocumentInCompanion(document_id)],
        }
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
            let title = self
                .explorer
                .title(&source_id)
                .unwrap_or(&source_id)
                .to_owned();
            let _ = self.editor.update(EditorMessage::OpenTab {
                pane,
                tab: TabSpec::new(source_id.clone(), title),
            });
            return match pane {
                EditorPane::Primary => vec![ProjectEffect::OpenDocumentInPrimary(source_id)],
                EditorPane::Companion => vec![ProjectEffect::OpenDocumentInCompanion(source_id)],
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
            | ProjectTask::AcceptRecovery => {
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
                self.replacement_preview.captured_project_revision =
                    completion.ticket.captured_project_revision;
                true
            }
            ProjectTaskPayload::ReplacementApplied { revision }
            | ProjectTaskPayload::HistoryRestored { revision }
            | ProjectTaskPayload::DeletedRestored { revision }
            | ProjectTaskPayload::RecoveryAccepted { revision } => {
                self.project_revision = revision;
                self.save.state = SaveState::Dirty {
                    current_revision: revision,
                };
                true
            }
            ProjectTaskPayload::HistoryLoaded { checkpoints } => {
                self.history.checkpoints = checkpoints;
                self.history.error = None;
                true
            }
            ProjectTaskPayload::HistoryPreviewReady => true,
            ProjectTaskPayload::SavedThrough(revision) => {
                self.finish_save(revision);
                true
            }
            ProjectTaskPayload::ExportProgress { completed, total } => {
                self.export.state = ExportState::Exporting {
                    completed: completed.min(total),
                    total,
                };
                true
            }
            ProjectTaskPayload::ExportSucceeded { output_name } => {
                self.export.state = ExportState::Succeeded { output_name };
                true
            }
            ProjectTaskPayload::WorkspacePersisted => true,
            ProjectTaskPayload::Failed(error) => {
                match completion.ticket.task {
                    ProjectTask::GlobalSearch { .. } => self.global_search.error = Some(error),
                    ProjectTask::LoadHistory
                    | ProjectTask::PreviewHistory { .. }
                    | ProjectTask::RestoreHistory { .. } => self.history.error = Some(error),
                    ProjectTask::Export { .. } => self.export.state = ExportState::Failed(error),
                    ProjectTask::Save { .. } | ProjectTask::AcceptRecovery => {
                        self.save.state = SaveState::Error(error);
                        self.save.recovery_intact = true;
                    }
                    ProjectTask::ReplacementPreview
                    | ProjectTask::ApplyReplacement
                    | ProjectTask::RestoreDeleted { .. }
                    | ProjectTask::PersistWorkspace => {
                        self.content_state = ContentState::Error(error)
                    }
                }
                true
            }
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
            ProjectTaskPayload::HistoryPreviewReady | ProjectTaskPayload::Failed(_)
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
            ProjectTaskPayload::ExportProgress { .. }
                | ProjectTaskPayload::ExportSucceeded { .. }
                | ProjectTaskPayload::Failed(_)
        ) | (
            ProjectTask::AcceptRecovery,
            ProjectTaskPayload::RecoveryAccepted { .. } | ProjectTaskPayload::Failed(_)
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
            | (ProjectTask::PersistWorkspace, ProjectTask::PersistWorkspace)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
                ProjectTaskPayload::HistoryPreviewReady,
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
    fn group_copy_and_keyboard_cut_remain_deferred() {
        let mut workspace = ProjectWorkspace::from_fixture(ProjectFixture::Explorer);
        workspace.update(ProjectMessage::SelectHierarchy {
            node_id: "part-one".to_owned(),
            gesture: SelectionGesture::Replace,
        });
        assert!(workspace.update(ProjectMessage::CopySelection).is_empty());
        assert!(workspace.update(ProjectMessage::CutSelection).is_empty());
        assert!(!workspace.explorer().is_cut_pending("part-one"));
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
}
