#![allow(missing_docs)] // Revisioned worker protocol is documented in the Stage 14 handoff.
//! Project-facing search/cache coordination and Unicode-aware text statistics.

use parchmint_domain::{DocumentId, NodeId, Project};
use parchmint_index::{
    CountTotals, IndexDocument, IndexError, SearchIndex, SearchQuery, SearchResult,
};
use parchmint_storage::{DocumentBodySnapshot, OpenMode, OpenProject, ProjectStorage};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, TryRecvError},
};
use std::thread::{self, JoinHandle};
use thiserror::Error;

/// Derived count rules shared by document, selection, and aggregate displays.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TextStatistics {
    /// Unicode word count. A word is a run containing at least one Unicode
    /// letter or number; apostrophes and hyphens join runs only between words.
    pub words: u64,
    /// Unicode scalar values, including whitespace and punctuation.
    pub characters: u64,
}

/// Counts source text without locale-sensitive platform APIs.
pub fn text_statistics(text: &str) -> TextStatistics {
    let characters = u64::try_from(text.chars().count()).unwrap_or(u64::MAX);
    let mut words = 0_u64;
    let mut in_word = false;
    for character in text.chars() {
        if character.is_alphanumeric() {
            if !in_word {
                words = words.saturating_add(1);
                in_word = true;
            }
        } else if character != '\'' && character != '’' && character != '-' && character != '‑'
        {
            in_word = false;
        }
    }
    TextStatistics { words, characters }
}

/// Availability exposed to UI without a blocking dialog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IndexStatus {
    /// The cache has not scanned canonical bodies yet.
    RebuildNeeded,
    /// A worker is publishing stable, revisioned SQLite batches.
    Indexing {
        revision: u64,
        completed: u64,
        total: u64,
    },
    /// Rows reflect the current workspace mutations.
    Ready,
    /// SQLite was unavailable; canonical project access remains normal.
    Unavailable(String),
}

/// A disposable cache owned by one open project.  It deliberately stores no
/// canonical state, so failed opens and rebuilds degrade search only.
pub struct ProjectSearch {
    path: PathBuf,
    index: Option<SearchIndex>,
    status: IndexStatus,
}

impl ProjectSearch {
    /// Opens the small cache metadata database but does not scan document bodies.
    pub fn open(root: &Path) -> Self {
        let path = root.join(".parchmint/index.sqlite");
        match SearchIndex::open(&path) {
            Ok(index) => Self {
                path,
                index: Some(index),
                status: IndexStatus::RebuildNeeded,
            },
            Err(error) => Self {
                path,
                index: None,
                status: IndexStatus::Unavailable(error.to_string()),
            },
        }
    }

    /// Cache path, useful for diagnostics and explicit recovery tooling.
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Current non-disruptive cache availability.
    pub fn status(&self) -> &IndexStatus {
        &self.status
    }

    /// Whether incremental rows may be safely applied without first rebuilding.
    pub fn is_ready(&self) -> bool {
        self.status == IndexStatus::Ready
    }

    pub fn set_indexing(&mut self, revision: u64, completed: u64, total: u64) {
        self.status = IndexStatus::Indexing {
            revision,
            completed,
            total,
        };
    }

    pub fn set_rebuild_needed(&mut self) {
        self.status = IndexStatus::RebuildNeeded;
    }

    pub fn set_ready(&mut self) {
        self.status = IndexStatus::Ready;
    }

    pub fn set_unavailable(&mut self, message: String) {
        self.status = IndexStatus::Unavailable(message);
    }

    /// Rebuilds only from the currently opened canonical project data. This is
    /// intentionally callable by a background worker; it never writes source files.
    pub fn rebuild(&mut self, opened: &OpenProject) -> Result<(), SearchServiceError> {
        let documents = opened
            .project
            .documents
            .values()
            .filter(|record| !opened.project.is_trashed(record.node_id))
            .map(|record| indexed_document(opened, record.node_id))
            .collect::<Result<Vec<_>, _>>()?;
        let Some(index) = self.index.as_mut() else {
            return Err(SearchServiceError::Unavailable(self.status.clone()));
        };
        index.rebuild_documents(documents.iter().map(OwnedIndexDocument::borrow))?;
        self.status = IndexStatus::Ready;
        Ok(())
    }

    /// Updates one saved document/metadata row after canonical persistence.
    pub fn upsert_node(
        &mut self,
        opened: &OpenProject,
        node: NodeId,
    ) -> Result<(), SearchServiceError> {
        if !self.is_ready() {
            return Ok(());
        }
        let Some(index) = self.index.as_mut() else {
            return Err(SearchServiceError::Unavailable(self.status.clone()));
        };
        if opened.project.is_trashed(node) {
            index.delete(&node.to_string())?;
            return Ok(());
        }
        let document = indexed_document(opened, node)?;
        index.upsert_document(document.borrow())?;
        self.status = IndexStatus::Ready;
        Ok(())
    }

