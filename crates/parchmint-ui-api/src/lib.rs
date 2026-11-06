//! Framework-neutral desktop UI boundary.
//!
//! This crate transfers ParchMint values and services into a desktop UI
//! implementation. Concrete widgets, event loops, and native handles belong
//! to implementation crates.

use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

use parchmint_application::{
    DocumentSnapshot, DocumentVisibility, GlobalReplacement, ProjectCommandDispatcher,
};
use parchmint_assistant_api::{AssistantError, AssistantMode, AssistantResponse, ContextPassage};
use parchmint_domain::Project;
use parchmint_editor_api::EditorAdapter;
use parchmint_editor_api::EditorRevision;
use parchmint_export_api::ExportRunOptions;
use parchmint_export_api::Exporter;
use parchmint_history_api::{CheckpointId, HistoryStore};
use parchmint_platform_api::{
    ApplicationPathService, ClipboardService, CredentialStore, DialogService, ExternalOpenService,
    MenuActivationService, MenuService, SystemAppearanceEventService, SystemAppearanceService,
    UntrustedPathSelection, WindowCapability,
};
use parchmint_preferences::{AppearanceService, PreferenceService, ThemeSnapshot};
use parchmint_recovery_api::RecoveryJournal;
use parchmint_save::SaveStatusSnapshot;
use parchmint_search_api::SearchIndex;
use parchmint_workspace_state::WorkspaceStateStore;

pub use parchmint_application::{
    CreateDocumentWorkflow, CreateResearchDocumentWorkflow, CreatedDocumentRevision,
    DeleteSubtreesWorkflow, DeletedSubtreesRevision, DuplicateSubtreesWorkflow,
    DuplicatedSubtreesRevision, DurableProjectionAck, MoveNodeWorkflow, MoveNodesWorkflow,
    PersistenceRecoveryState as ProjectRecoveryState,
    PersistenceRevision as ProjectPersistenceRevision, PersistenceSaveHandle as ProjectSaveHandle,
    PersistenceSaveKind as ProjectSaveKind, PersistenceSavedRevision as SavedProjectRevision,
    PersistenceStatus as ProjectPersistenceStatus, ProjectPersistenceError,
    RecoveryAcceptance as ProjectRecoveryAcceptance, RestoredProjectRevision,
};
pub use parchmint_editor_api::CanonicalProjection;
pub use parchmint_spellcheck_api::{
    DictionaryRevision, LanguageId, RevisionedTextRange, SpellcheckGeneration, SpellcheckPriority,
    SpellcheckRequest, SpellcheckResult, SpellcheckService,
};

/// The exit status returned when a desktop UI finishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExitCode(i32);

impl ExitCode {
    pub const SUCCESS: Self = Self(0);

    pub const fn new(value: i32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> i32 {
        self.0
    }
}

/// A UI startup or runtime failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiError {
    message: String,
}

impl UiError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for UiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for UiError {}

/// A project path requested at process startup before project services validate it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestedProjectPath(PathBuf);

impl RequestedProjectPath {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn into_path(self) -> PathBuf {
        self.0
    }
}

/// A ParchMint-owned capability for one live project session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectSessionCapability {
    session_id: u64,
    generation: u64,
}

impl ProjectSessionCapability {
    pub const fn session_id(self) -> u64 {
        self.session_id
    }

    pub const fn generation(self) -> u64 {
        self.generation
    }
}

/// Tracks the live project sessions a UI may scope work to.
///
/// Retiring and recreating a session keeps its logical ID while incrementing
/// its generation, so delayed work cannot target the replacement session.
#[derive(Debug, Default)]
pub struct ProjectSessionRegistry {
    sessions: BTreeMap<u64, SessionState>,
}

#[derive(Debug, Clone, Copy, Default)]
struct SessionState {
    generation: u64,
    live: bool,
}

impl ProjectSessionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, session_id: u64) -> ProjectSessionCapability {
        let state = self.sessions.entry(session_id).or_default();
        state.generation = state.generation.saturating_add(1);
        state.live = true;
        ProjectSessionCapability {
            session_id,
            generation: state.generation,
        }
    }

    pub fn is_current(&self, capability: ProjectSessionCapability) -> bool {
        self.sessions
            .get(&capability.session_id)
            .is_some_and(|state| state.live && state.generation == capability.generation)
    }

    pub fn retire(&mut self, capability: ProjectSessionCapability) -> bool {
        if !self.is_current(capability) {
            return false;
        }
        self.sessions
            .get_mut(&capability.session_id)
            .expect("current session must have registry state")
            .live = false;
        true
    }
}

