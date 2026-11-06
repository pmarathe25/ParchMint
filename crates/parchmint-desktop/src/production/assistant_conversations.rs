//! Coalesced, ordered application-data persistence for private assistant chats.

use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use parchmint_ui_api::AssistantConversationState;
use sha2::{Digest, Sha256};

const MAX_BYTES: usize = 8 * 1024 * 1024;
static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

struct State {
    snapshot: AssistantConversationState,
    generation: u64,
    persisted: u64,
    active: bool,
    failure: Option<String>,
}

struct Inner {
    path: PathBuf,
    state: Mutex<State>,
    changed: Condvar,
}

/// A project lease owns one store. Queueing is nonblocking; a single worker
/// serializes and writes the newest snapshot in generation order.
#[derive(Clone)]
pub(super) struct ConversationStore(Arc<Inner>);

impl ConversationStore {
    pub(super) fn open(
        directory: &Path,
        project_id: &[u8],
        project_path: &Path,
    ) -> Result<Self, String> {
        let canonical = fs::canonicalize(project_path)
            .map_err(|_| "Could not identify the project for assistant history.".to_owned())?;
        let mut digest = Sha256::new();
        digest.update(project_id);
        digest.update([0]);
        hash_path(&mut digest, &canonical);
        let name = digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let path = directory.join(format!("{name}.json"));
        let snapshot = match fs::File::open(&path) {
            Ok(file) => {
                let bytes = read_history(file)?;
                let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| {
                    "Assistant history is invalid; the existing file was preserved.".to_owned()
                })?;
                let version = value.get("version").and_then(serde_json::Value::as_u64);
                if !matches!(version, Some(1 | 2)) {
                    return Err("Assistant history uses an unsupported version; the existing file was preserved.".into());
                }
                let snapshot: AssistantConversationState =
                    serde_json::from_value(value.get("state").cloned().ok_or_else(|| {
                        "Assistant history is invalid; the existing file was preserved.".to_owned()
                    })?)
                    .map_err(|_| {
                        "Assistant history is invalid; the existing file was preserved.".to_owned()
                    })?;
                validate_snapshot(&snapshot, version == Some(1))?;
                snapshot
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                AssistantConversationState::default()
            }
            Err(_) => return Err("Could not read assistant history.".into()),
        };
        Ok(Self(Arc::new(Inner {
            path,
            state: Mutex::new(State {
                snapshot,
                generation: 0,
                persisted: 0,
                active: false,
                failure: None,
            }),
            changed: Condvar::new(),
        })))
    }

    pub(super) fn snapshot(&self) -> Result<AssistantConversationState, String> {
        self.0
            .state
            .lock()
            .map(|state| state.snapshot.clone())
            .map_err(|_| "Assistant history is unavailable.".into())
    }

    pub(super) fn write_pending(&self) -> Result<bool, String> {
        let state = self
            .0
            .state
            .lock()
            .map_err(|_| "Assistant history is unavailable.".to_owned())?;
        if state.active {
            Ok(true)
        } else if let Some(error) = &state.failure {
            Err(error.clone())
        } else if state.persisted != state.generation {
            Err("Assistant history could not be saved.".into())
        } else {
            Ok(false)
        }
    }

    pub(super) fn queue(&self, snapshot: AssistantConversationState) -> Result<(), String> {
        let mut state = self
            .0
            .state
            .lock()
            .map_err(|_| "Assistant history is unavailable.".to_owned())?;
        state.generation = state
            .generation
            .checked_add(1)
            .ok_or_else(|| "Assistant history is unavailable.".to_owned())?;
        state.snapshot = snapshot;
        state.failure = None;
        if !state.active {
            state.active = true;
            let inner = self.0.clone();
            std::thread::Builder::new()
                .name("parchmint-assistant-history".into())
                .spawn(move || {
                    let recovery = inner.clone();
                    if std::panic::catch_unwind(|| write_pending(inner)).is_err()
                        && let Ok(mut state) = recovery.state.lock()
                    {
                        state.failure =
                            Some("Assistant history worker stopped unexpectedly.".into());
                        state.active = false;
                        recovery.changed.notify_all();
                    }
                })
                .map_err(|_| {
                    state.active = false;
                    "Could not start the assistant history worker.".to_owned()
                })?;
        }
        Ok(())
    }

    /// Waits on a worker, never from the UI thread. A failed save leaves the
    /// prior file untouched and prevents a paid request or successful close.
    pub(super) fn flush(&self) -> Result<(), String> {
        let mut state = self
            .0
            .state
            .lock()
            .map_err(|_| "Assistant history is unavailable.".to_owned())?;
        while state.active {
            state = self
                .0
                .changed
                .wait(state)
                .map_err(|_| "Assistant history is unavailable.".to_owned())?;
        }
        if let Some(error) = &state.failure {
            return Err(error.clone());
        }
        if state.persisted != state.generation {
            return Err("Assistant history could not be saved.".into());
        }
        Ok(())
    }
}

