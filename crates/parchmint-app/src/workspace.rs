#![allow(missing_docs)] // Public bridge vocabulary is documented by the Stage 04 handoff.
//! Rust-owned binder, outline, cards, and project-shell use cases.
//!
//! This module deliberately exposes immutable rows.  Qt/QML may retain visual
//! state such as an expanded item, but it never owns another mutable copy of
//! the project graph.

use crate::{IndexStatus, ProjectSearch, SearchServiceError};
use parchmint_domain::{
    DocumentId, DocumentMetadata, DocumentRecord, Node, NodeId, NodeKind, Project, ProjectCommand,
    ProjectError, ProjectEvent, RelativeProjectPath,
};
use parchmint_index::{CountTotals, SearchQuery, SearchResult};
use parchmint_storage::{
    AttachmentPreview, AttachmentRecord, OpenMode, OpenProject, ProjectStorage, StorageError,
    atomic_write,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Presentation-only outline sort.  It never changes canonical binder order.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum OutlineSort {
    /// Canonical sibling ordering.
    #[default]
    Binder,
    /// Case-insensitive title order for the current projection.
    Title,
    /// Status then title for the current projection.
    Status,
}

/// A drop target calculated by the UI before it reaches the domain layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DropPlacement {
    /// Insert immediately before the target.
    Before(NodeId),
    /// Insert immediately after the target.
    After(NodeId),
    /// Append as a child of the target.
    Inside(NodeId),
}

/// Compact, immutable row shared by binder, outline, and cards.
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(clippy::struct_excessive_bools)] // Flat Qt row roles intentionally avoid nested allocations.
pub struct BinderRow {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub depth: u16,
    pub is_group: bool,
    pub is_root: bool,
    pub has_children: bool,
    pub title: String,
    pub synopsis: String,
    pub status: String,
    pub label: String,
    pub word_count: usize,
    pub include_in_compile: bool,
}

/// Immutable snapshot used by virtualized QML views.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BinderSnapshot {
    rows: Vec<BinderRow>,
}

impl BinderSnapshot {
    pub fn rows(&self) -> &[BinderRow] {
        &self.rows
    }

    pub fn visible_rows(&self, start: usize, count: usize) -> &[BinderRow] {
        let start = start.min(self.rows.len());
        let end = start.saturating_add(count).min(self.rows.len());
        &self.rows[start..end]
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// The independently versioned, disposable workspace format.
pub const WORKSPACE_FORMAT_VERSION: u32 = 1;

/// One of the two symmetric panes. The string representation is intentional:
/// unknown future views can fall back without invalidating a project.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaneView {
    #[default]
    Editor,
    Attachment,
    Outline,
    Cards,
}

/// Split placement recorded independently from content data.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitOrientation {
    #[default]
    Horizontal,
    Vertical,
}

/// Local restoration hints for a single editor/reference pane.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PaneWorkspaceState {
    #[serde(default)]
    pub node: Option<NodeId>,
    #[serde(default)]
    pub view: PaneView,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub cursor: u32,
    #[serde(default)]
    pub scroll: u32,
}

impl Default for PaneWorkspaceState {
    fn default() -> Self {
        Self {
            node: None,
            view: PaneView::Editor,
            pinned: false,
            cursor: 0,
            scroll: 0,
        }
    }
}

/// Local, disposable UI state. Removing this file cannot affect a project.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // Persisted flat QML workspace preferences.
pub struct WorkspacePreferences {
    #[serde(default = "workspace_format_version")]
    pub version: u32,
    #[serde(default)]
    pub selected_nodes: Vec<NodeId>,
    #[serde(default)]
    pub expanded_nodes: Vec<NodeId>,
    #[serde(default)]
    pub active_view: String,
    #[serde(default)]
    pub binder_visible: bool,
    #[serde(default)]
    pub inspector_visible: bool,
    #[serde(default)]
    pub panes: [PaneWorkspaceState; 2],
    #[serde(default)]
    pub focused_pane: u8,
    #[serde(default)]
    pub split_enabled: bool,
    #[serde(default)]
    pub split_orientation: SplitOrientation,
    #[serde(default = "default_split_ratio")]
    pub split_ratio_milli: u16,
    #[serde(default)]
    pub window_x: Option<i32>,
    #[serde(default)]
    pub window_y: Option<i32>,
    #[serde(default)]
    pub window_width: Option<u32>,
    #[serde(default)]
    pub window_height: Option<u32>,
    #[serde(default)]
    pub window_maximized: bool,
}

impl Default for WorkspacePreferences {
    fn default() -> Self {
        Self {
            version: WORKSPACE_FORMAT_VERSION,
            selected_nodes: Vec::new(),
            expanded_nodes: Vec::new(),
            active_view: String::new(),
            binder_visible: true,
            inspector_visible: true,
            panes: [PaneWorkspaceState::default(), PaneWorkspaceState::default()],
            focused_pane: 0,
            split_enabled: false,
            split_orientation: SplitOrientation::Horizontal,
            split_ratio_milli: default_split_ratio(),
            window_x: None,
            window_y: None,
            window_width: None,
            window_height: None,
            window_maximized: false,
        }
    }
}

fn workspace_format_version() -> u32 {
    WORKSPACE_FORMAT_VERSION
}
fn default_split_ratio() -> u16 {
    500
}

/// A recent project entry stored outside canonical project data.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecentProject {
    pub path: PathBuf,
    pub name: String,
}

/// Small bounded recent-project list. Callers choose an application settings path.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecentProjects {
    #[serde(default)]
    entries: Vec<RecentProject>,
}