/// An immutable, framework-neutral view of the authored project state.
///
/// The snapshot is suitable for initial UI hydration. Consumers that need a
/// newer view must ask the session-scoped [`ProjectSnapshotQuery`] again.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectSnapshot {
    pub project: Project,
    /// Summaries for every canonical document, including bodies not yet loaded.
    pub document_summaries: Vec<DocumentSummary>,
    /// Session-loaded bodies only. Initial hydration normally contains one body.
    pub documents: Vec<DocumentSnapshot>,
    /// Canonical authored project styles used by export planning.
    pub styles_css: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentWordCount {
    Pending,
    Known(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSummary {
    pub document_id: parchmint_domain::DocumentId,
    pub revision: EditorRevision,
    pub visibility: DocumentVisibility,
    pub content_hash: Option<[u8; 32]>,
    pub word_count: DocumentWordCount,
}

/// A failure while reading current project state for the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectQueryError {
    message: String,
    before_mutation: bool,
}

impl ProjectQueryError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            before_mutation: false,
        }
    }

    /// The operation failed before it changed project state.
    pub fn not_started(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            before_mutation: true,
        }
    }

    pub fn is_not_started(&self) -> bool {
        self.before_mutation
    }
}

impl fmt::Display for ProjectQueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for ProjectQueryError {}

/// Authoritative current-project reads supplied beside mutation commands.
pub trait ProjectSnapshotQuery: Send + Sync {
    fn snapshot(&self) -> Result<ProjectSnapshot, ProjectQueryError>;

    /// Materializes every live document body for an off-loop read workflow.
    fn snapshot_with_documents(&self) -> Result<ProjectSnapshot, ProjectQueryError> {
        self.snapshot()
    }

    /// Returns last-durable source bytes after the resource owner checks them.
    /// Callers must also compare their captured manifest metadata before use.
    fn research_original_bytes(
        &self,
        _source_id: parchmint_domain::ResearchSourceId,
    ) -> Result<Vec<u8>, ProjectQueryError> {
        Err(ProjectQueryError::new("Research original is unavailable"))
    }

    /// Loads and validates one canonical document body for this exact session.
    fn load_document(
        &self,
        document: parchmint_domain::DocumentId,
    ) -> Result<DocumentSnapshot, ProjectQueryError> {
        self.snapshot()?
            .documents
            .into_iter()
            .find(|snapshot| snapshot.document_id == document)
            .ok_or_else(|| ProjectQueryError::new("document body is not loaded"))
    }
}

/// Read-only save state exposed to the UI.
///
/// Save initiation, final-save reconciliation, and writable-lease ownership
/// remain with the desktop runtime.
pub trait ProjectSaveStatus: Send + Sync {
    fn status(&self) -> SaveStatusSnapshot;
}

/// High-level persistence operations for one exact writable project session.
/// Canonical writes, recovery payloads, and History inputs stay behind this port.
pub trait ProjectPersistencePort: Send + Sync {
    fn persist_editor_projection(
        &self,
        projection: CanonicalProjection,
    ) -> Result<DurableProjectionAck, ProjectPersistenceError>;

    fn request_save(
        &self,
        kind: ProjectSaveKind,
    ) -> Result<(ProjectSaveHandle, ProjectPersistenceRevision), ProjectPersistenceError>;

    /// Starts an ordinary save only when authored project state changed.
    ///
    /// A `None` result is a successful no-op. Named snapshots and other
    /// meaningful checkpoint kinds still return a save handle.
    fn request_save_if_changed(
        &self,
        kind: ProjectSaveKind,
    ) -> Result<Option<(ProjectSaveHandle, ProjectPersistenceRevision)>, ProjectPersistenceError>;

    fn await_save(
        &self,
        handle: ProjectSaveHandle,
    ) -> Result<SavedProjectRevision, ProjectPersistenceError>;

    fn status(&self) -> ProjectPersistenceStatus;

    fn reconcile_recovery(&self) -> Result<ProjectRecoveryState, ProjectPersistenceError>;

    fn accept_recovery(
        &self,
        acceptance: ProjectRecoveryAcceptance,
    ) -> Result<ProjectRecoveryState, ProjectPersistenceError>;

    fn discard_recovery(
        &self,
        acceptance: ProjectRecoveryAcceptance,
    ) -> Result<ProjectRecoveryState, ProjectPersistenceError>;
}

/// Result of a high-level authored-project mutation after its structural save.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectWorkflowSnapshot {
    pub snapshot: ProjectSnapshot,
    pub checkpoint: CheckpointId,
}

/// Authoritative result of one atomic subtree duplicate workflow.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectDuplicateWorkflowSnapshot {
    pub workflow: ProjectWorkflowSnapshot,
    pub created_roots: Vec<parchmint_domain::NodeId>,
    pub node_ids: BTreeMap<parchmint_domain::NodeId, parchmint_domain::NodeId>,
    pub document_ids: BTreeMap<parchmint_domain::DocumentId, parchmint_domain::DocumentId>,
}

/// Explicit author-selected URL and destination for one original source.
#[derive(Clone, PartialEq, Eq)]
pub struct FetchResearchOriginalWorkflow {
    pub document: CreateDocumentWorkflow,
    pub source_id: parchmint_domain::ResearchSourceId,
    pub source_url: String,
    pub body: String,
}

impl fmt::Debug for FetchResearchOriginalWorkflow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FetchResearchOriginalWorkflow")
            .field("document", &self.document)
            .field("source_id", &self.source_id)
            .finish_non_exhaustive()
    }
}

