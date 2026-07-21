#![allow(missing_docs)] // Public workspace vocabulary follows docs/user-guide/projects.md.
//! Rust-owned binder, outline, cards, and project-shell use cases.
//!
//! This module deliberately exposes immutable rows.  Qt/QML may retain visual
//! state such as an expanded item, but it never owns another mutable copy of
//! the project graph.

use crate::{
    CanonicalSaveRequest, CompletionDisposition, ContentFingerprint, DocumentLifecycleConfig,
    DocumentLifecycleError, DocumentSession, ExternalChange, ExternalConflict, IndexStatus,
    JournalRequest, ProjectSearch, RecoveryCandidate, RecoveryIssue, RecoveryScan, RecoveryStore,
    SaveState, SearchIndexWorker, SearchRebuildProgress, SearchServiceError, TextStatistics,
    text_statistics,
};
use parchmint_compile::{
    self, CancellationToken, CompileError, CompileInput, CompileIr, CompilePreview, ExportError,
    ExportOptions, ExportReport,
};
use parchmint_domain::{
    CompilePreset, CompilePresetId, DocumentId, DocumentMetadata, DocumentRecord, Node, NodeId,
    NodeKind, Project, ProjectCommand, ProjectError, ProjectEvent, ProjectGeneration,
    RelativeProjectPath, Revision, WorkStamp,
};
use parchmint_index::{CountTotals, SearchQuery, SearchResult};
use parchmint_storage::{
    AttachmentPreview, AttachmentRecord, OpenMode, OpenProject, ProjectSavePlan, ProjectStorage,
    SaveMetrics, ScheduledCommandRollback, StorageError, atomic_write,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::Instant;
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
    positions: BTreeMap<NodeId, usize>,
}

/// Independent invalidation domains used by UI models and background jobs.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorkspaceRevisions {
    pub content: u64,
    pub structure: u64,
    pub selection: u64,
    pub presentation: u64,
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

    /// Constant-logarithmic stable-ID lookup used to encode parent rows once
    /// per cached FFI payload.
    pub fn row_for_node(&self, node: NodeId) -> Option<usize> {
        self.positions.get(&node).copied()
    }

    fn rebuild_positions(&mut self) {
        self.positions = self
            .rows
            .iter()
            .enumerate()
            .map(|(index, row)| (row.id, index))
            .collect();
    }

    fn subtree_range(&self, node: NodeId) -> Option<std::ops::Range<usize>> {
        let start = *self.positions.get(&node)?;
        let depth = self.rows[start].depth;
        let end = self.rows[start + 1..]
            .iter()
            .position(|row| row.depth <= depth)
            .map_or(self.rows.len(), |offset| start + 1 + offset);
        Some(start..end)
    }
}

/// Typed, stable-ID-derived changes consumed by the Qt outline model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OutlineDelta {
    Insert {
        first: usize,
        count: usize,
    },
    Remove {
        first: usize,
        count: usize,
    },
    Move {
        first: usize,
        destination: usize,
        count: usize,
    },
    Data {
        first: usize,
        count: usize,
    },
    Reset,
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

fn literal_match_ranges(source: &str, query: &str, case_sensitive: bool) -> Vec<(usize, usize)> {
    if case_sensitive {
        return source
            .match_indices(query)
            .map(|(start, value)| (start, start + value.len()))
            .collect();
    }
    source
        .char_indices()
        .filter_map(|(start, _)| {
            let end = start.checked_add(query.len())?;
            source
                .get(start..end)
                .is_some_and(|value| value.eq_ignore_ascii_case(query))
                .then_some((start, end))
        })
        .collect()
}

fn replacement_context(source: &str, start: usize, end: usize) -> String {
    let line_start = source[..start].rfind('\n').map_or(0, |index| index + 1);
    let line_end = source[end..]
        .find('\n')
        .map_or(source.len(), |index| end + index);
    let line = &source[line_start..line_end];
    let compact = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= 160 {
        compact
    } else {
        compact.chars().take(157).collect::<String>() + "…"
    }
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

/// One independently selectable literal match in a project replacement preview.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectReplaceMatch {
    pub node: NodeId,
    pub title: String,
    pub start: usize,
    pub end: usize,
    pub context: String,
    pub selected: bool,
}

/// Conflict-protected replacement proposal. Callers may only change `selected`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectReplacePreview {
    query: String,
    replacement: String,
    matches: Vec<ProjectReplaceMatch>,
    fingerprints: std::collections::BTreeMap<NodeId, ContentFingerprint>,
}

impl ProjectReplacePreview {
    pub fn matches(&self) -> &[ProjectReplaceMatch] {
        &self.matches
    }

    pub fn selected_count(&self) -> usize {
        self.matches.iter().filter(|item| item.selected).count()
    }

    pub fn set_selected(&mut self, index: usize, selected: bool) -> bool {
        let Some(item) = self.matches.get_mut(index) else {
            return false;
        };
        item.selected = selected;
        true
    }
}

#[derive(Clone, Debug)]
struct ProjectReplaceUndo {
    originals: Vec<(NodeId, String)>,
    replacements: Vec<(NodeId, String)>,
    expected: Vec<(NodeId, ContentFingerprint)>,
}

struct PendingStructuralSave {
    sequence: u64,
    rollback: ScheduledCommandRollback,
}

struct StructuralSaveCompletion {
    sequence: u64,
    outcome: Result<SaveMetrics, String>,
}

struct StructuralSaveWorker {
    jobs: Option<mpsc::Sender<(u64, ProjectSavePlan)>>,
    completions: mpsc::Receiver<StructuralSaveCompletion>,
    worker: Option<JoinHandle<()>>,
}