impl RecentProjects {
    pub fn load(path: &Path) -> Result<Self, WorkspaceError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        toml::from_str(&fs::read_to_string(path).map_err(WorkspaceError::ReadPreferences)?)
            .map_err(WorkspaceError::ReadPreferencesFormat)
    }

    pub fn entries(&self) -> &[RecentProject] {
        &self.entries
    }

    pub fn remember(&mut self, path: PathBuf, name: String) {
        self.entries.retain(|entry| entry.path != path);
        self.entries.insert(0, RecentProject { path, name });
        self.entries.truncate(12);
    }

    pub fn save(&self, path: &Path) -> Result<(), WorkspaceError> {
        let source = toml::to_string_pretty(self).map_err(WorkspaceError::Preferences)?;
        atomic_write(path, source.as_bytes()).map_err(WorkspaceError::WritePreferences)
    }
}

/// One project incarnation and all Stage 04 structural/metadata use cases.
pub struct ProjectWorkspace {
    opened: OpenProject,
    snapshot: BinderSnapshot,
    selection: Vec<NodeId>,
    undo: Vec<ProjectCommand>,
    redo: Vec<ProjectCommand>,
    preferences: WorkspacePreferences,
    workspace_diagnostic: Option<String>,
    search: ProjectSearch,
    index_diagnostic: Option<String>,
}

impl ProjectWorkspace {
    pub fn create(root: impl AsRef<Path>, name: impl Into<String>) -> Result<Self, WorkspaceError> {
        let opened = ProjectStorage::create(root, name).map_err(WorkspaceError::Storage)?;
        Ok(Self::from_opened(opened))
    }

    pub fn open(root: impl AsRef<Path>) -> Result<Self, WorkspaceError> {
        let opened =
            ProjectStorage::open(root, OpenMode::ReadWrite).map_err(WorkspaceError::Storage)?;
        Ok(Self::from_opened(opened))
    }

    fn from_opened(opened: OpenProject) -> Self {
        let (mut preferences, workspace_diagnostic) = load_preferences(opened.root());
        reconcile_preferences(&opened.project, &mut preferences);
        let valid = preferences
            .selected_nodes
            .iter()
            .copied()
            .filter(|id| opened.project.nodes.contains_key(id) && !opened.project.is_trashed(*id))
            .collect();
        let snapshot = build_snapshot(&opened.project, None, "", OutlineSort::Binder);
        let search = ProjectSearch::open(opened.root());
        Self {
            opened,
            snapshot,
            selection: valid,
            undo: Vec::new(),
            redo: Vec::new(),
            preferences,
            workspace_diagnostic,
            search,
            index_diagnostic: None,
        }
    }

    pub fn project(&self) -> &Project {
        &self.opened.project
    }

    pub fn snapshot(&self) -> &BinderSnapshot {
        &self.snapshot
    }

    pub fn selected(&self) -> &[NodeId] {
        &self.selection
    }

    pub fn preferences(&self) -> &WorkspacePreferences {
        &self.preferences
    }

    /// Non-fatal restore diagnostic. A malformed workspace never blocks open.
    pub fn workspace_diagnostic(&self) -> Option<&str> {
        self.workspace_diagnostic.as_deref()
    }

    /// Non-fatal search/index diagnostic. Canonical editing is always available.
    pub fn index_diagnostic(&self) -> Option<&str> {
        self.index_diagnostic.as_deref()
    }

    /// Current disposable-cache availability for a passive progress/status UI.
    pub fn index_status(&self) -> &IndexStatus {
        self.search.status()
    }

    /// Explicitly (re)builds the derived cache from current canonical project state.
    pub fn rebuild_search_index(&mut self) -> Result<(), WorkspaceError> {
        self.search
            .rebuild(&self.opened)
            .map_err(WorkspaceError::Search)?;
        self.index_diagnostic = None;
        Ok(())
    }

    /// Searches active documents. The first query may initialize a missing cache;
    /// callers that require no synchronous work can invoke `rebuild_search_index`
    /// on their Rust worker first.
    pub fn search_project(
        &mut self,
        query: &SearchQuery<'_>,
        limit: u32,
    ) -> Result<Vec<SearchResult>, WorkspaceError> {
        self.search
            .search(&self.opened, query, limit)
            .map_err(WorkspaceError::Search)
    }

    /// Returns stored aggregate counts for manuscript/research/project/subtree.
    pub fn search_totals(
        &mut self,
        scope: Option<&str>,
        subtree: Option<NodeId>,
    ) -> Result<CountTotals, WorkspaceError> {
        self.search
            .totals(&self.opened, scope, subtree)
            .map_err(WorkspaceError::Search)
    }

    pub fn pane(&self, index: usize) -> Option<&PaneWorkspaceState> {
        self.preferences.panes.get(index)
    }

    /// Opens a node in either symmetric pane. Replacing one pane deliberately
    /// does not touch the other pane's node/cursor/scroll state, which is what
    /// lets its Qt document retain its independent undo history.
    pub fn open_in_pane(
        &mut self,
        index: usize,
        node: Option<NodeId>,
        view: PaneView,
    ) -> Result<(), WorkspaceError> {
        let pane = self
            .preferences
            .panes
            .get_mut(index)
            .ok_or(WorkspaceError::Invariant("pane index must be zero or one"))?;
        if let Some(node) = node
            && (!self.opened.project.nodes.contains_key(&node)
                || self.opened.project.is_trashed(node))
        {
            return Err(WorkspaceError::Invariant("pane node is unavailable"));
        }
        pane.node = node;
        pane.view = view;
        pane.cursor = 0;
        pane.scroll = 0;
        self.preferences.focused_pane = u8::try_from(index).unwrap_or(0);
        self.save_preferences()
    }

    /// Binder navigation changes the focused pane only when it is not pinned.
    pub fn navigate_focused_pane(&mut self, node: NodeId) -> Result<bool, WorkspaceError> {
        let index = usize::from(self.preferences.focused_pane.min(1));
        if self.preferences.panes[index].pinned {
            return Ok(false);
        }
        let view = preferred_view_for_node(&self.opened.project, node);
        self.open_in_pane(index, Some(node), view)?;
        Ok(true)
    }