/// Production-owned workflows that must coordinate domain, document, and
/// storage state rather than exposing low-level write plans to the UI.
pub trait ProjectWorkflowPort: Send + Sync {
    fn set_continuity_decision(
        &self,
        _record: parchmint_domain::ContinuityDecisionRecord,
        _expected: Option<parchmint_domain::ContinuityDecisionRecord>,
    ) -> Result<ProjectWorkflowSnapshot, ProjectQueryError> {
        Err(ProjectQueryError::new(
            "Continuity decisions are unavailable",
        ))
    }

    fn create_document(
        &self,
        request: CreateDocumentWorkflow,
    ) -> Result<ProjectWorkflowSnapshot, ProjectQueryError>;

    fn create_research_document(
        &self,
        _request: CreateResearchDocumentWorkflow,
    ) -> Result<ProjectWorkflowSnapshot, ProjectQueryError> {
        Err(ProjectQueryError::new("Research import is unavailable"))
    }

    fn fetch_research_original(
        &self,
        _request: FetchResearchOriginalWorkflow,
    ) -> Result<ProjectWorkflowSnapshot, ProjectQueryError> {
        Err(ProjectQueryError::new(
            "Original-source import is unavailable",
        ))
    }

    fn save_research_original(
        &self,
        _source_id: parchmint_domain::ResearchSourceId,
        _destination: parchmint_platform_api::UntrustedPathSelection,
    ) -> Result<(), ProjectQueryError> {
        Err(ProjectQueryError::new(
            "Saving the original source is unavailable",
        ))
    }

    fn restore_checkpoint(
        &self,
        checkpoint: CheckpointId,
    ) -> Result<ProjectWorkflowSnapshot, ProjectQueryError>;

    /// Restores one live document's writing and comments while preserving the
    /// current outline, project settings, and every other document.
    fn restore_document_checkpoint(
        &self,
        checkpoint: CheckpointId,
        document: parchmint_domain::DocumentId,
    ) -> Result<ProjectWorkflowSnapshot, ProjectQueryError>;

    fn create_named_snapshot(
        &self,
        name: String,
    ) -> Result<ProjectWorkflowSnapshot, ProjectQueryError>;

    fn delete_subtrees(
        &self,
        request: DeleteSubtreesWorkflow,
    ) -> Result<ProjectWorkflowSnapshot, ProjectQueryError>;

    fn restore_deleted_subtree(
        &self,
        node: parchmint_domain::NodeId,
    ) -> Result<ProjectWorkflowSnapshot, ProjectQueryError>;

    fn move_nodes(
        &self,
        request: MoveNodesWorkflow,
    ) -> Result<ProjectWorkflowSnapshot, ProjectQueryError>;

    fn duplicate_subtrees(
        &self,
        request: DuplicateSubtreesWorkflow,
    ) -> Result<ProjectDuplicateWorkflowSnapshot, ProjectQueryError>;
}

/// Opaque proof of a completed production export. It deliberately does not
/// expose an operating-system path back to UI code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExportArtifactToken(u64);

