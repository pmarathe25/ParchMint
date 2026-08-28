//! Embedded, app-managed Git History for canonical ParchMint project files.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    str,
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicU64, Ordering},
    },
};

use git2::{
    Commit, ErrorCode, IndexEntry, IndexTime, ObjectType, Oid, Repository, RepositoryInitOptions,
    Signature, Time,
};
use parchmint_history_api::{
    CheckpointCategory, CheckpointId, CheckpointInput, CheckpointIntentHash, CheckpointResource,
    CheckpointSummary, HistoryCursor, HistoryError, HistoryIntegrityReport, HistoryPage,
    HistoryPageQuery, HistoryReinitializeAvailability, HistoryReinitializeReport, HistoryState,
    HistoryStore, MaintenanceBudget, MaintenanceReport, RestorePlan, SnapshotName, SnapshotPreview,
    SnapshotResourcePaths,
};
use parchmint_project_format::{CanonicalRelativePath, ContentHash};
use parchmint_project_fs::{
    NativeProjectFileSystem, ProjectFileSystem, ProjectRootCapability as NativeProjectRoot,
};
use parchmint_project_repository::{AtomicWritePlan, ProjectRootCapability, StagedResource};

const MAIN_REF: &str = "refs/heads/main";
const FORMAT_MARKER: &str = "parchmint.historyFormat";
const HISTORY_ID: &str = "parchmint.historyId";
const MESSAGE_HEADER: &str = "ParchMint-History/1";
const CURSOR_HEADER: &str = "pmh1";

type RepositoryGate = Arc<Mutex<()>>;
type GateRegistry = BTreeMap<PathBuf, Weak<Mutex<()>>>;

static REPOSITORY_GATES: OnceLock<Mutex<GateRegistry>> = OnceLock::new();
static NEXT_PACK_VERIFICATION: AtomicU64 = AtomicU64::new(0);

/// A Git-backed History store scoped to one locked native project root.
///
/// Repositories are opened for each operation because `git2::Repository` is
/// not `Sync`. A process-wide per-root gate additionally keeps independently
/// constructed stores on the same project linear.
pub struct Git2HistoryStore {
    root: NativeProjectRoot,
    gate: OnceLock<RepositoryGate>,
}

impl Git2HistoryStore {
    pub const fn new(root: NativeProjectRoot) -> Self {
        Self {
            root,
            gate: OnceLock::new(),
        }
    }

    fn run_serialized<T>(
        &self,
        operation: &'static str,
        action: impl FnOnce(&Path) -> Result<T, HistoryError>,
    ) -> Result<T, HistoryError> {
        let path = self.checked_root(operation)?;
        let gate = self.gate.get_or_init(|| repository_gate(&path));
        let _guard = gate.lock().map_err(|_| HistoryError::Storage {
            operation,
            reason: "repository operation lock was poisoned".into(),
        })?;
        let path = self.checked_root(operation)?;
        action(&path)
    }

    fn checked_root(&self, operation: &'static str) -> Result<PathBuf, HistoryError> {
        self.root
            .checked_path()
            .map(Path::to_path_buf)
            .map_err(|error| HistoryError::Storage {
                operation,
                reason: error.to_string(),
            })
    }
}

impl HistoryStore for Git2HistoryStore {
    fn initialize(&self, project: ProjectRootCapability) -> Result<HistoryState, HistoryError> {
        self.run_serialized("initialize", |root| {
            let git_dir = root.join(".git");
            let repository = if git_dir.exists() {
                reject_symlink(&git_dir, "Git directory")?;
                recover_stale_locks(&git_dir)?;
                let repository = Repository::open(root)
                    .map_err(|error| corrupt_git("open embedded repository", error))?;
                validate_repository(&repository, root)?;
                repository
            } else {
                initialize_new_repository(root)?
            };

            configure_portability(&repository)?;
            validate_repository(&repository, root)?;
            let checkpoint_count = history_records(&repository)?.len();
            Ok(HistoryState {
                project,
                checkpoint_count,
            })
        })
    }

    fn reinitialize_availability(&self) -> Result<HistoryReinitializeAvailability, HistoryError> {
        self.run_serialized(
            "inspect History reinitialization",
            reinitialize_availability,
        )
    }

    fn reinitialize(
        &self,
        project: ProjectRootCapability,
    ) -> Result<HistoryReinitializeReport, HistoryError> {
        self.run_serialized("reinitialize", |root| {
            let availability = reinitialize_availability(root)?;
            let preserves_existing = match availability {
                HistoryReinitializeAvailability::Ready { preserves_existing } => preserves_existing,
                HistoryReinitializeAvailability::NotNeeded => {
                    return Err(HistoryError::InvalidInput {
                        field: "History reinitialization",
                        reason: "is unnecessary while History is healthy",
                    });
                }
                HistoryReinitializeAvailability::Blocked { reason } => {
                    return Err(HistoryError::Storage {
                        operation: "reinitialize",
                        reason,
                    });
                }
            };

            let git_dir = root.join(".git");
            let preserved_history = if preserves_existing {
                let preserved = next_preserved_history_path(root)?;
                fs::rename(&git_dir, &preserved).map_err(|error| HistoryError::Storage {
                    operation: "preserve damaged History",
                    reason: error.to_string(),
                })?;
                Some(preserved)
            } else {
                None
            };

            let initialized = initialize_new_repository(root).and_then(|repository| {
                configure_portability(&repository)?;
                validate_repository(&repository, root)?;
                Ok(repository)
            });
            let repository = match initialized {
                Ok(repository) => repository,
                Err(error) => {
                    if fs::symlink_metadata(&git_dir).is_ok_and(|metadata| {
                        metadata.is_dir() && !metadata.file_type().is_symlink()
                    }) {
                        let _ = fs::remove_dir_all(&git_dir);
                    }
                    if let Some(preserved) = &preserved_history
                        && let Err(rollback) = fs::rename(preserved, &git_dir)
                    {
                        return Err(HistoryError::Storage {
                            operation: "roll back History reinitialization",
                            reason: format!("{error}; rollback failed: {rollback}"),
                        });
                    }
                    return Err(error);
                }
            };
            drop(repository);
            Ok(HistoryReinitializeReport {
                state: HistoryState {
                    project,
                    checkpoint_count: 0,
                },
                preserved_history: preserved_history.map(|path| {
                    path.strip_prefix(root)
                        .unwrap_or(path.as_path())
                        .to_string_lossy()
                        .replace('\\', "/")
                }),
            })
        })
    }