    pub fn open_node_in_pane(&mut self, index: usize, node: NodeId) -> Result<(), WorkspaceError> {
        let view = preferred_view_for_node(&self.opened.project, node);
        self.open_in_pane(index, Some(node), view)
    }

    pub fn set_pane_pin(&mut self, index: usize, pinned: bool) -> Result<(), WorkspaceError> {
        let pane = self
            .preferences
            .panes
            .get_mut(index)
            .ok_or(WorkspaceError::Invariant("pane index must be zero or one"))?;
        pane.pinned = pinned && pane.node.is_some();
        self.save_preferences()
    }

    pub fn focus_next_pane(&mut self) -> usize {
        self.preferences.focused_pane = if self.preferences.split_enabled {
            1 - self.preferences.focused_pane.min(1)
        } else {
            0
        };
        let _ = self.save_preferences();
        usize::from(self.preferences.focused_pane)
    }

    pub fn focus_pane(&mut self, index: usize) -> Result<(), WorkspaceError> {
        if index > 1 {
            return Err(WorkspaceError::Invariant("pane index must be zero or one"));
        }
        self.preferences.focused_pane = u8::try_from(index).unwrap_or(0);
        self.save_preferences()
    }

    pub fn pane_document_body(&self, index: usize) -> Result<&str, WorkspaceError> {
        let node = self
            .pane(index)
            .and_then(|pane| pane.node)
            .ok_or(WorkspaceError::Invariant("pane has no document"))?;
        self.document_body(node)
    }

    pub fn save_pane_document_body(
        &mut self,
        index: usize,
        body: String,
    ) -> Result<(), WorkspaceError> {
        let node = self
            .pane(index)
            .and_then(|pane| pane.node)
            .ok_or(WorkspaceError::Invariant("pane has no document"))?;
        self.save_document_body(node, body)
    }

    pub fn pane_attachment(&self, index: usize) -> Result<&AttachmentRecord, WorkspaceError> {
        let node = self
            .pane(index)
            .and_then(|pane| pane.node)
            .ok_or(WorkspaceError::Invariant("pane has no attachment"))?;
        let document = document_for_node(&self.opened.project, node)?;
        let attachment = self
            .opened
            .project
            .documents
            .get(&document)
            .and_then(|record| record.metadata.attachment)
            .ok_or(WorkspaceError::Invariant("pane node is not an attachment"))?;
        self.opened
            .attachments()
            .get(&attachment)
            .ok_or(WorkspaceError::Invariant(
                "attachment catalog entry is missing",
            ))
    }

    pub fn swap_panes(&mut self) -> Result<(), WorkspaceError> {
        self.preferences.panes.swap(0, 1);
        self.preferences.focused_pane = 1 - self.preferences.focused_pane.min(1);
        self.save_preferences()
    }

    pub fn close_pane(&mut self, index: usize) -> Result<(), WorkspaceError> {
        let pane = self
            .preferences
            .panes
            .get_mut(index)
            .ok_or(WorkspaceError::Invariant("pane index must be zero or one"))?;
        *pane = PaneWorkspaceState {
            view: PaneView::Outline,
            ..PaneWorkspaceState::default()
        };
        if index == 1 {
            self.preferences.split_enabled = false;
            self.preferences.focused_pane = 0;
        }
        self.save_preferences()
    }

    pub fn set_split(
        &mut self,
        enabled: bool,
        orientation: SplitOrientation,
        ratio_milli: u16,
    ) -> Result<(), WorkspaceError> {
        self.preferences.split_enabled = enabled;
        self.preferences.split_orientation = orientation;
        self.preferences.split_ratio_milli = ratio_milli.clamp(100, 900);
        self.save_preferences()
    }

    pub fn attachments(
        &self,
    ) -> &std::collections::BTreeMap<parchmint_domain::AssetId, AttachmentRecord> {
        self.opened.attachments()
    }

    /// Creates a normal Markdown note below the research root or a research
    /// group. The existing stage-3 lifecycle treats it exactly like a
    /// manuscript note; only default compile inclusion differs.
    pub fn create_research_node(
        &mut self,
        parent: NodeId,
        title: impl Into<String>,
        is_group: bool,
    ) -> Result<NodeId, WorkspaceError> {
        if !is_within_research(&self.opened.project, parent) {
            return Err(WorkspaceError::Invariant(
                "research nodes must be created below research",
            ));
        }
        self.create_node(parent, title, is_group)
    }

    /// Copies an attachment and creates a research document that references it.
    /// The metadata reference is stable; closing/trashing it never deletes the
    /// stored bytes, preventing accidental loss.
    pub fn import_attachment(
        &mut self,
        parent: NodeId,
        source: impl AsRef<Path>,
    ) -> Result<(NodeId, AttachmentRecord), WorkspaceError> {
        if !is_within_research(&self.opened.project, parent) {
            return Err(WorkspaceError::Invariant(
                "attachments belong below research",
            ));
        }
        let attachment = ProjectStorage::import_attachment(&mut self.opened, source)
            .map_err(WorkspaceError::Storage)?;
        let node = self.create_node(parent, attachment.display_name.clone(), false)?;
        let document = document_for_node(&self.opened.project, node)?;
        let mut metadata = self.opened.project.documents[&document].metadata.clone();
        metadata.attachment = Some(attachment.id);
        metadata.flags.insert("include-in-compile".into(), false);
        self.edit_metadata(node, metadata)?;
        Ok((node, attachment))
    }

    pub fn attachment_preview(
        &self,
        id: parchmint_domain::AssetId,
    ) -> Result<(PathBuf, AttachmentPreview), WorkspaceError> {
        ProjectStorage::attachment_preview(&self.opened, id).map_err(WorkspaceError::Storage)
    }

    /// Loads the canonical Markdown body for a pane document. The same
    /// Markdown parser/lifecycle contract is used for research and manuscript
    /// nodes; roots and attachment references intentionally have no body view.
    pub fn document_body(&self, node: NodeId) -> Result<&str, WorkspaceError> {
        let document = document_for_node(&self.opened.project, node)?;
        self.opened.body(document).map_err(WorkspaceError::Storage)
    }

