use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Barrier, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::Duration,
};

use parchmint_search_api::{
    BlockId, DocumentId, ProjectId, RevisionId, SearchBatch, SearchBatchSink,
    SearchDocumentProjection, SearchField, SearchFrontierId, SearchIndex, SearchIndexProblem,
    SearchIndexState, SearchProjectionSource, SearchProjectionVisitor, SearchQuery,
    SearchRebuildStatus, SearchTextProjection,
};
use parchmint_search_sqlite::SqliteSearchIndex;

static NEXT_PROJECT: AtomicU64 = AtomicU64::new(0);

struct TestProject(PathBuf);

impl TestProject {
    fn new(label: &str) -> Self {
        let id = NEXT_PROJECT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "parchmint-search-{label}-{}-{id}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("test project should be created");
        Self(path)
    }

    fn index(&self) -> SqliteSearchIndex {
        SqliteSearchIndex::new(&self.0)
    }

    fn cache(&self) -> PathBuf {
        self.0.join(".parchmint/cache/search.sqlite")
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Source(Vec<SearchDocumentProjection>);

impl SearchProjectionSource for Source {
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

struct FailingSource(SearchDocumentProjection);

impl SearchProjectionSource for FailingSource {
    fn visit_projections(
        &self,
        visitor: &mut dyn SearchProjectionVisitor,
    ) -> Result<(), parchmint_search_api::SearchError> {
        visitor.visit(self.0.clone())?;
        Err(parchmint_search_api::SearchError::Source {
            reason: "injected canonical projection failure".into(),
        })
    }
}

struct FrontierSource {
    frontier: SearchFrontierId,
    projections: Vec<SearchDocumentProjection>,
    visits: Arc<AtomicU64>,
}

struct GatedFrontierSource {
    frontier: SearchFrontierId,
    projections: Vec<SearchDocumentProjection>,
    entered: Arc<Barrier>,
    release: Arc<Barrier>,
    visits: Arc<AtomicU64>,
}

impl SearchProjectionSource for GatedFrontierSource {
    fn visit_projections(
        &self,
        visitor: &mut dyn SearchProjectionVisitor,
    ) -> Result<(), parchmint_search_api::SearchError> {
        self.entered.wait();
        self.release.wait();
        for projection in &self.projections {
            self.visits.fetch_add(1, Ordering::SeqCst);
            visitor.visit(projection.clone())?;
        }
        Ok(())
    }

    fn frontier_identity(&self) -> Option<SearchFrontierId> {
        Some(self.frontier)
    }
}

fn wait_for_rebuild(index: &SqliteSearchIndex) -> SearchRebuildStatus {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let status = index.rebuild_status();
        if !matches!(status, SearchRebuildStatus::Running { .. }) {
            return status;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "rebuild did not settle"
        );
        thread::yield_now();
    }
}

impl SearchProjectionSource for FrontierSource {
    fn visit_projections(
        &self,
        visitor: &mut dyn SearchProjectionVisitor,
    ) -> Result<(), parchmint_search_api::SearchError> {
        self.visits.fetch_add(1, Ordering::SeqCst);
        for projection in &self.projections {
            visitor.visit(projection.clone())?;
        }
        Ok(())
    }

    fn frontier_identity(&self) -> Option<SearchFrontierId> {
        Some(self.frontier)
    }
}

#[derive(Default, Clone)]
struct Sink(Arc<Mutex<Vec<SearchBatch>>>);

impl SearchBatchSink for Sink {
    fn push(&self, batch: SearchBatch) {
        self.0
            .lock()
            .expect("sink lock should not be poisoned")
            .push(batch);
    }
}

impl Sink {
    fn batches(&self) -> Vec<SearchBatch> {
        self.0.lock().unwrap().clone()
    }

    fn hit_count(&self) -> usize {
        self.batches().iter().map(|batch| batch.hits.len()).sum()
    }
}

struct BlockingSink {
    batches: Sink,
    entered: Arc<Barrier>,
    release: Arc<Barrier>,
    first: AtomicBool,
}

impl SearchBatchSink for BlockingSink {
    fn push(&self, batch: SearchBatch) {
        let finished = batch.finished;
        self.batches.push(batch);
        if !finished && self.first.swap(false, Ordering::SeqCst) {
            self.entered.wait();
            self.release.wait();
        }
    }
}

fn id(value: u8) -> DocumentId {
    DocumentId::from_bytes([value; 16])
}

fn block(value: u8) -> BlockId {
    BlockId::from_bytes([value; 16])
}

fn project_id(value: u8) -> ProjectId {
    ProjectId::from_bytes([value; 16])
}

fn projection(
    document: u8,
    revision: u64,
    field: SearchField,
    text: &str,
) -> SearchDocumentProjection {
    SearchDocumentProjection {
        document_id: id(document),
        revision: RevisionId::from(revision),
        texts: vec![SearchTextProjection {
            block_id: block(document),
            field,
            text: text.into(),
        }],
    }
}

fn fields(fields: impl IntoIterator<Item = SearchField>) -> BTreeSet<SearchField> {
    fields.into_iter().collect()
}

fn query(text: &str, generation: u64) -> SearchQuery {
    SearchQuery {
        text: text.into(),
        fields: fields([SearchField::Body]),
        case_sensitive: false,
        whole_word: false,
        generation,
    }
}

fn whole_word_query(text: &str, generation: u64) -> SearchQuery {
    SearchQuery {
        whole_word: true,
        ..query(text, generation)
    }
}

fn open(index: &SqliteSearchIndex, project: u8, projections: Vec<SearchDocumentProjection>) {
    index
        .open_or_rebuild(project_id(project), &Source(projections))
        .expect("search index should open");
}

fn damage_cache(path: &Path, problem: SearchIndexProblem) {
    match problem {
        SearchIndexProblem::Corrupt => fs::write(path, b"not a SQLite database").unwrap(),
        SearchIndexProblem::Incompatible => {
            let connection = rusqlite::Connection::open(path).unwrap();
            connection
                .execute("DROP TRIGGER search_content_insert", [])
                .unwrap();
        }
        SearchIndexProblem::Missing => unreachable!("damage cannot make a cache missing"),
    }
}

#[test]
fn sqlite_fts5_opens_in_the_project_cache() {
    let project = TestProject::new("fts5");
    let index = project.index();
    open(
        &index,
        1,
        vec![projection(1, 1, SearchField::Body, "FTS5 available")],
    );

    let sink = Sink::default();
    index
        .query(whole_word_query("available", 1), Box::new(sink.clone()))
        .expect("FTS5 query should work");
    assert_eq!(sink.hit_count(), 1);
    assert!(
        project.cache().is_file(),
        "the index must use the project cache path"
    );
}

#[test]
fn user_text_is_literal_not_raw_fts5_syntax() {
    let project = TestProject::new("escaping");
    let index = project.index();
    open(
        &index,
        1,
        vec![projection(
            1,
            1,
            SearchField::Body,
            "alpha beta OR gamma \" *",
        )],
    );

    let sink = Sink::default();
    index
        .query(whole_word_query("alpha OR *", 2), Box::new(sink.clone()))
        .expect("literal query should not fail");
    assert_eq!(sink.hit_count(), 0);

    let sink = Sink::default();
    index
        .query(whole_word_query("gamma \" *", 3), Box::new(sink.clone()))
        .expect("quoted punctuation should remain searchable literal text");
    assert_eq!(sink.hit_count(), 1);

    let sink = Sink::default();
    index
        .query(query("\" *", 4), Box::new(sink.clone()))
        .expect("punctuation-only text should remain searchable");
    assert_eq!(sink.hit_count(), 1);
}

#[test]
fn unicode_whole_word_and_case_sensitive_filters_are_checked_after_fts5_candidates() {
    let project = TestProject::new("unicode");
    let index = project.index();
    open(
        &index,
        1,
        vec![projection(
            1,
            1,
            SearchField::Body,
            "Straße STRASSE straßeX café CAFÉ",
        )],
    );

    let mut exact = query("Straße", 3);
    exact.case_sensitive = true;
    exact.whole_word = true;
    let sink = Sink::default();
    index
        .query(exact, Box::new(sink.clone()))
        .expect("Unicode search should work");
    let batches = sink.0.lock().unwrap();
    let hits: Vec<_> = batches.iter().flat_map(|batch| &batch.hits).collect();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].snippet.matched_text(), Some("Straße"));