    fn checkpoint(&self, input: CheckpointInput) -> Result<CheckpointId, HistoryError> {
        self.run_serialized("checkpoint", |root| {
            input.validate()?;
            validate_resource_paths(input.resources.keys())?;
            let repository = open_repository(root)?;
            let affected_documents = canonical_documents(&input.affected_documents);
            let records = history_records(&repository)?;

            let mut matching = records
                .iter()
                .filter(|record| record.metadata.intent_hash == input.intent_hash);
            if let Some(record) = matching.next() {
                if matching.next().is_some() {
                    return Err(corrupt("checkpoint intent appears more than once"));
                }
                let same_metadata = record.metadata.category == input.category
                    && record.metadata.affected_documents == affected_documents
                    && record.metadata.name == input.name;
                let same_resources =
                    load_snapshot(&repository, record.tree_id)?.resources == input.resources;
                if same_metadata && same_resources {
                    return Ok(checkpoint_id(record.oid));
                }
                return Err(HistoryError::InvalidInput {
                    field: "checkpoint intent",
                    reason: "was already used for a different checkpoint",
                });
            }

            let tree_id = build_tree(&repository, &self.root, &input)?;
            // Autosave, explicit save, and structural persistence can all be
            // requested after a prior operation has already made the exact
            // same canonical tree durable. A new commit would be visually
            // indistinguishable in History and turns an active session into a
            // wall of empty versions. A named milestone remains an intentional
            // marker, and restoration remains an auditable event, even when
            // their tree happens to match the current one.
            if !matches!(
                input.category,
                CheckpointCategory::NamedSnapshot | CheckpointCategory::Restoration
            ) && records
                .first()
                .is_some_and(|current| current.tree_id == tree_id)
            {
                return Ok(checkpoint_id(
                    records.first().expect("current record was observed").oid,
                ));
            }
            let sequence = u64::try_from(records.len())
                .ok()
                .and_then(|count| count.checked_add(1))
                .ok_or_else(|| HistoryError::CorruptHistory {
                    reason: "checkpoint sequence overflowed".into(),
                })?;
            let metadata = CommitMetadata {
                sequence,
                intent_hash: input.intent_hash,
                category: input.category,
                affected_documents,
                name: input.name.clone(),
            };
            let message = encode_metadata(&metadata);
            let signature = Signature::new(
                "ParchMint",
                "history@parchmint.invalid",
                &Time::new(sequence as i64, 0),
            )
            .map_err(|error| storage_git("create checkpoint signature", error))?;
            let tree = repository
                .find_tree(tree_id)
                .map_err(|error| corrupt_git("load checkpoint tree", error))?;
            let parent = records
                .first()
                .map(|record| repository.find_commit(record.oid))
                .transpose()
                .map_err(|error| corrupt_git("load checkpoint parent", error))?;
            let parents: Vec<&Commit<'_>> = parent.iter().collect();
            let oid = repository
                .commit(
                    Some(MAIN_REF),
                    &signature,
                    &signature,
                    &message,
                    &tree,
                    &parents,
                )
                .map_err(|error| storage_git("append checkpoint", error))?;
            let committed = repository
                .find_commit(oid)
                .map_err(|error| corrupt_git("verify appended checkpoint", error))?;
            if committed.tree_id() != tree_id {
                return Err(HistoryError::CorruptHistory {
                    reason: "appended checkpoint points to the wrong tree".into(),
                });
            }
            Ok(checkpoint_id(oid))
        })
    }

    fn list(&self, query: HistoryPageQuery) -> Result<HistoryPage, HistoryError> {
        self.run_serialized("list", |root| {
            query.validate()?;
            let repository = open_repository(root)?;
            list_history(&repository, query)
        })
    }

    fn preview(&self, checkpoint: CheckpointId) -> Result<SnapshotPreview, HistoryError> {
        self.run_serialized("preview", |root| {
            let repository = open_repository(root)?;
            let record = resolve_checkpoint(&repository, checkpoint)?;
            let snapshot = load_snapshot(&repository, record.tree_id)?;
            Ok(SnapshotPreview {
                checkpoint: record.summary(),
                resources: snapshot.resources,
            })
        })
    }

    fn preview_resource_paths(
        &self,
        checkpoint: CheckpointId,
    ) -> Result<SnapshotResourcePaths, HistoryError> {
        self.run_serialized("preview resource paths", |root| {
            let repository = open_repository(root)?;
            let record = resolve_checkpoint(&repository, checkpoint)?;
            Ok(SnapshotResourcePaths {
                checkpoint: record.summary(),
                resource_paths: snapshot_resource_paths(&repository, record.tree_id)?,
            })
        })
    }

    fn read_resource(
        &self,
        checkpoint: CheckpointId,
        path: &CanonicalRelativePath,
    ) -> Result<CheckpointResource, HistoryError> {
        self.run_serialized("read resource", |root| {
            let repository = open_repository(root)?;
            let record = resolve_checkpoint(&repository, checkpoint)?;
            let (content_hash, bytes) =
                load_snapshot_resource(&repository, checkpoint, record.tree_id, path)?;
            Ok(CheckpointResource {
                checkpoint,
                path: path.clone(),
                content_hash,
                bytes,
            })
        })
    }

    fn restore(&self, checkpoint: CheckpointId) -> Result<RestorePlan, HistoryError> {
        self.run_serialized("restore", |root| {
            let repository = open_repository(root)?;
            let record = resolve_checkpoint(&repository, checkpoint)?;
            let snapshot = load_snapshot(&repository, record.tree_id)?;
            let writes = snapshot
                .bytes
                .iter()
                .map(|(path, bytes)| StagedResource {
                    path: path.as_str().to_owned(),
                    bytes: bytes.clone(),
                })
                .collect();
            let current = current_canonical_paths(root)?;
            let snapshot_paths = snapshot.resources.keys().cloned().collect();
            let deletions = current.difference(&snapshot_paths).cloned().collect();
            RestorePlan::complete(
                checkpoint,
                snapshot.resources,
                AtomicWritePlan::new(writes),
                deletions,
            )
        })
    }

