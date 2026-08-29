//! Native command-line entry point for core ParchMint operations.

use std::{
    collections::BTreeMap,
    env,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use parchmint_application::{
    DocumentCommand, DocumentSnapshot, DocumentStateOwner, DocumentVisibility,
    NativeDocumentStateOwner,
};
use parchmint_contracts::generated::CliOutputV1;
use parchmint_domain::{BlockId, CheckpointId, DocumentId, ProjectId, ProjectRevision};
use parchmint_export_api::{
    ExportDefaults, ExportHandle, ExportNode, ExportRequest, ExportRunOptions, ExportSink,
    ExportSource, ExportStyleCatalog, Exporter, IgnoreExportProgress,
    ProjectSnapshot as ExportProjectSnapshot, SourceRevision,
};
use parchmint_export_html::HtmlExporter;
use parchmint_history_api::{HistoryPageQuery, HistoryStore, SnapshotName};
use parchmint_history_git2::Git2HistoryStore;
use parchmint_project_format::{
    CanonicalCodec, CanonicalRelativePath, ContentHash, FormatVersion, ProjectFormatCodec,
    ResourceId, SourceFormatSnapshot,
};
use parchmint_project_fs::{
    FsAtomicWriter, FsProjectRepository, NativeAtomicFileOps, NativeProjectFileSystem,
    ProjectFileSystem, UntrustedProjectPath,
};
use parchmint_project_repository::{
    AtomicWritePlan, AtomicWriter, CreateProject, ProjectPath, ProjectRepository,
    ProjectRootCapability, StagedResource,
};
use parchmint_recovery_api::{
    DocumentRevision, DurableRevisionVector, EditorRevisionRange, RecoveryBaseSnapshot,
    RecoveryBatch, RecoveryJournal, RecoveryRevisionVector, VersionedRecoveryPayload,
};
use parchmint_recovery_fs::FsRecoveryJournal;
use parchmint_save::{
    CheckpointCategory, CheckpointInput, CheckpointIntentHash, ProjectSaveCoordinator,
    ResourceRevision, SaveCoordinator, SaveGeneration, SavePriority, SaveRequest,
    SaveRevisionVector,
};
use parchmint_search_api::{
    RevisionId, SearchBatch, SearchBatchSink, SearchDocumentProjection, SearchField, SearchIndex,
    SearchIndexState, SearchProjectionSource, SearchProjectionVisitor, SearchQuery,
    SearchTextProjection,
};
use parchmint_search_sqlite::SqliteSearchIndex;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const CLI_SCHEMA: &str = "parchmint.cli-output/v1";
// A marked project must finish its whole-project restore before normal open.
const PENDING_RESTORE_PATH: &str = ".parchmint/pending-restore";

/// Runs the CLI with process arguments and returns its stable numeric exit code.
pub fn run_process() -> i32 {
    run_args(env::args().skip(1))
}

/// Runs the CLI with supplied arguments and returns its stable numeric exit code.
pub fn run_args(arguments: impl IntoIterator<Item = String>) -> i32 {
    let parsed = parse(arguments);
    let result = match parsed.command {
        Ok(_) if parsed.cancelled => CommandResult::from(Outcome::cancelled()),
        Ok(command) => execute(command),
        Err(outcome) => CommandResult::from(outcome),
    };
    emit(parsed.machine, result)
}

#[derive(Debug)]
struct ParsedCommand {
    machine: bool,
    cancelled: bool,
    command: Result<Command, Outcome>,
}

#[derive(Debug)]
enum Command {
    Create(PathBuf),
    Open(PathBuf),
    Validate(PathBuf),
    Migrate(PathBuf),
    Inspect(PathBuf),
    Apply(PathBuf, String),
    Edit(PathBuf, String, String),
    Terminate(PathBuf),
    Save(PathBuf),
    Recover(PathBuf),
    Checkpoint(PathBuf, String),
    Restore(PathBuf, String),
    History(PathBuf),
    Index(PathBuf),
    Search(PathBuf, String),
    Rebuild(PathBuf),
    Close(PathBuf),
    Export(PathBuf, PathBuf),
}

#[derive(Debug)]
struct CommandResult {
    outcome: Outcome,
    data: Option<Value>,
}

impl CommandResult {
    fn success(data: Value) -> Self {
        Self {
            outcome: Outcome::success(),
            data: Some(data),
        }
    }
}

impl From<Outcome> for CommandResult {
    fn from(outcome: Outcome) -> Self {
        Self {
            outcome,
            data: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(i32)]
enum Outcome {
    Success = 0,
    Failed = 1,
    Usage = 2,
    UnsafeInput = 3,
    Locked = 4,
    InvalidProject = 5,
    Cancelled = 6,
}

impl Outcome {
    const fn success() -> Self {
        Self::Success
    }

    const fn usage() -> Self {
        Self::Usage
    }

    const fn unsafe_input() -> Self {
        Self::UnsafeInput
    }

    const fn locked() -> Self {
        Self::Locked
    }

    const fn invalid_project() -> Self {
        Self::InvalidProject
    }

    const fn cancelled() -> Self {
        Self::Cancelled
    }

    const fn failed() -> Self {
        Self::Failed
    }

    const fn message(self) -> &'static str {
        match self {
            Self::Success => "operation completed",
            Self::Failed => "operation failed",
            Self::Usage => "invalid command input",
            Self::UnsafeInput => "unsafe project input",
            Self::Locked => "project is locked",
            Self::InvalidProject => "project cannot be opened",
            Self::Cancelled => "operation cancelled",
        }
    }
}

fn parse(arguments: impl IntoIterator<Item = String>) -> ParsedCommand {
    let mut machine = false;
    let mut cancelled = false;
    let mut invalid_option = false;
    let mut positional = Vec::new();
    for argument in arguments {
        match argument.as_str() {
            "--machine" => machine = true,
            "--cancel" => cancelled = true,
            option if option.starts_with('-') => invalid_option = true,
            _ => positional.push(argument),
        }
    }

    let command = if invalid_option {
        Err(Outcome::usage())
    } else {
        parse_command(&positional)
    };
    ParsedCommand {
        machine,
        cancelled,
        command,
    }
}

fn parse_command(positional: &[String]) -> Result<Command, Outcome> {
    let Some((name, arguments)) = positional.split_first() else {
        return Err(Outcome::usage());
    };
    let one_path = |arguments: &[String]| {
        arguments
            .first()
            .filter(|_| arguments.len() == 1)
            .map(PathBuf::from)
            .ok_or_else(Outcome::usage)
    };
    Ok(match name.as_str() {
        "create" => Command::Create(one_path(arguments)?),
        "open" => Command::Open(one_path(arguments)?),
        "validate" => Command::Validate(one_path(arguments)?),
        "migrate" => Command::Migrate(one_path(arguments)?),
        "inspect" => Command::Inspect(one_path(arguments)?),
        "save" => Command::Save(one_path(arguments)?),
        "terminate" => Command::Terminate(one_path(arguments)?),
        "recover" => Command::Recover(one_path(arguments)?),
        "history" => Command::History(one_path(arguments)?),
        "index" => Command::Index(one_path(arguments)?),
        "rebuild" => Command::Rebuild(one_path(arguments)?),
        "close" => Command::Close(one_path(arguments)?),
        "edit" if arguments.len() == 3 => Command::Edit(
            PathBuf::from(&arguments[0]),
            arguments[1].clone(),
            arguments[2].clone(),
        ),
        "checkpoint" if arguments.len() == 2 => {
            Command::Checkpoint(PathBuf::from(&arguments[0]), arguments[1].clone())
        }
        "restore" if arguments.len() == 2 => {
            Command::Restore(PathBuf::from(&arguments[0]), arguments[1].clone())
        }
        "command" if arguments.len() == 2 => {
            Command::Apply(PathBuf::from(&arguments[0]), arguments[1].clone())
        }
        "search" | "query" if arguments.len() == 2 => {
            Command::Search(PathBuf::from(&arguments[0]), arguments[1].clone())
        }
        "export" if arguments.len() == 2 => {
            Command::Export(PathBuf::from(&arguments[0]), PathBuf::from(&arguments[1]))
        }
        _ => return Err(Outcome::usage()),
    })
}

fn execute(command: Command) -> CommandResult {
    match command {
        Command::Create(path) => create(path).into(),
        Command::Open(path) | Command::Validate(path) => open(path).into(),
        Command::Migrate(path) => migrate(path).into(),
        Command::Inspect(path) => inspect(path).into(),
        Command::Apply(path, operation) => apply(path, operation).into(),
        Command::Edit(path, resource, body) => edit(path, resource, body).into(),
        Command::Terminate(path) => terminate(path).into(),
        Command::Save(path) => save(path, SavePriority::Explicit).into(),
        Command::Recover(path) => recover(path).into(),
        Command::Checkpoint(path, name) => checkpoint(path, name),
        Command::Restore(path, checkpoint) => restore(path, checkpoint).into(),
        Command::History(path) => history(path),
        Command::Index(path) => index(path).into(),
        Command::Search(path, text) => search(path, text),
        Command::Rebuild(path) => rebuild(path).into(),
        Command::Close(path) => save(path, SavePriority::Close).into(),
        Command::Export(path, output) => export(path, output).into(),
    }
}

fn create(path: PathBuf) -> Outcome {
    if !safe_path(&path) {
        return Outcome::unsafe_input();
    }
    let repository = FsProjectRepository::native();
    match repository.create(CreateProject::new(ProjectPath::new(path))) {
        Ok(_) => Outcome::success(),
        Err(error) => repository_outcome(error),
    }
}

fn open(path: PathBuf) -> Outcome {
    match open_project(&path) {
        Ok(()) => Outcome::success(),
        Err(outcome) => outcome,
    }
}

fn migrate(path: PathBuf) -> Outcome {
    if let Err(outcome) = open_project(&path) {
        return outcome;
    }
    let files = NativeProjectFileSystem::new();
    let (root, _lease) = match files.acquire(UntrustedProjectPath::new(path)) {
        Ok(value) => value,
        Err(error) => return filesystem_outcome(error),
    };
    let mut resources = match canonical_resource_bytes(&root, &files) {
        Ok(resources) => resources,
        Err(outcome) => return outcome,
    };
    let format_path = CanonicalRelativePath::parse(".parchmint/format-version")
        .expect("the format control path is canonical");
    let Some(format_control) = resources.remove(&format_path) else {
        return Outcome::invalid_project();
    };
    resources.remove(
        &CanonicalRelativePath::parse("deletions.json").expect("the deletion path is canonical"),
    );
    let migrated = match ProjectFormatCodec::default().migrate(
        SourceFormatSnapshot {
            format_control,
            resources,
        },
        FormatVersion::V1,
    ) {
        Ok(migrated) => migrated,
        Err(_) => return Outcome::invalid_project(),
    };
    let writes = migrated
        .resources
        .into_values()
        .map(|resource| StagedResource {
            path: resource.path.to_string(),
            bytes: resource.bytes,
        })
        .collect();
    let writer = FsAtomicWriter::new(NativeAtomicFileOps::new(root));
    let staged = match writer.stage(AtomicWritePlan::new(writes)) {
        Ok(staged) => staged,
        Err(_) => return Outcome::failed(),
    };
    if !writer.validate_staged(&staged).is_valid() {
        let _ = writer.abandon(staged);
        return Outcome::failed();
    }
    match writer.commit(staged) {
        Ok(_) => Outcome::success(),
        Err(_) => Outcome::failed(),
    }
}

fn inspect(path: PathBuf) -> Outcome {
    if let Err(outcome) = open_project(&path) {
        return outcome;
    }
    match FsRecoveryJournal::open(&path).and_then(|journal| journal.inspect()) {
        Ok(_) => Outcome::success(),
        Err(_) => Outcome::failed(),
    }
}

fn apply(path: PathBuf, operation: String) -> Outcome {
    if operation != "noop" {
        return Outcome::usage();
    }
    open(path)
}

fn edit(path: PathBuf, resource: String, text: String) -> Outcome {
    if let Err(outcome) = open_project(&path) {
        return outcome;
    }
    let relative = match CanonicalRelativePath::parse(resource) {
        Ok(relative) if is_document_resource(&relative) => relative,
        _ => return Outcome::unsafe_input(),
    };
    let current = match read_optional_resource(&path, &relative) {
        Ok(bytes) => bytes,
        Err(outcome) => return outcome,
    };
    let replacement = match canonical_document_from_text(&text) {
        Ok(bytes) => bytes,
        Err(outcome) => return outcome,
    };
    let document = DocumentId::from_bytes(project_key(Path::new(relative.as_str())));
    let owner = NativeDocumentStateOwner::new([DocumentSnapshot {
        document_id: document,
        body: String::from_utf8_lossy(&current).into_owned(),
        revision: Default::default(),
        visibility: DocumentVisibility::Open,
        comments: Vec::new(),
    }]);
    let command = DocumentCommand {
        document_id: document,
        observed_revision: Default::default(),
        body: String::from_utf8_lossy(&replacement).into_owned(),
    };
    if owner.execute(command).is_err()
        || owner.undo(document).is_err()
        || owner.redo(document).is_err()
    {
        return Outcome::failed();
    }
    let replacement = match owner.snapshot(document) {
        Ok(snapshot) => snapshot.body.into_bytes(),
        Err(_) => return Outcome::failed(),
    };

    let pending = match pending_recovery(&path) {
        Ok(pending) => pending,
        Err(outcome) => return outcome,
    };
    if pending
        .last()
        .is_some_and(|batch| !batch.documents.contains_key(&document))
    {
        return Outcome::failed();
    }
    let project_revision = pending.last().map_or(ProjectRevision::from(1), |batch| {
        batch.project_revision.next()
    });
    let document_revision = pending
        .last()
        .and_then(|batch| batch.documents.get(&document))
        .map_or(DocumentRevision::from(1), |range| range.last.next());
    let base_hash = pending
        .last()
        .and_then(|batch| batch.result_hashes.get(&ResourceId::Document))
        .copied()
        .unwrap_or_else(|| content_hash(&current));
    let result_hash = content_hash(&replacement);
    let batch = RecoveryBatch {
        project_revision,
        documents: BTreeMap::from([(
            document,
            EditorRevisionRange::new(document_revision, document_revision)
                .expect("one revision is a valid recovery range"),
        )]),
        base_hashes: BTreeMap::from([(ResourceId::Document, base_hash)]),
        result_hashes: BTreeMap::from([(ResourceId::Document, result_hash)]),
        payload: VersionedRecoveryPayload::V1(parchmint_contracts::generated::RecoveryRecordV1 {
            schema: "parchmint.recovery-record/v1".into(),
            record_id: encode_hex(&Sha256::digest(&replacement)),
            operations: vec![json!({
                "kind": "replace-document",
                "path": relative.as_str(),
                "body": String::from_utf8_lossy(&replacement),
            })],
        }),
    };
    match FsRecoveryJournal::open(path).and_then(|journal| journal.append(batch)) {
        Ok(_) => Outcome::success(),
        Err(_) => Outcome::failed(),
    }
}

fn terminate(path: PathBuf) -> Outcome {
    if let Err(outcome) = open_project(&path) {
        return outcome;
    }
    match FsRecoveryJournal::open(path).and_then(|journal| journal.inspect()) {
        Ok(inventory) if !inventory.records.is_empty() => Outcome::success(),
        Ok(_) | Err(_) => Outcome::failed(),
    }
}

fn save(path: PathBuf, priority: SavePriority) -> Outcome {
    if let Err(outcome) = open_project(&path) {
        return outcome;
    }
    let (files, root, _lease) = match acquire_project_root(&path) {
        Ok(value) => value,
        Err(outcome) => return outcome,
    };
    let manifest_path =
        CanonicalRelativePath::parse("project.toml").expect("the manifest path is canonical");
    let manifest = match files.read(&root, &manifest_path) {
        Ok(bytes) => bytes,
        Err(error) => return filesystem_outcome(error),
    };
    let resources = match canonical_resources(&root, &files) {
        Ok(resources) => resources,
        Err(outcome) => return outcome,
    };
    let history = Arc::new(Git2HistoryStore::new(root.clone()));
    if history.initialize(ProjectRootCapability::new(0)).is_err() {
        return Outcome::failed();
    }
    let intents = match FsRecoveryJournal::open(&path) {
        Ok(journal) => Arc::new(journal),
        Err(_) => return Outcome::failed(),
    };
    let writer = Arc::new(FsAtomicWriter::new(NativeAtomicFileOps::new(root)));
    let coordinator = match ProjectSaveCoordinator::new(
        ProjectId::from_bytes(project_key(&path)),
        writer,
        history,
        intents,
    ) {
        Ok(coordinator) => coordinator,
        Err(_) => return Outcome::failed(),
    };
    let digest: [u8; 32] = Sha256::digest(&manifest).into();
    let intent_hash = checkpoint_intent_hash(&resources, b"save");
    let request = SaveRequest::new(
        SaveRevisionVector {
            project_revision: 0.into(),
            open_documents: BTreeMap::new(),
            closed_resources: BTreeMap::from([(ResourceId::Manifest, ResourceRevision::from(0))]),
            canonical_hashes: BTreeMap::from([(
                ResourceId::Manifest,
                ContentHash::from_bytes(digest),
            )]),
            generation: SaveGeneration::from(1),
        },
        AtomicWritePlan::new(vec![StagedResource {
            path: "project.toml".into(),
            bytes: manifest,
        }]),
        CheckpointInput {
            intent_hash,
            resources,
            category: CheckpointCategory::ExplicitSave,
            affected_documents: Vec::new(),
            name: None,
            recorded_at_unix_millis: current_unix_millis(),
        },
        priority,
    );
    if coordinator.reconcile_open().is_err() {
        return Outcome::failed();
    }
    match coordinator
        .request(request)
        .and_then(|ticket| ticket.wait())
    {
        Ok(_) => match FsRecoveryJournal::open(&path).and_then(|journal| {
            let inventory = journal.inspect()?;
            match inventory.durable_through {
                Some(revisions) => journal
                    .compact(DurableRevisionVector::new(revisions))
                    .map(|_| ()),
                None => Ok(()),
            }
        }) {
            Ok(()) => Outcome::success(),
            Err(_) => Outcome::failed(),
        },
        Err(_) => Outcome::failed(),
    }
}

fn recover(path: PathBuf) -> Outcome {
    if let Err(outcome) = open_project(&path) {
        return outcome;
    }
    let pending = match pending_recovery(&path) {
        Ok(pending) => pending,
        Err(outcome) => return outcome,
    };
    if pending.is_empty() {
        return Outcome::success();
    }
    let mut replacements = BTreeMap::new();
    for batch in pending {
        let VersionedRecoveryPayload::V1(payload) = batch.payload;
        for operation in payload.operations {
            let Some(fields) = operation.as_object() else {
                return Outcome::failed();
            };
            if fields.get("kind").and_then(Value::as_str) != Some("replace-document") {
                return Outcome::failed();
            }
            let Some(relative) = fields
                .get("path")
                .and_then(Value::as_str)
                .and_then(|path| CanonicalRelativePath::parse(path).ok())
                .filter(is_document_resource)
            else {
                return Outcome::failed();
            };
            let Some(body) = fields.get("body").and_then(Value::as_str) else {
                return Outcome::failed();
            };
            let canonical = match ProjectFormatCodec::default().decode_document(body.as_bytes()) {
                Ok(document) => document.as_html().as_bytes().to_vec(),
                Err(_) => return Outcome::failed(),
            };
            replacements.insert(relative, canonical);
        }
    }
    let writes = replacements
        .into_iter()
        .map(|(path, bytes)| StagedResource {
            path: path.to_string(),
            bytes,
        })
        .collect();
    atomic_write(&path, AtomicWritePlan::new(writes))
}

fn checkpoint(path: PathBuf, name: String) -> CommandResult {
    if let Err(outcome) = open_project(&path) {
        return outcome.into();
    }
    let snapshot_name = match SnapshotName::new(name) {
        Ok(name) => name,
        Err(_) => return Outcome::usage().into(),
    };
    let (files, root, _lease) = match acquire_project_root(&path) {
        Ok(value) => value,
        Err(outcome) => return outcome.into(),
    };
    let resources = match canonical_resources(&root, &files) {
        Ok(resources) => resources,
        Err(outcome) => return outcome.into(),
    };
    let store = Git2HistoryStore::new(root);
    if store.initialize(ProjectRootCapability::new(0)).is_err() {
        return Outcome::failed().into();
    }
    let input = CheckpointInput {
        intent_hash: checkpoint_intent_hash(&resources, snapshot_name.as_str().as_bytes()),
        resources,
        category: CheckpointCategory::NamedSnapshot,
        affected_documents: Vec::new(),
        name: Some(snapshot_name),
        recorded_at_unix_millis: current_unix_millis(),
    };
    match store.checkpoint(input) {
        Ok(checkpoint) => CommandResult::success(json!({
            "checkpoint_id": encode_hex(checkpoint.as_bytes()),
        })),
        Err(_) => Outcome::failed().into(),
    }
}

fn restore(path: PathBuf, encoded_checkpoint: String) -> Outcome {
    if let Err(outcome) = open_project(&path) {
        return outcome;
    }
    let checkpoint = match decode_hex::<16>(&encoded_checkpoint) {
        Some(bytes) => CheckpointId::from_bytes(bytes),
        None => return Outcome::usage(),
    };
    let (files, root, _lease) = match acquire_project_root(&path) {
        Ok(value) => value,
        Err(outcome) => return outcome,
    };
    let store = Git2HistoryStore::new(root.clone());
    if store.initialize(ProjectRootCapability::new(0)).is_err() {
        return Outcome::failed();
    }
    if store.preview(checkpoint).is_err() {
        return Outcome::failed();
    }
    if write_pending_restore(&root, &encoded_checkpoint).is_err() {
        return Outcome::failed();
    }
    finish_pending_restore(&root, &files, checkpoint, &encoded_checkpoint)
}

fn history(path: PathBuf) -> CommandResult {
    if let Err(outcome) = open_project(&path) {
        return outcome.into();
    }
    let (_files, root, _lease) = match acquire_project_root(&path) {
        Ok(value) => value,
        Err(outcome) => return outcome.into(),
    };
    let store = Git2HistoryStore::new(root);
    if store.initialize(ProjectRootCapability::new(0)).is_err() {
        return Outcome::failed().into();
    }
    match store.list(HistoryPageQuery::newest_first(20)) {
        Ok(page) => CommandResult::success(json!({
            "checkpoint_count": page.checkpoints.len(),
        })),
        Err(_) => Outcome::failed().into(),
    }
}

fn index(path: PathBuf) -> Outcome {
    if let Err(outcome) = open_project(&path) {
        return outcome;
    }
    search_index(&path).map_or_else(|outcome| outcome, |_| Outcome::success())
}

fn search(path: PathBuf, text: String) -> CommandResult {
    if text.is_empty() {
        return Outcome::usage().into();
    }
    if let Err(outcome) = open_project(&path) {
        return outcome.into();
    }
    let (index, _, _) = match search_index(&path) {
        Ok(services) => services,
        Err(outcome) => return outcome.into(),
    };
    let query = SearchQuery {
        text,
        fields: [SearchField::Body].into_iter().collect(),
        case_sensitive: false,
        whole_word: false,
        generation: 1,
    };
    let sink = CountSearchHits::default();
    match index.query(query, Box::new(sink.clone())) {
        Ok(()) => CommandResult::success(json!({
            "hit_count": sink.hit_count(),
        })),
        Err(_) => Outcome::failed().into(),
    }
}

fn rebuild(path: PathBuf) -> Outcome {
    if let Err(outcome) = open_project(&path) {
        return outcome;
    }
    let (index, source, rebuilt) = match search_index(&path) {
        Ok(services) => services,
        Err(outcome) => return outcome,
    };
    if rebuilt {
        return Outcome::success();
    }
    match index.rebuild(&source) {
        Ok(_) => Outcome::success(),
        Err(_) => Outcome::failed(),
    }
}

fn export(path: PathBuf, output: PathBuf) -> Outcome {
    if let Err(outcome) = open_project(&path) {
        return outcome;
    }
    let (target, destination) = match export_destination(&path, &output) {
        Some(value) => value,
        None => return Outcome::unsafe_input(),
    };
    let snapshot = match export_snapshot(&path) {
        Ok(snapshot) => snapshot,
        Err(_) => return Outcome::failed(),
    };
    let exporter = HtmlExporter;
    let plan = match exporter.plan(
        ExportRequest::new(target, ExportRunOptions::default()),
        &snapshot,
    ) {
        Ok(plan) => plan,
        Err(_) => return Outcome::failed(),
    };
    match exporter.export(
        plan,
        Box::new(NativeExportSink::new(destination)),
        ExportHandle::new(),
        Arc::new(IgnoreExportProgress),
    ) {
        Ok(_) => Outcome::success(),
        Err(_) => Outcome::failed(),
    }
}

fn open_project(path: &Path) -> Result<(), Outcome> {
    if !safe_path(path) {
        return Err(Outcome::unsafe_input());
    }
    reconcile_pending_restore(path)?;
    let repository = FsProjectRepository::native();
    repository
        .open(ProjectPath::new(path))
        .map(|_| ())
        .map_err(repository_outcome)
}

fn repository_outcome(error: parchmint_project_repository::RepositoryError) -> Outcome {
    match error {
        parchmint_project_repository::RepositoryError::UnsafePath { .. } => Outcome::unsafe_input(),
        parchmint_project_repository::RepositoryError::Locked { .. } => Outcome::locked(),
        parchmint_project_repository::RepositoryError::Missing { .. }
        | parchmint_project_repository::RepositoryError::MissingResource { .. }
        | parchmint_project_repository::RepositoryError::Integrity { .. }
        | parchmint_project_repository::RepositoryError::Interrupted { .. }
        | parchmint_project_repository::RepositoryError::NotFound { .. } => {
            Outcome::invalid_project()
        }
    }
}

fn filesystem_outcome(error: parchmint_project_fs::FsError) -> Outcome {
    match error {
        parchmint_project_fs::FsError::UnsafePath { .. } => Outcome::unsafe_input(),
        parchmint_project_fs::FsError::Locked { .. } => Outcome::locked(),
        parchmint_project_fs::FsError::Missing { .. }
        | parchmint_project_fs::FsError::Corrupt { .. }
        | parchmint_project_fs::FsError::AlreadyExists { .. }
        | parchmint_project_fs::FsError::NotLockOwner { .. }
        | parchmint_project_fs::FsError::Io { .. } => Outcome::invalid_project(),
        parchmint_project_fs::FsError::Injected { .. } => Outcome::failed(),
    }
}

fn acquire_project_root(
    path: &Path,
) -> Result<
    (
        NativeProjectFileSystem,
        parchmint_project_fs::ProjectRootCapability,
        parchmint_project_fs::ProjectLockLease,
    ),
    Outcome,
> {
    let files = NativeProjectFileSystem::new();
    let (root, lease) = files
        .acquire(UntrustedProjectPath::new(path))
        .map_err(filesystem_outcome)?;
    Ok((files, root, lease))
}

fn safe_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
}

fn project_digest(path: &Path) -> [u8; 32] {
    Sha256::digest(path.as_os_str().as_encoded_bytes()).into()
}

fn project_key(path: &Path) -> [u8; 16] {
    let digest = project_digest(path);
    digest[..16].try_into().expect("digest has a fixed length")
}

fn content_hash(bytes: &[u8]) -> ContentHash {
    ContentHash::from_bytes(Sha256::digest(normalize_line_endings(bytes)).into())
}

fn checkpoint_intent_hash(
    resources: &BTreeMap<CanonicalRelativePath, ContentHash>,
    salt: &[u8],
) -> CheckpointIntentHash {
    let mut digest = Sha256::new();
    digest.update(salt);
    for (path, hash) in resources {
        digest.update(path.as_str().as_bytes());
        digest.update([0]);
        digest.update(hash.as_bytes());
    }
    CheckpointIntentHash::from_bytes(digest.finalize().into())
}

fn current_unix_millis() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[usize::from(byte >> 4)] as char);
        encoded.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    encoded
}

