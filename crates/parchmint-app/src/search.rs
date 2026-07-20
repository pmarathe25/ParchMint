//! Project-facing search/cache coordination and Unicode-aware text statistics.

use parchmint_domain::{NodeId, Project};
use parchmint_index::{
    CountTotals, IndexDocument, IndexError, SearchIndex, SearchQuery, SearchResult,
};
use parchmint_storage::OpenProject;
use std::path::{Path, PathBuf};
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
        opened: &OpenProject,
        query: &SearchQuery<'_>,
        limit: u32,
    ) -> Result<Vec<SearchResult>, SearchServiceError> {
        if self.status == IndexStatus::RebuildNeeded {
            self.rebuild(opened)?;
        }
        self.index
            .as_ref()
            .ok_or_else(|| SearchServiceError::Unavailable(self.status.clone()))?
            .search_detailed(query, limit)
            .map_err(Into::into)
    }

    /// Returns stored aggregate counts. It performs an initial rebuild only if
    /// necessary, and never reparses bodies after the cache is ready.
    pub fn totals(
        &mut self,
        opened: &OpenProject,
        scope: Option<&str>,
        subtree: Option<NodeId>,
    ) -> Result<CountTotals, SearchServiceError> {
        if self.status == IndexStatus::RebuildNeeded {
            self.rebuild(opened)?;
        }
        self.index
            .as_ref()
            .ok_or_else(|| SearchServiceError::Unavailable(self.status.clone()))?
            .totals(scope, subtree.map(|id| id.to_string()).as_deref())
            .map_err(Into::into)
    }
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
    let project = &opened.project;
    let document_id = project
        .nodes
        .get(&node)
        .and_then(|entry| entry.kind.document_id())
        .ok_or(SearchServiceError::MissingNode(node))?;
    let record = project
        .documents
        .get(&document_id)
        .ok_or(SearchServiceError::MissingNode(node))?;
    let body = opened.body(document_id)?.to_owned();
    let statistics = text_statistics(&body);
    let mut ancestry = Vec::new();
    let mut current = Some(node);
    while let Some(id) = current {
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
    Ok(OwnedIndexDocument {
        node_id: node.to_string(),
        document_id: document_id.to_string(),
        scope: scope.into(),
        title: record.metadata.title.clone(),
        synopsis: record.metadata.summary.clone(),
        body: normalize_markdown(&body),
        path: record.path.as_str().to_owned(),
        fingerprint_bytes: u64::try_from(body.len()).unwrap_or(u64::MAX),
        fingerprint_hash: fnv1a(&body),
        status: record.metadata.status.clone().unwrap_or_default(),
        labels: record.metadata.labels.join(" "),
        tags: record.metadata.tags.join(" "),
        hierarchy: ancestry.join("|"),
        word_count: statistics.words,
        character_count: statistics.characters,
    })
}

fn normalize_markdown(body: &str) -> String {
    // Keep user-facing Unicode letters/numbers and whitespace. Markdown syntax,
    // link punctuation, and formatting delimiters are deliberately not tokens.
    body.chars()
        .map(|character| {
            if character.is_alphanumeric()
                || character.is_whitespace()
                || matches!(character, '\'' | '’' | '-' | '‑')
            {
                character
            } else {
                ' '
            }
        })
        .collect()
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
