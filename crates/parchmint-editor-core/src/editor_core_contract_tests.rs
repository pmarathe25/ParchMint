use super::*;

fn document(value: u8) -> DocumentId {
    DocumentId::from_bytes([value; 16])
}

fn block(value: u8) -> BlockId {
    BlockId::from_bytes([value; 16])
}

fn comment(value: u8) -> CommentId {
    CommentId::from_bytes([value; 16])
}

fn view(value: u8) -> ViewId {
    ViewId::from_bytes([value; 16])
}

fn revision(value: u64) -> EditorRevision {
    EditorRevision::from(value)
}

fn position(value: u64) -> DocumentPosition {
    DocumentPosition::from(value)
}

fn selection(anchor: u64, head: u64) -> EditorSelection {
    EditorSelection::new(position(anchor), position(head))
}

fn load(body: &str) -> CanonicalDocumentLoad {
    CanonicalDocumentLoad::new(document(9), body)
}

fn command(observed: u64, kind: EditorCommandKind) -> EditorCommand {
    EditorCommand::new(revision(observed), kind)
}

fn origin(view: ViewId) -> EditorCommandOrigin {
    EditorCommandOrigin::new(view)
}

fn projection(revision: u64) -> Projection {
    Projection {
        document_id: document(9),
        revision: EditorRevision::from(revision),
        blocks: Vec::new(),
        comments: Vec::new(),
        anchors: Vec::new(),
    }
}

#[test]
fn two_views_share_content_and_undo_but_map_logical_selections_independently() {
    let left = view(1);
    let right = view(2);
    let mut session = EditorCoreSession::open(load("alpha")).expect("open session");
    session.attach_view(left).expect("attach left view");
    session.attach_view(right).expect("attach right view");
    session
        .execute(
            origin(left),
            command(
                0,
                EditorCommandKind::SetSelection {
                    selection: selection(0, 5),
                },
            ),
        )
        .expect("select in left view");
    session
        .execute(
            origin(right),
            command(
                0,
                EditorCommandKind::SetSelection {
                    selection: selection(2, 2),
                },
            ),
        )
        .expect("select in right view");

    session
        .execute(
            origin(left),
            command(
                0,
                EditorCommandKind::InsertText {
                    at: position(5),
                    text: " beta".into(),
                },
            ),
        )
        .expect("insert from left view");
    session
        .execute(origin(right), command(1, EditorCommandKind::Undo))
        .expect("undo from right view");

    assert_eq!(session.canonical_projection().body(), "alpha");
    assert_eq!(session.selection(left), Ok(selection(0, 5)));
    assert_eq!(session.selection(right), Ok(selection(2, 2)));
}

#[test]
fn ids_transactions_and_revisions_are_core_owned_and_monotonic() {
    let mounted = view(1);
    let mut session = EditorCoreSession::open(load("a")).expect("open session");
    session.attach_view(mounted).expect("attach view");
    let first = session
        .execute(
            origin(mounted),
            command(
                0,
                EditorCommandKind::InsertText {
                    at: position(1),
                    text: "b".into(),
                },
            ),
        )
        .expect("first insert");
    let second = session
        .execute(
            origin(mounted),
            command(
                1,
                EditorCommandKind::InsertText {
                    at: position(2),
                    text: "c".into(),
                },
            ),
        )
        .expect("second insert");

    assert_eq!(session.revision(), revision(2));
    assert_eq!(first.transaction().map(TransactionId::value), Some(1));
    assert_eq!(second.transaction().map(TransactionId::value), Some(2));
    assert_eq!(first.changed_blocks(), &[block(9)]);
    assert_eq!(second.changed_blocks(), &[block(9)]);
    assert_eq!(session.primary_block(), block(9));
}

#[test]
fn comments_and_anchors_map_or_orphan_with_document_edits() {
    let comment_id = comment(7);
    let mut canonical = load("alpha");
    canonical.comments.push(CanonicalComment {
        id: comment_id,
        range: selection(2, 5),
        body: "note".into(),
    });
    canonical.anchors.push(CanonicalAnchor {
        block: block(3),
        position: position(3),
    });
    let mounted = view(1);
    let mut session = EditorCoreSession::open(canonical).expect("open session");
    session.attach_view(mounted).expect("attach view");

    session
        .execute(
            origin(mounted),
            command(
                0,
                EditorCommandKind::InsertText {
                    at: position(0),
                    text: "new ".into(),
                },
            ),
        )
        .expect("insert before anchors");

    let anchor = session.comment_anchor(comment_id).expect("comment anchor");
    assert_eq!(anchor.range(), selection(6, 9));
    assert_eq!(anchor.block(), block(9));
    assert_eq!(anchor.quote(), "pha");
    assert!(!anchor.is_orphaned());
    assert_eq!(
        session.canonical_projection().anchors()[0].position,
        position(7)
    );
    assert_eq!(
        session.map_position(revision(0), revision(1), position(2)),
        Ok(position(6))
    );

    session
        .execute(
            origin(mounted),
            command(
                1,
                EditorCommandKind::DeleteRange {
                    range: selection(7, 8),
                },
            ),
        )
        .expect("delete through comment anchor");

    assert!(
        session
            .comment_anchor(comment_id)
            .expect("comment anchor")
            .is_orphaned()
    );
}