fn decode_hex<const N: usize>(encoded: &str) -> Option<[u8; N]> {
    if encoded.len() != N * 2 {
        return None;
    }
    let mut decoded = [0_u8; N];
    for (index, pair) in encoded.as_bytes().chunks_exact(2).enumerate() {
        let high = (pair[0] as char).to_digit(16)? as u8;
        let low = (pair[1] as char).to_digit(16)? as u8;
        decoded[index] = (high << 4) | low;
    }
    Some(decoded)
}

fn canonical_document_from_text(text: &str) -> Result<Vec<u8>, Outcome> {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            character => escaped.push(character),
        }
    }
    let html = format!("<p data-block-id=\"cli-body\">{escaped}</p>");
    ProjectFormatCodec::default()
        .decode_document(html.as_bytes())
        .map(|document| document.as_html().as_bytes().to_vec())
        .map_err(|_| Outcome::failed())
}

fn searchable_document_text(html: &str) -> String {
    let mut visible = String::with_capacity(html.len());
    let mut in_tag = false;
    for character in html.chars() {
        match character {
            '<' => {
                in_tag = true;
                if !visible.chars().last().is_some_and(char::is_whitespace) {
                    visible.push(' ');
                }
            }
            '>' => in_tag = false,
            character if !in_tag => visible.push(character),
            _ => {}
        }
    }
    visible
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn read_optional_resource(
    project: &Path,
    relative: &CanonicalRelativePath,
) -> Result<Vec<u8>, Outcome> {
    let files = NativeProjectFileSystem::new();
    let (root, _lease) = files
        .acquire(UntrustedProjectPath::new(project))
        .map_err(filesystem_outcome)?;
    match files.read(&root, relative) {
        Ok(bytes) => Ok(bytes),
        Err(parchmint_project_fs::FsError::Missing { .. }) => Ok(Vec::new()),
        Err(error) => Err(filesystem_outcome(error)),
    }
}

fn atomic_write(project: &Path, plan: AtomicWritePlan) -> Outcome {
    if plan.writes.is_empty() {
        return Outcome::success();
    }
    let files = NativeProjectFileSystem::new();
    let (root, _lease) = match files.acquire(UntrustedProjectPath::new(project)) {
        Ok(value) => value,
        Err(error) => return filesystem_outcome(error),
    };
    let writer = FsAtomicWriter::new(NativeAtomicFileOps::new(root));
    let staged = match writer.stage(plan) {
        Ok(staged) => staged,
        Err(_) => return Outcome::failed(),
    };
    if !writer.validate_staged(&staged).is_valid() {
        let _ = writer.abandon(staged);
        return Outcome::failed();
    }
    match writer.commit(staged) {
        Ok(_) => Outcome::success(),
        Err(_) => Outcome::failed(),
    }
}

fn reconcile_pending_restore(project: &Path) -> Result<(), Outcome> {
    let marker = project.join(PENDING_RESTORE_PATH);
    match fs::symlink_metadata(&marker) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {}
        Ok(_) | Err(_) => return Err(Outcome::failed()),
    }

    let files = NativeProjectFileSystem::new();
    let (root, _lease) = files
        .acquire(UntrustedProjectPath::new(project))
        .map_err(filesystem_outcome)?;
    let marker_path = CanonicalRelativePath::parse(PENDING_RESTORE_PATH)
        .expect("the pending restore path is canonical");
    let marker = files
        .read(&root, &marker_path)
        .map_err(filesystem_outcome)?;
    let encoded = std::str::from_utf8(&marker)
        .ok()
        .map(str::trim)
        .ok_or_else(Outcome::failed)?;
    let checkpoint = decode_hex::<16>(encoded)
        .map(CheckpointId::from_bytes)
        .ok_or_else(Outcome::failed)?;
    match finish_pending_restore(&root, &files, checkpoint, encoded) {
        Outcome::Success => Ok(()),
        _ => Err(Outcome::failed()),
    }
}