impl ExportArtifactToken {
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportArtifact {
    pub token: ExportArtifactToken,
    pub display_name: String,
}

/// Identifies one registered export operation independently of its filename.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExportOperationToken(u64);

impl ExportOperationToken {
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportOutcome {
    Completed(ExportArtifact),
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportArtifactAction {
    Open,
    Reveal,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum HistoryMaintenanceStatus {
    #[default]
    Available,
    Reinitializable {
        problem: String,
    },
    Unavailable {
        problem: String,
        reason: String,
    },
}

pub trait ProjectHistoryMaintenancePort: Send + Sync {
    fn status(&self) -> Result<HistoryMaintenanceStatus, ProjectQueryError>;
    fn reinitialize(&self) -> Result<String, ProjectQueryError>;
}

/// Validates an untrusted SaveFile selection, acquires the writable target,
/// runs planning/validation/export, and retains the completed artifact for
/// later file-specific open or reveal actions.
pub trait ProjectExportPort: Send + Sync {
    /// Registers the cancellation handle and progress receiver before any
    /// planning or rendering begins. Registering a new operation makes the
    /// previous operation stale and requests its cancellation.
    fn begin_export(
        &self,
        progress: Arc<dyn parchmint_export_api::ExportProgressSink>,
    ) -> Result<ExportOperationToken, ProjectQueryError>;

    fn export_to_path(
        &self,
        operation: ExportOperationToken,
        selection: UntrustedPathSelection,
        options: ExportRunOptions,
    ) -> Result<ExportOutcome, ProjectQueryError>;

    fn cancel_export(
        &self,
        operation: ExportOperationToken,
    ) -> Result<parchmint_export_api::CancelOutcome, ProjectQueryError>;

    fn act_on_artifact(
        &self,
        artifact: ExportArtifactToken,
        action: ExportArtifactAction,
    ) -> Result<(), ProjectQueryError>;
}

impl ProjectPersistencePort for parchmint_application::ProjectPersistenceCoordinator {
    fn persist_editor_projection(
        &self,
        projection: CanonicalProjection,
    ) -> Result<DurableProjectionAck, ProjectPersistenceError> {
        self.persist_editor_projection(projection)
    }

    fn request_save(
        &self,
        kind: ProjectSaveKind,
    ) -> Result<(ProjectSaveHandle, ProjectPersistenceRevision), ProjectPersistenceError> {
        self.request_save(kind)
    }

    fn request_save_if_changed(
        &self,
        kind: ProjectSaveKind,
    ) -> Result<Option<(ProjectSaveHandle, ProjectPersistenceRevision)>, ProjectPersistenceError>
    {
        self.request_save_if_changed(kind)
    }

    fn await_save(
        &self,
        handle: ProjectSaveHandle,
    ) -> Result<SavedProjectRevision, ProjectPersistenceError> {
        self.await_save(handle)
    }

    fn status(&self) -> ProjectPersistenceStatus {
        self.status()
    }

    fn reconcile_recovery(&self) -> Result<ProjectRecoveryState, ProjectPersistenceError> {
        self.reconcile_recovery()
    }

    fn accept_recovery(
        &self,
        acceptance: ProjectRecoveryAcceptance,
    ) -> Result<ProjectRecoveryState, ProjectPersistenceError> {
        self.accept_recovery(acceptance)
    }

    fn discard_recovery(
        &self,
        acceptance: ProjectRecoveryAcceptance,
    ) -> Result<ProjectRecoveryState, ProjectPersistenceError> {
        self.discard_recovery(acceptance)
    }
}

/// Decides whether an exact project-session generation may still start work.
pub trait ProjectSessionAuthority: Send + Sync {
    fn is_current(&self, session: ProjectSessionCapability) -> bool;
}

/// An attempt to use ports after their exact project session was retired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaleProjectSession {
    session: ProjectSessionCapability,
}

impl StaleProjectSession {
    pub const fn session(self) -> ProjectSessionCapability {
        self.session
    }
}

impl fmt::Display for StaleProjectSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "project session {} generation {} is no longer current",
            self.session.session_id(),
            self.session.generation()
        )
    }
}

impl Error for StaleProjectSession {}

/// Application command services used by a desktop UI.
#[derive(Clone)]
pub struct ApplicationServices {
    pub commands: Arc<dyn ProjectCommandDispatcher>,
    pub replacements: Arc<dyn GlobalReplacement>,
}

impl ApplicationServices {
    pub fn new(
        commands: Arc<dyn ProjectCommandDispatcher>,
        replacements: Arc<dyn GlobalReplacement>,
    ) -> Self {
        Self {
            commands,
            replacements,
        }
    }
}

/// Platform services used by a desktop UI.
#[derive(Clone)]
pub struct PlatformServices {
    pub credentials: Option<Arc<dyn CredentialStore>>,
    pub session_credentials: Option<Arc<dyn CredentialStore>>,
    pub menus: Arc<dyn MenuService>,
    pub menu_activations: Option<Arc<dyn MenuActivationService>>,
    pub dialogs: Arc<dyn DialogService>,
    pub clipboard: Arc<dyn ClipboardService>,
    pub external_open: Arc<dyn ExternalOpenService>,
    pub application_paths: Arc<dyn ApplicationPathService>,
    pub system_appearance: Arc<dyn SystemAppearanceService>,
    pub system_appearance_events: Option<Arc<dyn SystemAppearanceEventService>>,
}

/// Typed framework-neutral services belonging to one writable project lease.
///
/// The fields are private so a UI cannot clone a raw service and accidentally
/// bypass session authorization. Use [`ProjectUiPorts::access`] to obtain a
/// short-lived typed view after checking the exact session generation.
#[derive(Clone)]
pub struct ProjectUiServices {
    assistant: Option<Arc<dyn AssistantWorkflowPort>>,
    application: ApplicationServices,
    query: Arc<dyn ProjectSnapshotQuery>,
    history: Arc<dyn HistoryStore>,
    history_maintenance: Arc<dyn ProjectHistoryMaintenancePort>,
    recovery: Arc<dyn RecoveryJournal>,
    search: Arc<dyn SearchIndex>,
    save_status: Arc<dyn ProjectSaveStatus>,
    persistence: Arc<dyn ProjectPersistencePort>,
    workflows: Arc<dyn ProjectWorkflowPort>,
    export_target: Arc<dyn ProjectExportPort>,
    exporter: Arc<dyn Exporter>,
    editor: Arc<dyn EditorAdapter>,
    spellcheck: Arc<dyn SpellcheckService>,
    workspace_state: Arc<dyn WorkspaceStateStore>,
    preferences: Arc<dyn PreferenceService>,
    appearance: Arc<dyn AppearanceService>,
    platform: PlatformServices,
}