#[test]
fn projections_are_deterministic_and_sort_stable_comments_and_anchors() {
    let mut left_load = load("alpha");
    left_load.comments = vec![
        CanonicalComment {
            id: comment(2),
            range: selection(0, 1),
            body: "second".into(),
        },
        CanonicalComment {
            id: comment(1),
            range: selection(1, 2),
            body: "first".into(),
        },
    ];
    left_load.anchors = vec![
        CanonicalAnchor {
            block: block(2),
            position: position(2),
        },
        CanonicalAnchor {
            block: block(1),
            position: position(1),
        },
    ];
    let mut right_load = left_load.clone();
    right_load.comments.reverse();
    right_load.anchors.reverse();

    let left = EditorCoreSession::open(left_load).expect("open left session");
    let right = EditorCoreSession::open(right_load).expect("open right session");
    let left_projection = left.canonical_projection();
    let right_projection = right.canonical_projection();

    assert_eq!(left_projection, right_projection);
    assert_eq!(
        left_projection
            .comments()
            .iter()
            .map(|comment| comment.id)
            .collect::<Vec<_>>(),
        vec![comment(1), comment(2)]
    );
    assert_eq!(
        left_projection
            .anchors()
            .iter()
            .map(|anchor| anchor.block)
            .collect::<Vec<_>>(),
        vec![block(1), block(2)]
    );
}

#[test]
fn projection_queue_coalesces_consecutive_revisions_with_a_bounded_pending_set() {
    let mut queue = ProjectionQueue::new(2);
    for revision in 0..=20 {
        queue.offer(projection(revision));
    }

    assert_eq!(queue.len(), 1);
    assert_eq!(
        queue.take().map(|batch| batch.revision()),
        Some(revision(20))
    );
    assert!(queue.take().is_none());
}

#[test]
fn incremental_backlog_overflow_restarts_with_one_full_snapshot() {
    let mut queue = ProjectionQueue::new(1);
    queue.offer(projection(1));
    queue.offer(projection(3));
    let mut latest = projection(5);
    latest.blocks.push(SemanticBlockSnapshot {
        id: block(1),
        text: "latest".into(),
    });
    queue.offer(latest);

    assert!(matches!(
        queue.take(),
        Some(ProjectionBatch::FullSnapshot(projection)) if projection.revision == revision(5)
    ));
    assert!(queue.take().is_none());
}

#[test]
fn rejected_commands_leave_document_revision_and_history_unchanged() {
    let mounted = view(1);
    let mut session = EditorCoreSession::open(load("alpha")).expect("open session");
    session.attach_view(mounted).expect("attach view");
    let before = session.canonical_projection();

    let stale = session.execute(
        origin(mounted),
        command(
            7,
            EditorCommandKind::InsertText {
                at: position(0),
                text: "bad".into(),
            },
        ),
    );
    let outside = session.execute(
        origin(mounted),
        command(
            0,
            EditorCommandKind::InsertText {
                at: position(99),
                text: "bad".into(),
            },
        ),
    );

    assert!(matches!(stale, Err(EditorError::StaleCommand { .. })));
    assert!(matches!(outside, Err(EditorError::InvalidCommand { .. })));
    assert_eq!(session.revision(), revision(0));
    assert_eq!(session.canonical_projection(), before);
}

#[test]
fn exhausted_core_sequences_reject_an_edit_before_the_engine_changes() {
    let mounted = view(1);
    let mut session = EditorCoreSession::open(load("alpha")).expect("open session");
    session.attach_view(mounted).expect("attach view");
    session.inner.next_transaction = u64::MAX;
    let before = session.canonical_projection();

    let result = session.execute(
        origin(mounted),
        command(
            0,
            EditorCommandKind::InsertText {
                at: position(0),
                text: "bad".into(),
            },
        ),
    );

    assert!(matches!(result, Err(EditorError::InvalidCommand { .. })));
    assert_eq!(session.revision(), revision(0));
    assert_eq!(session.canonical_projection(), before);
}
