//! Deterministic codecs for ParchMint's authored project files.
//!
//! The format is intentionally small and conservative: data that cannot be
//! represented safely and portably is rejected before it reaches a canonical
//! project resource.

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

use parchmint_contracts::{
    AnnotationAnchor, AnnotationMessage, AnnotationThread, AnnotationValue,
    generated::AnnotationSidecarV1,
};
use parchmint_domain::{
    CheckpointId, DeletedNodeSnapshot, DeletionTombstone, DocumentId as DomainDocumentId, NodeId,
    NodeKind, Project, ProjectExportSetting, ProjectExportSettings, ProjectId, ProjectNode,
    ProjectSection, StyleCatalog, StyleDefinition, StyleId, StyleProperties, StyleRole,
    TextAlignment,
};
use sha2::{Digest, Sha256};

const FORMAT_CONTROL_V1: &[u8] = b"1\n";
const ANNOTATION_SCHEMA_V1: &str = "parchmint.annotation-sidecar/v1";

/// A format version understood by this build of ParchMint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FormatVersion {
    V1,
}

impl FormatVersion {
    /// Returns the exact bytes stored in `.parchmint/format-version`.
    pub const fn control_bytes(self) -> &'static [u8] {
        match self {
            Self::V1 => FORMAT_CONTROL_V1,
        }
    }
}

/// An error produced while parsing, validating, or encoding project data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatError {
    NonUtf8 { resource: &'static str },
    InvalidFormatControl,
    UnsafePath { path: String, reason: &'static str },
    PathCollision { first: String, second: String },
    InvalidManifest(String),
    InvalidStyles(String),
    InvalidDictionary(String),
    InvalidDocument(String),
    InvalidAnnotations(String),
    MissingFormatControl,
    MissingManifest,
    UnsupportedResource { path: String },
}

impl fmt::Display for FormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonUtf8 { resource } => write!(formatter, "{resource} is not UTF-8"),
            Self::InvalidFormatControl => formatter.write_str("unknown or invalid format control"),
            Self::UnsafePath { path, reason } => {
                write!(formatter, "unsafe project path {path:?}: {reason}")
            }
            Self::PathCollision { first, second } => {
                write!(
                    formatter,
                    "project paths collide on a portable filesystem: {first:?} and {second:?}"
                )
            }
            Self::InvalidManifest(reason) => {
                write!(formatter, "invalid project manifest: {reason}")
            }
            Self::InvalidStyles(reason) => write!(formatter, "invalid project styles: {reason}"),
            Self::InvalidDictionary(reason) => {
                write!(formatter, "invalid project dictionary: {reason}")
            }
            Self::InvalidDocument(reason) => write!(formatter, "invalid canonical HTML: {reason}"),
            Self::InvalidAnnotations(reason) => {
                write!(formatter, "invalid annotation sidecar: {reason}")
            }
            Self::MissingFormatControl => {
                formatter.write_str("project is missing its format control")
            }
            Self::MissingManifest => formatter.write_str("project is missing its manifest"),
            Self::UnsupportedResource { path } => {
                write!(formatter, "unsupported canonical resource {path:?}")
            }
        }
    }
}

impl Error for FormatError {}

/// A project-relative, slash-separated portable path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CanonicalRelativePath(String);

impl CanonicalRelativePath {
    pub fn parse(path: impl AsRef<str>) -> Result<Self, FormatError> {
        let path = path.as_ref();
        if path.is_empty() {
            return Self::unsafe_path(path, "path is empty");
        }
        if path.starts_with('/') || path.starts_with("//") {
            return Self::unsafe_path(path, "path must be relative");
        }
        if path.contains('\\') {
            return Self::unsafe_path(path, "backslashes are not portable separators");
        }
        if path.as_bytes().get(1) == Some(&b':') && path.as_bytes()[0].is_ascii_alphabetic() {
            return Self::unsafe_path(path, "drive-qualified paths are not relative");
        }

        for segment in path.split('/') {
            if segment.is_empty() {
                return Self::unsafe_path(path, "empty path segments are not normalized");
            }
            if matches!(segment, "." | "..") {
                return Self::unsafe_path(path, "dot segments are not allowed");
            }
            if segment.ends_with('.') || segment.ends_with(' ') {
                return Self::unsafe_path(path, "segments cannot end in a dot or space");
            }
            if segment.contains(':') {
                return Self::unsafe_path(path, "colons are not portable in filenames");
            }
            if segment.chars().any(|character| {
                character.is_control() || matches!(character, '<' | '>' | '"' | '|' | '?' | '*')
            }) {
                return Self::unsafe_path(path, "path contains a nonportable character");
            }
            if segment.chars().any(is_combining_mark) {
                return Self::unsafe_path(path, "path must use precomposed Unicode characters");
            }
            if is_windows_device_name(segment) {
                return Self::unsafe_path(path, "path uses a reserved Windows device name");
            }
        }
        Ok(Self(path.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn unsafe_path<T>(path: &str, reason: &'static str) -> Result<T, FormatError> {
        Err(FormatError::UnsafePath {
            path: path.to_owned(),
            reason,
        })
    }
}

impl fmt::Display for CanonicalRelativePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A stable SHA-256 digest of canonical bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// The identity of a canonical project resource.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResourceId {
    FormatControl,
    Manifest,
    Styles,
    Dictionary,
    /// Legacy document resource identity used by v1 recovery records.
    Document,
    /// Stable identity for one exact document in a multi-document project.
    DocumentById {
        document_id: String,
    },
    Annotations {
        document_id: String,
    },
}

/// Canonical bytes ready for the repository layer to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalBytes {
    pub resource: ResourceId,
    pub path: CanonicalRelativePath,
    pub bytes: Vec<u8>,
    pub hash: ContentHash,
}

/// A canonical TOML project manifest.
#[derive(Debug, Clone, PartialEq)]
pub struct CanonicalManifest(toml::Value);

impl CanonicalManifest {
    pub fn value(&self) -> &toml::Value {
        &self.0
    }
}

/// A canonical semantic stylesheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalStyles {
    rules: Vec<CssRule>,
}

impl CanonicalStyles {
    pub fn as_css(&self) -> String {
        render_css(&self.rules)
    }
}

/// A canonical project dictionary, one word or phrase per line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalDictionary {
    entries: BTreeSet<String>,
}

impl CanonicalDictionary {
    pub fn entries(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(String::as_str)
    }
}

/// Sanitized, deterministic restricted HTML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalDocument {
    html: String,
}

impl CanonicalDocument {
    pub fn as_html(&self) -> &str {
        &self.html
    }

    /// Appends `suffix` to the first Document Title block only when its
    /// rendered text still matches `display_title`. This preserves divergent
    /// authored titles while keeping a synchronized copy's display and
    /// content titles aligned.
    pub fn append_copy_suffix_to_matching_title(&self, display_title: &str, suffix: &str) -> Self {
        let mut nodes = match parse_html(&self.html) {
            Ok(nodes) => nodes,
            // A CanonicalDocument has already passed this parser. Retaining
            // the original bytes is nevertheless safer than making a title
            // copy operation destructive if that invariant ever changes.
            Err(_) => return self.clone(),
        };
        if append_suffix_to_first_matching_document_title(&mut nodes, display_title, suffix) {
            Self {
                html: render_html(&nodes),
            }
        } else {
            self.clone()
        }
    }
}

/// A validated JSON annotation sidecar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalAnnotations(AnnotationSidecarV1);

impl CanonicalAnnotations {
    pub fn document_id(&self) -> &str {
        &self.0.document_id
    }

    pub fn threads(&self) -> &[serde_json::Value] {
        &self.0.threads
    }

    pub fn typed_threads(&self) -> Result<Vec<AnnotationThread>, FormatError> {
        self.0.threads.iter().map(decode_thread).collect()
    }

    pub fn from_typed(
        document_id: impl Into<String>,
        threads: &[AnnotationThread],
    ) -> Result<Self, FormatError> {
        let sidecar = AnnotationSidecarV1 {
            schema: ANNOTATION_SCHEMA_V1.to_owned(),
            document_id: document_id.into(),
            threads: threads
                .iter()
                .map(encode_thread)
                .collect::<Result<_, _>>()?,
        };
        if sidecar.document_id.is_empty() || sidecar.document_id.chars().any(char::is_control) {
            return Err(FormatError::InvalidAnnotations(
                "document ID is empty or unsafe".into(),
            ));
        }
        Ok(Self(canonicalize_annotations(sidecar)?))
    }

    pub fn empty(document_id: impl Into<String>) -> Result<Self, FormatError> {
        Self::from_typed(document_id, &[])
    }
}

/// Every canonical value that can be encoded by [`CanonicalCodec`].
#[derive(Debug, Clone, PartialEq)]
pub enum CanonicalResource {
    FormatControl(FormatVersion),
    Manifest(CanonicalManifest),
    Styles(CanonicalStyles),
    Dictionary(CanonicalDictionary),
    Document(CanonicalDocument),
    Annotations(CanonicalAnnotations),
}

/// The raw, project-relative bytes supplied to [`CanonicalCodec::decode_project`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CanonicalInputSet {
    pub format_control: Option<Vec<u8>>,
    pub resources: BTreeMap<CanonicalRelativePath, Vec<u8>>,
}

/// Decoded values for a whole project. The paths remain authoritative.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectModel {
    pub format_version: FormatVersion,
    pub resources: BTreeMap<CanonicalRelativePath, CanonicalResource>,
}

/// A complete resource set produced by a migration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalResourceSet {
    pub format_version: FormatVersion,
    pub resources: BTreeMap<CanonicalRelativePath, CanonicalBytes>,
}

/// Stable document locations retained alongside a decoded project session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CanonicalProjectPathMap {
    pub documents: BTreeMap<DomainDocumentId, CanonicalRelativePath>,
}

/// Revision frontier represented by the canonical files in one completed save.
///
/// The recovery sequence is distinct from the domain project's structural
/// revision. Persisting it lets recovery safely discard a saved journal prefix
/// while retaining and replaying later document edits after restart.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CanonicalPersistenceFrontier {
    pub recovery_project_revision: u64,
    pub document_revisions: BTreeMap<DomainDocumentId, u64>,
    /// Identity of one complete canonical save frontier. Legacy manifests omit it.
    pub save_identity: Option<ContentHash>,
    /// Body-independent summaries used for lazy project hydration.
    pub document_summaries: BTreeMap<DomainDocumentId, CanonicalDocumentSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalDocumentSummary {
    pub revision: u64,
    pub content_hash: ContentHash,
    pub word_count: usize,
}

/// A deterministic complete project encoding and paths removed by this save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalProjectEncoding {
    pub resources: BTreeMap<CanonicalRelativePath, CanonicalBytes>,
    pub paths: CanonicalProjectPathMap,
    pub persistence_frontier: CanonicalPersistenceFrontier,
    pub deletions: Vec<CanonicalRelativePath>,
}

/// One document body and annotation sidecar that changed since the durable
/// canonical baseline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalDocumentUpdate {
    pub body: String,
    pub annotations: Vec<AnnotationThread>,
}

/// Domain-owned canonical resources that need re-encoding for one save.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CanonicalDomainUpdate {
    pub manifest: bool,
    pub styles: bool,
    pub dictionary: bool,
    pub documents: BTreeMap<DomainDocumentId, CanonicalDocumentUpdate>,
}

/// Identity and hash of one resource in a complete durable canonical baseline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalResourceMetadata {
    pub resource: ResourceId,
    pub hash: ContentHash,
}

/// Changed bytes plus complete metadata for one incrementally encoded project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalProjectPatch {
    pub resources: BTreeMap<CanonicalRelativePath, CanonicalBytes>,
    pub complete_resources: BTreeMap<CanonicalRelativePath, CanonicalResourceMetadata>,
    pub paths: CanonicalProjectPathMap,
    pub persistence_frontier: CanonicalPersistenceFrontier,
    pub deletions: Vec<CanonicalRelativePath>,
}

/// An in-memory snapshot used as migration input.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceFormatSnapshot {
    pub format_control: Vec<u8>,
    pub resources: BTreeMap<CanonicalRelativePath, Vec<u8>>,
}

/// An error that leaves the source snapshot unchanged.
#[derive(Debug)]
pub enum MigrationError {
    Format(FormatError),
    UnsupportedTarget(FormatVersion),
}

impl fmt::Display for MigrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Format(error) => write!(formatter, "migration input is invalid: {error}"),
            Self::UnsupportedTarget(version) => {
                write!(formatter, "unsupported migration target {version:?}")
            }
        }
    }
}

impl Error for MigrationError {}

impl From<FormatError> for MigrationError {
    fn from(error: FormatError) -> Self {
        Self::Format(error)
    }
}

/// The format boundary used by repository and migration code.
pub trait CanonicalCodec: Send + Sync {
    fn detect(&self, control: &[u8]) -> Result<FormatVersion, FormatError>;
    fn decode_project(&self, input: CanonicalInputSet) -> Result<ProjectModel, FormatError>;
    fn decode_document(&self, bytes: &[u8]) -> Result<CanonicalDocument, FormatError>;
    fn decode_annotations(&self, bytes: &[u8]) -> Result<CanonicalAnnotations, FormatError>;
    fn encode(&self, value: &CanonicalResource) -> Result<CanonicalBytes, FormatError>;
    fn migrate(
        &self,
        source: SourceFormatSnapshot,
        target: FormatVersion,
    ) -> Result<CanonicalResourceSet, MigrationError>;
}

/// The v1 project codec.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProjectFormatCodec {
    _private: (),
}

impl ProjectFormatCodec {
    pub fn decode_manifest(&self, bytes: &[u8]) -> Result<CanonicalManifest, FormatError> {
        let text = utf8(bytes, "project manifest")?;
        let value = text
            .parse::<toml::Table>()
            .map(toml::Value::Table)
            .map_err(|error| FormatError::InvalidManifest(error.to_string()))?;
        Ok(CanonicalManifest(value))
    }

    pub fn decode_styles(&self, bytes: &[u8]) -> Result<CanonicalStyles, FormatError> {
        let text = utf8(bytes, "stylesheet")?;
        parse_css(text).map(|rules| CanonicalStyles { rules })
    }

    pub fn decode_dictionary(&self, bytes: &[u8]) -> Result<CanonicalDictionary, FormatError> {
        let text = utf8(bytes, "dictionary")?;
        if text.contains('\r') {
            return Err(FormatError::InvalidDictionary(
                "dictionary must use LF line endings".into(),
            ));
        }
        let mut entries = BTreeSet::new();
        for entry in text.split('\n') {
            if entry.is_empty() {
                continue;
            }
            if entry.trim() != entry || entry.chars().any(char::is_control) {
                return Err(FormatError::InvalidDictionary(
                    "entries cannot have surrounding whitespace or control characters".into(),
                ));
            }
            entries.insert(entry.to_owned());
        }
        Ok(CanonicalDictionary { entries })
    }

    /// Encodes a complete application project without exposing canonical
    /// write assembly to the UI layer.
    pub fn encode_domain_project(
        &self,
        project: &Project,
        documents: &BTreeMap<DomainDocumentId, String>,
        existing: &BTreeMap<CanonicalRelativePath, Vec<u8>>,
        previous_paths: &CanonicalProjectPathMap,
    ) -> Result<CanonicalProjectEncoding, FormatError> {
        self.encode_domain_project_with_frontier(
            project,
            documents,
            existing,
            previous_paths,
            &CanonicalPersistenceFrontier::default(),
        )
    }

    pub fn encode_domain_project_with_frontier(
        &self,
        project: &Project,
        documents: &BTreeMap<DomainDocumentId, String>,
        existing: &BTreeMap<CanonicalRelativePath, Vec<u8>>,
        previous_paths: &CanonicalProjectPathMap,
        frontier: &CanonicalPersistenceFrontier,
    ) -> Result<CanonicalProjectEncoding, FormatError> {
        self.encode_domain_project_with_annotations(
            project,
            documents,
            &BTreeMap::new(),
            existing,
            previous_paths,
            frontier,
        )
    }