    /// Persists a document body through the existing atomic document writer.
    /// This small bridge is intentionally node-agnostic so a research note has
    /// exactly the same editor behavior as a manuscript note.
    pub fn save_document_body(&mut self, node: NodeId, body: String) -> Result<(), WorkspaceError> {
        let document = document_for_node(&self.opened.project, node)?;
        parchmint_markdown::Document::parse_body(
            &body,
            &parchmint_markdown::ParseOptions::default(),
        )
        .map_err(|error| WorkspaceError::InvalidDocument(error.to_string()))?;
        self.opened
            .set_body(document, body)
            .map_err(WorkspaceError::Storage)?;
        ProjectStorage::save_document(&mut self.opened, document)
            .map_err(WorkspaceError::Storage)?;
        self.update_index_node(node);
        Ok(())
    }

    /// Changes selection only after removing stale, trashed, and duplicate IDs.
    pub fn select(&mut self, nodes: impl IntoIterator<Item = NodeId>) {
        let mut seen = BTreeSet::new();
        self.selection = nodes
            .into_iter()
            .filter(|id| self.opened.project.nodes.contains_key(id))
            .filter(|id| !self.opened.project.is_trashed(*id))
            .filter(|id| seen.insert(*id))
            .collect();
        self.preferences.selected_nodes = self.selection.clone();
        let _ = self.save_preferences();
    }

    /// Rebuilds a projection from authoritative Rust state. Filtering keeps all
    /// ancestors of a matching row, and sorting is never a structural mutation.
    pub fn project_snapshot(&mut self, focus: Option<NodeId>, filter: &str, sort: OutlineSort) {
        self.snapshot = build_snapshot(&self.opened.project, focus, filter, sort);
    }

    pub fn create_node(
        &mut self,
        parent: NodeId,
        title: impl Into<String>,
        is_group: bool,
    ) -> Result<NodeId, WorkspaceError> {
        let node_id = NodeId::new();
        let document_id = DocumentId::new();
        let index = self
            .opened
            .project
            .nodes
            .get(&parent)
            .ok_or(ProjectError::MissingNode(parent))?
            .children
            .len();
        let node = Node {
            id: node_id,
            kind: if is_group {
                NodeKind::Group { document_id }
            } else {
                NodeKind::Document { document_id }
            },
            parent: Some(parent),
            children: Vec::new(),
        };
        let document = DocumentRecord {
            id: document_id,
            node_id,
            path: RelativeProjectPath::new(format!("manuscript/{node_id}.md"))?,
            metadata: DocumentMetadata {
                title: title.into(),
                flags: std::collections::BTreeMap::from([(
                    "include-in-compile".into(),
                    !is_within_research(&self.opened.project, parent),
                )]),
                ..DocumentMetadata::default()
            },
        };
        self.apply(ProjectCommand::Create {
            parent,
            node,
            document,
            index,
        })?;
        self.select([node_id]);
        Ok(node_id)
    }

    pub fn rename(&mut self, node: NodeId, title: impl Into<String>) -> Result<(), WorkspaceError> {
        self.apply(ProjectCommand::Rename {
            node,
            title: title.into(),
        })
    }

    pub fn edit_metadata(
        &mut self,
        node: NodeId,
        metadata: DocumentMetadata,
    ) -> Result<(), WorkspaceError> {
        let document = document_for_node(&self.opened.project, node)?;
        self.apply(ProjectCommand::EditMetadata { document, metadata })
    }

    pub fn duplicate(&mut self, node: NodeId) -> Result<NodeId, WorkspaceError> {
        let parent = parent_for(&self.opened.project, node)?;
        let index = sibling_index(&self.opened.project, parent, node)?.saturating_add(1);
        self.apply(ProjectCommand::Duplicate {
            node,
            parent,
            index,
        })?;
        let copy = self
            .opened
            .project
            .nodes
            .get(&parent)
            .and_then(|entry| entry.children.get(index))
            .copied()
            .ok_or(WorkspaceError::Invariant(
                "duplicate did not produce a sibling",
            ))?;
        self.select([copy]);
        Ok(copy)
    }

    /// Applies an explicitly distinguished drag/drop placement.
    pub fn drop_node(
        &mut self,
        node: NodeId,
        placement: DropPlacement,
    ) -> Result<(), WorkspaceError> {
        let (parent, index) = match placement {
            DropPlacement::Inside(parent) => (parent, child_count(&self.opened.project, parent)?),
            DropPlacement::Before(target) => {
                let parent = parent_for(&self.opened.project, target)?;
                (parent, sibling_index(&self.opened.project, parent, target)?)
            }
            DropPlacement::After(target) => {
                let parent = parent_for(&self.opened.project, target)?;
                (
                    parent,
                    sibling_index(&self.opened.project, parent, target)?.saturating_add(1),
                )
            }
        };
        let current_parent = parent_for(&self.opened.project, node)?;
        if current_parent == parent {
            let old = sibling_index(&self.opened.project, parent, node)?;
            let adjusted = if index > old { index - 1 } else { index };
            self.apply(ProjectCommand::Reorder {
                node,
                index: adjusted,
            })
        } else {
            self.apply(ProjectCommand::Reparent {
                node,
                parent,
                index,
            })
        }
    }

    pub fn move_up(&mut self, node: NodeId) -> Result<(), WorkspaceError> {
        let parent = parent_for(&self.opened.project, node)?;
        let index = sibling_index(&self.opened.project, parent, node)?;
        if index > 0 {
            self.apply(ProjectCommand::Reorder {
                node,
                index: index - 1,
            })?;
        }
        Ok(())
    }

    pub fn move_down(&mut self, node: NodeId) -> Result<(), WorkspaceError> {
        let parent = parent_for(&self.opened.project, node)?;
        let index = sibling_index(&self.opened.project, parent, node)?;
        let count = child_count(&self.opened.project, parent)?;
        if index + 1 < count {
            self.apply(ProjectCommand::Reorder {
                node,
                index: index + 1,
            })?;
        }
        Ok(())
    }