    fn verify(&self) -> Result<HistoryIntegrityReport, HistoryError> {
        self.run_serialized("verify", |root| {
            let repository = open_repository(root)?;
            let records = history_records(&repository)?;
            for record in &records {
                load_snapshot(&repository, record.tree_id)?;
            }
            Ok(HistoryIntegrityReport {
                checked_checkpoints: records.len(),
            })
        })
    }

    fn maintain(&self, budget: MaintenanceBudget) -> Result<MaintenanceReport, HistoryError> {
        self.run_serialized("maintain", |root| {
            let repository = open_repository(root)?;
            let records = history_records(&repository)?;
            let checked_objects = maintain_loose_objects(&repository, budget.max_objects)?;
            Ok(MaintenanceReport {
                checked_objects,
                retained_checkpoints: records.len(),
            })
        })
    }
}

struct LooseObject {
    oid: Oid,
    path: PathBuf,
    kind: ObjectType,
    bytes: Vec<u8>,
}

fn maintain_loose_objects(
    repository: &Repository,
    max_objects: usize,
) -> Result<usize, HistoryError> {
    if max_objects == 0 {
        return Ok(0);
    }
    let object_directory = repository.path().join("objects");
    let odb = repository
        .odb()
        .map_err(|error| corrupt_git("open History object database", error))?;
    let mut loose_paths = loose_object_paths(&object_directory)?;
    loose_paths.truncate(max_objects);
    if loose_paths.is_empty() {
        return Ok(0);
    }

    let mut objects = Vec::with_capacity(loose_paths.len());
    let mut pack = repository
        .packbuilder()
        .map_err(|error| storage_git("create History pack", error))?;
    pack.set_threads(1);
    for (oid, path) in loose_paths {
        let object = odb
            .read(oid)
            .map_err(|error| corrupt_git("read loose History object", error))?;
        let kind = object.kind();
        let bytes = object.data().to_vec();
        pack.insert_object(oid, None)
            .map_err(|error| corrupt_git("add History object to pack", error))?;
        objects.push(LooseObject {
            oid,
            path,
            kind,
            bytes,
        });
    }
    if pack.object_count() != objects.len() {
        return Err(corrupt("History pack omitted a selected object"));
    }

    let pack_directory = object_directory.join("pack");
    fs::create_dir_all(&pack_directory).map_err(|error| HistoryError::Storage {
        operation: "create History pack directory",
        reason: error.to_string(),
    })?;
    pack.write(&pack_directory, 0)
        .map_err(|error| storage_git("write History pack", error))?;
    let pack_name = pack
        .name()
        .map_err(|error| storage_git("read History pack name", error))?
        .ok_or_else(|| corrupt("written History pack has no name"))?;
    if pack_name.len() != 40 || !pack_name.bytes().all(is_lower_hex) {
        return Err(corrupt("written History pack name is invalid"));
    }
    for extension in ["pack", "idx"] {
        let path = pack_directory.join(format!("pack-{pack_name}.{extension}"));
        let metadata = fs::symlink_metadata(&path).map_err(|error| HistoryError::Storage {
            operation: "verify written History pack",
            reason: error.to_string(),
        })?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(corrupt("written History pack is not a regular file"));
        }
    }

    if let Err(error) = verify_new_pack(repository.path(), pack_name, &objects) {
        remove_pack_files(&pack_directory, pack_name)?;
        return Err(error);
    }
    for object in &objects {
        fs::remove_file(&object.path).map_err(|error| HistoryError::Storage {
            operation: "remove verified loose History object",
            reason: error.to_string(),
        })?;
    }
    Ok(objects.len())
}

fn verify_new_pack(
    git_dir: &Path,
    pack_name: &str,
    objects: &[LooseObject],
) -> Result<(), HistoryError> {
    let sequence = NEXT_PACK_VERIFICATION.fetch_add(1, Ordering::Relaxed);
    let verification_root = git_dir.join(format!(
        ".parchmint-pack-verification-{}-{sequence}",
        std::process::id()
    ));
    fs::create_dir(&verification_root).map_err(|error| HistoryError::Storage {
        operation: "create History pack verification directory",
        reason: error.to_string(),
    })?;

    let verification = (|| {
        let bare_path = verification_root.join("repository.git");
        let mut options = RepositoryInitOptions::new();
        options.bare(true).external_template(false).no_reinit(true);
        let bare = Repository::init_opts(&bare_path, &options)
            .map_err(|error| storage_git("initialize History pack verification", error))?;
        let target_pack_directory = bare.path().join("objects/pack");
        for extension in ["pack", "idx"] {
            let file_name = format!("pack-{pack_name}.{extension}");
            link_or_copy(
                &git_dir.join("objects/pack").join(&file_name),
                &target_pack_directory.join(file_name),
            )?;
        }
        drop(bare);

        let bare = Repository::open_bare(&bare_path)
            .map_err(|error| corrupt_git("open History pack verification", error))?;
        let odb = bare
            .odb()
            .map_err(|error| corrupt_git("open History pack object database", error))?;
        for expected in objects {
            let actual = odb
                .read(expected.oid)
                .map_err(|error| corrupt_git("read object from new History pack", error))?;
            if actual.kind() != expected.kind || actual.data() != expected.bytes {
                return Err(corrupt("new History pack changed an object"));
            }
        }
        Ok(())
    })();

    let cleanup = fs::remove_dir_all(&verification_root).map_err(|error| HistoryError::Storage {
        operation: "remove History pack verification directory",
        reason: error.to_string(),
    });
    verification.and(cleanup)
}

fn remove_pack_files(pack_directory: &Path, pack_name: &str) -> Result<(), HistoryError> {
    for extension in ["pack", "idx"] {
        fs::remove_file(pack_directory.join(format!("pack-{pack_name}.{extension}"))).map_err(
            |error| HistoryError::Storage {
                operation: "remove failed History pack",
                reason: error.to_string(),
            },
        )?;
    }
    Ok(())
}

fn link_or_copy(source: &Path, target: &Path) -> Result<(), HistoryError> {
    if fs::hard_link(source, target).is_ok() {
        return Ok(());
    }
    fs::copy(source, target)
        .map(|_| ())
        .map_err(|error| HistoryError::Storage {
            operation: "copy History pack for verification",
            reason: error.to_string(),
        })
}