    pub fn encode_domain_project_with_annotations(
        &self,
        project: &Project,
        documents: &BTreeMap<DomainDocumentId, String>,
        annotations: &BTreeMap<DomainDocumentId, Vec<AnnotationThread>>,
        existing: &BTreeMap<CanonicalRelativePath, Vec<u8>>,
        previous_paths: &CanonicalProjectPathMap,
        frontier: &CanonicalPersistenceFrontier,
    ) -> Result<CanonicalProjectEncoding, FormatError> {
        project
            .validate()
            .map_err(|error| FormatError::InvalidManifest(error.to_string()))?;
        let mut frontier = frontier.clone();
        frontier.document_summaries = documents
            .iter()
            .map(|(document, body)| {
                let content_hash = ContentHash::of_bytes(body.as_bytes());
                (
                    *document,
                    CanonicalDocumentSummary {
                        revision: frontier
                            .document_revisions
                            .get(document)
                            .copied()
                            .unwrap_or_default(),
                        content_hash,
                        word_count: body.split_whitespace().count(),
                    },
                )
            })
            .collect();
        finalize_save_identity(project, &mut frontier);
        let (manifest, paths) = domain_manifest(project, &frontier)?;
        let mut resources = BTreeMap::new();
        let control = self.encode(&CanonicalResource::FormatControl(FormatVersion::V1))?;
        resources.insert(control.path.clone(), control);
        let manifest = self.encode(&CanonicalResource::Manifest(CanonicalManifest(manifest)))?;
        resources.insert(manifest.path.clone(), manifest);

        let dictionary_text = project.dictionary.iter().collect::<Vec<_>>().join("\n");
        let dictionary_text = if dictionary_text.is_empty() {
            String::new()
        } else {
            format!("{dictionary_text}\n")
        };
        let dictionary = self.decode_dictionary(dictionary_text.as_bytes())?;
        let dictionary = self.encode(&CanonicalResource::Dictionary(dictionary))?;
        resources.insert(dictionary.path.clone(), dictionary);

        for (path, bytes) in existing {
            if matches!(
                path.as_str(),
                ".parchmint/format-version" | "project.toml" | "dictionary.txt"
            ) || is_document_path(path.as_str())
            {
                continue;
            }
            resources.insert(
                path.clone(),
                CanonicalBytes {
                    resource: resource_for_path(path),
                    path: path.clone(),
                    bytes: bytes.clone(),
                    hash: ContentHash::of_bytes(bytes),
                },
            );
        }
        let existing_styles = existing
            .get(&CanonicalRelativePath::parse("styles.css")?)
            .map(|bytes| self.decode_styles(bytes))
            .transpose()?
            .unwrap_or(CanonicalStyles { rules: Vec::new() });
        let styles = merge_managed_styles(&project.styles, &existing_styles)?;
        let styles = self.encode(&CanonicalResource::Styles(styles))?;
        resources.insert(styles.path.clone(), styles);
        for (document, path) in &paths.documents {
            let body = documents.get(document).ok_or_else(|| {
                FormatError::InvalidDocument(format!(
                    "document {} has no captured body",
                    stable_id_text(document.as_bytes())
                ))
            })?;
            let decoded = self.decode_document(body.as_bytes())?;
            let mut encoded = self.encode(&CanonicalResource::Document(decoded))?;
            encoded.path = path.clone();
            encoded.resource = ResourceId::DocumentById {
                document_id: stable_id_text(document.as_bytes()),
            };
            resources.insert(path.clone(), encoded);
            if let Some(threads) = annotations.get(document) {
                let document_id = stable_id_text(document.as_bytes());
                let sidecar = CanonicalAnnotations::from_typed(document_id, threads)?;
                let encoded = self.encode(&CanonicalResource::Annotations(sidecar))?;
                resources.insert(encoded.path.clone(), encoded);
            }
        }
        let current_paths: BTreeSet<_> = paths.documents.values().cloned().collect();
        let mut deletions: Vec<_> = previous_paths
            .documents
            .values()
            .filter(|path| !current_paths.contains(*path))
            .cloned()
            .collect();
        for path in existing
            .keys()
            .filter(|path| is_annotation_path(path.as_str()))
        {
            let retained = paths.documents.keys().any(|document| {
                path.as_str() == format!("annotations/{}.json", stable_id_text(document.as_bytes()))
            });
            if !retained {
                resources.remove(path);
                deletions.push(path.clone());
            }
        }
        deletions.sort();
        deletions.dedup();
        Ok(CanonicalProjectEncoding {
            resources,
            paths,
            persistence_frontier: frontier,
            deletions,
        })
    }

    /// Encodes only changed domain resources while retaining a complete,
    /// hash-addressed view of the resulting canonical project.
    ///
    /// `existing` may omit document bodies so callers can keep closed
    /// documents lazy. `baseline` and `previous_frontier` must still describe
    /// every durable document; an incomplete baseline fails instead of
    /// silently producing a partial checkpoint.
    pub fn encode_domain_project_patch(
        &self,
        project: &Project,
        update: &CanonicalDomainUpdate,
        existing: &BTreeMap<CanonicalRelativePath, Vec<u8>>,
        baseline: &BTreeMap<CanonicalRelativePath, CanonicalResourceMetadata>,
        previous_paths: &CanonicalProjectPathMap,
        previous_frontier: &CanonicalPersistenceFrontier,
        frontier: &CanonicalPersistenceFrontier,
    ) -> Result<CanonicalProjectPatch, FormatError> {
        project
            .validate()
            .map_err(|error| FormatError::InvalidManifest(error.to_string()))?;
        let mut frontier = frontier.clone();
        let current_documents = project
            .nodes
            .iter()
            .filter_map(|(_, node)| match node.kind {
                NodeKind::Document(document) => Some(document),
                NodeKind::Root(_) | NodeKind::Group => None,
            })
            .collect::<BTreeSet<_>>();
        if let Some(document) = update
            .documents
            .keys()
            .find(|document| !current_documents.contains(document))
        {
            return Err(FormatError::InvalidDocument(format!(
                "updated document {} is not in the project",
                stable_id_text(document.as_bytes())
            )));
        }
        frontier.document_summaries.clear();
        for document in &current_documents {
            let revision = frontier
                .document_revisions
                .get(document)
                .copied()
                .unwrap_or_default();
            let summary = if let Some(update) = update.documents.get(document) {
                CanonicalDocumentSummary {
                    revision,
                    content_hash: ContentHash::of_bytes(update.body.as_bytes()),
                    word_count: update.body.split_whitespace().count(),
                }
            } else {
                let summary = previous_frontier
                    .document_summaries
                    .get(document)
                    .ok_or_else(|| {
                        FormatError::InvalidDocument(format!(
                            "durable baseline is missing summary for unchanged document {}",
                            stable_id_text(document.as_bytes())
                        ))
                    })?
                    .clone();
                if summary.revision != revision {
                    return Err(FormatError::InvalidDocument(format!(
                        "unchanged document {} advanced beyond its durable summary",
                        stable_id_text(document.as_bytes())
                    )));
                }
                summary
            };
            frontier.document_summaries.insert(*document, summary);
        }
        finalize_save_identity(project, &mut frontier);
        let (manifest, paths) = domain_manifest(project, &frontier)?;

        let mut complete_resources = baseline.clone();
        let mut resources = BTreeMap::new();

        let control_path = CanonicalRelativePath::parse(".parchmint/format-version")?;
        if !complete_resources.contains_key(&control_path) {
            record_patch_resource(
                self.encode(&CanonicalResource::FormatControl(FormatVersion::V1))?,
                &mut resources,
                &mut complete_resources,
            );
        }

        let manifest_path = CanonicalRelativePath::parse("project.toml")?;
        if update.manifest
            || update.styles
            || update.dictionary
            || !update.documents.is_empty()
            || !complete_resources.contains_key(&manifest_path)
        {
            record_patch_resource(
                self.encode(&CanonicalResource::Manifest(CanonicalManifest(manifest)))?,
                &mut resources,
                &mut complete_resources,
            );
        }

        let dictionary_path = CanonicalRelativePath::parse("dictionary.txt")?;
        if update.dictionary || !complete_resources.contains_key(&dictionary_path) {
            let dictionary_text = project.dictionary.iter().collect::<Vec<_>>().join("\n");
            let dictionary_text = if dictionary_text.is_empty() {
                String::new()
            } else {
                format!("{dictionary_text}\n")
            };
            let dictionary = self.decode_dictionary(dictionary_text.as_bytes())?;
            record_patch_resource(
                self.encode(&CanonicalResource::Dictionary(dictionary))?,
                &mut resources,
                &mut complete_resources,
            );
        }

        let styles_path = CanonicalRelativePath::parse("styles.css")?;
        if update.styles || !complete_resources.contains_key(&styles_path) {
            let existing_styles = existing
                .get(&styles_path)
                .map(|bytes| self.decode_styles(bytes))
                .transpose()?
                .unwrap_or(CanonicalStyles { rules: Vec::new() });
            let styles = merge_managed_styles(&project.styles, &existing_styles)?;
            record_patch_resource(
                self.encode(&CanonicalResource::Styles(styles))?,
                &mut resources,
                &mut complete_resources,
            );
        }

        for (document, update) in &update.documents {
            let path = paths.documents.get(document).ok_or_else(|| {
                FormatError::InvalidDocument(format!(
                    "updated document {} has no canonical path",
                    stable_id_text(document.as_bytes())
                ))
            })?;
            let decoded = self.decode_document(update.body.as_bytes())?;
            let mut encoded = self.encode(&CanonicalResource::Document(decoded))?;
            encoded.path = path.clone();
            encoded.resource = ResourceId::DocumentById {
                document_id: stable_id_text(document.as_bytes()),
            };
            record_patch_resource(encoded, &mut resources, &mut complete_resources);
            let document_id = stable_id_text(document.as_bytes());
            let sidecar = CanonicalAnnotations::from_typed(document_id, &update.annotations)?;
            record_patch_resource(
                self.encode(&CanonicalResource::Annotations(sidecar))?,
                &mut resources,
                &mut complete_resources,
            );
        }

        let current_paths = paths.documents.values().cloned().collect::<BTreeSet<_>>();
        let mut deletions = previous_paths
            .documents
            .iter()
            .filter(|(document, path)| {
                !current_documents.contains(document) || !current_paths.contains(*path)
            })
            .map(|(_, path)| path.clone())
            .collect::<Vec<_>>();
        for (document, path) in &paths.documents {
            if !complete_resources.contains_key(path) {
                return Err(FormatError::InvalidDocument(format!(
                    "durable baseline is missing unchanged document {}",
                    stable_id_text(document.as_bytes())
                )));
            }
        }
        for document in previous_paths
            .documents
            .keys()
            .filter(|document| !current_documents.contains(document))
        {
            let annotation = annotation_path(&stable_id_text(document.as_bytes()))?;
            if complete_resources.contains_key(&annotation) {
                deletions.push(annotation);
            }
        }
        deletions.sort();
        deletions.dedup();
        for path in &deletions {
            complete_resources.remove(path);
            resources.remove(path);
        }

        Ok(CanonicalProjectPatch {
            resources,
            complete_resources,
            paths,
            persistence_frontier: frontier,
            deletions,
        })
    }

    /// Decodes the structure extension written by [`Self::encode_domain_project`].
    /// Legacy manifests without this extension return `Ok(None)`.
    pub fn decode_domain_project(
        &self,
        manifest: &CanonicalManifest,
        project_id: ProjectId,
    ) -> Result<Option<(Project, CanonicalProjectPathMap)>, FormatError> {
        decode_domain_manifest(manifest.value(), project_id)
    }

    /// Decodes the manifest-owned style identities together with the
    /// stylesheet-owned style properties. Callers loading a complete project
    /// or History checkpoint should use this combined boundary.
    pub fn decode_domain_project_with_styles(
        &self,
        manifest: &CanonicalManifest,
        styles: Option<&CanonicalStyles>,
        project_id: ProjectId,
    ) -> Result<Option<(Project, CanonicalProjectPathMap)>, FormatError> {
        let Some((mut project, paths)) = decode_domain_manifest(manifest.value(), project_id)?
        else {
            return Ok(None);
        };
        if let Some(styles) = styles {
            hydrate_style_properties(&mut project.styles, styles)?;
        }
        Ok(Some((project, paths)))
    }

    pub fn decode_persistence_frontier(
        &self,
        manifest: &CanonicalManifest,
    ) -> Result<CanonicalPersistenceFrontier, FormatError> {
        decode_persistence_frontier(manifest.value())
    }
}

fn record_patch_resource(
    encoded: CanonicalBytes,
    resources: &mut BTreeMap<CanonicalRelativePath, CanonicalBytes>,
    complete_resources: &mut BTreeMap<CanonicalRelativePath, CanonicalResourceMetadata>,
) {
    complete_resources.insert(
        encoded.path.clone(),
        CanonicalResourceMetadata {
            resource: encoded.resource.clone(),
            hash: encoded.hash,
        },
    );
    resources.insert(encoded.path.clone(), encoded);
}

fn finalize_save_identity(project: &Project, frontier: &mut CanonicalPersistenceFrontier) {
    let mut identity = Sha256::new();
    identity.update(frontier.recovery_project_revision.to_be_bytes());
    identity.update(project.revision.value().to_be_bytes());
    for (document, summary) in &frontier.document_summaries {
        identity.update(document.as_bytes());
        identity.update(summary.revision.to_be_bytes());
        identity.update(summary.content_hash.as_bytes());
        identity.update((summary.word_count as u64).to_be_bytes());
    }
    frontier.save_identity = Some(ContentHash::from_bytes(identity.finalize().into()));
}

fn domain_manifest(
    project: &Project,
    frontier: &CanonicalPersistenceFrontier,
) -> Result<(toml::Value, CanonicalProjectPathMap), FormatError> {
    let mut root = toml::map::Map::new();
    let mut project_table = toml::map::Map::new();
    project_table.insert(
        "title".into(),
        toml::Value::String(project.display_title.clone()),
    );
    project_table.insert(
        "spellcheck-language".into(),
        toml::Value::String("en-US".into()),
    );
    project_table.insert(
        "revision".into(),
        toml::Value::Integer(project.revision.value() as i64),
    );
    if let Some(author) = &project.author {
        project_table.insert("author".into(), toml::Value::String(author.clone()));
    }
    project_table.insert(
        "export-excluded".into(),
        toml::Value::Boolean(project.export_settings.excluded),
    );
    project_table.insert(
        "export-emit-titles".into(),
        toml::Value::String(encode_export_setting(project.export_settings.emit_titles).into()),
    );
    project_table.insert(
        "export-starts-new-page".into(),
        toml::Value::Boolean(project.export_settings.starts_new_page),
    );
    root.insert("project".into(), toml::Value::Table(project_table));

    let mut structure = toml::map::Map::new();
    structure.insert("version".into(), toml::Value::Integer(1));
    let mut nodes = Vec::new();
    let mut paths = CanonicalProjectPathMap::default();
    for section in [ProjectSection::Manuscript, ProjectSection::Research] {
        encode_node_children(project, section.root_id(), section, &mut nodes, &mut paths)?;
    }
    structure.insert("nodes".into(), toml::Value::Array(nodes));
    structure.insert(
        "deletions".into(),
        toml::Value::Array(
            project
                .deleted
                .values()
                .map(encode_deletion_tombstone)
                .collect::<Result<Vec<_>, _>>()?,
        ),
    );
    let metadata_fields = project
        .metadata
        .iter()
        .map(|field| {
            use parchmint_domain::{MetadataApplicability, MetadataTextKind};
            let mut value = toml::map::Map::new();
            value.insert(
                "id".into(),
                toml::Value::String(stable_id_text(field.id.as_bytes())),
            );
            value.insert("label".into(), toml::Value::String(field.label.clone()));
            if let Some(description) = &field.description {
                value.insert(
                    "description".into(),
                    toml::Value::String(description.clone()),
                );
            }
            value.insert(
                "applicability".into(),
                toml::Value::String(
                    match field.applicability {
                        MetadataApplicability::Groups => "groups",
                        MetadataApplicability::Documents => "documents",
                        MetadataApplicability::GroupsAndDocuments => "groups-and-documents",
                    }
                    .into(),
                ),
            );
            value.insert(
                "text-kind".into(),
                toml::Value::String(
                    match field.text_kind {
                        MetadataTextKind::SingleLine => "single-line",
                        MetadataTextKind::Multiline => "multiline",
                    }
                    .into(),
                ),
            );
            if let Some(default) = &field.default_value {
                value.insert("default".into(), toml::Value::String(default.clone()));
            }
            value.insert(
                "visible-on-cards".into(),
                toml::Value::Boolean(field.visible_on_cards),
            );
            toml::Value::Table(value)
        })
        .collect();
    structure.insert(
        "metadata-fields".into(),
        toml::Value::Array(metadata_fields),
    );
    root.insert(
        "style-definitions".into(),
        toml::Value::Array(
            project
                .styles
                .iter()
                .enumerate()
                .map(|(order, style)| encode_style_definition(style, order))
                .collect(),
        ),
    );
    root.insert("parchmint-structure".into(), toml::Value::Table(structure));
    let mut persistence = toml::map::Map::new();
    persistence.insert(
        "recovery-project-revision".into(),
        toml::Value::Integer(frontier.recovery_project_revision as i64),
    );
    persistence.insert(
        "document-revisions".into(),
        toml::Value::Table(
            frontier
                .document_revisions
                .iter()
                .map(|(document, revision)| {
                    (
                        stable_id_text(document.as_bytes()),
                        toml::Value::Integer(*revision as i64),
                    )
                })
                .collect(),
        ),
    );
    if let Some(identity) = frontier.save_identity {
        persistence.insert(
            "save-identity".into(),
            toml::Value::String(hex_text(identity.as_bytes())),
        );
    }
    persistence.insert(
        "document-summaries".into(),
        toml::Value::Table(
            frontier
                .document_summaries
                .iter()
                .map(|(document, summary)| {
                    let mut value = toml::map::Map::new();
                    value.insert(
                        "revision".into(),
                        toml::Value::Integer(summary.revision as i64),
                    );
                    value.insert(
                        "content-hash".into(),
                        toml::Value::String(hex_text(summary.content_hash.as_bytes())),
                    );
                    value.insert(
                        "word-count".into(),
                        toml::Value::Integer(summary.word_count as i64),
                    );
                    (
                        stable_id_text(document.as_bytes()),
                        toml::Value::Table(value),
                    )
                })
                .collect(),
        ),
    );
    root.insert(
        "parchmint-persistence".into(),
        toml::Value::Table(persistence),
    );
    Ok((toml::Value::Table(root), paths))
}

