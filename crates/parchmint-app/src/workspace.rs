#![allow(missing_docs)] // Public bridge vocabulary is documented by the Stage 04 handoff.
//! Rust-owned binder, outline, cards, and project-shell use cases.
//!
//! This module deliberately exposes immutable rows.  Qt/QML may retain visual
//! state such as an expanded item, but it never owns another mutable copy of
//! the project graph.

use parchmint_domain::{
    DocumentId, DocumentMetadata, DocumentRecord, Node, NodeId, NodeKind, Project, ProjectCommand,
    ProjectError, RelativeProjectPath,
};
use parchmint_storage::{OpenMode, OpenProject, ProjectStorage, StorageError, atomic_write};
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

/// Local, disposable UI state. Removing this file cannot affect a project.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspacePreferences {
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
        let preferences = load_preferences(opened.root());
        let valid = preferences
            .selected_nodes
            .iter()
            .copied()
            .filter(|id| opened.project.nodes.contains_key(id) && !opened.project.is_trashed(*id))
            .collect();
        let snapshot = build_snapshot(&opened.project, None, "", OutlineSort::Binder);
        Self {
            opened,
            snapshot,
            selection: valid,
            undo: Vec::new(),
            redo: Vec::new(),
            preferences,
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
                flags: std::collections::BTreeMap::from([("include-in-compile".into(), true)]),
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
        Ok(outcome.undo.inverse)
    }
}

fn load_preferences(root: &Path) -> WorkspacePreferences {
    let path = root.join(".parchmint/workspace.toml");
    fs::read_to_string(path)
        .ok()
        .and_then(|source| toml::from_str(&source).ok())
        .unwrap_or_default()
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
}