    /// Removes all rows in a detached subtree. Restoring invokes `upsert_node`
    /// for each document in the restored subtree.
    pub fn delete_subtree(
        &mut self,
        project: &Project,
        root: NodeId,
    ) -> Result<(), SearchServiceError> {
        if !self.is_ready() {
            return Ok(());
        }
        let Some(index) = self.index.as_mut() else {
            return Err(SearchServiceError::Unavailable(self.status.clone()));
        };
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            index.delete(&node.to_string())?;
            if let Some(entry) = project.nodes.get(&node) {
                stack.extend(entry.children.iter().copied());
            }
        }
        self.status = IndexStatus::Ready;
        Ok(())
    }

    /// Streams a stable prefix of ranked results. A UI worker should request
    /// increasing limits to append batches while retaining the current query.
    pub fn search(
        &mut self,
        _opened: &OpenProject,
        query: &SearchQuery<'_>,
        limit: u32,
    ) -> Result<Vec<SearchResult>, SearchServiceError> {
        if matches!(
            self.status,
            IndexStatus::RebuildNeeded | IndexStatus::Indexing { completed: 0, .. }
        ) {
            return Ok(Vec::new());
        }
        self.index
            .as_ref()
            .ok_or_else(|| SearchServiceError::Unavailable(self.status.clone()))?
            .search_detailed(query, limit)
            .map_err(Into::into)
    }

    /// Returns totals from the currently published cache prefix. It never
    /// triggers a synchronous rebuild.
    pub fn totals(
        &mut self,
        _opened: &OpenProject,
        scope: Option<&str>,
        subtree: Option<NodeId>,
    ) -> Result<CountTotals, SearchServiceError> {
        if matches!(
            self.status,
            IndexStatus::RebuildNeeded | IndexStatus::Indexing { completed: 0, .. }
        ) {
            return Ok(CountTotals::default());
        }
        self.index
            .as_ref()
            .ok_or_else(|| SearchServiceError::Unavailable(self.status.clone()))?
            .totals(scope, subtree.map(|id| id.to_string()).as_deref())
            .map_err(Into::into)
    }
}

/// Immutable canonical handles captured at project open. Body clones are
/// `Arc` bumps, so scheduling a rebuild does not duplicate the 10M-word corpus
/// on the UI thread.
#[derive(Clone)]
pub struct SearchRebuildSnapshot {
    project: Project,
    bodies: BTreeMap<DocumentId, DocumentBodySnapshot>,
}