    let mut folded = query("café", 4);
    folded.whole_word = true;
    let sink = Sink::default();
    index
        .query(folded, Box::new(sink.clone()))
        .expect("case-folded search should work");
    assert_eq!(sink.hit_count(), 2);

    let sink = Sink::default();
    index
        .query(query("traß", 5), Box::new(sink.clone()))
        .expect("non-whole-word search should include embedded Unicode matches");
    assert_eq!(sink.hit_count(), 2);
}

#[test]
fn cancellation_waits_for_in_flight_delivery_then_stops_the_generation() {
    let project = TestProject::new("cancel");
    let index = Arc::new(project.index());
    let projections = (1..=128)
        .map(|document| projection(document, 1, SearchField::Body, "needle"))
        .collect();
    open(&index, 1, projections);

    let sink = Sink::default();
    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let query_index = Arc::clone(&index);
    let query_sink = sink.clone();
    let query_entered = Arc::clone(&entered);
    let query_release = Arc::clone(&release);
    let query_thread = thread::spawn(move || {
        query_index.query(
            whole_word_query("needle", 11),
            Box::new(BlockingSink {
                batches: query_sink,
                entered: query_entered,
                release: query_release,
                first: AtomicBool::new(true),
            }),
        )
    });

    entered.wait();
    let releaser = thread::spawn(move || {
        thread::sleep(Duration::from_millis(20));
        release.wait();
    });
    index.cancel(11);
    releaser.join().unwrap();
    query_thread.join().unwrap().unwrap();

    let batches = sink.batches();
    assert_eq!(batches.len(), 1);
    assert!(!batches[0].finished);
}