impl ProjectUiServices {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        application: ApplicationServices,
        query: Arc<dyn ProjectSnapshotQuery>,
        history: Arc<dyn HistoryStore>,
        history_maintenance: Arc<dyn ProjectHistoryMaintenancePort>,
        recovery: Arc<dyn RecoveryJournal>,
        search: Arc<dyn SearchIndex>,
        save_status: Arc<dyn ProjectSaveStatus>,
        persistence: Arc<dyn ProjectPersistencePort>,
        workflows: Arc<dyn ProjectWorkflowPort>,
        export_target: Arc<dyn ProjectExportPort>,
        exporter: Arc<dyn Exporter>,
        editor: Arc<dyn EditorAdapter>,
        spellcheck: Arc<dyn SpellcheckService>,
        workspace_state: Arc<dyn WorkspaceStateStore>,
        preferences: Arc<dyn PreferenceService>,
        appearance: Arc<dyn AppearanceService>,
        platform: PlatformServices,
    ) -> Self {
        Self {
            assistant: None,
            application,
            query,
            history,
            history_maintenance,
            recovery,
            search,
            save_status,
            persistence,
            workflows,
            export_target,
            exporter,
            editor,
            spellcheck,
            workspace_state,
            preferences,
            appearance,
            platform,
        }
    }

    pub fn with_assistant(mut self, assistant: Arc<dyn AssistantWorkflowPort>) -> Self {
        self.assistant = Some(assistant);
        self
    }
}

/// Maximum captured selection size; Check reserves remaining room for its target.
pub const MAX_ASSISTANT_SELECTION_BYTES: usize = 32 * 1024;

/// Explicit, one-request author permission to include notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssistantExplicitNotes {
    Document {
        document: parchmint_domain::DocumentId,
        project_revision: u64,
        tree_hash: [u8; 32],
    },
    Project {
        project_revision: u64,
        tree_hash: [u8; 32],
    },
}

/// The hierarchy facts that determine which documents a scope can include.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssistantNoteTreeEntry {
    pub node: parchmint_domain::NodeId,
    pub parent: Option<parchmint_domain::NodeId>,
    /// 0 = root, 1 = group, 2 = document.
    pub kind: u8,
    pub root_section: Option<parchmint_domain::ProjectSection>,
    pub document: Option<parchmint_domain::DocumentId>,
}

pub fn assistant_note_tree_hash(
    project_id: parchmint_domain::ProjectId,
    mut entries: Vec<AssistantNoteTreeEntry>,
) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    entries.sort_by_key(|entry| entry.node);
    let mut digest = Sha256::new();
    digest.update(b"parchmint.note-tree.v1\0");
    digest.update(project_id.as_bytes());
    for entry in entries {
        digest.update(entry.node.as_bytes());
        let section = match entry.root_section {
            None => 0,
            Some(parchmint_domain::ProjectSection::Manuscript) => 1,
            Some(parchmint_domain::ProjectSection::Lore) => 2,
            Some(parchmint_domain::ProjectSection::Research) => 3,
            Some(parchmint_domain::ProjectSection::Unfiled) => 4,
        };
        digest.update([
            entry.kind,
            section,
            u8::from(entry.parent.is_some()),
            u8::from(entry.document.is_some()),
        ]);
        if let Some(parent) = entry.parent {
            digest.update(parent.as_bytes());
        }
        if let Some(document) = entry.document {
            digest.update(document.as_bytes());
        }
    }
    digest.finalize().into()
}

/// One author-requested assistant operation against this project session.
#[derive(Debug, Clone, PartialEq)]
pub struct AssistantWorkflowRequest {
    pub mode: AssistantMode,
    pub prompt: String,
    pub target: Option<parchmint_domain::NodeId>,
    /// Optional evidence scope. Check also includes its separate target.
    pub context_scope: Option<parchmint_domain::NodeId>,
    /// Exact editor selection captured at submission, when selection-only is enabled.
    pub selected_text: Option<AssistantSelectedText>,
    /// Explicit project nodes, expanded to their current descendant documents.
    pub pinned_context: Vec<parchmint_domain::NodeId>,
    /// Exclusions take precedence over every implicit or pinned context source.
    pub excluded_context: Vec<parchmint_domain::NodeId>,
    pub history: Vec<AssistantHistoryTurn>,
    /// These are granted only in response to an explicit author request.
    pub include_notes: bool,
    /// Set only by a local user control; never inferred from provider output.
    pub explicit_notes: Option<AssistantExplicitNotes>,
    pub create_note: bool,
    /// Captured editor revisions, including edits not yet persisted.
    pub live_documents: Vec<CanonicalProjection>,
    /// An explicit request to propose an edit to one verified Check passage.
    pub edit_proposal: Option<AssistantEditProposalRequest>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssistantEditProposalRequest {
    pub finding: parchmint_assistant_api::CheckFinding,
    pub scope: parchmint_assistant_api::CheckScope,
    pub context: Vec<ContextPassage>,
    pub evidence_index: usize,
}

/// The target is established from local source evidence, not model output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssistantEditProposalTarget {
    pub document: parchmint_domain::DocumentId,
    pub revision: u64,
    pub start: u64,
    pub end: u64,
    pub original: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssistantSelectedText {
    pub document: parchmint_domain::DocumentId,
    pub revision: EditorRevision,
    pub text: String,
}

/// A previous exchange, tagged with the actual destination and note access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssistantHistoryTurn {
    pub question: String,
    pub answer: String,
    pub provider_binding: String,
    pub notes_derived: bool,
}