    pub fn indent(&mut self, node: NodeId) -> Result<(), WorkspaceError> {
        let parent = parent_for(&self.opened.project, node)?;
        let index = sibling_index(&self.opened.project, parent, node)?;
        if index == 0 {
            return Ok(());
        }
        let previous = self.opened.project.nodes[&parent].children[index - 1];
        self.apply(ProjectCommand::Reparent {
            node,
            parent: previous,
            index: child_count(&self.opened.project, previous)?,
        })
    }

    pub fn outdent(&mut self, node: NodeId) -> Result<(), WorkspaceError> {
        let parent = parent_for(&self.opened.project, node)?;
        let grandparent = parent_for(&self.opened.project, parent)?;
        let index = sibling_index(&self.opened.project, grandparent, parent)?.saturating_add(1);
        self.apply(ProjectCommand::Reparent {
            node,
            parent: grandparent,
            index,
        })
    }

    pub fn trash(&mut self, node: NodeId) -> Result<(), WorkspaceError> {
        self.apply(ProjectCommand::Trash { node })?;
        let remaining = self
            .selection
            .iter()
            .copied()
            .filter(|id| *id != node)
            .collect::<Vec<_>>();
        self.select(remaining);
        Ok(())
    }

    /// Trashes only selected roots, avoiding a duplicate command for descendants.
    pub fn trash_selection(&mut self) -> Result<(), WorkspaceError> {
        let selected = self.selection.clone();
        let selected_set = selected.iter().copied().collect::<HashSet<_>>();
        for node in selected {
            let mut ancestor = self.opened.project.nodes[&node].parent;
            let mut nested = false;
            while let Some(id) = ancestor {
                if selected_set.contains(&id) {
                    nested = true;
                    break;
                }
                ancestor = self.opened.project.nodes[&id].parent;
            }
            if !nested {
                self.apply(ProjectCommand::Trash { node })?;
            }
        }
        self.select([]);
        Ok(())
    }

    pub fn restore(&mut self, node: NodeId) -> Result<(), WorkspaceError> {
        let tombstone = self
            .opened
            .project
            .trash
            .get(&node)
            .cloned()
            .ok_or(ProjectError::NotTrashed(node))?;
        self.apply(ProjectCommand::Restore {
            node,
            parent: tombstone.parent,
            index: tombstone.index,
        })?;
        self.select([node]);
        Ok(())
    }

    pub fn undo(&mut self) -> Result<bool, WorkspaceError> {
        let Some(command) = self.undo.pop() else {
            return Ok(false);
        };
        let inverse = self.execute_persisted(command)?;
        self.redo.push(inverse);
        self.project_snapshot(None, "", OutlineSort::Binder);
        Ok(true)
    }

    pub fn redo(&mut self) -> Result<bool, WorkspaceError> {
        let Some(command) = self.redo.pop() else {
            return Ok(false);
        };
        let inverse = self.execute_persisted(command)?;
        self.undo.push(inverse);
        self.project_snapshot(None, "", OutlineSort::Binder);
        Ok(true)
    }

    pub fn set_preferences(&mut self, preferences: WorkspacePreferences) {
        self.preferences = preferences;
        reconcile_preferences(&self.opened.project, &mut self.preferences);
        self.select(self.preferences.selected_nodes.clone());
    }

    pub fn save_preferences(&self) -> Result<(), WorkspaceError> {
        let path = self.opened.root().join(".parchmint/workspace.toml");
        let source =
            toml::to_string_pretty(&self.preferences).map_err(WorkspaceError::Preferences)?;
        atomic_write(&path, source.as_bytes()).map_err(WorkspaceError::WritePreferences)
    }

    fn apply(&mut self, command: ProjectCommand) -> Result<(), WorkspaceError> {
        let inverse = self.execute_persisted(command)?;
        self.undo.push(inverse);
        self.redo.clear();
        self.project_snapshot(None, "", OutlineSort::Binder);
        Ok(())
    }

    fn execute_persisted(
        &mut self,
        command: ProjectCommand,
    ) -> Result<ProjectCommand, WorkspaceError> {
        let outcome = self
            .opened
            .execute(command)
            .map_err(WorkspaceError::Storage)?;
        ProjectStorage::save(&mut self.opened).map_err(WorkspaceError::Storage)?;
        self.update_index_events(&outcome.events);
        Ok(outcome.undo.inverse)
    }

    fn update_index_node(&mut self, node: NodeId) {
        if let Err(error) = self.search.upsert_node(&self.opened, node) {
            self.index_diagnostic = Some(error.to_string());
        }
    }

    fn update_index_subtree(&mut self, root: NodeId) {
        let mut nodes = vec![root];
        while let Some(node) = nodes.pop() {
            if let Some(entry) = self.opened.project.nodes.get(&node) {
                nodes.extend(entry.children.iter().copied());
                if entry.kind.document_id().is_some() {
                    self.update_index_node(node);
                }
            }
        }
    }

    fn update_index_events(&mut self, events: &[ProjectEvent]) {
        for event in events {
            match *event {
                ProjectEvent::NodeCreated(node)
                | ProjectEvent::NodeRenamed(node)
                | ProjectEvent::NodeReordered(node)
                | ProjectEvent::NodeReparented { node, .. }
                | ProjectEvent::NodeDuplicated { copy: node, .. }
                | ProjectEvent::NodeRestored(node) => self.update_index_subtree(node),
                ProjectEvent::NodeTrashed(node) => {
                    if let Err(error) = self.search.delete_subtree(&self.opened.project, node) {
                        self.index_diagnostic = Some(error.to_string());
                    }
                }
                ProjectEvent::MetadataEdited(document) => {
                    if let Some(record) = self.opened.project.documents.get(&document) {
                        self.update_index_node(record.node_id);
                    }
                }
                ProjectEvent::StyleMutated(_) | ProjectEvent::StyleReplaced { .. } => {}
            }
        }
    }
}

