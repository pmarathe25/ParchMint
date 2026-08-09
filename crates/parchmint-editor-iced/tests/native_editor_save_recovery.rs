//! Native Stage 34 coverage for editor/save/recovery pause and crash boundaries.

#[path = "fixtures/editor_save_recovery.rs"]
mod editor_save_recovery;

use editor_save_recovery::{
    Boundary, EditorSaveRecoveryHarness, PersistenceFailure, recovered_body,
};
use parchmint_editor_api::EditorRevision;
use parchmint_recovery_api::{
    DocumentRevision, RecoveryBatch, ResourceId, VersionedRecoveryPayload,
};
use parchmint_save::{SaveGeneration, SaveState};

const NEWER_REVISIONS: usize = 24;
const ALL_PAUSE_NEWER_REVISIONS: usize = 96;

#[test]
fn sustained_typing_remains_nonblocking_with_each_persistence_boundary_paused() {
    for boundary in [
        Boundary::BeforeProjection,
        Boundary::AfterProjection,
        Boundary::BeforeRecoveryAppend,
        Boundary::AfterRecoveryAppend,
        Boundary::BeforeSave,
        Boundary::AfterCanonicalCommit,
        Boundary::BeforeSaveAcknowledgement,
    ] {
        let mut harness =
            EditorSaveRecoveryHarness::with_projection_budget("", NEWER_REVISIONS + 4);
        harness.boundaries().pause_at(boundary);
        assert_eq!(harness.type_text("seed", true), EditorRevision::from(1));
        harness.boundaries().wait_until(boundary);

        for _ in 0..NEWER_REVISIONS {
            harness.type_text("x", false);
        }
        let (persistence, save, recovery) = harness.queue_bounds();
        assert!(
            persistence <= 2,
            "{boundary:?} persistence backlog unbounded"
        );
        assert!(save <= 2, "{boundary:?} save backlog unbounded");
        assert!(recovery <= 2, "{boundary:?} recovery backlog unbounded");
        assert!(
            harness.projected_count() <= 1,
            "{boundary:?} pause should not require foreground projection work"
        );

        harness.boundaries().release(boundary);
        harness.wait_until_idle();
        assert!(harness.boundaries().count(boundary) > 0);

        let status = harness.status();
        assert_eq!(status.state, SaveState::Dirty, "boundary {boundary:?}");
        assert_eq!(harness.acknowledgements().len(), 1);
        let saved = status.saved_through.expect("first save acknowledgement");
        assert_eq!(
            saved.open_documents.values().copied().collect::<Vec<_>>(),
            [DocumentRevision::from(1)]
        );
        assert_eq!(saved.generation, SaveGeneration::from(1));
        assert_eq!(
            status
                .requested
                .expect("newest dirty frontier")
                .open_documents
                .values()
                .copied()
                .collect::<Vec<_>>(),
            [DocumentRevision::from((NEWER_REVISIONS + 1) as u64)]
        );
        assert!(harness.recovery_batch_count() <= 2);
        harness.force_terminate();
    }
}

#[test]
fn sustained_typing_remains_nonblocking_with_all_persistence_boundaries_paused() {
    let mut harness =
        EditorSaveRecoveryHarness::with_projection_budget("", ALL_PAUSE_NEWER_REVISIONS + 4);
    let boundaries = [
        Boundary::BeforeProjection,
        Boundary::AfterProjection,
        Boundary::BeforeRecoveryAppend,
        Boundary::AfterRecoveryAppend,
        Boundary::BeforeSave,
        Boundary::AfterCanonicalCommit,
        Boundary::BeforeSaveAcknowledgement,
    ];
    for boundary in boundaries {
        harness.boundaries().pause_at(boundary);
    }

    assert_eq!(harness.type_text("seed", true), EditorRevision::from(1));
    harness.boundaries().wait_until(Boundary::BeforeProjection);
    for _ in 0..ALL_PAUSE_NEWER_REVISIONS {
        harness.type_text("x", false);
    }
    let (persistence, save, recovery) = harness.queue_bounds();
    assert!(
        persistence <= 2,
        "all-pause persistence queue grew unbounded"
    );
    assert!(save <= 2, "all-pause save queue grew unbounded");
    assert!(recovery <= 2, "all-pause recovery queue grew unbounded");
    assert_eq!(
        harness
            .status()
            .requested
            .unwrap()
            .open_documents
            .values()
            .copied()
            .collect::<Vec<_>>(),
        [DocumentRevision::from(
            (ALL_PAUSE_NEWER_REVISIONS + 1) as u64
        )]
    );

    harness.boundaries().release_all();
    harness.wait_until_idle();
    for boundary in boundaries {
        assert!(
            harness.boundaries().count(boundary) > 0,
            "missing {boundary:?}"
        );
    }
    assert_eq!(harness.status().state, SaveState::Dirty);
    assert_eq!(harness.acknowledgements().len(), 1);
    harness.force_terminate();
}

