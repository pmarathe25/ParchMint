//! Native command-line entry point for core ParchMint operations.

use std::{
    collections::BTreeMap,
    env,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use parchmint_contracts::generated::CliOutputV1;
use parchmint_domain::{DocumentId, ProjectId};
use parchmint_export_api::{
    ExportDefaults, ExportNode, ExportRequest, ExportRunOptions, ExportSink, ExportSource,
    ExportStyleCatalog, Exporter, ProjectSnapshot as ExportProjectSnapshot, SourceRevision,
};
use parchmint_export_html::HtmlExporter;
use parchmint_history_api::{HistoryPageQuery, HistoryStore};
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
use parchmint_recovery_api::RecoveryJournal;
use parchmint_recovery_fs::FsRecoveryJournal;
use parchmint_save::{
    CheckpointCategory, CheckpointInput, CheckpointIntentHash, ProjectSaveCoordinator,
    ResourceRevision, SaveCoordinator, SaveGeneration, SavePriority, SaveRequest,
    SaveRevisionVector,
};
use parchmint_search_api::{
    SearchBatch, SearchBatchSink, SearchField, SearchIndex, SearchProjectionSource,
    SearchProjectionVisitor, SearchQuery,
};
use parchmint_search_sqlite::SqliteSearchIndex;
use serde_json::Value;
use sha2::{Digest, Sha256};

const CLI_SCHEMA: &str = "parchmint.cli-output/v1";

/// Runs the CLI with process arguments and returns its stable numeric exit code.
pub fn run_process() -> i32 {
    run_args(env::args().skip(1))
}

/// Runs the CLI with supplied arguments and returns its stable numeric exit code.
pub fn run_args(arguments: impl IntoIterator<Item = String>) -> i32 {
    let parsed = parse(arguments);
    let outcome = match parsed.command {
        Ok(_) if parsed.cancelled => Outcome::cancelled(),
        Ok(command) => execute(command),
        Err(outcome) => outcome,
    };
    emit(parsed.machine, outcome)
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
    Save(PathBuf),
    Recover(PathBuf),
    History(PathBuf),
    Search(PathBuf, String),
    Rebuild(PathBuf),
    Export(PathBuf, PathBuf),
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
        "recover" => Command::Recover(one_path(arguments)?),
        "history" => Command::History(one_path(arguments)?),
        "rebuild" => Command::Rebuild(one_path(arguments)?),
        "command" if arguments.len() == 2 => {
            Command::Apply(PathBuf::from(&arguments[0]), arguments[1].clone())
        }
        "search" if arguments.len() == 2 => {
            Command::Search(PathBuf::from(&arguments[0]), arguments[1].clone())
        }
        "export" if arguments.len() == 2 => {
            Command::Export(PathBuf::from(&arguments[0]), PathBuf::from(&arguments[1]))
        }
        _ => return Err(Outcome::usage()),
    })
}