fn write_pending_restore(
    root: &parchmint_project_fs::ProjectRootCapability,
    encoded_checkpoint: &str,
) -> Result<(), Outcome> {
    let marker = root
        .checked_path()
        .map_err(filesystem_outcome)?
        .join(PENDING_RESTORE_PATH);
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&marker)
        .map_err(|_| Outcome::failed())?;
    if writeln!(file, "{encoded_checkpoint}").is_err() || file.sync_all().is_err() {
        drop(file);
        let _ = fs::remove_file(&marker);
        let _ = sync_directory(marker.parent().expect("restore marker has a parent"));
        return Err(Outcome::failed());
    }
    sync_directory(marker.parent().expect("restore marker has a parent"))
}

fn finish_pending_restore(
    root: &parchmint_project_fs::ProjectRootCapability,
    files: &NativeProjectFileSystem,
    checkpoint: CheckpointId,
    encoded_checkpoint: &str,
) -> Outcome {
    let writer = FsAtomicWriter::new(NativeAtomicFileOps::new(root.clone()));
    let records = match files.transaction_records(root) {
        Ok(records) => records,
        Err(_) => return Outcome::failed(),
    };
    if records
        .into_iter()
        .any(|record| writer.reconcile(record).is_err())
    {
        return Outcome::failed();
    }

    let store = Git2HistoryStore::new(root.clone());
    if store.initialize(ProjectRootCapability::new(0)).is_err() {
        return Outcome::failed();
    }
    let plan = match store.restore(checkpoint) {
        Ok(plan) => plan,
        Err(_) => return Outcome::failed(),
    };
    let staged = match writer.stage(plan.writes().clone()) {
        Ok(staged) => staged,
        Err(_) => return Outcome::failed(),
    };
    if !writer.validate_staged(&staged).is_valid() {
        let _ = writer.abandon(staged);
        return Outcome::failed();
    }

    let root_path = match root.checked_path() {
        Ok(path) => path,
        Err(_) => {
            let _ = writer.abandon(staged);
            return Outcome::failed();
        }
    };
    let mut deletions = Vec::new();
    for deletion in plan.deletions() {
        let target = root_path.join(deletion.as_str());
        match fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                deletions.push(target);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) | Err(_) => {
                let _ = writer.abandon(staged);
                return Outcome::failed();
            }
        }
    }
    for target in deletions {
        let parent = target.parent().expect("canonical resource has a parent");
        if fs::remove_file(&target).is_err() || sync_directory(parent).is_err() {
            let _ = writer.abandon(staged);
            return Outcome::failed();
        }
    }
    if writer.commit(staged).is_err() {
        return Outcome::failed();
    }

    let resources = match canonical_resources(root, files) {
        Ok(resources) if resources == *plan.resources() => resources,
        Ok(_) | Err(_) => return Outcome::failed(),
    };
    if store
        .checkpoint(CheckpointInput {
            intent_hash: checkpoint_intent_hash(&resources, encoded_checkpoint.as_bytes()),
            resources,
            category: CheckpointCategory::Restoration,
            affected_documents: Vec::new(),
            name: None,
            recorded_at_unix_millis: current_unix_millis(),
        })
        .is_err()
    {
        return Outcome::failed();
    }
    match clear_pending_restore(root) {
        Ok(()) => Outcome::success(),
        Err(outcome) => outcome,
    }
}