fn decode_persistence_frontier(
    value: &toml::Value,
) -> Result<CanonicalPersistenceFrontier, FormatError> {
    let Some(table) = value
        .get("parchmint-persistence")
        .and_then(toml::Value::as_table)
    else {
        return Ok(CanonicalPersistenceFrontier::default());
    };
    let recovery_project_revision = table
        .get("recovery-project-revision")
        .and_then(toml::Value::as_integer)
        .and_then(|revision| u64::try_from(revision).ok())
        .ok_or_else(|| FormatError::InvalidManifest("invalid recovery project revision".into()))?;
    let mut document_revisions = BTreeMap::new();
    for (document, revision) in table
        .get("document-revisions")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flatten()
    {
        let revision = revision
            .as_integer()
            .and_then(|revision| u64::try_from(revision).ok())
            .ok_or_else(|| FormatError::InvalidManifest("invalid document revision".into()))?;
        document_revisions.insert(
            DomainDocumentId::from_bytes(parse_stable_id(document)?),
            revision,
        );
    }
    let save_identity = table
        .get("save-identity")
        .and_then(toml::Value::as_str)
        .map(parse_content_hash)
        .transpose()?;
    let mut document_summaries = BTreeMap::new();
    for (document, value) in table
        .get("document-summaries")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flatten()
    {
        let summary = value
            .as_table()
            .ok_or_else(|| FormatError::InvalidManifest("invalid document summary".into()))?;
        let revision = summary
            .get("revision")
            .and_then(toml::Value::as_integer)
            .and_then(|value| u64::try_from(value).ok())
            .ok_or_else(|| FormatError::InvalidManifest("invalid summary revision".into()))?;
        let content_hash = summary
            .get("content-hash")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| FormatError::InvalidManifest("missing summary content hash".into()))
            .and_then(parse_content_hash)?;
        let word_count = summary
            .get("word-count")
            .and_then(toml::Value::as_integer)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| FormatError::InvalidManifest("invalid summary word count".into()))?;
        let document = DomainDocumentId::from_bytes(parse_stable_id(document)?);
        if revision
            != document_revisions
                .get(&document)
                .copied()
                .unwrap_or_default()
        {
            return Err(FormatError::InvalidManifest(
                "document summary revision does not match frontier".into(),
            ));
        }
        document_summaries.insert(
            document,
            CanonicalDocumentSummary {
                revision,
                content_hash,
                word_count,
            },
        );
    }
    Ok(CanonicalPersistenceFrontier {
        recovery_project_revision,
        document_revisions,
        save_identity,
        document_summaries,
    })
}

fn hex_text(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut text, "{byte:02x}").expect("writing to String cannot fail");
    }
    text
}

fn parse_content_hash(value: &str) -> Result<ContentHash, FormatError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(FormatError::InvalidManifest(
            "content hash is not 64 hexadecimal digits".into(),
        ));
    }
    let mut bytes = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = (hex_digit(pair[0])? << 4) | hex_digit(pair[1])?;
    }
    Ok(ContentHash::from_bytes(bytes))
}

fn hex_digit(byte: u8) -> Result<u8, FormatError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(FormatError::InvalidManifest(
            "invalid hexadecimal digit".into(),
        )),
    }
}

fn encode_node_children(
    project: &Project,
    parent: NodeId,
    section: ProjectSection,
    output: &mut Vec<toml::Value>,
    paths: &mut CanonicalProjectPathMap,
) -> Result<(), FormatError> {
    for (order, id) in project.nodes.children(parent).iter().copied().enumerate() {
        let node = project.nodes.get(id).ok_or_else(|| {
            FormatError::InvalidManifest("project tree references a missing node".into())
        })?;
        let mut value = toml::map::Map::new();
        value.insert(
            "id".into(),
            toml::Value::String(stable_id_text(id.as_bytes())),
        );
        value.insert(
            "parent".into(),
            toml::Value::String(stable_id_text(parent.as_bytes())),
        );
        value.insert("order".into(), toml::Value::Integer(order as i64));
        value.insert("title".into(), toml::Value::String(node.title.clone()));
        value.insert(
            "synopsis".into(),
            toml::Value::String(node.synopsis.clone()),
        );
        value.insert(
            "export-excluded".into(),
            toml::Value::Boolean(node.export_settings.excluded),
        );
        value.insert(
            "export-emit-titles".into(),
            toml::Value::String(encode_export_setting(node.export_settings.emit_titles).into()),
        );
        value.insert(
            "export-starts-new-page".into(),
            toml::Value::Boolean(node.export_settings.starts_new_page),
        );
        let metadata = node
            .metadata
            .iter()
            .map(|(field, value)| {
                (
                    stable_id_text(field.as_bytes()),
                    toml::Value::String(value.clone()),
                )
            })
            .collect();
        value.insert("metadata".into(), toml::Value::Table(metadata));
        match node.kind {
            NodeKind::Group => {
                value.insert("kind".into(), toml::Value::String("group".into()));
            }
            NodeKind::Document(document) => {
                value.insert("kind".into(), toml::Value::String("document".into()));
                value.insert(
                    "document-id".into(),
                    toml::Value::String(stable_id_text(document.as_bytes())),
                );
                let directory = match section {
                    ProjectSection::Manuscript => "manuscript",
                    ProjectSection::Research => "research",
                };
                let path = CanonicalRelativePath::parse(format!(
                    "{directory}/{}.html",
                    stable_id_text(document.as_bytes())
                ))?;
                value.insert("path".into(), toml::Value::String(path.as_str().into()));
                paths.documents.insert(document, path);
            }
            NodeKind::Root(_) => {
                return Err(FormatError::InvalidManifest(
                    "fixed project roots cannot be nested".into(),
                ));
            }
        }
        output.push(toml::Value::Table(value));
        if node.kind == NodeKind::Group {
            encode_node_children(project, id, section, output, paths)?;
        }
    }
    Ok(())
}

fn encode_deletion_tombstone(tombstone: &DeletionTombstone) -> Result<toml::Value, FormatError> {
    let mut value = toml::map::Map::new();
    value.insert(
        "node-id".into(),
        toml::Value::String(stable_id_text(tombstone.node_id.as_bytes())),
    );
    value.insert("title".into(), toml::Value::String(tombstone.title.clone()));
    encode_node_kind(&mut value, tombstone.kind);
    value.insert(
        "section".into(),
        toml::Value::String(
            match tombstone.section {
                ProjectSection::Manuscript => "manuscript",
                ProjectSection::Research => "research",
            }
            .into(),
        ),
    );
    value.insert(
        "former-parent".into(),
        toml::Value::String(stable_id_text(tombstone.former_parent.as_bytes())),
    );
    value.insert(
        "former-index".into(),
        toml::Value::Integer(i64::try_from(tombstone.former_index).map_err(|_| {
            FormatError::InvalidManifest("deleted item order exceeds the format limit".into())
        })?),
    );
    value.insert(
        "deleted-at-unix-millis".into(),
        toml::Value::Integer(
            i64::try_from(tombstone.deleted_at_unix_millis).map_err(|_| {
                FormatError::InvalidManifest("deletion time exceeds the format limit".into())
            })?,
        ),
    );
    if let Some(checkpoint) = tombstone.restoring_checkpoint {
        value.insert(
            "restoring-checkpoint".into(),
            toml::Value::String(stable_id_text(checkpoint.as_bytes())),
        );
    }
    value.insert(
        "subtree".into(),
        toml::Value::Array(tombstone.subtree.iter().map(encode_deleted_node).collect()),
    );
    Ok(toml::Value::Table(value))
}

fn encode_deleted_node(snapshot: &DeletedNodeSnapshot) -> toml::Value {
    let mut value = encode_project_node(&snapshot.node);
    if let Some(parent) = snapshot.parent {
        value.insert(
            "parent".into(),
            toml::Value::String(stable_id_text(parent.as_bytes())),
        );
    }
    value.insert(
        "children".into(),
        toml::Value::Array(
            snapshot
                .children
                .iter()
                .map(|child| toml::Value::String(stable_id_text(child.as_bytes())))
                .collect(),
        ),
    );
    toml::Value::Table(value)
}

fn encode_project_node(node: &ProjectNode) -> toml::Table {
    let mut value = toml::map::Map::new();
    value.insert(
        "id".into(),
        toml::Value::String(stable_id_text(node.id.as_bytes())),
    );
    value.insert("title".into(), toml::Value::String(node.title.clone()));
    value.insert(
        "synopsis".into(),
        toml::Value::String(node.synopsis.clone()),
    );
    value.insert(
        "export-excluded".into(),
        toml::Value::Boolean(node.export_settings.excluded),
    );
    value.insert(
        "export-emit-titles".into(),
        toml::Value::String(encode_export_setting(node.export_settings.emit_titles).into()),
    );
    value.insert(
        "export-starts-new-page".into(),
        toml::Value::Boolean(node.export_settings.starts_new_page),
    );
    value.insert(
        "metadata".into(),
        toml::Value::Table(
            node.metadata
                .iter()
                .map(|(field, text)| {
                    (
                        stable_id_text(field.as_bytes()),
                        toml::Value::String(text.clone()),
                    )
                })
                .collect(),
        ),
    );
    encode_node_kind(&mut value, node.kind);
    value
}

fn encode_node_kind(value: &mut toml::Table, kind: NodeKind) {
    match kind {
        NodeKind::Root(section) => {
            value.insert("kind".into(), toml::Value::String("root".into()));
            value.insert(
                "root-section".into(),
                toml::Value::String(
                    match section {
                        ProjectSection::Manuscript => "manuscript",
                        ProjectSection::Research => "research",
                    }
                    .into(),
                ),
            );
        }
        NodeKind::Group => {
            value.insert("kind".into(), toml::Value::String("group".into()));
        }
        NodeKind::Document(document) => {
            value.insert("kind".into(), toml::Value::String("document".into()));
            value.insert(
                "document-id".into(),
                toml::Value::String(stable_id_text(document.as_bytes())),
            );
        }
    }
}

fn decode_domain_manifest(
    value: &toml::Value,
    project_id: ProjectId,
) -> Result<Option<(Project, CanonicalProjectPathMap)>, FormatError> {
    use parchmint_domain::{
        MetadataApplicability, MetadataFieldDefinition, MetadataFieldId, MetadataTextKind,
    };
    let Some(structure) = value
        .get("parchmint-structure")
        .and_then(toml::Value::as_table)
    else {
        return Ok(None);
    };
    if structure.get("version").and_then(toml::Value::as_integer) != Some(1) {
        return Err(FormatError::InvalidManifest(
            "unsupported parchmint structure version".into(),
        ));
    }
    let project_value = value.get("project").and_then(toml::Value::as_table);
    let mut project = Project::new(project_id);
    project.display_title = project_value
        .and_then(|table| table.get("title"))
        .and_then(toml::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    project.author = project_value
        .and_then(|table| table.get("author"))
        .and_then(toml::Value::as_str)
        .map(str::to_owned);
    project.export_settings = ProjectExportSettings {
        excluded: project_value
            .and_then(|table| table.get("export-excluded"))
            .and_then(toml::Value::as_bool)
            .unwrap_or(false),
        emit_titles: decode_export_setting(project_value.and_then(|table| {
            table
                .get("export-emit-titles")
                .and_then(toml::Value::as_str)
        }))?,
        starts_new_page: project_value
            .and_then(|table| table.get("export-starts-new-page"))
            .and_then(toml::Value::as_bool)
            .unwrap_or(false),
    };
    project.revision = project_value
        .and_then(|table| table.get("revision"))
        .and_then(toml::Value::as_integer)
        .and_then(|revision| u64::try_from(revision).ok())
        .map(Into::into)
        .unwrap_or_default();
    if let Some(style_values) = value.get("style-definitions") {
        let style_values = style_values.as_array().ok_or_else(|| {
            FormatError::InvalidManifest("style-definitions must be an array".into())
        })?;
        let mut ordered = BTreeMap::new();
        for value in style_values {
            let table = required_table(value, "style definition")?;
            let order = table
                .get("order")
                .and_then(toml::Value::as_integer)
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| {
                    FormatError::InvalidManifest("style definition order is invalid".into())
                })?;
            if ordered
                .insert(order, decode_style_definition(table)?)
                .is_some()
            {
                return Err(FormatError::InvalidManifest(
                    "duplicate style definition order".into(),
                ));
            }
        }
        if ordered.keys().copied().ne(0..ordered.len()) {
            return Err(FormatError::InvalidManifest(
                "style definition order must be contiguous".into(),
            ));
        }
        project.styles = StyleCatalog::from_definitions(ordered.into_values())
            .map_err(|error| FormatError::InvalidManifest(error.to_string()))?;
    }
    for field in structure
        .get("metadata-fields")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
    {
        let table = required_table(field, "metadata field")?;
        let applicability = match required_str(table, "applicability")? {
            "groups" => MetadataApplicability::Groups,
            "documents" => MetadataApplicability::Documents,
            "groups-and-documents" => MetadataApplicability::GroupsAndDocuments,
            _ => {
                return Err(FormatError::InvalidManifest(
                    "invalid metadata applicability".into(),
                ));
            }
        };
        let text_kind = match required_str(table, "text-kind")? {
            "single-line" => MetadataTextKind::SingleLine,
            "multiline" => MetadataTextKind::Multiline,
            _ => {
                return Err(FormatError::InvalidManifest(
                    "invalid metadata text kind".into(),
                ));
            }
        };
        project
            .metadata
            .upsert(MetadataFieldDefinition {
                id: MetadataFieldId::from_bytes(parse_stable_id(required_str(table, "id")?)?),
                label: required_str(table, "label")?.to_owned(),
                description: table
                    .get("description")
                    .and_then(toml::Value::as_str)
                    .map(str::to_owned),
                applicability,
                text_kind,
                default_value: table
                    .get("default")
                    .and_then(toml::Value::as_str)
                    .map(str::to_owned),
                visible_on_cards: table
                    .get("visible-on-cards")
                    .and_then(toml::Value::as_bool)
                    .unwrap_or(false),
            })
            .map_err(|error| FormatError::InvalidManifest(error.to_string()))?;
    }
    let mut paths = CanonicalProjectPathMap::default();
    for node in structure
        .get("nodes")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
    {
        let table = required_table(node, "project node")?;
        let id = NodeId::from_bytes(parse_stable_id(required_str(table, "id")?)?);
        let parent = NodeId::from_bytes(parse_stable_id(required_str(table, "parent")?)?);
        let order = table
            .get("order")
            .and_then(toml::Value::as_integer)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| FormatError::InvalidManifest("node order is invalid".into()))?;
        let title = required_str(table, "title")?;
        match required_str(table, "kind")? {
            "group" => project.nodes.try_insert_group(id, parent, order, title),
            "document" => {
                let document = DomainDocumentId::from_bytes(parse_stable_id(required_str(
                    table,
                    "document-id",
                )?)?);
                let path = CanonicalRelativePath::parse(required_str(table, "path")?)?;
                paths.documents.insert(document, path);
                project
                    .nodes
                    .try_insert_document(id, document, parent, order, title)
            }
            _ => return Err(FormatError::InvalidManifest("node kind is invalid".into())),
        }
        .map_err(|error| FormatError::InvalidManifest(error.to_string()))?;
        let inserted = project.nodes.get_mut(id).expect("inserted node exists");
        inserted.synopsis = table
            .get("synopsis")
            .and_then(toml::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        inserted.export_settings = ProjectExportSettings {
            excluded: table
                .get("export-excluded")
                .and_then(toml::Value::as_bool)
                .unwrap_or(false),
            emit_titles: decode_export_setting(
                table
                    .get("export-emit-titles")
                    .and_then(toml::Value::as_str),
            )?,
            starts_new_page: table
                .get("export-starts-new-page")
                .and_then(toml::Value::as_bool)
                .unwrap_or(false),
        };
        if let Some(metadata) = table.get("metadata").and_then(toml::Value::as_table) {
            for (field, value) in metadata {
                let field = MetadataFieldId::from_bytes(parse_stable_id(field)?);
                let value = value.as_str().ok_or_else(|| {
                    FormatError::InvalidManifest("metadata value is not text".into())
                })?;
                inserted.metadata.insert(field, value.to_owned());
            }
        }
    }
    for deletion in structure
        .get("deletions")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
    {
        let tombstone = decode_deletion_tombstone(required_table(deletion, "deletion")?)?;
        if project
            .deleted
            .insert(tombstone.node_id, tombstone)
            .is_some()
        {
            return Err(FormatError::InvalidManifest(
                "duplicate deletion tombstone ID".into(),
            ));
        }
    }
    project
        .validate()
        .map_err(|error| FormatError::InvalidManifest(error.to_string()))?;
    Ok(Some((project, paths)))
}