#[test]
fn corrupt_or_incompatible_cache_rebuilds_without_touching_authored_bytes() {
    for problem in [
        SearchIndexProblem::Corrupt,
        SearchIndexProblem::Incompatible,
    ] {
        let project = TestProject::new("invalid-cache");
        let authored = b"canonical bytes stay untouched";
        let authored_path = project.0.join("manuscript/chapter.html");
        fs::create_dir_all(authored_path.parent().unwrap()).unwrap();
        fs::write(&authored_path, authored).unwrap();

        let first = project.index();
        open(
            &first,
            1,
            vec![projection(1, 1, SearchField::Body, "old projection")],
        );
        drop(first);
        damage_cache(&project.cache(), problem);

        let second = project.index();
        let state = second.open_or_rebuild(
            project_id(1),
            &Source(vec![projection(
                1,
                2,
                SearchField::Body,
                "canonical replacement",
            )]),
        );
        assert_eq!(state, Ok(SearchIndexState::Rebuilt { previous: problem }));
        assert_eq!(fs::read(&authored_path).unwrap(), authored);
        let report = second.verify().unwrap();
        assert!(report.healthy);
        assert_eq!(report.indexed_documents, 1);
    }
}

#[test]
fn warm_index_with_matching_frontier_does_not_visit_canonical_bodies() {
    let project = TestProject::new("warm-frontier");
    let frontier = SearchFrontierId::from_bytes([23; 32]);
    let initial_visits = Arc::new(AtomicU64::new(0));
    let first = project.index();
    assert_eq!(
        first.open_or_rebuild(
            project_id(1),
            &FrontierSource {
                frontier,
                projections: vec![projection(1, 4, SearchField::Body, "warm body")],
                visits: initial_visits.clone(),
            },
        ),
        Ok(SearchIndexState::Rebuilt {
            previous: SearchIndexProblem::Missing,
        })
    );
    assert_eq!(initial_visits.load(Ordering::SeqCst), 1);
    drop(first);

    let warm_visits = Arc::new(AtomicU64::new(0));
    let reopened = project.index();
    assert_eq!(
        reopened.open_or_rebuild(
            project_id(1),
            &FrontierSource {
                frontier,
                projections: Vec::new(),
                visits: warm_visits.clone(),
            },
        ),
        Ok(SearchIndexState::Opened)
    );
    assert_eq!(warm_visits.load(Ordering::SeqCst), 0);

    let receipt = reopened
        .replace_document(projection(1, 4, SearchField::Body, "warm body"))
        .unwrap();
    assert!(!receipt.replaced);
    drop(reopened);
    let preserved = project.index();
    let preserved_visits = Arc::new(AtomicU64::new(0));
    assert_eq!(
        preserved.open_or_rebuild(
            project_id(1),
            &FrontierSource {
                frontier,
                projections: Vec::new(),
                visits: preserved_visits.clone(),
            },
        ),
        Ok(SearchIndexState::Opened)
    );
    assert_eq!(preserved_visits.load(Ordering::SeqCst), 0);

    preserved
        .replace_document(projection(1, 5, SearchField::Body, "unsaved live body"))
        .unwrap();
    drop(preserved);
    let rebuild_visits = Arc::new(AtomicU64::new(0));
    let after_live_write = project.index();
    assert_eq!(
        after_live_write.open_or_rebuild(
            project_id(1),
            &FrontierSource {
                frontier,
                projections: vec![projection(1, 4, SearchField::Body, "warm body")],
                visits: rebuild_visits.clone(),
            },
        ),
        Ok(SearchIndexState::Rebuilt {
            previous: SearchIndexProblem::Incompatible,
        })
    );
    assert_eq!(rebuild_visits.load(Ordering::SeqCst), 1);
}