fn load_preferences(root: &Path) -> (WorkspacePreferences, Option<String>) {
    let path = root.join(".parchmint/workspace.toml");
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return (WorkspacePreferences::default(), None);
        }
        Err(error) => {
            return (
                WorkspacePreferences::default(),
                Some(format!("workspace state could not be read: {error}")),
            );
        }
    };
    match toml::from_str::<WorkspacePreferences>(&source) {
        Ok(preferences) if preferences.version == WORKSPACE_FORMAT_VERSION => (preferences, None),
        Ok(preferences) => (
            WorkspacePreferences::default(),
            Some(format!(
                "workspace format {} is unsupported",
                preferences.version
            )),
        ),
        Err(error) => (
            WorkspacePreferences::default(),
            Some(format!("workspace state is invalid: {error}")),
        ),
    }
}

fn reconcile_preferences(project: &Project, preferences: &mut WorkspacePreferences) {
    preferences.version = WORKSPACE_FORMAT_VERSION;
    preferences
        .selected_nodes
        .retain(|id| project.nodes.contains_key(id) && !project.is_trashed(*id));
    preferences
        .expanded_nodes
        .retain(|id| project.nodes.contains_key(id) && !project.is_trashed(*id));
    preferences.focused_pane = preferences.focused_pane.min(1);
    preferences.split_ratio_milli = preferences.split_ratio_milli.clamp(100, 900);
    for pane in &mut preferences.panes {
        let valid = pane
            .node
            .is_some_and(|id| project.nodes.contains_key(&id) && !project.is_trashed(id));
        if !valid {
            pane.node = None;
            pane.pinned = false;
            pane.cursor = 0;
            pane.scroll = 0;
        }
        if pane.view == PaneView::Attachment && pane.node.is_none() {
            pane.view = PaneView::Outline;
        }
    }
    if !preferences.split_enabled {
        preferences.panes[1].pinned = false;
    }
}

fn is_within_research(project: &Project, node: NodeId) -> bool {
    let mut current = Some(node);
    let mut seen = BTreeSet::new();
    while let Some(id) = current {
        if !seen.insert(id) {
            return false;
        }
        if id == project.research_root() {
            return true;
        }
        current = project.nodes.get(&id).and_then(|entry| entry.parent);
    }
    false
}

fn preferred_view_for_node(project: &Project, node: NodeId) -> PaneView {
    if project
        .nodes
        .get(&node)
        .is_some_and(|entry| entry.kind.is_builtin_root())
    {
        return PaneView::Outline;
    }
    project
        .nodes
        .get(&node)
        .and_then(|entry| entry.kind.document_id())
        .and_then(|id| project.documents.get(&id))
        .and_then(|record| record.metadata.attachment)
        .map_or(PaneView::Editor, |_| PaneView::Attachment)
}

fn document_for_node(project: &Project, node: NodeId) -> Result<DocumentId, WorkspaceError> {
    project
        .nodes
        .get(&node)
        .and_then(|entry| entry.kind.document_id())
        .ok_or(ProjectError::MissingDocumentForNode(node).into())
}

fn parent_for(project: &Project, node: NodeId) -> Result<NodeId, WorkspaceError> {
    project
        .nodes
        .get(&node)
        .ok_or(ProjectError::MissingNode(node))?
        .parent
        .ok_or(ProjectError::RootMutation(node).into())
}

fn child_count(project: &Project, parent: NodeId) -> Result<usize, WorkspaceError> {
    project
        .nodes
        .get(&parent)
        .map(|entry| entry.children.len())
        .ok_or(ProjectError::MissingNode(parent).into())
}

fn sibling_index(project: &Project, parent: NodeId, node: NodeId) -> Result<usize, WorkspaceError> {
    project
        .nodes
        .get(&parent)
        .ok_or(ProjectError::MissingNode(parent))?
        .children
        .iter()
        .position(|id| *id == node)
        .ok_or(WorkspaceError::Invariant("node is absent from parent"))
}

fn build_snapshot(
    project: &Project,
    focus: Option<NodeId>,
    filter: &str,
    sort: OutlineSort,
) -> BinderSnapshot {
    let query = filter.trim().to_lowercase();
    let roots = focus
        .filter(|id| project.nodes.contains_key(id) && !project.is_trashed(*id))
        .map_or_else(|| project.roots.to_vec(), |id| vec![id]);
    let mut matching = BTreeSet::new();
    if !query.is_empty() {
        for (id, node) in &project.nodes {
            if project.is_trashed(*id) || !matches_row(project, node, &query) {
                continue;
            }
            let mut current = Some(*id);
            while let Some(value) = current {
                matching.insert(value);
                current = project.nodes.get(&value).and_then(|entry| entry.parent);
            }
        }
    }
    let mut rows = Vec::new();
    for root in roots {
        append_rows(project, root, 0, &query, &matching, sort, &mut rows);
    }
    BinderSnapshot { rows }
}

fn matches_row(project: &Project, node: &Node, query: &str) -> bool {
    let Some(document) = node
        .kind
        .document_id()
        .and_then(|id| project.documents.get(&id))
    else {
        return node.id.to_string().contains(query);
    };
    document.metadata.title.to_lowercase().contains(query)
        || document.metadata.summary.to_lowercase().contains(query)
        || document
            .metadata
            .status
            .as_deref()
            .unwrap_or_default()
            .to_lowercase()
            .contains(query)
        || document
            .metadata
            .labels
            .iter()
            .any(|label| label.to_lowercase().contains(query))
}