fn decode_deletion_tombstone(table: &toml::Table) -> Result<DeletionTombstone, FormatError> {
    let node_id = NodeId::from_bytes(parse_stable_id(required_str(table, "node-id")?)?);
    let kind = decode_node_kind(table)?;
    let section = decode_section(required_str(table, "section")?)?;
    let former_parent = NodeId::from_bytes(parse_stable_id(required_str(table, "former-parent")?)?);
    let former_index = table
        .get("former-index")
        .and_then(toml::Value::as_integer)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| FormatError::InvalidManifest("former deletion order is invalid".into()))?;
    let deleted_at_unix_millis = table
        .get("deleted-at-unix-millis")
        .and_then(toml::Value::as_integer)
        .and_then(|value| u64::try_from(value).ok())
        .ok_or_else(|| FormatError::InvalidManifest("deletion time is invalid".into()))?;
    let restoring_checkpoint = table
        .get("restoring-checkpoint")
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| {
                    FormatError::InvalidManifest("restoring checkpoint is invalid".into())
                })
                .and_then(parse_stable_id)
                .map(CheckpointId::from_bytes)
        })
        .transpose()?;
    let subtree = table
        .get("subtree")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| FormatError::InvalidManifest("deleted subtree is missing".into()))?
        .iter()
        .map(|value| decode_deleted_node(required_table(value, "deleted node")?))
        .collect::<Result<Vec<_>, _>>()?;
    let root = subtree.first().ok_or_else(|| {
        FormatError::InvalidManifest("deleted subtree must contain its root".into())
    })?;
    if root.node.id != node_id || root.node.kind != kind {
        return Err(FormatError::InvalidManifest(
            "deletion tombstone does not match its subtree root".into(),
        ));
    }
    Ok(DeletionTombstone {
        node_id,
        title: required_str(table, "title")?.to_owned(),
        kind,
        section,
        former_parent,
        former_index,
        deleted_at_unix_millis,
        restoring_checkpoint,
        subtree,
    })
}

fn decode_deleted_node(table: &toml::Table) -> Result<DeletedNodeSnapshot, FormatError> {
    let id = NodeId::from_bytes(parse_stable_id(required_str(table, "id")?)?);
    let parent = table
        .get("parent")
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| {
                    FormatError::InvalidManifest("deleted node parent is invalid".into())
                })
                .and_then(parse_stable_id)
                .map(NodeId::from_bytes)
        })
        .transpose()?;
    let children = table
        .get("children")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| FormatError::InvalidManifest("deleted node children are missing".into()))?
        .iter()
        .map(|child| {
            child
                .as_str()
                .ok_or_else(|| FormatError::InvalidManifest("deleted child ID is invalid".into()))
                .and_then(parse_stable_id)
                .map(NodeId::from_bytes)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let title = required_str(table, "title")?;
    let mut node = match decode_node_kind(table)? {
        NodeKind::Group => ProjectNode::group(id, title),
        NodeKind::Document(document) => ProjectNode::document(id, document, title),
        NodeKind::Root(_) => {
            return Err(FormatError::InvalidManifest(
                "fixed roots cannot appear in a deleted subtree".into(),
            ));
        }
    };
    node.synopsis = table
        .get("synopsis")
        .and_then(toml::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    node.export_settings = ProjectExportSettings {
        excluded: table
            .get("export-excluded")
            .and_then(toml::Value::as_bool)
            .unwrap_or(false),
        emit_titles: decode_export_setting(
            table
                .get("export-emit-titles")
                .and_then(toml::Value::as_str),
        )?,
        starts_new_page: table
            .get("export-starts-new-page")
            .and_then(toml::Value::as_bool)
            .unwrap_or(false),
    };
    if let Some(metadata) = table.get("metadata").and_then(toml::Value::as_table) {
        for (field, value) in metadata {
            let field = parchmint_domain::MetadataFieldId::from_bytes(parse_stable_id(field)?);
            let text = value.as_str().ok_or_else(|| {
                FormatError::InvalidManifest("deleted node metadata is not text".into())
            })?;
            node.metadata.insert(field, text.to_owned());
        }
    }
    Ok(DeletedNodeSnapshot {
        node,
        parent,
        children,
    })
}

fn decode_node_kind(table: &toml::Table) -> Result<NodeKind, FormatError> {
    match required_str(table, "kind")? {
        "root" => Ok(NodeKind::Root(decode_section(required_str(
            table,
            "root-section",
        )?)?)),
        "group" => Ok(NodeKind::Group),
        "document" => Ok(NodeKind::Document(DomainDocumentId::from_bytes(
            parse_stable_id(required_str(table, "document-id")?)?,
        ))),
        _ => Err(FormatError::InvalidManifest("node kind is invalid".into())),
    }
}

fn decode_section(value: &str) -> Result<ProjectSection, FormatError> {
    match value {
        "manuscript" => Ok(ProjectSection::Manuscript),
        "research" => Ok(ProjectSection::Research),
        _ => Err(FormatError::InvalidManifest(
            "project section is invalid".into(),
        )),
    }
}

fn required_table<'a>(
    value: &'a toml::Value,
    field: &'static str,
) -> Result<&'a toml::Table, FormatError> {
    value
        .as_table()
        .ok_or_else(|| FormatError::InvalidManifest(format!("{field} is not a table")))
}

fn required_str<'a>(table: &'a toml::Table, field: &'static str) -> Result<&'a str, FormatError> {
    table
        .get(field)
        .and_then(toml::Value::as_str)
        .ok_or_else(|| FormatError::InvalidManifest(format!("{field} is missing or invalid")))
}

fn encode_style_definition(style: &StyleDefinition, order: usize) -> toml::Value {
    let mut value = toml::map::Map::new();
    value.insert(
        "id".into(),
        toml::Value::String(stable_id_text(style.id.as_bytes())),
    );
    value.insert(
        "display-name".into(),
        toml::Value::String(style.display_name.clone()),
    );
    value.insert(
        "role".into(),
        toml::Value::String(style_role_text(style.role).into()),
    );
    if let Some(parent) = style.inherits {
        value.insert(
            "inherits".into(),
            toml::Value::String(stable_id_text(parent.as_bytes())),
        );
    }
    value.insert("order".into(), toml::Value::Integer(order as i64));
    toml::Value::Table(value)
}

fn decode_style_definition(table: &toml::Table) -> Result<StyleDefinition, FormatError> {
    let role = match required_str(table, "role")? {
        "body" => StyleRole::Body,
        "document-title" => StyleRole::DocumentTitle,
        "heading-1" => StyleRole::Heading1,
        "heading-2" => StyleRole::Heading2,
        "heading-3" => StyleRole::Heading3,
        "block-quote" => StyleRole::BlockQuote,
        "verse" => StyleRole::Verse,
        "custom" => StyleRole::Custom,
        _ => return Err(FormatError::InvalidManifest("invalid style role".into())),
    };
    Ok(StyleDefinition {
        id: StyleId::from_bytes(parse_stable_id(required_str(table, "id")?)?),
        display_name: required_str(table, "display-name")?.to_owned(),
        role,
        inherits: table
            .get("inherits")
            .map(|value| {
                value
                    .as_str()
                    .ok_or_else(|| {
                        FormatError::InvalidManifest("style inherits must be an ID".into())
                    })
                    .and_then(parse_stable_id)
                    .map(StyleId::from_bytes)
            })
            .transpose()?,
        properties: StyleProperties::default(),
    })
}

fn style_role_text(role: StyleRole) -> &'static str {
    match role {
        StyleRole::Body => "body",
        StyleRole::DocumentTitle => "document-title",
        StyleRole::Heading1 => "heading-1",
        StyleRole::Heading2 => "heading-2",
        StyleRole::Heading3 => "heading-3",
        StyleRole::BlockQuote => "block-quote",
        StyleRole::Verse => "verse",
        StyleRole::Custom => "custom",
    }
}

fn encode_export_setting(setting: ProjectExportSetting) -> &'static str {
    match setting {
        ProjectExportSetting::Inherit => "inherit",
        ProjectExportSetting::Enabled => "enabled",
        ProjectExportSetting::Disabled => "disabled",
    }
}

fn decode_export_setting(value: Option<&str>) -> Result<ProjectExportSetting, FormatError> {
    match value.unwrap_or("inherit") {
        "inherit" => Ok(ProjectExportSetting::Inherit),
        "enabled" => Ok(ProjectExportSetting::Enabled),
        "disabled" => Ok(ProjectExportSetting::Disabled),
        _ => Err(FormatError::InvalidManifest(
            "export title setting must be inherit, enabled, or disabled".into(),
        )),
    }
}

fn stable_id_text(bytes: &[u8; 16]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn parse_stable_id(value: &str) -> Result<[u8; 16], FormatError> {
    if value.len() != 32 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(FormatError::InvalidManifest(
            "stable ID is not 32 hexadecimal digits".into(),
        ));
    }
    let mut bytes = [0; 16];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|error| FormatError::InvalidManifest(error.to_string()))?;
    }
    Ok(bytes)
}

fn resource_for_path(path: &CanonicalRelativePath) -> ResourceId {
    match path.as_str() {
        ".parchmint/format-version" => ResourceId::FormatControl,
        "project.toml" => ResourceId::Manifest,
        "styles.css" => ResourceId::Styles,
        "dictionary.txt" => ResourceId::Dictionary,
        path if path.starts_with("annotations/") && path.ends_with(".json") => {
            ResourceId::Annotations {
                document_id: path
                    .trim_start_matches("annotations/")
                    .trim_end_matches(".json")
                    .to_owned(),
            }
        }
        _ => ResourceId::Document,
    }
}

impl CanonicalCodec for ProjectFormatCodec {
    fn detect(&self, control: &[u8]) -> Result<FormatVersion, FormatError> {
        if control == FORMAT_CONTROL_V1 {
            Ok(FormatVersion::V1)
        } else {
            Err(FormatError::InvalidFormatControl)
        }
    }

    fn decode_project(&self, input: CanonicalInputSet) -> Result<ProjectModel, FormatError> {
        let format_control = input
            .format_control
            .as_deref()
            .ok_or(FormatError::MissingFormatControl)?;
        let format_version = self.detect(format_control)?;
        validate_paths(input.resources.keys())?;
        if !input
            .resources
            .contains_key(&CanonicalRelativePath::parse("project.toml")?)
        {
            return Err(FormatError::MissingManifest);
        }

        let mut resources = BTreeMap::new();
        for (path, bytes) in input.resources {
            let resource = match path.as_str() {
                "project.toml" => CanonicalResource::Manifest(self.decode_manifest(&bytes)?),
                "styles.css" => CanonicalResource::Styles(self.decode_styles(&bytes)?),
                "dictionary.txt" => CanonicalResource::Dictionary(self.decode_dictionary(&bytes)?),
                path if is_document_path(path) => {
                    CanonicalResource::Document(self.decode_document(&bytes)?)
                }
                path if is_annotation_path(path) => {
                    let annotations = self.decode_annotations(&bytes)?;
                    if annotation_path(annotations.document_id())?.as_str() != path {
                        return Err(FormatError::InvalidAnnotations(
                            "annotation filename does not match its document ID".into(),
                        ));
                    }
                    CanonicalResource::Annotations(annotations)
                }
                _ => {
                    return Err(FormatError::UnsupportedResource {
                        path: path.to_string(),
                    });
                }
            };
            resources.insert(path, resource);
        }
        Ok(ProjectModel {
            format_version,
            resources,
        })
    }

    fn decode_document(&self, bytes: &[u8]) -> Result<CanonicalDocument, FormatError> {
        let html = utf8(bytes, "document")?;
        parse_html(html).map(|nodes| CanonicalDocument {
            html: render_html(&nodes),
        })
    }

    fn decode_annotations(&self, bytes: &[u8]) -> Result<CanonicalAnnotations, FormatError> {
        let sidecar: AnnotationSidecarV1 = serde_json::from_slice(bytes)
            .map_err(|error| FormatError::InvalidAnnotations(error.to_string()))?;
        if sidecar.schema != ANNOTATION_SCHEMA_V1 {
            return Err(FormatError::InvalidAnnotations("unknown schema".into()));
        }
        if sidecar.document_id.is_empty() || sidecar.document_id.chars().any(char::is_control) {
            return Err(FormatError::InvalidAnnotations(
                "document ID is empty or unsafe".into(),
            ));
        }
        let annotations = CanonicalAnnotations(canonicalize_annotations(sidecar)?);
        let threads = annotations.typed_threads()?;
        let mut thread_ids = BTreeSet::new();
        let mut message_ids = BTreeSet::new();
        for thread in threads {
            if !thread_ids.insert(thread.id) {
                return Err(FormatError::InvalidAnnotations(
                    "duplicate thread ID".into(),
                ));
            }
            for message in thread.messages {
                if !message_ids.insert(message.id) {
                    return Err(FormatError::InvalidAnnotations(
                        "duplicate message ID".into(),
                    ));
                }
            }
        }
        Ok(annotations)
    }