fn clear_pending_restore(
    root: &parchmint_project_fs::ProjectRootCapability,
) -> Result<(), Outcome> {
    let marker = root
        .checked_path()
        .map_err(filesystem_outcome)?
        .join(PENDING_RESTORE_PATH);
    fs::remove_file(&marker).map_err(|_| Outcome::failed())?;
    sync_directory(marker.parent().expect("restore marker has a parent"))
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), Outcome> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| Outcome::failed())
}

#[cfg(not(unix))]
fn sync_directory(path: &Path) -> Result<(), Outcome> {
    fs::metadata(path)
        .map(|_| ())
        .map_err(|_| Outcome::failed())
}

fn pending_recovery(project: &Path) -> Result<Vec<RecoveryBatch>, Outcome> {
    let journal = FsRecoveryJournal::open(project).map_err(|_| Outcome::failed())?;
    let inventory = journal.inspect().map_err(|_| Outcome::failed())?;
    if inventory.records.is_empty() {
        return Ok(Vec::new());
    }

    let mut base_hashes = vec![content_hash(&[])];
    let (files, root, lease) = acquire_project_root(project)?;
    for (path, bytes) in canonical_resource_bytes(&root, &files)? {
        if is_document_resource(&path) {
            let hash = content_hash(&bytes);
            if !base_hashes.contains(&hash) {
                base_hashes.push(hash);
            }
        }
    }
    drop(lease);

    for hash in base_hashes {
        let replay = journal
            .replay(RecoveryBaseSnapshot {
                revisions: RecoveryRevisionVector::new(ProjectRevision::default(), BTreeMap::new()),
                hashes: BTreeMap::from([(ResourceId::Document, hash)]),
            })
            .map_err(|_| Outcome::failed())?;
        if replay.isolation.is_none() && replay.accepted.len() == inventory.records.len() {
            return Ok(replay.accepted);
        }
    }
    Err(Outcome::failed())
}