#[test]
fn background_open_returns_while_source_is_blocked_and_cancel_prevents_publication() {
    let project = TestProject::new("background-cancel");
    let index = project.index();
    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let visits = Arc::new(AtomicU64::new(0));
    let state = index
        .open_or_rebuild_background(
            project_id(1),
            Arc::new(GatedFrontierSource {
                frontier: SearchFrontierId::from_bytes([31; 32]),
                projections: (1..=32)
                    .map(|document| projection(document, 1, SearchField::Body, "needle"))
                    .collect(),
                entered: entered.clone(),
                release: release.clone(),
                visits: visits.clone(),
            }),
        )
        .unwrap();
    let SearchIndexState::Rebuilding { generation, .. } = state else {
        panic!("missing index should rebuild in the background")
    };
    entered.wait();
    assert!(matches!(
        index.query(query("needle", 71), Box::new(Sink::default())),
        Err(parchmint_search_api::SearchError::Rebuilding {
            generation: active
        }) if active == generation
    ));
    index.cancel(generation);
    release.wait();
    assert_eq!(
        wait_for_rebuild(&index),
        SearchRebuildStatus::Cancelled { generation }
    );
    assert_eq!(visits.load(Ordering::SeqCst), 1);
    assert!(!index.verify().unwrap().healthy);
}

#[test]
fn background_rebuild_publishes_frontier_only_after_bounded_stream_finishes() {
    let project = TestProject::new("background-publish");
    let index = project.index();
    let frontier = SearchFrontierId::from_bytes([37; 32]);
    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let visits = Arc::new(AtomicU64::new(0));
    let state = index
        .open_or_rebuild_background(
            project_id(1),
            Arc::new(GatedFrontierSource {
                frontier,
                projections: (1..=48)
                    .map(|document| projection(document, 1, SearchField::Body, "bounded"))
                    .collect(),
                entered: entered.clone(),
                release: release.clone(),
                visits: visits.clone(),
            }),
        )
        .unwrap();
    let SearchIndexState::Rebuilding { generation, .. } = state else {
        panic!("missing index should rebuild in the background")
    };
    entered.wait();
    assert!(matches!(
        index.rebuild_status(),
        SearchRebuildStatus::Running {
            processed_documents: 0,
            ..
        }
    ));
    release.wait();
    assert_eq!(
        wait_for_rebuild(&index),
        SearchRebuildStatus::Complete {
            generation,
            indexed_documents: 48,
        }
    );
    assert_eq!(visits.load(Ordering::SeqCst), 48);
    drop(index);

    let warm_visits = Arc::new(AtomicU64::new(0));
    let reopened = project.index();
    assert_eq!(
        reopened.open_or_rebuild_background(
            project_id(1),
            Arc::new(FrontierSource {
                frontier,
                projections: Vec::new(),
                visits: warm_visits.clone(),
            }),
        ),
        Ok(SearchIndexState::Opened)
    );
    assert_eq!(warm_visits.load(Ordering::SeqCst), 0);
}

#[test]
fn failed_projection_stream_rolls_back_and_cannot_publish_an_incomplete_cache() {
    let project = TestProject::new("rollback");
    let index = project.index();
    open(
        &index,
        1,
        vec![projection(1, 1, SearchField::Body, "stable needle")],
    );

    let result = index.rebuild(&FailingSource(projection(
        2,
        1,
        SearchField::Body,
        "uncommitted replacement",
    )));
    assert!(matches!(
        result,
        Err(parchmint_search_api::SearchError::Source { .. })
    ));

    let sink = Sink::default();
    index
        .query(query("needle", 15), Box::new(sink.clone()))
        .unwrap();
    assert_eq!(sink.hit_count(), 1);

    let missing = TestProject::new("incomplete");
    let fresh = missing.index();
    let result = fresh.open_or_rebuild(
        project_id(2),
        &FailingSource(projection(
            2,
            1,
            SearchField::Body,
            "uncommitted first build",
        )),
    );
    assert!(matches!(
        result,
        Err(parchmint_search_api::SearchError::Source { .. })
    ));
    let retry = fresh.open_or_rebuild(
        project_id(2),
        &Source(vec![projection(2, 1, SearchField::Body, "committed retry")]),
    );
    assert_eq!(
        retry,
        Ok(SearchIndexState::Rebuilt {
            previous: SearchIndexProblem::Incompatible,
        })
    );
}

