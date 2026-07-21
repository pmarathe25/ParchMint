//! Disposable, versioned SQLite FTS5 search and statistics cache.
//!
//! Nothing in this crate is canonical.  Callers may remove the database at
//! any time and rebuild it exclusively from Markdown/TOML source.

use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;
use thiserror::Error;

/// Independently versioned schema stored inside `index.sqlite`.
pub const INDEX_SCHEMA_VERSION: u32 = 2;

/// Canonical data projected into one searchable row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IndexDocument<'a> {
    /// Stable binder node identity; this is the search-result identity.
    pub node_id: &'a str,
    /// Stable Markdown document identity.
    pub document_id: &'a str,
    /// `manuscript` or `research`.
    pub scope: &'a str,
    /// User-visible title from front matter.
    pub title: &'a str,
    /// User-visible synopsis from front matter.
    pub synopsis: &'a str,
    /// Normalized, plain body text.
    pub body: &'a str,
    /// Canonical project-relative Markdown path.
    pub path: &'a str,
    /// Change-detection byte length of the source body.
    pub fingerprint_bytes: u64,
    /// Change-detection FNV-1a hash of the source body.
    pub fingerprint_hash: i64,
    /// Optional workflow status.
    pub status: &'a str,
    /// Space-separated labels.
    pub labels: &'a str,
    /// Space-separated tags.
    pub tags: &'a str,
    /// Pipe-delimited node ancestry, including this node.
    pub hierarchy: &'a str,
    /// Derived Unicode-aware word count.
    pub word_count: u64,
    /// Derived Unicode scalar-value count.
    pub character_count: u64,
}

/// Compatibility source row retained for the Stage 01 spike API.
#[derive(Clone, Copy, Debug)]
pub struct SourceDocument<'a> {
    /// Stable row identity.
    pub id: &'a str,
    /// Searchable title.
    pub title: &'a str,
    /// Searchable body.
    pub body: &'a str,
}

impl<'a> From<SourceDocument<'a>> for IndexDocument<'a> {
    fn from(value: SourceDocument<'a>) -> Self {
        Self {
            node_id: value.id,
            document_id: value.id,
            scope: "manuscript",
            title: value.title,
            synopsis: "",
            body: value.body,
            path: "",
            fingerprint_bytes: u64::try_from(value.body.len()).unwrap_or(u64::MAX),
            fingerprint_hash: 0,
            status: "",
            labels: "",
            tags: "",
            hierarchy: value.id,
            word_count: 0,
            character_count: u64::try_from(value.body.chars().count()).unwrap_or(u64::MAX),
        }
    }
}

/// Fields enabled for a search query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchFields {
    /// Search titles.
    pub title: bool,
    /// Search synopses.
    pub synopsis: bool,
    /// Search normalized Markdown body text.
    pub body: bool,
}

impl Default for SearchFields {
    fn default() -> Self {
        Self {
            title: true,
            synopsis: true,
            body: true,
        }
    }
}

/// Optional, composable project-search filters.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SearchQuery<'a> {
    /// User query text. Unquoted tokens are prefix matched; quoted phrases are exact.
    pub text: &'a str,
    /// Restrict fields searched.
    pub fields: SearchFields,
    /// Optional manuscript/research scope.
    pub scope: Option<&'a str>,
    /// Restrict to an ancestor node (including that node).
    pub subtree: Option<&'a str>,
    /// Exact workflow status.
    pub status: Option<&'a str>,
    /// Label token.
    pub label: Option<&'a str>,
    /// Tag token.
    pub tag: Option<&'a str>,
}

/// Stable relevance-ranked search row suitable for batched UI delivery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchResult {
    /// Stable node identity.
    pub node_id: String,
    /// `manuscript` or `research`.
    pub scope: String,
    /// Result title.
    pub title: String,
    /// Result synopsis.
    pub synopsis: String,
    /// Canonical project-relative path.
    pub path: String,
    /// Pipe-delimited hierarchy context.
    pub hierarchy: String,
    /// HTML-safe marker-free text with `\u{1}` / `\u{2}` around the best match.
    pub snippet: String,
    /// Stored body word count.
    pub word_count: u64,
    /// Stored body character count.
    pub character_count: u64,
}

/// Aggregate stored counts returned without rescanning Markdown.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CountTotals {
    /// Sum of document word counts.
    pub words: u64,
    /// Sum of document Unicode scalar counts.
    pub characters: u64,
    /// Number of matching documents.
    pub documents: u64,
}