fn append_rows(
    project: &Project,
    id: NodeId,
    depth: u16,
    query: &str,
    matching: &BTreeSet<NodeId>,
    sort: OutlineSort,
    rows: &mut Vec<BinderRow>,
) {
    if project.is_trashed(id) || (!query.is_empty() && !matching.contains(&id)) {
        return;
    }
    let node = &project.nodes[&id];
    let metadata = node
        .kind
        .document_id()
        .and_then(|id| project.documents.get(&id).map(|record| &record.metadata));
    rows.push(BinderRow {
        id,
        parent: node.parent,
        depth,
        is_group: matches!(node.kind, NodeKind::Group { .. }),
        is_root: node.kind.is_builtin_root(),
        has_children: !node.children.is_empty(),
        title: metadata.map_or_else(
            || project.builtin_root_key(id).unwrap_or("root").to_owned(),
            |entry| entry.title.clone(),
        ),
        synopsis: metadata.map_or_else(String::new, |entry| entry.summary.clone()),
        status: metadata
            .and_then(|entry| entry.status.clone())
            .unwrap_or_default(),
        label: metadata
            .and_then(|entry| entry.labels.first().cloned())
            .unwrap_or_default(),
        word_count: 0,
        include_in_compile: metadata
            .and_then(|entry| entry.flags.get("include-in-compile").copied())
            .unwrap_or(false),
    });
    let mut children = node.children.clone();
    if sort != OutlineSort::Binder {
        children.sort_by(|left, right| {
            let left_meta = project.nodes[left]
                .kind
                .document_id()
                .and_then(|id| project.documents.get(&id));
            let right_meta = project.nodes[right]
                .kind
                .document_id()
                .and_then(|id| project.documents.get(&id));
            let key = |value: Option<&DocumentRecord>| match sort {
                OutlineSort::Title => (
                    String::new(),
                    value.map_or_else(String::new, |entry| entry.metadata.title.to_lowercase()),
                ),
                OutlineSort::Status => (
                    value
                        .and_then(|entry| entry.metadata.status.as_ref())
                        .map_or_else(String::new, |status| status.to_lowercase()),
                    value.map_or_else(String::new, |entry| entry.metadata.title.to_lowercase()),
                ),
                OutlineSort::Binder => unreachable!(),
            };
            key(left_meta).cmp(&key(right_meta))
        });
    }
    for child in children {
        append_rows(
            project,
            child,
            depth.saturating_add(1),
            query,
            matching,
            sort,
            rows,
        );
    }
}