fn loose_object_paths(object_directory: &Path) -> Result<Vec<(Oid, PathBuf)>, HistoryError> {
    let entries = fs::read_dir(object_directory).map_err(|error| HistoryError::Storage {
        operation: "scan loose History objects",
        reason: error.to_string(),
    })?;
    let mut objects = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| HistoryError::Storage {
            operation: "scan loose History object directory",
            reason: error.to_string(),
        })?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if matches!(name.as_ref(), "info" | "pack") {
            continue;
        }
        let metadata = entry.file_type().map_err(|error| HistoryError::Storage {
            operation: "inspect loose History object directory",
            reason: error.to_string(),
        })?;
        if metadata.is_symlink()
            || !metadata.is_dir()
            || name.len() != 2
            || !name.bytes().all(is_lower_hex)
        {
            return Err(corrupt("History object database has an unexpected entry"));
        }
        let files = fs::read_dir(entry.path()).map_err(|error| HistoryError::Storage {
            operation: "scan loose History object bucket",
            reason: error.to_string(),
        })?;
        for file in files {
            let file = file.map_err(|error| HistoryError::Storage {
                operation: "scan loose History object",
                reason: error.to_string(),
            })?;
            let file_name = file.file_name();
            let file_name = file_name.to_string_lossy();
            let file_type = file.file_type().map_err(|error| HistoryError::Storage {
                operation: "inspect loose History object",
                reason: error.to_string(),
            })?;
            if file_type.is_symlink()
                || !file_type.is_file()
                || file_name.len() != 38
                || !file_name.bytes().all(is_lower_hex)
            {
                return Err(corrupt(
                    "History object database has an invalid loose object",
                ));
            }
            let oid = Oid::from_str(&format!("{name}{file_name}"))
                .map_err(|_| corrupt("loose History object name is invalid"))?;
            objects.push((oid, file.path()));
        }
    }
    objects.sort_by_key(|(oid, _)| *oid);
    Ok(objects)
}

fn is_lower_hex(byte: u8) -> bool {
    byte.is_ascii_digit() || matches!(byte, b'a'..=b'f')
}

fn repository_gate(path: &Path) -> RepositoryGate {
    let registry = REPOSITORY_GATES.get_or_init(|| Mutex::new(BTreeMap::new()));
    let Ok(mut registry) = registry.lock() else {
        return Arc::new(Mutex::new(()));
    };
    registry.retain(|_, gate| gate.strong_count() > 0);
    if let Some(gate) = registry.get(path).and_then(Weak::upgrade) {
        return gate;
    }
    let gate = Arc::new(Mutex::new(()));
    registry.insert(path.to_path_buf(), Arc::downgrade(&gate));
    gate
}

fn configure_new_repository(repository: &Repository, root: &Path) -> Result<(), HistoryError> {
    repository
        .set_head(MAIN_REF)
        .map_err(|error| storage_git("set History main branch", error))?;
    let root_id = project_identity(root)?;
    let mut config = repository
        .config()
        .map_err(|error| storage_git("open History configuration", error))?;
    config
        .set_i64(FORMAT_MARKER, 1)
        .and_then(|()| config.set_str(HISTORY_ID, &root_id))
        .map_err(|error| storage_git("mark embedded History repository", error))
}

fn initialize_new_repository(root: &Path) -> Result<Repository, HistoryError> {
    let mut options = RepositoryInitOptions::new();
    options
        .initial_head("main")
        .external_template(false)
        .no_reinit(true);
    let repository = Repository::init_opts(root, &options)
        .map_err(|error| storage_git("initialize embedded repository", error))?;
    configure_new_repository(&repository, root)?;
    Ok(repository)
}

fn reinitialize_availability(root: &Path) -> Result<HistoryReinitializeAvailability, HistoryError> {
    let git_dir = root.join(".git");
    let metadata = match fs::symlink_metadata(&git_dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(HistoryReinitializeAvailability::Ready {
                preserves_existing: false,
            });
        }
        Err(error) => {
            return Err(HistoryError::Storage {
                operation: "inspect History reinitialization",
                reason: error.to_string(),
            });
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Ok(HistoryReinitializeAvailability::Blocked {
            reason: "the .git path is not a regular app-managed History directory".into(),
        });
    }
    let repository = match Repository::open(root) {
        Ok(repository) => repository,
        Err(error) => {
            return Ok(HistoryReinitializeAvailability::Blocked {
                reason: format!("cannot prove the damaged repository is app-managed: {error}"),
            });
        }
    };
    if validate_repository(&repository, root)
        .and_then(|()| history_records(&repository).map(|_| ()))
        .is_ok()
    {
        return Ok(HistoryReinitializeAvailability::NotNeeded);
    }
    let expected_identity = project_identity(root)?;
    let managed = repository.config().is_ok_and(|config| {
        config.get_i64(FORMAT_MARKER) == Ok(1)
            && matches!(config.get_string(HISTORY_ID), Ok(identity) if identity == expected_identity)
    });
    if managed {
        Ok(HistoryReinitializeAvailability::Ready {
            preserves_existing: true,
        })
    } else {
        Ok(HistoryReinitializeAvailability::Blocked {
            reason: "the repository is not identifiable as app-managed History".into(),
        })
    }
}

fn next_preserved_history_path(root: &Path) -> Result<PathBuf, HistoryError> {
    let preservation_root = root.join(".parchmint");
    let metadata =
        fs::symlink_metadata(&preservation_root).map_err(|error| HistoryError::Storage {
            operation: "inspect History preservation directory",
            reason: error.to_string(),
        })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(HistoryError::Storage {
            operation: "inspect History preservation directory",
            reason: ".parchmint is not a regular directory".into(),
        });
    }
    for sequence in 1..=u32::MAX {
        let candidate = preservation_root.join(format!("damaged-history-{sequence}.git"));
        match fs::symlink_metadata(&candidate) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(candidate),
            Ok(_) => {}
            Err(error) => {
                return Err(HistoryError::Storage {
                    operation: "select History preservation path",
                    reason: error.to_string(),
                });
            }
        }
    }
    Err(HistoryError::Storage {
        operation: "select History preservation path",
        reason: "no preservation name is available".into(),
    })
}

fn project_identity(root: &Path) -> Result<String, HistoryError> {
    let root_id = fs::read_to_string(root.join(".parchmint/root-id")).map_err(|error| {
        HistoryError::Storage {
            operation: "read project identity",
            reason: error.to_string(),
        }
    })?;
    let root_id = u64::from_str_radix(root_id.trim(), 16).map_err(|_| HistoryError::Storage {
        operation: "read project identity",
        reason: "project root identity is invalid".into(),
    })?;
    Ok(format!("{root_id:016x}"))
}

