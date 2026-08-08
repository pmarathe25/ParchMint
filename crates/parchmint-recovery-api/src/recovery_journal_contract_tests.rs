use std::collections::BTreeMap;

use parchmint_contracts::generated::RecoveryRecordV1;

use super::*;

fn hash(value: u8) -> ContentHash {
    ContentHash::from_bytes([value; 32])
}

fn revisions(project: u64) -> RecoveryRevisionVector {
    RecoveryRevisionVector::new(ProjectRevision::from(project), BTreeMap::new())
}

fn base(project: u64, hash_value: u8) -> RecoveryBaseSnapshot {
    RecoveryBaseSnapshot {
        revisions: revisions(project),
        hashes: BTreeMap::from([(ResourceId::Manifest, hash(hash_value))]),
    }
}

fn batch(revision: u64, base_hash: u8, result_hash: u8) -> RecoveryBatch {
    RecoveryBatch {
        project_revision: ProjectRevision::from(revision),
        documents: BTreeMap::new(),
        base_hashes: BTreeMap::from([(ResourceId::Manifest, hash(base_hash))]),
        result_hashes: BTreeMap::from([(ResourceId::Manifest, hash(result_hash))]),
        payload: VersionedRecoveryPayload::V1(RecoveryRecordV1 {
            schema: "parchmint.recovery-record/v1".into(),
            record_id: format!("recovery-{revision}"),
            operations: vec![serde_json::json!({"replace": "manifest"})],
        }),
    }
}

fn append_three() -> Vec<RecoveryRecord> {
    vec![
        RecoveryRecord::Complete(batch(1, 0, 1)),
        RecoveryRecord::Complete(batch(2, 1, 2)),
        RecoveryRecord::Complete(batch(3, 2, 3)),
    ]
}

#[test]
fn append_validation_preserves_order_and_revision_receipts_are_exact() {
    let first = batch(1, 0, 1);
    let third = batch(3, 1, 3);
    let second = batch(2, 1, 2);

    assert_eq!(first.validate_after(None), Ok(()));
    assert!(matches!(
        third.validate_after(Some(&first)),
        Err(RecoveryError::NonConsecutiveProjectRevision { .. })
    ));
    assert_eq!(second.validate_after(Some(&first)), Ok(()));
    let document = DocumentId::from_bytes([7; 16]);
    let mut invalid_document = batch(2, 1, 2);
    invalid_document.documents.insert(
        document,
        EditorRevisionRange::new(DocumentRevision::from(2), DocumentRevision::from(2)).unwrap(),
    );
    assert!(matches!(
        invalid_document.validate_after(Some(&first)),
        Err(RecoveryError::NonConsecutiveDocumentRevision { .. })
    ));
    assert_eq!(
        RecoveryReceipt {
            durable_through: second.revision_vector(),
        }
        .durable_through,
        revisions(2)
    );
}

#[test]
fn batch_rejects_unknown_versions_and_unchanged_hashes() {
    let mut unsupported = batch(1, 0, 1);
    unsupported.payload = VersionedRecoveryPayload::V1(RecoveryRecordV1 {
        schema: "parchmint.recovery-record/v2".into(),
        record_id: "recovery-1".into(),
        operations: vec![serde_json::json!({})],
    });
    assert!(matches!(
        unsupported.validate(),
        Err(RecoveryError::UnsupportedPayloadVersion { .. })
    ));
    assert!(matches!(
        batch(1, 4, 4).validate(),
        Err(RecoveryError::InvalidBatch {
            field: "resource hashes",
            ..
        })
    ));
}

#[test]
fn replay_accepts_all_consecutive_matching_records() {
    let replay = replay_records(&base(0, 0), append_three());

    assert_eq!(
        replay
            .accepted
            .iter()
            .map(|batch| batch.project_revision)
            .collect::<Vec<_>>(),
        vec![
            ProjectRevision::from(1),
            ProjectRevision::from(2),
            ProjectRevision::from(3),
        ]
    );
    assert!(replay.isolated.is_empty());
    assert_eq!(replay.isolation, None);
}