    fn encode(&self, value: &CanonicalResource) -> Result<CanonicalBytes, FormatError> {
        let (resource, path, bytes) = match value {
            CanonicalResource::FormatControl(version) => (
                ResourceId::FormatControl,
                CanonicalRelativePath::parse(".parchmint/format-version")?,
                version.control_bytes().to_vec(),
            ),
            CanonicalResource::Manifest(manifest) => (
                ResourceId::Manifest,
                CanonicalRelativePath::parse("project.toml")?,
                canonical_toml(&manifest.0)?.into_bytes(),
            ),
            CanonicalResource::Styles(styles) => (
                ResourceId::Styles,
                CanonicalRelativePath::parse("styles.css")?,
                styles.as_css().into_bytes(),
            ),
            CanonicalResource::Dictionary(dictionary) => (
                ResourceId::Dictionary,
                CanonicalRelativePath::parse("dictionary.txt")?,
                render_dictionary(dictionary).into_bytes(),
            ),
            CanonicalResource::Document(document) => (
                ResourceId::Document,
                CanonicalRelativePath::parse("manuscript/document.html")?,
                document.html.as_bytes().to_vec(),
            ),
            CanonicalResource::Annotations(annotations) => (
                ResourceId::Annotations {
                    document_id: annotations.document_id().to_owned(),
                },
                annotation_path(annotations.document_id())?,
                canonical_json(&annotations.0)?.into_bytes(),
            ),
        };
        let hash = ContentHash::of_bytes(&bytes);
        Ok(CanonicalBytes {
            resource,
            path,
            bytes,
            hash,
        })
    }

    fn migrate(
        &self,
        source: SourceFormatSnapshot,
        target: FormatVersion,
    ) -> Result<CanonicalResourceSet, MigrationError> {
        if target != FormatVersion::V1 {
            return Err(MigrationError::UnsupportedTarget(target));
        }
        let project = self.decode_project(CanonicalInputSet {
            format_control: Some(source.format_control),
            resources: source.resources,
        })?;
        let mut resources = BTreeMap::new();
        let control = self.encode(&CanonicalResource::FormatControl(target))?;
        resources.insert(control.path.clone(), control);
        for (path, resource) in project.resources {
            let mut encoded = self.encode(&resource)?;
            encoded.path = path.clone();
            resources.insert(path, encoded);
        }
        validate_paths(resources.keys())?;
        Ok(CanonicalResourceSet {
            format_version: target,
            resources,
        })
    }
}

fn utf8<'a>(bytes: &'a [u8], resource: &'static str) -> Result<&'a str, FormatError> {
    std::str::from_utf8(bytes).map_err(|_| FormatError::NonUtf8 { resource })
}

fn validate_paths<'a>(
    paths: impl IntoIterator<Item = &'a CanonicalRelativePath>,
) -> Result<(), FormatError> {
    let mut portable_paths = BTreeMap::new();
    for path in paths {
        let folded = path.as_str().to_lowercase();
        if let Some(first) = portable_paths.insert(folded, path.as_str()) {
            return Err(FormatError::PathCollision {
                first: first.to_owned(),
                second: path.as_str().to_owned(),
            });
        }
    }
    Ok(())
}

fn is_document_path(path: &str) -> bool {
    (path.starts_with("manuscript/") || path.starts_with("research/")) && path.ends_with(".html")
}

fn is_annotation_path(path: &str) -> bool {
    path.starts_with("annotations/") && path.ends_with(".json")
}

fn annotation_path(document_id: &str) -> Result<CanonicalRelativePath, FormatError> {
    if document_id.contains('/') || document_id.contains('\\') {
        return Err(FormatError::InvalidAnnotations(
            "document ID cannot contain a path separator".into(),
        ));
    }
    CanonicalRelativePath::parse(format!("annotations/{document_id}.json"))
}

fn is_combining_mark(character: char) -> bool {
    matches!(character as u32, 0x0300..=0x036f | 0x1ab0..=0x1aff | 0x1dc0..=0x1dff | 0x20d0..=0x20ff | 0xfe20..=0xfe2f)
}

fn is_windows_device_name(segment: &str) -> bool {
    let stem = segment.split('.').next().unwrap_or_default();
    let upper = stem.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || upper
            .strip_prefix("COM")
            .or_else(|| upper.strip_prefix("LPT"))
            .is_some_and(|number| {
                matches!(number, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            })
}

fn canonical_toml(value: &toml::Value) -> Result<String, FormatError> {
    let mut text = toml::to_string_pretty(value)
        .map_err(|error| FormatError::InvalidManifest(error.to_string()))?;
    text = text.replace("\r\n", "\n");
    if !text.ends_with('\n') {
        text.push('\n');
    }
    Ok(text)
}

fn canonical_json(value: &AnnotationSidecarV1) -> Result<String, FormatError> {
    let value = serde_json::to_value(value)
        .map_err(|error| FormatError::InvalidAnnotations(error.to_string()))?;
    let canonical = sort_json(value);
    let mut text = serde_json::to_string_pretty(&canonical)
        .map_err(|error| FormatError::InvalidAnnotations(error.to_string()))?;
    text.push('\n');
    Ok(text)
}

fn canonicalize_annotations(
    value: AnnotationSidecarV1,
) -> Result<AnnotationSidecarV1, FormatError> {
    let canonical_threads = value
        .threads
        .into_iter()
        .map(|thread| validate_annotation_value(sort_json(thread)))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(AnnotationSidecarV1 {
        schema: ANNOTATION_SCHEMA_V1.to_owned(),
        document_id: value.document_id,
        threads: canonical_threads,
    })
}

fn decode_thread(value: &serde_json::Value) -> Result<AnnotationThread, FormatError> {
    let mut object = annotation_object(value, "thread")?.clone();
    let id = annotation_id(take_string(&mut object, "id")?)?;
    let resolved = take_bool(&mut object, "resolved")?;
    let messages = take_array(&mut object, "messages")?
        .iter()
        .map(decode_message)
        .collect::<Result<Vec<_>, _>>()?;
    if messages.is_empty() {
        return annotation_error("thread messages cannot be empty");
    }
    let anchor = decode_comment_anchor(&take_value(&mut object, "anchor")?)?;
    Ok(AnnotationThread {
        id,
        messages,
        resolved,
        anchor,
        unknown_fields: decode_unknown(object)?,
    })
}

fn decode_message(value: &serde_json::Value) -> Result<AnnotationMessage, FormatError> {
    let mut object = annotation_object(value, "message")?.clone();
    let id = annotation_id(take_string(&mut object, "id")?)?;
    let body = take_string(&mut object, "body")?;
    if body.trim().is_empty() {
        return annotation_error("comment body must contain text");
    }
    Ok(AnnotationMessage {
        id,
        body,
        unknown_fields: decode_unknown(object)?,
    })
}

fn decode_comment_anchor(value: &serde_json::Value) -> Result<AnnotationAnchor, FormatError> {
    let mut object = annotation_object(value, "anchor")?.clone();
    match take_string(&mut object, "kind")?.as_str() {
        "document" => Ok(AnnotationAnchor::Document {
            unknown_fields: decode_unknown(object)?,
        }),
        "range" | "position" => {
            let block = annotation_id(take_string(&mut object, "block_id")?)?;
            let start = take_u64(&mut object, "start")?;
            let end = take_u64(&mut object, "end")?;
            if end < start {
                return annotation_error("anchor end precedes start");
            }
            let quote = take_string(&mut object, "quote")?;
            let context_before = take_string(&mut object, "context_before")?;
            let context_after = take_string(&mut object, "context_after")?;
            let orphaned = take_bool(&mut object, "orphaned")?;
            Ok(AnnotationAnchor::Text {
                block,
                start,
                end,
                quote,
                context_before,
                context_after,
                orphaned,
                unknown_fields: decode_unknown(object)?,
            })
        }
        _ => annotation_error("anchor kind is unknown"),
    }
}

fn encode_thread(thread: &AnnotationThread) -> Result<serde_json::Value, FormatError> {
    let mut object = encode_unknown(&thread.unknown_fields)?;
    object.insert("id".into(), stable_id_text(&thread.id).into());
    object.insert("resolved".into(), thread.resolved.into());
    object.insert(
        "messages".into(),
        serde_json::Value::Array(
            thread
                .messages
                .iter()
                .map(encode_message)
                .collect::<Result<_, _>>()?,
        ),
    );
    object.insert("anchor".into(), encode_comment_anchor(&thread.anchor)?);
    Ok(serde_json::Value::Object(object))
}

fn encode_message(message: &AnnotationMessage) -> Result<serde_json::Value, FormatError> {
    let mut object = encode_unknown(&message.unknown_fields)?;
    object.insert("id".into(), stable_id_text(&message.id).into());
    object.insert("body".into(), message.body.clone().into());
    Ok(serde_json::Value::Object(object))
}

fn encode_comment_anchor(anchor: &AnnotationAnchor) -> Result<serde_json::Value, FormatError> {
    let (mut object, kind) = match anchor {
        AnnotationAnchor::Document { unknown_fields } => {
            (encode_unknown(unknown_fields)?, "document")
        }
        AnnotationAnchor::Text {
            block,
            start,
            end,
            quote,
            context_before,
            context_after,
            orphaned,
            unknown_fields,
        } => {
            let mut object = encode_unknown(unknown_fields)?;
            object.insert("block_id".into(), stable_id_text(block).into());
            object.insert("start".into(), (*start).into());
            object.insert("end".into(), (*end).into());
            object.insert("quote".into(), quote.clone().into());
            object.insert("context_before".into(), context_before.clone().into());
            object.insert("context_after".into(), context_after.clone().into());
            object.insert("orphaned".into(), (*orphaned).into());
            (object, if start == end { "position" } else { "range" })
        }
    };
    object.insert("kind".into(), kind.into());
    Ok(serde_json::Value::Object(object))
}

fn annotation_object<'a>(
    value: &'a serde_json::Value,
    name: &str,
) -> Result<&'a serde_json::Map<String, serde_json::Value>, FormatError> {
    value
        .as_object()
        .ok_or_else(|| FormatError::InvalidAnnotations(format!("{name} must be an object")))
}

fn take_value(
    object: &mut serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<serde_json::Value, FormatError> {
    object
        .remove(field)
        .ok_or_else(|| FormatError::InvalidAnnotations(format!("{field} is required")))
}

fn take_string(
    object: &mut serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<String, FormatError> {
    take_value(object, field)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| FormatError::InvalidAnnotations(format!("{field} must be a string")))
}

fn take_bool(
    object: &mut serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<bool, FormatError> {
    take_value(object, field)?
        .as_bool()
        .ok_or_else(|| FormatError::InvalidAnnotations(format!("{field} must be a boolean")))
}

fn take_u64(
    object: &mut serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<u64, FormatError> {
    take_value(object, field)?.as_u64().ok_or_else(|| {
        FormatError::InvalidAnnotations(format!("{field} must be a non-negative integer"))
    })
}

fn take_array(
    object: &mut serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<Vec<serde_json::Value>, FormatError> {
    take_value(object, field)?
        .as_array()
        .cloned()
        .ok_or_else(|| FormatError::InvalidAnnotations(format!("{field} must be an array")))
}

fn annotation_id(value: String) -> Result<[u8; 16], FormatError> {
    parse_stable_id(&value).map_err(|_| {
        FormatError::InvalidAnnotations("annotation ID must be 32 hexadecimal digits".into())
    })
}

fn annotation_error<T>(reason: &str) -> Result<T, FormatError> {
    Err(FormatError::InvalidAnnotations(reason.into()))
}

fn decode_unknown(
    object: serde_json::Map<String, serde_json::Value>,
) -> Result<BTreeMap<String, AnnotationValue>, FormatError> {
    object
        .into_iter()
        .map(|(key, value)| Ok((key, annotation_value(value)?)))
        .collect()
}

fn encode_unknown(
    object: &BTreeMap<String, AnnotationValue>,
) -> Result<serde_json::Map<String, serde_json::Value>, FormatError> {
    object
        .iter()
        .map(|(key, value)| Ok((key.clone(), json_value(value)?)))
        .collect()
}

fn annotation_value(value: serde_json::Value) -> Result<AnnotationValue, FormatError> {
    Ok(match value {
        serde_json::Value::Null => AnnotationValue::Null,
        serde_json::Value::Bool(value) => AnnotationValue::Bool(value),
        serde_json::Value::Number(value) => AnnotationValue::Number(value.to_string()),
        serde_json::Value::String(value) => AnnotationValue::String(value),
        serde_json::Value::Array(values) => AnnotationValue::Array(
            values
                .into_iter()
                .map(annotation_value)
                .collect::<Result<_, _>>()?,
        ),
        serde_json::Value::Object(values) => AnnotationValue::Object(decode_unknown(values)?),
    })
}

fn json_value(value: &AnnotationValue) -> Result<serde_json::Value, FormatError> {
    Ok(match value {
        AnnotationValue::Null => serde_json::Value::Null,
        AnnotationValue::Bool(value) => (*value).into(),
        AnnotationValue::Number(value) => serde_json::from_str(value)
            .map_err(|_| FormatError::InvalidAnnotations("unknown number is invalid".into()))?,
        AnnotationValue::String(value) => value.clone().into(),
        AnnotationValue::Array(values) => {
            serde_json::Value::Array(values.iter().map(json_value).collect::<Result<_, _>>()?)
        }
        AnnotationValue::Object(values) => serde_json::Value::Object(encode_unknown(values)?),
    })
}

fn validate_annotation_value(value: serde_json::Value) -> Result<serde_json::Value, FormatError> {
    fn check(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {
                true
            }
            serde_json::Value::String(text) => text
                .chars()
                .all(|character| character == '\n' || !character.is_control()),
            serde_json::Value::Array(values) => values.iter().all(check),
            serde_json::Value::Object(values) => {
                values.keys().all(|key| !key.is_empty()) && values.values().all(check)
            }
        }
    }
    if check(&value) {
        Ok(value)
    } else {
        Err(FormatError::InvalidAnnotations(
            "thread data contains an unsafe string or key".into(),
        ))
    }
}

fn sort_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(sort_json).collect())
        }
        serde_json::Value::Object(values) => {
            let sorted = values
                .into_iter()
                .map(|(key, value)| (key, sort_json(value)))
                .collect::<BTreeMap<_, _>>();
            serde_json::Value::Object(sorted.into_iter().collect())
        }
        value => value,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CssRule {
    selector: String,
    declarations: BTreeMap<String, String>,
}

fn managed_style_selector(id: StyleId) -> String {
    format!("[data-style-id=\"{}\"]", stable_id_text(id.as_bytes()))
}

fn managed_style_id(selector: &str) -> Option<Result<StyleId, FormatError>> {
    let value = selector
        .strip_prefix("[data-style-id=\"")?
        .strip_suffix("\"]")?;
    Some(
        parse_stable_id(value)
            .map(StyleId::from_bytes)
            .map_err(|_| {
                FormatError::InvalidStyles("managed style selector has an invalid ID".into())
            }),
    )
}

fn merge_managed_styles(
    catalog: &StyleCatalog,
    existing: &CanonicalStyles,
) -> Result<CanonicalStyles, FormatError> {
    catalog
        .validate()
        .map_err(|error| FormatError::InvalidStyles(error.to_string()))?;
    let mut rules = existing
        .rules
        .iter()
        .filter(|rule| managed_style_id(&rule.selector).is_none())
        .cloned()
        .collect::<Vec<_>>();
    for style in catalog.iter() {
        let declarations = encode_style_properties(&style.properties);
        if !declarations.is_empty() {
            rules.push(CssRule {
                selector: managed_style_selector(style.id),
                declarations,
            });
        }
    }
    rules.sort_by(|left, right| left.selector.cmp(&right.selector));
    Ok(CanonicalStyles { rules })
}

fn hydrate_style_properties(
    catalog: &mut StyleCatalog,
    styles: &CanonicalStyles,
) -> Result<(), FormatError> {
    let mut definitions = catalog.iter().cloned().collect::<Vec<_>>();
    let by_id = definitions
        .iter()
        .enumerate()
        .map(|(index, style)| (style.id, index))
        .collect::<BTreeMap<_, _>>();
    for rule in &styles.rules {
        let Some(id) = managed_style_id(&rule.selector) else {
            continue;
        };
        let id = id?;
        let index = by_id.get(&id).copied().ok_or_else(|| {
            FormatError::InvalidStyles("managed rule refers to an unknown style ID".into())
        })?;
        definitions[index].properties = decode_style_properties(&rule.declarations)?;
    }
    *catalog = StyleCatalog::from_definitions(definitions)
        .map_err(|error| FormatError::InvalidStyles(error.to_string()))?;
    Ok(())
}

fn encode_style_properties(properties: &StyleProperties) -> BTreeMap<String, String> {
    let mut declarations = BTreeMap::new();
    if let Some(value) = &properties.font_family {
        declarations.insert("font-family".into(), value.clone());
    }
    if let Some(value) = properties.font_size_points {
        declarations.insert("font-size".into(), point_value(value));
    }
    if let Some(value) = properties.weight {
        declarations.insert("font-weight".into(), value.to_string());
    }
    if let Some(value) = properties.italic {
        declarations.insert(
            "font-style".into(),
            if value { "italic" } else { "normal" }.into(),
        );
    }
    if let Some(value) = properties.alignment {
        declarations.insert(
            "text-align".into(),
            match value {
                TextAlignment::Start => "start",
                TextAlignment::Center => "center",
                TextAlignment::End => "end",
                TextAlignment::Justify => "justify",
            }
            .into(),
        );
    }
    for (property, value) in [
        ("text-indent", properties.first_line_indent_points),
        ("margin-left", properties.left_indent_points),
        ("margin-right", properties.right_indent_points),
        ("margin-top", properties.space_before_points),
        ("margin-bottom", properties.space_after_points),
    ] {
        if let Some(value) = value {
            declarations.insert(property.into(), point_value(value));
        }
    }
    if let Some(value) = properties.line_spacing {
        declarations.insert("line-height".into(), finite_number(value));
    }
    if let Some(value) = properties.keep_with_next {
        declarations.insert("keep-with-next".into(), value.to_string());
    }
    if let Some(value) = properties.page_break_before {
        declarations.insert(
            "page-break-before".into(),
            if value { "always" } else { "auto" }.into(),
        );
    }
    declarations
}

fn decode_style_properties(
    declarations: &BTreeMap<String, String>,
) -> Result<StyleProperties, FormatError> {
    let mut properties = StyleProperties::default();
    for (property, value) in declarations {
        match property.as_str() {
            "font-family" => properties.font_family = Some(value.clone()),
            "font-size" => properties.font_size_points = Some(parse_points(value)?),
            "font-weight" => {
                properties.weight = Some(value.parse().map_err(|_| {
                    FormatError::InvalidStyles("font-weight must be an integer".into())
                })?)
            }
            "font-style" => {
                properties.italic = Some(match value.as_str() {
                    "italic" => true,
                    "normal" => false,
                    _ => {
                        return Err(FormatError::InvalidStyles(
                            "font-style must be italic or normal".into(),
                        ));
                    }
                })
            }
            "text-align" => {
                properties.alignment = Some(match value.as_str() {
                    "start" => TextAlignment::Start,
                    "center" => TextAlignment::Center,
                    "end" => TextAlignment::End,
                    "justify" => TextAlignment::Justify,
                    _ => {
                        return Err(FormatError::InvalidStyles(
                            "text-align has an unsupported value".into(),
                        ));
                    }
                })
            }
            "text-indent" => properties.first_line_indent_points = Some(parse_points(value)?),
            "margin-left" => properties.left_indent_points = Some(parse_points(value)?),
            "margin-right" => properties.right_indent_points = Some(parse_points(value)?),
            "line-height" => properties.line_spacing = Some(parse_finite_number(value)?),
            "margin-top" => properties.space_before_points = Some(parse_points(value)?),
            "margin-bottom" => properties.space_after_points = Some(parse_points(value)?),
            "keep-with-next" => {
                properties.keep_with_next = Some(parse_css_bool(value, "keep-with-next")?)
            }
            "page-break-before" => {
                properties.page_break_before = Some(match value.as_str() {
                    "always" => true,
                    "auto" => false,
                    _ => {
                        return Err(FormatError::InvalidStyles(
                            "page-break-before must be always or auto".into(),
                        ));
                    }
                })
            }
            _ => unreachable!("the CSS parser rejects unsupported properties"),
        }
    }
    Ok(properties)
}

fn point_value(value: f32) -> String {
    format!("{}pt", finite_number(value))
}

fn finite_number(value: f32) -> String {
    let value = value.to_string();
    value.strip_suffix(".0").unwrap_or(&value).to_owned()
}

fn parse_points(value: &str) -> Result<f32, FormatError> {
    let number = value.strip_suffix("pt").ok_or_else(|| {
        FormatError::InvalidStyles("managed point values must use pt units".into())
    })?;
    parse_finite_number(number)
}

fn parse_finite_number(value: &str) -> Result<f32, FormatError> {
    let number: f32 = value
        .parse()
        .map_err(|_| FormatError::InvalidStyles("managed numeric value is invalid".into()))?;
    if !number.is_finite() {
        return Err(FormatError::InvalidStyles(
            "managed numeric value must be finite".into(),
        ));
    }
    Ok(number)
}

fn parse_css_bool(value: &str, property: &str) -> Result<bool, FormatError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(FormatError::InvalidStyles(format!(
            "{property} must be true or false"
        ))),
    }
}