fn configure_portability(repository: &Repository) -> Result<(), HistoryError> {
    let mut config = repository
        .config()
        .map_err(|error| storage_git("open History configuration", error))?;
    config
        .set_bool("core.filemode", false)
        .and_then(|()| config.set_bool("core.autocrlf", false))
        .and_then(|()| config.set_bool("core.symlinks", false))
        .map_err(|error| storage_git("configure portable History", error))
}

fn validate_repository(repository: &Repository, root: &Path) -> Result<(), HistoryError> {
    if repository.is_bare() {
        return Err(corrupt("embedded History repository is bare"));
    }
    let workdir = repository
        .workdir()
        .ok_or_else(|| corrupt("embedded History has no worktree"))?;
    if fs::canonicalize(workdir).ok().as_deref() != fs::canonicalize(root).ok().as_deref() {
        return Err(corrupt("embedded History worktree is not the project root"));
    }
    let config = repository
        .config()
        .map_err(|error| corrupt_git("read History configuration", error))?;
    if config.get_i64(FORMAT_MARKER) != Ok(1) {
        return Err(corrupt("project Git repository is not app-managed History"));
    }
    let history_id = config
        .get_string(HISTORY_ID)
        .map_err(|error| corrupt_git("read History identity", error))?;
    if history_id != project_identity(root)? {
        return Err(corrupt("embedded History belongs to a different project"));
    }
    let head = repository
        .find_reference("HEAD")
        .map_err(|error| corrupt_git("read History HEAD", error))?;
    if head
        .symbolic_target()
        .map_err(|error| corrupt_git("read symbolic History HEAD", error))?
        != Some(MAIN_REF)
    {
        return Err(corrupt("History HEAD is not app-managed main"));
    }
    let remotes = repository
        .remotes()
        .map_err(|error| corrupt_git("inspect History remotes", error))?;
    if !remotes.is_empty() {
        return Err(corrupt("embedded History must not have remotes"));
    }
    let local_heads = repository
        .references_glob("refs/heads/*")
        .map_err(|error| corrupt_git("inspect History branches", error))?;
    for reference in local_heads {
        let reference = reference.map_err(|error| corrupt_git("read History branch", error))?;
        if reference
            .name()
            .map_err(|error| corrupt_git("read History branch name", error))?
            != MAIN_REF
        {
            return Err(corrupt("embedded History has an unexpected branch"));
        }
    }
    Ok(())
}

fn open_repository(root: &Path) -> Result<Repository, HistoryError> {
    let git_dir = root.join(".git");
    if !git_dir.exists() {
        return Err(HistoryError::MissingHistory);
    }
    reject_symlink(&git_dir, "Git directory")?;
    let repository =
        Repository::open(root).map_err(|error| corrupt_git("open embedded repository", error))?;
    validate_repository(&repository, root)?;
    Ok(repository)
}

fn recover_stale_locks(git_dir: &Path) -> Result<(), HistoryError> {
    for relative in [
        "index.lock",
        "HEAD.lock",
        "config.lock",
        "packed-refs.lock",
        "refs/heads/main.lock",
    ] {
        let lock = git_dir.join(relative);
        let metadata = match fs::symlink_metadata(&lock) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(HistoryError::Storage {
                    operation: "inspect stale Git lock",
                    reason: error.to_string(),
                });
            }
        };
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(corrupt("stale Git lock path is not a regular file"));
        }
        fs::remove_file(&lock).map_err(|error| HistoryError::Storage {
            operation: "recover stale Git lock",
            reason: error.to_string(),
        })?;
    }
    Ok(())
}

