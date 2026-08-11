//! Immutable, whole-manuscript export contracts.
//!
//! This crate deliberately describes export work without selecting an output
//! format or exposing operating-system paths.  Format implementations receive
//! a fixed [`ExportPlan`] and write it through [`ExportSink`].

use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
    sync::{Arc, Mutex},
};

pub use parchmint_domain::DocumentId;
pub use parchmint_project_format::CanonicalRelativePath;

/// The only export scope supported in v1.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OrderedExportScope {
    #[default]
    EntireManuscript,
}

/// A setting at a group or document level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum InheritedSetting {
    #[default]
    Inherit,
    Enabled,
    Disabled,
}

/// Project-level defaults for exportable nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportDefaults {
    pub emit_titles: bool,
    pub start_new_page: bool,
}

impl Default for ExportDefaults {
    fn default() -> Self {
        Self {
            emit_titles: true,
            start_new_page: false,
        }
    }
}

/// Group or document overrides, resolved from the Manuscript root downward.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExportSettings {
    pub emit_titles: InheritedSetting,
    pub start_new_page: InheritedSetting,
}

/// Fully resolved settings carried by every planned semantic item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectiveExportSettings {
    pub emit_titles: bool,
    pub start_new_page: bool,
}

impl EffectiveExportSettings {
    fn from_defaults(defaults: ExportDefaults) -> Self {
        Self {
            emit_titles: defaults.emit_titles,
            start_new_page: defaults.start_new_page,
        }
    }

    fn apply(self, overrides: ExportSettings) -> Self {
        Self {
            emit_titles: resolve(self.emit_titles, overrides.emit_titles),
            start_new_page: resolve(self.start_new_page, overrides.start_new_page),
        }
    }
}

fn resolve(inherited: bool, setting: InheritedSetting) -> bool {
    match setting {
        InheritedSetting::Inherit => inherited,
        InheritedSetting::Enabled => true,
        InheritedSetting::Disabled => false,
    }
}

/// A revision captured with one document body.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceRevision(u64);

impl SourceRevision {
    pub const fn value(self) -> u64 {
        self.0
    }
}

impl From<u64> for SourceRevision {
    fn from(value: u64) -> Self {
        Self(value)
    }
}

/// The revisioned document data captured for an export request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportSource {
    pub revision: SourceRevision,
    pub body: String,
}

/// One ordered node in a project section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportNode {
    Group {
        title: String,
        settings: ExportSettings,
        children: Vec<Self>,
    },
    Document {
        id: DocumentId,
        title: String,
        settings: ExportSettings,
    },
}

impl ExportNode {
    pub fn group(title: impl Into<String>, settings: ExportSettings, children: Vec<Self>) -> Self {
        Self::Group {
            title: title.into(),
            settings,
            children,
        }
    }

    pub fn document(id: DocumentId, title: impl Into<String>, settings: ExportSettings) -> Self {
        Self::Document {
            id,
            title: title.into(),
            settings,
        }
    }
}

/// A complete project capture used to create one plan.
///
/// Research, comments, and metadata are present only so callers can pass a
/// whole capture.  Plan construction intentionally never reads them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSnapshot {
    pub styles: ExportStyleCatalog,
    pub defaults: ExportDefaults,
    pub manuscript: Vec<ExportNode>,
    pub research: Vec<ExportNode>,
    pub sources: BTreeMap<DocumentId, ExportSource>,
    pub comments: BTreeMap<DocumentId, Vec<String>>,
    pub metadata: BTreeMap<String, String>,
}

impl ProjectSnapshot {
    pub fn new(
        styles: ExportStyleCatalog,
        defaults: ExportDefaults,
        manuscript: Vec<ExportNode>,
        sources: BTreeMap<DocumentId, ExportSource>,
    ) -> Self {
        Self {
            styles,
            defaults,
            manuscript,
            research: Vec::new(),
            sources,
            comments: BTreeMap::new(),
            metadata: BTreeMap::new(),
        }
    }
}

/// Deterministic project CSS captured independently from application appearance.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExportStyleCatalog {
    css: String,
}

impl ExportStyleCatalog {
    pub fn new(css: impl Into<String>) -> Self {
        Self { css: css.into() }
    }

    pub fn css(&self) -> &str {
        &self.css
    }
}

/// Per-run controls that are not persisted on individual project nodes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExportRunOptions {
    pub numbering: ExportNumbering,
}

/// The numbering policy selected for one export run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ExportNumbering {
    #[default]
    None,
    Documents,
}

/// A requested export before its output target has been checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportRequest {
    pub output_target: String,
    pub run_options: ExportRunOptions,
}

