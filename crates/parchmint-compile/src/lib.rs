#![allow(missing_docs)]
// The IR conversion and package writers intentionally keep a complete format
// traversal together. Splitting them solely to meet arbitrary line/argument
// limits would obscure the format-specific invariants they enforce.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::fn_params_excessive_bools,
    clippy::only_used_in_recursion,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::unnecessary_wraps
)]
//! Deterministic, Qt-independent manuscript compilation and v1 export.
//!
//! `CompileInput` is an immutable snapshot of canonical Rust state.  The
//! compiler never holds a Qt object and never mutates source Markdown; title,
//! scene and page boundaries are represented only in the resulting IR.

use parchmint_domain::{
    CompilePreset, CompileSeparator, DocumentId, DocumentTitleBehavior, NodeId, Project,
    ProjectTitleBehavior, ResearchInclusion, StyleId, StyleKind, WorkStamp,
};
use parchmint_markdown::{
    Alignment, Attributes, Block, BlockNode, Document, Inline, ListItem, ParseOptions,
};
use parchmint_storage::{AttachmentRecord, OpenProject, atomic_write};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tempfile::Builder as TempBuilder;
use thiserror::Error;

/// Bump when the public compile IR changes incompatibly.
pub const COMPILE_IR_VERSION: u32 = 1;

/// A cooperative cancellation handle that can safely cross the worker/UI boundary.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Immutable, worker-safe attachment reference used by exporters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileAsset {
    pub id: parchmint_domain::AssetId,
    pub display_name: String,
    pub safe_name: String,
    pub media_type: String,
    pub bytes: u64,
    /// Canonical source path. It is read only by an exporter while producing a
    /// temporary output; it is never written or treated as an executable URL.
    pub source_path: PathBuf,
}

/// A frozen canonical project snapshot. It is deliberately independent of the
/// live editor and can be moved to an application worker.
#[derive(Clone, Debug)]
pub struct CompileInput {
    pub project: Project,
    pub bodies: BTreeMap<DocumentId, String>,
    pub assets: BTreeMap<parchmint_domain::AssetId, CompileAsset>,
    pub stamp: WorkStamp,
}

impl CompileInput {
    pub fn from_open_project(opened: &OpenProject, stamp: WorkStamp) -> Result<Self, CompileError> {
        let mut bodies = BTreeMap::new();
        for id in opened.project.documents.keys().copied() {
            bodies.insert(id, opened.body(id)?.to_owned());
        }
        let assets = opened
            .attachments()
            .values()
            .map(|asset| (asset.id, compile_asset(opened.root(), asset)))
            .collect();
        Ok(Self {
            project: opened.project.clone(),
            bodies,
            assets,
            stamp,
        })
    }
}

fn compile_asset(root: &Path, asset: &AttachmentRecord) -> CompileAsset {
    CompileAsset {
        id: asset.id,
        display_name: asset.display_name.clone(),
        safe_name: asset.safe_name.clone(),
        media_type: asset.media_type.clone(),
        bytes: asset.bytes,
        source_path: root.join("assets").join(&asset.safe_name),
    }
}