/// Revisioned cache-build state published without consulting canonical files.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RebuildProgress {
    /// Canonical/derived revision being built.
    pub revision: u64,
    /// Rows durably available to readers.
    pub completed: u64,
    /// Total rows expected for this revision.
    pub total: u64,
    /// Whether the revision is fully published.
    pub complete: bool,
}

/// SQLite cache that can always be rebuilt from canonical documents.
pub struct SearchIndex {
    connection: Connection,
}

impl SearchIndex {
    /// Creates or opens the disposable cache. An old/incompatible database is
    /// replaced in-place; callers can continue opening the canonical project.
    pub fn open(path: &Path) -> Result<Self, IndexError> {
        match Self::open_once(path) {
            Ok(index) => Ok(index),
            Err(error) if is_corrupt(&error) => {
                // This is deliberately limited to the exact derived database
                // and its SQLite sidecars. Canonical Markdown/TOML is never
                // opened for write by this crate.
                let _ = std::fs::remove_file(path);
                let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
                let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
                Self::open_once(path)
            }
            Err(error) => Err(error),
        }
    }

    fn open_once(path: &Path) -> Result<Self, IndexError> {
        let connection = Connection::open(path)?;
        connection.busy_timeout(std::time::Duration::from_millis(250))?;
        let mut index = Self { connection };
        index.initialize()?;
        Ok(index)
    }