impl ExportRequest {
    pub fn new(output_target: impl Into<String>, run_options: ExportRunOptions) -> Self {
        Self {
            output_target: output_target.into(),
            run_options,
        }
    }
}

/// A checked, portable output name.  It is deliberately not an OS path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExportTargetCapability {
    name: CanonicalRelativePath,
}

impl ExportTargetCapability {
    pub fn checked(target: impl AsRef<str>) -> Result<Self, ExportValidationIssue> {
        let target = target.as_ref();
        CanonicalRelativePath::parse(target)
            .map(|name| Self { name })
            .map_err(|_| ExportValidationIssue::UnsafeOutputTarget {
                target: target.to_owned(),
            })
    }

    pub fn name(&self) -> &CanonicalRelativePath {
        &self.name
    }
}

/// An ordered heading emitted for a group without a prose body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportHeading {
    pub title: String,
    pub settings: EffectiveExportSettings,
}

/// One fully captured document for an immutable export plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportDocument {
    pub id: DocumentId,
    pub title: String,
    pub body: String,
    pub settings: EffectiveExportSettings,
    pub source_revision: SourceRevision,
}

/// A format-neutral item rendered in exactly this order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticExportItem {
    GroupHeading(ExportHeading),
    Document(ExportDocument),
    PageBreak,
}

/// An immutable export plan.  Construction captures no Research, comments, or metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportPlan {
    scope: OrderedExportScope,
    styles: ExportStyleCatalog,
    items: Vec<SemanticExportItem>,
    run_options: ExportRunOptions,
    target: ExportTargetCapability,
    source_revisions: BTreeMap<DocumentId, SourceRevision>,
}

impl ExportPlan {
    pub fn build(
        request: ExportRequest,
        project: &ProjectSnapshot,
    ) -> Result<Self, ExportValidationReport> {
        let mut issues = Vec::new();
        let target = match ExportTargetCapability::checked(&request.output_target) {
            Ok(target) => Some(target),
            Err(issue) => {
                issues.push(issue);
                None
            }
        };
        let mut builder = PlanBuilder::new(&project.sources, issues);
        builder.visit(
            &project.manuscript,
            EffectiveExportSettings::from_defaults(project.defaults),
        );
        if has_mixed_revisions(&builder.revisions) {
            builder
                .issues
                .push(ExportValidationIssue::MixedSourceRevisions);
        }

        if builder.issues.is_empty() {
            Ok(Self {
                scope: OrderedExportScope::EntireManuscript,
                styles: project.styles.clone(),
                items: builder.items,
                run_options: request.run_options,
                target: target.expect("a valid target is present when validation succeeds"),
                source_revisions: builder.revisions,
            })
        } else {
            Err(ExportValidationReport {
                issues: builder.issues,
            })
        }
    }

    pub const fn scope(&self) -> OrderedExportScope {
        self.scope
    }

    pub fn styles(&self) -> &ExportStyleCatalog {
        &self.styles
    }

    pub fn items(&self) -> &[SemanticExportItem] {
        &self.items
    }

    pub const fn run_options(&self) -> ExportRunOptions {
        self.run_options
    }

    pub fn target(&self) -> &ExportTargetCapability {
        &self.target
    }

    pub fn source_revisions(&self) -> &BTreeMap<DocumentId, SourceRevision> {
        &self.source_revisions
    }
}

struct PlanBuilder<'a> {
    sources: &'a BTreeMap<DocumentId, ExportSource>,
    items: Vec<SemanticExportItem>,
    revisions: BTreeMap<DocumentId, SourceRevision>,
    seen_documents: BTreeSet<DocumentId>,
    emitted_document: bool,
    issues: Vec<ExportValidationIssue>,
}

impl<'a> PlanBuilder<'a> {
    fn new(
        sources: &'a BTreeMap<DocumentId, ExportSource>,
        issues: Vec<ExportValidationIssue>,
    ) -> Self {
        Self {
            sources,
            items: Vec::new(),
            revisions: BTreeMap::new(),
            seen_documents: BTreeSet::new(),
            emitted_document: false,
            issues,
        }
    }

    fn visit(&mut self, nodes: &[ExportNode], inherited: EffectiveExportSettings) {
        for node in nodes {
            match node {
                ExportNode::Group {
                    title,
                    settings,
                    children,
                } => {
                    let settings = inherited.apply(*settings);
                    if settings.emit_titles {
                        self.items
                            .push(SemanticExportItem::GroupHeading(ExportHeading {
                                title: title.clone(),
                                settings,
                            }));
                    }
                    self.visit(children, settings);
                }
                ExportNode::Document {
                    id,
                    title,
                    settings,
                } => self.visit_document(*id, title, inherited.apply(*settings)),
            }
        }
    }