/// The source of an IR block. Generated blocks intentionally identify their
/// owning document (where applicable) without pretending they were Markdown.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceProvenance {
    Generated {
        node_id: Option<NodeId>,
        document_id: Option<DocumentId>,
        role: &'static str,
    },
    Markdown {
        node_id: NodeId,
        document_id: DocumentId,
        start: usize,
        end: usize,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompileAttributes {
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub extra: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedStyle {
    pub id: StyleId,
    pub kind: StyleKind,
    pub class_name: String,
    pub properties: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompileInline {
    Text(String),
    Emphasis(Vec<Self>),
    Strong(Vec<Self>),
    Strikethrough(Vec<Self>),
    Code(String),
    Link {
        label: Vec<Self>,
        destination: String,
        title: Option<String>,
    },
    Image {
        alt: String,
        asset: Option<parchmint_domain::AssetId>,
        destination: String,
        title: Option<String>,
    },
    Superscript(Vec<Self>),
    Subscript(Vec<Self>),
    Styled {
        children: Vec<Self>,
        style: Option<ResolvedStyle>,
        attributes: CompileAttributes,
    },
    SoftBreak,
    HardBreak,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileListItem {
    pub checked: Option<bool>,
    pub content: Vec<CompileInline>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompileBlockKind {
    Title {
        text: String,
    },
    Heading {
        level: u8,
        content: Vec<CompileInline>,
        attributes: CompileAttributes,
    },
    Paragraph {
        content: Vec<CompileInline>,
        attributes: CompileAttributes,
    },
    BlockQuote {
        source: String,
    },
    CodeBlock {
        info: String,
        text: String,
    },
    List {
        ordered: bool,
        start: u64,
        items: Vec<CompileListItem>,
    },
    Table {
        source: String,
    },
    Footnote {
        source: String,
    },
    ThematicBreak,
    /// A compiler-inserted document boundary.
    SceneBreak,
    PageBreak,
    Alignment {
        alignment: Alignment,
        attributes: CompileAttributes,
        children: Vec<CompileBlock>,
    },
    /// Preserved source that a format cannot semantically represent.
    Opaque {
        reason: String,
        source: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileBlock {
    pub kind: CompileBlockKind,
    pub style: Option<ResolvedStyle>,
    pub provenance: SourceProvenance,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompileCounts {
    pub words: usize,
    pub characters: usize,
    pub blocks: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileIr {
    pub schema_version: u32,
    pub project_id: parchmint_domain::ProjectId,
    pub preset_id: parchmint_domain::CompilePresetId,
    pub stamp: WorkStamp,
    pub title: String,
    pub metadata: parchmint_domain::CompileMetadata,
    pub page: parchmint_domain::PageSettings,
    pub assets: BTreeMap<parchmint_domain::AssetId, CompileAsset>,
    pub styles: BTreeMap<StyleId, ResolvedStyle>,
    pub blocks: Vec<CompileBlock>,
    pub counts: CompileCounts,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WarningKind {
    Validation,
    UnsupportedContent,
    MissingAsset,
    Degradation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileWarning {
    pub kind: WarningKind,
    pub code: &'static str,
    pub message: String,
    pub node_id: Option<NodeId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewNode {
    pub node_id: NodeId,
    pub title: String,
    pub is_research: bool,
    pub included: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompilePreview {
    /// Nodes always appear in stable binder preorder, irrespective of selection order.
    pub nodes: Vec<PreviewNode>,
    pub warnings: Vec<CompileWarning>,
    pub approximate_counts: CompileCounts,
    pub final_counts: Option<CompileCounts>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompilePhase {
    Traversing,
    Parsing,
    Complete,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileProgress {
    pub stamp: WorkStamp,
    pub phase: CompilePhase,
    pub completed: usize,
    pub total: usize,
}

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("compile preset references missing or trashed root {0}")]
    MissingRoot(NodeId),
    #[error("compile preset has an invalid generated heading level {0}; expected 1 through 6")]
    InvalidHeadingLevel(u8),
    #[error("node {0} has no document record")]
    MissingDocument(NodeId),
    #[error("compile snapshot has no body for document {0}")]
    MissingBody(DocumentId),
    #[error("document {document} cannot be parsed for export: {message}")]
    Markdown {
        document: DocumentId,
        message: String,
    },
    #[error("style {0} cannot be resolved for export: {1}")]
    Style(StyleId, String),
    #[error("compile cancelled")]
    Cancelled,
    #[error(transparent)]
    Storage(#[from] parchmint_storage::StorageError),
}

/// Builds an ordered preview. Parsing also reports the same validation and
/// opaque-source warnings that a subsequent final compile will produce.
pub fn preview(
    input: &CompileInput,
    preset: &CompilePreset,
    cancellation: &CancellationToken,
) -> Result<CompilePreview, CompileError> {
    let result = compile_with_progress(input, preset, cancellation, &mut |_| {})?;
    Ok(CompilePreview {
        nodes: result.preview_nodes,
        warnings: result.warnings,
        approximate_counts: result.ir.counts.clone(),
        final_counts: Some(result.ir.counts),
    })
}

/// Compiles a frozen project snapshot into format-neutral semantic IR.
pub fn compile(
    input: &CompileInput,
    preset: &CompilePreset,
    cancellation: &CancellationToken,
) -> Result<(CompileIr, Vec<CompileWarning>), CompileError> {
    let result = compile_with_progress(input, preset, cancellation, &mut |_| {})?;
    Ok((result.ir, result.warnings))
}

/// Compile with monotonic, revision-carrying progress callbacks. A caller must
/// reject a completion whose `stamp` no longer matches its current workspace.
pub fn compile_with_progress(
    input: &CompileInput,
    preset: &CompilePreset,
    cancellation: &CancellationToken,
    progress: &mut dyn FnMut(CompileProgress),
) -> Result<CompileResult, CompileError> {
    if !(1..=6).contains(&preset.titles.document_heading_level) {
        return Err(CompileError::InvalidHeadingLevel(
            preset.titles.document_heading_level,
        ));
    }
    let selection = compile_selection_plan(input, preset)?;
    let candidates = selection
        .iter()
        .filter(|entry| entry.reason.is_none())
        .map(|entry| entry.node_id)
        .collect::<Vec<_>>();
    progress(CompileProgress {
        stamp: input.stamp,
        phase: CompilePhase::Traversing,
        completed: 0,
        total: candidates.len(),
    });

    let title = if preset.metadata.title.trim().is_empty() {
        input.project.name.clone()
    } else {
        preset.metadata.title.trim().to_owned()
    };
    let mut blocks = Vec::new();
    let mut warnings = Vec::new();
    let mut preview_nodes = selection
        .iter()
        .map(|entry| {
            let document_id = input.project.nodes[&entry.node_id]
                .kind
                .document_id()
                .expect("selection plan contains only document nodes");
            let title = input
                .project
                .documents
                .get(&document_id)
                .expect("project validation guarantees document records")
                .metadata
                .title
                .clone();
            PreviewNode {
                node_id: entry.node_id,
                title,
                is_research: is_research_node(&input.project, entry.node_id),
                included: entry.reason.is_none(),
                reason: entry.reason.clone(),
            }
        })
        .collect::<Vec<_>>();
    let mut styles = BTreeMap::new();
    let known_style_ids = input
        .project
        .styles
        .keys()
        .map(ToString::to_string)
        .collect::<BTreeSet<_>>();

    if preset.titles.project_title == ProjectTitleBehavior::Heading && !title.trim().is_empty() {
        blocks.push(CompileBlock {
            kind: CompileBlockKind::Title {
                text: title.clone(),
            },
            style: None,
            provenance: SourceProvenance::Generated {
                node_id: None,
                document_id: None,
                role: "project-title",
            },
        });
    }

    let mut emitted_documents = 0usize;
    for (index, node_id) in candidates.iter().copied().enumerate() {
        if cancellation.is_cancelled() {
            return Err(CompileError::Cancelled);
        }
        progress(CompileProgress {
            stamp: input.stamp,
            phase: CompilePhase::Parsing,
            completed: index,
            total: candidates.len(),
        });
        let node = &input.project.nodes[&node_id];
        let document_id = node
            .kind
            .document_id()
            .ok_or(CompileError::MissingDocument(node_id))?;
        let record = input
            .project
            .documents
            .get(&document_id)
            .ok_or(CompileError::MissingDocument(node_id))?;
        let body = input
            .bodies
            .get(&document_id)
            .ok_or(CompileError::MissingBody(document_id))?;

        if preset.inclusion.respect_include_flag
            && record.metadata.flags.get("include-in-compile") == Some(&false)
        {
            set_preview_excluded(&mut preview_nodes, node_id, "document flag excludes it");
            continue;
        }
        let document = Document::parse_body(
            body,
            &ParseOptions {
                known_style_ids: known_style_ids.clone(),
            },
        )
        .map_err(|error| CompileError::Markdown {
            document: document_id,
            message: error.to_string(),
        })?;
        if !preset.inclusion.include_empty_documents && document.blocks().is_empty() {
            set_preview_excluded(&mut preview_nodes, node_id, "empty document");
            continue;
        }
        set_preview_included(&mut preview_nodes, node_id);
        for diagnostic in document.diagnostics() {
            warnings.push(CompileWarning {
                kind: WarningKind::Validation,
                code: diagnostic.code,
                message: diagnostic.message.clone(),
                node_id: Some(node_id),
            });
        }
        if emitted_documents > 0 {
            let kind = match preset.separator {
                CompileSeparator::None => None,
                CompileSeparator::SceneBreak => Some(CompileBlockKind::SceneBreak),
                CompileSeparator::PageBreak => Some(CompileBlockKind::PageBreak),
            };
            if let Some(kind) = kind {
                blocks.push(CompileBlock {
                    kind,
                    style: None,
                    provenance: SourceProvenance::Generated {
                        node_id: Some(node_id),
                        document_id: Some(document_id),
                        role: "document-separator",
                    },
                });
            }
        }
        if preset.titles.document_titles == DocumentTitleBehavior::Heading
            && !record.metadata.title.trim().is_empty()
        {
            blocks.push(CompileBlock {
                kind: CompileBlockKind::Heading {
                    level: preset.titles.document_heading_level,
                    content: vec![CompileInline::Text(record.metadata.title.clone())],
                    attributes: CompileAttributes::default(),
                },
                style: None,
                provenance: SourceProvenance::Generated {
                    node_id: Some(node_id),
                    document_id: Some(document_id),
                    role: "document-title",
                },
            });
        }
        for (block_index, block) in document.blocks().iter().enumerate() {
            blocks.push(convert_block(
                input,
                preset,
                node_id,
                document_id,
                block_index,
                block,
                &mut styles,
                &mut warnings,
            )?);
        }
        emitted_documents += 1;
    }

    let counts = count_blocks(&blocks);
    progress(CompileProgress {
        stamp: input.stamp,
        phase: CompilePhase::Complete,
        completed: candidates.len(),
        total: candidates.len(),
    });
    Ok(CompileResult {
        ir: CompileIr {
            schema_version: COMPILE_IR_VERSION,
            project_id: input.project.id,
            preset_id: preset.id,
            stamp: input.stamp,
            title,
            metadata: preset.metadata.clone(),
            page: preset.page.clone(),
            assets: input.assets.clone(),
            styles,
            blocks,
            counts,
        },
        warnings,
        preview_nodes,
    })
}

#[derive(Clone, Debug)]
pub struct CompileResult {
    pub ir: CompileIr,
    pub warnings: Vec<CompileWarning>,
    pub preview_nodes: Vec<PreviewNode>,
}

fn set_preview_excluded(nodes: &mut [PreviewNode], node_id: NodeId, reason: &str) {
    if let Some(node) = nodes.iter_mut().find(|node| node.node_id == node_id) {
        node.included = false;
        node.reason = Some(reason.to_owned());
    }
}

fn set_preview_included(nodes: &mut [PreviewNode], node_id: NodeId) {
    if let Some(node) = nodes.iter_mut().find(|node| node.node_id == node_id) {
        node.included = true;
        node.reason = None;
    }
}

#[derive(Clone, Debug)]
struct SelectionPlanNode {
    node_id: NodeId,
    reason: Option<String>,
}

fn compile_selection_plan(
    input: &CompileInput,
    preset: &CompilePreset,
) -> Result<Vec<SelectionPlanNode>, CompileError> {
    let mut selected = if preset.selected_roots.is_empty() {
        BTreeSet::from([input.project.manuscript_root()])
    } else {
        preset
            .selected_roots
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
    };
    if preset.inclusion.research == ResearchInclusion::All {
        selected.insert(input.project.research_root());
    }
    for node in &selected {
        if !input.project.nodes.contains_key(node) || input.project.is_trashed(*node) {
            return Err(CompileError::MissingRoot(*node));
        }
    }
    let selected_research = selected
        .iter()
        .any(|node| is_research_node(&input.project, *node));
    let mut preorder = Vec::new();
    append_preorder(
        &input.project,
        input.project.manuscript_root(),
        &mut preorder,
    );
    append_preorder(&input.project, input.project.research_root(), &mut preorder);
    Ok(preorder
        .into_iter()
        .filter(|node| input.project.nodes[node].kind.document_id().is_some())
        .map(|node| {
            let selected_here =
                selected.contains(&node) || has_selected_ancestor(&input.project, node, &selected);
            let reason = if is_research_node(&input.project, node) {
                match preset.inclusion.research {
                    ResearchInclusion::Exclude => {
                        Some("research is disabled by this preset".into())
                    }
                    ResearchInclusion::SelectedRoots if !selected_research => {
                        Some("research was not explicitly selected".into())
                    }
                    _ if !selected_here => Some("not selected by this preset".into()),
                    _ => None,
                }
            } else if !selected_here {
                Some("not selected by this preset".into())
            } else {
                None
            };
            SelectionPlanNode {
                node_id: node,
                reason,
            }
        })
        .collect())
}

fn append_preorder(project: &Project, node: NodeId, out: &mut Vec<NodeId>) {
    if project.is_trashed(node) {
        return;
    }
    out.push(node);
    for child in &project.nodes[&node].children {
        append_preorder(project, *child, out);
    }
}

fn has_selected_ancestor(project: &Project, node: NodeId, selected: &BTreeSet<NodeId>) -> bool {
    let mut current = project.nodes[&node].parent;
    while let Some(id) = current {
        if selected.contains(&id) {
            return true;
        }
        current = project.nodes[&id].parent;
    }
    false
}

fn is_research_node(project: &Project, node: NodeId) -> bool {
    let mut current = Some(node);
    while let Some(id) = current {
        if id == project.research_root() {
            return true;
        }
        current = project.nodes.get(&id).and_then(|entry| entry.parent);
    }
    false
}

#[allow(clippy::too_many_arguments)]
fn convert_block(
    input: &CompileInput,
    preset: &CompilePreset,
    node_id: NodeId,
    document_id: DocumentId,
    block_index: usize,
    block: &Block,
    styles: &mut BTreeMap<StyleId, ResolvedStyle>,
    warnings: &mut Vec<CompileWarning>,
) -> Result<CompileBlock, CompileError> {
    let provenance = SourceProvenance::Markdown {
        node_id,
        document_id,
        start: block.range.start,
        end: block.range.end,
    };
    let (kind, style) = match &block.node {
        BlockNode::Paragraph {
            content,
            attributes,
        } => (
            CompileBlockKind::Paragraph {
                content: convert_inlines(input, preset, node_id, content, styles, warnings)?,
                attributes: convert_attributes(attributes),
            },
            resolve_style(input, preset, node_id, attributes, styles, warnings)?,
        ),
        BlockNode::Heading {
            level,
            content,
            attributes,
        } => (
            CompileBlockKind::Heading {
                level: (*level).clamp(1, 6),
                content: convert_inlines(input, preset, node_id, content, styles, warnings)?,
                attributes: convert_attributes(attributes),
            },
            resolve_style(input, preset, node_id, attributes, styles, warnings)?,
        ),
        BlockNode::BlockQuote { source } => (
            CompileBlockKind::BlockQuote {
                source: source.clone(),
            },
            None,
        ),
        BlockNode::CodeBlock { info, text } => (
            CompileBlockKind::CodeBlock {
                info: info.clone(),
                text: text.clone(),
            },
            None,
        ),
        BlockNode::List {
            ordered,
            start,
            items,
        } => (
            CompileBlockKind::List {
                ordered: *ordered,
                start: *start,
                items: items
                    .iter()
                    .map(|item| convert_list_item(input, preset, node_id, item, styles, warnings))
                    .collect::<Result<Vec<_>, _>>()?,
            },
            None,
        ),
        BlockNode::Table { source } => (
            CompileBlockKind::Table {
                source: source.clone(),
            },
            None,
        ),
        BlockNode::Footnote { source } => (
            CompileBlockKind::Footnote {
                source: source.clone(),
            },
            None,
        ),
        BlockNode::ThematicBreak => (CompileBlockKind::ThematicBreak, None),
        BlockNode::PageBreak => (CompileBlockKind::PageBreak, None),
        BlockNode::Opaque { reason, source } => {
            warnings.push(CompileWarning {
                kind: WarningKind::UnsupportedContent,
                code: "opaque-markdown",
                message: format!("Preserved unsupported source: {reason}"),
                node_id: Some(node_id),
            });
            (
                CompileBlockKind::Opaque {
                    reason: reason.clone(),
                    source: source.clone(),
                },
                None,
            )
        }
        BlockNode::Alignment {
            alignment,
            attributes,
            children,
        } => (
            CompileBlockKind::Alignment {
                alignment: *alignment,
                attributes: convert_attributes(attributes),
                children: children
                    .iter()
                    .map(|child| {
                        convert_block(
                            input,
                            preset,
                            node_id,
                            document_id,
                            block_index,
                            child,
                            styles,
                            warnings,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            },
            resolve_style(input, preset, node_id, attributes, styles, warnings)?,
        ),
    };
    Ok(CompileBlock {
        kind,
        style,
        provenance,
    })
}

fn convert_list_item(
    input: &CompileInput,
    preset: &CompilePreset,
    node_id: NodeId,
    item: &ListItem,
    styles: &mut BTreeMap<StyleId, ResolvedStyle>,
    warnings: &mut Vec<CompileWarning>,
) -> Result<CompileListItem, CompileError> {
    Ok(CompileListItem {
        checked: item.checked,
        content: convert_inlines(input, preset, node_id, &item.content, styles, warnings)?,
    })
}

fn convert_attributes(attributes: &Attributes) -> CompileAttributes {
    CompileAttributes {
        id: attributes.id.clone(),
        classes: attributes.classes.clone(),
        extra: attributes.extra.clone(),
    }
}

fn resolve_style(
    input: &CompileInput,
    preset: &CompilePreset,
    node_id: NodeId,
    attributes: &Attributes,
    styles: &mut BTreeMap<StyleId, ResolvedStyle>,
    warnings: &mut Vec<CompileWarning>,
) -> Result<Option<ResolvedStyle>, CompileError> {
    let Some(id) = attributes.style_id.as_deref() else {
        return Ok(None);
    };
    let Ok(id) = StyleId::parse(id) else {
        warnings.push(CompileWarning {
            kind: WarningKind::Validation,
            code: "invalid-style-id",
            message: "A style attribute does not contain a stable UUID.".into(),
            node_id: Some(node_id),
        });
        return Ok(None);
    };
    let computed = match input.project.computed_style(id) {
        Ok(style) => style,
        Err(error) => {
            warnings.push(CompileWarning {
                kind: WarningKind::Validation,
                code: "unknown-style-id",
                message: format!("Style {id} cannot be resolved: {error}"),
                node_id: Some(node_id),
            });
            return Ok(None);
        }
    };
    let mapping = preset.style_mapping.get(&id);
    let mut properties = computed.properties;
    if let Some(mapping) = mapping {
        properties.extend(mapping.properties.clone());
    }
    let style = ResolvedStyle {
        id,
        kind: computed.kind,
        class_name: mapping
            .map(|entry| sanitize_class(&entry.class_name))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| format!("pm-style-{id}")),
        properties,
    };
    styles.insert(id, style.clone());
    Ok(Some(style))
}

fn convert_inlines(
    input: &CompileInput,
    preset: &CompilePreset,
    node_id: NodeId,
    inlines: &[Inline],
    styles: &mut BTreeMap<StyleId, ResolvedStyle>,
    warnings: &mut Vec<CompileWarning>,
) -> Result<Vec<CompileInline>, CompileError> {
    inlines
        .iter()
        .map(|inline| match inline {
            Inline::Text(text) => Ok(CompileInline::Text(text.clone())),
            Inline::Emphasis(children) => Ok(CompileInline::Emphasis(convert_inlines(
                input, preset, node_id, children, styles, warnings,
            )?)),
            Inline::Strong(children) => Ok(CompileInline::Strong(convert_inlines(
                input, preset, node_id, children, styles, warnings,
            )?)),
            Inline::Strikethrough(children) => Ok(CompileInline::Strikethrough(convert_inlines(
                input, preset, node_id, children, styles, warnings,
            )?)),
            Inline::Code(text) => Ok(CompileInline::Code(text.clone())),
            Inline::Link {
                label,
                destination,
                title,
            } => Ok(CompileInline::Link {
                label: convert_inlines(input, preset, node_id, label, styles, warnings)?,
                destination: destination.clone(),
                title: title.clone(),
            }),
            Inline::Image {
                alt,
                destination,
                title,
            } => {
                let asset = destination
                    .strip_prefix("asset:")
                    .and_then(|id| parchmint_domain::AssetId::parse(id).ok());
                if destination.starts_with("asset:") {
                    match asset.and_then(|id| input.assets.get(&id)) {
                        Some(asset) if asset.source_path.is_file() => {}
                        Some(_) => warnings.push(CompileWarning {
                            kind: WarningKind::MissingAsset,
                            code: "missing-asset-file",
                            message: format!("Asset `{destination}` is missing from the project."),
                            node_id: Some(node_id),
                        }),
                        None => warnings.push(CompileWarning {
                            kind: WarningKind::MissingAsset,
                            code: "unknown-asset",
                            message: format!(
                                "Asset `{destination}` is not in the project catalog."
                            ),
                            node_id: Some(node_id),
                        }),
                    }
                }
                Ok(CompileInline::Image {
                    alt: alt.clone(),
                    asset,
                    destination: destination.clone(),
                    title: title.clone(),
                })
            }
            Inline::Superscript(children) => Ok(CompileInline::Superscript(convert_inlines(
                input, preset, node_id, children, styles, warnings,
            )?)),
            Inline::Subscript(children) => Ok(CompileInline::Subscript(convert_inlines(
                input, preset, node_id, children, styles, warnings,
            )?)),
            Inline::Styled {
                children,
                attributes,
            } => Ok(CompileInline::Styled {
                children: convert_inlines(input, preset, node_id, children, styles, warnings)?,
                style: resolve_style(input, preset, node_id, attributes, styles, warnings)?,
                attributes: convert_attributes(attributes),
            }),
            Inline::SoftBreak => Ok(CompileInline::SoftBreak),
            Inline::HardBreak => Ok(CompileInline::HardBreak),
        })
        .collect()
}

fn count_blocks(blocks: &[CompileBlock]) -> CompileCounts {
    let text = blocks.iter().map(plain_block).collect::<String>();
    CompileCounts {
        words: count_words(&text),
        characters: text.chars().count(),
        blocks: count_block_total(blocks),
    }
}

fn count_block_total(blocks: &[CompileBlock]) -> usize {
    blocks
        .iter()
        .map(|block| {
            1 + match &block.kind {
                CompileBlockKind::Alignment { children, .. } => count_block_total(children),
                _ => 0,
            }
        })
        .sum()
}

fn count_words(text: &str) -> usize {
    let mut words = 0;
    let mut in_word = false;
    for character in text.chars() {
        let joiner = matches!(character, '\'' | '\u{2019}' | '-' | '\u{2011}');
        if character.is_alphanumeric() {
            if !in_word {
                words += 1;
            }
            in_word = true;
        } else if !joiner {
            in_word = false;
        }
    }
    words
}

fn sanitize_class(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        .take(80)
        .collect()
}

/// All stable version-1 exporter targets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExportFormat {
    Markdown,
    PlainText,
    Html,
    Pdf,
    Epub,
    Docx,
}

impl ExportFormat {
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::PlainText => "txt",
            Self::Html => "html",
            Self::Pdf => "pdf",
            Self::Epub => "epub",
            Self::Docx => "docx",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CollisionPolicy {
    /// Refuse to touch an existing destination. This is the default for UI use.
    #[default]
    Fail,
    /// Atomically replace an existing file only after output construction and
    /// structural validation completed successfully.
    ReplaceFile,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MarkdownOutput {
    #[default]
    CombinedFile,
    /// One deterministic file per source document is intentionally limited to
    /// destinations that do not yet exist, avoiding a non-atomic directory
    /// replacement across platforms.
    Directory,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HtmlAssetMode {
    /// Embed known image assets as data URLs, producing a single portable file.
    #[default]
    SelfContained,
    /// Use safe `assets/<uuid-name>` paths and report a warning that callers
    /// must copy the matching assets beside the exported document.
    Relative,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportOptions {
    pub format: ExportFormat,
    pub destination: PathBuf,
    pub collision: CollisionPolicy,
    pub markdown_output: MarkdownOutput,
    pub html_asset_mode: HtmlAssetMode,
    /// Plain text boundary between ordinary source documents. An empty setting
    /// is normalized to two newlines so files never accidentally join words.
    pub text_separator: String,
}

impl ExportOptions {
    pub fn file(format: ExportFormat, destination: impl Into<PathBuf>) -> Self {
        Self {
            format,
            destination: destination.into(),
            collision: CollisionPolicy::Fail,
            markdown_output: MarkdownOutput::CombinedFile,
            html_asset_mode: HtmlAssetMode::SelfContained,
            text_separator: "\n\n".into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportWarning {
    pub code: &'static str,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportReport {
    pub format: ExportFormat,
    pub destination: PathBuf,
    pub bytes: u64,
    pub warnings: Vec<ExportWarning>,
}

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("export cancelled before destination replacement")]
    Cancelled,
    #[error("export destination must name a file or directory: {0}")]
    InvalidDestination(PathBuf),
    #[error("export destination already exists and collision policy is fail: {0}")]
    DestinationExists(PathBuf),
    #[error("directory Markdown output cannot replace an existing destination: {0}")]
    DirectoryReplacement(PathBuf),
    #[error("export output failed structural validation: {0}")]
    Validation(String),
    #[error("could not prepare export destination {path}: {error}")]
    PrepareDestination {
        path: PathBuf,
        error: std::io::Error,
    },
    #[error("could not write export output {path}: {error}")]
    Write {
        path: PathBuf,
        error: std::io::Error,
    },
    #[error(transparent)]
    Atomic(#[from] parchmint_storage::AtomicWriteError),
}

/// Produces, validates, and atomically installs a v1 export. Existing files
/// are untouched until the final `atomic_write` after validation succeeds.
pub fn export(ir: &CompileIr, options: &ExportOptions) -> Result<ExportReport, ExportError> {
    export_cancellable(ir, options, &CancellationToken::default())
}

/// Same as [`export`], with a cooperative cancellation check before temporary
/// output becomes a destination replacement. UI code cancels this token when
/// its workspace revision advances, then rejects any non-current completion.
pub fn export_cancellable(
    ir: &CompileIr,
    options: &ExportOptions,
    cancellation: &CancellationToken,
) -> Result<ExportReport, ExportError> {
    if cancellation.is_cancelled() {
        return Err(ExportError::Cancelled);
    }
    if options.format == ExportFormat::Markdown
        && options.markdown_output == MarkdownOutput::Directory
    {
        return export_markdown_directory(ir, options, cancellation);
    }
    validate_file_destination(options)?;
    let (bytes, warnings) = match options.format {
        ExportFormat::Markdown => (render_markdown(ir).into_bytes(), Vec::new()),
        ExportFormat::PlainText => (
            render_plain_text(ir, &options.text_separator).into_bytes(),
            Vec::new(),
        ),
        ExportFormat::Html => {
            let (html, warnings) = render_html(ir, options.html_asset_mode);
            (html.into_bytes(), warnings)
        }
        ExportFormat::Pdf => render_pdf(ir),
        ExportFormat::Epub => render_epub(ir),
        ExportFormat::Docx => render_docx(ir),
    };
    if cancellation.is_cancelled() {
        return Err(ExportError::Cancelled);
    }
    validate_export(options.format, &bytes)?;
    if cancellation.is_cancelled() {
        return Err(ExportError::Cancelled);
    }
    atomic_write(&options.destination, &bytes)?;
    Ok(ExportReport {
        format: options.format,
        destination: options.destination.clone(),
        bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        warnings,
    })
}

fn validate_file_destination(options: &ExportOptions) -> Result<(), ExportError> {
    let destination = &options.destination;
    let Some(parent) = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    else {
        return Err(ExportError::InvalidDestination(destination.clone()));
    };
    if destination.file_name().is_none() {
        return Err(ExportError::InvalidDestination(destination.clone()));
    }
    fs::create_dir_all(parent).map_err(|error| ExportError::PrepareDestination {
        path: parent.to_owned(),
        error,
    })?;
    if destination.exists() && options.collision == CollisionPolicy::Fail {
        return Err(ExportError::DestinationExists(destination.clone()));
    }
    if destination.is_dir() {
        return Err(ExportError::InvalidDestination(destination.clone()));
    }
    Ok(())
}

fn export_markdown_directory(
    ir: &CompileIr,
    options: &ExportOptions,
    cancellation: &CancellationToken,
) -> Result<ExportReport, ExportError> {
    let destination = &options.destination;
    let Some(parent) = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    else {
        return Err(ExportError::InvalidDestination(destination.clone()));
    };
    if destination.exists() {
        return Err(if options.collision == CollisionPolicy::ReplaceFile {
            ExportError::DirectoryReplacement(destination.clone())
        } else {
            ExportError::DestinationExists(destination.clone())
        });
    }
    fs::create_dir_all(parent).map_err(|error| ExportError::PrepareDestination {
        path: parent.to_owned(),
        error,
    })?;
    let temporary = TempBuilder::new()
        .prefix(".parchmint-export-")
        .tempdir_in(parent)
        .map_err(|error| ExportError::PrepareDestination {
            path: parent.to_owned(),
            error,
        })?;
    let mut count = 0u64;
    let mut current = String::new();
    let mut part = 1usize;
    for block in &ir.blocks {
        if cancellation.is_cancelled() {
            return Err(ExportError::Cancelled);
        }
        current.push_str(&markdown_block(block));
        if matches!(
            block.provenance,
            SourceProvenance::Generated {
                role: "document-separator",
                ..
            }
        ) {
            let path = temporary.path().join(format!("{part:04}.md"));
            fs::write(&path, &current).map_err(|error| ExportError::Write { path, error })?;
            count = count.saturating_add(u64::try_from(current.len()).unwrap_or(u64::MAX));
            current.clear();
            part += 1;
        }
    }
    if !current.is_empty() {
        let path = temporary.path().join(format!("{part:04}.md"));
        fs::write(&path, &current).map_err(|error| ExportError::Write { path, error })?;
        count = count.saturating_add(u64::try_from(current.len()).unwrap_or(u64::MAX));
    }
    if cancellation.is_cancelled() {
        return Err(ExportError::Cancelled);
    }
    fs::rename(temporary.path(), destination).map_err(|error| ExportError::Write {
        path: destination.clone(),
        error,
    })?;
    Ok(ExportReport {
        format: ExportFormat::Markdown,
        destination: destination.clone(),
        bytes: count,
        warnings: Vec::new(),
    })
}

/// Deterministic ParchMint-flavoured Markdown. It deliberately preserves
/// opaque source rather than guessing at a semantic conversion.
pub fn render_markdown(ir: &CompileIr) -> String {
    ir.blocks.iter().map(markdown_block).collect()
}

fn markdown_block(block: &CompileBlock) -> String {
    match &block.kind {
        CompileBlockKind::Title { text } => format!("# {}\n\n", markdown_text(text)),
        CompileBlockKind::Heading {
            level,
            content,
            attributes,
        } => format!(
            "{} {}{}\n\n",
            "#".repeat(usize::from(*level)),
            markdown_inlines(content),
            markdown_attributes(attributes, true)
        ),
        CompileBlockKind::Paragraph {
            content,
            attributes,
        } => format!(
            "{}{}\n\n",
            markdown_inlines(content),
            markdown_attributes(attributes, true)
        ),
        CompileBlockKind::BlockQuote { source }
        | CompileBlockKind::Table { source }
        | CompileBlockKind::Footnote { source }
        | CompileBlockKind::Opaque { source, .. } => spaced(source),
        CompileBlockKind::CodeBlock { info, text } => format!(
            "```{info}\n{}{}```\n\n",
            text,
            if text.ends_with('\n') { "" } else { "\n" }
        ),
        CompileBlockKind::List {
            ordered,
            start,
            items,
        } => {
            let mut result = String::new();
            for (index, item) in items.iter().enumerate() {
                let marker = if *ordered {
                    format!("{}.", start + u64::try_from(index).unwrap_or(0))
                } else {
                    "-".into()
                };
                let task = match item.checked {
                    Some(true) => "[x] ",
                    Some(false) => "[ ] ",
                    None => "",
                };
                let _ = writeln!(result, "{marker} {task}{}", markdown_inlines(&item.content));
            }
            result.push('\n');
            result
        }
        CompileBlockKind::ThematicBreak | CompileBlockKind::SceneBreak => "---\n\n".into(),
        CompileBlockKind::PageBreak => "<!-- parchmint:page-break -->\n\n".into(),
        CompileBlockKind::Alignment {
            alignment,
            attributes,
            children,
        } => {
            let mut attributes = attributes.clone();
            if !attributes
                .classes
                .iter()
                .any(|class| class == "parchmint-align")
            {
                attributes.classes.push("parchmint-align".into());
            }
            attributes
                .extra
                .insert("align".into(), alignment_name(*alignment).into());
            format!(
                ":::\n{}:::\n\n",
                children.iter().map(markdown_block).collect::<String>()
            )
            .replacen(
                ":::\n",
                &format!("::: {}\n", markdown_attributes(&attributes, false)),
                1,
            )
        }
    }
}

fn markdown_inlines(inlines: &[CompileInline]) -> String {
    let mut result = String::new();
    for inline in inlines {
        match inline {
            CompileInline::Text(text) => result.push_str(&markdown_text(text)),
            CompileInline::Emphasis(children) => {
                let _ = write!(result, "*{}*", markdown_inlines(children));
            }
            CompileInline::Strong(children) => {
                let _ = write!(result, "**{}**", markdown_inlines(children));
            }
            CompileInline::Strikethrough(children) => {
                let _ = write!(result, "~~{}~~", markdown_inlines(children));
            }
            CompileInline::Code(text) => {
                let fence = if text.contains('`') { "``" } else { "`" };
                let _ = write!(result, "{fence}{text}{fence}");
            }
            CompileInline::Link {
                label,
                destination,
                title,
            } => {
                let _ = write!(
                    result,
                    "[{}]({}",
                    markdown_inlines(label),
                    destination.replace(')', "\\)")
                );
                if let Some(title) = title {
                    let _ = write!(result, " \"{}\"", title.replace('"', "\\\""));
                }
                result.push(')');
            }
            CompileInline::Image {
                alt,
                destination,
                title,
                ..
            } => {
                let _ = write!(
                    result,
                    "![{}]({}",
                    alt.replace(']', "\\]"),
                    destination.replace(')', "\\)")
                );
                if let Some(title) = title {
                    let _ = write!(result, " \"{}\"", title.replace('"', "\\\""));
                }
                result.push(')');
            }
            CompileInline::Superscript(children) => {
                let _ = write!(result, "<sup>{}</sup>", markdown_inlines(children));
            }
            CompileInline::Subscript(children) => {
                let _ = write!(result, "<sub>{}</sub>", markdown_inlines(children));
            }
            CompileInline::Styled {
                children,
                style,
                attributes,
            } => {
                let mut attributes = attributes.clone();
                if let Some(style) = style {
                    attributes.classes.push("parchmint-style".into());
                    attributes
                        .extra
                        .insert("style-id".into(), style.id.to_string());
                }
                let _ = write!(
                    result,
                    "[{}]{}",
                    markdown_inlines(children),
                    markdown_attributes(&attributes, false)
                );
            }
            CompileInline::SoftBreak => result.push('\n'),
            CompileInline::HardBreak => result.push_str("  \n"),
        }
    }
    result
}

fn markdown_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('*', "\\*")
        .replace('_', "\\_")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

fn markdown_attributes(attributes: &CompileAttributes, leading_space: bool) -> String {
    if attributes.id.is_none() && attributes.classes.is_empty() && attributes.extra.is_empty() {
        return String::new();
    }
    let mut values = Vec::new();
    if let Some(id) = &attributes.id {
        values.push(format!("#{id}"));
    }
    values.extend(attributes.classes.iter().map(|class| format!(".{class}")));
    values.extend(attributes.extra.iter().map(|(key, value)| {
        format!(
            "{key}=\"{}\"",
            value.replace('\\', "\\\\").replace('"', "\\\"")
        )
    }));
    format!(
        "{}{{{}}}",
        if leading_space { " " } else { "" },
        values.join(" ")
    )
}

/// Plain text normalization uses Unicode text, two newlines between blocks, a
/// line containing `***` for scene breaks, and U+000C for semantic page breaks.
pub fn render_plain_text(ir: &CompileIr, separator: &str) -> String {
    let separator = if separator.is_empty() {
        "\n\n"
    } else {
        separator
    };
    let mut output = String::new();
    for block in &ir.blocks {
        let value = plain_block(block);
        if value.is_empty() {
            continue;
        }
        if !output.is_empty() && !output.ends_with('\u{c}') && !value.starts_with('\u{c}') {
            output.push_str(separator);
        }
        output.push_str(&value);
    }
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output
}

fn plain_block(block: &CompileBlock) -> String {
    match &block.kind {
        CompileBlockKind::Title { text } | CompileBlockKind::CodeBlock { text, .. } => text.clone(),
        CompileBlockKind::Heading { content, .. } | CompileBlockKind::Paragraph { content, .. } => {
            plain_inlines(content)
        }
        CompileBlockKind::BlockQuote { source }
        | CompileBlockKind::Table { source }
        | CompileBlockKind::Footnote { source }
        | CompileBlockKind::Opaque { source, .. } => source.clone(),
        CompileBlockKind::List { items, .. } => items
            .iter()
            .map(|item| format!("• {}", plain_inlines(&item.content)))
            .collect::<Vec<_>>()
            .join("\n"),
        CompileBlockKind::ThematicBreak | CompileBlockKind::SceneBreak => "***".into(),
        CompileBlockKind::PageBreak => "\u{c}".into(),
        CompileBlockKind::Alignment { children, .. } => children
            .iter()
            .map(plain_block)
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn plain_inlines(inlines: &[CompileInline]) -> String {
    inlines
        .iter()
        .map(|inline| match inline {
            CompileInline::Text(text) | CompileInline::Code(text) => text.clone(),
            CompileInline::Emphasis(children)
            | CompileInline::Strong(children)
            | CompileInline::Strikethrough(children)
            | CompileInline::Superscript(children)
            | CompileInline::Subscript(children)
            | CompileInline::Styled { children, .. } => plain_inlines(children),
            CompileInline::Link { label, .. } => plain_inlines(label),
            CompileInline::Image { alt, .. } => alt.clone(),
            CompileInline::SoftBreak | CompileInline::HardBreak => "\n".into(),
        })
        .collect()
}

/// Renders semantic HTML5 with generated CSS. Only `http`, `https`, `mailto`,
/// fragments, and project asset references become links; unsafe schemes are
/// displayed as text with an actionable warning.
pub fn render_html(ir: &CompileIr, asset_mode: HtmlAssetMode) -> (String, Vec<ExportWarning>) {
    let mut warnings = Vec::new();
    let mut body = String::new();
    for block in &ir.blocks {
        html_block(&mut body, block, ir, asset_mode, &mut warnings);
    }
    let language = if ir.metadata.language.trim().is_empty() {
        "en"
    } else {
        &ir.metadata.language
    };
    let mut output = String::new();
    let _ = write!(
        output,
        "<!doctype html>\n<html lang=\"{}\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{}</title>\n",
        html_escape(language),
        html_escape(&ir.title)
    );
    if !ir.metadata.author.trim().is_empty() {
        let _ = writeln!(
            output,
            "<meta name=\"author\" content=\"{}\">",
            html_escape(&ir.metadata.author)
        );
    }
    if !ir.metadata.subject.trim().is_empty() {
        let _ = writeln!(
            output,
            "<meta name=\"description\" content=\"{}\">",
            html_escape(&ir.metadata.subject)
        );
    }
    output.push_str("<style>\n");
    output.push_str(&html_css(ir));
    output.push_str("</style>\n</head>\n<body>\n");
    output.push_str(&body);
    output.push_str("</body>\n</html>\n");
    (output, warnings)
}

fn html_css(ir: &CompileIr) -> String {
    let mut css = String::from(
        "body{max-width:48rem;margin:2rem auto;padding:0 1rem;color:#1f1f1f;font-family:serif;line-height:1.55} img{max-width:100%;height:auto} pre{white-space:pre-wrap}.scene-break{border:0;border-top:1px solid currentColor;width:20%;margin:2rem auto}.page-break{break-before:page;border:0}.opaque{background:#fff6e5;padding:.75rem;white-space:pre-wrap}.task{margin-right:.45em}\n",
    );
    for style in ir.styles.values() {
        let _ = write!(css, ".{}{{", sanitize_class(&style.class_name));
        for (key, value) in &style.properties {
            if let Some(property) = css_property(key) {
                let value = css_value(value);
                if !value.is_empty() {
                    let _ = write!(css, "{property}:{value};");
                }
            }
        }
        css.push_str("}\n");
    }
    css
}

fn css_property(key: &str) -> Option<&'static str> {
    match key {
        "alignment" => Some("text-align"),
        "background" => Some("background-color"),
        "font-family" => Some("font-family"),
        "font-size" => Some("font-size"),
        "font-style" => Some("font-style"),
        "font-weight" => Some("font-weight"),
        "foreground" => Some("color"),
        "line-height" => Some("line-height"),
        "space-after" => Some("margin-bottom"),
        "space-before" => Some("margin-top"),
        "text-decoration" => Some("text-decoration"),
        _ => None,
    }
}

fn css_value(value: &str) -> String {
    value
        .chars()
        .filter(|character| {
            !character.is_control() && !matches!(character, ';' | '{' | '}' | '<' | '>')
        })
        .take(256)
        .collect()
}

fn html_block(
    output: &mut String,
    block: &CompileBlock,
    ir: &CompileIr,
    asset_mode: HtmlAssetMode,
    warnings: &mut Vec<ExportWarning>,
) {
    let class = block
        .style
        .as_ref()
        .map(|style| {
            format!(
                " class=\"{}\"",
                html_escape(&sanitize_class(&style.class_name))
            )
        })
        .unwrap_or_default();
    match &block.kind {
        CompileBlockKind::Title { text } => {
            let _ = writeln!(
                output,
                "<h1 class=\"pm-project-title\">{}</h1>",
                html_escape(text)
            );
        }
        CompileBlockKind::Heading {
            level,
            content,
            attributes,
        } => {
            let level = (*level).clamp(1, 6);
            let id = html_id(attributes);
            let _ = write!(output, "<h{level}{id}{class}>");
            html_inlines(output, content, ir, asset_mode, warnings);
            let _ = writeln!(output, "</h{level}>");
        }
        CompileBlockKind::Paragraph {
            content,
            attributes,
        } => {
            let id = html_id(attributes);
            let _ = write!(output, "<p{id}{class}>");
            html_inlines(output, content, ir, asset_mode, warnings);
            output.push_str("</p>\n");
        }
        CompileBlockKind::BlockQuote { source } => {
            let _ = writeln!(
                output,
                "<blockquote{}><pre>{}</pre></blockquote>",
                class,
                html_escape(source)
            );
        }
        CompileBlockKind::CodeBlock { info, text } => {
            let _ = writeln!(
                output,
                "<pre{}><code class=\"language-{}\">{}</code></pre>",
                class,
                html_escape(info),
                html_escape(text)
            );
        }
        CompileBlockKind::List {
            ordered,
            start,
            items,
        } => {
            if *ordered {
                let _ = writeln!(output, "<ol{class} start=\"{start}\">");
            } else {
                let _ = writeln!(output, "<ul{class}>");
            }
            for item in items {
                output.push_str("<li>");
                if let Some(checked) = item.checked {
                    let _ = write!(
                        output,
                        "<input class=\"task\" type=\"checkbox\" disabled{}>",
                        if checked { " checked" } else { "" }
                    );
                }
                html_inlines(output, &item.content, ir, asset_mode, warnings);
                output.push_str("</li>\n");
            }
            output.push_str(if *ordered { "</ol>\n" } else { "</ul>\n" });
        }
        CompileBlockKind::Table { source } => {
            let _ = writeln!(
                output,
                "<pre class=\"pm-table{}\">{}</pre>",
                class,
                html_escape(source)
            );
            warnings.push(ExportWarning { code: "html-table-source", message: "A source-preserved table was emitted as preformatted text because the canonical table AST is intentionally source-backed.".into() });
        }
        CompileBlockKind::Footnote { source } => {
            let _ = writeln!(
                output,
                "<aside class=\"pm-footnote{}\"><pre>{}</pre></aside>",
                class,
                html_escape(source)
            );
        }
        CompileBlockKind::ThematicBreak | CompileBlockKind::SceneBreak => {
            output.push_str("<hr class=\"scene-break\">\n");
        }
        CompileBlockKind::PageBreak => output.push_str("<hr class=\"page-break\">\n"),
        CompileBlockKind::Alignment {
            alignment,
            children,
            ..
        } => {
            let _ = writeln!(
                output,
                "<section style=\"text-align:{}\">",
                alignment_name(*alignment)
            );
            for child in children {
                html_block(output, child, ir, asset_mode, warnings);
            }
            output.push_str("</section>\n");
        }
        CompileBlockKind::Opaque { reason, source } => {
            let _ = writeln!(
                output,
                "<pre class=\"opaque\" data-parchmint-reason=\"{}\">{}</pre>",
                html_escape(reason),
                html_escape(source)
            );
            warnings.push(ExportWarning {
                code: "html-opaque-source",
                message: format!("Unsupported source was visibly preserved in HTML: {reason}"),
            });
        }
    }
}

fn html_id(attributes: &CompileAttributes) -> String {
    attributes
        .id
        .as_deref()
        .map(|id| format!(" id=\"{}\"", html_escape(id)))
        .unwrap_or_default()
}

fn html_inlines(
    output: &mut String,
    inlines: &[CompileInline],
    ir: &CompileIr,
    asset_mode: HtmlAssetMode,
    warnings: &mut Vec<ExportWarning>,
) {
    for inline in inlines {
        match inline {
            CompileInline::Text(text) => output.push_str(&html_escape(text)),
            CompileInline::Emphasis(children) => {
                output.push_str("<em>");
                html_inlines(output, children, ir, asset_mode, warnings);
                output.push_str("</em>");
            }
            CompileInline::Strong(children) => {
                output.push_str("<strong>");
                html_inlines(output, children, ir, asset_mode, warnings);
                output.push_str("</strong>");
            }
            CompileInline::Strikethrough(children) => {
                output.push_str("<s>");
                html_inlines(output, children, ir, asset_mode, warnings);
                output.push_str("</s>");
            }
            CompileInline::Code(text) => {
                let _ = write!(output, "<code>{}</code>", html_escape(text));
            }
            CompileInline::Link {
                label,
                destination,
                title,
            } => {
                if safe_html_href(destination) {
                    let _ = write!(output, "<a href=\"{}\"", html_escape(destination));
                    if let Some(title) = title {
                        let _ = write!(output, " title=\"{}\"", html_escape(title));
                    }
                    output.push('>');
                    html_inlines(output, label, ir, asset_mode, warnings);
                    output.push_str("</a>");
                } else {
                    html_inlines(output, label, ir, asset_mode, warnings);
                    warnings.push(ExportWarning {
                        code: "unsafe-link-suppressed",
                        message: format!("Unsafe link scheme was rendered as text: {destination}"),
                    });
                }
            }
            CompileInline::Image {
                alt,
                asset,
                destination,
                title,
            } => {
                let source = asset.and_then(|id| ir.assets.get(&id));
                let src = match (source, asset_mode) {
                    (Some(asset), HtmlAssetMode::SelfContained)
                        if asset.source_path.is_file()
                            && asset.media_type.starts_with("image/") =>
                    {
                        fs::read(&asset.source_path).ok().map(|bytes| {
                            format!("data:{};base64,{}", asset.media_type, base64(&bytes))
                        })
                    }
                    (Some(asset), HtmlAssetMode::Relative) => {
                        Some(format!("assets/{}", percent_encode_path(&asset.safe_name)))
                    }
                    _ => None,
                };
                if let Some(src) = src {
                    let _ = write!(
                        output,
                        "<img src=\"{}\" alt=\"{}\"",
                        html_escape(&src),
                        html_escape(alt)
                    );
                    if let Some(title) = title {
                        let _ = write!(output, " title=\"{}\"", html_escape(title));
                    }
                    output.push('>');
                    if asset_mode == HtmlAssetMode::Relative {
                        warnings.push(ExportWarning { code: "html-relative-asset", message: format!("Copy `{destination}` to assets/ beside the HTML file to resolve this image.") });
                    }
                } else {
                    let _ = write!(
                        output,
                        "<span class=\"missing-asset\">[{}]</span>",
                        html_escape(alt)
                    );
                    warnings.push(ExportWarning {
                        code: "html-missing-asset",
                        message: format!("Image could not be exported: {destination}"),
                    });
                }
            }
            CompileInline::Superscript(children) => {
                output.push_str("<sup>");
                html_inlines(output, children, ir, asset_mode, warnings);
                output.push_str("</sup>");
            }
            CompileInline::Subscript(children) => {
                output.push_str("<sub>");
                html_inlines(output, children, ir, asset_mode, warnings);
                output.push_str("</sub>");
            }
            CompileInline::Styled {
                children, style, ..
            } => {
                if let Some(style) = style {
                    let _ = write!(
                        output,
                        "<span class=\"{}\">",
                        html_escape(&sanitize_class(&style.class_name))
                    );
                    html_inlines(output, children, ir, asset_mode, warnings);
                    output.push_str("</span>");
                } else {
                    html_inlines(output, children, ir, asset_mode, warnings);
                }
            }
            CompileInline::SoftBreak => output.push('\n'),
            CompileInline::HardBreak => output.push_str("<br>\n"),
        }
    }
}

fn safe_html_href(value: &str) -> bool {
    value.starts_with("https://")
        || value.starts_with("http://")
        || value.starts_with("mailto:")
        || value.starts_with('#')
        || (!value.contains(':') && !value.starts_with('/'))
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn percent_encode_path(value: &str) -> String {
    value
        .bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_') {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

fn alignment_name(alignment: Alignment) -> &'static str {
    match alignment {
        Alignment::Left => "left",
        Alignment::Center => "center",
        Alignment::Right => "right",
        Alignment::Justify => "justify",
    }
}

fn spaced(value: &str) -> String {
    let mut output = value.trim_end().to_owned();
    output.push_str("\n\n");
    output
}

fn base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let first = u32::from(chunk[0]);
        let second = u32::from(*chunk.get(1).unwrap_or(&0));
        let third = u32::from(*chunk.get(2).unwrap_or(&0));
        let value = (first << 16) | (second << 8) | third;
        output.push(char::from(TABLE[((value >> 18) & 0x3f) as usize]));
        output.push(char::from(TABLE[((value >> 12) & 0x3f) as usize]));
        output.push(if chunk.len() > 1 {
            char::from(TABLE[((value >> 6) & 0x3f) as usize])
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            char::from(TABLE[(value & 0x3f) as usize])
        } else {
            '='
        });
    }
    output
}

/// A small, deterministic PDF fallback used by the Qt-independent exporter
/// tests. The Qt bridge may instead consume the same `CompileIr` through a
/// `QTextDocument`/`QPdfWriter` adapter. This fallback has deliberately
/// documented degradation for non-Latin glyph shaping rather than dropping it
/// silently.
fn render_pdf(ir: &CompileIr) -> (Vec<u8>, Vec<ExportWarning>) {
    let mut warnings = Vec::new();
    let mut warned_unicode = false;
    let plain = render_plain_text(ir, "\n\n");
    let width = micrometres_to_points(ir.page.width_micrometres);
    let height = micrometres_to_points(ir.page.height_micrometres);
    let margin_left = micrometres_to_points(ir.page.margin_left_micrometres);
    let margin_top = micrometres_to_points(ir.page.margin_top_micrometres);
    let mut pages = Vec::<Vec<String>>::new();
    let mut page = Vec::new();
    let max_lines =
        ((height - margin_top - micrometres_to_points(ir.page.margin_bottom_micrometres)) / 14.0)
            .max(1.0) as usize;
    for paragraph in plain.split('\u{c}') {
        for line in wrap_pdf_text(paragraph, 90) {
            if page.len() == max_lines {
                pages.push(std::mem::take(&mut page));
            }
            if !line.is_ascii() && !warned_unicode {
                warnings.push(ExportWarning { code: "pdf-unicode-fallback", message: "The Qt PDF renderer is required for shaped non-Latin text on this platform; the portable fallback substituted unsupported glyphs.".into() });
                warned_unicode = true;
            }
            page.push(pdf_latin1(&line));
        }
        if page.len() == max_lines {
            pages.push(std::mem::take(&mut page));
        }
        page.push(String::new());
    }
    if !page.is_empty() || pages.is_empty() {
        pages.push(page);
    }
    let font_id = 3usize;
    let first_page_id = 4usize;
    let mut objects = BTreeMap::<usize, Vec<u8>>::new();
    objects.insert(1, b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    let page_refs = (0..pages.len())
        .map(|index| format!("{} 0 R", first_page_id + index * 2))
        .collect::<Vec<_>>()
        .join(" ");
    objects.insert(
        2,
        format!(
            "<< /Type /Pages /Kids [ {page_refs} ] /Count {} >>",
            pages.len()
        )
        .into_bytes(),
    );
    objects.insert(
        font_id,
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
    );
    for (index, lines) in pages.iter().enumerate() {
        let page_id = first_page_id + index * 2;
        let content_id = page_id + 1;
        objects.insert(page_id, format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width:.2} {height:.2}] /Resources << /Font << /F1 {font_id} 0 R >> >> /Contents {content_id} 0 R >>").into_bytes());
        let mut commands = format!(
            "BT\n/F1 11 Tf\n{margin_left:.2} {:.2} Td\n",
            height - margin_top
        );
        for (line_number, line) in lines.iter().enumerate() {
            if line_number > 0 {
                commands.push_str("0 -14 Td\n");
            }
            let _ = writeln!(commands, "({}) Tj", pdf_escape(line));
        }
        commands.push_str("ET\n");
        let stream = format!(
            "<< /Length {} >>\nstream\n{}endstream",
            commands.len(),
            commands
        );
        objects.insert(content_id, stream.into_bytes());
    }
    (pdf_document(objects), dedupe_warnings(warnings))
}

fn micrometres_to_points(value: u32) -> f64 {
    f64::from(value) * 72.0 / 25_400.0
}

fn wrap_pdf_text(value: &str, width: usize) -> Vec<String> {
    let mut output = Vec::new();
    for source_line in value.lines() {
        let mut line = String::new();
        for word in source_line.split_whitespace() {
            if !line.is_empty() && line.chars().count() + word.chars().count() + 1 > width {
                output.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        output.push(line);
    }
    if output.is_empty() {
        output.push(String::new());
    }
    output
}

fn pdf_latin1(value: &str) -> String {
    value
        .chars()
        .map(|character| if character.is_ascii() { character } else { '?' })
        .collect()
}

fn pdf_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

fn pdf_document(objects: BTreeMap<usize, Vec<u8>>) -> Vec<u8> {
    let max_id = *objects.keys().max().unwrap_or(&0);
    let mut output = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = vec![0usize; max_id + 1];
    for (id, data) in objects {
        offsets[id] = output.len();
        output.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        output.extend_from_slice(&data);
        output.extend_from_slice(b"\nendobj\n");
    }
    let start_xref = output.len();
    output.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", max_id + 1).as_bytes());
    for offset in offsets.iter().skip(1) {
        output.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    output.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{start_xref}\n%%EOF\n",
            max_id + 1
        )
        .as_bytes(),
    );
    output
}

fn dedupe_warnings(mut warnings: Vec<ExportWarning>) -> Vec<ExportWarning> {
    let mut seen = BTreeSet::new();
    warnings.retain(|warning| seen.insert((warning.code, warning.message.clone())));
    warnings
}

fn render_epub(ir: &CompileIr) -> (Vec<u8>, Vec<ExportWarning>) {
    let mut warnings = Vec::new();
    let mut body = String::new();
    for block in &ir.blocks {
        html_block(&mut body, block, ir, HtmlAssetMode::Relative, &mut warnings);
    }
    body = body.replace("src=\"assets/", "src=\"../assets/");
    warnings.retain(|warning| warning.code != "html-relative-asset");
    let language = if ir.metadata.language.trim().is_empty() {
        "en"
    } else {
        &ir.metadata.language
    };
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xml:lang=\"{}\" lang=\"{}\"><head><title>{}</title><link rel=\"stylesheet\" type=\"text/css\" href=\"../styles.css\" /></head><body>{}</body></html>\n",
        xml_escape(language),
        xml_escape(language),
        xml_escape(&ir.title),
        xhtml_void_elements(&add_epub_section_ids(&body))
    );
    let nav = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xml:lang=\"{}\"><head><title>Contents</title></head><body><nav epub:type=\"toc\" id=\"toc\" xmlns:epub=\"http://www.idpf.org/2007/ops\"><h1>Contents</h1><ol>{}</ol></nav></body></html>\n",
        xml_escape(language),
        epub_nav(ir)
    );
    let mut manifest = String::from(
        "<item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/><item id=\"book\" href=\"text/book.xhtml\" media-type=\"application/xhtml+xml\"/><item id=\"css\" href=\"styles.css\" media-type=\"text/css\"/>",
    );
    let mut files = vec![
        ("mimetype".into(), b"application/epub+zip".to_vec()),
        ("META-INF/container.xml".into(), b"<?xml version=\"1.0\"?><container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\"><rootfiles><rootfile full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/></rootfiles></container>".to_vec()),
        ("OEBPS/nav.xhtml".into(), nav.into_bytes()),
        ("OEBPS/styles.css".into(), html_css(ir).into_bytes()),
        ("OEBPS/text/book.xhtml".into(), content.into_bytes()),
    ];
    for asset in ir.assets.values() {
        if !asset.media_type.starts_with("image/") || !asset.source_path.is_file() {
            continue;
        }
        match fs::read(&asset.source_path) {
            Ok(bytes) => {
                let id = format!("asset-{}", asset.id);
                let _ = write!(
                    manifest,
                    "<item id=\"{}\" href=\"assets/{}\" media-type=\"{}\"/>",
                    xml_escape(&id),
                    xml_escape(&asset.safe_name),
                    xml_escape(&asset.media_type)
                );
                files.push((format!("OEBPS/assets/{}", asset.safe_name), bytes));
            }
            Err(_) => warnings.push(ExportWarning {
                code: "epub-missing-asset",
                message: format!("Image asset could not be packaged: {}", asset.safe_name),
            }),
        }
    }
    let metadata_title = if ir.metadata.title.trim().is_empty() {
        &ir.title
    } else {
        &ir.metadata.title
    };
    let author = if ir.metadata.author.trim().is_empty() {
        String::new()
    } else {
        format!(
            "<dc:creator>{}</dc:creator>",
            xml_escape(&ir.metadata.author)
        )
    };
    let opf = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?><package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"book-id\" xml:lang=\"{}\"><metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:identifier id=\"book-id\">urn:uuid:{}</dc:identifier><dc:title>{}</dc:title><dc:language>{}</dc:language>{}</metadata><manifest>{}</manifest><spine><itemref idref=\"book\"/></spine></package>",
        xml_escape(language),
        ir.project_id,
        xml_escape(metadata_title),
        xml_escape(language),
        author,
        manifest
    );
    files.push(("OEBPS/content.opf".into(), opf.into_bytes()));
    (zip_store(files), dedupe_warnings(warnings))
}

fn epub_nav(ir: &CompileIr) -> String {
    let mut result = String::new();
    let mut heading_index = 0usize;
    append_epub_nav(&ir.blocks, &mut result, &mut heading_index);
    if result.is_empty() {
        result.push_str("<li><a href=\"text/book.xhtml\">Book</a></li>");
    }
    result
}

fn append_epub_nav(blocks: &[CompileBlock], result: &mut String, heading_index: &mut usize) {
    for block in blocks {
        let label = match &block.kind {
            CompileBlockKind::Title { text } => Some(text.clone()),
            CompileBlockKind::Heading { content, .. } => Some(plain_inlines(content)),
            _ => None,
        };
        if let Some(label) = label {
            *heading_index += 1;
            let _ = write!(
                result,
                "<li><a href=\"text/book.xhtml#section-{}\">{}</a></li>",
                *heading_index,
                xml_escape(&label)
            );
        }
        if let CompileBlockKind::Alignment { children, .. } = &block.kind {
            append_epub_nav(children, result, heading_index);
        }
    }
}

fn xhtml_void_elements(value: &str) -> String {
    let mut output = String::new();
    let mut rest = value;
    while let Some(start) = rest.find('<') {
        output.push_str(&rest[..start]);
        let tail = &rest[start..];
        let Some(end) = tail.find('>') else {
            output.push_str(tail);
            break;
        };
        let tag = &tail[..=end];
        let name = tag[1..]
            .trim_start()
            .split(|character: char| {
                character.is_whitespace() || character == '>' || character == '/'
            })
            .next()
            .unwrap_or("");
        if matches!(name, "br" | "hr" | "img" | "input" | "meta" | "link") && !tag.ends_with("/>") {
            output.push_str(&tag[..tag.len() - 1]);
            output.push_str(" />");
        } else {
            output.push_str(tag);
        }
        rest = &tail[end + 1..];
    }
    output
}

fn add_epub_section_ids(value: &str) -> String {
    let mut output = String::new();
    let mut rest = value;
    let mut section = 0usize;
    while let Some(start) = rest.find("<h") {
        output.push_str(&rest[..start]);
        let tail = &rest[start..];
        let Some(end) = tail.find('>') else {
            output.push_str(tail);
            break;
        };
        let tag = &tail[..=end];
        let heading = tag.as_bytes().get(2).is_some_and(u8::is_ascii_digit)
            && tag.as_bytes().get(3) != Some(&b'-');
        if heading {
            section += 1;
            if tag.contains(" id=") {
                output.push_str(tag);
            } else {
                output.push_str(&tag[..tag.len() - 1]);
                let _ = write!(output, " id=\"section-{section}\">");
            }
        } else {
            output.push_str(tag);
        }
        rest = &tail[end + 1..];
    }
    output
}

fn xml_escape(value: &str) -> String {
    html_escape(value)
}

/// Uncompressed deterministic ZIP writer. EPUB requires `mimetype` to be the
/// first entry and uncompressed; using store mode for every entry makes both
/// EPUB and golden DOCX output deterministic without a platform zlib binding.
fn zip_store(files: Vec<(String, Vec<u8>)>) -> Vec<u8> {
    let mut output = Vec::new();
    let mut entries = Vec::new();
    for (name, data) in files {
        let offset = u32::try_from(output.len()).unwrap_or(u32::MAX);
        let crc = crc32(&data);
        let name_bytes = name.as_bytes();
        push_u32(&mut output, 0x0403_4b50);
        push_u16(&mut output, 20);
        push_u16(&mut output, 0x0800);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 33);
        push_u32(&mut output, crc);
        push_u32(&mut output, u32::try_from(data.len()).unwrap_or(u32::MAX));
        push_u32(&mut output, u32::try_from(data.len()).unwrap_or(u32::MAX));
        push_u16(
            &mut output,
            u16::try_from(name_bytes.len()).unwrap_or(u16::MAX),
        );
        push_u16(&mut output, 0);
        output.extend_from_slice(name_bytes);
        output.extend_from_slice(&data);
        entries.push((name, crc, data.len(), offset));
    }
    let central_offset = u32::try_from(output.len()).unwrap_or(u32::MAX);
    for (name, crc, size, offset) in &entries {
        let name_bytes = name.as_bytes();
        push_u32(&mut output, 0x0201_4b50);
        push_u16(&mut output, 20);
        push_u16(&mut output, 20);
        push_u16(&mut output, 0x0800);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 33);
        push_u32(&mut output, *crc);
        push_u32(&mut output, u32::try_from(*size).unwrap_or(u32::MAX));
        push_u32(&mut output, u32::try_from(*size).unwrap_or(u32::MAX));
        push_u16(
            &mut output,
            u16::try_from(name_bytes.len()).unwrap_or(u16::MAX),
        );
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u16(&mut output, 0);
        push_u32(&mut output, 0);
        push_u32(&mut output, *offset);
        output.extend_from_slice(name_bytes);
    }
    let central_size = u32::try_from(output.len())
        .unwrap_or(u32::MAX)
        .saturating_sub(central_offset);
    push_u32(&mut output, 0x0605_4b50);
    push_u16(&mut output, 0);
    push_u16(&mut output, 0);
    push_u16(
        &mut output,
        u16::try_from(entries.len()).unwrap_or(u16::MAX),
    );
    push_u16(
        &mut output,
        u16::try_from(entries.len()).unwrap_or(u16::MAX),
    );
    push_u32(&mut output, central_size);
    push_u32(&mut output, central_offset);
    push_u16(&mut output, 0);
    output
}

fn push_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}
fn push_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn render_docx(ir: &CompileIr) -> (Vec<u8>, Vec<ExportWarning>) {
    let mut renderer = DocxRenderer::new(ir);
    for block in &ir.blocks {
        renderer.block(block);
    }
    let document = renderer.finish_document();
    let styles = docx_styles(ir);
    let content_types = docx_content_types(&renderer.images);
    let mut files = vec![
        ("[Content_Types].xml".into(), content_types.into_bytes()),
        ("_rels/.rels".into(), b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/><Relationship Id=\"rId3\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties\" Target=\"docProps/app.xml\"/></Relationships>".to_vec()),
        ("docProps/core.xml".into(), docx_core_properties(ir).into_bytes()),
        ("docProps/app.xml".into(), b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\"><Application>ParchMint</Application></Properties>".to_vec()),
        ("word/document.xml".into(), document.into_bytes()),
        ("word/styles.xml".into(), styles.into_bytes()),
        ("word/numbering.xml".into(), docx_numbering().into_bytes()),
        ("word/_rels/document.xml.rels".into(), renderer.relationships_xml().into_bytes()),
    ];
    for image in &renderer.images {
        files.push((format!("word/media/{}", image.name), image.bytes.clone()));
    }
    (zip_store(files), dedupe_warnings(renderer.warnings))
}

#[derive(Clone, Debug)]
struct DocxImage {
    name: String,
    bytes: Vec<u8>,
}

struct DocxRenderer<'a> {
    ir: &'a CompileIr,
    body: String,
    relationships: Vec<(String, String, String, bool)>,
    images: Vec<DocxImage>,
    image_relations: BTreeMap<parchmint_domain::AssetId, String>,
    warnings: Vec<ExportWarning>,
    next_relation: usize,
    next_drawing: usize,
}

impl<'a> DocxRenderer<'a> {
    fn new(ir: &'a CompileIr) -> Self {
        Self {
            ir,
            body: String::new(),
            relationships: vec![(
                "rId1".into(),
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering"
                    .into(),
                "numbering.xml".into(),
                false,
            )],
            images: Vec::new(),
            image_relations: BTreeMap::new(),
            warnings: Vec::new(),
            next_relation: 2,
            next_drawing: 1,
        }
    }

    fn block(&mut self, block: &CompileBlock) {
        match &block.kind {
            CompileBlockKind::Title { text } => self.paragraph(
                &[CompileInline::Text(text.clone())],
                Some("Title"),
                None,
                None,
            ),
            CompileBlockKind::Heading { level, content, .. } => self.paragraph(
                content,
                Some(&format!("Heading{}", (*level).clamp(1, 6))),
                block.style.as_ref(),
                None,
            ),
            CompileBlockKind::Paragraph { content, .. } => {
                self.paragraph(content, None, block.style.as_ref(), None);
            }
            CompileBlockKind::BlockQuote { source } => self.preformatted(source, Some("Quote")),
            CompileBlockKind::CodeBlock { text, .. } => self.preformatted(text, Some("Code")),
            CompileBlockKind::List { ordered, items, .. } => {
                for item in items {
                    let num = if *ordered { 1 } else { 2 };
                    self.paragraph(&item.content, None, None, Some(num));
                }
            }
            CompileBlockKind::Table { source } | CompileBlockKind::Footnote { source } => {
                self.preformatted(source, None);
                self.warnings.push(ExportWarning {
                    code: "docx-source-block",
                    message:
                        "A source-backed table or footnote was visibly retained as text in DOCX."
                            .into(),
                });
            }
            CompileBlockKind::ThematicBreak | CompileBlockKind::SceneBreak => {
                self.paragraph(&[CompileInline::Text("***".into())], None, None, None);
            }
            CompileBlockKind::PageBreak => self
                .body
                .push_str("<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>"),
            CompileBlockKind::Alignment {
                alignment,
                children,
                ..
            } => {
                for child in children {
                    self.aligned_block(child, *alignment);
                }
            }
            CompileBlockKind::Opaque { reason, source } => {
                self.preformatted(source, None);
                self.warnings.push(ExportWarning {
                    code: "docx-opaque-source",
                    message: format!("Unsupported source was retained as text in DOCX: {reason}"),
                });
            }
        }
    }

    fn aligned_block(&mut self, block: &CompileBlock, alignment: Alignment) {
        match &block.kind {
            CompileBlockKind::Paragraph { content, .. } => self.paragraph(
                content,
                None,
                block.style.as_ref(),
                none_with_alignment(alignment),
            ),
            CompileBlockKind::Heading { level, content, .. } => self.paragraph(
                content,
                Some(&format!("Heading{}", (*level).clamp(1, 6))),
                block.style.as_ref(),
                none_with_alignment(alignment),
            ),
            _ => self.block(block),
        }
    }

    fn preformatted(&mut self, value: &str, style: Option<&str>) {
        for line in value.lines() {
            self.paragraph(&[CompileInline::Text(line.into())], style, None, None);
        }
    }

    /// `number_or_alignment` reserves negative values for a paragraph
    /// justification request, keeping this renderer deliberately small.
    fn paragraph(
        &mut self,
        content: &[CompileInline],
        builtin_style: Option<&str>,
        style: Option<&ResolvedStyle>,
        number_or_alignment: Option<i32>,
    ) {
        self.body.push_str("<w:p><w:pPr>");
        if let Some(name) = builtin_style {
            let _ = write!(self.body, "<w:pStyle w:val=\"{}\"/>", xml_escape(name));
        }
        if let Some(style) = style.filter(|style| style.kind == StyleKind::Paragraph) {
            let _ = write!(self.body, "<w:pStyle w:val=\"{}\"/>", docx_style_id(style));
        }
        match number_or_alignment {
            Some(number) if number > 0 => {
                let _ = write!(
                    self.body,
                    "<w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"{number}\"/></w:numPr>"
                );
            }
            Some(number) if number < 0 => {
                let _ = write!(
                    self.body,
                    "<w:jc w:val=\"{}\"/>",
                    match -number {
                        1 => "center",
                        2 => "right",
                        3 => "both",
                        _ => "left",
                    }
                );
            }
            _ => {}
        }
        self.body.push_str("</w:pPr>");
        self.inlines(content, None);
        self.body.push_str("</w:p>");
    }

    fn inlines(&mut self, inlines: &[CompileInline], inherited_style: Option<&ResolvedStyle>) {
        for inline in inlines {
            match inline {
                CompileInline::Text(text) | CompileInline::Code(text) => {
                    self.run(text, inherited_style, false, false, false, false, false);
                }
                CompileInline::Emphasis(children) => self.inlines_with_format(
                    children,
                    inherited_style,
                    false,
                    true,
                    false,
                    false,
                    false,
                ),
                CompileInline::Strong(children) => self.inlines_with_format(
                    children,
                    inherited_style,
                    true,
                    false,
                    false,
                    false,
                    false,
                ),
                CompileInline::Strikethrough(children) => self.inlines_with_format(
                    children,
                    inherited_style,
                    false,
                    false,
                    true,
                    false,
                    false,
                ),
                CompileInline::Link {
                    label, destination, ..
                } => {
                    if safe_html_href(destination) {
                        let relation = self.external_relation(destination);
                        let _ = write!(self.body, "<w:hyperlink r:id=\"{relation}\">");
                        self.inlines(label, inherited_style);
                        self.body.push_str("</w:hyperlink>");
                    } else {
                        self.inlines(label, inherited_style);
                        self.warnings.push(ExportWarning {
                            code: "docx-unsafe-link-suppressed",
                            message: format!("Unsafe link was rendered as text: {destination}"),
                        });
                    }
                }
                CompileInline::Image {
                    alt,
                    asset,
                    destination,
                    ..
                } => self.image(*asset, alt, destination),
                CompileInline::Superscript(children) => self.inlines_with_format(
                    children,
                    inherited_style,
                    false,
                    false,
                    false,
                    true,
                    false,
                ),
                CompileInline::Subscript(children) => self.inlines_with_format(
                    children,
                    inherited_style,
                    false,
                    false,
                    false,
                    false,
                    true,
                ),
                CompileInline::Styled {
                    children, style, ..
                } => self.inlines(children, style.as_ref().or(inherited_style)),
                CompileInline::SoftBreak | CompileInline::HardBreak => {
                    self.body.push_str("<w:r><w:br/></w:r>");
                }
            }
        }
    }

    fn inlines_with_format(
        &mut self,
        children: &[CompileInline],
        style: Option<&ResolvedStyle>,
        bold: bool,
        italic: bool,
        strike: bool,
        super_script: bool,
        sub_script: bool,
    ) {
        for child in children {
            match child {
                CompileInline::Text(text) | CompileInline::Code(text) => {
                    self.run(text, style, bold, italic, strike, super_script, sub_script);
                }
                _ => self.inlines(std::slice::from_ref(child), style),
            }
        }
    }

    fn run(
        &mut self,
        text: &str,
        style: Option<&ResolvedStyle>,
        bold: bool,
        italic: bool,
        strike: bool,
        super_script: bool,
        sub_script: bool,
    ) {
        self.body.push_str("<w:r><w:rPr>");
        if let Some(style) = style.filter(|style| style.kind == StyleKind::Character) {
            let _ = write!(self.body, "<w:rStyle w:val=\"{}\"/>", docx_style_id(style));
        }
        if bold {
            self.body.push_str("<w:b/>");
        }
        if italic {
            self.body.push_str("<w:i/>");
        }
        if strike {
            self.body.push_str("<w:strike/>");
        }
        if super_script {
            self.body.push_str("<w:vertAlign w:val=\"superscript\"/>");
        }
        if sub_script {
            self.body.push_str("<w:vertAlign w:val=\"subscript\"/>");
        }
        self.body.push_str("</w:rPr>");
        let _ = write!(
            self.body,
            "<w:t xml:space=\"preserve\">{}</w:t></w:r>",
            xml_escape(text)
        );
    }

    fn image(&mut self, asset_id: Option<parchmint_domain::AssetId>, alt: &str, destination: &str) {
        let Some(asset_id) = asset_id else {
            self.run(alt, None, false, false, false, false, false);
            self.warnings.push(ExportWarning {
                code: "docx-image-not-asset",
                message: format!(
                    "Only project image assets can be embedded in DOCX: {destination}"
                ),
            });
            return;
        };
        let relation = if let Some(relation) = self.image_relations.get(&asset_id) {
            relation.clone()
        } else {
            let Some(asset) = self.ir.assets.get(&asset_id) else {
                self.run(alt, None, false, false, false, false, false);
                self.warnings.push(ExportWarning {
                    code: "docx-missing-asset",
                    message: format!("Image asset is missing: {destination}"),
                });
                return;
            };
            if !asset.media_type.starts_with("image/") {
                self.run(alt, None, false, false, false, false, false);
                self.warnings.push(ExportWarning {
                    code: "docx-unsupported-image",
                    message: format!("Asset is not an embeddable image: {}", asset.safe_name),
                });
                return;
            }
            let Ok(bytes) = fs::read(&asset.source_path) else {
                self.run(alt, None, false, false, false, false, false);
                self.warnings.push(ExportWarning {
                    code: "docx-missing-asset",
                    message: format!("Image asset is missing: {}", asset.safe_name),
                });
                return;
            };
            let relation = self.add_relation(
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image",
                format!("media/{}", asset.safe_name),
                false,
            );
            self.images.push(DocxImage {
                name: asset.safe_name.clone(),
                bytes,
            });
            self.image_relations.insert(asset_id, relation.clone());
            relation
        };
        let drawing_id = self.next_drawing;
        self.next_drawing += 1;
        let _ = write!(
            self.body,
            "<w:r><w:drawing><wp:inline xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\"><wp:extent cx=\"3657600\" cy=\"2743200\"/><wp:docPr id=\"{drawing_id}\" name=\"{}\" descr=\"{}\"/><a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:blipFill><a:blip r:embed=\"{relation}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"3657600\" cy=\"2743200\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>",
            xml_escape(alt),
            xml_escape(alt)
        );
    }

    fn external_relation(&mut self, target: &str) -> String {
        self.add_relation(
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink",
            target.into(),
            true,
        )
    }

    fn add_relation(&mut self, relationship_type: &str, target: String, external: bool) -> String {
        let id = format!("rId{}", self.next_relation);
        self.next_relation += 1;
        self.relationships
            .push((id.clone(), relationship_type.into(), target, external));
        id
    }

    fn relationships_xml(&self) -> String {
        let mut output = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
        );
        for (id, kind, target, external) in &self.relationships {
            let _ = write!(
                output,
                "<Relationship Id=\"{}\" Type=\"{}\" Target=\"{}\"{} />",
                xml_escape(id),
                xml_escape(kind),
                xml_escape(target),
                if *external {
                    " TargetMode=\"External\""
                } else {
                    ""
                }
            );
        }
        output.push_str("</Relationships>");
        output
    }

    fn finish_document(&self) -> String {
        let page = &self.ir.page;
        let width = micrometres_to_twips(page.width_micrometres);
        let height = micrometres_to_twips(page.height_micrometres);
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><w:body>{}<w:sectPr><w:pgSz w:w=\"{width}\" w:h=\"{height}\"/><w:pgMar w:top=\"{}\" w:right=\"{}\" w:bottom=\"{}\" w:left=\"{}\" w:header=\"720\" w:footer=\"720\" w:gutter=\"0\"/></w:sectPr></w:body></w:document>",
            self.body,
            micrometres_to_twips(page.margin_top_micrometres),
            micrometres_to_twips(page.margin_right_micrometres),
            micrometres_to_twips(page.margin_bottom_micrometres),
            micrometres_to_twips(page.margin_left_micrometres)
        )
    }
}

fn none_with_alignment(alignment: Alignment) -> Option<i32> {
    match alignment {
        Alignment::Left => None,
        Alignment::Center => Some(-1),
        Alignment::Right => Some(-2),
        Alignment::Justify => Some(-3),
    }
}

fn micrometres_to_twips(value: u32) -> u32 {
    (f64::from(value) / 17.638_888_889).round() as u32
}

fn docx_style_id(style: &ResolvedStyle) -> String {
    format!("PM{}", style.id.to_string().replace('-', ""))
}

fn docx_styles(ir: &CompileIr) -> String {
    let mut output = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:docDefaults/><w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/></w:style><w:style w:type=\"paragraph\" w:styleId=\"Title\"><w:name w:val=\"Title\"/><w:rPr><w:b/><w:sz w:val=\"36\"/></w:rPr></w:style><w:style w:type=\"paragraph\" w:styleId=\"Quote\"><w:name w:val=\"Quote\"/></w:style><w:style w:type=\"paragraph\" w:styleId=\"Code\"><w:name w:val=\"Code\"/><w:rPr><w:rFonts w:ascii=\"Courier New\" w:hAnsi=\"Courier New\"/></w:rPr></w:style>",
    );
    for level in 1..=6 {
        let _ = write!(
            output,
            "<w:style w:type=\"paragraph\" w:styleId=\"Heading{level}\"><w:name w:val=\"heading {level}\"/><w:basedOn w:val=\"Normal\"/><w:rPr><w:b/><w:sz w:val=\"{}\"/></w:rPr></w:style>",
            36u32.saturating_sub(level * 3)
        );
    }
    for style in ir.styles.values() {
        let kind = if style.kind == StyleKind::Paragraph {
            "paragraph"
        } else {
            "character"
        };
        let _ = write!(
            output,
            "<w:style w:type=\"{kind}\" w:customStyle=\"1\" w:styleId=\"{}\"><w:name w:val=\"{}\"/><w:rPr>",
            docx_style_id(style),
            xml_escape(&style.class_name)
        );
        if style
            .properties
            .get("font-weight")
            .is_some_and(|value| value == "bold" || value == "700")
        {
            output.push_str("<w:b/>");
        }
        if style
            .properties
            .get("font-style")
            .is_some_and(|value| value == "italic")
        {
            output.push_str("<w:i/>");
        }
        if let Some(size) = style
            .properties
            .get("font-size")
            .and_then(|value| value.trim_end_matches("pt").parse::<u32>().ok())
        {
            let _ = write!(output, "<w:sz w:val=\"{}\"/>", size.saturating_mul(2));
        }
        output.push_str("</w:rPr></w:style>");
    }
    output.push_str("</w:styles>");
    output
}

fn docx_numbering() -> String {
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:numbering xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:abstractNum w:abstractNumId=\"0\"><w:lvl w:ilvl=\"0\"><w:start w:val=\"1\"/><w:numFmt w:val=\"decimal\"/><w:lvlText w:val=\"%1.\"/></w:lvl></w:abstractNum><w:abstractNum w:abstractNumId=\"1\"><w:lvl w:ilvl=\"0\"><w:start w:val=\"1\"/><w:numFmt w:val=\"bullet\"/><w:lvlText w:val=\"•\"/></w:lvl></w:abstractNum><w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num><w:num w:numId=\"2\"><w:abstractNumId w:val=\"1\"/></w:num></w:numbering>".into()
}

fn docx_content_types(images: &[DocxImage]) -> String {
    let mut output = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"png\" ContentType=\"image/png\"/><Default Extension=\"jpg\" ContentType=\"image/jpeg\"/><Default Extension=\"jpeg\" ContentType=\"image/jpeg\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/><Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/><Override PartName=\"/word/numbering.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml\"/><Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/><Override PartName=\"/docProps/app.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.extended-properties+xml\"/>",
    );
    for image in images {
        if Path::new(&image.name)
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|extension| {
                !matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "png" | "jpg" | "jpeg"
                )
            })
        {
            let _ = write!(
                output,
                "<Override PartName=\"/word/media/{}\" ContentType=\"application/octet-stream\"/>",
                xml_escape(&image.name)
            );
        }
    }
    output.push_str("</Types>");
    output
}

fn docx_core_properties(ir: &CompileIr) -> String {
    let author = if ir.metadata.author.trim().is_empty() {
        "ParchMint"
    } else {
        &ir.metadata.author
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"><dc:title>{}</dc:title><dc:creator>{}</dc:creator><dc:subject>{}</dc:subject><dcterms:created xsi:type=\"dcterms:W3CDTF\">1980-01-01T00:00:00Z</dcterms:created><dcterms:modified xsi:type=\"dcterms:W3CDTF\">1980-01-01T00:00:00Z</dcterms:modified></cp:coreProperties>",
        xml_escape(&ir.title),
        xml_escape(author),
        xml_escape(&ir.metadata.subject)
    )
}

fn validate_export(format: ExportFormat, bytes: &[u8]) -> Result<(), ExportError> {
    match format {
        ExportFormat::Markdown | ExportFormat::PlainText => Ok(()),
        ExportFormat::Html => validate_html(bytes).map_err(ExportError::Validation),
        ExportFormat::Pdf => validate_pdf(bytes).map_err(ExportError::Validation),
        ExportFormat::Epub => validate_epub(bytes).map_err(ExportError::Validation),
        ExportFormat::Docx => validate_docx(bytes).map_err(ExportError::Validation),
    }
}

/// A lightweight structural validator suitable for deterministic tests. It is
/// intentionally conservative and accepts only the generated HTML5 shape.
pub fn validate_html(bytes: &[u8]) -> Result<(), String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "HTML is not UTF-8".to_owned())?;
    if !text.starts_with("<!doctype html>")
        || !text.contains("<html ")
        || !text.ends_with("</html>\n")
    {
        return Err("HTML lacks a complete HTML5 document shell".into());
    }
    if !text.contains("<meta charset=\"utf-8\">") {
        return Err("HTML lacks UTF-8 metadata".into());
    }
    if text.contains("<script") {
        return Err("generated HTML unexpectedly contains active script".into());
    }
    Ok(())
}

pub fn validate_pdf(bytes: &[u8]) -> Result<(), String> {
    if !bytes.starts_with(b"%PDF-1.") || !bytes.ends_with(b"%%EOF\n") {
        return Err("PDF header or EOF marker is missing".into());
    }
    if !bytes.windows(5).any(|window| window == b"xref\n")
        || !bytes.windows(7).any(|window| window == b"trailer")
    {
        return Err("PDF cross-reference structure is missing".into());
    }
    Ok(())
}

/// Validates EPUB package/container/spine structure without relying on a host
/// ZIP command. The writer uses store mode, so it also detects accidental
/// compression changes that would violate the EPUB `mimetype` requirement.
pub fn validate_epub(bytes: &[u8]) -> Result<(), String> {
    let archive = parse_store_zip(bytes)?;
    let mimetype = archive.get("mimetype").ok_or("EPUB lacks mimetype entry")?;
    if *mimetype != b"application/epub+zip" {
        return Err("EPUB mimetype entry is invalid".into());
    }
    let container = archive
        .get("META-INF/container.xml")
        .ok_or("EPUB lacks META-INF/container.xml")?;
    let container =
        std::str::from_utf8(container).map_err(|_| "EPUB container is not UTF-8".to_owned())?;
    if !container.contains("OEBPS/content.opf") {
        return Err("EPUB container does not point to package".into());
    }
    let opf = archive
        .get("OEBPS/content.opf")
        .ok_or("EPUB lacks package document")?;
    let opf = std::str::from_utf8(opf).map_err(|_| "EPUB package is not UTF-8".to_owned())?;
    if !opf.contains("version=\"3.0\"")
        || !opf.contains("<spine>")
        || !opf.contains("idref=\"book\"")
    {
        return Err("EPUB package has no valid deterministic spine".into());
    }
    let book = archive
        .get("OEBPS/text/book.xhtml")
        .ok_or("EPUB lacks book XHTML")?;
    let book = std::str::from_utf8(book).map_err(|_| "EPUB XHTML is not UTF-8".to_owned())?;
    if !book.contains("xmlns=\"http://www.w3.org/1999/xhtml\"") || !book.contains("</html>") {
        return Err("EPUB content is not XHTML-shaped".into());
    }
    if !archive.contains_key("OEBPS/nav.xhtml") {
        return Err("EPUB lacks navigation document".into());
    }
    Ok(())
}

/// Checks the OOXML package parts required by Word/LibreOffice before the
/// temporary export replaces a destination. Full consumer compatibility is
/// covered by the platform matrix in Stage 08/09.
pub fn validate_docx(bytes: &[u8]) -> Result<(), String> {
    let archive = parse_store_zip(bytes)?;
    for name in [
        "[Content_Types].xml",
        "_rels/.rels",
        "word/document.xml",
        "word/styles.xml",
        "word/numbering.xml",
        "word/_rels/document.xml.rels",
    ] {
        if !archive.contains_key(name) {
            return Err(format!("DOCX lacks required package part `{name}`"));
        }
    }
    let document = std::str::from_utf8(archive["word/document.xml"])
        .map_err(|_| "DOCX document XML is not UTF-8".to_owned())?;
    if !document.contains("<w:document ")
        || !document.contains("<w:body>")
        || !document.contains("</w:document>")
    {
        return Err("DOCX document part is malformed".into());
    }
    let types = std::str::from_utf8(archive["[Content_Types].xml"])
        .map_err(|_| "DOCX content types are not UTF-8".to_owned())?;
    if !types.contains("wordprocessingml.document.main+xml") {
        return Err("DOCX main document content type is missing".into());
    }
    Ok(())
}

fn parse_store_zip(bytes: &[u8]) -> Result<BTreeMap<String, &[u8]>, String> {
    let end = bytes
        .windows(4)
        .rposition(|window| window == b"PK\x05\x06")
        .ok_or("ZIP end record is missing")?;
    if end + 22 > bytes.len() {
        return Err("ZIP end record is truncated".into());
    }
    let count = usize::from(read_u16(bytes, end + 10)?);
    let central_size =
        usize::try_from(read_u32(bytes, end + 12)?).map_err(|_| "ZIP central size overflows")?;
    let central_offset =
        usize::try_from(read_u32(bytes, end + 16)?).map_err(|_| "ZIP central offset overflows")?;
    if central_offset
        .checked_add(central_size)
        .is_none_or(|value| value > end)
    {
        return Err("ZIP central directory is outside archive".into());
    }
    let mut cursor = central_offset;
    let mut output = BTreeMap::new();
    for _ in 0..count {
        if read_u32(bytes, cursor)? != 0x0201_4b50 {
            return Err("ZIP central directory record is invalid".into());
        }
        let method = read_u16(bytes, cursor + 10)?;
        if method != 0 {
            return Err("validator only accepts deterministic store-mode ZIP entries".into());
        }
        let size = usize::try_from(read_u32(bytes, cursor + 24)?)
            .map_err(|_| "ZIP entry size overflows")?;
        let name_length = usize::from(read_u16(bytes, cursor + 28)?);
        let extra_length = usize::from(read_u16(bytes, cursor + 30)?);
        let comment_length = usize::from(read_u16(bytes, cursor + 32)?);
        let local_offset = usize::try_from(read_u32(bytes, cursor + 42)?)
            .map_err(|_| "ZIP local offset overflows")?;
        let name_end = cursor
            .checked_add(46 + name_length)
            .ok_or("ZIP entry name overflows")?;
        let name = std::str::from_utf8(
            bytes
                .get(cursor + 46..name_end)
                .ok_or("ZIP entry name is truncated")?,
        )
        .map_err(|_| "ZIP entry name is not UTF-8")?
        .to_owned();
        if read_u32(bytes, local_offset)? != 0x0403_4b50 {
            return Err("ZIP local header is invalid".into());
        }
        let local_name = usize::from(read_u16(bytes, local_offset + 26)?);
        let local_extra = usize::from(read_u16(bytes, local_offset + 28)?);
        let data_start = local_offset
            .checked_add(30 + local_name + local_extra)
            .ok_or("ZIP data start overflows")?;
        let data_end = data_start
            .checked_add(size)
            .ok_or("ZIP data end overflows")?;
        let data = bytes
            .get(data_start..data_end)
            .ok_or("ZIP data is truncated")?;
        if crc32(data) != read_u32(bytes, cursor + 16)? {
            return Err("ZIP entry CRC mismatch".into());
        }
        if output.insert(name, data).is_some() {
            return Err("ZIP contains duplicate entry name".into());
        }
        cursor = name_end
            .checked_add(extra_length + comment_length)
            .ok_or("ZIP central cursor overflows")?;
    }
    Ok(output)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or("ZIP record is truncated")?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or("ZIP record is truncated")?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use parchmint_domain::{
        CompilePreset, DocumentMetadata, DocumentRecord, Node, NodeKind, ProjectCommand,
        ProjectGeneration, RelativeProjectPath,
    };
    use parchmint_storage::ProjectStorage;
    use tempfile::TempDir;

    fn stamp() -> WorkStamp {
        WorkStamp {
            generation: ProjectGeneration::new(1).unwrap(),
            revision: parchmint_domain::Revision::new(7),
        }
    }

    fn add_document(
        opened: &mut OpenProject,
        parent: NodeId,
        title: &str,
        body: &str,
        group: bool,
        research: bool,
    ) -> NodeId {
        let node_id = NodeId::new();
        let document_id = DocumentId::new();
        let node = Node {
            id: node_id,
            kind: if group {
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
            path: RelativeProjectPath::new(format!(
                "{}/{node_id}.md",
                if research { "research" } else { "manuscript" }
            ))
            .unwrap(),
            metadata: DocumentMetadata {
                title: title.into(),
                ..DocumentMetadata::default()
            },
        };
        let index = opened.project.nodes[&parent].children.len();
        opened
            .execute(ProjectCommand::Create {
                parent,
                node,
                document,
                index,
            })
            .unwrap();
        opened.set_body(document_id, body.into()).unwrap();
        node_id
    }

    fn fixture() -> (TempDir, CompileInput, NodeId, NodeId) {
        let directory = tempfile::tempdir().unwrap();
        let mut opened = ProjectStorage::create(directory.path(), "Harbor Lights").unwrap();
        let manuscript = opened.project.manuscript_root();
        let part = add_document(
            &mut opened,
            manuscript,
            "Part One",
            "Opening group text.",
            true,
            false,
        );
        let scene = add_document(
            &mut opened,
            part,
            "Arrival",
            "Mara *arrives* at the harbor.\n\n<!-- parchmint:page-break -->\n",
            false,
            false,
        );
        let research_root = opened.project.research_root();
        let research = add_document(
            &mut opened,
            research_root,
            "Map note",
            "Research only.",
            false,
            true,
        );
        let input = CompileInput::from_open_project(&opened, stamp()).unwrap();
        (directory, input, scene, research)
    }

    #[test]
    fn binder_preorder_is_stable_and_research_requires_explicit_selection() {
        let (_directory, input, scene, research) = fixture();
        let preset = CompilePreset::manuscript("Manuscript");
        let (ir, _) = compile(&input, &preset, &CancellationToken::default()).unwrap();
        let document_titles = ir
            .blocks
            .iter()
            .filter_map(|block| match &block.kind {
                CompileBlockKind::Heading { content, .. } => Some(plain_inlines(content)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(document_titles, ["Part One", "Arrival"]);
        assert!(!document_titles.iter().any(|title| title == "Map note"));
        let preview = preview(&input, &preset, &CancellationToken::default()).unwrap();
        assert!(
            preview
                .nodes
                .iter()
                .any(|node| node.title == "Map note" && !node.included && node.reason.is_some())
        );

        let mut selected = preset.clone();
        selected.selected_roots = vec![research, scene]; // selection order must not reorder binder output
        let (ir, _) = compile(&input, &selected, &CancellationToken::default()).unwrap();
        let titles = ir
            .blocks
            .iter()
            .filter_map(|block| match &block.kind {
                CompileBlockKind::Heading { content, .. } => Some(plain_inlines(content)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(titles, ["Arrival", "Map note"]);
    }

    #[test]
    fn cancellation_and_opaque_input_are_actionable() {
        let (_directory, mut input, scene, _) = fixture();
        let document_id = input.project.nodes[&scene].kind.document_id().unwrap();
        input.bodies.insert(
            document_id,
            "```{=parchmint-opaque source-format=\"future\"}\nretain\n```\n".into(),
        );
        let (_, warnings) = compile(
            &input,
            &CompilePreset::manuscript("x"),
            &CancellationToken::default(),
        )
        .unwrap();
        assert!(
            warnings
                .iter()
                .any(|warning| warning.code == "opaque-markdown")
        );
        let cancelled = CancellationToken::default();
        cancelled.cancel();
        assert!(matches!(
            compile(&input, &CompilePreset::manuscript("x"), &cancelled),
            Err(CompileError::Cancelled)
        ));
    }

    #[test]
    fn every_format_is_deterministic_and_structurally_valid() {
        let (_directory, input, _, _) = fixture();
        let (ir, _) = compile(
            &input,
            &CompilePreset::manuscript("Manuscript"),
            &CancellationToken::default(),
        )
        .unwrap();
        let markdown = render_markdown(&ir);
        assert!(markdown.contains("<!-- parchmint:page-break -->"));
        let (html, _) = render_html(&ir, HtmlAssetMode::SelfContained);
        validate_html(html.as_bytes()).unwrap();
        let (pdf, _) = render_pdf(&ir);
        validate_pdf(&pdf).unwrap();
        let (epub, _) = render_epub(&ir);
        validate_epub(&epub).unwrap();
        let epub_parts = parse_store_zip(&epub).unwrap();
        assert!(
            std::str::from_utf8(epub_parts["OEBPS/text/book.xhtml"])
                .unwrap()
                .contains("id=\"section-1\"")
        );
        assert_eq!(epub, render_epub(&ir).0);
        let (docx, _) = render_docx(&ir);
        validate_docx(&docx).unwrap();
        assert_eq!(docx, render_docx(&ir).0);
    }

    #[test]
    fn markdown_export_emits_reparseable_fences_attributes_and_titles() {
        let (_directory, mut input, scene, _) = fixture();
        let document_id = input.project.nodes[&scene].kind.document_id().unwrap();
        input.bodies.insert(
            document_id,
            "```rust\nfn main() {}\n```\n\n::: {.parchmint-align align=\"center\"}\nCentered.\n:::\n"
                .into(),
        );
        let (mut ir, _) = compile(
            &input,
            &CompilePreset::manuscript("Manuscript"),
            &CancellationToken::default(),
        )
        .unwrap();
        ir.blocks.push(CompileBlock {
            kind: CompileBlockKind::Paragraph {
                content: vec![CompileInline::Link {
                    label: vec![CompileInline::Text("closing]".into())],
                    destination: "notes.md".into(),
                    title: Some("A \"quote\"".into()),
                }],
                attributes: CompileAttributes::default(),
            },
            style: None,
            provenance: SourceProvenance::Generated {
                node_id: None,
                document_id: None,
                role: "test-link",
            },
        });
        let markdown = render_markdown(&ir);
        assert!(markdown.contains("fn main() {}\n```\n\n"));
        assert!(markdown.contains("::: {.parchmint-align align=\"center\"}"));
        assert!(markdown.contains("[closing\\]](notes.md"), "{markdown}");
        assert!(markdown.contains(" \"A \\\"quote\\\"\""), "{markdown}");
        parchmint_markdown::Document::parse_body(
            &markdown,
            &parchmint_markdown::ParseOptions::default(),
        )
        .unwrap();
    }

    #[test]
    fn epub_links_packaged_assets_and_nested_headings() {
        let (directory, input, _, _) = fixture();
        let (mut ir, _) = compile(
            &input,
            &CompilePreset::manuscript("Manuscript"),
            &CancellationToken::default(),
        )
        .unwrap();
        let asset_id = parchmint_domain::AssetId::new();
        let safe_name = "cover #snow.png";
        let source_path = directory.path().join(safe_name);
        fs::write(&source_path, b"not-a-real-png").unwrap();
        ir.assets.insert(
            asset_id,
            CompileAsset {
                id: asset_id,
                display_name: safe_name.into(),
                safe_name: safe_name.into(),
                media_type: "image/png".into(),
                bytes: 14,
                source_path,
            },
        );
        ir.blocks.push(CompileBlock {
            kind: CompileBlockKind::Paragraph {
                content: vec![CompileInline::Image {
                    alt: "Cover".into(),
                    asset: Some(asset_id),
                    destination: "asset:cover".into(),
                    title: None,
                }],
                attributes: CompileAttributes::default(),
            },
            style: None,
            provenance: SourceProvenance::Generated {
                node_id: None,
                document_id: None,
                role: "test-image",
            },
        });
        ir.blocks.push(CompileBlock {
            kind: CompileBlockKind::Alignment {
                alignment: Alignment::Center,
                attributes: CompileAttributes::default(),
                children: vec![CompileBlock {
                    kind: CompileBlockKind::Heading {
                        level: 2,
                        content: vec![CompileInline::Text("Nested heading".into())],
                        attributes: CompileAttributes::default(),
                    },
                    style: None,
                    provenance: SourceProvenance::Generated {
                        node_id: None,
                        document_id: None,
                        role: "test-heading",
                    },
                }],
            },
            style: None,
            provenance: SourceProvenance::Generated {
                node_id: None,
                document_id: None,
                role: "test-alignment",
            },
        });

        let (epub, warnings) = render_epub(&ir);
        assert!(
            !warnings
                .iter()
                .any(|warning| warning.code == "html-relative-asset")
        );
        let parts = parse_store_zip(&epub).unwrap();
        assert!(parts.contains_key("OEBPS/assets/cover #snow.png"));
        let book = std::str::from_utf8(parts["OEBPS/text/book.xhtml"]).unwrap();
        assert!(book.contains("src=\"../assets/cover%20%23snow.png\""));
        let nav = std::str::from_utf8(parts["OEBPS/nav.xhtml"]).unwrap();
        assert!(nav.contains(">Nested heading</a>"));
        for fragment in nav.split("#section-").skip(1) {
            let number = fragment.split('"').next().unwrap();
            assert!(book.contains(&format!("id=\"section-{number}\"")));
        }
    }

    #[test]
    fn failed_collision_keeps_existing_destination_intact() {
        let (directory, input, _, _) = fixture();
        let (ir, _) = compile(
            &input,
            &CompilePreset::manuscript("Manuscript"),
            &CancellationToken::default(),
        )
        .unwrap();
        let destination = directory.path().join("existing.md");
        fs::write(&destination, "original").unwrap();
        let options = ExportOptions::file(ExportFormat::Markdown, &destination);
        assert!(matches!(
            export(&ir, &options),
            Err(ExportError::DestinationExists(_))
        ));
        assert_eq!(fs::read_to_string(&destination).unwrap(), "original");
        let mut replacement = options;
        replacement.collision = CollisionPolicy::ReplaceFile;
        export(&ir, &replacement).unwrap();
        assert_ne!(fs::read_to_string(&destination).unwrap(), "original");
    }
}