impl SearchRebuildSnapshot {
    pub fn from_opened(opened: &OpenProject) -> Self {
        Self {
            project: opened.project.clone(),
            bodies: opened.body_snapshot(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SearchRebuildProgress {
    Batch {
        revision: u64,
        completed: u64,
        total: u64,
    },
    Counts {
        revision: u64,
        rows: Vec<IndexCountRow>,
    },
    Complete {
        revision: u64,
        total: u64,
    },
    Cancelled {
        revision: u64,
    },
    Failed {
        revision: u64,
        message: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IndexCountRow {
    pub node: NodeId,
    pub document: Option<DocumentId>,
    pub document_words: u64,
    pub document_characters: u64,
    pub subtree_words: u64,
    pub subtree_characters: u64,
}

/// One cancellable cache builder per open project. SQLite writes and body
/// normalization occur exclusively on this worker.
pub struct SearchIndexWorker {
    progress: Receiver<SearchRebuildProgress>,
    cancellation: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

enum SearchRebuildSource {
    Snapshot(SearchRebuildSnapshot),
    Canonical(PathBuf),
}

impl SearchIndexWorker {
    pub fn start(
        path: PathBuf,
        snapshot: SearchRebuildSnapshot,
        revision: u64,
    ) -> Result<Self, SearchServiceError> {
        Self::start_source(path, SearchRebuildSource::Snapshot(snapshot), revision)
    }

    /// Starts an open-time rebuild that opens canonical headers and bodies on
    /// the worker, avoiding a full `Project` clone on the owner thread.
    pub fn start_canonical(
        path: PathBuf,
        project_root: PathBuf,
        revision: u64,
    ) -> Result<Self, SearchServiceError> {
        Self::start_source(path, SearchRebuildSource::Canonical(project_root), revision)
    }

    fn start_source(
        path: PathBuf,
        source: SearchRebuildSource,
        revision: u64,
    ) -> Result<Self, SearchServiceError> {
        let (sender, progress) = mpsc::channel();
        let cancellation = Arc::new(AtomicBool::new(false));
        let worker_cancellation = Arc::clone(&cancellation);
        let worker = thread::Builder::new()
            .name("parchmint-index".into())
            .spawn(move || {
                let result = match source {
                    SearchRebuildSource::Snapshot(snapshot) => {
                        rebuild_snapshot(&path, &snapshot, revision, &worker_cancellation, &sender)
                    }
                    SearchRebuildSource::Canonical(root) => {
                        ProjectStorage::open(root, OpenMode::ReadOnly)
                            .map_err(SearchServiceError::from)
                            .and_then(|opened| {
                                let bodies = opened.body_snapshot();
                                rebuild_project(
                                    &path,
                                    &opened.project,
                                    &bodies,
                                    revision,
                                    &worker_cancellation,
                                    &sender,
                                )
                            })
                    }
                };
                let terminal = match result {
                    Ok(total) => SearchRebuildProgress::Complete { revision, total },
                    Err(SearchServiceError::Cancelled) => {
                        SearchRebuildProgress::Cancelled { revision }
                    }
                    Err(error) => SearchRebuildProgress::Failed {
                        revision,
                        message: error.to_string(),
                    },
                };
                let _ = sender.send(terminal);
            })
            .map_err(SearchServiceError::SpawnWorker)?;
        Ok(Self {
            progress,
            cancellation,
            worker: Some(worker),
        })
    }

    pub fn try_progress(&self) -> Result<Option<SearchRebuildProgress>, SearchServiceError> {
        match self.progress.try_recv() {
            Ok(progress) => Ok(Some(progress)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(SearchServiceError::WorkerDisconnected),
        }
    }

    pub fn cancel(&self) {
        self.cancellation.store(true, Ordering::Release);
    }
}

impl Drop for SearchIndexWorker {
    fn drop(&mut self) {
        self.cancel();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn rebuild_snapshot(
    path: &Path,
    snapshot: &SearchRebuildSnapshot,
    revision: u64,
    cancellation: &AtomicBool,
    progress: &std::sync::mpsc::Sender<SearchRebuildProgress>,
) -> Result<u64, SearchServiceError> {
    rebuild_project(
        path,
        &snapshot.project,
        &snapshot.bodies,
        revision,
        cancellation,
        progress,
    )
}

fn rebuild_project(
    path: &Path,
    project: &Project,
    bodies: &BTreeMap<DocumentId, DocumentBodySnapshot>,
    revision: u64,
    cancellation: &AtomicBool,
    progress: &std::sync::mpsc::Sender<SearchRebuildProgress>,
) -> Result<u64, SearchServiceError> {
    let mut all_nodes = Vec::new();
    let mut nodes = Vec::new();
    let mut pending = project.roots.to_vec();
    while let Some(node) = pending.pop() {
        if cancellation.load(Ordering::Acquire) {
            return Err(SearchServiceError::Cancelled);
        }
        let Some(entry) = project.nodes.get(&node) else {
            continue;
        };
        all_nodes.push(node);
        pending.extend(entry.children.iter().rev().copied());
        if entry.kind.document_id().is_some() {
            nodes.push(node);
        }
    }
    let total = u64::try_from(nodes.len()).unwrap_or(u64::MAX);
    let mut index = SearchIndex::open(path)?;
    index.begin_rebuild(revision, total)?;
    let mut completed = 0u64;
    let mut direct_counts = BTreeMap::<NodeId, TextStatistics>::new();
    for chunk in nodes.chunks(64) {
        let mut documents = Vec::with_capacity(chunk.len());
        for node in chunk {
            if cancellation.load(Ordering::Acquire) {
                return Err(SearchServiceError::Cancelled);
            }
            let document = project.nodes[node]
                .kind
                .document_id()
                .ok_or(SearchServiceError::MissingNode(*node))?;
            let body = bodies
                .get(&document)
                .ok_or(SearchServiceError::MissingNode(*node))?
                .load()?;
            let owned = indexed_project_document(project, *node, &body, || {
                cancellation.load(Ordering::Acquire)
            })
            .ok_or(SearchServiceError::Cancelled)?;
            direct_counts.insert(
                *node,
                TextStatistics {
                    words: owned.word_count,
                    characters: owned.character_count,
                },
            );
            documents.push(owned);
        }
        completed = completed.saturating_add(u64::try_from(documents.len()).unwrap_or(u64::MAX));
        index.upsert_documents_batch(
            documents.iter().map(OwnedIndexDocument::borrow),
            revision,
            completed,
        )?;
        let _ = progress.send(SearchRebuildProgress::Batch {
            revision,
            completed,
            total,
        });
    }
    index.finish_rebuild(revision, total)?;
    let mut aggregates = direct_counts.clone();
    for node in all_nodes.iter().rev() {
        let child = aggregates.get(node).copied().unwrap_or_default();
        if let Some(parent) = project.nodes.get(node).and_then(|entry| entry.parent) {
            let aggregate = aggregates.entry(parent).or_default();
            aggregate.words = aggregate.words.saturating_add(child.words);
            aggregate.characters = aggregate.characters.saturating_add(child.characters);
        }
    }
    for chunk in all_nodes.chunks(64) {
        if cancellation.load(Ordering::Acquire) {
            return Err(SearchServiceError::Cancelled);
        }
        let rows = chunk
            .iter()
            .map(|node| {
                let direct = direct_counts.get(node).copied().unwrap_or_default();
                let subtree = aggregates.get(node).copied().unwrap_or_default();
                IndexCountRow {
                    node: *node,
                    document: project.nodes[node].kind.document_id(),
                    document_words: direct.words,
                    document_characters: direct.characters,
                    subtree_words: subtree.words,
                    subtree_characters: subtree.characters,
                }
            })
            .collect();
        let _ = progress.send(SearchRebuildProgress::Counts { revision, rows });
    }
    Ok(total)
}

#[derive(Clone, Debug)]
struct OwnedIndexDocument {
    node_id: String,
    document_id: String,
    scope: String,
    title: String,
    synopsis: String,
    body: String,
    path: String,
    fingerprint_bytes: u64,
    fingerprint_hash: i64,
    status: String,
    labels: String,
    tags: String,
    hierarchy: String,
    word_count: u64,
    character_count: u64,
}

impl OwnedIndexDocument {
    fn borrow(&self) -> IndexDocument<'_> {
        IndexDocument {
            node_id: &self.node_id,
            document_id: &self.document_id,
            scope: &self.scope,
            title: &self.title,
            synopsis: &self.synopsis,
            body: &self.body,
            path: &self.path,
            fingerprint_bytes: self.fingerprint_bytes,
            fingerprint_hash: self.fingerprint_hash,
            status: &self.status,
            labels: &self.labels,
            tags: &self.tags,
            hierarchy: &self.hierarchy,
            word_count: self.word_count,
            character_count: self.character_count,
        }
    }
}

fn indexed_document(
    opened: &OpenProject,
    node: NodeId,
) -> Result<OwnedIndexDocument, SearchServiceError> {
    let document_id = opened
        .project
        .nodes
        .get(&node)
        .and_then(|entry| entry.kind.document_id())
        .ok_or(SearchServiceError::MissingNode(node))?;
    indexed_project_document(&opened.project, node, opened.body(document_id)?, || false)
        .ok_or(SearchServiceError::Cancelled)
}

fn indexed_project_document(
    project: &Project,
    node: NodeId,
    body: &str,
    cancelled: impl Fn() -> bool,
) -> Option<OwnedIndexDocument> {
    let document_id = project
        .nodes
        .get(&node)
        .and_then(|entry| entry.kind.document_id())?;
    let record = project.documents.get(&document_id)?;
    let (normalized, statistics) = normalize_markdown(body, &cancelled)?;
    let mut ancestry = Vec::new();
    let mut current = Some(node);
    while let Some(id) = current {
        if cancelled() {
            return None;
        }
        ancestry.push(id.to_string());
        current = project.nodes.get(&id).and_then(|entry| entry.parent);
    }
    ancestry.reverse();
    let scope = if ancestry
        .first()
        .is_some_and(|id| *id == project.research_root().to_string())
    {
        "research"
    } else {
        "manuscript"
    };
    Some(OwnedIndexDocument {
        node_id: node.to_string(),
        document_id: document_id.to_string(),
        scope: scope.into(),
        title: record.metadata.title.clone(),
        synopsis: record.metadata.summary.clone(),
        body: normalized,
        path: record.path.as_str().to_owned(),
        fingerprint_bytes: u64::try_from(body.len()).unwrap_or(u64::MAX),
        fingerprint_hash: fnv1a(body),
        status: record.metadata.status.clone().unwrap_or_default(),
        labels: record.metadata.labels.join(" "),
        tags: record.metadata.tags.join(" "),
        hierarchy: ancestry.join("|"),
        word_count: statistics.words,
        character_count: statistics.characters,
    })
}

fn normalize_markdown(
    body: &str,
    cancelled: &impl Fn() -> bool,
) -> Option<(String, TextStatistics)> {
    // Keep user-facing Unicode letters/numbers and whitespace. Markdown syntax,
    // link punctuation, and formatting delimiters are deliberately not tokens.
    let mut normalized = String::with_capacity(body.len());
    let mut words = 0u64;
    let mut characters = 0u64;
    let mut in_word = false;
    for (index, character) in body.chars().enumerate() {
        if index % 4_096 == 0 && cancelled() {
            return None;
        }
        characters = characters.saturating_add(1);
        let alphanumeric = character.is_alphanumeric();
        let joiner = matches!(character, '\'' | '’' | '-' | '‑');
        if alphanumeric {
            if !in_word {
                words = words.saturating_add(1);
            }
            in_word = true;
        } else if !joiner {
            in_word = false;
        }
        normalized.push(if alphanumeric || character.is_whitespace() || joiner {
            character
        } else {
            ' '
        });
    }
    Some((normalized, TextStatistics { words, characters }))
}

fn fnv1a(source: &str) -> i64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in source.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash.cast_signed()
}

/// Search/cache service error. It never indicates damaged canonical data.
#[derive(Debug, Error)]
pub enum SearchServiceError {
    /// A document node was absent from the current project projection.
    #[error("search document node is absent: {0}")]
    MissingNode(NodeId),
    /// Canonical storage could not supply a body.
    #[error(transparent)]
    Storage(#[from] parchmint_storage::StorageError),
    /// Disposable SQLite cache error.
    #[error(transparent)]
    Index(#[from] IndexError),
    /// Search is degraded but project editing remains available.
    #[error("search is unavailable: {0:?}")]
    Unavailable(IndexStatus),
    /// Cooperative background cache cancellation.
    #[error("search indexing was cancelled")]
    Cancelled,
    /// Worker thread creation failed.
    #[error("could not start search-index worker: {0}")]
    SpawnWorker(std::io::Error),
    /// The index worker ended before publishing a terminal state.
    #[error("search-index worker disconnected")]
    WorkerDisconnected,
}

#[cfg(test)]
mod tests {
    use super::*;
    use parchmint_domain::{DocumentMetadata, DocumentRecord, Node, NodeKind, RelativeProjectPath};
    use std::time::{Duration, Instant};

    #[test]
    fn unicode_word_rules_are_platform_independent() {
        assert_eq!(
            text_statistics("Café—東京 can't 42"),
            TextStatistics {
                words: 4,
                characters: 16
            }
        );
        assert_eq!(
            text_statistics("  --  "),
            TextStatistics {
                words: 0,
                characters: 6
            }
        );
    }

    #[test]
    fn worker_publishes_a_bounded_first_batch_and_cancels_promptly() {
        let directory = tempfile::tempdir().unwrap();
        let mut project = Project::new("Index worker");
        let root = project.manuscript_root();
        let body: Arc<str> = Arc::from("winter orchard harbor ".repeat(1_000));
        let mut bodies = BTreeMap::new();
        for index in 0..128 {
            let node = NodeId::new();
            let document = DocumentId::new();
            project.nodes.get_mut(&root).unwrap().children.push(node);
            project.nodes.insert(
                node,
                Node {
                    id: node,
                    kind: NodeKind::Document {
                        document_id: document,
                    },
                    parent: Some(root),
                    children: Vec::new(),
                },
            );
            project.documents.insert(
                document,
                DocumentRecord {
                    id: document,
                    node_id: node,
                    path: RelativeProjectPath::new(format!("manuscript/{node}.md")).unwrap(),
                    metadata: DocumentMetadata {
                        title: format!("Scene {index}"),
                        ..DocumentMetadata::default()
                    },
                },
            );
            bodies.insert(
                document,
                DocumentBodySnapshot::from_body(document, Arc::clone(&body)),
            );
        }
        let started = Instant::now();
        let worker = SearchIndexWorker::start(
            directory.path().join("index.sqlite"),
            SearchRebuildSnapshot { project, bodies },
            7,
        )
        .unwrap();
        loop {
            if matches!(
                worker.try_progress().unwrap(),
                Some(SearchRebuildProgress::Batch {
                    revision: 7,
                    completed: 64,
                    total: 128
                })
            ) {
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(2));
            std::thread::yield_now();
        }
        if !cfg!(debug_assertions) {
            assert!(started.elapsed() < Duration::from_millis(300));
        }
        let cancelled = Instant::now();
        worker.cancel();
        drop(worker);
        assert!(
            cancelled.elapsed() < Duration::from_millis(250),
            "worker cancellation took {:?}",
            cancelled.elapsed()
        );
    }
}