fn canonical_resources(
    root: &parchmint_project_fs::ProjectRootCapability,
    files: &NativeProjectFileSystem,
) -> Result<BTreeMap<CanonicalRelativePath, ContentHash>, Outcome> {
    Ok(canonical_resource_bytes(root, files)?
        .into_iter()
        .map(|(path, bytes)| (path, content_hash(&bytes)))
        .collect())
}

fn canonical_resource_bytes(
    root: &parchmint_project_fs::ProjectRootCapability,
    files: &NativeProjectFileSystem,
) -> Result<BTreeMap<CanonicalRelativePath, Vec<u8>>, Outcome> {
    let root_path = root.checked_path().map_err(filesystem_outcome)?;
    let paths = canonical_resource_paths(root_path)?;
    paths
        .into_iter()
        .map(|path| {
            files
                .read(root, &path)
                .map(|bytes| (path, bytes))
                .map_err(filesystem_outcome)
        })
        .collect()
}

fn canonical_resource_paths(root_path: &Path) -> Result<Vec<CanonicalRelativePath>, Outcome> {
    let mut paths = Vec::new();
    for relative in [
        ".parchmint/format-version",
        "project.toml",
        "styles.css",
        "dictionary.txt",
        "deletions.json",
    ] {
        let path = root_path.join(relative);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                paths
                    .push(CanonicalRelativePath::parse(relative).expect("fixed path is canonical"));
            }
            Ok(_) => return Err(Outcome::failed()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(Outcome::failed()),
        }
    }
    for directory in ["manuscript", "research", "annotations"] {
        collect_canonical_paths(root_path, &root_path.join(directory), &mut paths)?;
    }

    paths.sort();
    Ok(paths)
}