#[test]
fn default_eight_revision_retention_eviction_is_fallible_and_not_saved() {
    let mut harness = EditorSaveRecoveryHarness::new("");
    harness.boundaries().pause_at(Boundary::BeforeProjection);
    harness.type_text("seed", true);
    harness.boundaries().wait_until(Boundary::BeforeProjection);
    for _ in 0..8 {
        harness.type_text("x", false);
    }
    harness.boundaries().release(Boundary::BeforeProjection);
    harness.wait_until_idle();

    let status = harness.status();
    assert!(matches!(
        status.failure,
        Some(PersistenceFailure::Projection(_))
    ));
    assert!(status.saved_through.is_none());
    assert_eq!(harness.production_status().state, SaveState::Error);
    harness.force_terminate();
}

#[test]
fn interrupted_durable_recovery_batch_resumes_exactly_after_reopen() {
    let mut harness = EditorSaveRecoveryHarness::new("");
    harness.boundaries().pause_at(Boundary::AfterRecoveryAppend);
    harness.type_text("unacknowledged", false);
    harness
        .boundaries()
        .wait_until(Boundary::AfterRecoveryAppend);

    let interrupted = harness
        .in_flight_recovery()
        .expect("durable batch before in-memory acknowledgement");
    let interrupted_receipt = harness
        .in_flight_receipt()
        .expect("original durable receipt before interruption");
    assert_eq!(harness.recovery_batch_count(), 0);
    assert!(harness.acknowledgements().is_empty());

    harness.force_terminate();
    let replay = harness.replay_after_reopen("");
    let reconciled = harness.reconciled_frontier_after_reopen("");
    assert_eq!(replay.accepted.len(), 1);
    assert_eq!(replay.accepted[0], interrupted);
    assert!(replay.isolated.is_empty());

    let resumed = &replay.accepted[0];
    assert_eq!(resumed.project_revision, interrupted.project_revision);
    assert_eq!(resumed.revision_vector(), interrupted.revision_vector());
    assert_eq!(resumed.base_hashes, interrupted.base_hashes);
    assert_eq!(resumed.result_hashes, interrupted.result_hashes);
    assert_eq!(resumed.payload, interrupted.payload);
    assert_eq!(
        interrupted_receipt.durable_through,
        interrupted.revision_vector()
    );
    assert_eq!(reconciled, resumed.revision_vector());
    assert_ne!(
        resumed.result_hashes[&ResourceId::Document],
        resumed.base_hashes[&ResourceId::Document]
    );
    assert_eq!(recovered_body(&replay), Some("unacknowledged"));
    assert_eq!(
        harness.resume_interrupted_recovery_after_reopen(""),
        interrupted.revision_vector()
    );
}

