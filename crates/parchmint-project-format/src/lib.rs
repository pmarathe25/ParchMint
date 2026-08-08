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

use parchmint_contracts::generated::AnnotationSidecarV1;
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
    Document,
    Annotations { document_id: String },
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
            .parse::<toml::Value>()
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
        Ok(CanonicalAnnotations(canonicalize_annotations(sidecar)?))
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
        let hash = ContentHash(Sha256::digest(&bytes).into());
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
    fn migration_reencodes_a_complete_v1_snapshot_without_changing_resource_paths() {
        let document_path = CanonicalRelativePath::parse("manuscript/chapter.html").unwrap();
        let annotation_path = CanonicalRelativePath::parse("annotations/document-1.json").unwrap();
        let mut resources = BTreeMap::new();
        resources.insert(
            document_path.clone(),
            b"<p data-style-id=\"body\" data-block-id=\"block-1\">Text</p>".to_vec(),
        );
        resources.insert(
            annotation_path.clone(),
            br#"{"threads":[{"message":"line one\nline two"}],"document_id":"document-1","schema":"parchmint.annotation-sidecar/v1"}"#.to_vec(),
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
}