fn collect_canonical_paths(
    root: &Path,
    directory: &Path,
    paths: &mut Vec<CanonicalRelativePath>,
) -> Result<(), Outcome> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(Outcome::failed()),
    };
    for entry in entries {
        let entry = entry.map_err(|_| Outcome::failed())?;
        let kind = entry.file_type().map_err(|_| Outcome::failed())?;
        if kind.is_symlink() {
            return Err(Outcome::failed());
        }
        if kind.is_dir() {
            collect_canonical_paths(root, &entry.path(), paths)?;
            continue;
        }
        if !kind.is_file() {
            return Err(Outcome::failed());
        }
        let relative = entry
            .path()
            .strip_prefix(root)
            .ok()
            .and_then(|path| path.to_str())
            .map(|path| path.replace('\\', "/"))
            .and_then(|path| CanonicalRelativePath::parse(path).ok())
            .filter(is_canonical_resource)
            .ok_or_else(Outcome::failed)?;
        paths.push(relative);
    }
    Ok(())
}

fn is_canonical_resource(path: &CanonicalRelativePath) -> bool {
    let path = path.as_str();
    (path.starts_with("manuscript/") || path.starts_with("research/")) && path.ends_with(".html")
        || path.starts_with("annotations/") && path.ends_with(".json")
}

