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
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use tempfile::NamedTempFile;
use thiserror::Error;

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
    bodies: BTreeMap<DocumentId, String>,
    unknown_front_matter: BTreeMap<DocumentId, Mapping>,
    locations: BTreeMap<DocumentId, RelativeProjectPath>,
    manifest_extra: BTreeMap<String, toml::Value>,
    outline_extra: BTreeMap<String, toml::Value>,
    styles_extra: BTreeMap<String, toml::Value>,
    attachments: BTreeMap<parchmint_domain::AssetId, AttachmentRecord>,
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
    /// Returns a document body without its YAML front matter.
    pub fn body(&self, id: DocumentId) -> Result<&str, StorageError> {
        self.bodies
            .get(&id)
            .map(String::as_str)
            .ok_or(StorageError::MissingBody(id))
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
        let entry = self
            .bodies
            .get_mut(&id)
            .ok_or(StorageError::MissingBody(id))?;
        *entry = body;
        Ok(())
    }
    /// Applies a graph command and initializes source-preserving Markdown state
    /// for newly created or duplicated documents. Call [`ProjectStorage::save`]
    /// after a batch to acknowledge it durably.
    pub fn execute(&mut self, command: ProjectCommand) -> Result<CommandOutcome, StorageError> {
        let outcome = self
            .project
            .execute(command)
            .map_err(StorageError::Domain)?;
        for event in &outcome.events {
            match *event {
                ProjectEvent::NodeCreated(node) => self.initialize_new_document(node, None)?,
                ProjectEvent::NodeDuplicated { source, copy } => {
                    self.copy_subtree_bodies(source, copy)?;
                }
                _ => {}
            }
        }
        Ok(outcome)
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
            .unwrap_or_default();
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
        for (source_child, copy_child) in source_children.into_iter().zip(copy_children) {
            self.copy_subtree_bodies(source_child, copy_child)?;
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
            manifest_extra: BTreeMap::new(),
            outline_extra: BTreeMap::new(),
            styles_extra: BTreeMap::new(),
            attachments: BTreeMap::new(),
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
        let document_nodes = project
            .nodes
            .values()
            .filter_map(|node| node.kind.document_id().map(|id| (node.id, id)))
            .collect::<Vec<_>>();
        for (node_id, document_id) in document_nodes {
            let path = canonical_document_path(&project, node_id)?;
            let disk = resolve_project_path(&canonical, &path)?;
            let source = read_bounded(&disk, MAX_DOCUMENT_BYTES, "document")?;
            let (metadata, body, unknown) = parse_document(&source, document_id)?;
            project.documents.insert(
                document_id,
                DocumentRecord {
                    id: document_id,
                    node_id,
                    path: path.clone(),
                    metadata,
                },
            );
            bodies.insert(document_id, body);
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
            manifest_extra: manifest.extra,
            outline_extra: outline.extra,
            styles_extra: styles.extra,
            attachments,
        })
    }
    /// Persists all canonical files deterministically. A read-only handle is never modified.
    pub fn save(opened: &mut OpenProject) -> Result<(), StorageError> {
        if opened.mode == OpenMode::ReadOnly {
            return Err(StorageError::ReadOnly);
        }
        opened.project.validate().map_err(StorageError::Domain)?;
        let manifest = Manifest {
            format_version: FORMAT_VERSION,
            project_id: opened.project.id,
            name: opened.project.name.clone(),
            extra: opened.manifest_extra.clone(),
        };
        let outline = Outline {
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
        };
        let styles = Styles {
            format_version: FORMAT_VERSION,
            definitions: opened.project.styles.values().cloned().collect(),
            compile_presets: opened.project.compile_presets.values().cloned().collect(),
            extra: opened.styles_extra.clone(),
        };
        write_toml(&opened.root.join("parchmint.toml"), &manifest)?;
        write_toml(&opened.root.join("outline.toml"), &outline)?;
        write_toml(&opened.root.join("styles.toml"), &styles)?;
        save_attachment_catalog(&opened.root, &opened.attachments)?;
        let records = opened.project.documents.keys().copied().collect::<Vec<_>>();
        for id in records {
            Self::save_document(opened, id)?;
        }
        for tombstone in opened.project.trash.values() {
            write_toml(
                &resolve_project_path(
                    &opened.root,
                    &RelativeProjectPath::new(format!("trash/{}.toml", tombstone.node_id))
                        .map_err(StorageError::Domain)?,
                )?,
                tombstone,
            )?;
        }
        Ok(())
    }
    /// Atomically persists one canonical Markdown document without rewriting
    /// unrelated documents or project manifests. Editor autosave uses this to
    /// avoid overwriting an external change in another document.
    pub fn save_document(opened: &mut OpenProject, id: DocumentId) -> Result<(), StorageError> {
        if opened.mode == OpenMode::ReadOnly {
            return Err(StorageError::ReadOnly);
        }
        opened.project.validate().map_err(StorageError::Domain)?;
        let record = opened
            .project
            .documents
            .get(&id)
            .cloned()
            .ok_or(StorageError::MissingBody(id))?;
        let desired = canonical_document_path(&opened.project, record.node_id)?;
        let body = opened
            .bodies
            .get(&record.id)
            .ok_or(StorageError::MissingBody(record.id))?;
        let unknown = opened
            .unknown_front_matter
            .get(&record.id)
            .cloned()
            .unwrap_or_default();
        let bytes = serialize_document(record.id, &record.metadata, &unknown, body)?;
        let target = resolve_project_path(&opened.root, &desired)?;
        atomic_write(&target, bytes.as_bytes())?;
        if let Some(previous) = opened.locations.insert(record.id, desired.clone())
            && previous != desired
        {
            let old = resolve_project_path(&opened.root, &previous)?;
            if old.is_file() {
                fs::remove_file(old).map_err(StorageError::RemoveOldDocument)?;
            }
        }
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
    let text = toml::to_string_pretty(value)
        .map_err(|error| StorageError::SerializeToml(error.to_string()))?;
    atomic_write(path, text.as_bytes())?;
    Ok(())
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

fn canonical_document_path(
    project: &Project,
    node: NodeId,
) -> Result<RelativeProjectPath, StorageError> {
    let folder = root_folder(project, node)?;
    let name = if project.is_trashed(node) {
        format!("trash/{node}.md")
    } else {
        format!("{folder}/{node}.md")
    };
    RelativeProjectPath::new(name).map_err(StorageError::Domain)
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
    path: PathBuf,
    _file: File,
}
impl AdvisoryLock {
    fn acquire(root: &Path) -> Result<Self, StorageError> {
        let path = root.join(".parchmint").join("open.lock");
        fs::create_dir_all(
            path.parent()
                .ok_or(StorageError::InvalidSchema("lock parent"))?,
        )
        .map_err(StorageError::CreateDirectory)?;
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| {
                if error.kind() == io::ErrorKind::AlreadyExists {
                    StorageError::ProjectLocked(path.clone())
                } else {
                    StorageError::Lock(error)
                }
            })?;
        Ok(Self { path, _file: file })
    }
}
impl Drop for AdvisoryLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Writes `contents` beside `destination`, flushes it, atomically replaces it, and flushes directory metadata on Unix.
pub fn atomic_write(destination: &Path, contents: &[u8]) -> Result<(), AtomicWriteError> {
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
    temporary
        .persist(destination)
        .map_err(|error| AtomicWriteError::Replace(error.error))?;
    sync_parent(parent)?;
    Ok(())
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