fn build_tree(
    repository: &Repository,
    root: &NativeProjectRoot,
    input: &CheckpointInput,
) -> Result<Oid, HistoryError> {
    let checked_root = root.checked_path().map_err(|error| HistoryError::Storage {
        operation: "validate complete checkpoint resource set",
        reason: error.to_string(),
    })?;
    let current_paths = current_canonical_paths(checked_root)?;
    let input_paths: BTreeSet<_> = input.resources.keys().cloned().collect();
    if current_paths != input_paths {
        return Err(HistoryError::InvalidInput {
            field: "checkpoint resources",
            reason: "must contain the complete canonical project resource set",
        });
    }
    let mut index = repository
        .index()
        .map_err(|error| storage_git("open private History index", error))?;
    index
        .clear()
        .map_err(|error| storage_git("clear private History index", error))?;
    let files = NativeProjectFileSystem::new();
    for (path, expected_hash) in &input.resources {
        let bytes = files
            .read(root, path)
            .map_err(|error| HistoryError::Storage {
                operation: "read canonical checkpoint resource",
                reason: error.to_string(),
            })?;
        let bytes = normalize_line_endings(&bytes);
        let actual_hash = ContentHash::of_bytes(&bytes);
        if &actual_hash != expected_hash {
            return Err(HistoryError::InvalidInput {
                field: "checkpoint resources",
                reason: "do not match the canonical files on disk",
            });
        }
        let entry = IndexEntry {
            ctime: IndexTime::new(0, 0),
            mtime: IndexTime::new(0, 0),
            dev: 0,
            ino: 0,
            mode: 0o100644,
            uid: 0,
            gid: 0,
            file_size: 0,
            id: Oid::ZERO_SHA1,
            flags: 0,
            flags_extended: 0,
            path: path.as_str().as_bytes().to_vec(),
        };
        index
            .add_frombuffer(&entry, &bytes)
            .map_err(|error| storage_git("stage checkpoint resource", error))?;
    }
    index
        .write_tree()
        .map_err(|error| storage_git("write checkpoint tree", error))
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

#[derive(Clone)]
struct CommitMetadata {
    sequence: u64,
    intent_hash: CheckpointIntentHash,
    category: CheckpointCategory,
    affected_documents: Vec<parchmint_history_api::DocumentId>,
    name: Option<SnapshotName>,
}

#[derive(Clone)]
struct HistoryRecord {
    oid: Oid,
    tree_id: Oid,
    metadata: CommitMetadata,
}

impl HistoryRecord {
    fn summary(&self) -> CheckpointSummary {
        CheckpointSummary {
            id: checkpoint_id(self.oid),
            sequence: self.metadata.sequence,
            category: self.metadata.category,
            affected_documents: self.metadata.affected_documents.clone(),
            name: self.metadata.name.clone(),
        }
    }
}

fn history_records(repository: &Repository) -> Result<Vec<HistoryRecord>, HistoryError> {
    let mut records = Vec::new();
    let mut next = head_oid(repository)?;
    let mut visited = BTreeSet::new();
    while let Some(oid) = next {
        if !visited.insert(oid) {
            return Err(corrupt("History commit chain contains a cycle"));
        }
        let commit = repository
            .find_commit(oid)
            .map_err(|error| corrupt_git("load History commit", error))?;
        let metadata = decode_metadata(commit.message_bytes())?;
        next = parent_oid(&commit)?;
        records.push(HistoryRecord {
            oid,
            tree_id: commit.tree_id(),
            metadata,
        });
    }
    for (index, record) in records.iter().enumerate() {
        let expected = u64::try_from(records.len() - index)
            .map_err(|_| corrupt("History checkpoint count cannot be represented as a sequence"))?;
        if record.metadata.sequence != expected {
            return Err(corrupt("History checkpoint sequence is not contiguous"));
        }
    }
    Ok(records)
}

fn head_oid(repository: &Repository) -> Result<Option<Oid>, HistoryError> {
    match repository.find_reference(MAIN_REF) {
        Ok(reference) => reference
            .target()
            .map(Some)
            .ok_or_else(|| corrupt("History main reference is symbolic")),
        Err(error) if error.code() == ErrorCode::NotFound => Ok(None),
        Err(error) => Err(corrupt_git("read History main reference", error)),
    }
}

fn parent_oid(commit: &Commit<'_>) -> Result<Option<Oid>, HistoryError> {
    match commit.parent_count() {
        0 => Ok(None),
        1 => commit
            .parent_id(0)
            .map(Some)
            .map_err(|error| corrupt_git("read History parent", error)),
        _ => Err(corrupt("History contains a merge commit")),
    }
}

fn encode_metadata(metadata: &CommitMetadata) -> String {
    let affected = metadata
        .affected_documents
        .iter()
        .map(|document| encode_hex(document.as_bytes()))
        .collect::<Vec<_>>()
        .join(",");
    let name = metadata
        .name
        .as_ref()
        .map(|name| encode_hex(name.as_str().as_bytes()))
        .unwrap_or_else(|| "-".into());
    format!(
        "{MESSAGE_HEADER}\nsequence={}\nintent={}\ncategory={}\naffected={affected}\nname={name}\n",
        metadata.sequence,
        encode_hex(metadata.intent_hash.as_bytes()),
        encode_category(metadata.category),
    )
}

fn decode_metadata(message: &[u8]) -> Result<CommitMetadata, HistoryError> {
    let message =
        str::from_utf8(message).map_err(|_| corrupt("checkpoint metadata is not UTF-8"))?;
    let mut lines = message.lines();
    if lines.next() != Some(MESSAGE_HEADER) {
        return Err(corrupt("checkpoint metadata header is invalid"));
    }
    let sequence = parse_field(&mut lines, "sequence=")?
        .parse::<u64>()
        .map_err(|_| corrupt("checkpoint sequence is invalid"))?;
    let intent = decode_fixed::<32>(parse_field(&mut lines, "intent=")?)?;
    let category = decode_category(parse_field(&mut lines, "category=")?)?;
    let affected = parse_field(&mut lines, "affected=")?;
    let mut affected_documents = Vec::new();
    if !affected.is_empty() {
        for document in affected.split(',') {
            affected_documents.push(parchmint_history_api::DocumentId::from_bytes(
                decode_fixed::<16>(document)?,
            ));
        }
    }
    if canonical_documents(&affected_documents) != affected_documents {
        return Err(corrupt("checkpoint document metadata is not canonical"));
    }
    let name = match parse_field(&mut lines, "name=")? {
        "-" => None,
        encoded => {
            let bytes = decode_hex(encoded)?;
            let name = String::from_utf8(bytes)
                .map_err(|_| corrupt("snapshot name metadata is not UTF-8"))?;
            Some(SnapshotName::new(name).map_err(|_| corrupt("snapshot name is invalid"))?)
        }
    };
    if lines.any(|line| !line.is_empty()) {
        return Err(corrupt("checkpoint metadata has unexpected fields"));
    }
    if matches!(category, CheckpointCategory::NamedSnapshot) != name.is_some() {
        return Err(corrupt("checkpoint snapshot metadata is inconsistent"));
    }
    Ok(CommitMetadata {
        sequence,
        intent_hash: CheckpointIntentHash::from_bytes(intent),
        category,
        affected_documents,
        name,
    })
}

fn parse_field<'a>(
    lines: &mut impl Iterator<Item = &'a str>,
    prefix: &str,
) -> Result<&'a str, HistoryError> {
    lines
        .next()
        .and_then(|line| line.strip_prefix(prefix))
        .ok_or_else(|| corrupt("checkpoint metadata field is missing or invalid"))
}

fn encode_category(category: CheckpointCategory) -> &'static str {
    match category {
        CheckpointCategory::Autosave => "autosave",
        CheckpointCategory::ExplicitSave => "explicit-save",
        CheckpointCategory::StructuralChange => "structural-change",
        CheckpointCategory::NamedSnapshot => "named-snapshot",
        CheckpointCategory::Restoration => "restoration",
    }
}

fn decode_category(category: &str) -> Result<CheckpointCategory, HistoryError> {
    match category {
        "autosave" => Ok(CheckpointCategory::Autosave),
        "explicit-save" => Ok(CheckpointCategory::ExplicitSave),
        "structural-change" => Ok(CheckpointCategory::StructuralChange),
        "named-snapshot" => Ok(CheckpointCategory::NamedSnapshot),
        "restoration" => Ok(CheckpointCategory::Restoration),
        _ => Err(corrupt("checkpoint category is invalid")),
    }
}

fn canonical_documents(
    documents: &[parchmint_history_api::DocumentId],
) -> Vec<parchmint_history_api::DocumentId> {
    let mut documents = documents.to_vec();
    documents.sort_unstable();
    documents.dedup();
    documents
}