    fn visit_document(&mut self, id: DocumentId, title: &str, settings: EffectiveExportSettings) {
        if !self.seen_documents.insert(id) {
            self.issues
                .push(ExportValidationIssue::DuplicateDocument { document: id });
            return;
        }
        let Some(source) = self.sources.get(&id) else {
            self.issues
                .push(ExportValidationIssue::MissingSource { document: id });
            return;
        };
        if settings.start_new_page && self.emitted_document {
            self.items.push(SemanticExportItem::PageBreak);
        }
        self.items
            .push(SemanticExportItem::Document(ExportDocument {
                id,
                title: title.to_owned(),
                body: source.body.clone(),
                settings,
                source_revision: source.revision,
            }));
        self.revisions.insert(id, source.revision);
        self.emitted_document = true;
    }
}

fn has_mixed_revisions(revisions: &BTreeMap<DocumentId, SourceRevision>) -> bool {
    let Some(first) = revisions.values().next().copied() else {
        return false;
    };
    revisions.values().any(|revision| *revision != first)
}

/// A validation problem that prevents an export from starting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportValidationIssue {
    MissingSource { document: DocumentId },
    DuplicateDocument { document: DocumentId },
    MixedSourceRevisions,
    UnsafeOutputTarget { target: String },
}

/// All problems found while validating one export request or plan.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExportValidationReport {
    issues: Vec<ExportValidationIssue>,
}

impl ExportValidationReport {
    pub fn from_issue(issue: ExportValidationIssue) -> Self {
        Self {
            issues: vec![issue],
        }
    }

    pub fn issues(&self) -> &[ExportValidationIssue] {
        &self.issues
    }

    pub fn is_valid(&self) -> bool {
        self.issues.is_empty()
    }
}

/// A failure while planning or writing an export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportError {
    Validation(ExportValidationReport),
    Cancelled,
    Sink {
        operation: &'static str,
        reason: String,
    },
    InvalidState,
}

impl fmt::Display for ExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(report) => write!(formatter, "export validation failed: {report:?}"),
            Self::Cancelled => formatter.write_str("export was cancelled"),
            Self::Sink { operation, reason } => {
                write!(formatter, "export output {operation} failed: {reason}")
            }
            Self::InvalidState => formatter.write_str("export is not in a writable state"),
        }
    }
}

impl Error for ExportError {}

/// A temporary-destination writer supplied by the platform layer.
///
/// `finish` must make the completed output visible atomically.  `abort` must
/// remove or otherwise mark the temporary result incomplete.
pub trait ExportSink: Send {
    fn start(&mut self, target: &ExportTargetCapability) -> Result<(), ExportError>;
    fn write_chunk(&mut self, bytes: &[u8]) -> Result<(), ExportError>;
    fn finish(&mut self) -> Result<(), ExportError>;
    fn abort(&mut self);
}

/// The result returned only after a temporary destination was safely completed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportCompletion {
    pub target: ExportTargetCapability,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportStatus {
    Pending,
    Running,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelOutcome {
    Cancelled,
    TooLate,
}

/// Authoritative progress for one export operation.
///
/// Planning and commit are intentionally phase-only. Rendering is determinate
/// because an immutable plan has a fixed number of semantic items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportProgress {
    Planning,
    Rendering { completed: u64, total: u64 },
    Committing,
}

/// Receives progress from the synchronous exporter worker.
pub trait ExportProgressSink: Send + Sync {
    fn report(&self, progress: ExportProgress);
}

#[derive(Debug, Default)]
pub struct IgnoreExportProgress;

impl ExportProgressSink for IgnoreExportProgress {
    fn report(&self, _: ExportProgress) {}
}

#[derive(Debug)]
struct ExportControl {
    status: Mutex<ExportStatus>,
}

/// A cancellable operation handle.  It never reports completion after cancellation wins.
#[derive(Debug, Clone)]
pub struct ExportHandle {
    control: Arc<ExportControl>,
}

impl Default for ExportHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl ExportHandle {
    pub fn new() -> Self {
        Self {
            control: Arc::new(ExportControl {
                status: Mutex::new(ExportStatus::Pending),
            }),
        }
    }

    pub fn status(&self) -> ExportStatus {
        self.control
            .status
            .lock()
            .map_or(ExportStatus::Failed, |status| *status)
    }