#[test]
fn warm_results_are_streamed_in_bounded_batches() {
    let project = TestProject::new("bounds");
    let index = project.index();
    let projections = (1..=128)
        .map(|n| projection(n, 1, SearchField::Body, "needle"))
        .collect();
    open(&index, 1, projections);
    let sink = Sink::default();
    index
        .query(whole_word_query("needle", 20), Box::new(sink.clone()))
        .expect("warm query should work");
    let batches = sink.batches();
    assert!(batches.iter().all(|batch| batch.hits.len() <= 64));
    assert!(batches.last().is_some_and(|batch| batch.finished));
}

#[test]
fn project_workers_are_isolated_and_cache_identity_is_rebuildable() {
    let first_project = TestProject::new("worker-a");
    let second_project = TestProject::new("worker-b");
    let first = first_project.index();
    let second = second_project.index();
    open(
        &first,
        1,
        vec![projection(1, 1, SearchField::Body, "alpha")],
    );
    open(
        &second,
        2,
        vec![projection(2, 1, SearchField::Body, "beta")],
    );

    let first_sink = Sink::default();
    first
        .query(query("beta", 30), Box::new(first_sink.clone()))
        .unwrap();
    assert_eq!(first_sink.hit_count(), 0);
    assert!(first_project.cache().is_file() && second_project.cache().is_file());
    drop(first);

    let reopened = first_project.index();
    let state = reopened.open_or_rebuild(
        project_id(3),
        &Source(vec![projection(
            3,
            1,
            SearchField::Body,
            "replacement project",
        )]),
    );
    assert_eq!(
        state,
        Ok(SearchIndexState::Rebuilt {
            previous: SearchIndexProblem::Incompatible,
        })
    );
}

#[test]
fn revisioned_transactions_keep_newer_projection_and_delete_revision_atomic() {
    let project = TestProject::new("revisions");
    let index = project.index();
    open(&index, 1, vec![projection(1, 2, SearchField::Body, "new")]);
    let stale = index
        .replace_document(projection(1, 1, SearchField::Body, "old"))
        .unwrap();
    assert!(!stale.replaced);
    let deleted = index.delete_document(id(1), RevisionId::from(1)).unwrap();
    assert!(!deleted.replaced);

    let sink = Sink::default();
    index
        .query(query("new", 40), Box::new(sink.clone()))
        .unwrap();
    assert_eq!(sink.hit_count(), 1);

    let deleted = index.delete_document(id(1), RevisionId::from(3)).unwrap();
    assert!(deleted.replaced);
    let stale = index
        .replace_document(projection(1, 3, SearchField::Body, "resurrected"))
        .unwrap();
    assert!(
        !stale.replaced,
        "an equal-revision projection must not cross a tombstone"
    );
}

#[test]
fn equal_document_revision_replaces_changed_project_field_projection() {
    let project = TestProject::new("equal-document-revision");
    let index = project.index();
    open(
        &index,
        1,
        vec![projection(
            1,
            7,
            SearchField::DisplayTitle,
            "Original title",
        )],
    );

    let refreshed = index
        .replace_document(projection(1, 7, SearchField::DisplayTitle, "Fresh title"))
        .expect("equal canonical document revision should refresh project fields");
    assert!(refreshed.replaced);

    let sink = Sink::default();
    index
        .query(
            SearchQuery {
                fields: fields([SearchField::DisplayTitle]),
                ..query("Fresh title", 41)
            },
            Box::new(sink.clone()),
        )
        .expect("fresh title query");
    let batches = sink.batches();
    assert_eq!(
        batches.iter().map(|batch| batch.hits.len()).sum::<usize>(),
        1
    );
    assert_eq!(batches[0].hits[0].indexed_revision, RevisionId::from(7));
}
