#![allow(missing_docs)] // Error payload fields are self-describing and documented in the handoff.
//! Canonical ParchMint v1 project storage.
//!
//! `.parchmint/` is deliberately absent from the authoritative model: it can
//! contain an advisory lock and local workspace/index state, but deleting it
//! leaves every project document, manifest, style, asset, and tombstone intact.

use noyalib::compat::serde_yaml::{self as yaml, Mapping, Value};
use parchmint_domain::{
    CommandOutcome, DocumentId, DocumentMetadata, DocumentRecord, Node, NodeId, NodeKind, Project,
    ProjectCommand, ProjectError, ProjectEvent, ProjectId, RelativeProjectPath, StyleDefinition,
    TrashTombstone,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, OnceLock};
use tempfile::NamedTempFile;
use thiserror::Error;
use uuid::Uuid;

/// The only supported canonical project version.
pub const FORMAT_VERSION: u32 = 1;
/// Maximum individual TOML manifest size accepted before parsing.
pub const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;
/// Maximum Markdown document size accepted by this storage layer.
pub const MAX_DOCUMENT_BYTES: u64 = 64 * 1024 * 1024;
/// Maximum YAML front-matter size accepted before parsing.
pub const MAX_FRONT_MATTER_BYTES: usize = 256 * 1024;
/// Maximum imported attachment size. Attachments are copied rather than linked
/// so a project remains portable and the original is never modified.
pub const MAX_ATTACHMENT_BYTES: u64 = 100 * 1024 * 1024;
/// Independently versioned catalog format. It is additive to project format 1.
pub const ASSET_CATALOG_VERSION: u32 = 1;
const MAX_YAML_NESTING: usize = 64;

/// Whether an opened project may make canonical changes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenMode {
    /// Acquires an advisory writer lock.
    ReadWrite,
    /// Never acquires a lock and never saves.
    ReadOnly,
}

/// Result of a read-only support/CI validation pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationReport {
    /// Canonical root used for validation.
    pub root: PathBuf,
    /// Detected format version.
    pub format_version: u32,
    /// Number of active and trashed Markdown records.
    pub documents: usize,
    /// Non-fatal observations.
    pub warnings: Vec<String>,
}

/// An opened project and its source-preserving document state.
pub struct OpenProject {
    /// Project graph and front-matter metadata.
    pub project: Project,
    root: PathBuf,
    mode: OpenMode,
    _lock: Option<AdvisoryLock>,
    bodies: BTreeMap<DocumentId, DocumentBodySnapshot>,
    unknown_front_matter: BTreeMap<DocumentId, Mapping>,
    locations: BTreeMap<DocumentId, RelativeProjectPath>,
    pending_locations: BTreeMap<DocumentId, RelativeProjectPath>,
    manifest_extra: BTreeMap<String, toml::Value>,
    outline_extra: BTreeMap<String, toml::Value>,
    styles_extra: BTreeMap<String, toml::Value>,
    attachments: BTreeMap<parchmint_domain::AssetId, AttachmentRecord>,
    dirty: DirtySet,
    last_save_metrics: SaveMetrics,
}

/// Canonical resources changed since the last acknowledged save. The set is
/// deliberately resource-shaped: a metadata edit can dirty one Markdown file
/// without implying that every document body changed.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct DirtySet {
    pub manifest: bool,
    pub outline: bool,
    pub styles: bool,
    pub attachments: bool,
    pub documents: BTreeSet<DocumentId>,
    pub tombstones: BTreeSet<NodeId>,
}

impl DirtySet {
    pub fn is_empty(&self) -> bool {
        !self.manifest
            && !self.outline
            && !self.styles
            && !self.attachments
            && self.documents.is_empty()
            && self.tombstones.is_empty()
    }
}

/// Observable canonical I/O from the last acknowledged save operation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SaveMetrics {
    pub files_written: usize,
    pub files_removed: usize,
    pub bytes_written: u64,
}

/// Immutable canonical write set prepared on the project owner and executed by
/// the serial persistence worker. It contains only dirty resources.
pub struct ProjectSavePlan {
    root: PathBuf,
    mutations: BTreeMap<String, Option<PlannedContents>>,
    location_updates: Vec<(DocumentId, RelativeProjectPath)>,
}

enum PlannedContents {
    Bytes(Vec<u8>),
    Outline(Outline),
}

impl ProjectSavePlan {
    /// Commits the prepared write set with the project recovery protocol.
    pub fn execute(self) -> Result<SaveMetrics, StorageError> {
        self.execute_with_fault(None)
    }

    fn execute_with_fault(
        self,
        fault: Option<TransactionFault>,
    ) -> Result<SaveMetrics, StorageError> {
        let mutations = self
            .mutations
            .into_iter()
            .map(|(path, contents)| {
                let contents = contents
                    .map(|contents| match contents {
                        PlannedContents::Bytes(bytes) => Ok(bytes),
                        PlannedContents::Outline(outline) => toml_bytes(&outline),
                    })
                    .transpose()?;
                Ok((path, contents))
            })
            .collect::<Result<BTreeMap<_, _>, StorageError>>()?;
        transactional_write_set(&self.root, &mutations, fault)
    }
}

/// Opaque owner-thread state needed to reverse a scheduled command if its
/// worker transaction fails. Only affected document locations are retained.
pub struct ScheduledCommandRollback {
    outcome: CommandOutcome,
    prior_locations: Vec<(DocumentId, Option<RelativeProjectPath>)>,
    prior_metrics: SaveMetrics,
}

impl ScheduledCommandRollback {
    /// Events published optimistically for this scheduled command.
    pub fn outcome(&self) -> &CommandOutcome {
        &self.outcome
    }
}

/// Owner-thread command result plus the disk plan that may be moved to a
/// worker. The project has already advanced when this value is returned.
pub struct ScheduledProjectCommand {
    pub outcome: CommandOutcome,
    pub plan: ProjectSavePlan,
    pub rollback: ScheduledCommandRollback,
}

/// Cheap, thread-safe handle to one immutable canonical body revision. Open
/// stores deferred file handles; editors replace them with loaded snapshots.
/// A cloned handle always observes the revision captured when it was cloned.
#[derive(Clone, Debug)]
pub struct DocumentBodySnapshot(Arc<DocumentBodySource>);

#[derive(Debug)]
struct DocumentBodySource {
    document: DocumentId,
    path: Option<PathBuf>,
    loaded: OnceLock<Arc<str>>,
}

impl DocumentBodySnapshot {
    fn deferred(document: DocumentId, path: PathBuf) -> Self {
        Self(Arc::new(DocumentBodySource {
            document,
            path: Some(path),
            loaded: OnceLock::new(),
        }))
    }

    fn loaded(document: DocumentId, body: Arc<str>) -> Self {
        let loaded = OnceLock::new();
        let _ = loaded.set(body);
        Self(Arc::new(DocumentBodySource {
            document,
            path: None,
            loaded,
        }))
    }

    /// Creates an already-loaded snapshot for an immutable worker fixture or
    /// caller-owned canonical revision.
    pub fn from_body(document: DocumentId, body: impl Into<Arc<str>>) -> Self {
        Self::loaded(document, body.into())
    }

    /// Loads at most this body and returns a shared immutable buffer.
    pub fn load(&self) -> Result<Arc<str>, StorageError> {
        self.load_ref().map(Arc::clone)
    }

    fn load_ref(&self) -> Result<&Arc<str>, StorageError> {
        if self.0.loaded.get().is_none() {
            let path = self
                .0
                .path
                .as_ref()
                .ok_or(StorageError::MissingBody(self.0.document))?;
            let source = read_bounded(path, MAX_DOCUMENT_BYTES, "document")?;
            let (_, body, _) = parse_document(&source, self.0.document)?;
            let _ = self.0.loaded.set(Arc::from(body));
        }
        self.0
            .loaded
            .get()
            .ok_or(StorageError::MissingBody(self.0.document))
    }
}

/// Deterministic structural-save failure injection used by slow-disk and
/// partial-commit tests. Production calls always use `None`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransactionFault {
    AfterMutation(usize),
}

/// Immutable canonical write payload prepared on the Rust owner thread and
/// safe to execute on a persistence worker. It contains no mutable project
/// state and therefore cannot let a worker race the domain model.
#[derive(Clone, Debug)]
pub struct DocumentSavePlan {
    pub document_id: DocumentId,
    pub body: String,
    pub canonical_path: PathBuf,
    pub canonical_bytes: Vec<u8>,
}

/// Safe, immutable metadata for one copied project attachment.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AttachmentRecord {
    pub id: parchmint_domain::AssetId,
    /// Original user-facing filename; never used as a path.
    pub display_name: String,
    /// Validated filename below `assets/`, generated from the stable ID.
    pub safe_name: String,
    /// Conservative media type inferred from the filename extension only.
    pub media_type: String,
    pub bytes: u64,
}

/// Rendering policy for an attachment. No active content is embedded or run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachmentPreview {
    Image,
    Pdf,
    PlainText,
    ExternalOnly,
}