fn parse_css(text: &str) -> Result<Vec<CssRule>, FormatError> {
    if text.contains('\r') {
        return Err(FormatError::InvalidStyles(
            "stylesheets must use LF line endings".into(),
        ));
    }
    if text.contains("/*") || text.contains('@') {
        return Err(FormatError::InvalidStyles(
            "comments and at-rules are not part of canonical styles".into(),
        ));
    }

    let mut remaining = text.trim();
    let mut rules = BTreeMap::new();
    while !remaining.is_empty() {
        let open = remaining
            .find('{')
            .ok_or_else(|| FormatError::InvalidStyles("expected an opening brace".into()))?;
        let selector = canonical_selector(&remaining[..open])?;
        let after_open = &remaining[open + 1..];
        let close = after_open
            .find('}')
            .ok_or_else(|| FormatError::InvalidStyles("expected a closing brace".into()))?;
        let body = &after_open[..close];
        if body.contains('{') {
            return Err(FormatError::InvalidStyles(
                "nested CSS rules are not supported".into(),
            ));
        }
        let declarations = parse_declarations(body)?;
        if rules
            .insert(
                selector.clone(),
                CssRule {
                    selector,
                    declarations,
                },
            )
            .is_some()
        {
            return Err(FormatError::InvalidStyles("duplicate selector".into()));
        }
        remaining = after_open[close + 1..].trim();
    }
    Ok(rules.into_values().collect())
}

fn canonical_selector(selector: &str) -> Result<String, FormatError> {
    let selector = collapse_whitespace(selector);
    if let Some(id) = managed_style_id(&selector) {
        id?;
        return Ok(selector);
    }
    if selector.is_empty()
        || selector.chars().any(|character| {
            character.is_control() || matches!(character, '<' | '>' | '"' | '\'' | '\\')
        })
    {
        return Err(FormatError::InvalidStyles(
            "unsafe or empty selector".into(),
        ));
    }
    Ok(selector)
}

fn parse_declarations(body: &str) -> Result<BTreeMap<String, String>, FormatError> {
    let mut declarations = BTreeMap::new();
    for declaration in body.split(';').filter(|part| !part.trim().is_empty()) {
        let (property, value) = declaration
            .split_once(':')
            .ok_or_else(|| FormatError::InvalidStyles("property is missing a colon".into()))?;
        let property = property.trim().to_ascii_lowercase();
        if !is_supported_style_property(&property) {
            return Err(FormatError::InvalidStyles(format!(
                "unsupported style property {property:?}"
            )));
        }
        let value = collapse_whitespace(value);
        let lower_value = value.to_ascii_lowercase();
        if value.is_empty()
            || value.contains(['{', '}', ';'])
            || lower_value.contains("url(")
            || lower_value.contains("expression")
            || lower_value.contains("javascript:")
        {
            return Err(FormatError::InvalidStyles("unsafe style value".into()));
        }
        if declarations.insert(property, value).is_some() {
            return Err(FormatError::InvalidStyles(
                "duplicate style property".into(),
            ));
        }
    }
    if declarations.is_empty() {
        return Err(FormatError::InvalidStyles(
            "a style rule needs a property".into(),
        ));
    }
    Ok(declarations)
}

fn is_supported_style_property(property: &str) -> bool {
    matches!(
        property,
        "font-family"
            | "font-size"
            | "font-weight"
            | "font-style"
            | "text-align"
            | "text-indent"
            | "margin-left"
            | "margin-right"
            | "line-height"
            | "margin-top"
            | "margin-bottom"
            | "keep-with-next"
            | "page-break-before"
    )
}

fn render_css(rules: &[CssRule]) -> String {
    let mut output = String::new();
    for rule in rules {
        output.push_str(&rule.selector);
        output.push_str(" {\n");
        for (property, value) in &rule.declarations {
            output.push_str("  ");
            output.push_str(property);
            output.push_str(": ");
            output.push_str(value);
            output.push_str(";\n");
        }
        output.push_str("}\n");
    }
    output
}

fn render_dictionary(dictionary: &CanonicalDictionary) -> String {
    let mut output = dictionary
        .entries
        .iter()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    if !output.is_empty() {
        output.push('\n');
    }
    output
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HtmlNode {
    Text(String),
    Element {
        tag: String,
        attributes: BTreeMap<String, String>,
        children: Vec<HtmlNode>,
    },
}

fn append_suffix_to_first_matching_document_title(
    nodes: &mut [HtmlNode],
    display_title: &str,
    suffix: &str,
) -> bool {
    for node in nodes {
        let HtmlNode::Element {
            attributes,
            children,
            ..
        } = node
        else {
            continue;
        };
        if attributes
            .get("data-style-id")
            .is_some_and(|style| style == "document-title")
        {
            let mut text = String::new();
            collect_html_text(children, &mut text);
            if text != display_title {
                return false;
            }
            return append_suffix_to_last_text(children, suffix);
        }
        if append_suffix_to_first_matching_document_title(children, display_title, suffix) {
            return true;
        }
    }
    false
}

fn collect_html_text(nodes: &[HtmlNode], output: &mut String) {
    for node in nodes {
        match node {
            HtmlNode::Text(text) => output.push_str(text),
            HtmlNode::Element { children, .. } => collect_html_text(children, output),
        }
    }
}

fn append_suffix_to_last_text(nodes: &mut [HtmlNode], suffix: &str) -> bool {
    for node in nodes.iter_mut().rev() {
        match node {
            HtmlNode::Text(text) => {
                text.push_str(suffix);
                return true;
            }
            HtmlNode::Element { children, .. } => {
                if append_suffix_to_last_text(children, suffix) {
                    return true;
                }
            }
        }
    }
    false
}

fn parse_html(html: &str) -> Result<Vec<HtmlNode>, FormatError> {
    let mut root = Vec::new();
    let mut stack: Vec<(String, BTreeMap<String, String>, Vec<HtmlNode>)> = Vec::new();
    let mut cursor = 0;

    while cursor < html.len() {
        let next_tag = html[cursor..].find('<').map(|offset| cursor + offset);
        let Some(tag_start) = next_tag else {
            push_html_text(&mut root, &mut stack, &html[cursor..])?;
            break;
        };
        push_html_text(&mut root, &mut stack, &html[cursor..tag_start])?;
        let tag_end = find_tag_end(html, tag_start + 1)?;
        let token = &html[tag_start + 1..tag_end];
        cursor = tag_end + 1;

        if token.starts_with('!') || token.starts_with('?') {
            return Err(FormatError::InvalidDocument(
                "comments, doctypes, and processing instructions are not canonical".into(),
            ));
        }
        if let Some(rest) = token.strip_prefix('/') {
            let tag = parse_end_tag(rest)?;
            let (open_tag, attributes, children) = stack
                .pop()
                .ok_or_else(|| FormatError::InvalidDocument("closing unopened element".into()))?;
            if open_tag != tag {
                return Err(FormatError::InvalidDocument(
                    "mismatched closing element".into(),
                ));
            }
            push_html_node(
                &mut root,
                &mut stack,
                HtmlNode::Element {
                    tag: open_tag,
                    attributes,
                    children,
                },
            );
            continue;
        }

        let (tag, attributes, self_closed) = parse_start_tag(token)?;
        if is_void_tag(&tag) {
            push_html_node(
                &mut root,
                &mut stack,
                HtmlNode::Element {
                    tag,
                    attributes,
                    children: Vec::new(),
                },
            );
        } else if self_closed {
            return Err(FormatError::InvalidDocument(
                "non-void elements must use explicit end tags".into(),
            ));
        } else {
            stack.push((tag, attributes, Vec::new()));
        }
    }

    if !stack.is_empty() {
        return Err(FormatError::InvalidDocument("unclosed element".into()));
    }
    Ok(root)
}

fn find_tag_end(html: &str, start: usize) -> Result<usize, FormatError> {
    let mut quote = None;
    for (offset, character) in html[start..].char_indices() {
        match (quote, character) {
            (Some(current), character) if character == current => quote = None,
            (None, '"' | '\'') => quote = Some(character),
            (None, '>') => return Ok(start + offset),
            _ => {}
        }
    }
    Err(FormatError::InvalidDocument("unterminated element".into()))
}

fn parse_end_tag(token: &str) -> Result<String, FormatError> {
    let tag = token.trim();
    if tag.is_empty() || tag.contains(char::is_whitespace) {
        return Err(FormatError::InvalidDocument(
            "invalid closing element".into(),
        ));
    }
    validate_html_tag(tag)
}

fn parse_start_tag(token: &str) -> Result<(String, BTreeMap<String, String>, bool), FormatError> {
    let token = token.trim();
    let (token, self_closed) = match token.strip_suffix('/') {
        Some(token) => (token.trim_end(), true),
        None => (token, false),
    };
    let tag_end = token.find(char::is_whitespace).unwrap_or(token.len());
    let tag = validate_html_tag(&token[..tag_end])?;
    let mut remaining = &token[tag_end..];
    let mut attributes = BTreeMap::new();
    while !remaining.trim_start().is_empty() {
        remaining = remaining.trim_start();
        let name_end = remaining
            .find(|character: char| character.is_whitespace() || character == '=')
            .unwrap_or(remaining.len());
        let name = &remaining[..name_end];
        remaining = &remaining[name_end..];
        if name.is_empty() {
            return Err(FormatError::InvalidDocument("invalid attribute".into()));
        }
        remaining = remaining.trim_start();
        let value = remaining
            .strip_prefix('=')
            .ok_or_else(|| FormatError::InvalidDocument("attributes need quoted values".into()))?;
        let value = value.trim_start();
        let quote = value
            .chars()
            .next()
            .filter(|character| matches!(character, '"' | '\''))
            .ok_or_else(|| FormatError::InvalidDocument("attributes need quoted values".into()))?;
        let after_quote = &value[quote.len_utf8()..];
        let value_end = after_quote
            .find(quote)
            .ok_or_else(|| FormatError::InvalidDocument("unterminated attribute".into()))?;
        let value = decode_html_entities(&after_quote[..value_end])?;
        remaining = &after_quote[value_end + quote.len_utf8()..];
        let name = canonical_attribute_name(name)?;
        validate_html_attribute(&tag, &name, &value)?;
        if attributes.insert(name, value).is_some() {
            return Err(FormatError::InvalidDocument("duplicate attribute".into()));
        }
    }
    Ok((tag, attributes, self_closed))
}

fn validate_html_tag(tag: &str) -> Result<String, FormatError> {
    let tag = tag.to_ascii_lowercase();
    if matches!(
        tag.as_str(),
        "p" | "h1"
            | "h2"
            | "h3"
            | "blockquote"
            | "ul"
            | "ol"
            | "li"
            | "br"
            | "strong"
            | "em"
            | "u"
            | "s"
            | "sup"
            | "sub"
            | "a"
            | "span"
            | "hr"
    ) {
        Ok(tag)
    } else {
        Err(FormatError::InvalidDocument(format!(
            "unsupported element {tag:?}"
        )))
    }
}

fn canonical_attribute_name(name: &str) -> Result<String, FormatError> {
    if !name.is_ascii() || name.is_empty() {
        return Err(FormatError::InvalidDocument(
            "attribute names must be ASCII".into(),
        ));
    }
    Ok(name.to_ascii_lowercase())
}

fn validate_html_attribute(tag: &str, name: &str, value: &str) -> Result<(), FormatError> {
    if name.starts_with("on") || matches!(name, "style" | "src") {
        return Err(FormatError::InvalidDocument(
            "event, style, and source attributes are unsafe".into(),
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(FormatError::InvalidDocument(
            "attributes cannot contain control characters".into(),
        ));
    }
    let permitted = match (tag, name) {
        ("a", "href") => is_safe_href(value),
        ("span", "data-semantic") => matches!(value, "small-caps"),
        ("hr", "data-kind") => matches!(value, "scene-break" | "page-break"),
        (_, "data-block-id" | "data-style-id") => is_safe_identifier(value),
        _ => false,
    };
    if permitted {
        Ok(())
    } else {
        Err(FormatError::InvalidDocument(format!(
            "unsupported attribute {name:?} on {tag:?}"
        )))
    }
}

fn is_safe_href(value: &str) -> bool {
    if value.is_empty()
        || value.starts_with(['/', '\\'])
        || value.starts_with("//")
        || value.contains('\\')
    {
        return false;
    }
    match value.split_once(':') {
        Some((scheme, _)) => matches!(
            scheme.to_ascii_lowercase().as_str(),
            "http" | "https" | "mailto"
        ),
        None => !value.split('/').any(|segment| segment == ".."),
    }
}

fn is_safe_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
}

fn is_void_tag(tag: &str) -> bool {
    matches!(tag, "br" | "hr")
}

fn push_html_text(
    root: &mut Vec<HtmlNode>,
    stack: &mut [(String, BTreeMap<String, String>, Vec<HtmlNode>)],
    source: &str,
) -> Result<(), FormatError> {
    if source.is_empty() {
        return Ok(());
    }
    let text = decode_html_entities(&source.replace("\r\n", "\n").replace('\r', "\n"))?;
    if text.trim().is_empty() && text.contains('\n') {
        return Ok(());
    }
    push_html_node(root, stack, HtmlNode::Text(text));
    Ok(())
}

fn push_html_node(
    root: &mut Vec<HtmlNode>,
    stack: &mut [(String, BTreeMap<String, String>, Vec<HtmlNode>)],
    node: HtmlNode,
) {
    if let Some((_, _, children)) = stack.last_mut() {
        children.push(node);
    } else {
        root.push(node);
    }
}

fn decode_html_entities(text: &str) -> Result<String, FormatError> {
    let mut output = String::new();
    let mut remaining = text;
    while let Some(start) = remaining.find('&') {
        output.push_str(&remaining[..start]);
        let after = &remaining[start + 1..];
        let end = after
            .find(';')
            .ok_or_else(|| FormatError::InvalidDocument("unterminated HTML entity".into()))?;
        let entity = &after[..end];
        let character = match entity {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            entity if entity.starts_with("#x") || entity.starts_with("#X") => {
                let number = u32::from_str_radix(&entity[2..], 16)
                    .map_err(|_| FormatError::InvalidDocument("invalid numeric entity".into()))?;
                char::from_u32(number)
                    .ok_or_else(|| FormatError::InvalidDocument("invalid numeric entity".into()))?
            }
            entity if entity.starts_with('#') => {
                let number = entity[1..]
                    .parse::<u32>()
                    .map_err(|_| FormatError::InvalidDocument("invalid numeric entity".into()))?;
                char::from_u32(number)
                    .ok_or_else(|| FormatError::InvalidDocument("invalid numeric entity".into()))?
            }
            _ => {
                return Err(FormatError::InvalidDocument(
                    "unsupported HTML entity".into(),
                ));
            }
        };
        output.push(character);
        remaining = &after[end + 1..];
    }
    output.push_str(remaining);
    Ok(output)
}

fn render_html(nodes: &[HtmlNode]) -> String {
    let mut output = String::new();
    for node in nodes {
        render_html_node(node, &mut output);
    }
    output
}

fn render_html_node(node: &HtmlNode, output: &mut String) {
    match node {
        HtmlNode::Text(text) => escape_html_text(text, output),
        HtmlNode::Element {
            tag,
            attributes,
            children,
        } => {
            output.push('<');
            output.push_str(tag);
            for (name, value) in attributes {
                output.push(' ');
                output.push_str(name);
                output.push_str("=\"");
                escape_html_attribute(value, output);
                output.push('"');
            }
            output.push('>');
            if !is_void_tag(tag) {
                for child in children {
                    render_html_node(child, output);
                }
                output.push_str("</");
                output.push_str(tag);
                output.push('>');
            }
        }
    }
}

fn escape_html_text(text: &str, output: &mut String) {
    for character in text.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            _ => output.push(character),
        }
    }
}