#[test]
fn replay_isolates_each_unsafe_record_and_every_later_record() {
    let cases = [
        (
            RecoveryRecord::UnknownVersion {
                project_revision: Some(ProjectRevision::from(2)),
                version: "parchmint.recovery-record/v9".into(),
            },
            RecoveryIsolationReason::UnknownVersion {
                version: "parchmint.recovery-record/v9".into(),
            },
        ),
        (
            RecoveryRecord::Truncated {
                project_revision: Some(ProjectRevision::from(2)),
            },
            RecoveryIsolationReason::Truncated,
        ),
        (
            RecoveryRecord::Mismatched {
                project_revision: Some(ProjectRevision::from(2)),
                reason: "frame checksum failed".into(),
            },
            RecoveryIsolationReason::Mismatched {
                reason: "frame checksum failed".into(),
            },
        ),
        (
            RecoveryRecord::Ambiguous {
                project_revision: Some(ProjectRevision::from(2)),
            },
            RecoveryIsolationReason::Ambiguous,
        ),
    ];

    for (invalid, reason) in cases {
        let replay = replay_records(
            &base(0, 0),
            vec![
                RecoveryRecord::Complete(batch(1, 0, 1)),
                invalid,
                RecoveryRecord::Complete(batch(3, 2, 3)),
            ],
        );
        assert_eq!(replay.accepted, vec![batch(1, 0, 1)]);
        assert_eq!(replay.isolated.len(), 2);
        assert_eq!(
            replay.isolation,
            Some(RecoveryIsolation {
                position: 1,
                reason
            })
        );
    }
}

#[test]
fn replay_skips_records_covered_by_the_matching_base_snapshot() {
    let replay = replay_records(&base(1, 1), append_three());

    assert_eq!(replay.accepted, vec![batch(2, 1, 2), batch(3, 2, 3)]);
    assert!(replay.isolated.is_empty());
    assert_eq!(replay.isolation, None);
}

#[test]
fn replay_rejects_a_project_revision_gap_and_keeps_later_records_isolated() {
    let replay = replay_records(
        &base(0, 0),
        vec![
            RecoveryRecord::Complete(batch(1, 0, 1)),
            RecoveryRecord::Complete(batch(3, 1, 3)),
            RecoveryRecord::Complete(batch(4, 3, 4)),
        ],
    );

    assert_eq!(replay.accepted, vec![batch(1, 0, 1)]);
    assert_eq!(replay.isolated.len(), 2);
    assert!(matches!(
        replay.isolation,
        Some(RecoveryIsolation {
            position: 1,
            reason: RecoveryIsolationReason::InvalidBatch(
                RecoveryError::NonConsecutiveProjectRevision { .. }
            ),
        })
    ));
}

#[test]
fn replay_rejects_a_hash_mismatch_and_keeps_later_records_isolated() {
    let replay = replay_records(&base(0, 9), append_three());

    assert!(replay.accepted.is_empty());
    assert_eq!(replay.isolated.len(), 3);
    assert!(matches!(
        replay.isolation,
        Some(RecoveryIsolation {
            position: 0,
            reason: RecoveryIsolationReason::InvalidBatch(RecoveryError::HashMismatch { .. }),
        })
    ));
}

#[test]
fn flush_and_cleanup_helpers_require_exact_or_fully_covered_revisions() {
    let document = DocumentId::from_bytes([7; 16]);
    let mut first = batch(1, 0, 1);
    first.documents.insert(
        document,
        EditorRevisionRange::new(DocumentRevision::from(1), DocumentRevision::from(2)).unwrap(),
    );
    let record = RecoveryRecord::Complete(first.clone());
    let records = vec![record.clone()];

    assert!(contains_revision_vector(&records, &first.revision_vector()));
    assert!(!contains_revision_vector(&records, &revisions(2)));
    assert!(!is_covered_by_durable(
        &record,
        &DurableRevisionVector::new(revisions(1))
    ));
    assert!(is_covered_by_durable(
        &record,
        &DurableRevisionVector::new(RecoveryRevisionVector::new(
            ProjectRevision::from(1),
            BTreeMap::from([(document, DocumentRevision::from(2))]),
        )),
    ));
    assert!(!is_covered_by_durable(
        &RecoveryRecord::Truncated {
            project_revision: Some(ProjectRevision::from(1)),
        },
        &DurableRevisionVector::new(RecoveryRevisionVector::new(
            ProjectRevision::from(1),
            BTreeMap::from([(document, DocumentRevision::from(2))]),
        )),
    ));
}