fn list_history(
    repository: &Repository,
    query: HistoryPageQuery,
) -> Result<HistoryPage, HistoryError> {
    let history_id = repository_history_id(repository)?;
    let filter_token = query
        .affected_document
        .map(|document| encode_hex(document.as_bytes()))
        .unwrap_or_else(|| "-".into());
    let anchor = query
        .cursor
        .as_ref()
        .map(|cursor| parse_cursor(cursor, &history_id, &filter_token))
        .transpose()?;
    let mut anchor_found = anchor.is_none();
    let mut next = head_oid(repository)?;
    let mut visited = BTreeSet::new();
    let mut matches = Vec::new();
    while let Some(oid) = next {
        if !visited.insert(oid) {
            return Err(corrupt("History commit chain contains a cycle"));
        }
        let commit = repository
            .find_commit(oid)
            .map_err(|error| corrupt_git("load listed checkpoint", error))?;
        let metadata = decode_metadata(commit.message_bytes())?;
        next = parent_oid(&commit)?;
        if !anchor_found {
            if Some(oid) == anchor {
                let belongs_to_filter = query
                    .affected_document
                    .is_none_or(|document| metadata.affected_documents.contains(&document));
                if !belongs_to_filter {
                    return Err(HistoryError::InvalidCursor);
                }
                anchor_found = true;
            }
            continue;
        }
        if query
            .affected_document
            .is_none_or(|document| metadata.affected_documents.contains(&document))
        {
            matches.push(HistoryRecord {
                oid,
                tree_id: commit.tree_id(),
                metadata,
            });
            if matches.len() > query.limit {
                break;
            }
        }
    }
    if !anchor_found {
        return Err(HistoryError::InvalidCursor);
    }
    let has_more = matches.len() > query.limit;
    if has_more {
        matches.pop();
    }
    let next_cursor = has_more.then(|| {
        let anchor = matches
            .last()
            .expect("a page with a continuation has a checkpoint")
            .oid;
        HistoryCursor::new(format!(
            "{CURSOR_HEADER}|{history_id}|{filter_token}|{anchor}"
        ))
    });
    Ok(HistoryPage {
        checkpoints: matches.iter().map(HistoryRecord::summary).collect(),
        next_cursor,
    })
}

fn repository_history_id(repository: &Repository) -> Result<String, HistoryError> {
    repository
        .config()
        .and_then(|config| config.get_string(HISTORY_ID))
        .map_err(|error| corrupt_git("read History identity", error))
}

fn parse_cursor(
    cursor: &HistoryCursor,
    history_id: &str,
    filter: &str,
) -> Result<Oid, HistoryError> {
    let fields: Vec<_> = cursor.as_str().split('|').collect();
    if fields.len() != 4
        || fields[0] != CURSOR_HEADER
        || fields[1] != history_id
        || fields[2] != filter
    {
        return Err(HistoryError::InvalidCursor);
    }
    Oid::from_str(fields[3]).map_err(|_| HistoryError::InvalidCursor)
}

fn resolve_checkpoint(
    repository: &Repository,
    checkpoint: CheckpointId,
) -> Result<HistoryRecord, HistoryError> {
    let mut found = None;
    for record in history_records(repository)? {
        if checkpoint_id(record.oid) == checkpoint {
            if found.is_some() {
                return Err(corrupt("two commits share one checkpoint identifier"));
            }
            found = Some(record);
        }
    }
    found.ok_or(HistoryError::UnknownCheckpoint { checkpoint })
}

struct SnapshotData {
    resources: BTreeMap<CanonicalRelativePath, ContentHash>,
    bytes: BTreeMap<CanonicalRelativePath, Vec<u8>>,
}

/// Reads one resource without materializing every blob in the checkpoint.
/// History preview already inspects the checkpoint manifest; repeating that
/// complete traversal just to load one selected manuscript made comparisons
/// scale with the whole project twice.
fn load_snapshot_resource(
    repository: &Repository,
    checkpoint: CheckpointId,
    tree_id: Oid,
    path: &CanonicalRelativePath,
) -> Result<(ContentHash, Vec<u8>), HistoryError> {
    let tree = repository
        .find_tree(tree_id)
        .map_err(|error| corrupt_git("load checkpoint tree", error))?;
    let entry =
        tree.get_path(Path::new(path.as_str()))
            .map_err(|_| HistoryError::UnknownResource {
                checkpoint,
                path: path.clone(),
            })?;
    if entry.kind() != Some(ObjectType::Blob) || entry.filemode() != 0o100644 {
        return Err(corrupt("checkpoint resource is not a regular file"));
    }
    let blob = repository
        .find_blob(entry.id())
        .map_err(|error| corrupt_git("load checkpoint resource", error))?;
    let bytes = blob.content().to_vec();
    Ok((ContentHash::of_bytes(&bytes), bytes))
}

fn snapshot_resource_paths(
    repository: &Repository,
    tree_id: Oid,
) -> Result<Vec<CanonicalRelativePath>, HistoryError> {
    Ok(snapshot_tree_entries(repository, tree_id)?
        .into_iter()
        .map(|(path, _)| path)
        .collect())
}

fn load_snapshot(repository: &Repository, tree_id: Oid) -> Result<SnapshotData, HistoryError> {
    let mut resources = BTreeMap::new();
    let mut bytes_by_path = BTreeMap::new();
    for (path, blob_id) in snapshot_tree_entries(repository, tree_id)? {
        let blob = repository
            .find_blob(blob_id)
            .map_err(|error| corrupt_git("load checkpoint resource", error))?;
        let bytes = blob.content().to_vec();
        let hash = ContentHash::of_bytes(&bytes);
        resources.insert(path.clone(), hash);
        bytes_by_path.insert(path, bytes);
    }
    Ok(SnapshotData {
        resources,
        bytes: bytes_by_path,
    })
}

fn snapshot_tree_entries(
    repository: &Repository,
    tree_id: Oid,
) -> Result<Vec<(CanonicalRelativePath, Oid)>, HistoryError> {
    let tree = repository
        .find_tree(tree_id)
        .map_err(|error| corrupt_git("load checkpoint tree", error))?;
    let mut index =
        git2::Index::new().map_err(|error| corrupt_git("create checkpoint tree reader", error))?;
    index
        .read_tree(&tree)
        .map_err(|error| corrupt_git("read checkpoint tree", error))?;
    let mut entries = Vec::new();
    let mut portable_paths = BTreeSet::new();
    for entry in index.iter() {
        if entry.mode != 0o100644 {
            return Err(corrupt("checkpoint tree contains a nonportable file mode"));
        }
        let path = str::from_utf8(&entry.path)
            .map_err(|_| corrupt("checkpoint tree path is not UTF-8"))?;
        let path = CanonicalRelativePath::parse(path)
            .map_err(|_| corrupt("checkpoint tree path is not canonical"))?;
        if !is_history_resource(&path) {
            return Err(corrupt("checkpoint tree contains an unexpected path"));
        }
        if !portable_paths.insert(path.as_str().to_lowercase()) {
            return Err(corrupt(
                "checkpoint tree paths collide on a portable filesystem",
            ));
        }
        entries.push((path, entry.id));
    }
    Ok(entries)
}

