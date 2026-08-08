//! Contract scenarios for `HistoryStore` implementations.

use crate::*;

const COMPLETE_PROJECT: &[&str] = &[
    ".parchmint/format-version",
    "project.toml",
    "styles.css",
    "dictionary.txt",
    "manuscript/first.html",
    "annotations/first.json",
    "deletions.json",
];

#[derive(Clone, Debug, PartialEq, Eq)]
struct SnapshotFixture {
    resources: Vec<&'static str>,
    affected_documents: Vec<&'static str>,
    category: CheckpointCategoryFixture,
    name: Option<&'static str>,
}

impl SnapshotFixture {
    fn autosave(resources: &[&'static str], affected_documents: &[&'static str]) -> Self {
        Self {
            resources: resources.to_vec(),
            affected_documents: affected_documents.to_vec(),
            category: CheckpointCategoryFixture::Autosave,
            name: None,
        }
    }

    fn named_empty_snapshot(name: &'static str) -> Self {
        Self {
            resources: Vec::new(),
            affected_documents: Vec::new(),
            category: CheckpointCategoryFixture::NamedSnapshot,
            name: Some(name),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CheckpointCategoryFixture {
    Autosave,
    NamedSnapshot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PageFixture {
    checkpoints: Vec<CheckpointId>,
    next_cursor: Option<TestCursor>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RestoreFixture {
    source: CheckpointId,
    resources: Vec<&'static str>,
}

#[derive(Debug, PartialEq, Eq)]
enum TestHistoryError {
    MissingHistory,
    CorruptHistory,
    UnknownCheckpoint,
    ConflictingIntent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FixtureHealth {
    Healthy,
    Missing,
    Corrupt,
}

#[derive(Clone, Debug)]
struct StoredCheckpoint {
    id: CheckpointId,
    intent: &'static str,
    snapshot: SnapshotFixture,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TestCursor(CheckpointId);

/// A minimal, storage-independent model used to exercise the API contract.
struct TestHistoryFixture {
    health: FixtureHealth,
    checkpoints: Vec<StoredCheckpoint>,
}

impl TestHistoryFixture {
    fn initialized() -> Self {
        Self {
            health: FixtureHealth::Healthy,
            checkpoints: Vec::new(),
        }
    }

    fn missing_history() -> Self {
        Self {
            health: FixtureHealth::Missing,
            checkpoints: Vec::new(),
        }
    }

    fn corrupt_history() -> Self {
        Self {
            health: FixtureHealth::Corrupt,
            checkpoints: Vec::new(),
        }
    }

    fn ready(&self) -> Result<(), TestHistoryError> {
        match self.health {
            FixtureHealth::Healthy => Ok(()),
            FixtureHealth::Missing => Err(TestHistoryError::MissingHistory),
            FixtureHealth::Corrupt => Err(TestHistoryError::CorruptHistory),
        }
    }

    fn checkpoint(
        &mut self,
        intent: &'static str,
        snapshot: SnapshotFixture,
    ) -> Result<CheckpointId, TestHistoryError> {
        self.ready()?;
        if let Some(existing) = self
            .checkpoints
            .iter()
            .find(|checkpoint| checkpoint.intent == intent)
        {
            return if existing.snapshot == snapshot {
                Ok(existing.id)
            } else {
                Err(TestHistoryError::ConflictingIntent)
            };
        }

        let id = CheckpointId::from_bytes((self.checkpoints.len() as u128 + 1).to_be_bytes());
        self.checkpoints.push(StoredCheckpoint {
            id,
            intent,
            snapshot,
        });
        Ok(id)
    }

    fn list(
        &self,
        cursor: Option<TestCursor>,
        limit: usize,
        affected_document: Option<&str>,
    ) -> Result<PageFixture, TestHistoryError> {
        self.ready()?;
        let checkpoints: Vec<_> = self
            .checkpoints
            .iter()
            .rev()
            .filter(|checkpoint| {
                affected_document.is_none_or(|document| {
                    checkpoint.snapshot.affected_documents.contains(&document)
                })
            })
            .collect();
        let start = cursor
            .map(|TestCursor(id)| {
                checkpoints
                    .iter()
                    .position(|checkpoint| checkpoint.id == id)
                    .ok_or(TestHistoryError::UnknownCheckpoint)
                    .map(|position| position + 1)
            })
            .transpose()?
            .unwrap_or_default();
        let page: Vec<_> = checkpoints
            .iter()
            .skip(start)
            .take(limit)
            .map(|checkpoint| checkpoint.id)
            .collect();
        let next_cursor = (start + page.len() < checkpoints.len())
            .then(|| TestCursor(*page.last().expect("partial pages are nonempty")));

        Ok(PageFixture {
            checkpoints: page,
            next_cursor,
        })
    }

    fn preview(&self, checkpoint: CheckpointId) -> Result<SnapshotFixture, TestHistoryError> {
        self.ready()?;
        self.checkpoints
            .iter()
            .find(|stored| stored.id == checkpoint)
            .map(|stored| stored.snapshot.clone())
            .ok_or(TestHistoryError::UnknownCheckpoint)
    }

    fn restore(&self, checkpoint: CheckpointId) -> Result<RestoreFixture, TestHistoryError> {
        let snapshot = self.preview(checkpoint)?;
        Ok(RestoreFixture {
            source: checkpoint,
            resources: snapshot.resources,
        })
    }

    fn verify(&self) -> Result<(), TestHistoryError> {
        self.ready()
    }

    fn read_current_project(&self) -> &'static str {
        "current canonical project"
    }
}

fn autosave(resources: &[&'static str], affected_documents: &[&'static str]) -> SnapshotFixture {
    SnapshotFixture::autosave(resources, affected_documents)
}

fn checkpoint(
    fixture: &mut TestHistoryFixture,
    intent: &'static str,
    resources: &[&'static str],
    affected_documents: &[&'static str],
) -> CheckpointId {
    fixture
        .checkpoint(intent, autosave(resources, affected_documents))
        .unwrap_or_else(|error| panic!("checkpoint {intent} failed: {error:?}"))
}

#[test]
fn checkpoint_intents_are_idempotent() {
    let mut fixture = TestHistoryFixture::initialized();
    let snapshot = autosave(COMPLETE_PROJECT, &["first"]);

    let first = fixture.checkpoint("save-42", snapshot.clone()).unwrap();
    assert_eq!(fixture.checkpoint("save-42", snapshot), Ok(first));
}

#[test]
fn named_empty_snapshots_are_retained() {
    let mut fixture = TestHistoryFixture::initialized();
    let checkpoint = fixture
        .checkpoint(
            "named-snapshot-1",
            SnapshotFixture::named_empty_snapshot("Before restructuring"),
        )
        .unwrap();

    assert_eq!(
        fixture.preview(checkpoint).unwrap(),
        SnapshotFixture::named_empty_snapshot("Before restructuring")
    );
}

#[test]
fn history_pages_are_newest_first_and_cursor_stable() {
    let mut fixture = TestHistoryFixture::initialized();
    let oldest = checkpoint(&mut fixture, "first", &["project.toml"], &["first"]);
    let middle = checkpoint(&mut fixture, "second", &["project.toml"], &["second"]);
    let newest = checkpoint(&mut fixture, "third", &["project.toml"], &["third"]);

    let first_page = fixture.list(None, 2, None).unwrap();
    assert_eq!(first_page.checkpoints, vec![newest, middle]);

    let second_page = fixture.list(first_page.next_cursor, 2, None).unwrap();
    assert_eq!(second_page.checkpoints, vec![oldest]);
    assert_eq!(second_page.next_cursor, None);
}

#[test]
fn document_filtered_history_preserves_global_order() {
    let mut fixture = TestHistoryFixture::initialized();
    let first = checkpoint(
        &mut fixture,
        "first-document",
        &["manuscript/first.html"],
        &["first"],
    );
    checkpoint(
        &mut fixture,
        "second-document",
        &["manuscript/second.html"],
        &["second"],
    );
    let latest_first = checkpoint(
        &mut fixture,
        "first-document-again",
        &["manuscript/first.html"],
        &["first"],
    );

    assert_eq!(
        fixture.list(None, 10, Some("first")).unwrap().checkpoints,
        vec![latest_first, first]
    );
}

#[test]
fn restore_plans_cover_the_whole_project_without_rewinding_history() {
    let mut fixture = TestHistoryFixture::initialized();
    let source = checkpoint(&mut fixture, "before-restore", COMPLETE_PROJECT, &["first"]);
    checkpoint(
        &mut fixture,
        "current-state",
        &["project.toml"],
        &["second"],
    );
    let before_restore = fixture.list(None, 10, None).unwrap();

    assert_eq!(
        fixture.restore(source).unwrap(),
        RestoreFixture {
            source,
            resources: COMPLETE_PROJECT.to_vec(),
        }
    );
    assert_eq!(fixture.list(None, 10, None).unwrap(), before_restore);
}

#[test]
fn history_failures_do_not_block_current_project_reads() {
    let missing = TestHistoryFixture::missing_history();
    assert_eq!(
        missing.list(None, 1, None),
        Err(TestHistoryError::MissingHistory)
    );
    assert_eq!(missing.read_current_project(), "current canonical project");

    let corrupt = TestHistoryFixture::corrupt_history();
    assert_eq!(corrupt.verify(), Err(TestHistoryError::CorruptHistory));
    assert_eq!(corrupt.read_current_project(), "current canonical project");

    let initialized = TestHistoryFixture::initialized();
    assert_eq!(
        initialized.preview(CheckpointId::from_bytes([0xff; 16])),
        Err(TestHistoryError::UnknownCheckpoint)
    );
    assert_eq!(
        initialized.read_current_project(),
        "current canonical project"
    );
}

#[test]
fn named_snapshot_inputs_require_only_a_nonempty_name() {
    assert!(SnapshotName::new(" ").is_err());

    let input = CheckpointInput {
        intent_hash: CheckpointIntentHash::from_bytes([1; 32]),
        resources: Default::default(),
        category: CheckpointCategory::NamedSnapshot,
        affected_documents: Vec::new(),
        name: Some(SnapshotName::new("Before restructuring").unwrap()),
    };
    assert!(input.validate().is_ok());
}

#[test]
fn page_queries_reject_an_empty_page_size() {
    assert!(HistoryPageQuery::newest_first(0).validate().is_err());
}

#[test]
fn restore_plan_rejects_partial_or_extra_writes() {
    let writes = AtomicWritePlan::new(vec![StagedResource {
        path: "project.toml".into(),
        bytes: b"manifest".to_vec(),
    }]);

    assert!(
        RestorePlan::new(
            CheckpointId::from_bytes([1; 16]),
            Default::default(),
            writes
        )
        .is_err()
    );
}

#[test]
fn complete_restore_plan_tracks_resources_absent_from_the_checkpoint() {
    let source = CheckpointId::from_bytes([2; 16]);
    let manifest = CanonicalRelativePath::parse("project.toml").unwrap();
    let obsolete = CanonicalRelativePath::parse("manuscript/obsolete.html").unwrap();
    let resources = BTreeMap::from([(manifest.clone(), ContentHash::from_bytes([3; 32]))]);
    let writes = AtomicWritePlan::new(vec![StagedResource {
        path: manifest.as_str().into(),
        bytes: b"manifest".to_vec(),
    }]);

    let plan = RestorePlan::complete(source, resources, writes, vec![obsolete.clone()]).unwrap();

    assert_eq!(plan.deletions(), &[obsolete]);
}
