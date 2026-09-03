//! Behavioral contracts for project-wide search.
//!
//! These tests deliberately use a deterministic model instead of a storage
//! implementation.  The production index must preserve the same observable
//! rules: projections are revisioned, results are disposable candidates, and
//! the index is rebuildable cache data rather than authored project state.

use std::collections::{BTreeMap, BTreeSet};

use crate::*;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Projection {
    document: u64,
    revision: u64,
    body: String,
    display_title: String,
    synopsis: String,
    metadata: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AuthoredDocument {
    revision: u64,
    body: String,
    bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Hit {
    document: u64,
    revision: u64,
    field: &'static str,
    start: usize,
    end: usize,
    snippet: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Batch {
    generation: u64,
    hits: Vec<Hit>,
    finished: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IndexState {
    Missing,
    Corrupt,
    Healthy,
}

struct SearchModel {
    authored: BTreeMap<u64, AuthoredDocument>,
    projections: BTreeMap<u64, Projection>,
    state: IndexState,
    cancelled: BTreeSet<u64>,
}

impl Default for SearchModel {
    fn default() -> Self {
        Self {
            authored: BTreeMap::new(),
            projections: BTreeMap::new(),
            state: IndexState::Missing,
            cancelled: BTreeSet::new(),
        }
    }
}

impl SearchModel {
    fn new(authored: BTreeMap<u64, AuthoredDocument>) -> Self {
        Self {
            authored,
            state: IndexState::Missing,
            ..Self::default()
        }
    }

    fn projection(document: u64, revision: u64, body: &str) -> Projection {
        Projection {
            document,
            revision,
            body: body.into(),
            display_title: "needle chapter".into(),
            synopsis: "needle synopsis".into(),
            metadata: BTreeMap::from([("status".into(), "needle".into())]),
        }
    }

    fn replace_document(&mut self, projection: Projection) {
        let should_replace = self
            .projections
            .get(&projection.document)
            .is_none_or(|current| projection.revision >= current.revision);
        if should_replace {
            self.projections.insert(projection.document, projection);
        }
    }

    fn query(&self, generation: u64, term: &str) -> Vec<Batch> {
        let mut hits = Vec::new();
        for projection in self.projections.values() {
            for (field, text) in [
                ("body", projection.body.as_str()),
                ("display_title", projection.display_title.as_str()),
                ("synopsis", projection.synopsis.as_str()),
            ] {
                if let Some(start) = text.find(term) {
                    hits.push(Hit {
                        document: projection.document,
                        revision: projection.revision,
                        field,
                        start,
                        end: start + term.len(),
                        snippet: text.to_owned(),
                    });
                }
            }
            for (field, text) in &projection.metadata {
                if let Some(start) = text.find(term) {
                    hits.push(Hit {
                        document: projection.document,
                        revision: projection.revision,
                        field: "metadata",
                        start,
                        end: start + term.len(),
                        snippet: format!("{field}={text}"),
                    });
                }
            }
        }
        hits.chunks(1)
            .map(|batch| Batch {
                generation,
                hits: batch.to_vec(),
                finished: false,
            })
            .chain([Batch {
                generation,
                hits: Vec::new(),
                finished: true,
            }])
            .collect()
    }

    fn cancel(&mut self, generation: u64) {
        self.cancelled.insert(generation);
    }

    fn stream(&self, generation: u64, batches: Vec<Batch>) -> Vec<Batch> {
        if self.cancelled.contains(&generation) {
            Vec::new()
        } else {
            batches
        }
    }

    fn rebuild(&mut self) {
        self.projections.clear();
        let projections: Vec<_> = self
            .authored
            .iter()
            .map(|(&document, source)| Self::projection(document, source.revision, &source.body))
            .collect();
        for projection in projections {
            self.replace_document(projection);
        }
        self.state = IndexState::Healthy;
    }

    fn open_or_rebuild(&mut self) {
        if self.state != IndexState::Healthy {
            self.rebuild();
        }
    }

    fn verify(&self) -> bool {
        self.state == IndexState::Healthy
            && self.projections.iter().all(|(document, projection)| {
                self.authored
                    .get(document)
                    .is_some_and(|source| source.revision == projection.revision)
            })
    }
}
#[derive(Default)]
struct BatchConsumer {
    active_generation: u64,
    hits: Vec<Hit>,
    finished: bool,
}

impl BatchConsumer {
    fn start(&mut self, generation: u64) {
        self.active_generation = generation;
        self.hits.clear();
        self.finished = false;
    }

    fn accept(&mut self, batch: Batch) {
        if batch.generation != self.active_generation {
            return;
        }
        self.hits.extend(batch.hits);
        self.finished |= batch.finished;
    }
}

fn authored(body: &str, revision: u64, bytes: &[u8]) -> AuthoredDocument {
    AuthoredDocument {
        revision,
        body: body.into(),
        bytes: bytes.to_vec(),
    }
}

#[test]
fn document_projection_rejects_older_revisions_and_indexes_all_searchable_fields() {
    let mut model = SearchModel::new(BTreeMap::new());
    model.replace_document(SearchModel::projection(7, 2, "body needle"));
    model.replace_document(SearchModel::projection(7, 1, "old needle"));

    let batches = model.query(2, "needle");
    let hits: Vec<_> = batches.into_iter().flat_map(|batch| batch.hits).collect();
    assert_eq!(hits.len(), 4);
    assert!(hits.iter().all(|hit| hit.revision == 2));
    assert!(hits.iter().any(|hit| hit.field == "body"));
    assert!(hits.iter().any(|hit| hit.field == "display_title"));
    assert!(hits.iter().any(|hit| hit.field == "synopsis"));
    assert!(hits.iter().any(|hit| hit.field == "metadata"));
}

#[test]
fn search_delivers_small_batches_and_marks_the_stream_finished() {
    let mut model = SearchModel::new(BTreeMap::new());
    model.replace_document(SearchModel::projection(1, 4, "needle"));

    let generation = 4;
    let batches = model.query(generation, "needle");
    assert!(batches.len() >= 2);
    assert!(
        batches[..batches.len() - 1]
            .iter()
            .all(|batch| batch.hits.len() <= 1 && batch.generation == generation)
    );
    assert!(batches.last().unwrap().finished);
}

#[test]
fn cancellation_stops_future_batches_without_mutating_authored_documents() {
    let bytes = b"canonical needle bytes";
    let mut model = SearchModel::new(BTreeMap::from([(1, authored("needle", 1, bytes))]));
    model.replace_document(SearchModel::projection(1, 1, "needle"));
    let before = model.authored.clone();
    let generation = 1;
    let batches = model.query(generation, "needle");
    model.cancel(generation);

    assert!(model.stream(generation, batches).is_empty());
    assert_eq!(model.authored, before);
}

#[test]
fn stale_batches_from_an_older_generation_are_ignored() {
    let mut model = SearchModel::new(BTreeMap::new());
    model.replace_document(SearchModel::projection(1, 1, "needle"));
    let old_generation = 1;
    let new_generation = 2;
    let old_batches = model.query(old_generation, "needle");
    let new_batches = model.query(new_generation, "other");
    let mut consumer = BatchConsumer::default();
    consumer.start(new_generation);
    for batch in old_batches {
        consumer.accept(batch);
    }
    for batch in new_batches {
        consumer.accept(batch);
    }

    assert_eq!(consumer.active_generation, new_generation);
    assert!(consumer.hits.is_empty());
    assert!(consumer.finished);
}

#[test]
fn missing_or_corrupt_index_rebuilds_from_authored_data_without_changing_bytes() {
    let original = BTreeMap::from([
        (1, authored("first needle", 4, b"document-one-authored")),
        (2, authored("second", 9, b"document-two-authored")),
    ]);
    for state in [IndexState::Missing, IndexState::Corrupt] {
        let mut model = SearchModel::new(original.clone());
        model.replace_document(SearchModel::projection(1, 4, "stale cache text"));
        model.state = state;
        assert!(!model.verify());
        model.open_or_rebuild();

        assert_eq!(model.state, IndexState::Healthy);
        assert!(model.verify());
        assert_eq!(model.authored, original);
        assert_eq!(model.projections[&1].body, "first needle");
        assert_eq!(model.projections[&2].body, "second");
    }
}

fn document_id(value: u8) -> DocumentId {
    DocumentId::from_bytes([value; 16])
}

fn block_id(value: u8) -> BlockId {
    BlockId::from_bytes([value; 16])
}

fn body_projection(revision: u64, body: &str) -> SearchDocumentProjection {
    SearchDocumentProjection {
        document_id: document_id(1),
        revision: RevisionId::from(revision),
        texts: vec![SearchTextProjection {
            block_id: block_id(2),
            field: SearchField::Body,
            text: body.into(),
        }],
    }
}

fn body_hit(revision: u64, snippet: &str, match_start: usize, match_end: usize) -> SearchHit {
    SearchHit {
        document_id: document_id(1),
        block_id: block_id(2),
        indexed_revision: RevisionId::from(revision),
        field: SearchField::Body,
        candidate_range: TextRange::new(match_start, match_end).unwrap(),
        snippet: SearchSnippet {
            text: snippet.into(),
            match_range: TextRange::new(match_start, match_end).unwrap(),
        },
    }
}

#[test]
fn public_candidate_revalidation_checks_revision_utf8_range_and_exact_text() {
    let hit = body_hit(8, "prefix needle suffix", 7, 13);
    let candidate = ReplacementCandidate::from_hit(&hit).unwrap();

    assert!(candidate.revalidates(&body_projection(8, "prefix needle suffix")));
    assert!(!candidate.revalidates(&body_projection(9, "prefix needle suffix")));
    assert!(!candidate.revalidates(&body_projection(8, "prefix change! suffix")));
    assert!(!candidate.revalidates(&body_projection(8, "préfix needle suffix")));
}

#[test]
fn public_candidate_rejects_non_body_hits_and_invalid_snippets() {
    let mut title_hit = body_hit(1, "needle", 0, 6);
    title_hit.field = SearchField::DisplayTitle;
    assert!(ReplacementCandidate::from_hit(&title_hit).is_err());

    let invalid = SearchHit {
        snippet: SearchSnippet {
            text: "needle".into(),
            match_range: TextRange::new(2, 8).unwrap(),
        },
        ..body_hit(1, "needle", 0, 6)
    };
    assert!(ReplacementCandidate::from_hit(&invalid).is_err());
}

#[test]
fn public_projection_rejects_ambiguous_source_units() {
    let mut projection = body_projection(1, "needle");
    projection.texts.push(SearchTextProjection {
        block_id: block_id(2),
        field: SearchField::Body,
        text: "replacement".into(),
    });

    assert!(projection.validate().is_err());
}

#[test]
fn public_query_and_batches_use_a_ui_generation() {
    let query = SearchQuery {
        text: "needle".into(),
        fields: BTreeSet::from([SearchField::Body]),
        case_sensitive: false,
        whole_word: true,
        generation: 4,
    };
    assert!(query.validate().is_ok());

    let stale = SearchBatch {
        generation: 3,
        hits: Vec::new(),
        finished: true,
    };
    let current = SearchBatch {
        generation: query.generation,
        hits: Vec::new(),
        finished: true,
    };

    assert_ne!(stale.generation, query.generation);
    assert_eq!(current.generation, query.generation);
}

#[test]
fn public_query_rejects_empty_text_or_field_selection() {
    let empty_text = SearchQuery {
        text: String::new(),
        fields: BTreeSet::from([SearchField::Body]),
        case_sensitive: false,
        whole_word: false,
        generation: 1,
    };
    assert!(empty_text.validate().is_err());

    let empty_fields = SearchQuery {
        text: "needle".into(),
        fields: BTreeSet::new(),
        case_sensitive: false,
        whole_word: false,
        generation: 1,
    };
    assert!(empty_fields.validate().is_err());
}