fn execute(command: Command) -> Outcome {
    match command {
        Command::Create(path) => create(path),
        Command::Open(path) | Command::Validate(path) => open(path),
        Command::Migrate(path) => migrate(path),
        Command::Inspect(path) => inspect(path),
        Command::Apply(path, operation) => apply(path, operation),
        Command::Save(path) => save(path),
        Command::Recover(path) => recover(path),
        Command::History(path) => history(path),
        Command::Search(path, text) => search(path, text),
        Command::Rebuild(path) => rebuild(path),
        Command::Export(path, output) => export(path, output),
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

fn save(path: PathBuf) -> Outcome {
    if let Err(outcome) = open_project(&path) {
        return outcome;
    }
    let files = NativeProjectFileSystem::new();
    let (root, _lease) = match files.acquire(UntrustedProjectPath::new(path.clone())) {
        Ok(value) => value,
        Err(error) => return filesystem_outcome(error),
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
            intent_hash: CheckpointIntentHash::from_bytes(project_digest(&path)),
            resources,
            category: CheckpointCategory::ExplicitSave,
            affected_documents: Vec::new(),
            name: None,
        },
        SavePriority::Explicit,
    );
    match coordinator
        .request(request)
        .and_then(|ticket| ticket.wait())
    {
        Ok(_) => Outcome::success(),
        Err(_) => Outcome::failed(),
    }
}

fn recover(path: PathBuf) -> Outcome {
    if let Err(outcome) = open_project(&path) {
        return outcome;
    }
    match FsRecoveryJournal::open(path).and_then(|journal| journal.inspect()) {
        Ok(_) => Outcome::success(),
        Err(_) => Outcome::failed(),
    }
}

fn history(path: PathBuf) -> Outcome {
    if let Err(outcome) = open_project(&path) {
        return outcome;
    }
    let files = NativeProjectFileSystem::new();
    let (root, _lease) = match files.acquire(UntrustedProjectPath::new(path)) {
        Ok(value) => value,
        Err(error) => return filesystem_outcome(error),
    };
    let store = Git2HistoryStore::new(root);
    if store.initialize(ProjectRootCapability::new(0)).is_err() {
        return Outcome::failed();
    }
    match store.list(HistoryPageQuery::newest_first(20)) {
        Ok(_) => Outcome::success(),
        Err(_) => Outcome::failed(),
    }
}

fn search(path: PathBuf, text: String) -> Outcome {
    if text.is_empty() {
        return Outcome::usage();
    }
    if let Err(outcome) = open_project(&path) {
        return outcome;
    }
    let index = SqliteSearchIndex::new(&path);
    let source = EmptyProjectionSource;
    if index
        .open_or_rebuild(ProjectId::from_bytes(project_key(&path)), &source)
        .is_err()
    {
        return Outcome::failed();
    }
    let query = SearchQuery {
        text,
        fields: [SearchField::Body].into_iter().collect(),
        case_sensitive: false,
        whole_word: false,
        generation: 1,
    };
    match index.query(query, Box::new(DiscardSearchBatches)) {
        Ok(()) => Outcome::success(),
        Err(_) => Outcome::failed(),
    }
}

fn rebuild(path: PathBuf) -> Outcome {
    if let Err(outcome) = open_project(&path) {
        return outcome;
    }
    let index = SqliteSearchIndex::new(&path);
    let source = EmptyProjectionSource;
    match index.open_or_rebuild(ProjectId::from_bytes(project_key(&path)), &source) {
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
    match exporter.export(plan, Box::new(NativeExportSink::new(destination))) {
        Ok(_) => Outcome::success(),
        Err(_) => Outcome::failed(),
    }
}

fn open_project(path: &Path) -> Result<(), Outcome> {
    if !safe_path(path) {
        return Err(Outcome::unsafe_input());
    }
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

fn canonical_resources(
    root: &parchmint_project_fs::ProjectRootCapability,
    files: &NativeProjectFileSystem,
) -> Result<BTreeMap<CanonicalRelativePath, ContentHash>, Outcome> {
    Ok(canonical_resource_bytes(root, files)?
        .into_iter()
        .map(|(path, bytes)| {
            (
                path,
                ContentHash::from_bytes(Sha256::digest(normalize_line_endings(&bytes)).into()),
            )
        })
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

struct EmptyProjectionSource;

impl SearchProjectionSource for EmptyProjectionSource {
    fn visit_projections(
        &self,
        _: &mut dyn SearchProjectionVisitor,
    ) -> Result<(), parchmint_search_api::SearchError> {
        Ok(())
    }
}

struct DiscardSearchBatches;

impl SearchBatchSink for DiscardSearchBatches {
    fn push(&self, _: SearchBatch) {}
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

fn emit(machine: bool, outcome: Outcome) -> i32 {
    let output = CliOutputV1 {
        schema: CLI_SCHEMA.into(),
        ok: matches!(outcome, Outcome::Success),
        message: Some(outcome.message().into()),
        data: None::<Value>,
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