    pub fn same_operation(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.control, &other.control)
    }

    pub fn cancel(&self) -> CancelOutcome {
        let Ok(mut status) = self.control.status.lock() else {
            return CancelOutcome::Cancelled;
        };
        match *status {
            ExportStatus::Pending | ExportStatus::Running => {
                *status = ExportStatus::Cancelled;
                CancelOutcome::Cancelled
            }
            ExportStatus::Completed | ExportStatus::Cancelled | ExportStatus::Failed => {
                CancelOutcome::TooLate
            }
        }
    }

    /// Marks a registered operation failed when planning or validation fails
    /// before a temporary output is started.
    pub fn fail(&self) {
        if let Ok(mut status) = self.control.status.lock()
            && matches!(*status, ExportStatus::Pending | ExportStatus::Running)
        {
            *status = ExportStatus::Failed;
        }
    }

    pub fn begin_temporary<'a>(
        &self,
        sink: &'a mut dyn ExportSink,
        target: &ExportTargetCapability,
    ) -> Result<TemporaryExport<'a>, ExportError> {
        {
            let mut status = self
                .control
                .status
                .lock()
                .map_err(|_| ExportError::InvalidState)?;
            match *status {
                ExportStatus::Pending => *status = ExportStatus::Running,
                ExportStatus::Cancelled => return Err(ExportError::Cancelled),
                ExportStatus::Running | ExportStatus::Completed | ExportStatus::Failed => {
                    return Err(ExportError::InvalidState);
                }
            }
        }
        if let Err(error) = sink.start(target) {
            sink.abort();
            self.fail_if_running();
            return Err(error);
        }
        if self.status() == ExportStatus::Cancelled {
            sink.abort();
            return Err(ExportError::Cancelled);
        }
        Ok(TemporaryExport {
            handle: self.clone(),
            sink,
            target: target.clone(),
            settled: false,
        })
    }

    fn fail_if_running(&self) {
        if let Ok(mut status) = self.control.status.lock()
            && *status == ExportStatus::Running
        {
            *status = ExportStatus::Failed;
        }
    }
}

/// An active temporary output.  Dropping it cannot create a completed result.
pub struct TemporaryExport<'a> {
    handle: ExportHandle,
    sink: &'a mut dyn ExportSink,
    target: ExportTargetCapability,
    settled: bool,
}

impl TemporaryExport<'_> {
    /// Writes one bounded render chunk, observing cancellation before each chunk.
    pub fn write_chunk(&mut self, bytes: &[u8]) -> Result<(), ExportError> {
        if self.handle.status() == ExportStatus::Cancelled {
            self.sink.abort();
            self.settled = true;
            return Err(ExportError::Cancelled);
        }
        if self.handle.status() != ExportStatus::Running {
            return Err(ExportError::InvalidState);
        }
        if let Err(error) = self.sink.write_chunk(bytes) {
            self.sink.abort();
            self.handle.fail_if_running();
            self.settled = true;
            return Err(error);
        }
        Ok(())
    }

    /// Makes the output visible only if cancellation has not already won.
    pub fn finish(mut self) -> Result<ExportCompletion, ExportError> {
        let mut status = self
            .handle
            .control
            .status
            .lock()
            .map_err(|_| ExportError::InvalidState)?;
        if *status == ExportStatus::Cancelled {
            self.sink.abort();
            self.settled = true;
            return Err(ExportError::Cancelled);
        }
        if *status != ExportStatus::Running {
            self.sink.abort();
            self.settled = true;
            return Err(ExportError::InvalidState);
        }
        match self.sink.finish() {
            Ok(()) => {
                *status = ExportStatus::Completed;
                self.settled = true;
                Ok(ExportCompletion {
                    target: self.target.clone(),
                })
            }
            Err(error) => {
                self.sink.abort();
                *status = ExportStatus::Failed;
                self.settled = true;
                Err(error)
            }
        }
    }
}

impl Drop for TemporaryExport<'_> {
    fn drop(&mut self) {
        if !self.settled {
            self.sink.abort();
            self.handle.fail_if_running();
        }
    }
}

/// The common boundary implemented by each export format.
pub trait Exporter: Send + Sync {
    fn plan(
        &self,
        request: ExportRequest,
        project: &ProjectSnapshot,
    ) -> Result<ExportPlan, ExportError>;
    fn validate(&self, plan: &ExportPlan) -> ExportValidationReport;
    fn export(
        &self,
        plan: ExportPlan,
        sink: Box<dyn ExportSink>,
        handle: ExportHandle,
        progress: Arc<dyn ExportProgressSink>,
    ) -> Result<ExportCompletion, ExportError>;
    fn cancel(&self, handle: &ExportHandle) {
        let _ = handle.cancel();
    }
}

#[cfg(test)]
mod export_contract_tests;