/// Response and exact evidence captured before the request was dispatched.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AssistantWorkflowResult {
    pub response: AssistantResponse,
    /// Locally validated Check findings; absent for older or unstructured results.
    #[serde(default)]
    pub check_report: Option<parchmint_assistant_api::CheckReport>,
    #[serde(default)]
    pub check_report_invalid: bool,
    pub context: Vec<ContextPassage>,
    pub context_truncated: bool,
    /// Local extraction limitations for sources in this request's allowed scope.
    #[serde(default)]
    pub context_warnings: Vec<String>,
    pub estimated_cost_micros: Option<u64>,
    pub estimated_period_total_micros: Option<u64>,
    pub usage_warning: Option<String>,
    pub web_retrieved_at_unix_seconds: Option<u64>,
    pub provider_binding: String,
    pub notes_derived: bool,
    /// Session-owned proposal target; older saved conversations lack this field.
    #[serde(skip)]
    pub edit_proposal_target: Option<AssistantEditProposalTarget>,
}

/// Private application-data state, never part of a project or its History.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AssistantConversationState {
    pub threads: Vec<AssistantSavedThread>,
    #[serde(default)]
    pub research_import: Option<AssistantSavedResearchImport>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AssistantSavedResearchForm {
    Summary,
    SourceLink,
    Original,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AssistantSavedResearchImport {
    pub nonce: [u8; 16],
    pub title: String,
    pub parent_id: String,
    pub question: String,
    pub answer: String,
    pub sources: Vec<parchmint_assistant_api::WebCitation>,
    pub form: AssistantSavedResearchForm,
    pub retrieved_at: u64,
    pub manual_url: Option<String>,
    pub original_source: usize,
    #[serde(default)]
    pub fresh_id_on_edit: bool,
    /// A save started before the snapshot was written; completion is unknown.
    pub commit_started: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AssistantSavedThread {
    pub mode: AssistantMode,
    /// Empty only when loading a version-1, single-thread-per-mode file.
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub auto_title: bool,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub research_import: Option<AssistantSavedResearchImport>,
    pub draft: String,
    pub turns: Vec<AssistantSavedTurn>,
    pub note_proposal: Option<String>,
    pub interrupted_prompts: Vec<String>,
    #[serde(default)]
    pub pinned_context: Vec<String>,
    #[serde(default)]
    pub excluded_context: Vec<String>,
    #[serde(default)]
    pub context_scope: Option<String>,
    #[serde(default)]
    pub selection_only: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AssistantSavedTurn {
    pub question: String,
    pub result: AssistantWorkflowResult,
}

/// Blocking work belongs on a worker, and callers must recheck their session
/// and request generations when accepting results.
pub trait AssistantWorkflowPort: Send + Sync {
    /// Returns a snapshot loaded on the project-open worker; no disk I/O here.
    fn conversation_snapshot(&self) -> Result<AssistantConversationState, String> {
        Ok(AssistantConversationState::default())
    }
    /// Enqueues a private application-data snapshot without blocking the UI.
    fn queue_conversation(&self, _state: AssistantConversationState) -> Result<(), String> {
        Ok(())
    }
    /// Nonblocking status: true while a queued write is in progress.
    fn conversation_write_pending(&self) -> Result<bool, String> {
        Ok(false)
    }
    /// Runs on a worker before dispatch and project close.
    fn flush_conversation(&self) -> Result<(), String> {
        Ok(())
    }
    fn run(
        &self,
        request: AssistantWorkflowRequest,
        cancelled: &AtomicBool,
    ) -> Result<AssistantWorkflowResult, AssistantError>;

    /// Provisional text is transient and must not be persisted or treated as
    /// validated evidence. Implementations may omit progress entirely.
    fn run_with_progress(
        &self,
        request: AssistantWorkflowRequest,
        cancelled: &AtomicBool,
        _progress: &mut dyn FnMut(&str),
    ) -> Result<AssistantWorkflowResult, AssistantError> {
        self.run(request, cancelled)
    }
}

/// Session-scoped access to every typed project UI port.
#[derive(Clone)]
pub struct ProjectUiPorts {
    session: ProjectSessionCapability,
    services: Arc<ProjectUiServices>,
    authority: Arc<dyn ProjectSessionAuthority>,
}

impl fmt::Debug for ProjectUiPorts {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectUiPorts")
            .field("session", &self.session)
            .field("current", &self.is_current())
            .finish_non_exhaustive()
    }
}

impl ProjectUiPorts {
    pub fn new(
        session: ProjectSessionCapability,
        services: ProjectUiServices,
        authority: Arc<dyn ProjectSessionAuthority>,
    ) -> Self {
        Self {
            session,
            services: Arc::new(services),
            authority,
        }
    }

    pub const fn session(&self) -> ProjectSessionCapability {
        self.session
    }

    pub fn is_current(&self) -> bool {
        self.authority.is_current(self.session)
    }

    fn authorize(&self) -> Result<(), StaleProjectSession> {
        self.is_current().then_some(()).ok_or(StaleProjectSession {
            session: self.session,
        })
    }

    /// Authorizes this exact generation and returns borrowed typed services.
    /// Callers reacquire access for each user action or asynchronous task.
    pub fn access(&self) -> Result<ProjectUiAccess<'_>, StaleProjectSession> {
        self.authorize()?;
        Ok(ProjectUiAccess { ports: self })
    }
}

/// Borrowed typed services for one authorization check.
pub struct ProjectUiAccess<'a> {
    ports: &'a ProjectUiPorts,
}

