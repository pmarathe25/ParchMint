//! Disposable SQLite FTS5 index spike.

use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;
use thiserror::Error;

/// Source-owned row used to build or incrementally update the disposable cache.
#[derive(Clone, Copy, Debug)]
pub struct SourceDocument<'a> {
    /// Stable canonical document identifier.
    pub id: &'a str,
    /// Searchable title.
    pub title: &'a str,
    /// Searchable body.
    pub body: &'a str,
}

/// FTS5 cache that can always be rebuilt from canonical documents.
pub struct SearchIndex {
    connection: Connection,
}

impl SearchIndex {
    /// Creates or opens the disposable cache and verifies FTS5 availability.
    pub fn open(path: &Path) -> Result<Self, IndexError> {
        let connection = Connection::open(path)?;
        connection.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             CREATE VIRTUAL TABLE IF NOT EXISTS documents USING fts5(
                 id UNINDEXED, title, body, tokenize='unicode61'
             );",
        )?;
        Ok(Self { connection })
    }

    /// Inserts a document or replaces its prior indexed revision.
    pub fn upsert(&mut self, document: SourceDocument<'_>) -> Result<(), IndexError> {
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM documents WHERE id = ?1", [document.id])?;
        transaction.execute(
            "INSERT INTO documents(id, title, body) VALUES (?1, ?2, ?3)",
            params![document.id, document.title, document.body],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Deletes one document from the cache.
    pub fn delete(&mut self, id: &str) -> Result<(), IndexError> {
        self.connection
            .execute("DELETE FROM documents WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Drops derived rows and repopulates them only from supplied source data.
    pub fn rebuild<'a>(
        &mut self,
        documents: impl IntoIterator<Item = SourceDocument<'a>>,
    ) -> Result<(), IndexError> {
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM documents", [])?;
        {
            let mut statement = transaction
                .prepare("INSERT INTO documents(id, title, body) VALUES (?1, ?2, ?3)")?;
            for document in documents {
                statement.execute(params![document.id, document.title, document.body])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    /// Returns stable identifiers ordered by FTS5 rank.
    pub fn search(&self, query: &str, limit: u32) -> Result<Vec<String>, IndexError> {
        let mut statement = self
            .connection
            .prepare("SELECT id FROM documents WHERE documents MATCH ?1 ORDER BY rank LIMIT ?2")?;
        let rows = statement.query_map(params![query, limit], |row| row.get(0))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(IndexError::from)
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

/// SQLite cache failure.
#[derive(Debug, Error)]
pub enum IndexError {
    /// SQLite operation failed.
    #[error("search index operation failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_updates_deletes_and_rebuilds_fts5() {
        let directory = tempfile::tempdir().unwrap();
        let mut index = SearchIndex::open(&directory.path().join("index.sqlite")).unwrap();
        assert!(index.fts5_available().unwrap());

        index
            .upsert(SourceDocument {
                id: "one",
                title: "Orchard",
                body: "glass apples",
            })
            .unwrap();
        assert_eq!(index.search("glass", 10).unwrap(), ["one"]);
        index
            .upsert(SourceDocument {
                id: "one",
                title: "Orchard",
                body: "silver apples",
            })
            .unwrap();
        assert!(index.search("glass", 10).unwrap().is_empty());
        index.delete("one").unwrap();
        assert!(index.search("silver", 10).unwrap().is_empty());

        index
            .rebuild([
                SourceDocument {
                    id: "two",
                    title: "Research",
                    body: "corvid migration",
                },
                SourceDocument {
                    id: "three",
                    title: "Chapter",
                    body: "winter orchard",
                },
            ])
            .unwrap();
        assert_eq!(index.search("orchard", 10).unwrap(), ["three"]);
    }
}