impl OpenProject {
    /// Canonical filesystem root.
    pub fn root(&self) -> &Path {
        &self.root
    }
    /// Immutable asset catalog. The catalog is canonical but attachment bytes
    /// are never parsed or executed by Rust/QML.
    pub fn attachments(&self) -> &BTreeMap<parchmint_domain::AssetId, AttachmentRecord> {
        &self.attachments
    }
    /// Open mode selected by the caller.
    pub const fn mode(&self) -> OpenMode {
        self.mode
    }
    /// Pending resource set, exposed for diagnostics and performance tests.
    pub fn dirty_set(&self) -> &DirtySet {
        &self.dirty
    }
    /// Files and bytes touched by the last acknowledged canonical operation.
    pub const fn last_save_metrics(&self) -> SaveMetrics {
        self.last_save_metrics
    }
    /// Returns a document body without its YAML front matter.
    pub fn body(&self, id: DocumentId) -> Result<&str, StorageError> {
        self.bodies
            .get(&id)
            .ok_or(StorageError::MissingBody(id))?
            .load_ref()
            .map(AsRef::as_ref)
    }
    /// Cheap immutable body handles suitable for a derived-state worker.
    pub fn body_snapshot(&self) -> BTreeMap<DocumentId, DocumentBodySnapshot> {
        self.bodies.clone()
    }
    /// Number of body buffers resident in memory, for profiling and open-time
    /// regression tests.
    pub fn loaded_body_count(&self) -> usize {
        self.bodies
            .values()
            .filter(|body| body.0.loaded.get().is_some())
            .count()
    }
    /// Reads the current canonical body from disk, bypassing the in-memory copy.
    /// This is used by external-change detection and never mutates open state.
    pub fn canonical_body_on_disk(&self, id: DocumentId) -> Result<String, StorageError> {
        let record = self
            .project
            .documents
            .get(&id)
            .ok_or(StorageError::MissingBody(id))?;
        let path = resolve_project_path(&self.root, &record.path)?;
        let source = read_bounded(&path, MAX_DOCUMENT_BYTES, "document")?;
        let (_, body, _) = parse_document(&source, id)?;
        Ok(body)
    }
    /// Replaces a document body; callers should use the Markdown crate for semantic changes.
    pub fn set_body(&mut self, id: DocumentId, body: String) -> Result<(), StorageError> {
        if body.len() as u64 > MAX_DOCUMENT_BYTES {
            return Err(StorageError::SizeLimit("document", MAX_DOCUMENT_BYTES));
        }
        if !self.bodies.contains_key(&id) {
            return Err(StorageError::MissingBody(id));
        }
        self.bodies
            .insert(id, DocumentBodySnapshot::loaded(id, Arc::from(body)));
        self.dirty.documents.insert(id);
        Ok(())
    }
    /// Applies a graph command and initializes source-preserving Markdown state
    /// for newly created or duplicated documents. Call [`ProjectStorage::save`]
    /// after a batch to acknowledge it durably.
    pub fn execute(&mut self, command: ProjectCommand) -> Result<CommandOutcome, StorageError> {
        let prior_dirty = self.dirty.clone();
        let prior_pending_locations = self.pending_locations.clone();
        let outcome = self
            .project
            .execute(command)
            .map_err(StorageError::Domain)?;
        let result = (|| {
            for event in &outcome.events {
                match *event {
                    ProjectEvent::NodeCreated(node) => self.initialize_new_document(node, None)?,
                    ProjectEvent::NodeDuplicated { source, copy } => {
                        self.copy_subtree_bodies(source, copy)?;
                    }
                    _ => {}
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.rollback_command_state(&outcome)?;
            return Err(error);
        }
        if let Err(error) = self.mark_dirty(&outcome.events) {
            self.rollback_command_state(&outcome)?;
            self.dirty = prior_dirty;
            self.pending_locations = prior_pending_locations;
            return Err(error);
        }
        Ok(outcome)
    }
    fn mark_dirty(&mut self, events: &[ProjectEvent]) -> Result<(), StorageError> {
        for event in events {
            match *event {
                ProjectEvent::NodeCreated(node) => {
                    self.dirty.outline = true;
                    self.mark_subtree_documents(node, false)?;
                }
                ProjectEvent::NodeDuplicated { copy, .. } => {
                    self.dirty.outline = true;
                    self.mark_subtree_documents(copy, false)?;
                }
                ProjectEvent::NodeReparented { node, .. } | ProjectEvent::NodeRestored(node) => {
                    self.dirty.outline = true;
                    self.mark_subtree_documents(node, false)?;
                    self.dirty.tombstones.insert(node);
                }
                ProjectEvent::NodeTrashed(node) => {
                    self.dirty.outline = true;
                    self.mark_subtree_documents(node, true)?;
                    self.dirty.tombstones.insert(node);
                }
                ProjectEvent::NodeReordered(_) => self.dirty.outline = true,
                ProjectEvent::NodeRenamed(node) => {
                    let document = self
                        .project
                        .nodes
                        .get(&node)
                        .and_then(|entry| entry.kind.document_id())
                        .ok_or(StorageError::InvalidSchema("renamed node lacks document"))?;
                    self.dirty.documents.insert(document);
                }
                ProjectEvent::MetadataEdited(document) => {
                    self.dirty.documents.insert(document);
                }
                ProjectEvent::StyleMutated(_)
                | ProjectEvent::StyleReplaced { .. }
                | ProjectEvent::CompilePresetSaved(_)
                | ProjectEvent::CompilePresetRemoved(_) => {
                    self.dirty.styles = true;
                }
            }
        }
        Ok(())
    }
    fn mark_subtree_documents(&mut self, root: NodeId, trash: bool) -> Result<(), StorageError> {
        let folder = if trash {
            "trash"
        } else {
            root_folder(&self.project, root)?
        };
        let mut pending = vec![root];
        while let Some(node) = pending.pop() {
            let entry = self
                .project
                .nodes
                .get(&node)
                .ok_or(StorageError::InvalidSchema("changed subtree node vanished"))?;
            pending.extend(entry.children.iter().copied());
            if let Some(document) = entry.kind.document_id() {
                self.dirty.documents.insert(document);
                self.pending_locations.insert(
                    document,
                    RelativeProjectPath::new(format!("{folder}/{node}.md"))?,
                );
            }
        }
        Ok(())
    }
    fn rollback_command_state(&mut self, outcome: &CommandOutcome) -> Result<(), StorageError> {
        let created_documents = match outcome.events.as_slice() {
            [ProjectEvent::NodeCreated(node)] => self.subtree_documents(*node),
            [ProjectEvent::NodeDuplicated { copy, .. }] => self.subtree_documents(*copy),
            _ => Vec::new(),
        };
        self.project
            .rollback(outcome)
            .map_err(StorageError::Domain)?;
        for document in created_documents {
            self.bodies.remove(&document);
            self.unknown_front_matter.remove(&document);
            self.locations.remove(&document);
            self.pending_locations.remove(&document);
        }
        Ok(())
    }
    fn subtree_documents(&self, root: NodeId) -> Vec<DocumentId> {
        let mut result = Vec::new();
        let mut pending = vec![root];
        while let Some(node) = pending.pop() {
            let Some(entry) = self.project.nodes.get(&node) else {
                continue;
            };
            pending.extend(entry.children.iter().copied());
            if let Some(document) = entry.kind.document_id() {
                result.push(document);
            }
        }
        result
    }
    fn initialize_new_document(
        &mut self,
        node: NodeId,
        source: Option<NodeId>,
    ) -> Result<(), StorageError> {
        let id = self
            .project
            .nodes
            .get(&node)
            .and_then(|entry| entry.kind.document_id())
            .ok_or(StorageError::InvalidSchema("created node lacks document"))?;
        let source_id = source.and_then(|source| {
            self.project
                .nodes
                .get(&source)
                .and_then(|entry| entry.kind.document_id())
        });
        let body = source_id
            .and_then(|source| self.bodies.get(&source).cloned())
            .unwrap_or_else(|| DocumentBodySnapshot::loaded(id, Arc::from("")));
        let extra = source_id
            .and_then(|source| self.unknown_front_matter.get(&source).cloned())
            .unwrap_or_default();
        self.bodies.insert(id, body);
        self.unknown_front_matter.insert(id, extra);
        Ok(())
    }
    fn copy_subtree_bodies(&mut self, source: NodeId, copy: NodeId) -> Result<(), StorageError> {
        self.initialize_new_document(copy, Some(source))?;
        let source_children = self
            .project
            .nodes
            .get(&source)
            .ok_or(StorageError::InvalidSchema("duplicate source vanished"))?
            .children
            .clone();
        let copy_children = self
            .project
            .nodes
            .get(&copy)
            .ok_or(StorageError::InvalidSchema("duplicate copy vanished"))?
            .children
            .clone();
        if source_children.len() != copy_children.len() {
            return Err(StorageError::InvalidSchema(
                "duplicate subtree shape differs",
            ));
        }
        let mut pending = source_children
            .into_iter()
            .zip(copy_children)
            .collect::<Vec<_>>();
        while let Some((source_child, copy_child)) = pending.pop() {
            self.initialize_new_document(copy_child, Some(source_child))?;
            let source_grandchildren = self
                .project
                .nodes
                .get(&source_child)
                .ok_or(StorageError::InvalidSchema("duplicate source vanished"))?
                .children
                .clone();
            let copy_grandchildren = self
                .project
                .nodes
                .get(&copy_child)
                .ok_or(StorageError::InvalidSchema("duplicate copy vanished"))?
                .children
                .clone();
            if source_grandchildren.len() != copy_grandchildren.len() {
                return Err(StorageError::InvalidSchema(
                    "duplicate subtree shape differs",
                ));
            }
            pending.extend(source_grandchildren.into_iter().zip(copy_grandchildren));
        }
        Ok(())
    }
}

/// Creates, opens, validates, saves, and closes canonical project directories.
pub struct ProjectStorage;

impl ProjectStorage {
    /// Creates a new project directory and acknowledges it only after canonical replacement succeeds.
    pub fn create(
        root: impl AsRef<Path>,
        name: impl Into<String>,
    ) -> Result<OpenProject, StorageError> {
        let root = root.as_ref();
        fs::create_dir_all(root).map_err(StorageError::CreateDirectory)?;
        if fs::read_dir(root)
            .map_err(StorageError::ReadDirectory)?
            .next()
            .is_some()
        {
            return Err(StorageError::DestinationNotEmpty(root.to_owned()));
        }
        let canonical = fs::canonicalize(root).map_err(StorageError::CanonicalizeRoot)?;
        let mut opened = OpenProject {
            project: Project::new(name),
            root: canonical,
            mode: OpenMode::ReadWrite,
            _lock: Some(AdvisoryLock::acquire(root)?),
            bodies: BTreeMap::new(),
            unknown_front_matter: BTreeMap::new(),
            locations: BTreeMap::new(),
            pending_locations: BTreeMap::new(),
            manifest_extra: BTreeMap::new(),
            outline_extra: BTreeMap::new(),
            styles_extra: BTreeMap::new(),
            attachments: BTreeMap::new(),
            dirty: DirtySet {
                manifest: true,
                outline: true,
                styles: true,
                attachments: true,
                ..DirtySet::default()
            },
            last_save_metrics: SaveMetrics::default(),
        };
        Self::save(&mut opened)?;
        Ok(opened)
    }
    /// Opens and validates an existing project. Read-only access remains available while another writer holds the advisory lock.
    pub fn open(root: impl AsRef<Path>, mode: OpenMode) -> Result<OpenProject, StorageError> {
        let supplied = root.as_ref();
        let canonical = fs::canonicalize(supplied).map_err(StorageError::CanonicalizeRoot)?;
        let lock = (mode == OpenMode::ReadWrite)
            .then(|| AdvisoryLock::acquire(&canonical))
            .transpose()?;
        if mode == OpenMode::ReadWrite {
            recover_pending_transaction(&canonical)?;
        }
        let manifest: Manifest = parse_toml(&canonical.join("parchmint.toml"), "parchmint.toml")?;
        migrate_if_needed(&canonical, manifest.format_version)?;
        if manifest.format_version != FORMAT_VERSION {
            return Err(StorageError::UnsupportedFormat(manifest.format_version));
        }
        let outline: Outline = parse_toml(&canonical.join("outline.toml"), "outline.toml")?;
        let styles: Styles = parse_toml(&canonical.join("styles.toml"), "styles.toml")?;
        if outline.format_version != FORMAT_VERSION || styles.format_version != FORMAT_VERSION {
            return Err(StorageError::InconsistentVersion);
        }
        let nodes = outline
            .nodes
            .into_iter()
            .map(|node| {
                (
                    node.id,
                    Node {
                        id: node.id,
                        kind: node.kind,
                        parent: node.parent,
                        children: node.children,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let roots: [NodeId; 2] = outline.roots.try_into().map_err(|_| {
            StorageError::InvalidSchema("outline roots must contain manuscript then research")
        })?;
        let mut project = Project {
            id: manifest.project_id,
            name: manifest.name,
            roots,
            nodes,
            documents: BTreeMap::new(),
            styles: styles
                .definitions
                .into_iter()
                .map(|style| (style.id, style))
                .collect(),
            compile_presets: styles
                .compile_presets
                .into_iter()
                .map(|preset| (preset.id, preset))
                .collect(),
            trash: outline
                .trash
                .into_iter()
                .map(|entry| (entry.node_id, entry))
                .collect(),
        };
        project.ensure_required_builtin_styles();
        let attachments = load_attachment_catalog(&canonical)?;
        let mut bodies = BTreeMap::new();
        let mut extras = BTreeMap::new();
        let mut locations = BTreeMap::new();
        let document_paths = canonical_document_paths(&project)?;
        for (node_id, document_id, path) in document_paths {
            let disk = resolve_project_path(&canonical, &path)?;
            let (metadata, unknown) = parse_document_header(&disk, document_id)?;
            project.documents.insert(
                document_id,
                DocumentRecord {
                    id: document_id,
                    node_id,
                    path: path.clone(),
                    metadata,
                },
            );
            bodies.insert(
                document_id,
                DocumentBodySnapshot::deferred(document_id, disk),
            );
            extras.insert(document_id, unknown);
            locations.insert(document_id, path);
        }
        project.validate().map_err(StorageError::Domain)?;
        Ok(OpenProject {
            project,
            root: canonical,
            mode,
            _lock: lock,
            bodies,
            unknown_front_matter: extras,
            locations,
            pending_locations: BTreeMap::new(),
            manifest_extra: manifest.extra,
            outline_extra: outline.extra,
            styles_extra: styles.extra,
            attachments,
            dirty: DirtySet::default(),
            last_save_metrics: SaveMetrics::default(),
        })
    }
    /// Persists only canonical resources in the dirty set. The explicit full
    /// validator remains here for support/debug callers; normal structural
    /// commands use [`ProjectStorage::execute_command`] and command-local
    /// validation before entering this writer.
    pub fn save(opened: &mut OpenProject) -> Result<(), StorageError> {
        if opened.mode == OpenMode::ReadOnly {
            return Err(StorageError::ReadOnly);
        }
        opened.project.validate().map_err(StorageError::Domain)?;
        Self::save_dirty_with_fault(opened, None)
    }

    /// Applies, validates, and durably commits one command. A failed canonical
    /// transaction restores both disk and the in-memory project before the
    /// error is returned to the UI.
    pub fn execute_command(
        opened: &mut OpenProject,
        command: ProjectCommand,
    ) -> Result<CommandOutcome, StorageError> {
        Self::execute_command_with_fault(opened, command, None)
    }

    /// Failure-injectable form used by deterministic partial-save tests.
    pub fn execute_command_with_fault(
        opened: &mut OpenProject,
        command: ProjectCommand,
        fault: Option<TransactionFault>,
    ) -> Result<CommandOutcome, StorageError> {
        if opened.mode == OpenMode::ReadOnly {
            return Err(StorageError::ReadOnly);
        }
        let prior_dirty = opened.dirty.clone();
        let prior_pending_locations = opened.pending_locations.clone();
        let outcome = opened.execute(command)?;
        if let Err(error) = Self::save_dirty_with_fault(opened, fault) {
            opened.rollback_command_state(&outcome)?;
            opened.dirty = prior_dirty;
            opened.pending_locations = prior_pending_locations;
            return Err(error);
        }
        Ok(outcome)
    }

    /// Applies one command, freezes its small canonical write set, and advances
    /// owner-thread location bookkeeping without touching disk. The returned
    /// plan is safe to send to the serial persistence worker.
    pub fn schedule_command(
        opened: &mut OpenProject,
        command: ProjectCommand,
    ) -> Result<ScheduledProjectCommand, StorageError> {
        if opened.mode == OpenMode::ReadOnly {
            return Err(StorageError::ReadOnly);
        }
        if !opened.dirty.is_empty() {
            return Err(StorageError::InvalidSchema(
                "a prior dirty set must be scheduled before another command",
            ));
        }
        let outcome = opened.execute(command)?;
        let plan = match Self::prepare_dirty_plan(opened) {
            Ok(plan) => plan,
            Err(error) => {
                opened.rollback_command_state(&outcome)?;
                opened.dirty = DirtySet::default();
                opened.pending_locations.clear();
                return Err(error);
            }
        };
        let prior_locations = plan
            .location_updates
            .iter()
            .map(|(document, _)| (*document, opened.locations.get(document).cloned()))
            .collect();
        let rollback = ScheduledCommandRollback {
            outcome: outcome.clone(),
            prior_locations,
            prior_metrics: opened.last_save_metrics,
        };
        Self::acknowledge_scheduled_owner(opened, &plan);
        Ok(ScheduledProjectCommand {
            outcome,
            plan,
            rollback,
        })
    }

    /// Records worker success. Canonical location state was advanced when the
    /// plan was scheduled so this acknowledgement performs no graph work.
    pub fn acknowledge_scheduled_command(opened: &mut OpenProject, metrics: SaveMetrics) {
        opened.last_save_metrics = metrics;
    }

    /// Reverses an optimistically published command after its worker write set
    /// failed. Callers roll back queued commands in reverse submission order.
    pub fn rollback_scheduled_command(
        opened: &mut OpenProject,
        rollback: ScheduledCommandRollback,
    ) -> Result<(), StorageError> {
        opened.rollback_command_state(&rollback.outcome)?;
        for (document, prior) in rollback.prior_locations {
            match prior {
                Some(location) => {
                    opened.locations.insert(document, location.clone());
                    if let Some(record) = opened.project.documents.get_mut(&document) {
                        record.path = location;
                    }
                }
                None => {
                    opened.locations.remove(&document);
                }
            }
            opened.pending_locations.remove(&document);
        }
        opened.dirty = DirtySet::default();
        opened.last_save_metrics = rollback.prior_metrics;
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn prepare_dirty_plan(opened: &OpenProject) -> Result<ProjectSavePlan, StorageError> {
        let mut mutations = BTreeMap::<String, Option<PlannedContents>>::new();
        if opened.dirty.manifest {
            mutations.insert(
                "parchmint.toml".into(),
                Some(PlannedContents::Bytes(toml_bytes(&Manifest {
                    format_version: FORMAT_VERSION,
                    project_id: opened.project.id,
                    name: opened.project.name.clone(),
                    extra: opened.manifest_extra.clone(),
                })?)),
            );
        }
        if opened.dirty.outline {
            mutations.insert(
                "outline.toml".into(),
                Some(PlannedContents::Outline(Outline {
                    format_version: FORMAT_VERSION,
                    roots: opened.project.roots.to_vec(),
                    nodes: opened
                        .project
                        .nodes
                        .values()
                        .map(|node| NodeWire {
                            id: node.id,
                            kind: node.kind.clone(),
                            parent: node.parent,
                            children: node.children.clone(),
                        })
                        .collect(),
                    trash: opened.project.trash.values().cloned().collect(),
                    extra: opened.outline_extra.clone(),
                })),
            );
        }
        if opened.dirty.styles {
            mutations.insert(
                "styles.toml".into(),
                Some(PlannedContents::Bytes(toml_bytes(&Styles {
                    format_version: FORMAT_VERSION,
                    definitions: opened.project.styles.values().cloned().collect(),
                    compile_presets: opened.project.compile_presets.values().cloned().collect(),
                    extra: opened.styles_extra.clone(),
                })?)),
            );
        }
        if opened.dirty.attachments {
            mutations.insert(
                "assets.toml".into(),
                Some(PlannedContents::Bytes(toml_bytes(&AttachmentCatalog {
                    version: ASSET_CATALOG_VERSION,
                    attachments: opened.attachments.values().cloned().collect(),
                })?)),
            );
        }
        let mut location_updates = Vec::new();
        for id in &opened.dirty.documents {
            let record = opened
                .project
                .documents
                .get(id)
                .ok_or(StorageError::MissingBody(*id))?;
            let desired = opened
                .pending_locations
                .get(id)
                .or_else(|| opened.locations.get(id))
                .cloned()
                .ok_or(StorageError::MissingBody(*id))?;
            let body = opened
                .bodies
                .get(id)
                .ok_or(StorageError::MissingBody(*id))?
                .load()?;
            let unknown = opened
                .unknown_front_matter
                .get(id)
                .cloned()
                .unwrap_or_default();
            let bytes = serialize_document(*id, &record.metadata, &unknown, &body)?.into_bytes();
            mutations.insert(desired.as_str().into(), Some(PlannedContents::Bytes(bytes)));
            if let Some(previous) = opened.locations.get(id)
                && previous != &desired
            {
                mutations.insert(previous.as_str().into(), None);
            }
            location_updates.push((*id, desired));
        }
        for node in &opened.dirty.tombstones {
            let path = format!("trash/{node}.toml");
            mutations.insert(
                path,
                opened
                    .project
                    .trash
                    .get(node)
                    .map(toml_bytes)
                    .transpose()?
                    .map(PlannedContents::Bytes),
            );
        }
        Ok(ProjectSavePlan {
            root: opened.root.clone(),
            mutations,
            location_updates,
        })
    }

    fn acknowledge_scheduled_owner(opened: &mut OpenProject, plan: &ProjectSavePlan) {
        for (document, location) in &plan.location_updates {
            opened.locations.insert(*document, location.clone());
            opened.pending_locations.remove(document);
            if let Some(record) = opened.project.documents.get_mut(document) {
                record.path.clone_from(location);
            }
        }
        opened.dirty = DirtySet::default();
    }

    fn save_dirty_with_fault(
        opened: &mut OpenProject,
        fault: Option<TransactionFault>,
    ) -> Result<(), StorageError> {
        if opened.dirty.is_empty() {
            opened.last_save_metrics = SaveMetrics::default();
            return Ok(());
        }
        let plan = Self::prepare_dirty_plan(opened)?;
        let location_updates = plan.location_updates.clone();
        let metrics = plan.execute_with_fault(fault)?;
        for (document, location) in location_updates {
            opened.locations.insert(document, location.clone());
            opened.pending_locations.remove(&document);
            if let Some(record) = opened.project.documents.get_mut(&document) {
                record.path = location;
            }
        }
        opened.dirty = DirtySet::default();
        opened.last_save_metrics = metrics;
        Ok(())
    }
    /// Atomically persists one canonical Markdown document without rewriting
    /// unrelated documents or project manifests. Editor autosave uses this to
    /// avoid overwriting an external change in another document.
    pub fn save_document(opened: &mut OpenProject, id: DocumentId) -> Result<(), StorageError> {
        if opened.mode == OpenMode::ReadOnly {
            return Err(StorageError::ReadOnly);
        }
        let record = opened
            .project
            .documents
            .get(&id)
            .cloned()
            .ok_or(StorageError::MissingBody(id))?;
        let desired = opened
            .pending_locations
            .get(&id)
            .or_else(|| opened.locations.get(&id))
            .cloned()
            .ok_or(StorageError::MissingBody(id))?;
        let body = opened
            .bodies
            .get(&record.id)
            .ok_or(StorageError::MissingBody(record.id))?
            .load()?;
        let unknown = opened
            .unknown_front_matter
            .get(&record.id)
            .cloned()
            .unwrap_or_default();
        let bytes = serialize_document(record.id, &record.metadata, &unknown, &body)?;
        let mut mutations =
            BTreeMap::from([(desired.as_str().to_owned(), Some(bytes.into_bytes()))]);
        if let Some(previous) = opened.locations.get(&record.id)
            && previous != &desired
        {
            mutations.insert(previous.as_str().to_owned(), None);
        }
        opened.last_save_metrics = transactional_write_set(&opened.root, &mutations, None)?;
        opened.locations.insert(record.id, desired.clone());
        opened.pending_locations.remove(&record.id);
        if let Some(record) = opened.project.documents.get_mut(&record.id) {
            record.path = desired;
        }
        opened.dirty.documents.remove(&id);
        Ok(())
    }

    /// Freezes one complete document replacement without performing file I/O.
    /// The returned plan can be sent to the serial project persistence worker.
    pub fn prepare_document_save(
        opened: &OpenProject,
        id: DocumentId,
        body: String,
    ) -> Result<DocumentSavePlan, StorageError> {
        if opened.mode == OpenMode::ReadOnly {
            return Err(StorageError::ReadOnly);
        }
        if body.len() as u64 > MAX_DOCUMENT_BYTES {
            return Err(StorageError::SizeLimit("document", MAX_DOCUMENT_BYTES));
        }
        let record = opened
            .project
            .documents
            .get(&id)
            .ok_or(StorageError::MissingBody(id))?;
        let desired = opened
            .pending_locations
            .get(&id)
            .or_else(|| opened.locations.get(&id))
            .cloned()
            .ok_or(StorageError::MissingBody(id))?;
        let unknown = opened
            .unknown_front_matter
            .get(&id)
            .cloned()
            .unwrap_or_default();
        let canonical_bytes =
            serialize_document(id, &record.metadata, &unknown, &body)?.into_bytes();
        let canonical_path = resolve_project_path(&opened.root, &desired)?;
        Ok(DocumentSavePlan {
            document_id: id,
            body,
            canonical_path,
            canonical_bytes,
        })
    }

    /// Updates the owner-thread cache after a matching worker replacement has
    /// completed. This method performs no disk mutation.
    pub fn acknowledge_document_save(
        opened: &mut OpenProject,
        plan: &DocumentSavePlan,
    ) -> Result<(), StorageError> {
        opened.set_body(plan.document_id, plan.body.clone())?;
        opened.dirty.documents.remove(&plan.document_id);
        Ok(())
    }
    /// Saves and releases any advisory writer lock.
    pub fn close(mut opened: OpenProject) -> Result<(), StorageError> {
        Self::save(&mut opened)
    }

    /// Imports an ordinary file into `assets/` under a UUID-derived safe name.
    /// Source symlinks, device files, excessive files, and unsafe catalog
    /// entries are rejected. Deduplication is deliberately not attempted:
    /// equal bytes can still be distinct research material to a writer.
    pub fn import_attachment(
        opened: &mut OpenProject,
        source: impl AsRef<Path>,
    ) -> Result<AttachmentRecord, StorageError> {
        if opened.mode == OpenMode::ReadOnly {
            return Err(StorageError::ReadOnly);
        }
        let source = source.as_ref();
        let metadata = fs::symlink_metadata(source).map_err(|error| StorageError::Read {
            path: source.to_owned(),
            error,
        })?;
        if metadata.file_type().is_symlink() {
            return Err(StorageError::AttachmentSourceSymlink(source.to_owned()));
        }
        if !metadata.is_file() {
            return Err(StorageError::AttachmentSourceNotFile(source.to_owned()));
        }
        if metadata.len() > MAX_ATTACHMENT_BYTES {
            return Err(StorageError::SizeLimit("attachment", MAX_ATTACHMENT_BYTES));
        }
        let display_name = source
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .ok_or_else(|| StorageError::InvalidAttachmentName(source.to_owned()))?
            .to_owned();
        let id = parchmint_domain::AssetId::new();
        let extension = safe_extension(&display_name);
        let safe_name = format!(
            "{id}{}",
            extension
                .as_deref()
                .map_or(String::new(), |ext| format!(".{ext}"))
        );
        let relative = RelativeProjectPath::new(format!("assets/{safe_name}"))?;
        let destination = resolve_project_path(&opened.root, &relative)?;
        let parent = destination
            .parent()
            .ok_or(StorageError::InvalidSchema("assets parent"))?;
        fs::create_dir_all(parent).map_err(StorageError::CreateDirectory)?;
        // Resolve after creating the directory so a malicious pre-existing
        // `assets` symlink cannot redirect the copy.
        let destination = resolve_project_path(&opened.root, &relative)?;
        if destination.exists() {
            return Err(StorageError::AttachmentDestinationExists(destination));
        }
        let mut input = File::open(source).map_err(|error| StorageError::Read {
            path: source.to_owned(),
            error,
        })?;
        let mut temporary =
            NamedTempFile::new_in(parent).map_err(AtomicWriteError::CreateTemporary)?;
        let copied = io::copy(
            &mut Read::by_ref(&mut input).take(MAX_ATTACHMENT_BYTES + 1),
            temporary.as_file_mut(),
        )
        .map_err(StorageError::CopyAttachment)?;
        if copied > MAX_ATTACHMENT_BYTES {
            return Err(StorageError::SizeLimit("attachment", MAX_ATTACHMENT_BYTES));
        }
        temporary
            .as_file_mut()
            .sync_all()
            .map_err(AtomicWriteError::FlushTemporary)?;
        temporary
            .persist_noclobber(&destination)
            .map_err(|error| StorageError::AttachmentPersist(error.error))?;
        sync_parent(parent)?;
        let record = AttachmentRecord {
            id,
            display_name,
            safe_name,
            media_type: media_type_for(extension.as_deref()),
            bytes: copied,
        };
        opened.attachments.insert(id, record.clone());
        if let Err(error) = save_attachment_catalog(&opened.root, &opened.attachments) {
            opened.attachments.remove(&id);
            return Err(error);
        }
        Ok(record)
    }

    /// Returns a contained local path and a passive preview policy. Callers
    /// must require a separate, explicit system-open action for ExternalOnly.
    pub fn attachment_preview(
        opened: &OpenProject,
        id: parchmint_domain::AssetId,
    ) -> Result<(PathBuf, AttachmentPreview), StorageError> {
        let record = opened
            .attachments
            .get(&id)
            .ok_or(StorageError::MissingAttachment(id))?;
        let path = resolve_project_path(
            &opened.root,
            &RelativeProjectPath::new(format!("assets/{}", record.safe_name))?,
        )?;
        let preview = match record.media_type.as_str() {
            "image" => AttachmentPreview::Image,
            "pdf" => AttachmentPreview::Pdf,
            "text" => AttachmentPreview::PlainText,
            _ => AttachmentPreview::ExternalOnly,
        };
        Ok((path, preview))
    }
    /// Reopens an acknowledged project directory, useful after close/restart tests.
    pub fn reopen(root: impl AsRef<Path>, mode: OpenMode) -> Result<OpenProject, StorageError> {
        Self::open(root, mode)
    }
    /// Performs bounded, read-only validation suitable for CI or support diagnostics.
    pub fn validate(root: impl AsRef<Path>) -> Result<ValidationReport, StorageError> {
        let opened = Self::open(root, OpenMode::ReadOnly)?;
        Ok(ValidationReport {
            root: opened.root.clone(),
            format_version: FORMAT_VERSION,
            documents: opened.project.documents.len(),
            warnings: Vec::new(),
        })
    }
}

#[derive(Deserialize, Serialize)]
struct Manifest {
    format_version: u32,
    project_id: ProjectId,
    name: String,
    #[serde(flatten, default)]
    extra: BTreeMap<String, toml::Value>,
}
#[derive(Deserialize, Serialize)]
struct Outline {
    format_version: u32,
    roots: Vec<NodeId>,
    nodes: Vec<NodeWire>,
    #[serde(default)]
    trash: Vec<TrashTombstone>,
    #[serde(flatten, default)]
    extra: BTreeMap<String, toml::Value>,
}
#[derive(Deserialize, Serialize)]
struct NodeWire {
    id: NodeId,
    #[serde(flatten)]
    kind: NodeKind,
    parent: Option<NodeId>,
    #[serde(default)]
    children: Vec<NodeId>,
}
#[derive(Deserialize, Serialize)]
struct Styles {
    format_version: u32,
    #[serde(rename = "styles")]
    definitions: Vec<StyleDefinition>,
    #[serde(default)]
    compile_presets: Vec<parchmint_domain::CompilePreset>,
    #[serde(flatten, default)]
    extra: BTreeMap<String, toml::Value>,
}

#[derive(Deserialize, Serialize)]
struct AttachmentCatalog {
    version: u32,
    #[serde(default)]
    attachments: Vec<AttachmentRecord>,
}

fn load_attachment_catalog(
    root: &Path,
) -> Result<BTreeMap<parchmint_domain::AssetId, AttachmentRecord>, StorageError> {
    let path = root.join("assets.toml");
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let catalog: AttachmentCatalog = parse_toml(&path, "assets.toml")?;
    if catalog.version != ASSET_CATALOG_VERSION {
        return Err(StorageError::UnsupportedAssetCatalog(catalog.version));
    }
    let mut items = BTreeMap::new();
    for item in catalog.attachments {
        validate_attachment_record(root, &item)?;
        if items.insert(item.id, item).is_some() {
            return Err(StorageError::InvalidSchema("duplicate attachment ID"));
        }
    }
    Ok(items)
}

fn save_attachment_catalog(
    root: &Path,
    attachments: &BTreeMap<parchmint_domain::AssetId, AttachmentRecord>,
) -> Result<(), StorageError> {
    write_toml(
        &root.join("assets.toml"),
        &AttachmentCatalog {
            version: ASSET_CATALOG_VERSION,
            attachments: attachments.values().cloned().collect(),
        },
    )
}

fn validate_attachment_record(root: &Path, record: &AttachmentRecord) -> Result<(), StorageError> {
    if record.display_name.is_empty()
        || record.display_name.len() > 255
        || record.display_name.chars().any(char::is_control)
        || record.safe_name.contains('/')
        || record.safe_name.contains('\\')
        || !record.safe_name.starts_with(&record.id.to_string())
        || record.safe_name.len() > 300
        || record.bytes > MAX_ATTACHMENT_BYTES
    {
        return Err(StorageError::InvalidSchema(
            "invalid attachment catalog entry",
        ));
    }
    let path = resolve_project_path(
        root,
        &RelativeProjectPath::new(format!("assets/{}", record.safe_name))?,
    )?;
    let metadata = fs::metadata(&path).map_err(|error| StorageError::Read { path, error })?;
    if !metadata.is_file() || metadata.len() != record.bytes {
        return Err(StorageError::InvalidSchema(
            "attachment file does not match catalog",
        ));
    }
    Ok(())
}

fn safe_extension(name: &str) -> Option<String> {
    let extension = Path::new(name).extension()?.to_str()?.to_ascii_lowercase();
    (!extension.is_empty()
        && extension.len() <= 16
        && extension.bytes().all(|byte| byte.is_ascii_alphanumeric()))
    .then_some(extension)
}

fn media_type_for(extension: Option<&str>) -> String {
    match extension.unwrap_or_default() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" => "image",
        "pdf" => "pdf",
        "txt" | "md" | "csv" | "json" | "log" => "text",
        _ => "external",
    }
    .to_owned()
}

fn parse_toml<T: for<'de> Deserialize<'de>>(
    path: &Path,
    name: &'static str,
) -> Result<T, StorageError> {
    let source = read_bounded(path, MAX_MANIFEST_BYTES, "manifest")?;
    toml::from_str(&source).map_err(|error| StorageError::Toml {
        name,
        message: error.to_string(),
    })
}
fn write_toml<T: Serialize>(path: &Path, value: &T) -> Result<(), StorageError> {
    atomic_write(path, &toml_bytes(value)?)?;
    Ok(())
}

fn toml_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, StorageError> {
    toml::to_string_pretty(value)
        .map(String::into_bytes)
        .map_err(|error| StorageError::SerializeToml(error.to_string()))
}

const PENDING_TRANSACTION: &str = ".parchmint/pending-save-v1";

#[derive(Deserialize, Serialize)]
struct TransactionRecord {
    version: u32,
    entries: Vec<TransactionEntry>,
}

#[derive(Deserialize, Serialize)]
struct TransactionEntry {
    path: String,
    existed: bool,
    backup: Option<String>,
}

/// Commits a small deterministic set of canonical file replacements/deletions.
/// A durable old-byte record is published before the first canonical mutation;
/// open-time recovery rolls it back unless the whole operation was acknowledged.
fn transactional_write_set(
    root: &Path,
    mutations: &BTreeMap<String, Option<Vec<u8>>>,
    fault: Option<TransactionFault>,
) -> Result<SaveMetrics, StorageError> {
    if mutations.is_empty() {
        return Ok(SaveMetrics::default());
    }
    recover_pending_transaction(root)?;
    let state_root = root.join(".parchmint");
    fs::create_dir_all(&state_root).map_err(StorageError::CreateDirectory)?;
    let staging = state_root.join(format!("save-stage-{}", Uuid::new_v4()));
    fs::create_dir(&staging).map_err(StorageError::CreateDirectory)?;
    let mut entries = Vec::with_capacity(mutations.len());
    for (index, relative) in mutations.keys().enumerate() {
        let relative_path = RelativeProjectPath::new(relative.clone())?;
        let target = resolve_project_path(root, &relative_path)?;
        let existed = target.is_file();
        let backup = existed.then(|| format!("{index:06}.bak"));
        if let Some(backup) = &backup {
            let bytes = fs::read(&target).map_err(|error| StorageError::Read {
                path: target.clone(),
                error,
            })?;
            atomic_write(&staging.join(backup), &bytes)?;
        }
        entries.push(TransactionEntry {
            path: relative.clone(),
            existed,
            backup,
        });
    }
    write_toml(
        &staging.join("transaction.toml"),
        &TransactionRecord {
            version: 1,
            entries,
        },
    )?;
    sync_parent(&staging)?;
    let pending = root.join(PENDING_TRANSACTION);
    fs::rename(&staging, &pending).map_err(StorageError::PublishTransaction)?;
    sync_parent(&state_root)?;

    let mut metrics = SaveMetrics::default();
    let commit = (|| {
        for (relative, contents) in mutations {
            let target = resolve_project_path(root, &RelativeProjectPath::new(relative.clone())?)?;
            match contents {
                Some(contents) => {
                    atomic_write(&target, contents)?;
                    metrics.files_written = metrics.files_written.saturating_add(1);
                    metrics.bytes_written = metrics
                        .bytes_written
                        .saturating_add(u64::try_from(contents.len()).unwrap_or(u64::MAX));
                }
                None if target.is_file() => {
                    fs::remove_file(&target).map_err(StorageError::RemoveOldDocument)?;
                    if let Some(parent) = target.parent() {
                        sync_parent(parent)?;
                    }
                    metrics.files_removed = metrics.files_removed.saturating_add(1);
                }
                None => {}
            }
            let completed = metrics.files_written.saturating_add(metrics.files_removed);
            if fault == Some(TransactionFault::AfterMutation(completed)) {
                return Err(StorageError::InjectedTransactionFault(completed));
            }
        }
        Ok(())
    })();
    if let Err(error) = commit {
        recover_pending_transaction(root)?;
        return Err(error);
    }
    fs::remove_dir_all(&pending).map_err(StorageError::RemoveTransaction)?;
    sync_parent(&state_root)?;
    Ok(metrics)
}

fn recover_pending_transaction(root: &Path) -> Result<(), StorageError> {
    let pending = root.join(PENDING_TRANSACTION);
    if !pending.is_dir() {
        return Ok(());
    }
    let record: TransactionRecord = parse_toml(
        &pending.join("transaction.toml"),
        "pending transaction record",
    )?;
    if record.version != 1 {
        return Err(StorageError::InvalidSchema(
            "unsupported pending transaction version",
        ));
    }
    for entry in record.entries {
        let target = resolve_project_path(root, &RelativeProjectPath::new(entry.path)?)?;
        if entry.existed {
            let backup = entry.backup.ok_or(StorageError::InvalidSchema(
                "pending transaction backup is missing",
            ))?;
            let backup_path = pending.join(backup);
            let bytes = fs::read(&backup_path).map_err(|error| StorageError::Read {
                path: backup_path,
                error,
            })?;
            atomic_write(&target, &bytes)?;
        } else if target.is_file() {
            fs::remove_file(&target).map_err(StorageError::RemoveOldDocument)?;
            if let Some(parent) = target.parent() {
                sync_parent(parent)?;
            }
        }
    }
    fs::remove_dir_all(&pending).map_err(StorageError::RemoveTransaction)?;
    if let Some(parent) = pending.parent() {
        sync_parent(parent)?;
    }
    Ok(())
}

/// Reads only the bounded YAML prefix needed to make the binder usable. The
/// Markdown body remains behind `DocumentBodySnapshot` until an editor,
/// compile, or index worker requests that document.
fn parse_document_header(
    path: &Path,
    document_id: DocumentId,
) -> Result<(DocumentMetadata, Mapping), StorageError> {
    let metadata = fs::metadata(path).map_err(|error| StorageError::Read {
        path: path.to_owned(),
        error,
    })?;
    if metadata.len() > MAX_DOCUMENT_BYTES {
        return Err(StorageError::SizeLimit("document", MAX_DOCUMENT_BYTES));
    }
    let file = File::open(path).map_err(|error| StorageError::Read {
        path: path.to_owned(),
        error,
    })?;
    let mut reader = BufReader::new(file);
    let mut header = String::new();
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|error| StorageError::Read {
            path: path.to_owned(),
            error,
        })?;
    if line != "---\n" {
        return Err(StorageError::InvalidSchema(
            "document lacks YAML front matter",
        ));
    }
    header.push_str(&line);
    loop {
        line.clear();
        if reader
            .read_line(&mut line)
            .map_err(|error| StorageError::Read {
                path: path.to_owned(),
                error,
            })?
            == 0
        {
            return Err(StorageError::InvalidSchema(
                "document front matter is unclosed",
            ));
        }
        header.push_str(&line);
        if header.len() > MAX_FRONT_MATTER_BYTES {
            return Err(StorageError::SizeLimit(
                "front matter",
                MAX_FRONT_MATTER_BYTES as u64,
            ));
        }
        if line == "---\n" {
            break;
        }
    }
    let (metadata, _, unknown) = parse_document(&header, document_id)?;
    Ok((metadata, unknown))
}

fn parse_document(
    source: &str,
    document_id: DocumentId,
) -> Result<(DocumentMetadata, String, Mapping), StorageError> {
    if !source.starts_with("---\n") {
        return Err(StorageError::InvalidSchema(
            "document lacks YAML front matter",
        ));
    }
    let end = source[4..]
        .find("\n---\n")
        .ok_or(StorageError::InvalidSchema(
            "document front matter is unclosed",
        ))?
        + 4;
    if end > MAX_FRONT_MATTER_BYTES {
        return Err(StorageError::SizeLimit(
            "front matter",
            MAX_FRONT_MATTER_BYTES as u64,
        ));
    }
    let raw = &source[4..end];
    let mut mapping = yaml::from_str::<Value>(raw)
        .map_err(|error| StorageError::Yaml(error.to_string()))?
        .as_mapping()
        .cloned()
        .ok_or(StorageError::InvalidSchema(
            "front matter root must be a mapping",
        ))?;
    check_yaml_depth(&Value::Mapping(mapping.clone()), 0)?;
    let stored_id = mapping
        .remove("document_id")
        .ok_or(StorageError::InvalidSchema(
            "document_id is missing from front matter",
        ))?;
    if stored_id
        .as_str()
        .map(DocumentId::parse)
        .transpose()
        .map_err(|_| StorageError::InvalidSchema("document_id is invalid"))?
        != Some(document_id)
    {
        return Err(StorageError::InvalidSchema(
            "document_id disagrees with outline",
        ));
    }
    let known = [
        "title",
        "summary",
        "status",
        "labels",
        "tags",
        "flags",
        "attachment",
    ];
    let mut metadata_map = Mapping::new();
    for key in known {
        if let Some(value) = mapping.remove(key) {
            metadata_map.insert(key, value);
        }
    }
    let metadata = yaml::from_value::<DocumentMetadata>(Value::Mapping(metadata_map))
        .map_err(|error| StorageError::Yaml(error.to_string()))?;
    Ok((metadata, source[end + 5..].to_owned(), mapping))
}
fn serialize_document(
    id: DocumentId,
    metadata: &DocumentMetadata,
    unknown: &Mapping,
    body: &str,
) -> Result<String, StorageError> {
    let mut entries = BTreeMap::<String, Value>::new();
    entries.insert("document_id".into(), Value::String(id.to_string()));
    entries.insert("title".into(), Value::String(metadata.title.clone()));
    if !metadata.summary.is_empty() {
        entries.insert("summary".into(), Value::String(metadata.summary.clone()));
    }
    if let Some(status) = &metadata.status {
        entries.insert("status".into(), Value::String(status.clone()));
    }
    if !metadata.labels.is_empty() {
        entries.insert(
            "labels".into(),
            yaml::to_value(&metadata.labels).map_err(|e| StorageError::Yaml(e.to_string()))?,
        );
    }
    if !metadata.tags.is_empty() {
        entries.insert(
            "tags".into(),
            yaml::to_value(&metadata.tags).map_err(|e| StorageError::Yaml(e.to_string()))?,
        );
    }
    if !metadata.flags.is_empty() {
        entries.insert(
            "flags".into(),
            yaml::to_value(&metadata.flags).map_err(|e| StorageError::Yaml(e.to_string()))?,
        );
    }
    if let Some(attachment) = metadata.attachment {
        entries.insert("attachment".into(), Value::String(attachment.to_string()));
    }
    for (key, value) in unknown {
        if entries.contains_key(key) || key == "document_id" {
            continue;
        }
        entries.insert(key.to_owned(), value.clone());
    }
    let yaml = yaml::to_string(&entries).map_err(|error| StorageError::Yaml(error.to_string()))?;
    Ok(format!("---\n{}\n---\n{body}", yaml.trim_end()))
}
fn check_yaml_depth(value: &Value, depth: usize) -> Result<(), StorageError> {
    if depth > MAX_YAML_NESTING {
        return Err(StorageError::InvalidSchema(
            "front matter nesting exceeds 64",
        ));
    }
    match value {
        Value::Sequence(values) => {
            for value in values {
                check_yaml_depth(value, depth + 1)?;
            }
        }
        Value::Mapping(values) => {
            for value in values.values() {
                check_yaml_depth(value, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}
fn read_bounded(path: &Path, limit: u64, kind: &'static str) -> Result<String, StorageError> {
    let metadata = fs::metadata(path).map_err(|error| StorageError::Read {
        path: path.to_owned(),
        error,
    })?;
    if metadata.len() > limit {
        return Err(StorageError::SizeLimit(kind, limit));
    }
    fs::read_to_string(path).map_err(|error| StorageError::Read {
        path: path.to_owned(),
        error,
    })
}

/// Reads and validates a canonical Markdown file at a frozen path. Lifecycle
/// workers use this for fingerprint checks without borrowing `OpenProject`.
pub fn read_document_body_at(path: &Path, document_id: DocumentId) -> Result<String, StorageError> {
    let source = read_bounded(path, MAX_DOCUMENT_BYTES, "document")?;
    let (_, body, _) = parse_document(&source, document_id)?;
    Ok(body)
}

/// Reads a backup source with the same hard bound as canonical documents.
pub fn read_document_bytes_bounded(path: &Path) -> Result<Vec<u8>, StorageError> {
    let metadata = fs::metadata(path).map_err(|error| StorageError::Read {
        path: path.to_owned(),
        error,
    })?;
    if metadata.len() > MAX_DOCUMENT_BYTES {
        return Err(StorageError::SizeLimit("document", MAX_DOCUMENT_BYTES));
    }
    fs::read(path).map_err(|error| StorageError::Read {
        path: path.to_owned(),
        error,
    })
}

fn canonical_document_paths(
    project: &Project,
) -> Result<Vec<(NodeId, DocumentId, RelativeProjectPath)>, StorageError> {
    let mut result = Vec::with_capacity(project.documents.len());
    let mut visited = BTreeSet::new();
    for (root, folder) in [
        (project.manuscript_root(), "manuscript"),
        (project.research_root(), "research"),
    ] {
        let mut pending = vec![root];
        while let Some(node) = pending.pop() {
            if !visited.insert(node) {
                return Err(StorageError::InvalidSchema("node graph contains a cycle"));
            }
            let entry = project.nodes.get(&node).ok_or(StorageError::InvalidSchema(
                "outline references a missing node",
            ))?;
            pending.extend(entry.children.iter().rev().copied());
            if let Some(document) = entry.kind.document_id() {
                result.push((
                    node,
                    document,
                    RelativeProjectPath::new(format!("{folder}/{node}.md"))?,
                ));
            }
        }
    }
    let mut pending = project.trash.keys().copied().collect::<Vec<_>>();
    while let Some(node) = pending.pop() {
        if !visited.insert(node) {
            return Err(StorageError::InvalidSchema(
                "active and trash graphs overlap or contain a cycle",
            ));
        }
        let entry = project.nodes.get(&node).ok_or(StorageError::InvalidSchema(
            "trash references a missing node",
        ))?;
        pending.extend(entry.children.iter().rev().copied());
        if let Some(document) = entry.kind.document_id() {
            result.push((
                node,
                document,
                RelativeProjectPath::new(format!("trash/{node}.md"))?,
            ));
        }
    }
    if visited.len() != project.nodes.len() {
        return Err(StorageError::InvalidSchema(
            "outline contains unreachable nodes",
        ));
    }
    Ok(result)
}
fn root_folder(project: &Project, node: NodeId) -> Result<&'static str, StorageError> {
    let mut current = node;
    let mut seen = BTreeSet::new();
    loop {
        if !seen.insert(current) {
            return Err(StorageError::InvalidSchema("node parent cycle"));
        }
        if current == project.manuscript_root() {
            return Ok("manuscript");
        }
        if current == project.research_root() {
            return Ok("research");
        }
        current = project
            .nodes
            .get(&current)
            .and_then(|entry| entry.parent)
            .ok_or(StorageError::InvalidSchema("node does not reach a root"))?;
    }
}

/// Resolves one validated relative path below `root`, rejecting a symlink at any existing component.
pub fn resolve_project_path(
    root: &Path,
    relative: &RelativeProjectPath,
) -> Result<PathBuf, StorageError> {
    let root = fs::canonicalize(root).map_err(StorageError::CanonicalizeRoot)?;
    let mut target = root.clone();
    for component in Path::new(relative.as_str()).components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(StorageError::PathEscape(relative.as_str().into()));
        }
        target.push(component);
        if let Ok(metadata) = fs::symlink_metadata(&target) {
            if metadata.file_type().is_symlink() {
                return Err(StorageError::SymlinkEscape(target));
            }
            if let Ok(canonical) = fs::canonicalize(&target)
                && !canonical.starts_with(&root)
            {
                return Err(StorageError::PathEscape(relative.as_str().into()));
            }
        }
    }
    Ok(target)
}

fn migrate_if_needed(root: &Path, version: u32) -> Result<(), StorageError> {
    if version == FORMAT_VERSION {
        return Ok(());
    }
    if version > FORMAT_VERSION {
        return Err(StorageError::UnsupportedFormat(version));
    }
    backup_before_migration(root, version)?;
    Err(StorageError::MigrationUnavailable(version))
}
fn backup_before_migration(root: &Path, version: u32) -> Result<(), StorageError> {
    let backup = root
        .join(".parchmint")
        .join("backups")
        .join(format!("pre-migration-v{version}"));
    if backup.exists() {
        return Ok(());
    }
    copy_canonical_tree(root, &backup, root)?;
    Ok(())
}
fn copy_canonical_tree(current: &Path, backup: &Path, root: &Path) -> Result<(), StorageError> {
    fs::create_dir_all(backup).map_err(StorageError::CreateDirectory)?;
    for entry in fs::read_dir(current).map_err(StorageError::ReadDirectory)? {
        let entry = entry.map_err(StorageError::ReadDirectory)?;
        let source = entry.path();
        if source == root.join(".parchmint") {
            continue;
        }
        let destination = backup.join(entry.file_name());
        let metadata = entry.metadata().map_err(StorageError::ReadDirectory)?;
        if metadata.is_dir() {
            copy_canonical_tree(&source, &destination, root)?;
        } else if metadata.is_file() {
            fs::copy(source, destination).map_err(StorageError::CopyBackup)?;
        }
    }
    Ok(())
}

struct AdvisoryLock {
    file: File,
}
impl AdvisoryLock {
    fn acquire(root: &Path) -> Result<Self, StorageError> {
        let path = root.join(".parchmint").join("open.lock");
        fs::create_dir_all(
            path.parent()
                .ok_or(StorageError::InvalidSchema("lock parent"))?,
        )
        .map_err(StorageError::CreateDirectory)?;
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(StorageError::Lock)?;
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(StorageError::ProjectLocked(path)),
            Err(TryLockError::Error(error)) => return Err(StorageError::Lock(error)),
        }

        // This is diagnostic context only. The open file handle and OS lock
        // above are the sole lock authority, so stale metadata after a crash is
        // harmless and is replaced by the next successful owner.
        let hostname = std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("COMPUTERNAME"))
            .unwrap_or_else(|_| "unknown".into());
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        file.set_len(0).map_err(StorageError::Lock)?;
        writeln!(
            file,
            "pid = {}\nhostname = {:?}\nacquired_unix_seconds = {timestamp}",
            std::process::id(),
            hostname
        )
        .map_err(StorageError::Lock)?;
        file.sync_all().map_err(StorageError::Lock)?;
        Ok(Self { file })
    }
}

impl Drop for AdvisoryLock {
    fn drop(&mut self) {
        // Closing the handle also releases the OS lock. Unlock explicitly so
        // same-process close/reopen transitions are deterministic on every
        // supported primitive and do not depend on descriptor-close timing.
        let _ = self.file.unlock();
    }
}

/// A fully written and flushed same-directory temporary file awaiting its
/// short atomic replacement step.
pub struct PreparedAtomicWrite {
    destination: PathBuf,
    temporary: NamedTempFile,
}

impl PreparedAtomicWrite {
    /// Atomically installs the prepared bytes and flushes directory metadata.
    pub fn commit(self) -> Result<(), AtomicWriteError> {
        let parent = self
            .destination
            .parent()
            .expect("prepared atomic destinations always have a parent")
            .to_owned();
        self.temporary
            .persist(&self.destination)
            .map_err(|error| AtomicWriteError::Replace(error.error))?;
        sync_parent(&parent)
    }
}

/// Writes and flushes `contents` beside `destination` without replacing the
/// destination. Dropping the result leaves canonical data untouched.
pub fn prepare_atomic_write(
    destination: &Path,
    contents: &[u8],
) -> Result<PreparedAtomicWrite, AtomicWriteError> {
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .ok_or_else(|| AtomicWriteError::MissingParent(destination.to_owned()))?;
    fs::create_dir_all(parent).map_err(AtomicWriteError::PrepareDirectory)?;
    let mut temporary = NamedTempFile::new_in(parent).map_err(AtomicWriteError::CreateTemporary)?;
    temporary
        .write_all(contents)
        .map_err(AtomicWriteError::WriteTemporary)?;
    temporary
        .as_file_mut()
        .sync_all()
        .map_err(AtomicWriteError::FlushTemporary)?;
    Ok(PreparedAtomicWrite {
        destination: destination.to_owned(),
        temporary,
    })
}

/// Writes `contents` beside `destination`, flushes it, atomically replaces it, and flushes directory metadata on Unix.
pub fn atomic_write(destination: &Path, contents: &[u8]) -> Result<(), AtomicWriteError> {
    prepare_atomic_write(destination, contents)?.commit()
}
#[cfg(unix)]
fn sync_parent(parent: &Path) -> Result<(), AtomicWriteError> {
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(AtomicWriteError::FlushDirectory)
}
#[cfg(not(unix))]
fn sync_parent(_parent: &Path) -> Result<(), AtomicWriteError> {
    Ok(())
}

/// Storage failure with a phase/action suitable for the UI and diagnostics.
#[derive(Debug, Error)]
pub enum StorageError {
    /// Could not create a directory.
    #[error("could not create project directory: {0}")]
    CreateDirectory(io::Error),
    /// Could not inspect a directory.
    #[error("could not read project directory: {0}")]
    ReadDirectory(io::Error),
    /// New-project destination was not empty.
    #[error("new project directory is not empty: {0}")]
    DestinationNotEmpty(PathBuf),
    /// Root could not be canonicalized.
    #[error("could not canonicalize project root: {0}")]
    CanonicalizeRoot(io::Error),
    /// Read failed.
    #[error("could not read {path}: {error}")]
    Read { path: PathBuf, error: io::Error },
    /// Size bound exceeded.
    #[error("{0} exceeds configured size limit of {1} bytes")]
    SizeLimit(&'static str, u64),
    /// TOML syntax/schema failure.
    #[error("invalid {name}: {message}")]
    Toml { name: &'static str, message: String },
    /// YAML syntax/schema failure.
    #[error("invalid YAML front matter: {0}")]
    Yaml(String),
    /// Schema semantic failure.
    #[error("invalid project schema: {0}")]
    InvalidSchema(&'static str),
    /// Domain invariant failure.
    #[error(transparent)]
    Domain(#[from] ProjectError),
    /// Format is newer than this executable.
    #[error("project format version {0} is unsupported; upgrade ParchMint")]
    UnsupportedFormat(u32),
    /// Canonical files disagree on their format version.
    #[error("canonical manifest versions disagree")]
    InconsistentVersion,
    /// Asset catalog is newer than this executable.
    #[error("asset catalog version {0} is unsupported; upgrade ParchMint")]
    UnsupportedAssetCatalog(u32),
    /// No older migration exists yet.
    #[error("no migration from project format version {0} is available")]
    MigrationUnavailable(u32),
    /// Advisory writer lock exists.
    #[error("project is already open for writing: {0}")]
    ProjectLocked(PathBuf),
    /// Lock acquisition failed.
    #[error("could not acquire project advisory lock: {0}")]
    Lock(io::Error),
    /// Read-only project cannot be changed.
    #[error("project was opened read-only")]
    ReadOnly,
    /// Body was missing for a document record.
    #[error("document body is missing: {0}")]
    MissingBody(DocumentId),
    /// Attachment ID does not exist in the catalog.
    #[error("attachment is missing: {0}")]
    MissingAttachment(parchmint_domain::AssetId),
    #[error("attachment import source is a symlink: {0}")]
    AttachmentSourceSymlink(PathBuf),
    #[error("attachment import source is not a regular file: {0}")]
    AttachmentSourceNotFile(PathBuf),
    #[error("attachment source has no usable filename: {0}")]
    InvalidAttachmentName(PathBuf),
    #[error("attachment destination already exists: {0}")]
    AttachmentDestinationExists(PathBuf),
    #[error("could not copy attachment: {0}")]
    CopyAttachment(io::Error),
    #[error("could not persist attachment: {0}")]
    AttachmentPersist(io::Error),
    /// Lexical/canonical path containment failed.
    #[error("project path escapes root: {0}")]
    PathEscape(String),
    /// A symlink exists in a canonical project path.
    #[error("project path contains symlink: {0}")]
    SymlinkEscape(PathBuf),
    /// TOML serialization failed.
    #[error("could not serialize canonical TOML: {0}")]
    SerializeToml(String),
    /// Previous location cleanup failed after durable replacement at the new location.
    #[error("could not remove previous document location: {0}")]
    RemoveOldDocument(io::Error),
    /// Durable recovery record could not be published before canonical mutation.
    #[error("could not publish canonical transaction: {0}")]
    PublishTransaction(io::Error),
    /// A completed/recovered local transaction directory could not be removed.
    #[error("could not remove canonical transaction state: {0}")]
    RemoveTransaction(io::Error),
    /// Deterministic test-only failure after the given canonical mutation count.
    #[error("injected canonical transaction fault after mutation {0}")]
    InjectedTransactionFault(usize),
    /// Backup copy failed.
    #[error("could not create migration backup: {0}")]
    CopyBackup(io::Error),
    /// Atomic replacement failed.
    #[error(transparent)]
    Atomic(#[from] AtomicWriteError),
}
/// A phase-specific atomic-write failure.
#[derive(Debug, Error)]
pub enum AtomicWriteError {
    #[error("atomic-write destination has no parent: {0}")]
    MissingParent(PathBuf),
    #[error("could not prepare destination directory: {0}")]
    PrepareDirectory(io::Error),
    #[error("could not create same-directory temporary file: {0}")]
    CreateTemporary(io::Error),
    #[error("could not write temporary file: {0}")]
    WriteTemporary(io::Error),
    #[error("could not flush temporary file: {0}")]
    FlushTemporary(io::Error),
    #[error("could not atomically replace destination: {0}")]
    Replace(io::Error),
    #[error("replacement succeeded but directory metadata flush failed: {0}")]
    FlushDirectory(io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn create_save_close_reopen_is_deterministic_and_disposable_state_is_safe() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("Novel");
        let mut project = ProjectStorage::create(&root, "Novel").unwrap();
        assert!(root.join("parchmint.toml").is_file());
        let before = fs::read_to_string(root.join("outline.toml")).unwrap();
        ProjectStorage::save(&mut project).unwrap();
        assert_eq!(
            before,
            fs::read_to_string(root.join("outline.toml")).unwrap()
        );
        ProjectStorage::close(project).unwrap();
        fs::remove_dir_all(root.join(".parchmint")).unwrap();
        let reopened = ProjectStorage::reopen(&root, OpenMode::ReadWrite).unwrap();
        assert_eq!(reopened.project.name, "Novel");
    }

    #[test]
    fn open_defers_markdown_bodies_until_a_consumer_requests_one() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("Lazy");
        let mut opened = ProjectStorage::create(&root, "Lazy").unwrap();
        let node = NodeId::new();
        let document = DocumentId::new();
        opened
            .execute(ProjectCommand::Create {
                parent: opened.project.manuscript_root(),
                node: Node {
                    id: node,
                    kind: NodeKind::Document {
                        document_id: document,
                    },
                    parent: Some(opened.project.manuscript_root()),
                    children: Vec::new(),
                },
                document: DocumentRecord {
                    id: document,
                    node_id: node,
                    path: RelativeProjectPath::new(format!("manuscript/{node}.md")).unwrap(),
                    metadata: DocumentMetadata {
                        title: "Deferred".into(),
                        ..DocumentMetadata::default()
                    },
                },
                index: 0,
            })
            .unwrap();
        opened
            .set_body(document, "winter orchard\n".into())
            .unwrap();
        ProjectStorage::save(&mut opened).unwrap();
        drop(opened);

        let reopened = ProjectStorage::open(&root, OpenMode::ReadWrite).unwrap();
        assert_eq!(reopened.loaded_body_count(), 0);
        let snapshots = reopened.body_snapshot();
        assert_eq!(reopened.loaded_body_count(), 0);
        assert_eq!(
            snapshots[&document].load().unwrap().as_ref(),
            "winter orchard\n"
        );
        assert_eq!(reopened.loaded_body_count(), 1);
    }

    #[test]
    #[cfg_attr(debug_assertions, ignore = "release-mode Stage 14 10k/10M open gate")]
    #[allow(clippy::too_many_lines)]
    fn ten_thousand_node_ten_million_word_project_opens_with_bodies_deferred() {
        use std::fmt::Write as _;

        fn schedule_and_commit(
            opened: &mut OpenProject,
            command: ProjectCommand,
        ) -> (std::time::Duration, std::time::Duration, SaveMetrics) {
            let started = std::time::Instant::now();
            let scheduled = ProjectStorage::schedule_command(opened, command).unwrap();
            let owner = started.elapsed();
            let started = std::time::Instant::now();
            let metrics = scheduled.plan.execute().unwrap();
            let worker = started.elapsed();
            ProjectStorage::acknowledge_scheduled_command(opened, metrics);
            (owner, worker, metrics)
        }

        let target = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        let directory = tempfile::tempdir_in(target).unwrap();
        let root = directory.path().join("Canonical-scale");
        let opened = ProjectStorage::create(&root, "Canonical scale").unwrap();
        let manuscript = opened.project.manuscript_root();
        let research = opened.project.research_root();
        drop(opened);

        let mut outline = format!(
            "format_version = 1\nroots = [\"{manuscript}\", \"{research}\"]\n\n[[nodes]]\nid = \"{manuscript}\"\nkind = \"manuscript_root\"\nchildren = ["
        );
        let identities = (0..10_000)
            .map(|_| (NodeId::new(), DocumentId::new()))
            .collect::<Vec<_>>();
        for (index, (node, _)) in identities.iter().enumerate() {
            if index > 0 {
                outline.push_str(", ");
            }
            let _ = write!(outline, "\"{node}\"");
        }
        let _ = write!(
            outline,
            "]\n\n[[nodes]]\nid = \"{research}\"\nkind = \"research_root\"\nchildren = []\n"
        );
        for (node, document) in &identities {
            let _ = write!(
                outline,
                "\n[[nodes]]\nid = \"{node}\"\nkind = \"document\"\ndocument_id = \"{document}\"\nparent = \"{manuscript}\"\nchildren = []\n"
            );
        }
        fs::write(root.join("outline.toml"), outline).unwrap();
        fs::create_dir_all(root.join("manuscript")).unwrap();
        let body = "orchard β 雪 ".repeat(334);
        for (index, (node, document)) in identities.iter().enumerate() {
            fs::write(
                root.join(format!("manuscript/{node}.md")),
                format!("---\ndocument_id: {document}\ntitle: Scene {index}\n---\n{body}\n"),
            )
            .unwrap();
        }
        let started = std::time::Instant::now();
        let mut reopened = ProjectStorage::open(&root, OpenMode::ReadWrite).unwrap();
        let elapsed = started.elapsed();
        eprintln!(
            "stage14 canonical-nodes=10000 canonical-words=10020000 open={elapsed:?} loaded-bodies={}",
            reopened.loaded_body_count()
        );
        assert_eq!(reopened.project.documents.len(), 10_000);
        assert_eq!(reopened.loaded_body_count(), 0);
        assert!(
            elapsed < std::time::Duration::from_secs(3),
            "canonical scale open took {elapsed:?}"
        );

        let (first_node, first_document) = identities[0];
        let (rename_owner, rename_worker, rename_metrics) = schedule_and_commit(
            &mut reopened,
            ProjectCommand::Rename {
                node: first_node,
                title: "Bounded rename".into(),
            },
        );
        assert_eq!(rename_metrics.files_written, 1);
        assert_eq!(rename_metrics.files_removed, 0);

        let mut metadata = reopened.project.documents[&first_document].metadata.clone();
        metadata.summary = "A bounded synopsis".into();
        metadata.flags.insert("include-in-compile".into(), false);
        let (metadata_owner, metadata_worker, metadata_metrics) = schedule_and_commit(
            &mut reopened,
            ProjectCommand::EditMetadata {
                document: first_document,
                metadata,
            },
        );
        assert_eq!(metadata_metrics.files_written, 1);
        assert_eq!(metadata_metrics.files_removed, 0);

        let (reorder_owner, reorder_worker, reorder_metrics) = schedule_and_commit(
            &mut reopened,
            ProjectCommand::Reorder {
                node: first_node,
                index: 9_999,
            },
        );
        assert_eq!(reorder_metrics.files_written, 1);
        assert_eq!(reorder_metrics.files_removed, 0);

        let started = std::time::Instant::now();
        reopened
            .set_body(first_document, format!("{body}bounded edit\n"))
            .unwrap();
        ProjectStorage::save(&mut reopened).unwrap();
        let body_save = started.elapsed();
        let body_metrics = reopened.last_save_metrics();
        assert_eq!(body_metrics.files_written, 1);
        assert_eq!(body_metrics.files_removed, 0);
        eprintln!(
            "stage14 rename-owner={rename_owner:?} rename-worker={rename_worker:?}/{rename_metrics:?} metadata-owner={metadata_owner:?} metadata-worker={metadata_worker:?}/{metadata_metrics:?} reorder-owner={reorder_owner:?} reorder-worker={reorder_worker:?}/{reorder_metrics:?} body-save={body_save:?}/{body_metrics:?}"
        );
        for (operation, duration) in [
            ("rename owner", rename_owner),
            ("metadata owner", metadata_owner),
            ("reorder owner", reorder_owner),
        ] {
            assert!(
                duration < std::time::Duration::from_millis(100),
                "{operation} took {duration:?}"
            );
        }
        for (operation, duration) in [
            ("rename worker", rename_worker),
            ("metadata worker", metadata_worker),
            ("reorder worker", reorder_worker),
            ("body save", body_save),
        ] {
            assert!(
                duration < std::time::Duration::from_secs(1),
                "{operation} took {duration:?}"
            );
        }
    }
    #[test]
    fn read_only_access_survives_writer_lock_and_traversal_symlink_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("Novel");
        let writer = ProjectStorage::create(&root, "Novel").unwrap();
        assert!(ProjectStorage::open(&root, OpenMode::ReadWrite).is_err());
        assert!(ProjectStorage::open(&root, OpenMode::ReadOnly).is_ok());
        let bad = RelativeProjectPath::new("../secret");
        assert!(bad.is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(directory.path(), root.join("assets-link")).unwrap();
            let path = RelativeProjectPath::new("assets-link/secret").unwrap();
            assert!(resolve_project_path(&root, &path).is_err());
        }
        drop(writer);
        assert!(
            root.join(".parchmint/open.lock").is_file(),
            "diagnostic metadata may remain after release"
        );
        if let Err(error) = ProjectStorage::open(&root, OpenMode::ReadWrite) {
            panic!("reopen after release failed: {error}");
        }
    }

    #[test]
    fn advisory_lock_child() {
        let Ok(root) = std::env::var("PARCHMINT_LOCK_CHILD_ROOT") else {
            return;
        };
        let project = ProjectStorage::open(&root, OpenMode::ReadWrite).unwrap();
        fs::write(Path::new(&root).join("child-ready"), b"ready").unwrap();
        std::thread::sleep(std::time::Duration::from_secs(30));
        drop(project);
    }

    #[test]
    fn killed_process_releases_advisory_lock() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("Novel");
        drop(ProjectStorage::create(&root, "Novel").unwrap());
        let executable = std::env::current_exe().unwrap();
        let mut child = std::process::Command::new(executable)
            .args(["--exact", "tests::advisory_lock_child", "--nocapture"])
            .env("PARCHMINT_LOCK_CHILD_ROOT", &root)
            .spawn()
            .unwrap();
        let ready = root.join("child-ready");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !ready.is_file() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(ready.is_file(), "lock-holder helper did not start");
        assert!(matches!(
            ProjectStorage::open(&root, OpenMode::ReadWrite),
            Err(StorageError::ProjectLocked(_))
        ));
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(
            ProjectStorage::open(&root, OpenMode::ReadWrite).is_ok(),
            "process death must release the OS lock even with metadata present"
        );
    }

    #[test]
    fn attachment_import_copies_to_uuid_catalog_and_never_overwrites_source_name() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("Novel");
        let source = directory.path().join("reference notes.txt");
        fs::write(&source, b"ordinary reference text").unwrap();
        let mut opened = ProjectStorage::create(&root, "Novel").unwrap();
        let attachment = ProjectStorage::import_attachment(&mut opened, &source).unwrap();
        assert_eq!(attachment.display_name, "reference notes.txt");
        assert!(attachment.safe_name.starts_with(&attachment.id.to_string()));
        assert_eq!(attachment.media_type, "text");
        let (path, preview) = ProjectStorage::attachment_preview(&opened, attachment.id).unwrap();
        assert_eq!(preview, AttachmentPreview::PlainText);
        assert_eq!(fs::read(path).unwrap(), b"ordinary reference text");
        assert!(root.join("assets.toml").is_file());
        drop(opened);
        let reopened = ProjectStorage::open(&root, OpenMode::ReadWrite).unwrap();
        assert!(reopened.attachments().contains_key(&attachment.id));
    }

    #[cfg(unix)]
    #[test]
    fn attachment_import_rejects_source_symlinks_and_asset_symlink_escape() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("Novel");
        let source = directory.path().join("source.txt");
        fs::write(&source, b"reference").unwrap();
        let source_link = directory.path().join("source-link.txt");
        std::os::unix::fs::symlink(&source, &source_link).unwrap();
        let mut opened = ProjectStorage::create(&root, "Novel").unwrap();
        assert!(matches!(
            ProjectStorage::import_attachment(&mut opened, &source_link),
            Err(StorageError::AttachmentSourceSymlink(_))
        ));
        fs::remove_dir(root.join("assets")).ok();
        std::os::unix::fs::symlink(directory.path(), root.join("assets")).unwrap();
        assert!(matches!(
            ProjectStorage::import_attachment(&mut opened, &source),
            Err(StorageError::SymlinkEscape(_))
        ));
    }
    #[test]
    fn newer_version_and_unknown_keys_are_safe() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("Novel");
        let project = ProjectStorage::create(&root, "Novel").unwrap();
        drop(project);
        let manifest = root.join("parchmint.toml");
        let original = fs::read_to_string(&manifest).unwrap();
        fs::write(
            &manifest,
            original.replace("format_version = 1", "format_version = 99"),
        )
        .unwrap();
        assert!(matches!(
            ProjectStorage::open(&root, OpenMode::ReadOnly),
            Err(StorageError::UnsupportedFormat(99))
        ));
    }
    #[test]
    fn atomic_write_replaces_complete_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state");
        atomic_write(&path, b"old").unwrap();
        atomic_write(&path, b"new complete state").unwrap();
        assert_eq!(fs::read(path).unwrap(), b"new complete state");
    }
    #[test]
    fn structural_create_duplicate_trash_and_restore_survive_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("Novel");
        let mut opened = ProjectStorage::create(&root, "Novel").unwrap();
        let node_id = NodeId::new();
        let document_id = DocumentId::new();
        let node = Node {
            id: node_id,
            kind: NodeKind::Document { document_id },
            parent: Some(opened.project.manuscript_root()),
            children: vec![],
        };
        let record = DocumentRecord {
            id: document_id,
            node_id,
            path: RelativeProjectPath::new(format!("manuscript/{node_id}.md")).unwrap(),
            metadata: DocumentMetadata {
                title: "Chapter One".into(),
                ..Default::default()
            },
        };
        opened
            .execute(ProjectCommand::Create {
                parent: opened.project.manuscript_root(),
                node,
                document: record,
                index: 0,
            })
            .unwrap();
        opened.set_body(document_id, "Body text\n".into()).unwrap();
        let duplicate = opened
            .execute(ProjectCommand::Duplicate {
                node: node_id,
                parent: opened.project.manuscript_root(),
                index: 1,
            })
            .unwrap();
        let ProjectEvent::NodeDuplicated { copy: copied, .. } = duplicate.events[0] else {
            unreachable!();
        };
        let undo = opened
            .execute(ProjectCommand::Trash { node: copied })
            .unwrap()
            .undo;
        ProjectStorage::save(&mut opened).unwrap();
        assert!(root.join(format!("trash/{copied}.md")).is_file());
        opened.execute(undo.inverse).unwrap();
        ProjectStorage::close(opened).unwrap();
        let reopened = ProjectStorage::open(&root, OpenMode::ReadWrite).unwrap();
        assert_eq!(reopened.project.documents.len(), 2);
        assert!(reopened.project.nodes.contains_key(&copied));
    }
    #[test]
    fn metadata_and_order_saves_touch_only_their_canonical_resources() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("Novel");
        let mut opened = ProjectStorage::create(&root, "Novel").unwrap();
        let manuscript = opened.project.manuscript_root();
        let mut nodes = Vec::new();
        for index in 0..128 {
            let node_id = NodeId::new();
            let document_id = DocumentId::new();
            opened
                .execute(ProjectCommand::Create {
                    parent: manuscript,
                    node: Node {
                        id: node_id,
                        kind: NodeKind::Document { document_id },
                        parent: Some(manuscript),
                        children: Vec::new(),
                    },
                    document: DocumentRecord {
                        id: document_id,
                        node_id,
                        path: RelativeProjectPath::new(format!("manuscript/{node_id}.md")).unwrap(),
                        metadata: DocumentMetadata {
                            title: format!("Scene {index}"),
                            ..DocumentMetadata::default()
                        },
                    },
                    index,
                })
                .unwrap();
            opened
                .set_body(document_id, format!("body {index}\n"))
                .unwrap();
            nodes.push((node_id, document_id));
        }
        ProjectStorage::save(&mut opened).unwrap();
        let unrelated = root.join(format!("manuscript/{}.md", nodes[64].0));
        let unrelated_before = fs::read(&unrelated).unwrap();

        ProjectStorage::execute_command(
            &mut opened,
            ProjectCommand::Rename {
                node: nodes[0].0,
                title: "Bounded rename".into(),
            },
        )
        .unwrap();
        assert_eq!(
            opened.last_save_metrics(),
            SaveMetrics {
                files_written: 1,
                files_removed: 0,
                bytes_written: opened.last_save_metrics().bytes_written,
            }
        );
        assert_eq!(fs::read(&unrelated).unwrap(), unrelated_before);

        ProjectStorage::execute_command(
            &mut opened,
            ProjectCommand::Reorder {
                node: nodes[0].0,
                index: 127,
            },
        )
        .unwrap();
        assert_eq!(opened.last_save_metrics().files_written, 1);
        assert_eq!(opened.last_save_metrics().files_removed, 0);
        assert_eq!(fs::read(&unrelated).unwrap(), unrelated_before);
    }

    #[test]
    fn partial_structural_commit_restores_disk_and_in_memory_state() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("Novel");
        let mut opened = ProjectStorage::create(&root, "Novel").unwrap();
        let manuscript = opened.project.manuscript_root();
        let node_id = NodeId::new();
        let document_id = DocumentId::new();
        let result = ProjectStorage::execute_command_with_fault(
            &mut opened,
            ProjectCommand::Create {
                parent: manuscript,
                node: Node {
                    id: node_id,
                    kind: NodeKind::Document { document_id },
                    parent: Some(manuscript),
                    children: Vec::new(),
                },
                document: DocumentRecord {
                    id: document_id,
                    node_id,
                    path: RelativeProjectPath::new(format!("manuscript/{node_id}.md")).unwrap(),
                    metadata: DocumentMetadata {
                        title: "Never acknowledged".into(),
                        ..DocumentMetadata::default()
                    },
                },
                index: 0,
            },
            Some(TransactionFault::AfterMutation(1)),
        );
        assert!(matches!(
            result,
            Err(StorageError::InjectedTransactionFault(1))
        ));
        assert!(!opened.project.nodes.contains_key(&node_id));
        assert!(!opened.project.documents.contains_key(&document_id));
        assert!(!root.join(PENDING_TRANSACTION).exists());
        drop(opened);
        let reopened = ProjectStorage::open(&root, OpenMode::ReadWrite).unwrap();
        assert!(!reopened.project.nodes.contains_key(&node_id));
        reopened.project.validate().unwrap();
    }
    #[test]
    fn hand_authored_example_opens_without_local_state() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/harbor-lights");
        let opened = ProjectStorage::open(root, OpenMode::ReadOnly).unwrap();
        assert_eq!(opened.project.name, "Harbor Lights");
        assert_eq!(opened.project.documents.len(), 1);
    }

    #[test]
    fn example_project_catalog_is_human_readable_and_openable() {
        let examples = [
            ("tour", "ParchMint Tour", 1),
            ("medium-novel", "The Lantern Route", 3),
            ("research-heavy", "Field Notes for a Small Harbor", 3),
            ("unicode-notes", "Unicode Notes", 1),
            ("format-edge-case", "Format Edge Cases", 1),
        ];
        for (directory, name, documents) in examples {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples")
                .join(directory);
            let opened = ProjectStorage::open(root, OpenMode::ReadOnly).unwrap();
            assert_eq!(opened.project.name, name, "example {directory}");
            assert_eq!(
                opened.project.documents.len(),
                documents,
                "example {directory}"
            );
        }
    }
}