/// User-displayable structural or project-shell failure.
#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Domain(#[from] ProjectError),
    #[error("workspace state could not be read: {0}")]
    ReadPreferences(std::io::Error),
    #[error("workspace state could not be written: {0}")]
    WritePreferences(parchmint_storage::AtomicWriteError),
    #[error("workspace state is invalid: {0}")]
    Preferences(toml::ser::Error),
    #[error("recent-project state is invalid: {0}")]
    ReadPreferencesFormat(toml::de::Error),
    #[error("workspace invariant failed: {0}")]
    Invariant(&'static str),
    #[error("document source is invalid: {0}")]
    InvalidDocument(String),
    #[error(transparent)]
    Search(#[from] SearchServiceError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    use tempfile::tempdir;

    #[test]
    fn structural_commands_filtering_undo_and_restart_share_one_snapshot() {
        let directory = tempdir().unwrap();
        let mut workspace = ProjectWorkspace::create(directory.path(), "Outline").unwrap();
        let manuscript = workspace.project().manuscript_root();
        let part = workspace.create_node(manuscript, "Part I", true).unwrap();
        let chapter = workspace.create_node(part, "The Orchard", true).unwrap();
        let scene = workspace.create_node(chapter, "Arrival", false).unwrap();
        let mut metadata = workspace.project().documents
            [&document_for_node(workspace.project(), scene).unwrap()]
            .metadata
            .clone();
        metadata.summary = "Mara reaches the orchard.".into();
        metadata.status = Some("Draft".into());
        metadata.labels = vec!["Opening".into()];
        workspace.edit_metadata(scene, metadata).unwrap();
        workspace.project_snapshot(None, "orchard", OutlineSort::Title);
        assert_eq!(
            workspace.snapshot().rows().len(),
            4,
            "matching row retains ancestors"
        );
        assert_eq!(workspace.snapshot().rows()[3].title, "Arrival");
        workspace.project_snapshot(None, "", OutlineSort::Binder);
        workspace.indent(scene).unwrap(); // first child cannot indent, so this is a no-op
        workspace.trash(chapter).unwrap();
        assert!(
            !workspace
                .snapshot()
                .rows()
                .iter()
                .any(|row| row.id == chapter)
        );
        assert!(workspace.undo().unwrap());
        assert!(
            workspace
                .snapshot()
                .rows()
                .iter()
                .any(|row| row.id == chapter)
        );
        drop(workspace);
        let reopened = ProjectWorkspace::open(directory.path()).unwrap();
        assert_eq!(reopened.snapshot().rows().len(), 5);
    }

    #[test]
    fn before_after_inside_and_cycle_errors_leave_snapshot_authoritative() {
        let directory = tempdir().unwrap();
        let mut workspace = ProjectWorkspace::create(directory.path(), "Moves").unwrap();
        let root = workspace.project().manuscript_root();
        let first = workspace.create_node(root, "First", true).unwrap();
        let second = workspace.create_node(root, "Second", true).unwrap();
        workspace
            .drop_node(second, DropPlacement::Before(first))
            .unwrap();
        assert_eq!(
            workspace.project().nodes[&root].children,
            vec![second, first]
        );
        let before = workspace.snapshot().clone();
        assert!(
            workspace
                .drop_node(first, DropPlacement::Inside(first))
                .is_err()
        );
        assert_eq!(
            workspace.snapshot(),
            &before,
            "failed command emits no optimistic row update"
        );
    }

    #[test]
    fn ten_thousand_node_projection_is_bounded_and_lazy_at_the_consumer_edge() {
        let mut project = Project::new("Stress");
        let root = project.manuscript_root();
        for index in 0..10_000 {
            let node_id = NodeId::new();
            let document_id = DocumentId::new();
            project.nodes.get_mut(&root).unwrap().children.push(node_id);
            project.nodes.insert(
                node_id,
                Node {
                    id: node_id,
                    kind: NodeKind::Document { document_id },
                    parent: Some(root),
                    children: vec![],
                },
            );
            project.documents.insert(
                document_id,
                DocumentRecord {
                    id: document_id,
                    node_id,
                    path: RelativeProjectPath::new(format!("manuscript/{node_id}.md")).unwrap(),
                    metadata: DocumentMetadata {
                        title: format!("Scene {index}"),
                        ..DocumentMetadata::default()
                    },
                },
            );
        }
        let start = Instant::now();
        let snapshot = build_snapshot(&project, None, "", OutlineSort::Binder);
        assert_eq!(snapshot.len(), 10_002);
        assert_eq!(snapshot.visible_rows(9_990, 40).len(), 12);
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "projection took {:?}",
            start.elapsed()
        );
    }

    #[test]
    fn disposable_search_rebuild_and_incremental_workspace_events_match_canonical_state() {
        let directory = tempdir().unwrap();
        let mut workspace = ProjectWorkspace::create(directory.path(), "Search").unwrap();
        let manuscript = workspace.project().manuscript_root();
        let scene = workspace
            .create_node(manuscript, "Winter Harbor", false)
            .unwrap();
        workspace
            .save_document_body(scene, "Mara walks through the silent orchard.".into())
            .unwrap();

        // Open never scanned bodies, but a rebuild derives the cache solely
        // from canonical-open data and makes the row searchable.
        workspace.rebuild_search_index().unwrap();
        let query = SearchQuery {
            text: "orch",
            ..SearchQuery::default()
        };
        assert_eq!(workspace.search_project(&query, 10).unwrap().len(), 1);
        assert_eq!(
            workspace
                .search_totals(Some("manuscript"), None)
                .unwrap()
                .words,
            6
        );

        let document = document_for_node(workspace.project(), scene).unwrap();
        let mut metadata = workspace.project().documents[&document].metadata.clone();
        metadata.tags = vec!["coast".into()];
        workspace.edit_metadata(scene, metadata).unwrap();
        let tagged = SearchQuery {
            text: "winter",
            tag: Some("coast"),
            ..SearchQuery::default()
        };
        assert_eq!(
            workspace.search_project(&tagged, 10).unwrap()[0].node_id,
            scene.to_string()
        );

        workspace.trash(scene).unwrap();
        assert!(workspace.search_project(&query, 10).unwrap().is_empty());
        workspace.restore(scene).unwrap();
        assert_eq!(workspace.search_project(&query, 10).unwrap().len(), 1);

        std::fs::remove_file(directory.path().join(".parchmint/index.sqlite")).unwrap();
        // A missing cache is disposable: reopening + rebuilding sees exactly
        // the canonical source, not any former SQLite data.
        drop(workspace);
        let mut reopened = ProjectWorkspace::open(directory.path()).unwrap();
        reopened.rebuild_search_index().unwrap();
        assert_eq!(reopened.search_project(&query, 10).unwrap().len(), 1);
    }

    #[test]
    fn research_attachment_and_symmetric_panes_restore_without_compile_inclusion() {
        let directory = tempdir().unwrap();
        let root = directory.path().join("Novel");
        let source = directory.path().join("map.png");
        fs::write(&source, b"not decoded by the application").unwrap();
        let mut workspace = ProjectWorkspace::create(&root, "Novel").unwrap();
        let manuscript = workspace.project().manuscript_root();
        let research = workspace.project().research_root();
        let scene = workspace.create_node(manuscript, "Scene", false).unwrap();
        let note = workspace
            .create_research_node(research, "Research note", false)
            .unwrap();
        let (attachment, asset) = workspace.import_attachment(research, &source).unwrap();
        let attachment_document = document_for_node(workspace.project(), attachment).unwrap();
        assert_eq!(
            workspace.project().documents[&attachment_document]
                .metadata
                .attachment,
            Some(asset.id)
        );
        assert!(
            !workspace.project().documents[&attachment_document]
                .metadata
                .flags["include-in-compile"]
        );
        workspace
            .open_in_pane(0, Some(scene), PaneView::Editor)
            .unwrap();
        workspace
            .open_in_pane(1, Some(note), PaneView::Editor)
            .unwrap();
        workspace
            .set_split(true, SplitOrientation::Vertical, 620)
            .unwrap();
        workspace.set_pane_pin(1, true).unwrap();
        workspace.focus_pane(0).unwrap();
        assert!(workspace.navigate_focused_pane(attachment).unwrap());
        assert_eq!(workspace.pane(0).unwrap().node, Some(attachment));
        assert_eq!(workspace.pane(1).unwrap().node, Some(note));
        let first_cursor = workspace.pane(0).unwrap().cursor;
        workspace.close_pane(1).unwrap();
        assert_eq!(workspace.pane(0).unwrap().cursor, first_cursor);
        drop(workspace);
        let reopened = ProjectWorkspace::open(&root).unwrap();
        assert_eq!(reopened.pane(0).unwrap().node, Some(attachment));
        assert!(reopened.attachments().contains_key(&asset.id));
    }

    #[test]
    fn malformed_or_stale_workspace_falls_back_without_blocking_project_open() {
        let directory = tempdir().unwrap();
        let root = directory.path().join("Novel");
        let workspace = ProjectWorkspace::create(&root, "Novel").unwrap();
        drop(workspace);
        fs::write(
            root.join(".parchmint/workspace.toml"),
            "version = 99\nsplit_ratio_milli = 1\n",
        )
        .unwrap();
        let reopened = ProjectWorkspace::open(&root).unwrap();
        assert!(reopened.workspace_diagnostic().is_some());
        assert_eq!(reopened.preferences().version, WORKSPACE_FORMAT_VERSION);
        assert_eq!(reopened.preferences().split_ratio_milli, 500);
    }
}