    fn initialize(&mut self) -> Result<(), IndexError> {
        self.connection
            .execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        let version = self
            .connection
            .query_row(
                "SELECT value FROM index_meta WHERE key = 'schema_version'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional();
        match version {
            Ok(Some(value)) if value == INDEX_SCHEMA_VERSION.to_string() => Ok(()),
            Ok(_) | Err(rusqlite::Error::SqliteFailure(_, _)) => self.recreate_schema(),
            Err(error) => Err(IndexError::Sqlite(error)),
        }
    }

    fn recreate_schema(&mut self) -> Result<(), IndexError> {
        self.connection.execute_batch(
            "DROP TABLE IF EXISTS document_fts;
             DROP TABLE IF EXISTS document_meta;
             DROP TABLE IF EXISTS index_meta;
             CREATE TABLE index_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE document_meta (
               node_id TEXT PRIMARY KEY, document_id TEXT NOT NULL, scope TEXT NOT NULL,
               title TEXT NOT NULL, synopsis TEXT NOT NULL, path TEXT NOT NULL,
               fingerprint_bytes INTEGER NOT NULL, fingerprint_hash INTEGER NOT NULL,
               status TEXT NOT NULL, labels TEXT NOT NULL, tags TEXT NOT NULL,
               hierarchy TEXT NOT NULL, word_count INTEGER NOT NULL, character_count INTEGER NOT NULL
             );
             CREATE VIRTUAL TABLE document_fts USING fts5(
               node_id UNINDEXED, title, synopsis, body, labels, tags, status,
               tokenize='unicode61 remove_diacritics 2'
             );
             CREATE INDEX document_meta_scope ON document_meta(scope);
             CREATE INDEX document_meta_hierarchy ON document_meta(hierarchy);
             INSERT INTO index_meta(key, value) VALUES ('schema_version', '2');",
        )?;
        Ok(())
    }

    /// Current independent cache schema version.
    pub fn schema_version(&self) -> Result<u32, IndexError> {
        let value: String = self.connection.query_row(
            "SELECT value FROM index_meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )?;
        value
            .parse()
            .map_err(|_| IndexError::InvalidSchemaVersion(value))
    }

    /// Inserts a document or replaces its prior indexed revision.
    pub fn upsert_document(&mut self, document: IndexDocument<'_>) -> Result<(), IndexError> {
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "DELETE FROM document_fts WHERE node_id = ?1",
            [document.node_id],
        )?;
        transaction.execute(
            "DELETE FROM document_meta WHERE node_id = ?1",
            [document.node_id],
        )?;
        transaction.execute(
            "INSERT INTO document_meta(node_id, document_id, scope, title, synopsis, path, fingerprint_bytes, fingerprint_hash, status, labels, tags, hierarchy, word_count, character_count)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![document.node_id, document.document_id, document.scope, document.title, document.synopsis, document.path,
                document.fingerprint_bytes.cast_signed(), document.fingerprint_hash, document.status, document.labels, document.tags,
                document.hierarchy, document.word_count.cast_signed(), document.character_count.cast_signed()],
        )?;
        transaction.execute(
            "INSERT INTO document_fts(node_id, title, synopsis, body, labels, tags, status) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![document.node_id, document.title, document.synopsis, document.body, document.labels, document.tags, document.status],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Stage 01 compatibility wrapper.
    pub fn upsert(&mut self, document: SourceDocument<'_>) -> Result<(), IndexError> {
        self.upsert_document(document.into())
    }

    /// Deletes one document from the cache.
    pub fn delete(&mut self, node_id: &str) -> Result<(), IndexError> {
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM document_fts WHERE node_id = ?1", [node_id])?;
        transaction.execute("DELETE FROM document_meta WHERE node_id = ?1", [node_id])?;
        transaction.commit()?;
        Ok(())
    }

    /// Drops derived rows and repopulates them only from supplied source data.
    pub fn rebuild_documents<'a>(
        &mut self,
        documents: impl IntoIterator<Item = IndexDocument<'a>>,
    ) -> Result<(), IndexError> {
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM document_fts", [])?;
        transaction.execute("DELETE FROM document_meta", [])?;
        for document in documents {
            transaction.execute(
                "INSERT INTO document_meta(node_id, document_id, scope, title, synopsis, path, fingerprint_bytes, fingerprint_hash, status, labels, tags, hierarchy, word_count, character_count)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![document.node_id, document.document_id, document.scope, document.title, document.synopsis, document.path,
                    document.fingerprint_bytes.cast_signed(), document.fingerprint_hash, document.status, document.labels, document.tags,
                    document.hierarchy, document.word_count.cast_signed(), document.character_count.cast_signed()],
            )?;
            transaction.execute(
                "INSERT INTO document_fts(node_id, title, synopsis, body, labels, tags, status) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![document.node_id, document.title, document.synopsis, document.body, document.labels, document.tags, document.status],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Clears derived rows and publishes a revision before background batches
    /// begin. Readers may query the stable partial prefix while `complete` is
    /// false.
    pub fn begin_rebuild(&mut self, revision: u64, total: u64) -> Result<(), IndexError> {
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM document_fts", [])?;
        transaction.execute("DELETE FROM document_meta", [])?;
        for (key, value) in [
            ("rebuild_revision", revision.to_string()),
            ("rebuild_completed", "0".into()),
            ("rebuild_total", total.to_string()),
            ("rebuild_complete", "0".into()),
        ] {
            transaction.execute(
                "INSERT OR REPLACE INTO index_meta(key, value) VALUES(?1, ?2)",
                params![key, value],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Commits a bounded cache batch and advances revisioned progress in the
    /// same SQLite transaction as its rows.
    pub fn upsert_documents_batch<'a>(
        &mut self,
        documents: impl IntoIterator<Item = IndexDocument<'a>>,
        revision: u64,
        completed: u64,
    ) -> Result<(), IndexError> {
        let transaction = self.connection.transaction()?;
        for document in documents {
            transaction.execute(
                "DELETE FROM document_fts WHERE node_id = ?1",
                [document.node_id],
            )?;
            transaction.execute(
                "DELETE FROM document_meta WHERE node_id = ?1",
                [document.node_id],
            )?;
            transaction.execute(
                "INSERT INTO document_meta(node_id, document_id, scope, title, synopsis, path, fingerprint_bytes, fingerprint_hash, status, labels, tags, hierarchy, word_count, character_count)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![document.node_id, document.document_id, document.scope, document.title, document.synopsis, document.path,
                    document.fingerprint_bytes.cast_signed(), document.fingerprint_hash, document.status, document.labels, document.tags,
                    document.hierarchy, document.word_count.cast_signed(), document.character_count.cast_signed()],
            )?;
            transaction.execute(
                "INSERT INTO document_fts(node_id, title, synopsis, body, labels, tags, status) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![document.node_id, document.title, document.synopsis, document.body, document.labels, document.tags, document.status],
            )?;
        }
        transaction.execute(
            "INSERT OR REPLACE INTO index_meta(key, value) VALUES('rebuild_revision', ?1)",
            [revision.to_string()],
        )?;
        transaction.execute(
            "INSERT OR REPLACE INTO index_meta(key, value) VALUES('rebuild_completed', ?1)",
            [completed.to_string()],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Atomically marks a revision complete after its last batch.
    pub fn finish_rebuild(&mut self, revision: u64, total: u64) -> Result<(), IndexError> {
        let transaction = self.connection.transaction()?;
        for (key, value) in [
            ("rebuild_revision", revision.to_string()),
            ("rebuild_completed", total.to_string()),
            ("rebuild_total", total.to_string()),
            ("rebuild_complete", "1".into()),
        ] {
            transaction.execute(
                "INSERT OR REPLACE INTO index_meta(key, value) VALUES(?1, ?2)",
                params![key, value],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Reads revisioned progress without opening or scanning canonical files.
    pub fn rebuild_progress(&self) -> Result<Option<RebuildProgress>, IndexError> {
        let value = |key: &str| -> Result<Option<String>, rusqlite::Error> {
            self.connection
                .query_row(
                    "SELECT value FROM index_meta WHERE key = ?1",
                    [key],
                    |row| row.get(0),
                )
                .optional()
        };
        let Some(revision) = value("rebuild_revision")? else {
            return Ok(None);
        };
        Ok(Some(RebuildProgress {
            revision: revision.parse().unwrap_or(0),
            completed: value("rebuild_completed")?
                .and_then(|value| value.parse().ok())
                .unwrap_or(0),
            total: value("rebuild_total")?
                .and_then(|value| value.parse().ok())
                .unwrap_or(0),
            complete: value("rebuild_complete")?.as_deref() == Some("1"),
        }))
    }

    /// Stage 01 compatibility wrapper.
    pub fn rebuild<'a>(
        &mut self,
        documents: impl IntoIterator<Item = SourceDocument<'a>>,
    ) -> Result<(), IndexError> {
        self.rebuild_documents(documents.into_iter().map(Into::into))
    }

    /// Returns stable identifiers ordered by FTS5 rank.
    pub fn search(&self, query: &str, limit: u32) -> Result<Vec<String>, IndexError> {
        Ok(self
            .search_detailed(
                &SearchQuery {
                    text: query,
                    ..SearchQuery::default()
                },
                limit,
            )?
            .into_iter()
            .map(|row| row.node_id)
            .collect())
    }

    /// Searches derived rows. The caller can page with an increasing limit.
    /// Relevance is stable within a fixed candidate window, avoiding a global
    /// FTS rank sort that would score every dense match before returning the
    /// first bounded result from a 10-million-word corpus.
    pub fn search_detailed(
        &self,
        query: &SearchQuery<'_>,
        limit: u32,
    ) -> Result<Vec<SearchResult>, IndexError> {
        let match_query = fts_query(query.text, query.fields)?;
        let rank_window = limit.max(4_096);
        let mut statement = self.connection.prepare(
            "WITH candidates AS MATERIALIZED (
                 SELECT document_fts.rowid AS fts_rowid, document_fts.node_id, document_fts.rank
                 FROM document_fts JOIN document_meta candidate_meta USING(node_id)
                 WHERE document_fts MATCH ?1
                   AND (?2 IS NULL OR candidate_meta.scope = ?2)
                   AND (?3 IS NULL OR instr('|' || candidate_meta.hierarchy || '|', '|' || ?3 || '|') > 0)
                   AND (?4 IS NULL OR candidate_meta.status = ?4)
                   AND (?5 IS NULL OR instr(' ' || candidate_meta.labels || ' ', ' ' || ?5 || ' ') > 0)
                   AND (?6 IS NULL OR instr(' ' || candidate_meta.tags || ' ', ' ' || ?6 || ' ') > 0)
                 LIMIT ?8
             )
             SELECT m.node_id, m.scope, m.title, m.synopsis, m.path, m.hierarchy,
                    snippet(document_fts, 3, char(1), char(2), '…', 18), m.word_count, m.character_count
             FROM candidates
             JOIN document_fts ON document_fts.rowid = candidates.fts_rowid
             JOIN document_meta m USING(node_id)
             ORDER BY candidates.rank, m.node_id LIMIT ?7"
        )?;
        let rows = statement.query_map(
            params![
                match_query,
                query.scope,
                query.subtree,
                query.status,
                query.label,
                query.tag,
                limit,
                rank_window
            ],
            |row| {
                Ok(SearchResult {
                    node_id: row.get(0)?,
                    scope: row.get(1)?,
                    title: row.get(2)?,
                    synopsis: row.get(3)?,
                    path: row.get(4)?,
                    hierarchy: row.get(5)?,
                    snippet: row.get(6)?,
                    word_count: row.get::<_, i64>(7)?.cast_unsigned(),
                    character_count: row.get::<_, i64>(8)?.cast_unsigned(),
                })
            },
        )?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(IndexError::from)
    }

    /// Gets counts for scope/subtree without parsing bodies again.
    pub fn totals(
        &self,
        scope: Option<&str>,
        subtree: Option<&str>,
    ) -> Result<CountTotals, IndexError> {
        self.connection.query_row(
            "SELECT COALESCE(SUM(word_count), 0), COALESCE(SUM(character_count), 0), COUNT(*) FROM document_meta
             WHERE (?1 IS NULL OR scope = ?1)
               AND (?2 IS NULL OR instr('|' || hierarchy || '|', '|' || ?2 || '|') > 0)",
            params![scope, subtree],
            |row| Ok(CountTotals {
                words: row.get::<_, i64>(0)?.cast_unsigned(),
                characters: row.get::<_, i64>(1)?.cast_unsigned(),
                documents: row.get::<_, i64>(2)?.cast_unsigned(),
            }),
        ).map_err(IndexError::from)
    }

    /// Returns whether the linked SQLite library was built with FTS5.
    pub fn fts5_available(&self) -> Result<bool, IndexError> {
        Ok(self
            .connection
            .query_row(
                "SELECT 1 FROM pragma_compile_options WHERE compile_options = 'ENABLE_FTS5'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .is_some())
    }
}

fn is_corrupt(error: &IndexError) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    message.contains("file is not a database")
        || message.contains("database disk image is malformed")
        || message.contains("malformed database schema")
}

fn fts_query(text: &str, fields: SearchFields) -> Result<String, IndexError> {
    let text = text.trim();
    if text.is_empty() {
        return Ok("*".into());
    }
    let field_prefix = match (fields.title, fields.synopsis, fields.body) {
        (true, true, true) => String::new(),
        (false, false, false) => return Err(IndexError::NoSearchFields),
        _ => {
            let names = [
                (fields.title, "title"),
                (fields.synopsis, "synopsis"),
                (fields.body, "body"),
            ]
            .into_iter()
            .filter_map(|(enabled, name)| enabled.then_some(name))
            .collect::<Vec<_>>()
            .join(" OR ");
            format!("{{{names}}} : ")
        }
    };
    let mut terms = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character.is_whitespace() {
            continue;
        }
        if character == '"' {
            let mut phrase = String::new();
            let mut closed = false;
            for next in chars.by_ref() {
                if next == '"' {
                    closed = true;
                    break;
                }
                phrase.push(next);
            }
            if !closed {
                return Err(IndexError::InvalidQuery("unclosed quoted phrase".into()));
            }
            if !phrase.trim().is_empty() {
                terms.push(format!("\"{}\"", phrase.replace('"', "")));
            }
        } else if character.is_alphanumeric() || character == '_' {
            let mut token = String::from(character);
            while let Some(next) = chars.peek().copied() {
                if next.is_alphanumeric() || next == '_' || next == '\'' {
                    token.push(next);
                    chars.next();
                } else {
                    break;
                }
            }
            terms.push(format!("{token}*"));
        }
    }
    if terms.is_empty() {
        return Err(IndexError::InvalidQuery(
            "enter a word or quoted phrase".into(),
        ));
    }
    Ok(format!("{field_prefix}{}", terms.join(" AND ")))
}

/// SQLite cache failure.
#[derive(Debug, Error)]
pub enum IndexError {
    /// SQLite operation failed.
    #[error("search index operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// A cache metadata value is malformed.
    #[error("invalid index schema version: {0}")]
    InvalidSchemaVersion(String),
    /// Search fields excluded every searchable field.
    #[error("choose at least one search field")]
    NoSearchFields,
    /// User query cannot be safely expressed in the documented syntax.
    #[error("invalid search query: {0}")]
    InvalidQuery(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn indexed<'a>(id: &'a str, title: &'a str, body: &'a str) -> IndexDocument<'a> {
        IndexDocument {
            node_id: id,
            document_id: id,
            scope: "manuscript",
            title,
            synopsis: "harbor note",
            body,
            path: "manuscript/a.md",
            fingerprint_bytes: 1,
            fingerprint_hash: 2,
            status: "draft",
            labels: "important",
            tags: "sea winter",
            hierarchy: id,
            word_count: 3,
            character_count: 12,
        }
    }

    #[test]
    fn creates_updates_deletes_and_rebuilds_fts5() {
        let directory = tempfile::tempdir().unwrap();
        let mut index = SearchIndex::open(&directory.path().join("index.sqlite")).unwrap();
        assert!(index.fts5_available().unwrap());
        assert_eq!(index.schema_version().unwrap(), INDEX_SCHEMA_VERSION);
        index
            .upsert_document(indexed("one", "Orchard", "glass apples"))
            .unwrap();
        assert_eq!(index.search("glass", 10).unwrap(), ["one"]);
        index
            .upsert_document(indexed("one", "Orchard", "silver apples"))
            .unwrap();
        assert!(index.search("glass", 10).unwrap().is_empty());
        index.delete("one").unwrap();
        assert!(index.search("silver", 10).unwrap().is_empty());
        index
            .rebuild_documents([
                indexed("two", "Research", "corvid migration"),
                indexed("three", "Chapter", "winter orchard"),
            ])
            .unwrap();
        assert_eq!(index.search("orchard", 10).unwrap(), ["three"]);
    }

    #[test]
    fn prefixes_phrases_filters_and_stored_counts_are_stable() {
        let directory = tempfile::tempdir().unwrap();
        let mut index = SearchIndex::open(&directory.path().join("index.sqlite")).unwrap();
        let mut research = indexed("research", "Bird log", "winter corvid migration");
        research.scope = "research";
        research.tags = "birds winter";
        research.hierarchy = "research|child";
        index
            .rebuild_documents([
                indexed("chapter", "Winter Harbor", "the orchard sleeps"),
                research,
            ])
            .unwrap();
        let query = SearchQuery {
            text: "win",
            scope: Some("research"),
            tag: Some("birds"),
            ..SearchQuery::default()
        };
        assert_eq!(
            index.search_detailed(&query, 10).unwrap()[0].node_id,
            "research"
        );
        assert_eq!(index.search("\"winter harbor\"", 10).unwrap(), ["chapter"]);
        assert_eq!(
            index
                .totals(Some("research"), Some("child"))
                .unwrap()
                .documents,
            1
        );
    }

    #[test]
    fn incompatible_schema_is_recreated() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("index.sqlite");
        {
            let connection = Connection::open(&path).unwrap();
            connection.execute_batch("CREATE TABLE index_meta(key TEXT PRIMARY KEY, value TEXT); INSERT INTO index_meta VALUES('schema_version', '1');").unwrap();
        }
        let index = SearchIndex::open(&path).unwrap();
        assert_eq!(index.schema_version().unwrap(), INDEX_SCHEMA_VERSION);
    }

    #[test]
    fn corrupt_cache_is_replaced_without_touching_any_source() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("index.sqlite");
        std::fs::write(&path, b"not sqlite").unwrap();
        let index = SearchIndex::open(&path).unwrap();
        assert_eq!(index.schema_version().unwrap(), INDEX_SCHEMA_VERSION);
    }

    #[test]
    #[cfg_attr(debug_assertions, ignore = "release-mode Stage 14 10M-word gate")]
    fn records_stress_corpus_rebuild_and_first_result_timing() {
        let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        let directory = tempfile::tempdir_in(target).unwrap();
        let mut index = SearchIndex::open(&directory.path().join("index.sqlite")).unwrap();
        let rows = (0..10_000)
            .map(|number| {
                (
                    format!("node-{number}"),
                    format!("Chapter {number}"),
                    // 1,002 words per row: slightly over the canonical
                    // 10-million-word contract across 10,000 rows.
                    "orchard β 雪 ".repeat(334),
                )
            })
            .collect::<Vec<_>>();
        let start = std::time::Instant::now();
        index
            .rebuild_documents(
                rows.iter()
                    .map(|(id, title, body)| indexed(id, title, body)),
            )
            .unwrap();
        let rebuild = start.elapsed();
        let start = std::time::Instant::now();
        let results = index.search("orch", 50).unwrap();
        let first_results = start.elapsed();
        eprintln!(
            "stage14 words=10020000 index-rebuild={rebuild:?}; first-results={first_results:?}"
        );
        assert!(
            rebuild < std::time::Duration::from_mins(1),
            "10M-word index rebuild took {rebuild:?}"
        );
        assert!(
            first_results < std::time::Duration::from_millis(300),
            "first search took {first_results:?}"
        );
        assert_eq!(results.len(), 50);
        #[cfg(target_os = "linux")]
        {
            let status = std::fs::read_to_string("/proc/self/status").unwrap();
            let peak_kib = status
                .lines()
                .find_map(|line| line.strip_prefix("VmHWM:"))
                .and_then(|value| value.split_whitespace().next())
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap();
            eprintln!("stage14 peak-rss-kib={peak_kib}");
            assert!(peak_kib < 500 * 1024, "peak RSS was {peak_kib} KiB");
        }
    }
}