impl ProjectUiAccess<'_> {
    fn services(&self) -> Result<&ProjectUiServices, StaleProjectSession> {
        self.ports.authorize()?;
        Ok(self.ports.services.as_ref())
    }

    pub fn assistant<R>(
        &self,
        use_service: impl FnOnce(Option<&dyn AssistantWorkflowPort>) -> R,
    ) -> Result<R, StaleProjectSession> {
        Ok(use_service(self.services()?.assistant.as_deref()))
    }

    pub fn snapshot<R>(
        &self,
        use_service: impl for<'a> FnOnce(&'a dyn ProjectSnapshotQuery) -> R,
    ) -> Result<R, StaleProjectSession> {
        Ok(use_service(self.services()?.query.as_ref()))
    }

    pub fn commands<R>(
        &self,
        use_service: impl for<'a> FnOnce(&'a dyn ProjectCommandDispatcher) -> R,
    ) -> Result<R, StaleProjectSession> {
        Ok(use_service(self.services()?.application.commands.as_ref()))
    }

    /// Borrows the command port for an async call whose future is awaited
    /// while this authorized access value remains alive.
    pub fn commands_service(&self) -> Result<&dyn ProjectCommandDispatcher, StaleProjectSession> {
        Ok(self.services()?.application.commands.as_ref())
    }

    pub fn replacements<R>(
        &self,
        use_service: impl for<'a> FnOnce(&'a dyn GlobalReplacement) -> R,
    ) -> Result<R, StaleProjectSession> {
        Ok(use_service(
            self.services()?.application.replacements.as_ref(),
        ))
    }

    /// Borrows the replacement port for an async call whose future is awaited
    /// while this authorized access value remains alive.
    pub fn replacements_service(&self) -> Result<&dyn GlobalReplacement, StaleProjectSession> {
        Ok(self.services()?.application.replacements.as_ref())
    }

    pub fn history<R>(
        &self,
        use_service: impl for<'a> FnOnce(&'a dyn HistoryStore) -> R,
    ) -> Result<R, StaleProjectSession> {
        Ok(use_service(self.services()?.history.as_ref()))
    }

    pub fn history_maintenance<R>(
        &self,
        use_service: impl for<'a> FnOnce(&'a dyn ProjectHistoryMaintenancePort) -> R,
    ) -> Result<R, StaleProjectSession> {
        Ok(use_service(self.services()?.history_maintenance.as_ref()))
    }

    pub fn recovery<R>(
        &self,
        use_service: impl for<'a> FnOnce(&'a dyn RecoveryJournal) -> R,
    ) -> Result<R, StaleProjectSession> {
        Ok(use_service(self.services()?.recovery.as_ref()))
    }

    pub fn search<R>(
        &self,
        use_service: impl for<'a> FnOnce(&'a dyn SearchIndex) -> R,
    ) -> Result<R, StaleProjectSession> {
        Ok(use_service(self.services()?.search.as_ref()))
    }

    pub fn save_status<R>(
        &self,
        use_service: impl for<'a> FnOnce(&'a dyn ProjectSaveStatus) -> R,
    ) -> Result<R, StaleProjectSession> {
        Ok(use_service(self.services()?.save_status.as_ref()))
    }

    pub fn persistence<R>(
        &self,
        use_service: impl for<'a> FnOnce(&'a dyn ProjectPersistencePort) -> R,
    ) -> Result<R, StaleProjectSession> {
        Ok(use_service(self.services()?.persistence.as_ref()))
    }

    pub fn workflows<R>(
        &self,
        use_service: impl for<'a> FnOnce(&'a dyn ProjectWorkflowPort) -> R,
    ) -> Result<R, StaleProjectSession> {
        Ok(use_service(self.services()?.workflows.as_ref()))
    }

    pub fn export_target<R>(
        &self,
        use_service: impl for<'a> FnOnce(&'a dyn ProjectExportPort) -> R,
    ) -> Result<R, StaleProjectSession> {
        Ok(use_service(self.services()?.export_target.as_ref()))
    }

    pub fn exporter<R>(
        &self,
        use_service: impl for<'a> FnOnce(&'a dyn Exporter) -> R,
    ) -> Result<R, StaleProjectSession> {
        Ok(use_service(self.services()?.exporter.as_ref()))
    }

    pub fn editor<R>(
        &self,
        use_service: impl for<'a> FnOnce(&'a dyn EditorAdapter) -> R,
    ) -> Result<R, StaleProjectSession> {
        Ok(use_service(self.services()?.editor.as_ref()))
    }

    pub fn spellcheck<R>(
        &self,
        use_service: impl for<'a> FnOnce(&'a dyn SpellcheckService) -> R,
    ) -> Result<R, StaleProjectSession> {
        Ok(use_service(self.services()?.spellcheck.as_ref()))
    }

    pub fn workspace_state<R>(
        &self,
        use_service: impl for<'a> FnOnce(&'a dyn WorkspaceStateStore) -> R,
    ) -> Result<R, StaleProjectSession> {
        Ok(use_service(self.services()?.workspace_state.as_ref()))
    }

    /// Borrows workspace persistence for an async load/save while this exact
    /// project-session access remains authorized.
    pub fn workspace_state_service(&self) -> Result<&dyn WorkspaceStateStore, StaleProjectSession> {
        Ok(self.services()?.workspace_state.as_ref())
    }

    pub fn preferences<R>(
        &self,
        use_service: impl for<'a> FnOnce(&'a dyn PreferenceService) -> R,
    ) -> Result<R, StaleProjectSession> {
        Ok(use_service(self.services()?.preferences.as_ref()))
    }

    /// Borrows preferences for an async call without cloning a service past
    /// this session capability's authorization boundary.
    pub fn preferences_service(&self) -> Result<&dyn PreferenceService, StaleProjectSession> {
        Ok(self.services()?.preferences.as_ref())
    }

    pub fn appearance<R>(
        &self,
        use_service: impl for<'a> FnOnce(&'a dyn AppearanceService) -> R,
    ) -> Result<R, StaleProjectSession> {
        Ok(use_service(self.services()?.appearance.as_ref()))
    }

    /// Borrows appearance preferences for an async call scoped to this access.
    pub fn appearance_service(&self) -> Result<&dyn AppearanceService, StaleProjectSession> {
        Ok(self.services()?.appearance.as_ref())
    }

    pub fn platform<R>(
        &self,
        use_services: impl for<'a> FnOnce(&'a PlatformServices) -> R,
    ) -> Result<R, StaleProjectSession> {
        Ok(use_services(&self.services()?.platform))
    }

    /// Borrows platform services for an async call whose future is awaited
    /// while this authorized access value remains alive.
    pub fn platform_services(&self) -> Result<&PlatformServices, StaleProjectSession> {
        Ok(&self.services()?.platform)
    }
}