fn validate_resource_paths<'a>(
    paths: impl IntoIterator<Item = &'a CanonicalRelativePath>,
) -> Result<(), HistoryError> {
    let mut portable = BTreeSet::new();
    for path in paths {
        if !is_history_resource(path) {
            return Err(HistoryError::InvalidInput {
                field: "checkpoint resource path",
                reason: "is not part of canonical project History",
            });
        }
        if !portable.insert(path.as_str().to_lowercase()) {
            return Err(HistoryError::InvalidInput {
                field: "checkpoint resource paths",
                reason: "collide on a portable filesystem",
            });
        }
    }
    Ok(())
}

fn is_history_resource(path: &CanonicalRelativePath) -> bool {
    let path = path.as_str();
    matches!(
        path,
        ".parchmint/format-version"
            | "project.toml"
            | "styles.css"
            | "dictionary.txt"
            | "deletions.json"
    ) || ((path.starts_with("manuscript/") || path.starts_with("research/"))
        && path.ends_with(".html"))
        || (path.starts_with("annotations/") && path.ends_with(".json"))
}

fn current_canonical_paths(root: &Path) -> Result<BTreeSet<CanonicalRelativePath>, HistoryError> {
    let mut paths = BTreeSet::new();
    for relative in [
        ".parchmint/format-version",
        "project.toml",
        "styles.css",
        "dictionary.txt",
        "deletions.json",
    ] {
        let target = root.join(relative);
        match fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                paths.insert(
                    CanonicalRelativePath::parse(relative)
                        .expect("fixed canonical History path is valid"),
                );
            }
            Ok(_) => return Err(unsafe_current_path(&target)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(current_path_io("inspect current project resource", error)),
        }
    }
    for directory in ["manuscript", "research", "annotations"] {
        let target = root.join(directory);
        match fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
                walk_canonical_directory(root, &target, &mut paths)?;
            }
            Ok(_) => return Err(unsafe_current_path(&target)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(current_path_io("inspect canonical directory", error)),
        }
    }
    validate_resource_paths(paths.iter())?;
    Ok(paths)
}

fn walk_canonical_directory(
    root: &Path,
    directory: &Path,
    paths: &mut BTreeSet<CanonicalRelativePath>,
) -> Result<(), HistoryError> {
    let entries = fs::read_dir(directory)
        .map_err(|error| current_path_io("read canonical directory", error))?;
    for entry in entries {
        let entry = entry.map_err(|error| current_path_io("read canonical entry", error))?;
        let metadata = entry
            .file_type()
            .map_err(|error| current_path_io("inspect canonical entry", error))?;
        if metadata.is_symlink() {
            return Err(unsafe_current_path(&entry.path()));
        }
        if metadata.is_dir() {
            walk_canonical_directory(root, &entry.path(), paths)?;
            continue;
        }
        if !metadata.is_file() {
            return Err(unsafe_current_path(&entry.path()));
        }
        let entry_path = entry.path();
        let relative = entry_path
            .strip_prefix(root)
            .map_err(|_| unsafe_current_path(&entry_path))?
            .components()
            .map(|component| {
                component
                    .as_os_str()
                    .to_str()
                    .ok_or_else(|| unsafe_current_path(&entry_path))
            })
            .collect::<Result<Vec<_>, _>>()?
            .join("/");
        let path =
            CanonicalRelativePath::parse(relative).map_err(|_| unsafe_current_path(&entry_path))?;
        if !is_history_resource(&path) {
            return Err(unsafe_current_path(&entry_path));
        }
        paths.insert(path);
    }
    Ok(())
}

fn checkpoint_id(oid: Oid) -> CheckpointId {
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&oid.as_bytes()[..16]);
    CheckpointId::from_bytes(bytes)
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[usize::from(byte >> 4)] as char);
        output.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    output
}

fn decode_fixed<const N: usize>(encoded: &str) -> Result<[u8; N], HistoryError> {
    let decoded = decode_hex(encoded)?;
    decoded
        .try_into()
        .map_err(|_| corrupt("checkpoint metadata has the wrong byte length"))
}

fn decode_hex(encoded: &str) -> Result<Vec<u8>, HistoryError> {
    if !encoded.len().is_multiple_of(2) {
        return Err(corrupt("checkpoint metadata contains invalid hexadecimal"));
    }
    encoded
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_nibble(pair[0])?;
            let low = hex_nibble(pair[1])?;
            Ok((high << 4) | low)
        })
        .collect()
}

fn hex_nibble(byte: u8) -> Result<u8, HistoryError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(corrupt("checkpoint metadata contains invalid hexadecimal")),
    }
}

fn reject_symlink(path: &Path, label: &str) -> Result<(), HistoryError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| HistoryError::Storage {
        operation: "inspect embedded History path",
        reason: error.to_string(),
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(HistoryError::CorruptHistory {
            reason: format!("{label} is not a regular directory"),
        });
    }
    Ok(())
}

fn unsafe_current_path(path: &Path) -> HistoryError {
    HistoryError::Storage {
        operation: "build complete restore plan",
        reason: format!("unsafe or unexpected canonical path {}", path.display()),
    }
}

fn current_path_io(operation: &'static str, error: std::io::Error) -> HistoryError {
    HistoryError::Storage {
        operation,
        reason: error.to_string(),
    }
}

fn corrupt(reason: impl Into<String>) -> HistoryError {
    HistoryError::CorruptHistory {
        reason: reason.into(),
    }
}

fn corrupt_git(operation: &'static str, error: git2::Error) -> HistoryError {
    HistoryError::CorruptHistory {
        reason: format!("{operation}: {error}"),
    }
}

fn storage_git(operation: &'static str, error: git2::Error) -> HistoryError {
    HistoryError::Storage {
        operation,
        reason: error.to_string(),
    }
}