fn is_document_resource(path: &CanonicalRelativePath) -> bool {
    let path = path.as_str();
    (path.starts_with("manuscript/") || path.starts_with("research/")) && path.ends_with(".html")
}

fn normalize_line_endings(bytes: &[u8]) -> Vec<u8> {
    if !bytes.windows(2).any(|window| window == b"\r\n") {
        return bytes.to_vec();
    }
    let mut normalized = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes.get(index..index + 2) == Some(b"\r\n") {
            normalized.push(b'\n');
            index += 2;
        } else {
            normalized.push(bytes[index]);
            index += 1;
        }
    }
    normalized
}

fn export_destination(project: &Path, output: &Path) -> Option<(String, PathBuf)> {
    let destination = if output.is_absolute() {
        output.to_path_buf()
    } else {
        project.join(output)
    };
    let target = destination.strip_prefix(project).ok()?;
    if !safe_path(target) {
        return None;
    }
    Some((target.to_str()?.replace('\\', "/"), destination))
}

fn export_snapshot(path: &Path) -> Result<ExportProjectSnapshot, std::io::Error> {
    let manuscript = path.join("manuscript");
    let mut nodes = Vec::new();
    let mut sources = BTreeMap::new();
    if manuscript.is_dir() {
        for entry in fs::read_dir(manuscript)? {
            let entry = entry?;
            let entry_path = entry.path();
            if entry_path
                .extension()
                .and_then(|extension| extension.to_str())
                != Some("html")
            {
                continue;
            }
            let Some(stem) = entry_path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let id = DocumentId::from_bytes(project_key(Path::new(stem)));
            let body = fs::read_to_string(&entry_path)?;
            nodes.push(ExportNode::document(id, stem, Default::default()));
            sources.insert(
                id,
                ExportSource {
                    revision: SourceRevision::from(0),
                    body,
                },
            );
        }
    }
    Ok(ExportProjectSnapshot::new(
        ExportStyleCatalog::default(),
        ExportDefaults::default(),
        nodes,
        sources,
    ))
}