/// Initial hydration data and live ports for one exact project session.
#[derive(Clone)]
pub struct ProjectUiProject {
    pub snapshot: Arc<ProjectSnapshot>,
    pub ports: ProjectUiPorts,
}

impl fmt::Debug for ProjectUiProject {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectUiProject")
            .field("snapshot", &self.snapshot)
            .field("ports", &self.ports)
            .finish()
    }
}

impl ProjectUiProject {
    pub const fn session(&self) -> ProjectSessionCapability {
        self.ports.session()
    }
}

impl PlatformServices {
    pub fn new(
        menus: Arc<dyn MenuService>,
        dialogs: Arc<dyn DialogService>,
        clipboard: Arc<dyn ClipboardService>,
        external_open: Arc<dyn ExternalOpenService>,
        application_paths: Arc<dyn ApplicationPathService>,
        system_appearance: Arc<dyn SystemAppearanceService>,
    ) -> Self {
        Self {
            credentials: None,
            session_credentials: None,
            menus,
            menu_activations: None,
            dialogs,
            clipboard,
            external_open,
            application_paths,
            system_appearance,
            system_appearance_events: None,
        }
    }

    pub fn with_credentials(mut self, credentials: Arc<dyn CredentialStore>) -> Self {
        self.credentials = Some(credentials);
        self
    }

    pub fn with_session_credentials(mut self, credentials: Arc<dyn CredentialStore>) -> Self {
        self.session_credentials = Some(credentials);
        self
    }

    pub fn with_system_appearance_events(
        mut self,
        events: Arc<dyn SystemAppearanceEventService>,
    ) -> Self {
        self.system_appearance_events = Some(events);
        self
    }

    pub fn with_menu_activations(mut self, activations: Arc<dyn MenuActivationService>) -> Self {
        self.menu_activations = Some(activations);
        self
    }
}

/// Applies every appearance event to live windows in ascending logical-ID order.
///
/// Callers retain the complete capability, including its exact generation, in
/// each callback so a native service can reject a window that changed while an
/// event was pending.
pub fn apply_appearance_events(
    snapshots: &[ThemeSnapshot],
    windows: &[WindowCapability],
    mut apply: impl FnMut(WindowCapability, ThemeSnapshot),
) {
    let mut ordered = windows.to_vec();
    ordered.sort_by_key(|window| window.window_id());
    for snapshot in snapshots {
        for window in &ordered {
            apply(*window, *snapshot);
        }
    }
}