fn escape_html_attribute(value: &str, output: &mut String) {
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '"' => output.push_str("&quot;"),
            _ => output.push(character),
        }
    }
}

fn collapse_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codec() -> ProjectFormatCodec {
        ProjectFormatCodec::default()
    }

    #[test]
    fn content_hash_of_empty_bytes_is_stable() {
        assert_eq!(
            ContentHash::of_bytes(b""),
            ContentHash::from_bytes([
                0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14, 0x9a, 0xfb, 0xf4, 0xc8, 0x99, 0x6f,
                0xb9, 0x24, 0x27, 0xae, 0x41, 0xe4, 0x64, 0x9b, 0x93, 0x4c, 0xa4, 0x95, 0x99, 0x1b,
                0x78, 0x52, 0xb8, 0x55,
            ])
        );
    }

    #[test]
    fn canonical_html_normalizes_entities_tags_and_attributes() {
        let document = codec()
            .decode_document(
                br#"<P data-style-id='body' data-block-id="block-1">Tom &amp; &lt;Ada&gt;<BR></P>"#,
            )
            .unwrap();

        assert_eq!(
            document.as_html(),
            r#"<p data-block-id="block-1" data-style-id="body">Tom &amp; &lt;Ada&gt;<br></p>"#
        );
        let encoded = codec()
            .encode(&CanonicalResource::Document(document.clone()))
            .unwrap();
        assert_eq!(encoded.bytes, document.as_html().as_bytes());
        assert_eq!(codec().decode_document(&encoded.bytes).unwrap(), document);
    }

    #[test]
    fn copying_a_synchronized_document_title_updates_only_that_title() {
        let document = codec()
            .decode_document(
                br#"<p data-style-id="document-title"><strong>Chapter</strong> One</p><p>Body</p>"#,
            )
            .expect("canonical document");
        assert_eq!(
            document
                .append_copy_suffix_to_matching_title("Chapter One", " Copy")
                .as_html(),
            r#"<p data-style-id="document-title"><strong>Chapter</strong> One Copy</p><p>Body</p>"#
        );
        assert_eq!(
            document
                .append_copy_suffix_to_matching_title("Different display title", " Copy")
                .as_html(),
            document.as_html(),
            "a divergent display title must not rewrite authored content"
        );
    }

    #[test]
    fn dangerous_or_unsupported_html_never_becomes_canonical() {
        for html in [
            "<script>alert(1)</script>",
            "<p onclick=\"alert(1)\">x</p>",
            "<a href=\"javascript:alert(1)\">x</a>",
            "<iframe src=\"https://example.invalid\"></iframe>",
            "<img src=\"image.png\">",
            "<p style=\"color: red\">x</p>",
        ] {
            assert!(codec().decode_document(html.as_bytes()).is_err(), "{html}");
        }
    }

    #[test]
    fn annotation_sidecars_have_one_stable_utf8_lf_representation() {
        let codec = codec();
        let annotations = codec
            .decode_annotations(
                b"{\r\n  \"threads\": [],\r\n  \"document_id\": \"document-1\",\r\n  \"schema\": \"parchmint.annotation-sidecar/v1\"\r\n}\r\n",
            )
            .unwrap();
        let first = codec
            .encode(&CanonicalResource::Annotations(annotations))
            .unwrap()
            .bytes;
        let annotations = codec.decode_annotations(&first).unwrap();
        let second = codec
            .encode(&CanonicalResource::Annotations(annotations))
            .unwrap()
            .bytes;

        assert_eq!(first, second);
        assert!(std::str::from_utf8(&first).is_ok());
        assert!(!first.contains(&b'\r'));
    }

    #[test]
    fn annotation_sidecars_reject_invalid_json_and_non_utf8_input() {
        for bytes in [
            br#"{"schema":"parchmint.annotation-sidecar/v1","document_id":"document-1"}"#
                .as_slice(),
            br#"{"schema":"parchmint.annotation-sidecar/v1","document_id":"document-1","threads":[],"unexpected":true}"#
                .as_slice(),
            &[0xff][..],
        ] {
            assert!(codec().decode_annotations(bytes).is_err(), "{bytes:?}");
        }
    }

    #[test]
    fn typed_annotation_sidecars_preserve_unknown_fields_and_reject_malformed_ranges() {
        let bytes = br#"{
          "schema":"parchmint.annotation-sidecar/v1",
          "document_id":"01010101010101010101010101010101",
          "threads":[{
            "id":"02020202020202020202020202020202",
            "resolved":false,
            "future":{"author":"kept"},
            "messages":[{"id":"03030303030303030303030303030303","body":"note","display":"kept"}],
            "anchor":{"kind":"range","block_id":"04040404040404040404040404040404","start":1,"end":3,"quote":"bc","context_before":"a","context_after":"d","orphaned":false,"confidence":1}
          }]
        }"#;
        let decoded = codec().decode_annotations(bytes).unwrap();
        let threads = decoded.typed_threads().unwrap();
        assert!(threads[0].unknown_fields.contains_key("future"));
        assert!(
            threads[0].messages[0]
                .unknown_fields
                .contains_key("display")
        );
        let encoded = codec()
            .encode(&CanonicalResource::Annotations(
                CanonicalAnnotations::from_typed(decoded.document_id(), &threads).unwrap(),
            ))
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&encoded.bytes).unwrap();
        assert_eq!(value["threads"][0]["future"]["author"], "kept");
        assert_eq!(value["threads"][0]["anchor"]["confidence"], 1);

        let malformed = bytes.to_vec();
        let malformed = String::from_utf8(malformed)
            .unwrap()
            .replace("\"start\":1,\"end\":3", "\"start\":4,\"end\":3");
        assert!(codec().decode_annotations(malformed.as_bytes()).is_err());
    }

    #[test]
    fn semantic_css_has_one_stable_safe_representation() {
        let styles = codec()
            .decode_styles(
                b".heading { font-weight : bold; text-align: center; }\n.body { line-height: 1.5; font-family: Serif; }",
            )
            .unwrap();
        assert_eq!(
            styles.as_css(),
            ".body {\n  font-family: Serif;\n  line-height: 1.5;\n}\n.heading {\n  font-weight: bold;\n  text-align: center;\n}\n"
        );
        assert!(
            codec()
                .decode_styles(b".body { background: url(https://example.invalid); }")
                .is_err()
        );
        assert!(
            codec()
                .decode_styles(b"@import url(https://example.invalid); ")
                .is_err()
        );
    }

    #[test]
    fn style_definitions_and_managed_properties_round_trip_without_rewriting_unmanaged_css() {
        use parchmint_domain::{StyleDefinition, StyleProperties, TextAlignment};

        let mut project = Project::new(ProjectId::from_bytes([1; 16]));
        let mut body = project.styles.get(StyleCatalog::body_id()).unwrap().clone();
        body.display_name = "Narrative".into();
        body.properties = StyleProperties {
            font_family: Some("Source Serif 4".into()),
            font_size_points: Some(12.5),
            weight: Some(450),
            italic: Some(false),
            alignment: Some(TextAlignment::Justify),
            first_line_indent_points: Some(18.0),
            left_indent_points: Some(0.0),
            right_indent_points: Some(0.0),
            line_spacing: Some(1.4),
            space_before_points: Some(2.0),
            space_after_points: Some(6.0),
            keep_with_next: Some(false),
            page_break_before: Some(false),
        };
        project.styles.upsert(body.clone()).unwrap();
        let custom_id = StyleId::from_bytes([8; 16]);
        let mut custom = StyleDefinition::custom(custom_id, "Letter");
        custom.properties.italic = Some(true);
        project.styles.upsert(custom.clone()).unwrap();

        let styles_path = CanonicalRelativePath::parse("styles.css").unwrap();
        let existing = BTreeMap::from([(
            styles_path.clone(),
            b".legacy {\n  font-weight: bold;\n}\n".to_vec(),
        )]);
        let encoding = codec()
            .encode_domain_project(
                &project,
                &BTreeMap::new(),
                &existing,
                &CanonicalProjectPathMap::default(),
            )
            .unwrap();
        let css = std::str::from_utf8(&encoding.resources[&styles_path].bytes).unwrap();
        assert!(css.contains(".legacy {"));
        assert!(css.contains(&managed_style_selector(StyleCatalog::body_id())));
        assert!(css.contains("font-size: 12.5pt;"));

        let manifest_path = CanonicalRelativePath::parse("project.toml").unwrap();
        let manifest = codec()
            .decode_manifest(&encoding.resources[&manifest_path].bytes)
            .unwrap();
        let styles = codec().decode_styles(css.as_bytes()).unwrap();
        let (decoded, _) = codec()
            .decode_domain_project_with_styles(&manifest, Some(&styles), project.id)
            .unwrap()
            .unwrap();
        assert_eq!(decoded.styles.get(StyleCatalog::body_id()), Some(&body));
        assert_eq!(decoded.styles.get(custom_id), Some(&custom));
        assert_eq!(decoded.styles.iter().last().unwrap().id, custom_id);
    }

    #[test]
    fn combined_style_hydration_rejects_unknown_ids_and_invalid_units() {
        let project = Project::new(ProjectId::from_bytes([1; 16]));
        let encoding = codec()
            .encode_domain_project(
                &project,
                &BTreeMap::new(),
                &BTreeMap::new(),
                &CanonicalProjectPathMap::default(),
            )
            .unwrap();
        let manifest = codec()
            .decode_manifest(
                &encoding.resources[&CanonicalRelativePath::parse("project.toml").unwrap()].bytes,
            )
            .unwrap();
        for css in [
            format!(
                "{} {{ font-size: 12px; }}",
                managed_style_selector(StyleCatalog::body_id())
            ),
            format!(
                "{} {{ font-size: 12pt; }}",
                managed_style_selector(StyleId::from_bytes([9; 16]))
            ),
        ] {
            let styles = codec().decode_styles(css.as_bytes()).unwrap();
            assert!(
                codec()
                    .decode_domain_project_with_styles(&manifest, Some(&styles), project.id)
                    .is_err()
            );
        }
    }

    #[test]
    fn legacy_manifest_uses_reserved_defaults_and_preserves_unmanaged_styles() {
        let manifest = codec()
            .decode_manifest(
                b"[project]\ntitle = 'Legacy'\n[parchmint-structure]\nversion = 1\nmetadata-fields = []\nnodes = []\n",
            )
            .unwrap();
        let (project, _) = codec()
            .decode_domain_project_with_styles(&manifest, None, ProjectId::from_bytes([1; 16]))
            .unwrap()
            .unwrap();
        assert_eq!(project.styles.iter().count(), 7);

        let styles_path = CanonicalRelativePath::parse("styles.css").unwrap();
        let legacy_css = b".body {\n  line-height: 1.5;\n}\n".to_vec();
        let encoding = codec()
            .encode_domain_project(
                &project,
                &BTreeMap::new(),
                &BTreeMap::from([(styles_path.clone(), legacy_css.clone())]),
                &CanonicalProjectPathMap::default(),
            )
            .unwrap();
        assert_eq!(encoding.resources[&styles_path].bytes, legacy_css);
    }

    #[test]
    fn dictionary_uses_sorted_unique_lf_entries() {
        let dictionary = codec()
            .decode_dictionary("zebra\nalpha\nzebra\n".as_bytes())
            .unwrap();
        let encoded = codec()
            .encode(&CanonicalResource::Dictionary(dictionary))
            .unwrap();
        assert_eq!(encoded.bytes, b"alpha\nzebra\n");
        assert!(codec().decode_dictionary(b" alpha\n").is_err());
        assert!(codec().decode_dictionary(b"alpha\r\n").is_err());
    }

    #[test]
    fn relative_paths_reject_nonportable_or_escaping_paths() {
        for path in [
            "/tmp/project.toml",
            "C:/ParchMint/project.toml",
            "../project.toml",
            "manuscript/../../project.toml",
            "manuscript\\chapter.html",
            "manuscript//chapter.html",
        ] {
            assert!(CanonicalRelativePath::parse(path).is_err(), "{path}");
        }
        assert_eq!(
            CanonicalRelativePath::parse("manuscript/chapter.html")
                .unwrap()
                .as_str(),
            "manuscript/chapter.html"
        );
    }

    #[test]
    fn format_controls_reject_unknown_and_non_utf8_values() {
        for control in [
            b"99\n".as_slice(),
            b"future-format\n".as_slice(),
            &[0xff][..],
        ] {
            assert!(codec().detect(control).is_err(), "{control:?}");
        }
    }

    #[test]
    fn project_decode_rejects_portable_path_collisions() {
        let mut resources = BTreeMap::new();
        resources.insert(
            CanonicalRelativePath::parse("manuscript/Chapter.html").unwrap(),
            b"<p>one</p>".to_vec(),
        );
        resources.insert(
            CanonicalRelativePath::parse("manuscript/chapter.html").unwrap(),
            b"<p>two</p>".to_vec(),
        );
        assert!(
            codec()
                .decode_project(CanonicalInputSet {
                    format_control: Some(FORMAT_CONTROL_V1.to_vec()),
                    resources,
                })
                .is_err()
        );
        assert!(CanonicalRelativePath::parse("manuscript/cafe\u{301}.html").is_err());
        assert!(CanonicalRelativePath::parse("manuscript/café.html").is_ok());
    }

    #[test]
    fn decode_project_rejects_missing_manifest() {
        assert_eq!(
            codec().decode_project(CanonicalInputSet {
                format_control: Some(FORMAT_CONTROL_V1.to_vec()),
                resources: BTreeMap::new(),
            }),
            Err(FormatError::MissingManifest)
        );
    }

    #[test]
    fn migration_rejects_missing_manifest() {
        assert!(matches!(
            codec().migrate(
                SourceFormatSnapshot {
                    format_control: FORMAT_CONTROL_V1.to_vec(),
                    resources: BTreeMap::new(),
                },
                FormatVersion::V1,
            ),
            Err(MigrationError::Format(FormatError::MissingManifest))
        ));
    }

    #[test]
    fn migration_reencodes_a_complete_v1_snapshot_without_changing_resource_paths() {
        let document_path = CanonicalRelativePath::parse("manuscript/chapter.html").unwrap();
        let annotation_path = CanonicalRelativePath::parse("annotations/document-1.json").unwrap();
        let mut resources = BTreeMap::new();
        resources.insert(
            CanonicalRelativePath::parse("project.toml").unwrap(),
            b"[project]\n".to_vec(),
        );
        resources.insert(
            document_path.clone(),
            b"<p data-style-id=\"body\" data-block-id=\"block-1\">Text</p>".to_vec(),
        );
        resources.insert(
            annotation_path.clone(),
            br#"{"threads":[{"id":"01010101010101010101010101010101","resolved":false,"messages":[{"id":"02020202020202020202020202020202","body":"line one\nline two"}],"anchor":{"kind":"document"}}],"document_id":"document-1","schema":"parchmint.annotation-sidecar/v1"}"#.to_vec(),
        );

        let migrated = codec()
            .migrate(
                SourceFormatSnapshot {
                    format_control: FORMAT_CONTROL_V1.to_vec(),
                    resources,
                },
                FormatVersion::V1,
            )
            .unwrap();
        assert_eq!(migrated.format_version, FormatVersion::V1);
        assert!(migrated.resources.contains_key(&document_path));
        assert!(migrated.resources.contains_key(&annotation_path));
        assert!(
            migrated
                .resources
                .contains_key(&CanonicalRelativePath::parse(".parchmint/format-version").unwrap())
        );
        assert!(
            codec()
                .migrate(
                    SourceFormatSnapshot {
                        format_control: b"2\n".to_vec(),
                        resources: BTreeMap::new(),
                    },
                    FormatVersion::V1,
                )
                .is_err()
        );
    }

    #[test]
    fn annotation_sidecar_filename_must_match_its_document_id() {
        let mut resources = BTreeMap::new();
        resources.insert(
            CanonicalRelativePath::parse("annotations/other-document.json").unwrap(),
            br#"{"schema":"parchmint.annotation-sidecar/v1","document_id":"document-1","threads":[]}"#.to_vec(),
        );
        assert!(
            codec()
                .decode_project(CanonicalInputSet {
                    format_control: Some(FORMAT_CONTROL_V1.to_vec()),
                    resources,
                })
                .is_err()
        );
    }

    #[test]
    fn domain_project_round_trip_preserves_structure_metadata_and_stable_document_paths() {
        use parchmint_domain::{
            MetadataApplicability, MetadataFieldDefinition, MetadataFieldId, MetadataTextKind,
            ProjectCommand, ProjectExportSetting, ProjectExportSettings, apply_project_command,
        };

        let project_id = ProjectId::from_bytes([1; 16]);
        let group = NodeId::from_bytes([2; 16]);
        let node = NodeId::from_bytes([3; 16]);
        let document = DomainDocumentId::from_bytes([4; 16]);
        let metadata = MetadataFieldId::from_bytes([5; 16]);
        let mut project = Project::new(project_id);
        for command in [
            ProjectCommand::create_group(group, NodeId::manuscript_root(), 0, "Drafts"),
            ProjectCommand::create_document(node, document, group, 0, "Chapter One"),
            ProjectCommand::set_project_export_settings(ProjectExportSettings {
                excluded: false,
                emit_titles: ProjectExportSetting::Disabled,
                starts_new_page: true,
            }),
            ProjectCommand::set_node_export_settings(
                node,
                ProjectExportSettings {
                    excluded: false,
                    emit_titles: ProjectExportSetting::Enabled,
                    starts_new_page: true,
                },
            ),
            ProjectCommand::UpsertMetadataField {
                definition: MetadataFieldDefinition {
                    id: metadata,
                    label: "Status".into(),
                    description: Some("Draft status".into()),
                    applicability: MetadataApplicability::Documents,
                    text_kind: MetadataTextKind::SingleLine,
                    default_value: None,
                    visible_on_cards: true,
                },
            },
            ProjectCommand::SetMetadataValue {
                id: node,
                field: metadata,
                value: Some("Revised".into()),
            },
            ProjectCommand::move_node(node, NodeId::research_root(), 0),
            ProjectCommand::rename_node(node, "Research Note"),
        ] {
            let revision = project.revision;
            project = apply_project_command(&project, revision, command)
                .unwrap()
                .project;
        }
        project.display_title = "Round Trip".into();
        let previous_path = CanonicalRelativePath::parse("manuscript/old-name.html").unwrap();
        let previous = CanonicalProjectPathMap {
            documents: BTreeMap::from([(document, previous_path.clone())]),
        };
        let documents = BTreeMap::from([(document, "<p>Body</p>".to_owned())]);
        let frontier = CanonicalPersistenceFrontier {
            recovery_project_revision: 7,
            document_revisions: BTreeMap::from([(document, 11)]),
            ..Default::default()
        };
        let encoding = codec()
            .encode_domain_project_with_frontier(
                &project,
                &documents,
                &BTreeMap::new(),
                &previous,
                &frontier,
            )
            .unwrap();
        let repeated = codec()
            .encode_domain_project_with_frontier(
                &project,
                &documents,
                &BTreeMap::new(),
                &previous,
                &frontier,
            )
            .unwrap();
        assert_eq!(encoding, repeated);
        assert_eq!(encoding.deletions, [previous_path]);
        let document_path = encoding.paths.documents[&document].clone();
        assert!(document_path.as_str().starts_with("research/"));

        let manifest = codec()
            .decode_manifest(
                &encoding.resources[&CanonicalRelativePath::parse("project.toml").unwrap()].bytes,
            )
            .unwrap();
        let (decoded, decoded_paths) = codec()
            .decode_domain_project(&manifest, project_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            codec().decode_persistence_frontier(&manifest).unwrap(),
            encoding.persistence_frontier
        );
        let decoded_node = decoded.nodes.get(node).unwrap();
        assert_eq!(decoded.display_title, "Round Trip");
        assert_eq!(decoded.revision, project.revision);
        assert_eq!(decoded_node.title, "Research Note");
        assert_eq!(
            decoded.export_settings.emit_titles,
            ProjectExportSetting::Disabled
        );
        assert!(decoded.export_settings.starts_new_page);
        assert_eq!(
            decoded_node.export_settings.emit_titles,
            ProjectExportSetting::Enabled
        );
        assert!(decoded_node.export_settings.starts_new_page);
        assert_eq!(decoded.nodes.parent(node), Some(NodeId::research_root()));
        assert_eq!(
            decoded_node.metadata.get(&metadata).map(String::as_str),
            Some("Revised")
        );
        assert_eq!(decoded_paths.documents[&document], document_path);

        let restoring_checkpoint = CheckpointId::from_bytes([9; 16]);
        let deleted = apply_project_command(
            &project,
            project.revision,
            ProjectCommand::delete_node_from_checkpoint(node, 1_725_000, restoring_checkpoint),
        )
        .unwrap()
        .project;
        let encoding = codec()
            .encode_domain_project(
                &deleted,
                &BTreeMap::new(),
                &BTreeMap::new(),
                &encoding.paths,
            )
            .unwrap();
        let manifest = codec()
            .decode_manifest(
                &encoding.resources[&CanonicalRelativePath::parse("project.toml").unwrap()].bytes,
            )
            .unwrap();
        let (decoded, _) = codec()
            .decode_domain_project(&manifest, project_id)
            .unwrap()
            .unwrap();
        let tombstone = &decoded.deleted[&node];
        assert_eq!(tombstone.restoring_checkpoint, Some(restoring_checkpoint));
        assert_eq!(tombstone.deleted_at_unix_millis, 1_725_000);
        assert_eq!(tombstone.subtree[0].node.title, "Research Note");
        assert_eq!(tombstone.subtree[0].node.kind, NodeKind::Document(document));
    }

    #[test]
    fn incremental_domain_encoding_reuses_unchanged_documents_and_deletes_removed_resources() {
        use parchmint_domain::{ProjectCommand, apply_project_command};

        let project_id = ProjectId::from_bytes([31; 16]);
        let group = NodeId::from_bytes([32; 16]);
        let first_node = NodeId::from_bytes([33; 16]);
        let second_node = NodeId::from_bytes([34; 16]);
        let first = DomainDocumentId::from_bytes([35; 16]);
        let second = DomainDocumentId::from_bytes([36; 16]);
        let mut project = Project::new(project_id);
        for command in [
            ProjectCommand::create_group(group, NodeId::manuscript_root(), 0, "Draft"),
            ProjectCommand::create_document(first_node, first, group, 0, "First"),
            ProjectCommand::create_document(second_node, second, group, 1, "Second"),
        ] {
            project = apply_project_command(&project, project.revision, command)
                .unwrap()
                .project;
        }
        let documents = BTreeMap::from([
            (first, "<p>first</p>".to_owned()),
            (second, "<p>second</p>".to_owned()),
        ]);
        let annotations = BTreeMap::from([(first, Vec::new()), (second, Vec::new())]);
        let frontier = CanonicalPersistenceFrontier {
            recovery_project_revision: 1,
            document_revisions: BTreeMap::from([(first, 1), (second, 1)]),
            ..Default::default()
        };
        let baseline = codec()
            .encode_domain_project_with_annotations(
                &project,
                &documents,
                &annotations,
                &BTreeMap::new(),
                &CanonicalProjectPathMap::default(),
                &frontier,
            )
            .unwrap();
        let existing = baseline
            .resources
            .iter()
            .map(|(path, resource)| (path.clone(), resource.bytes.clone()))
            .collect::<BTreeMap<_, _>>();
        let metadata = baseline
            .resources
            .iter()
            .map(|(path, resource)| {
                (
                    path.clone(),
                    CanonicalResourceMetadata {
                        resource: resource.resource.clone(),
                        hash: resource.hash,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let next_frontier = CanonicalPersistenceFrontier {
            recovery_project_revision: 2,
            document_revisions: BTreeMap::from([(first, 2), (second, 1)]),
            ..Default::default()
        };
        let patch = codec()
            .encode_domain_project_patch(
                &project,
                &CanonicalDomainUpdate {
                    documents: BTreeMap::from([(
                        first,
                        CanonicalDocumentUpdate {
                            body: "<p>changed</p>".to_owned(),
                            annotations: Vec::new(),
                        },
                    )]),
                    ..Default::default()
                },
                &existing,
                &metadata,
                &baseline.paths,
                &baseline.persistence_frontier,
                &next_frontier,
            )
            .unwrap();
        let first_path = &baseline.paths.documents[&first];
        let second_path = &baseline.paths.documents[&second];
        assert!(patch.resources.contains_key(first_path));
        assert!(!patch.resources.contains_key(second_path));
        assert!(
            patch
                .resources
                .keys()
                .any(|path| path.as_str() == "project.toml")
        );
        assert_eq!(patch.complete_resources[second_path], metadata[second_path]);

        let mut incomplete = metadata.clone();
        incomplete.remove(second_path);
        assert!(
            codec()
                .encode_domain_project_patch(
                    &project,
                    &CanonicalDomainUpdate {
                        documents: BTreeMap::from([(
                            first,
                            CanonicalDocumentUpdate {
                                body: "<p>changed</p>".to_owned(),
                                annotations: Vec::new(),
                            },
                        )]),
                        ..Default::default()
                    },
                    &existing,
                    &incomplete,
                    &baseline.paths,
                    &baseline.persistence_frontier,
                    &next_frontier,
                )
                .is_err()
        );

        let dictionary_project = apply_project_command(
            &project,
            project.revision,
            ProjectCommand::add_dictionary_word("ParchMint"),
        )
        .unwrap()
        .project;
        let dictionary_patch = codec()
            .encode_domain_project_patch(
                &dictionary_project,
                &CanonicalDomainUpdate {
                    dictionary: true,
                    ..Default::default()
                },
                &existing,
                &metadata,
                &baseline.paths,
                &baseline.persistence_frontier,
                &CanonicalPersistenceFrontier {
                    recovery_project_revision: 2,
                    document_revisions: BTreeMap::from([(first, 1), (second, 1)]),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(
            dictionary_patch
                .resources
                .keys()
                .map(CanonicalRelativePath::as_str)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["dictionary.txt", "project.toml"])
        );
        let full_dictionary = codec()
            .encode_domain_project_with_annotations(
                &dictionary_project,
                &documents,
                &annotations,
                &existing,
                &baseline.paths,
                &CanonicalPersistenceFrontier {
                    recovery_project_revision: 2,
                    document_revisions: BTreeMap::from([(first, 1), (second, 1)]),
                    ..Default::default()
                },
            )
            .unwrap();
        let mut assembled = existing.clone();
        for path in &dictionary_patch.deletions {
            assembled.remove(path);
        }
        for (path, resource) in &dictionary_patch.resources {
            assembled.insert(path.clone(), resource.bytes.clone());
        }
        assert_eq!(
            assembled,
            full_dictionary
                .resources
                .iter()
                .map(|(path, resource)| (path.clone(), resource.bytes.clone()))
                .collect()
        );

        let project = apply_project_command(
            &project,
            project.revision,
            ProjectCommand::delete_node(second_node),
        )
        .unwrap()
        .project;
        let deletion = codec()
            .encode_domain_project_patch(
                &project,
                &CanonicalDomainUpdate {
                    manifest: true,
                    ..Default::default()
                },
                &existing,
                &metadata,
                &baseline.paths,
                &baseline.persistence_frontier,
                &CanonicalPersistenceFrontier {
                    recovery_project_revision: 2,
                    document_revisions: BTreeMap::from([(first, 1)]),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(deletion.deletions.contains(second_path));
        assert!(deletion.deletions.iter().any(|path| {
            path.as_str() == format!("annotations/{}.json", stable_id_text(second.as_bytes()))
        }));
        assert!(!deletion.complete_resources.contains_key(second_path));
    }
}