struct CanonicalSearchSource(Vec<SearchDocumentProjection>);

impl CanonicalSearchSource {
    fn load(project: &Path) -> Result<Self, Outcome> {
        let files = NativeProjectFileSystem::new();
        let (root, _lease) = files
            .acquire(UntrustedProjectPath::new(project))
            .map_err(filesystem_outcome)?;
        let resources = canonical_resource_bytes(&root, &files)?;
        let codec = ProjectFormatCodec::default();
        let mut projections = Vec::new();
        for (path, bytes) in resources {
            if !is_document_resource(&path) {
                continue;
            }
            let document = codec
                .decode_document(&bytes)
                .map_err(|_| Outcome::invalid_project())?;
            let id = project_key(Path::new(path.as_str()));
            projections.push(SearchDocumentProjection {
                document_id: DocumentId::from_bytes(id),
                revision: RevisionId::from(1),
                texts: vec![SearchTextProjection {
                    block_id: BlockId::from_bytes(id),
                    field: SearchField::Body,
                    text: searchable_document_text(document.as_html()),
                }],
            });
        }
        Ok(Self(projections))
    }
}

impl SearchProjectionSource for CanonicalSearchSource {
    fn visit_projections(
        &self,
        visitor: &mut dyn SearchProjectionVisitor,
    ) -> Result<(), parchmint_search_api::SearchError> {
        for projection in &self.0 {
            visitor.visit(projection.clone())?;
        }
        Ok(())
    }
}

#[derive(Clone, Default)]
struct CountSearchHits(Arc<AtomicUsize>);

impl CountSearchHits {
    fn hit_count(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }
}

impl SearchBatchSink for CountSearchHits {
    fn push(&self, batch: SearchBatch) {
        self.0.fetch_add(batch.hits.len(), Ordering::Relaxed);
    }
}

fn search_index(
    project: &Path,
) -> Result<(SqliteSearchIndex, CanonicalSearchSource, bool), Outcome> {
    let source = CanonicalSearchSource::load(project)?;
    let index = SqliteSearchIndex::new(project);
    let state = index
        .open_or_rebuild(ProjectId::from_bytes(project_key(project)), &source)
        .map_err(|_| Outcome::failed())?;
    Ok((
        index,
        source,
        matches!(state, SearchIndexState::Rebuilt { .. }),
    ))
}

struct NativeExportSink {
    destination: PathBuf,
    temporary: Option<PathBuf>,
    file: Option<File>,
}

impl NativeExportSink {
    fn new(destination: PathBuf) -> Self {
        Self {
            destination,
            temporary: None,
            file: None,
        }
    }
}

impl ExportSink for NativeExportSink {
    fn start(
        &mut self,
        _: &parchmint_export_api::ExportTargetCapability,
    ) -> Result<(), parchmint_export_api::ExportError> {
        let parent = self.destination.parent().ok_or_else(export_sink_error)?;
        fs::create_dir_all(parent).map_err(|_| export_sink_error())?;
        let temporary = self.destination.with_extension("parchmint-export.tmp");
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|_| export_sink_error())?;
        self.temporary = Some(temporary);
        self.file = Some(file);
        Ok(())
    }

    fn write_chunk(&mut self, bytes: &[u8]) -> Result<(), parchmint_export_api::ExportError> {
        self.file
            .as_mut()
            .ok_or_else(export_sink_error)?
            .write_all(bytes)
            .map_err(|_| export_sink_error())
    }

    fn finish(&mut self) -> Result<(), parchmint_export_api::ExportError> {
        let Some(mut file) = self.file.take() else {
            return Err(export_sink_error());
        };
        file.flush().map_err(|_| export_sink_error())?;
        file.sync_all().map_err(|_| export_sink_error())?;
        let temporary = self.temporary.take().ok_or_else(export_sink_error)?;
        fs::rename(temporary, &self.destination).map_err(|_| export_sink_error())
    }

    fn abort(&mut self) {
        self.file.take();
        if let Some(temporary) = self.temporary.take() {
            let _ = fs::remove_file(temporary);
        }
    }
}

fn export_sink_error() -> parchmint_export_api::ExportError {
    parchmint_export_api::ExportError::Sink {
        operation: "write export",
        reason: "native destination failed".into(),
    }
}

fn emit(machine: bool, result: CommandResult) -> i32 {
    let outcome = result.outcome;
    let output = CliOutputV1 {
        schema: CLI_SCHEMA.into(),
        ok: matches!(outcome, Outcome::Success),
        message: Some(outcome.message().into()),
        data: result.data,
    };
    if machine {
        println!(
            "{}",
            serde_json::to_string(&output).expect("CLI output is always serializable")
        );
    } else if matches!(outcome, Outcome::Success) {
        println!("{}", outcome.message());
    } else {
        eprintln!("{}", outcome.message());
    }
    outcome as i32
}