#[test]
fn multiple_newer_revisions_during_save_pause_keep_exact_ack_dirty() {
    let mut harness = EditorSaveRecoveryHarness::new("");
    harness.boundaries().pause_at(Boundary::BeforeSave);
    harness.type_text("saved", true);
    harness.boundaries().wait_until(Boundary::BeforeSave);
    for text in ["-a", "-b", "-c", "-d", "-e"] {
        harness.type_text(text, false);
    }
    harness.boundaries().release(Boundary::BeforeSave);
    harness.wait_until_idle();

    let status = harness.status();
    assert_eq!(status.state, SaveState::Dirty);
    assert_eq!(harness.production_status().state, SaveState::Dirty);
    assert_eq!(harness.acknowledgements().len(), 1);
    let acknowledgement = &harness.acknowledgements()[0];
    assert_eq!(
        acknowledgement
            .requested_revisions
            .open_documents
            .values()
            .copied()
            .collect::<Vec<_>>(),
        [DocumentRevision::from(1)]
    );
    assert_eq!(
        status
            .requested
            .unwrap()
            .open_documents
            .values()
            .copied()
            .collect::<Vec<_>>(),
        [DocumentRevision::from(6)]
    );
    assert_eq!(harness.committed_bodies(), ["saved"]);
    assert_eq!(recovered_body(&harness.replay()), Some("saved-a-b-c-d-e"));
    harness.force_terminate();
}

#[test]
fn longer_mixed_replay_preserves_each_batch_and_resumes_the_unacknowledged_tail() {
    let mut harness = EditorSaveRecoveryHarness::new("");
    let bodies = [
        "one",
        "one two",
        "one two three",
        "one two three four",
        "one two three four five",
    ];
    for word in ["one", " two", " three", " four", " five"] {
        harness.type_text(word, false);
        harness.wait_until_idle();
    }
    harness.boundaries().pause_at(Boundary::AfterRecoveryAppend);
    let next_recovery_append = harness.boundaries().count(Boundary::AfterRecoveryAppend) + 1;
    harness.type_text(" six", false);
    harness
        .boundaries()
        .wait_until_count(Boundary::AfterRecoveryAppend, next_recovery_append);
    let interrupted = harness.in_flight_recovery().expect("unacknowledged tail");
    let receipt = harness.in_flight_receipt().expect("tail receipt");
    harness.force_terminate();

    let replay = harness.replay_after_reopen("");
    assert_eq!(replay.accepted.len(), 6);
    assert!(replay.isolated.is_empty());
    for (index, (batch, body)) in replay.accepted.iter().zip(bodies).enumerate() {
        assert_batch_exact(batch, (index + 1) as u64, (index + 1) as u64, body);
        assert_ne!(batch.base_hashes, batch.result_hashes);
        if let Some(previous) = index.checked_sub(1).and_then(|i| replay.accepted.get(i)) {
            assert_eq!(previous.result_hashes, batch.base_hashes);
        }
    }
    assert_batch_exact(&replay.accepted[5], 6, 6, "one two three four five six");
    assert_eq!(interrupted, replay.accepted[5]);
    assert_eq!(receipt.durable_through, interrupted.revision_vector());
    assert_eq!(
        harness.resume_interrupted_recovery_after_reopen(""),
        interrupted.revision_vector()
    );
}

#[test]
fn interrupted_projection_never_reports_saved() {
    let harness = EditorSaveRecoveryHarness::with_projection_budget("", 1);
    harness.boundaries().pause_at(Boundary::BeforeProjection);
    harness.type_text("first", true);
    harness.boundaries().wait_until(Boundary::BeforeProjection);
    harness.type_text(" second", false);
    harness.boundaries().release(Boundary::BeforeProjection);
    harness.wait_until_idle();

    let status = harness.status();
    assert_eq!(status.state, SaveState::Error);
    assert!(matches!(
        status.failure,
        Some(PersistenceFailure::Projection(_))
    ));
    assert!(status.saved_through.is_none());
    assert_eq!(recovered_body(&harness.replay()), Some("first second"));
}

fn assert_batch_exact(
    batch: &RecoveryBatch,
    project_revision: u64,
    document_revision: u64,
    body: &str,
) {
    assert_eq!(batch.project_revision.value(), project_revision);
    assert_eq!(
        batch.revision_vector().project_revision.value(),
        project_revision
    );
    assert_eq!(
        batch.revision_vector().documents[&editor_save_recovery::document_id()].value(),
        document_revision
    );
    let VersionedRecoveryPayload::V1(payload) = &batch.payload;
    assert_eq!(payload.operations.len(), 1);
    assert_eq!(
        payload.operations[0]
            .as_object()
            .and_then(|fields| fields.get("body")),
        Some(&serde_json::Value::String(body.to_owned()))
    );
}