impl StructuralSaveWorker {
    fn start() -> Result<Self, std::io::Error> {
        let (jobs, receiver) = mpsc::channel::<(u64, ProjectSavePlan)>();
        let (sender, completions) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("parchmint-project-save".into())
            .spawn(move || {
                let mut failed = None::<String>;
                while let Ok((sequence, plan)) = receiver.recv() {
                    let outcome = failed
                        .clone()
                        .map_or_else(|| plan.execute().map_err(|error| error.to_string()), Err);
                    if let Err(error) = &outcome {
                        failed = Some(error.clone());
                    }
                    if sender
                        .send(StructuralSaveCompletion { sequence, outcome })
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        Ok(Self {
            jobs: Some(jobs),
            completions,
            worker: Some(worker),
        })
    }

    fn submit(&self, sequence: u64, plan: ProjectSavePlan) -> Result<(), WorkspaceError> {
        self.jobs
            .as_ref()
            .ok_or_else(|| WorkspaceError::StructuralPersistence("save worker is closed".into()))?
            .send((sequence, plan))
            .map_err(|_| WorkspaceError::StructuralPersistence("save worker is closed".into()))
    }
}

impl Drop for StructuralSaveWorker {
    fn drop(&mut self) {
        drop(self.jobs.take());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// One project incarnation and all structural/metadata use cases.
pub struct ProjectWorkspace {
    opened: OpenProject,
    generation: ProjectGeneration,
    lifecycle_config: DocumentLifecycleConfig,
    sessions: BTreeMap<DocumentId, DocumentSession>,
    external_conflicts: BTreeMap<DocumentId, ExternalConflict>,
    snapshot: BinderSnapshot,
    selection: Vec<NodeId>,
    undo: Vec<ProjectCommand>,
    redo: Vec<ProjectCommand>,
    preferences: WorkspacePreferences,
    workspace_diagnostic: Option<String>,
    search: ProjectSearch,
    index_diagnostic: Option<String>,
    replace_undo: Option<ProjectReplaceUndo>,
    document_counts: BTreeMap<DocumentId, TextStatistics>,
    subtree_counts: BTreeMap<NodeId, TextStatistics>,
    counts_complete: bool,
    index_worker: Option<SearchIndexWorker>,
    index_revision: u64,
    revisions: WorkspaceRevisions,
    outline_deltas: Vec<OutlineDelta>,
    snapshot_filter: String,
    snapshot_sort: OutlineSort,
    structural_save_worker: Option<StructuralSaveWorker>,
    pending_structural_saves: VecDeque<PendingStructuralSave>,
    next_structural_save: u64,
    structural_save_error: Option<String>,
}

impl Drop for ProjectWorkspace {
    fn drop(&mut self) {
        // Keep the advisory project lock alive until every already-published
        // structural write set has either committed or recovered.
        self.structural_save_worker.take();
    }
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

    pub fn open_read_only(root: impl AsRef<Path>) -> Result<Self, WorkspaceError> {
        let opened =
            ProjectStorage::open(root, OpenMode::ReadOnly).map_err(WorkspaceError::Storage)?;
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
        let mut search = ProjectSearch::open(opened.root());
        let index_revision = 1;
        let index_worker = match SearchIndexWorker::start_canonical(
            search.path().to_owned(),
            opened.root().to_owned(),
            index_revision,
        ) {
            Ok(worker) => {
                search.set_indexing(
                    index_revision,
                    0,
                    u64::try_from(opened.project.documents.len()).unwrap_or(u64::MAX),
                );
                Some(worker)
            }
            Err(error) => {
                search.set_unavailable(error.to_string());
                None
            }
        };
        Self {
            opened,
            generation: ProjectGeneration::new(1).expect("one is a valid generation"),
            lifecycle_config: DocumentLifecycleConfig::default(),
            sessions: BTreeMap::new(),
            external_conflicts: BTreeMap::new(),
            snapshot,
            selection: valid,
            undo: Vec::new(),
            redo: Vec::new(),
            preferences,
            workspace_diagnostic,
            search,
            index_diagnostic: None,
            replace_undo: None,
            document_counts: BTreeMap::new(),
            subtree_counts: BTreeMap::new(),
            counts_complete: false,
            index_worker,
            index_revision,
            revisions: WorkspaceRevisions {
                content: 1,
                structure: 1,
                selection: 1,
                presentation: 1,
            },
            outline_deltas: Vec::new(),
            snapshot_filter: String::new(),
            snapshot_sort: OutlineSort::Binder,
            structural_save_worker: None,
            pending_structural_saves: VecDeque::new(),
            next_structural_save: 1,
            structural_save_error: None,
        }
    }

    pub fn project(&self) -> &Project {
        &self.opened.project
    }

    pub const fn revisions(&self) -> WorkspaceRevisions {
        self.revisions
    }

    pub fn take_outline_deltas(&mut self) -> Vec<OutlineDelta> {
        std::mem::take(&mut self.outline_deltas)
    }

    pub fn project_root(&self) -> &Path {
        self.opened.root()
    }

    pub fn is_read_only(&self) -> bool {
        self.opened.mode() == OpenMode::ReadOnly
    }

    /// Moves structural transaction I/O to the project save worker. Preparing
    /// the bounded dirty set and publishing model deltas remain owner-thread work.
    pub fn enable_deferred_structural_saves(&mut self) -> Result<(), WorkspaceError> {
        if self.is_read_only() || self.structural_save_worker.is_some() {
            return Ok(());
        }
        self.structural_save_worker = Some(
            StructuralSaveWorker::start()
                .map_err(|error| WorkspaceError::StructuralPersistence(error.to_string()))?,
        );
        Ok(())
    }

    pub fn has_pending_structural_saves(&self) -> bool {
        !self.pending_structural_saves.is_empty()
    }

    pub fn structural_save_error(&self) -> Option<&str> {
        self.structural_save_error.as_deref()
    }

    /// Applies all completed persistence acknowledgements without blocking.
    /// A failed transaction rolls back that command and every later queued
    /// optimistic command, then publishes one authoritative model reset.
    pub fn poll_structural_saves(&mut self) -> Result<(), WorkspaceError> {
        loop {
            let completion = match self
                .structural_save_worker
                .as_ref()
                .map(|worker| worker.completions.try_recv())
            {
                None | Some(Err(mpsc::TryRecvError::Empty)) => break,
                Some(Err(mpsc::TryRecvError::Disconnected)) => {
                    return Err(WorkspaceError::StructuralPersistence(
                        "save worker disconnected".into(),
                    ));
                }
                Some(Ok(completion)) => completion,
            };
            let Some(front) = self.pending_structural_saves.front() else {
                return Err(WorkspaceError::Invariant(
                    "structural save completed without a pending command",
                ));
            };
            if front.sequence != completion.sequence {
                return Err(WorkspaceError::Invariant(
                    "structural save completions arrived out of order",
                ));
            }
            match completion.outcome {
                Ok(metrics) => {
                    self.pending_structural_saves.pop_front();
                    ProjectStorage::acknowledge_scheduled_command(&mut self.opened, metrics);
                    if self.pending_structural_saves.is_empty()
                        && self.search.status() == &IndexStatus::RebuildNeeded
                    {
                        self.restart_background_index();
                    }
                }
                Err(error) => {
                    self.structural_save_worker.take();
                    let pending = std::mem::take(&mut self.pending_structural_saves);
                    for command in pending.into_iter().rev() {
                        ProjectStorage::rollback_scheduled_command(
                            &mut self.opened,
                            command.rollback,
                        )?;
                    }
                    self.undo.clear();
                    self.redo.clear();
                    self.snapshot = build_snapshot(
                        &self.opened.project,
                        None,
                        &self.snapshot_filter,
                        self.snapshot_sort,
                    );
                    self.outline_deltas.clear();
                    self.outline_deltas.push(OutlineDelta::Reset);
                    self.revisions.structure = self.revisions.structure.saturating_add(1);
                    self.revisions.content = self.revisions.content.saturating_add(1);
                    self.restart_background_index();
                    self.structural_save_error = Some(error.clone());
                    return Err(WorkspaceError::StructuralPersistence(error));
                }
            }
        }
        Ok(())
    }

    pub fn set_project_generation(
        &mut self,
        generation: ProjectGeneration,
    ) -> Result<(), WorkspaceError> {
        if self.generation != generation {
            self.sessions.clear();
            self.external_conflicts.clear();
            self.generation = generation;
        }
        let nodes = self
            .preferences
            .panes
            .iter()
            .filter_map(|pane| pane.node)
            .collect::<Vec<_>>();
        for node in nodes {
            if let Ok(document) = document_for_node(&self.opened.project, node) {
                self.ensure_session(document)?;
            }
        }
        Ok(())
    }

    pub fn set_lifecycle_config(&mut self, config: DocumentLifecycleConfig) {
        self.lifecycle_config = config;
    }

    fn ensure_session(&mut self, document: DocumentId) -> Result<(), WorkspaceError> {
        if !self.sessions.contains_key(&document) {
            let session = DocumentSession::open(
                &self.opened,
                document,
                self.generation,
                self.lifecycle_config.clone(),
            )?;
            let counts = text_statistics(session.body());
            self.sessions.insert(document, session);
            self.set_document_counts(document, counts)?;
        }
        Ok(())
    }

    fn pane_document_id(&self, pane: usize) -> Result<DocumentId, WorkspaceError> {
        let node = self
            .pane(pane)
            .and_then(|state| state.node)
            .ok_or(WorkspaceError::Invariant("pane has no document"))?;
        document_for_node(&self.opened.project, node)
    }

    pub fn pane_live_body(&mut self, pane: usize) -> Result<&str, WorkspaceError> {
        let document = self.pane_document_id(pane)?;
        self.ensure_session(document)?;
        Ok(self.sessions[&document].body())
    }

    pub fn update_pane_live_body(
        &mut self,
        pane: usize,
        body: String,
        first_block: usize,
        last_block_exclusive: usize,
        now: Instant,
    ) -> Result<WorkStamp, WorkspaceError> {
        let document = self.pane_document_id(pane)?;
        self.ensure_session(document)?;
        let new_counts = text_statistics(&body);
        let session = self
            .sessions
            .get_mut(&document)
            .expect("session was just initialized");
        session.replace_body(body, first_block, last_block_exclusive, now)?;
        let stamp = session.stamp();
        self.set_document_counts(document, new_counts)?;
        self.revisions.content = self.revisions.content.saturating_add(1);
        Ok(stamp)
    }

    /// Applies one bounded Qt text delta and propagates its count adjustment to
    /// the document and all cached ancestors.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_pane_text_delta(
        &mut self,
        pane: usize,
        position_utf16: usize,
        removed_utf16: usize,
        inserted: &str,
        first_block: usize,
        last_block_exclusive: usize,
        now: Instant,
    ) -> Result<WorkStamp, WorkspaceError> {
        let document = self.pane_document_id(pane)?;
        self.ensure_session(document)?;
        let applied = self
            .sessions
            .get_mut(&document)
            .expect("session was just initialized")
            .apply_text_delta(
                position_utf16,
                removed_utf16,
                inserted,
                first_block,
                last_block_exclusive,
                now,
            )?;
        self.apply_document_count_delta(document, applied.counts)?;
        let mut count_node = self
            .opened
            .project
            .documents
            .get(&document)
            .map(|record| record.node_id);
        while let Some(node) = count_node {
            self.refresh_snapshot_node(node);
            count_node = self
                .opened
                .project
                .nodes
                .get(&node)
                .and_then(|entry| entry.parent);
        }
        self.revisions.content = self.revisions.content.saturating_add(1);
        Ok(WorkStamp {
            generation: self.generation,
            revision: applied.revision,
        })
    }

    pub fn pane_text_statistics(&mut self, pane: usize) -> Result<TextStatistics, WorkspaceError> {
        let document = self.pane_document_id(pane)?;
        self.ensure_session(document)?;
        Ok(self.document_counts[&document])
    }

    pub const fn counts_complete(&self) -> bool {
        self.counts_complete
    }

    fn set_document_counts(
        &mut self,
        document: DocumentId,
        counts: TextStatistics,
    ) -> Result<(), WorkspaceError> {
        let previous = self
            .document_counts
            .insert(document, counts)
            .unwrap_or_default();
        self.propagate_count_delta(
            document,
            signed_count_delta(counts.words, previous.words),
            signed_count_delta(counts.characters, previous.characters),
        )
    }

    fn apply_document_count_delta(
        &mut self,
        document: DocumentId,
        delta: crate::TextCountDelta,
    ) -> Result<(), WorkspaceError> {
        let counts = self.document_counts.entry(document).or_default();
        counts.words = apply_signed_count(counts.words, delta.words);
        counts.characters = apply_signed_count(counts.characters, delta.characters);
        self.propagate_count_delta(document, delta.words, delta.characters)
    }

    fn propagate_count_delta(
        &mut self,
        document: DocumentId,
        words: i64,
        characters: i64,
    ) -> Result<(), WorkspaceError> {
        let mut current = Some(
            self.opened
                .project
                .documents
                .get(&document)
                .ok_or(WorkspaceError::Invariant("count document is absent"))?
                .node_id,
        );
        let mut seen = BTreeSet::new();
        while let Some(node) = current {
            if !seen.insert(node) {
                return Err(WorkspaceError::Invariant("count ancestry contains a cycle"));
            }
            let aggregate = self.subtree_counts.entry(node).or_default();
            aggregate.words = apply_signed_count(aggregate.words, words);
            aggregate.characters = apply_signed_count(aggregate.characters, characters);
            current = self
                .opened
                .project
                .nodes
                .get(&node)
                .and_then(|entry| entry.parent);
        }
        Ok(())
    }

    pub fn open_session_ids(&self) -> Vec<DocumentId> {
        self.sessions.keys().copied().collect()
    }

    pub fn dirty_session_ids(&self) -> Vec<DocumentId> {
        self.sessions
            .iter()
            .filter_map(|(id, session)| session.is_dirty().then_some(*id))
            .collect()
    }

    pub fn session_stamp(&self, document: DocumentId) -> Option<WorkStamp> {
        self.sessions.get(&document).map(DocumentSession::stamp)
    }

    pub fn session_revision(&self, document: DocumentId) -> Revision {
        self.sessions
            .get(&document)
            .map_or(Revision::INITIAL, DocumentSession::revision)
    }

    pub fn session_save_state(&self, document: DocumentId) -> Option<&SaveState> {
        self.sessions
            .get(&document)
            .map(DocumentSession::save_state)
    }

    pub fn pane_save_state(&self, pane: usize) -> Option<&SaveState> {
        let document = self.pane_document_id(pane).ok()?;
        self.session_save_state(document)
    }

    pub fn pane_revision(&self, pane: usize) -> Revision {
        self.pane_document_id(pane)
            .ok()
            .map_or(Revision::INITIAL, |id| self.session_revision(id))
    }

    pub fn all_sessions_saved(&self) -> bool {
        self.sessions.values().all(|session| !session.is_dirty())
    }

    pub fn all_dirty_sessions_journaled(&self) -> bool {
        self.sessions.values().all(|session| {
            !session.is_dirty() || session.journaled_revision() >= session.revision()
        })
    }

    pub fn first_session_error(&self) -> Option<String> {
        self.sessions
            .values()
            .find_map(|session| match session.save_state() {
                SaveState::Error(error) => Some(error.clone()),
                _ => None,
            })
    }

    pub fn prepare_session_journal(
        &mut self,
        document: DocumentId,
        now: Instant,
        force: bool,
    ) -> Result<Option<JournalRequest>, WorkspaceError> {
        let session = self
            .sessions
            .get_mut(&document)
            .ok_or(WorkspaceError::Invariant("document session is not open"))?;
        session
            .prepare_journal(now, force)
            .map_err(WorkspaceError::Lifecycle)
    }

    pub fn acknowledge_session_journal(
        &mut self,
        document: DocumentId,
        stamp: WorkStamp,
        outcome: Result<(), String>,
    ) -> CompletionDisposition {
        self.sessions
            .get_mut(&document)
            .map_or(CompletionDisposition::Stale, |session| {
                session.acknowledge_journal(stamp, outcome)
            })
    }

    pub fn prepare_session_canonical(
        &mut self,
        document: DocumentId,
    ) -> Result<Option<(CanonicalSaveRequest, parchmint_storage::DocumentSavePlan)>, WorkspaceError>
    {
        let request = self
            .sessions
            .get_mut(&document)
            .ok_or(WorkspaceError::Invariant("document session is not open"))?
            .prepare_canonical_save()?;
        request
            .map(|request| {
                let plan = request.prepare_disk_plan(&self.opened)?;
                Ok((request, plan))
            })
            .transpose()
            .map_err(WorkspaceError::Lifecycle)
    }

    pub fn acknowledge_session_canonical(
        &mut self,
        document: DocumentId,
        stamp: WorkStamp,
        outcome: Result<(ContentFingerprint, parchmint_storage::DocumentSavePlan), String>,
    ) -> CompletionDisposition {
        let Some(session) = self.sessions.get_mut(&document) else {
            return CompletionDisposition::Stale;
        };
        if session.stamp() != stamp {
            return CompletionDisposition::Stale;
        }
        match outcome {
            Ok((fingerprint, plan)) => {
                if let Err(error) =
                    ProjectStorage::acknowledge_document_save(&mut self.opened, &plan)
                {
                    return session.acknowledge_canonical_save(stamp, Err(error.to_string()));
                }
                let disposition = session.acknowledge_canonical_save(stamp, Ok(fingerprint));
                if disposition == CompletionDisposition::Applied
                    && let Some(record) = self.opened.project.documents.get(&document)
                {
                    let node = record.node_id;
                    if self.index_worker.is_some() {
                        self.restart_background_index();
                    } else {
                        self.update_index_node(node);
                    }
                }
                disposition
            }
            Err(error) => session.acknowledge_canonical_save(stamp, Err(error)),
        }
    }

    pub fn external_poll_plan(
        &self,
        document: DocumentId,
    ) -> Result<(WorkStamp, PathBuf), WorkspaceError> {
        let session = self
            .sessions
            .get(&document)
            .ok_or(WorkspaceError::Invariant("document session is not open"))?;
        let plan = ProjectStorage::prepare_document_save(
            &self.opened,
            document,
            session.body().to_owned(),
        )?;
        Ok((session.stamp(), plan.canonical_path))
    }

    pub fn observe_external_body(
        &mut self,
        document: DocumentId,
        stamp: WorkStamp,
        body: String,
    ) -> Result<ExternalChange, WorkspaceError> {
        let session = self
            .sessions
            .get_mut(&document)
            .ok_or(WorkspaceError::Invariant("document session is not open"))?;
        if session.stamp() != stamp {
            return Ok(ExternalChange::Unchanged);
        }
        let change = session.observe_external_body(body)?;
        match &change {
            ExternalChange::Conflict(conflict) => {
                self.external_conflicts.insert(document, conflict.clone());
            }
            ExternalChange::AutoReloaded(_) => {
                self.external_conflicts.remove(&document);
                if let Some(node_id) = self
                    .opened
                    .project
                    .documents
                    .get(&document)
                    .map(|record| record.node_id)
                {
                    let body = session.body().to_owned();
                    self.opened.set_body(document, body)?;
                    self.update_index_node(node_id);
                }
            }
            ExternalChange::Unchanged => {}
        }
        Ok(change)
    }

    pub fn external_conflicts(&self) -> &BTreeMap<DocumentId, ExternalConflict> {
        &self.external_conflicts
    }

    pub fn resolve_external_reload(&mut self, document: DocumentId) -> Result<(), WorkspaceError> {
        let conflict =
            self.external_conflicts
                .remove(&document)
                .ok_or(WorkspaceError::Invariant(
                    "external conflict is unavailable",
                ))?;
        let session = self
            .sessions
            .get_mut(&document)
            .ok_or(WorkspaceError::Invariant("document session is not open"))?;
        session.resolve_external_reload(&conflict)?;
        self.opened.set_body(document, session.body().to_owned())?;
        Ok(())
    }

    pub fn resolve_external_overwrite(
        &mut self,
        document: DocumentId,
    ) -> Result<(), WorkspaceError> {
        let conflict =
            self.external_conflicts
                .remove(&document)
                .ok_or(WorkspaceError::Invariant(
                    "external conflict is unavailable",
                ))?;
        self.sessions
            .get_mut(&document)
            .ok_or(WorkspaceError::Invariant("document session is not open"))?
            .resolve_external_overwrite(&conflict)?;
        Ok(())
    }

    pub fn save_external_conflict_copy(
        &mut self,
        document: DocumentId,
        destination: &Path,
    ) -> Result<(), WorkspaceError> {
        let conflict =
            self.external_conflicts
                .remove(&document)
                .ok_or(WorkspaceError::Invariant(
                    "external conflict is unavailable",
                ))?;
        self.sessions
            .get_mut(&document)
            .ok_or(WorkspaceError::Invariant("document session is not open"))?
            .save_conflict_copy(&conflict, destination)?;
        let session = self
            .sessions
            .get_mut(&document)
            .expect("session was checked above");
        session.resolve_external_reload(&conflict)?;
        self.opened.set_body(document, session.body().to_owned())?;
        Ok(())
    }

    pub fn recovery_scan(&self) -> Result<RecoveryScan, WorkspaceError> {
        RecoveryStore::scan_isolated(&self.opened).map_err(WorkspaceError::Lifecycle)
    }

    pub fn restore_recovery(
        &mut self,
        candidate: &RecoveryCandidate,
        now: Instant,
    ) -> Result<WorkStamp, WorkspaceError> {
        let document = candidate.record.document_id;
        self.ensure_session(document)?;
        let session = self
            .sessions
            .get_mut(&document)
            .expect("session was just initialized");
        RecoveryStore::restore(session, candidate, now)?;
        Ok(session.stamp())
    }

    pub fn discard_recovery(candidate: RecoveryCandidate) -> Result<(), WorkspaceError> {
        candidate.discard().map_err(WorkspaceError::Lifecycle)
    }

    pub fn discard_recovery_issue(issue: RecoveryIssue) -> Result<(), WorkspaceError> {
        issue.discard().map_err(WorkspaceError::Lifecycle)
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
        if let Some(worker) = self.index_worker.take() {
            worker.cancel();
            drop(worker);
        }
        self.search
            .rebuild(&self.opened)
            .map_err(WorkspaceError::Search)?;
        self.index_diagnostic = None;
        Ok(())
    }

    /// Publishes bounded index/count worker deltas. The UI timer may call this
    /// frequently; it never waits for disk or body scanning.
    pub fn poll_search_index(&mut self) {
        loop {
            let progress = match self
                .index_worker
                .as_ref()
                .map(SearchIndexWorker::try_progress)
            {
                Some(Ok(Some(progress))) => progress,
                Some(Ok(None)) | None => break,
                Some(Err(error)) => {
                    self.search.set_unavailable(error.to_string());
                    self.index_worker = None;
                    break;
                }
            };
            match progress {
                SearchRebuildProgress::Batch {
                    revision,
                    completed,
                    total,
                } if revision == self.index_revision => {
                    self.search.set_indexing(revision, completed, total);
                }
                SearchRebuildProgress::Counts { revision, rows }
                    if revision == self.index_revision =>
                {
                    let mut changed_rows = Vec::new();
                    for row in rows {
                        if let Some(document) = row.document {
                            self.document_counts.insert(
                                document,
                                TextStatistics {
                                    words: row.document_words,
                                    characters: row.document_characters,
                                },
                            );
                        }
                        self.subtree_counts.insert(
                            row.node,
                            TextStatistics {
                                words: row.subtree_words,
                                characters: row.subtree_characters,
                            },
                        );
                        if let Some(&position) = self.snapshot.positions.get(&row.node) {
                            let depth = self.snapshot.rows[position].depth;
                            let words = usize::try_from(row.subtree_words).unwrap_or(usize::MAX);
                            self.snapshot.rows[position] =
                                binder_row(&self.opened.project, row.node, depth, words);
                            changed_rows.push(position);
                        }
                    }
                    changed_rows.sort_unstable();
                    let mut ranges = changed_rows.into_iter().peekable();
                    while let Some(first) = ranges.next() {
                        let mut count = 1usize;
                        while ranges.peek().is_some_and(|next| *next == first + count) {
                            ranges.next();
                            count += 1;
                        }
                        self.outline_deltas
                            .push(OutlineDelta::Data { first, count });
                    }
                }
                SearchRebuildProgress::Complete { revision, .. }
                    if revision == self.index_revision =>
                {
                    self.search.set_ready();
                    self.counts_complete = true;
                    self.index_worker = None;
                    break;
                }
                SearchRebuildProgress::Cancelled { revision }
                    if revision == self.index_revision =>
                {
                    self.index_worker = None;
                    break;
                }
                SearchRebuildProgress::Failed { revision, message }
                    if revision == self.index_revision =>
                {
                    self.search.set_unavailable(message);
                    self.index_worker = None;
                    break;
                }
                _ => {}
            }
        }
    }

    /// Searches the stable cache prefix currently published by the background
    /// worker. This call never scans canonical document bodies synchronously.
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
        if let Some(node) = node
            && let Some((other, _)) = self
                .preferences
                .panes
                .iter()
                .enumerate()
                .find(|(other, pane)| *other != index && pane.node == Some(node))
        {
            self.preferences.focused_pane = u8::try_from(other).unwrap_or(0);
            self.save_preferences()?;
            return Ok(());
        }
        if let Some(node) = node
            && (!self.opened.project.nodes.contains_key(&node)
                || self.opened.project.is_trashed(node))
        {
            return Err(WorkspaceError::Invariant("pane node is unavailable"));
        }
        if let Some(document) = node.and_then(|node| {
            self.opened
                .project
                .nodes
                .get(&node)
                .and_then(|entry| entry.kind.document_id())
        }) {
            self.ensure_session(document)?;
        }
        let pane = self
            .preferences
            .panes
            .get_mut(index)
            .ok_or(WorkspaceError::Invariant("pane index must be zero or one"))?;
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
        if let Some((existing, _)) = self
            .preferences
            .panes
            .iter()
            .enumerate()
            .find(|(pane, state)| *pane != index && state.node == Some(node))
        {
            self.preferences.focused_pane = u8::try_from(existing).unwrap_or(0);
            self.save_preferences()?;
            return Ok(true);
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

    /// Switches the presentation for one live pane without replacing its node
    /// or document session. This keeps the editor's Rust session and the Qt
    /// pane-local cursor/undo state authoritative while planning views are
    /// selected from the same pane header.
    pub fn set_pane_view(&mut self, index: usize, view: PaneView) -> Result<(), WorkspaceError> {
        let pane = self
            .preferences
            .panes
            .get_mut(index)
            .ok_or(WorkspaceError::Invariant("pane index must be zero or one"))?;
        if pane.node.is_none() && matches!(view, PaneView::Editor | PaneView::Attachment) {
            return Err(WorkspaceError::Invariant(
                "open a document before selecting this view",
            ));
        }
        pane.view = view;
        self.preferences.focused_pane = u8::try_from(index).unwrap_or(0);
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
        let closing_document = self.pane_document_id(index).ok();
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
        if let Some(document) = closing_document {
            let still_open = self.preferences.panes.iter().any(|pane| {
                pane.node
                    .and_then(|node| self.opened.project.nodes.get(&node))
                    .and_then(|node| node.kind.document_id())
                    == Some(document)
            });
            if !still_open {
                self.sessions.remove(&document);
                self.external_conflicts.remove(&document);
            }
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

    /// Freezes canonical state for a worker-safe compiler. Qt editor objects
    /// are deliberately absent; callers reject stale completions using `stamp`.
    pub fn compile_input(&self, stamp: WorkStamp) -> Result<CompileInput, WorkspaceError> {
        CompileInput::from_open_project(&self.opened, stamp).map_err(WorkspaceError::Compile)
    }

    /// Computes the ordered compile preview from an immutable canonical snapshot.
    pub fn compile_preview(
        &self,
        preset: &CompilePreset,
        stamp: WorkStamp,
        cancellation: &CancellationToken,
    ) -> Result<CompilePreview, WorkspaceError> {
        let input = self.compile_input(stamp)?;
        parchmint_compile::preview(&input, preset, cancellation).map_err(WorkspaceError::Compile)
    }

    /// Compiles without touching a destination. This is the API a background
    /// app worker owns before calling [`Self::export_compiled`].
    pub fn compile_project(
        &self,
        preset: &CompilePreset,
        stamp: WorkStamp,
        cancellation: &CancellationToken,
    ) -> Result<(CompileIr, Vec<parchmint_compile::CompileWarning>), WorkspaceError> {
        let input = self.compile_input(stamp)?;
        parchmint_compile::compile(&input, preset, cancellation).map_err(WorkspaceError::Compile)
    }

    /// Validates and atomically installs a previously compiled export. A
    /// caller must only use an IR whose stamp still matches its current state.
    pub fn export_compiled(
        &self,
        ir: &CompileIr,
        options: &ExportOptions,
    ) -> Result<ExportReport, WorkspaceError> {
        parchmint_compile::export(ir, options).map_err(WorkspaceError::Export)
    }

    /// Returns persisted compile presets in stable UUID order.
    pub fn compile_presets(&self) -> Vec<&CompilePreset> {
        self.opened.project.compile_presets.values().collect()
    }

    /// Creates or updates one human-readable persisted compile preset.
    pub fn save_compile_preset(&mut self, preset: CompilePreset) -> Result<(), WorkspaceError> {
        self.apply(ProjectCommand::UpsertCompilePreset { preset })
    }

    /// Deletes a compile preset through the normal structural undo layer.
    pub fn remove_compile_preset(&mut self, id: CompilePresetId) -> Result<(), WorkspaceError> {
        self.apply(ProjectCommand::RemoveCompilePreset { id })
    }

    /// Creates a normal Markdown note below the research root or a research
    /// group. The document lifecycle treats it exactly like a
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
        if self.is_read_only() {
            return Err(StorageError::ReadOnly.into());
        }
        let document = document_for_node(&self.opened.project, node)?;
        parchmint_markdown::Document::parse_body(
            &body,
            &parchmint_markdown::ParseOptions::default(),
        )
        .map_err(|error| WorkspaceError::InvalidDocument(error.to_string()))?;
        let counts = text_statistics(&body);
        self.opened
            .set_body(document, body)
            .map_err(WorkspaceError::Storage)?;
        ProjectStorage::save_document(&mut self.opened, document)
            .map_err(WorkspaceError::Storage)?;
        self.set_document_counts(document, counts)?;
        self.revisions.content = self.revisions.content.saturating_add(1);
        if self.index_worker.is_some() {
            self.restart_background_index();
        } else {
            self.update_index_node(node);
        }
        Ok(())
    }

    /// Builds a bounded, literal project-wide replacement preview without writing.
    /// Every match is selected initially and can be independently deselected.
    pub fn preview_project_replace(
        &self,
        query: &str,
        replacement: &str,
        case_sensitive: bool,
    ) -> Result<ProjectReplacePreview, WorkspaceError> {
        if query.is_empty() {
            return Err(WorkspaceError::InvalidReplace(
                "replacement query cannot be empty".into(),
            ));
        }
        if query.len() > 4_096 || replacement.len() > 1024 * 1024 {
            return Err(WorkspaceError::InvalidReplace(
                "replacement query or value exceeds its safety limit".into(),
            ));
        }
        if !case_sensitive && !query.is_ascii() {
            return Err(WorkspaceError::InvalidReplace(
                "case-insensitive project replacement currently requires an ASCII query; use case-sensitive matching for Unicode text".into(),
            ));
        }

        let mut matches = Vec::new();
        let mut fingerprints = std::collections::BTreeMap::new();
        for (node_id, node) in &self.opened.project.nodes {
            if self.opened.project.is_trashed(*node_id) {
                continue;
            }
            let Some(document_id) = node.kind.document_id() else {
                continue;
            };
            let body = self.opened.body(document_id)?;
            let ranges = literal_match_ranges(body, query, case_sensitive);
            if ranges.is_empty() {
                continue;
            }
            fingerprints.insert(*node_id, ContentFingerprint::of(body));
            let title = self
                .opened
                .project
                .documents
                .get(&document_id)
                .map_or_else(String::new, |record| record.metadata.title.clone());
            for (start, end) in ranges {
                if matches.len() >= 10_000 {
                    return Err(WorkspaceError::InvalidReplace(
                        "replacement preview exceeds 10,000 changes; narrow the query".into(),
                    ));
                }
                matches.push(ProjectReplaceMatch {
                    node: *node_id,
                    title: title.clone(),
                    start,
                    end,
                    context: replacement_context(body, start, end),
                    selected: true,
                });
            }
        }
        Ok(ProjectReplacePreview {
            query: query.to_owned(),
            replacement: replacement.to_owned(),
            matches,
            fingerprints,
        })
    }

    /// Applies selected preview rows only after every source still matches the
    /// preview fingerprint. Original bodies are backed up before the first write.
    pub fn apply_project_replace(
        &mut self,
        preview: &ProjectReplacePreview,
    ) -> Result<usize, WorkspaceError> {
        if self.is_read_only() {
            return Err(StorageError::ReadOnly.into());
        }
        let mut by_node = std::collections::BTreeMap::<NodeId, Vec<&ProjectReplaceMatch>>::new();
        for item in preview.matches.iter().filter(|item| item.selected) {
            by_node.entry(item.node).or_default().push(item);
        }
        if by_node.is_empty() {
            return Ok(0);
        }

        let mut updates = Vec::with_capacity(by_node.len());
        for (node, mut selected) in by_node {
            let document = document_for_node(&self.opened.project, node)?;
            let cached = self.opened.body(document)?.to_owned();
            let disk = self.opened.canonical_body_on_disk(document)?;
            let expected = preview
                .fingerprints
                .get(&node)
                .ok_or(WorkspaceError::ReplaceConflict(node))?;
            if ContentFingerprint::of(&cached) != *expected
                || ContentFingerprint::of(&disk) != *expected
            {
                return Err(WorkspaceError::ReplaceConflict(node));
            }
            selected.sort_by_key(|item| std::cmp::Reverse(item.start));
            let mut output = cached.clone();
            for item in selected {
                if output.get(item.start..item.end) != Some(preview.query.as_str())
                    && !output
                        .get(item.start..item.end)
                        .is_some_and(|value| value.eq_ignore_ascii_case(&preview.query))
                {
                    return Err(WorkspaceError::ReplaceConflict(node));
                }
                output.replace_range(item.start..item.end, &preview.replacement);
            }
            parchmint_markdown::Document::parse_body(
                &output,
                &parchmint_markdown::ParseOptions::default(),
            )
            .map_err(|error| WorkspaceError::InvalidDocument(error.to_string()))?;
            updates.push((node, cached, output));
        }

        self.back_up_replacement(&updates)?;
        let originals = updates
            .iter()
            .map(|(node, original, _)| (*node, original.clone()))
            .collect::<Vec<_>>();
        let expected = updates
            .iter()
            .map(|(node, _, output)| (*node, ContentFingerprint::of(output)))
            .collect::<Vec<_>>();
        let replacements = updates
            .iter()
            .map(|(node, _, output)| (*node, output.clone()))
            .collect::<Vec<_>>();
        let mut written = Vec::new();
        for (node, original, output) in &updates {
            if let Err(error) = self.save_document_body(*node, output.clone()) {
                let mut rollback_failures = Vec::new();
                if let Ok(document) = document_for_node(&self.opened.project, *node)
                    && self.opened.set_body(document, original.clone()).is_err()
                {
                    rollback_failures.push(*node);
                }
                for (written_node, written_body) in written.into_iter().rev() {
                    if self.save_document_body(written_node, written_body).is_err() {
                        rollback_failures.push(written_node);
                    }
                }
                return Err(replace_rollback_error(error, &rollback_failures));
            }
            written.push((*node, original.clone()));
        }
        self.replace_undo = Some(ProjectReplaceUndo {
            originals,
            replacements,
            expected,
        });
        Ok(preview.selected_count())
    }

    /// Restores the last project replacement if none of its documents changed.
    pub fn undo_project_replace(&mut self) -> Result<usize, WorkspaceError> {
        let undo = self.replace_undo.take().ok_or_else(|| {
            WorkspaceError::InvalidReplace("there is no project replacement to undo".into())
        })?;
        let conflict = undo.expected.iter().find_map(|(node, expected)| {
            let document = document_for_node(&self.opened.project, *node).ok()?;
            let disk = self.opened.canonical_body_on_disk(document).ok()?;
            (ContentFingerprint::of(&disk) != *expected).then_some(*node)
        });
        if let Some(node) = conflict {
            self.replace_undo = Some(undo);
            return Err(WorkspaceError::ReplaceConflict(node));
        }
        let count = undo.originals.len();
        let mut restored = Vec::new();
        for (node, body) in &undo.originals {
            if let Err(error) = self.save_document_body(*node, body.clone()) {
                let mut rollback_failures = Vec::new();
                for restored_node in restored.into_iter().rev() {
                    if let Some((_, replacement)) = undo
                        .replacements
                        .iter()
                        .find(|(candidate, _)| *candidate == restored_node)
                        && self
                            .save_document_body(restored_node, replacement.clone())
                            .is_err()
                    {
                        rollback_failures.push(restored_node);
                    }
                }
                self.replace_undo = Some(undo);
                return Err(replace_rollback_error(error, &rollback_failures));
            }
            restored.push(*node);
        }
        Ok(count)
    }

    fn back_up_replacement(
        &self,
        updates: &[(NodeId, String, String)],
    ) -> Result<(), WorkspaceError> {
        let transaction = self
            .opened
            .root()
            .join(".parchmint/backups/project-replace")
            .join(NodeId::new().to_string());
        fs::create_dir_all(&transaction).map_err(WorkspaceError::CreateReplaceBackup)?;
        for (node, original, _) in updates {
            atomic_write(&transaction.join(format!("{node}.md")), original.as_bytes())
                .map_err(WorkspaceError::WriteReplaceBackup)?;
        }
        atomic_write(
            &transaction.join("README.txt"),
            b"ParchMint project replacement backup. Each file contains the original Markdown body.\n",
        )
        .map_err(WorkspaceError::WriteReplaceBackup)
    }

    /// Changes selection only after removing stale, trashed, and duplicate IDs.
    pub fn select(&mut self, nodes: impl IntoIterator<Item = NodeId>) {
        let mut seen = BTreeSet::new();
        let selection = nodes
            .into_iter()
            .filter(|id| self.opened.project.nodes.contains_key(id))
            .filter(|id| !self.opened.project.is_trashed(*id))
            .filter(|id| seen.insert(*id))
            .collect();
        if self.selection != selection {
            self.revisions.selection = self.revisions.selection.saturating_add(1);
        }
        self.selection = selection;
        self.preferences.selected_nodes = self.selection.clone();
        let _ = self.save_preferences();
    }

    /// Rebuilds a projection from authoritative Rust state. Filtering keeps all
    /// ancestors of a matching row, and sorting is never a structural mutation.
    pub fn project_snapshot(&mut self, focus: Option<NodeId>, filter: &str, sort: OutlineSort) {
        let mut snapshot = build_snapshot(&self.opened.project, focus, filter, sort);
        // Derived totals are never rebuilt synchronously by a projection. Rows
        // whose canonical bodies have not been indexed/opened remain explicitly
        // zero while `counts_complete` is false.
        for row in &mut snapshot.rows {
            let Some(_document) = self
                .opened
                .project
                .nodes
                .get(&row.id)
                .and_then(|node| node.kind.document_id())
            else {
                continue;
            };
            row.word_count = self
                .subtree_counts
                .get(&row.id)
                .and_then(|counts| usize::try_from(counts.words).ok())
                .unwrap_or(0);
        }
        self.snapshot = snapshot;
        filter.clone_into(&mut self.snapshot_filter);
        self.snapshot_sort = sort;
        self.outline_deltas.push(OutlineDelta::Reset);
        self.revisions.presentation = self.revisions.presentation.saturating_add(1);
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
        let mut subtree = BTreeSet::new();
        let mut pending = vec![node];
        while let Some(current) = pending.pop() {
            if !subtree.insert(current) {
                continue;
            }
            if let Some(entry) = self.opened.project.nodes.get(&current) {
                pending.extend(entry.children.iter().copied());
            }
        }
        let documents = subtree
            .iter()
            .filter_map(|id| self.opened.project.nodes[id].kind.document_id())
            .collect::<Vec<_>>();
        self.apply(ProjectCommand::Trash { node })?;
        for pane in &mut self.preferences.panes {
            if pane.node.is_some_and(|id| subtree.contains(&id)) {
                *pane = PaneWorkspaceState {
                    view: PaneView::Outline,
                    ..PaneWorkspaceState::default()
                };
            }
        }
        for document in documents {
            self.sessions.remove(&document);
            self.external_conflicts.remove(&document);
        }
        self.preferences.focused_pane = self.preferences.focused_pane.min(1);
        self.save_preferences()?;
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
        let index = self
            .opened
            .project
            .nodes
            .get(&tombstone.parent)
            .map_or(tombstone.index, |parent| {
                tombstone.index.min(parent.children.len())
            });
        self.apply(ProjectCommand::Restore {
            node,
            parent: tombstone.parent,
            index,
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
        Ok(true)
    }

    pub fn redo(&mut self) -> Result<bool, WorkspaceError> {
        let Some(command) = self.redo.pop() else {
            return Ok(false);
        };
        let inverse = self.execute_persisted(command)?;
        self.undo.push(inverse);
        Ok(true)
    }

    pub fn set_preferences(&mut self, preferences: WorkspacePreferences) {
        self.preferences = preferences;
        reconcile_preferences(&self.opened.project, &mut self.preferences);
        self.select(self.preferences.selected_nodes.clone());
    }

    pub fn save_preferences(&self) -> Result<(), WorkspaceError> {
        if self.is_read_only() {
            return Ok(());
        }
        let path = self.opened.root().join(".parchmint/workspace.toml");
        let source =
            toml::to_string_pretty(&self.preferences).map_err(WorkspaceError::Preferences)?;
        atomic_write(&path, source.as_bytes()).map_err(WorkspaceError::WritePreferences)
    }

    fn apply(&mut self, command: ProjectCommand) -> Result<(), WorkspaceError> {
        let inverse = self.execute_persisted(command)?;
        self.undo.push(inverse);
        self.redo.clear();
        Ok(())
    }

    fn execute_persisted(
        &mut self,
        command: ProjectCommand,
    ) -> Result<ProjectCommand, WorkspaceError> {
        if self.is_read_only() {
            return Err(StorageError::ReadOnly.into());
        }
        let outcome = if let Some(worker) = self.structural_save_worker.as_ref() {
            let scheduled = ProjectStorage::schedule_command(&mut self.opened, command)
                .map_err(WorkspaceError::Storage)?;
            let sequence = self.next_structural_save;
            self.next_structural_save = self.next_structural_save.saturating_add(1);
            if let Err(error) = worker.submit(sequence, scheduled.plan) {
                ProjectStorage::rollback_scheduled_command(&mut self.opened, scheduled.rollback)?;
                return Err(error);
            }
            self.pending_structural_saves
                .push_back(PendingStructuralSave {
                    sequence,
                    rollback: scheduled.rollback,
                });
            scheduled.outcome
        } else {
            ProjectStorage::execute_command(&mut self.opened, command)
                .map_err(WorkspaceError::Storage)?
        };
        for event in &outcome.events {
            match event {
                ProjectEvent::NodeCreated(_)
                | ProjectEvent::NodeReordered(_)
                | ProjectEvent::NodeReparented { .. }
                | ProjectEvent::NodeDuplicated { .. }
                | ProjectEvent::NodeTrashed(_)
                | ProjectEvent::NodeRestored(_) => {
                    self.revisions.structure = self.revisions.structure.saturating_add(1);
                }
                ProjectEvent::NodeRenamed(_) | ProjectEvent::MetadataEdited(_) => {
                    self.revisions.content = self.revisions.content.saturating_add(1);
                }
                ProjectEvent::StyleMutated(_)
                | ProjectEvent::StyleReplaced { .. }
                | ProjectEvent::CompilePresetSaved(_)
                | ProjectEvent::CompilePresetRemoved(_) => {
                    self.revisions.presentation = self.revisions.presentation.saturating_add(1);
                }
            }
        }
        self.apply_outline_events(&outcome.events)?;
        if self.index_worker.is_some() {
            if self.has_pending_structural_saves() {
                if let Some(worker) = self.index_worker.take() {
                    worker.cancel();
                    drop(worker);
                }
                self.counts_complete = false;
                self.search.set_rebuild_needed();
            } else {
                self.restart_background_index();
            }
        } else {
            self.update_index_events(&outcome.events);
        }
        Ok(outcome.undo.inverse)
    }

    fn apply_outline_events(&mut self, events: &[ProjectEvent]) -> Result<(), WorkspaceError> {
        if !self.snapshot_filter.is_empty() || self.snapshot_sort != OutlineSort::Binder {
            let filter = self.snapshot_filter.clone();
            let sort = self.snapshot_sort;
            self.project_snapshot(None, &filter, sort);
            return Ok(());
        }
        for event in events {
            match *event {
                ProjectEvent::NodeCreated(node)
                | ProjectEvent::NodeDuplicated { copy: node, .. }
                | ProjectEvent::NodeRestored(node) => self.insert_snapshot_subtree(node)?,
                ProjectEvent::NodeTrashed(node) => self.remove_snapshot_subtree(node)?,
                ProjectEvent::NodeReordered(node) | ProjectEvent::NodeReparented { node, .. } => {
                    self.move_snapshot_subtree(node)?;
                }
                ProjectEvent::NodeRenamed(node) => self.refresh_snapshot_node(node),
                ProjectEvent::MetadataEdited(document) => {
                    if let Some(node) = self
                        .opened
                        .project
                        .documents
                        .get(&document)
                        .map(|record| record.node_id)
                    {
                        self.refresh_snapshot_node(node);
                    }
                }
                ProjectEvent::StyleMutated(_)
                | ProjectEvent::StyleReplaced { .. }
                | ProjectEvent::CompilePresetSaved(_)
                | ProjectEvent::CompilePresetRemoved(_) => {}
            }
        }
        Ok(())
    }

    fn insertion_row(&self, node: NodeId) -> Result<usize, WorkspaceError> {
        let entry = self
            .opened
            .project
            .nodes
            .get(&node)
            .ok_or(WorkspaceError::Invariant("inserted outline node is absent"))?;
        let parent = entry
            .parent
            .ok_or(WorkspaceError::Invariant("inserted outline node is a root"))?;
        let siblings = &self.opened.project.nodes[&parent].children;
        let sibling = siblings
            .iter()
            .position(|candidate| *candidate == node)
            .ok_or(WorkspaceError::Invariant(
                "inserted node is absent from parent",
            ))?;
        if sibling == 0 {
            return self
                .snapshot
                .positions
                .get(&parent)
                .map(|row| row.saturating_add(1))
                .ok_or(WorkspaceError::Invariant("outline parent row is absent"));
        }
        let previous = siblings[sibling - 1];
        self.snapshot
            .subtree_range(previous)
            .map(|range| range.end)
            .ok_or(WorkspaceError::Invariant("previous sibling row is absent"))
    }

    fn snapshot_subtree_rows(
        &self,
        root: NodeId,
        depth: u16,
    ) -> Result<Vec<BinderRow>, WorkspaceError> {
        let mut rows = Vec::new();
        let mut pending = vec![(root, depth)];
        while let Some((node, node_depth)) = pending.pop() {
            let entry = self
                .opened
                .project
                .nodes
                .get(&node)
                .ok_or(WorkspaceError::Invariant("outline subtree node is absent"))?;
            rows.push(binder_row(
                &self.opened.project,
                node,
                node_depth,
                self.subtree_counts
                    .get(&node)
                    .and_then(|counts| usize::try_from(counts.words).ok())
                    .unwrap_or(0),
            ));
            pending.extend(
                entry
                    .children
                    .iter()
                    .rev()
                    .map(|child| (*child, node_depth.saturating_add(1))),
            );
        }
        Ok(rows)
    }

    fn insert_snapshot_subtree(&mut self, node: NodeId) -> Result<(), WorkspaceError> {
        let destination = self.insertion_row(node)?;
        let parent = self.opened.project.nodes[&node]
            .parent
            .ok_or(WorkspaceError::Invariant("inserted node is a root"))?;
        let depth = self.snapshot.rows[*self
            .snapshot
            .positions
            .get(&parent)
            .ok_or(WorkspaceError::Invariant("outline parent row is absent"))?]
        .depth
        .saturating_add(1);
        let rows = self.snapshot_subtree_rows(node, depth)?;
        let count = rows.len();
        self.snapshot.rows.splice(destination..destination, rows);
        self.snapshot.rebuild_positions();
        self.outline_deltas.push(OutlineDelta::Insert {
            first: destination,
            count,
        });
        self.refresh_snapshot_node(parent);
        Ok(())
    }

    fn remove_snapshot_subtree(&mut self, node: NodeId) -> Result<(), WorkspaceError> {
        let parent = self.opened.project.nodes[&node].parent;
        let range = self
            .snapshot
            .subtree_range(node)
            .ok_or(WorkspaceError::Invariant("removed outline row is absent"))?;
        let first = range.start;
        let count = range.len();
        self.snapshot.rows.drain(range);
        self.snapshot.rebuild_positions();
        self.outline_deltas
            .push(OutlineDelta::Remove { first, count });
        if let Some(parent) = parent {
            self.refresh_snapshot_node(parent);
        }
        Ok(())
    }

    fn move_snapshot_subtree(&mut self, node: NodeId) -> Result<(), WorkspaceError> {
        let range = self
            .snapshot
            .subtree_range(node)
            .ok_or(WorkspaceError::Invariant("moved outline row is absent"))?;
        let first = range.start;
        let mut rows = self.snapshot.rows.drain(range).collect::<Vec<_>>();
        let old_parent = rows.first().and_then(|row| row.parent);
        let count = rows.len();
        self.snapshot.rebuild_positions();
        let destination = self.insertion_row(node)?;
        let parent = self.opened.project.nodes[&node]
            .parent
            .ok_or(WorkspaceError::Invariant("moved node is a root"))?;
        let target_depth = self.snapshot.rows[self.snapshot.positions[&parent]]
            .depth
            .saturating_add(1);
        let source_depth = rows[0].depth;
        rows[0].parent = Some(parent);
        for row in &mut rows {
            row.depth = if target_depth >= source_depth {
                row.depth.saturating_add(target_depth - source_depth)
            } else {
                row.depth.saturating_sub(source_depth - target_depth)
            };
        }
        self.snapshot.rows.splice(destination..destination, rows);
        self.snapshot.rebuild_positions();
        self.outline_deltas.push(OutlineDelta::Move {
            first,
            destination,
            count,
        });
        self.refresh_snapshot_node(parent);
        if old_parent != Some(parent)
            && let Some(old_parent) = old_parent
        {
            self.refresh_snapshot_node(old_parent);
        }
        Ok(())
    }

    fn refresh_snapshot_node(&mut self, node: NodeId) {
        let Some(&row) = self.snapshot.positions.get(&node) else {
            return;
        };
        let depth = self.snapshot.rows[row].depth;
        let words = self
            .subtree_counts
            .get(&node)
            .and_then(|counts| usize::try_from(counts.words).ok())
            .unwrap_or(0);
        self.snapshot.rows[row] = binder_row(&self.opened.project, node, depth, words);
        self.outline_deltas.push(OutlineDelta::Data {
            first: row,
            count: 1,
        });
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
                ProjectEvent::StyleMutated(_)
                | ProjectEvent::StyleReplaced { .. }
                | ProjectEvent::CompilePresetSaved(_)
                | ProjectEvent::CompilePresetRemoved(_) => {}
            }
        }
    }

    /// Restarts an open-time build by reopening canonical headers and lazy body
    /// handles on the worker. The cancelled revision can never publish into the
    /// replacement revision, so a slow scan cannot overwrite newer state.
    fn restart_background_index(&mut self) {
        if let Some(worker) = self.index_worker.take() {
            worker.cancel();
            drop(worker);
        }
        self.index_revision = self.index_revision.saturating_add(1);
        self.counts_complete = false;
        match SearchIndexWorker::start_canonical(
            self.search.path().to_owned(),
            self.opened.root().to_owned(),
            self.index_revision,
        ) {
            Ok(worker) => {
                self.search.set_indexing(
                    self.index_revision,
                    0,
                    u64::try_from(self.opened.project.documents.len()).unwrap_or(u64::MAX),
                );
                self.index_worker = Some(worker);
                self.index_diagnostic = None;
            }
            Err(error) => {
                self.search.set_unavailable(error.to_string());
                self.index_diagnostic = Some(error.to_string());
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

/// Wraps a project-replace failure so the user learns when the best-effort
/// rollback also failed; otherwise the project could be left partially
/// replaced with no visible signal.
fn replace_rollback_error(error: WorkspaceError, rollback_failures: &[NodeId]) -> WorkspaceError {
    if rollback_failures.is_empty() {
        return error;
    }
    let nodes = rollback_failures
        .iter()
        .map(NodeId::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    WorkspaceError::InvalidReplace(format!(
        "{error}; rollback also failed for {} document(s) ({nodes}). Compare them against the originals under .parchmint/backups/project-replace.",
        rollback_failures.len()
    ))
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
    let mut active = BTreeSet::new();
    let mut active_pending = project.roots.to_vec();
    while let Some(node) = active_pending.pop() {
        if !active.insert(node) {
            continue;
        }
        if let Some(entry) = project.nodes.get(&node) {
            active_pending.extend(entry.children.iter().copied());
        }
    }
    let mut matching = BTreeSet::new();
    if !query.is_empty() {
        for id in &active {
            let node = &project.nodes[id];
            if !matches_row(project, node, &query) {
                continue;
            }
            let mut current = Some(*id);
            while let Some(value) = current {
                if !matching.insert(value) {
                    break;
                }
                current = project.nodes.get(&value).and_then(|entry| entry.parent);
            }
        }
    }
    let mut rows = Vec::new();
    for root in roots {
        let mut pending = vec![(root, 0u16)];
        while let Some((id, depth)) = pending.pop() {
            if !query.is_empty() && !matching.contains(&id) {
                continue;
            }
            let node = &project.nodes[&id];
            rows.push(binder_row(project, id, depth, 0));
            let mut children = node.children.clone();
            sort_children(project, &mut children, sort);
            pending.extend(
                children
                    .into_iter()
                    .rev()
                    .map(|child| (child, depth.saturating_add(1))),
            );
        }
    }
    let positions = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row.id, index))
        .collect();
    BinderSnapshot { rows, positions }
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

fn binder_row(project: &Project, id: NodeId, depth: u16, word_count: usize) -> BinderRow {
    let node = &project.nodes[&id];
    let metadata = node.kind.document_id().and_then(|document| {
        project
            .documents
            .get(&document)
            .map(|record| &record.metadata)
    });
    BinderRow {
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
        word_count,
        include_in_compile: metadata
            .and_then(|entry| entry.flags.get("include-in-compile").copied())
            .unwrap_or(false),
    }
}

fn sort_children(project: &Project, children: &mut [NodeId], sort: OutlineSort) {
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
}

fn signed_count_delta(after: u64, before: u64) -> i64 {
    if after >= before {
        i64::try_from(after - before).unwrap_or(i64::MAX)
    } else {
        -i64::try_from(before - after).unwrap_or(i64::MAX)
    }
}

fn apply_signed_count(value: u64, delta: i64) -> u64 {
    if delta >= 0 {
        value.saturating_add(delta.cast_unsigned())
    } else {
        value.saturating_sub(delta.unsigned_abs())
    }
}

/// User-displayable structural or project-shell failure.
#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Lifecycle(#[from] DocumentLifecycleError),
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
    #[error("project replacement is invalid: {0}")]
    InvalidReplace(String),
    #[error("project replacement stopped because document {0} changed after preview")]
    ReplaceConflict(NodeId),
    #[error("project replacement backup directory could not be created: {0}")]
    CreateReplaceBackup(std::io::Error),
    #[error("project replacement backup could not be written: {0}")]
    WriteReplaceBackup(parchmint_storage::AtomicWriteError),
    #[error("project metadata could not be saved: {0}")]
    StructuralPersistence(String),
    #[error(transparent)]
    Search(#[from] SearchServiceError),
    #[error(transparent)]
    Compile(CompileError),
    #[error(transparent)]
    Export(ExportError),
}

impl WorkspaceError {
    pub fn is_project_locked(&self) -> bool {
        matches!(self, Self::Storage(StorageError::ProjectLocked(_)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    use tempfile::tempdir;

    fn flush_live_document(workspace: &mut ProjectWorkspace, document: DocumentId) {
        let journal = workspace
            .prepare_session_journal(document, Instant::now(), true)
            .unwrap()
            .unwrap();
        journal.execute().unwrap();
        workspace.acknowledge_session_journal(document, journal.stamp, Ok(()));
        let (request, plan) = workspace
            .prepare_session_canonical(document)
            .unwrap()
            .unwrap();
        let fingerprint = request.execute_disk(&plan, request.stamp).unwrap();
        workspace.acknowledge_session_canonical(document, request.stamp, Ok((fingerprint, plan)));
    }

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
        workspace
            .save_document_body(scene, "Mara reaches the orchard at dawn.".into())
            .unwrap();
        workspace.project_snapshot(None, "orchard", OutlineSort::Title);
        assert_eq!(
            workspace.snapshot().rows().len(),
            4,
            "matching row retains ancestors"
        );
        assert_eq!(workspace.snapshot().rows()[3].title, "Arrival");
        assert_eq!(workspace.snapshot().rows()[3].word_count, 6);
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
    fn deferred_structural_save_publishes_before_disk_and_flushes_in_order() {
        let directory = tempdir().unwrap();
        let mut workspace = ProjectWorkspace::create(directory.path(), "Deferred").unwrap();
        workspace.enable_deferred_structural_saves().unwrap();
        let manuscript = workspace.project().manuscript_root();
        let started = Instant::now();
        let scene = workspace
            .create_node(manuscript, "Immediate scene", false)
            .unwrap();
        assert!(started.elapsed() < Duration::from_millis(100));
        workspace.rename(scene, "Queued rename").unwrap();
        assert!(workspace.has_pending_structural_saves());
        let deadline = Instant::now() + Duration::from_secs(2);
        while workspace.has_pending_structural_saves() && Instant::now() < deadline {
            workspace.poll_structural_saves().unwrap();
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(!workspace.has_pending_structural_saves());
        assert_eq!(workspace.project().documents.len(), 1);
        drop(workspace);
        let reopened = ProjectWorkspace::open(directory.path()).unwrap();
        assert_eq!(reopened.project().documents.len(), 1);
        let row = reopened.snapshot().row_for_node(scene).unwrap();
        assert_eq!(reopened.snapshot().rows()[row].title, "Queued rename");
    }

    #[test]
    fn pane_view_switches_keep_the_live_document_reference() {
        let directory = tempdir().unwrap();
        let mut workspace = ProjectWorkspace::create(directory.path(), "Views").unwrap();
        let manuscript = workspace.project().manuscript_root();
        let scene = workspace.create_node(manuscript, "Scene", false).unwrap();
        workspace.open_node_in_pane(0, scene).unwrap();
        let live = workspace.pane_document_id(0).unwrap();

        workspace.set_pane_view(0, PaneView::Outline).unwrap();
        workspace.set_pane_view(0, PaneView::Cards).unwrap();
        workspace.set_pane_view(0, PaneView::Editor).unwrap();
        assert_eq!(workspace.pane_document_id(0).unwrap(), live);
        assert_eq!(workspace.pane(0).unwrap().view, PaneView::Editor);

        workspace.close_pane(0).unwrap();
        assert!(workspace.set_pane_view(0, PaneView::Editor).is_err());
        workspace.set_pane_view(0, PaneView::Cards).unwrap();
    }

    #[test]
    fn restore_clamps_a_stale_sibling_index() {
        let directory = tempdir().unwrap();
        let mut workspace = ProjectWorkspace::create(directory.path(), "Restore").unwrap();
        let root = workspace.project().manuscript_root();
        let first = workspace.create_node(root, "First", false).unwrap();
        let second = workspace.create_node(root, "Second", false).unwrap();

        workspace.trash(second).unwrap();
        workspace.trash(first).unwrap();
        workspace.restore(second).unwrap();

        assert_eq!(workspace.project().nodes[&root].children, [second]);
        workspace.project().validate().unwrap();
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
    fn deeply_nested_projection_uses_no_user_depth_stack() {
        let mut project = Project::new("Deep");
        let mut parent = project.manuscript_root();
        for index in 0..20_000 {
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
                    path: RelativeProjectPath::new(format!("manuscript/deep-{index}.md")).unwrap(),
                    metadata: DocumentMetadata {
                        title: format!("Depth {index}"),
                        ..DocumentMetadata::default()
                    },
                },
            );
            parent = node_id;
        }
        let snapshot = build_snapshot(&project, None, "", OutlineSort::Binder);
        assert_eq!(snapshot.len(), 20_002);
        assert_eq!(
            snapshot.rows().iter().map(|row| row.depth).max(),
            Some(20_000)
        );
    }

    #[test]
    fn structural_commands_publish_typed_deltas_and_independent_revisions() {
        let directory = tempdir().unwrap();
        let mut workspace = ProjectWorkspace::create(directory.path(), "Deltas").unwrap();
        let root = workspace.project().manuscript_root();
        let initial = workspace.revisions();

        let first = workspace.create_node(root, "First", false).unwrap();
        let create_deltas = workspace.take_outline_deltas();
        assert!(
            create_deltas
                .iter()
                .any(|delta| matches!(delta, OutlineDelta::Insert { count: 1, .. }))
        );
        assert!(!create_deltas.contains(&OutlineDelta::Reset));
        assert!(workspace.revisions().structure > initial.structure);

        let after_create = workspace.revisions();
        workspace.rename(first, "Renamed").unwrap();
        let rename_deltas = workspace.take_outline_deltas();
        assert!(
            rename_deltas
                .iter()
                .any(|delta| matches!(delta, OutlineDelta::Data { count: 1, .. }))
        );
        assert!(!rename_deltas.contains(&OutlineDelta::Reset));
        assert_eq!(workspace.revisions().structure, after_create.structure);
        assert!(workspace.revisions().content > after_create.content);

        let second = workspace.create_node(root, "Second", false).unwrap();
        workspace.take_outline_deltas();
        workspace.move_up(second).unwrap();
        assert!(
            workspace
                .take_outline_deltas()
                .iter()
                .any(|delta| matches!(delta, OutlineDelta::Move { count: 1, .. }))
        );

        let before_selection = workspace.revisions();
        workspace.select([first]);
        assert!(workspace.revisions().selection > before_selection.selection);
        assert_eq!(workspace.revisions().content, before_selection.content);
        assert_eq!(workspace.revisions().structure, before_selection.structure);

        let reference = build_snapshot(workspace.project(), None, "", OutlineSort::Binder);
        assert_eq!(workspace.snapshot(), &reference);
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
            .open_in_pane(0, Some(note), PaneView::Editor)
            .unwrap();
        assert_eq!(workspace.preferences().focused_pane, 1);
        assert_eq!(workspace.pane(0).unwrap().node, Some(scene));
        workspace.focus_pane(0).unwrap();
        assert!(workspace.navigate_focused_pane(note).unwrap());
        assert_eq!(workspace.preferences().focused_pane, 1);
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
    fn live_revision_survives_export_swap_close_trash_and_reopen_without_focus_loss() {
        let directory = tempdir().unwrap();
        let root = directory.path().join("Novel");
        let mut workspace = ProjectWorkspace::create(&root, "Novel").unwrap();
        workspace
            .set_project_generation(ProjectGeneration::new(91).unwrap())
            .unwrap();
        let scene = workspace
            .create_node(workspace.project().manuscript_root(), "Scene", false)
            .unwrap();
        let document = document_for_node(workspace.project(), scene).unwrap();
        workspace
            .open_in_pane(0, Some(scene), PaneView::Editor)
            .unwrap();
        workspace
            .set_split(true, SplitOrientation::Horizontal, 500)
            .unwrap();
        let latest = "Typing that never lost focus.\n";
        let stamp = workspace
            .update_pane_live_body(0, latest.into(), 0, 1, Instant::now())
            .unwrap();
        assert_eq!(stamp.revision, Revision::new(1));
        assert_eq!(workspace.pane_live_body(0).unwrap(), latest);

        flush_live_document(&mut workspace, document);
        let compile = workspace.compile_input(stamp).unwrap();
        assert_eq!(compile.bodies[&document].load().unwrap().as_ref(), latest);

        workspace.swap_panes().unwrap();
        assert_eq!(workspace.pane_live_body(1).unwrap(), latest);
        workspace.close_pane(1).unwrap();
        assert!(workspace.open_session_ids().is_empty());

        workspace
            .open_in_pane(0, Some(scene), PaneView::Editor)
            .unwrap();
        assert_eq!(workspace.pane_live_body(0).unwrap(), latest);
        workspace.trash(scene).unwrap();
        assert!(workspace.pane(0).unwrap().node.is_none());
        assert!(workspace.open_session_ids().is_empty());
        drop(workspace);

        let reopened = ProjectWorkspace::open(&root).unwrap();
        assert_eq!(reopened.document_body(scene).unwrap(), latest);
        assert!(reopened.project().is_trashed(scene));
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

    #[test]
    fn compile_presets_are_command_validated_and_survive_restart() {
        let directory = tempdir().unwrap();
        let root = directory.path().join("Novel");
        let mut workspace = ProjectWorkspace::create(&root, "Novel").unwrap();
        let scene = workspace
            .create_node(workspace.project().manuscript_root(), "Scene", false)
            .unwrap();
        let mut preset = CompilePreset::manuscript("Submission");
        preset.selected_roots = vec![scene];
        preset.metadata.author = "A. Writer".into();
        preset
            .exporter_settings
            .entry("html".into())
            .or_default()
            .insert("assets".into(), "self_contained".into());
        let preset_id = preset.id;
        workspace.save_compile_preset(preset).unwrap();
        assert_eq!(workspace.compile_presets().len(), 1);
        drop(workspace);

        let mut reopened = ProjectWorkspace::open(&root).unwrap();
        let persisted = reopened.compile_presets()[0];
        assert_eq!(persisted.id, preset_id);
        assert_eq!(persisted.selected_roots, vec![scene]);
        assert_eq!(persisted.metadata.author, "A. Writer");
        reopened.remove_compile_preset(preset_id).unwrap();
        assert!(reopened.compile_presets().is_empty());
    }

    #[test]
    fn project_replace_previews_selection_conflicts_backups_and_undo() {
        let directory = tempdir().unwrap();
        let root = directory.path().join("Novel");
        let mut workspace = ProjectWorkspace::create(&root, "Novel").unwrap();
        let manuscript = workspace.project().manuscript_root();
        let first = workspace.create_node(manuscript, "First", false).unwrap();
        let second = workspace.create_node(manuscript, "Second", false).unwrap();
        workspace
            .save_document_body(first, "mist on the misty hill\n".into())
            .unwrap();
        workspace
            .save_document_body(second, "MIST and mist\n".into())
            .unwrap();

        let mut preview = workspace
            .preview_project_replace("mist", "fog", true)
            .unwrap();
        assert_eq!(preview.matches().len(), 3);
        let first_exact = preview
            .matches()
            .iter()
            .position(|item| item.node == first && item.start == 0)
            .unwrap();
        assert!(preview.set_selected(first_exact, false));
        assert_eq!(workspace.apply_project_replace(&preview).unwrap(), 2);
        assert_eq!(
            workspace.document_body(first).unwrap(),
            "mist on the fogy hill\n"
        );
        assert_eq!(workspace.document_body(second).unwrap(), "MIST and fog\n");
        assert!(root.join(".parchmint/backups/project-replace").is_dir());
        assert_eq!(workspace.undo_project_replace().unwrap(), 2);
        assert_eq!(
            workspace.document_body(first).unwrap(),
            "mist on the misty hill\n"
        );

        let stale = workspace
            .preview_project_replace("mist", "rain", true)
            .unwrap();
        workspace
            .save_document_body(first, "an external-equivalent local change\n".into())
            .unwrap();
        assert!(matches!(
            workspace.apply_project_replace(&stale),
            Err(WorkspaceError::ReplaceConflict(id)) if id == first
        ));
    }
}