fn read_history(reader: impl Read) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read assistant history.".to_owned())?;
    if bytes.len() > MAX_BYTES {
        return Err(
            "Assistant history is too large to open; the existing file was preserved.".into(),
        );
    }
    Ok(bytes)
}

fn validate_snapshot(snapshot: &AssistantConversationState, legacy: bool) -> Result<(), String> {
    let invalid = || {
        "Assistant history has invalid conversations; the existing file was preserved.".to_owned()
    };
    let mut modes = std::collections::BTreeSet::new();
    let mut active_modes = std::collections::BTreeSet::new();
    let mut ids = std::collections::BTreeSet::new();
    let mut imports = std::collections::BTreeSet::new();
    for thread in &snapshot.threads {
        let mode = match thread.mode {
            parchmint_assistant_api::AssistantMode::Ask => 0,
            parchmint_assistant_api::AssistantMode::Check => 1,
            parchmint_assistant_api::AssistantMode::Research => 2,
        };
        if legacy {
            if !modes.insert(mode) {
                return Err(invalid());
            }
            continue;
        }
        modes.insert(mode);
        if thread.id.is_empty()
            || thread.id.len() > 128
            || thread.title.trim().is_empty()
            || thread.title.chars().count() > 80
            || !ids.insert(&thread.id)
            || (thread.active && !active_modes.insert(mode))
            || (mode != 2 && thread.research_import.is_some())
            || thread
                .research_import
                .as_ref()
                .is_some_and(|draft| !imports.insert(draft.nonce))
        {
            return Err(invalid());
        }
    }
    if !legacy && (snapshot.research_import.is_some() || modes != active_modes) {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(windows)]
fn hash_path(digest: &mut Sha256, path: &Path) {
    use std::os::windows::ffi::OsStrExt;
    for unit in path.as_os_str().encode_wide() {
        digest.update(unit.to_le_bytes());
    }
}

#[cfg(unix)]
fn hash_path(digest: &mut Sha256, path: &Path) {
    use std::os::unix::ffi::OsStrExt;
    digest.update(path.as_os_str().as_bytes());
}

fn write_pending(inner: Arc<Inner>) {
    loop {
        let (generation, snapshot) = match inner.state.lock() {
            Ok(state) => (state.generation, state.snapshot.clone()),
            Err(_) => return,
        };
        let result = write_snapshot(&inner.path, generation, &snapshot);
        let Ok(mut state) = inner.state.lock() else {
            return;
        };
        match result {
            Ok(()) => state.persisted = generation,
            Err(error) => {
                // A newer, smaller snapshot may have arrived while this one
                // was being written. Give that generation its own attempt.
                if state.generation != generation {
                    continue;
                }
                state.failure = Some(error);
                state.active = false;
                inner.changed.notify_all();
                return;
            }
        }
        if state.generation == generation {
            state.active = false;
            inner.changed.notify_all();
            return;
        }
    }
}

fn write_snapshot(
    path: &Path,
    generation: u64,
    snapshot: &AssistantConversationState,
) -> Result<(), String> {
    let fail = || "Could not durably save assistant history.".to_owned();
    let parent = path.parent().ok_or_else(fail)?;
    validate_snapshot(snapshot, false)?;
    fs::create_dir_all(parent).map_err(|_| fail())?;
    restrict_directory(parent).map_err(|_| fail())?;
    let bytes = serde_json::to_vec(&serde_json::json!({"version": 2, "state": snapshot}))
        .map_err(|_| fail())?;
    if bytes.len() > MAX_BYTES {
        return Err(
            "Assistant history exceeds the local size limit; the previous copy was preserved."
                .into(),
        );
    }
    let temporary = parent.join(format!(
        ".assistant-history-{}-{generation}-{}.tmp",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        restrict_file(&mut options);
        let mut file = options.open(&temporary).map_err(|_| fail())?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| fail())?;
        drop(file);
        fs::rename(&temporary, path).map_err(|_| fail())?;
        sync_directory(parent).map_err(|_| fail())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

#[cfg(unix)]
fn restrict_directory(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn restrict_directory(path: &Path) -> std::io::Result<()> {
    fs::metadata(path).map(|_| ())
}

#[cfg(unix)]
fn restrict_file(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(not(unix))]
fn restrict_file(_options: &mut OpenOptions) {}

#[cfg(unix)]
fn sync_directory(path: &Path) -> std::io::Result<()> {
    fs::File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(path: &Path) -> std::io::Result<()> {
    fs::metadata(path).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use parchmint_assistant_api::AssistantMode;
    use parchmint_ui_api::AssistantSavedThread;

    #[test]
    fn coalesced_drafts_survive_reopen_and_do_not_follow_project_copies() {
        let root = std::env::temp_dir().join(format!(
            "parchmint-assistant-history-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let project = root.join("project");
        let copy = root.join("copy");
        let data = root.join("data");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(&copy).unwrap();
        let store = ConversationStore::open(&data, b"same-project-id", &project).unwrap();
        for index in 0..20 {
            store
                .queue(AssistantConversationState {
                    threads: vec![AssistantSavedThread {
                        mode: AssistantMode::Ask,
                        id: "test-ask".into(),
                        title: "Test".into(),
                        auto_title: false,
                        active: true,
                        research_import: None,
                        draft: format!("draft {index}"),
                        turns: Vec::new(),
                        note_proposal: None,
                        interrupted_prompts: Vec::new(),
                        pinned_context: Vec::new(),
                        excluded_context: Vec::new(),
                        context_scope: None,
                        selection_only: false,
                    }],
                    ..Default::default()
                })
                .unwrap();
        }
        store.flush().unwrap();
        let path = store.0.path.clone();
        drop(store);
        let reopened = ConversationStore::open(&data, b"same-project-id", &project).unwrap();
        assert_eq!(reopened.snapshot().unwrap().threads[0].draft, "draft 19");
        assert!(
            ConversationStore::open(&data, b"same-project-id", &copy)
                .unwrap()
                .snapshot()
                .unwrap()
                .threads
                .is_empty()
        );
        drop(reopened);
        fs::write(&path, b"invalid history").unwrap();
        assert!(ConversationStore::open(&data, b"same-project-id", &project).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"invalid history");
        fs::remove_file(path).unwrap();
        fs::remove_dir(data).unwrap();
        fs::remove_dir(project).unwrap();
        fs::remove_dir(copy).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn older_saved_threads_default_to_automatic_context() {
        let state: AssistantConversationState = serde_json::from_value(serde_json::json!({
            "threads": [{
                "mode": "Ask",
                "draft": "question",
                "turns": [],
                "note_proposal": null,
                "interrupted_prompts": []
            }]
        }))
        .unwrap();
        assert!(state.threads[0].pinned_context.is_empty());
        assert!(state.threads[0].excluded_context.is_empty());
        assert!(state.threads[0].context_scope.is_none());
        assert!(!state.threads[0].selection_only);
        assert!(state.threads[0].id.is_empty());
        assert!(validate_snapshot(&state, true).is_ok());
        assert!(validate_snapshot(&state, false).is_err());
    }

    #[test]
    fn reads_version_one_history_and_preserves_invalid_version_two() {
        let root = std::env::temp_dir().join(format!(
            "parchmint-assistant-migration-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let project = root.join("project");
        let data = root.join("data");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(&data).unwrap();
        let store = ConversationStore::open(&data, b"project", &project).unwrap();
        let path = store.0.path.clone();
        drop(store);
        let old = serde_json::json!({"version":1,"state":{"threads":[{
            "mode":"Ask","draft":"old draft","turns":[],"note_proposal":null,"interrupted_prompts":[]
        }]}});
        fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
        let reopened = ConversationStore::open(&data, b"project", &project).unwrap();
        assert_eq!(reopened.snapshot().unwrap().threads[0].draft, "old draft");
        drop(reopened);
        let invalid = serde_json::json!({"version":2,"state":{"threads":[{
            "mode":"Ask","id":"same","title":"A","active":true,"draft":"first","turns":[],"note_proposal":null,"interrupted_prompts":[]
        },{
            "mode":"Ask","id":"same","title":"B","active":false,"draft":"second","turns":[],"note_proposal":null,"interrupted_prompts":[]
        }]}});
        let bytes = serde_json::to_vec(&invalid).unwrap();
        fs::write(&path, &bytes).unwrap();
        assert!(ConversationStore::open(&data, b"project", &project).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        fs::remove_file(path).unwrap();
        fs::remove_dir(data).unwrap();
        fs::remove_dir(project).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn older_saved_results_load_without_original_provenance_fields() {
        let result: parchmint_ui_api::AssistantWorkflowResult = serde_json::from_value(serde_json::json!({
            "response": {"text":"Answer", "project_citations":[], "unverified_project_citations":[], "web_citations":[], "tool_calls":[], "usage":null},
            "context":[{"source_id":"S1", "document_id":"doc", "document_revision":1, "section":"Lore", "title":"Lore", "anchor":null, "text":"passage"}],
            "context_truncated":false,
            "estimated_cost_micros":null,
            "estimated_period_total_micros":null,
            "usage_warning":null,
            "web_retrieved_at_unix_seconds":null,
            "provider_binding":"provider",
            "notes_derived":false
        })).unwrap();
        assert!(result.context_warnings.is_empty());
        assert!(result.context[0].original.is_none());
        assert!(result.check_report.is_none());
        assert!(!result.check_report_invalid);
    }

    #[test]
    fn oversized_history_preserves_previous_copy_and_can_be_retried() {
        let root = std::env::temp_dir().join(format!(
            "parchmint-assistant-history-limit-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let project = root.join("project");
        let data = root.join("data");
        fs::create_dir_all(&project).unwrap();
        let store = ConversationStore::open(&data, b"project", &project).unwrap();
        let saved = |draft: String| AssistantConversationState {
            threads: vec![AssistantSavedThread {
                mode: AssistantMode::Ask,
                id: "test-ask".into(),
                title: "Test".into(),
                auto_title: false,
                active: true,
                research_import: None,
                draft,
                turns: Vec::new(),
                note_proposal: None,
                interrupted_prompts: Vec::new(),
                pinned_context: Vec::new(),
                excluded_context: Vec::new(),
                context_scope: None,
                selection_only: false,
            }],
            ..Default::default()
        };
        store.queue(saved("safe draft".into())).unwrap();
        store.flush().unwrap();
        let original = fs::read(&store.0.path).unwrap();
        store.queue(saved("x".repeat(MAX_BYTES))).unwrap();
        assert!(store.flush().is_err());
        assert!(store.write_pending().is_err());
        assert_eq!(fs::read(&store.0.path).unwrap(), original);
        store.queue(saved("recovered draft".into())).unwrap();
        store.flush().unwrap();
        assert_eq!(store.write_pending(), Ok(false));
        assert_eq!(
            ConversationStore::open(&data, b"project", &project)
                .unwrap()
                .snapshot()
                .unwrap()
                .threads[0]
                .draft,
            "recovered draft"
        );
        fs::remove_file(&store.0.path).unwrap();
        fs::remove_dir(data).unwrap();
        fs::remove_dir(project).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn oversized_history_is_read_only_up_to_the_limit_and_preserved() {
        struct CountingReader(usize);
        impl Read for CountingReader {
            fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
                self.0 += output.len();
                output.fill(b'x');
                Ok(output.len())
            }
        }
        let mut reader = CountingReader(0);
        assert!(read_history(&mut reader).unwrap_err().contains("too large"));
        assert_eq!(reader.0, MAX_BYTES + 1);

        let root = std::env::temp_dir().join(format!(
            "parchmint-assistant-oversized-load-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let project = root.join("project");
        let data = root.join("data");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(&data).unwrap();
        let store = ConversationStore::open(&data, b"project", &project).unwrap();
        let path = store.0.path.clone();
        drop(store);
        let mut file = fs::File::create(&path).unwrap();
        file.write_all(b"preserve").unwrap();
        file.set_len(MAX_BYTES as u64 + 1).unwrap();
        drop(file);
        assert!(
            ConversationStore::open(&data, b"project", &project)
                .err()
                .unwrap()
                .contains("too large")
        );
        assert_eq!(fs::metadata(&path).unwrap().len(), MAX_BYTES as u64 + 1);
        let mut prefix = [0; 8];
        fs::File::open(&path)
            .unwrap()
            .read_exact(&mut prefix)
            .unwrap();
        assert_eq!(&prefix, b"preserve");
        fs::remove_file(path).unwrap();
        fs::remove_dir(data).unwrap();
        fs::remove_dir(project).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
