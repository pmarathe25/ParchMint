use super::*;
use crate::document_engine::SemanticBlockSnapshot;

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
        document: SemanticDocumentSnapshot {
            blocks: vec![SemanticBlockSnapshot {
                id: block(9),
                kind: SemanticBlockKind::Paragraph,
                attributes: BTreeMap::new(),
                text: String::new(),
                marks: Vec::new(),
                list_depth: 0,
            }],
            canonical_html: false,
        },
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
    canonical.comments.push(CanonicalComment::new(
        comment_id,
        selection(2, 5),
        "note",
        block(9),
    ));
    if let CanonicalCommentAnchor::Text { quote, .. } = &mut canonical.comments[0].anchor {
        *quote = "pha".into();
    }
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
        CanonicalComment::new(comment(2), selection(0, 1), "second", block(9)),
        CanonicalComment::new(comment(1), selection(1, 2), "first", block(9)),
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
    latest.document.blocks[0].text = "latest".into();
    queue.offer(latest);

    assert!(matches!(
        queue.take(),
        Some(ProjectionBatch::FullSnapshot(projection)) if projection.revision == revision(5)
    ));
    assert!(queue.take().is_none());
}

#[test]
fn canonical_html_projects_semantic_text_without_literal_tags() {
    let session = EditorCoreSession::open(load("<p>Hello <strong>world</strong></p>"))
        .expect("open semantic HTML");
    let projection = session.canonical_projection();

    assert_eq!(projection.body(), "<p>Hello <strong>world</strong></p>");
    assert_eq!(projection.semantic().plain_text(), "Hello world");
    assert!(!projection.semantic().plain_text().contains('<'));
    assert_eq!(projection.semantic().blocks().len(), 1);
    assert_eq!(
        projection.semantic().blocks()[0].marks()[0],
        SemanticMarkRange::new(selection(6, 11), SemanticInlineMark::Bold)
    );
}

#[test]
fn semantic_selection_clipboard_has_plain_text_and_deterministic_restricted_html() {
    let mounted = view(1);
    let mut session = EditorCoreSession::open(load(
        "<p>Alpha <strong>bold</strong></p><blockquote>Next</blockquote>",
    ))
    .expect("open semantic clipboard document");
    session.attach_view(mounted).expect("attach clipboard view");
    session
        .execute(
            origin(mounted),
            command(
                0,
                EditorCommandKind::SetSelection {
                    selection: selection(6, 15),
                },
            ),
        )
        .expect("select marked cross-block text");

    let content = session
        .selection_clipboard(mounted)
        .expect("capture selection")
        .expect("non-empty selection");
    assert_eq!(content.revision(), revision(0));
    assert_eq!(content.selection(), selection(6, 15));
    assert_eq!(content.plain_text(), "bold\nNext");
    assert_eq!(
        content.restricted_html(),
        Some("<p><strong>bold</strong></p><blockquote>Next</blockquote>")
    );
    assert!(!content.plain_text().contains('<'));

    session
        .execute(
            origin(mounted),
            command(
                0,
                EditorCommandKind::SetSelection {
                    selection: selection(10, 12),
                },
            ),
        )
        .expect("select boundary and next scalar");
    let boundary = session
        .selection_clipboard(mounted)
        .expect("capture boundary")
        .expect("boundary selection");
    assert_eq!(boundary.plain_text(), "\nN");
    assert_eq!(
        boundary.restricted_html(),
        Some("<p></p><blockquote>N</blockquote>")
    );
}

#[test]
fn collapsed_selection_clipboard_is_a_safe_no_op() {
    let mounted = view(1);
    let mut session = EditorCoreSession::open(load("plain")).expect("open plain document");
    session.attach_view(mounted).expect("attach view");

    assert_eq!(session.selection_clipboard(mounted), Ok(None));
    assert_eq!(session.revision(), revision(0));
}

#[test]
fn every_cross_block_selection_html_round_trips_to_the_same_plain_text() {
    let mounted = view(1);
    let mut session = EditorCoreSession::open(load(
        "<ul><li><strong>one</strong></li><li>two</li></ul><hr data-kind=\"scene-break\"><blockquote>x</blockquote>",
    ))
    .expect("open structured clipboard document");
    session.attach_view(mounted).expect("attach view");
    let length = session
        .canonical_projection()
        .semantic()
        .plain_text()
        .chars()
        .count();

    for start in 0..length {
        for end in start + 1..=length {
            session
                .execute(
                    origin(mounted),
                    command(
                        0,
                        EditorCommandKind::SetSelection {
                            selection: selection(start as u64, end as u64),
                        },
                    ),
                )
                .expect("set exhaustive selection");
            let content = session
                .selection_clipboard(mounted)
                .expect("serialize exhaustive selection")
                .expect("selection is non-empty");
            let parsed = semantic_html::parse(
                content
                    .restricted_html()
                    .expect("restricted selection HTML"),
                session.primary_block(),
            )
            .expect("selection HTML remains in the restricted grammar");
            assert_eq!(
                parsed.plain_text(),
                content.plain_text(),
                "selection {start}..{end}"
            );
        }
    }
}

#[test]
fn supported_block_structure_has_deterministic_semantic_projection() {
    let body = "<blockquote><p>Quoted</p></blockquote><ul><li>one</li><li><strong>two</strong></li></ul><hr data-kind=\"scene-break\">";
    let session = EditorCoreSession::open(load(body)).expect("open supported semantic blocks");
    let projection = session.canonical_projection();

    assert_eq!(
        projection.body(),
        "<blockquote>Quoted</blockquote><ul><li>one</li><li><strong>two</strong></li></ul><hr data-kind=\"scene-break\">"
    );
    assert_eq!(
        projection.semantic().plain_text(),
        "Quoted\none\ntwo\n\u{fffc}"
    );
    assert_eq!(projection.semantic().blocks().len(), 4);
    assert_eq!(
        projection.semantic().blocks()[0].kind(),
        SemanticBlockKind::BlockQuote
    );
    assert_eq!(
        projection.semantic().blocks()[1].kind(),
        SemanticBlockKind::UnorderedListItem
    );
    assert_eq!(
        projection.semantic().blocks()[3].kind(),
        SemanticBlockKind::SceneBreak
    );
}

#[test]
fn block_format_toggles_selected_blocks_and_preserves_inline_marks() {
    let mounted = view(1);
    let mut session =
        EditorCoreSession::open(load("<p><strong>one</strong></p><p>two</p><p>three</p>"))
            .expect("open block document");
    session.attach_view(mounted).expect("attach view");
    let bulleted = EditorCommandKind::ToggleBlockFormat {
        range: selection(0, 7),
        format: BlockFormatKind::BulletedList,
    };
    session
        .execute(origin(mounted), command(0, bulleted.clone()))
        .expect("toggle selected paragraphs into a list");
    assert_eq!(
        session.canonical_projection().body(),
        "<ul><li><strong>one</strong></li><li>two</li></ul><p>three</p>"
    );
    assert_eq!(session.revision(), revision(1));

    let stale = session.execute(origin(mounted), command(0, bulleted.clone()));
    assert!(matches!(stale, Err(EditorError::StaleCommand { .. })));
    assert_eq!(session.revision(), revision(1));
    session
        .execute(origin(mounted), command(1, bulleted))
        .expect("toggle selected list blocks back to paragraphs");
    assert_eq!(
        session.canonical_projection().body(),
        "<p><strong>one</strong></p><p>two</p><p>three</p>"
    );
    session
        .execute(origin(mounted), command(2, EditorCommandKind::Undo))
        .expect("undo block toggle");
    assert_eq!(
        session.canonical_projection().body(),
        "<ul><li><strong>one</strong></li><li>two</li></ul><p>three</p>"
    );
    session
        .execute(
            origin(mounted),
            command(
                3,
                EditorCommandKind::ToggleBlockFormat {
                    range: selection(0, 7),
                    format: BlockFormatKind::NumberedList,
                },
            ),
        )
        .expect("switch selected list blocks to numbered");
    assert_eq!(
        session.canonical_projection().body(),
        "<ol><li><strong>one</strong></li><li>two</li></ol><p>three</p>"
    );
}

#[test]
fn collapsed_and_cross_block_selections_toggle_block_quotes_safely() {
    let mounted = view(1);
    let mut session =
        EditorCoreSession::open(load("<p>one</p><p>two</p>")).expect("open block document");
    session.attach_view(mounted).expect("attach view");
    session
        .execute(
            origin(mounted),
            command(
                0,
                EditorCommandKind::ToggleBlockFormat {
                    range: selection(1, 1),
                    format: BlockFormatKind::BlockQuote,
                },
            ),
        )
        .expect("toggle caret block quote");
    assert_eq!(
        session.canonical_projection().body(),
        "<blockquote>one</blockquote><p>two</p>"
    );
    session
        .execute(
            origin(mounted),
            command(
                1,
                EditorCommandKind::ToggleBlockFormat {
                    range: selection(0, 7),
                    format: BlockFormatKind::BlockQuote,
                },
            ),
        )
        .expect("toggle cross-block quote");
    assert_eq!(
        session.canonical_projection().body(),
        "<blockquote>one</blockquote><blockquote>two</blockquote>"
    );
    session
        .execute(
            origin(mounted),
            command(
                2,
                EditorCommandKind::ToggleBlockFormat {
                    range: selection(0, 7),
                    format: BlockFormatKind::BlockQuote,
                },
            ),
        )
        .expect("remove cross-block quote");
    assert_eq!(
        session.canonical_projection().body(),
        "<p>one</p><p>two</p>"
    );
}

#[test]
fn block_format_rejects_atomic_ranges_without_mutation() {
    let mounted = view(1);
    let mut session =
        EditorCoreSession::open(load("<p>one</p><hr data-kind=\"scene-break\"><p>two</p>"))
            .expect("open atomic document");
    session.attach_view(mounted).expect("attach view");
    let before = session.canonical_projection();
    let result = session.execute(
        origin(mounted),
        command(
            0,
            EditorCommandKind::ToggleBlockFormat {
                range: selection(0, 8),
                format: BlockFormatKind::BulletedList,
            },
        ),
    );
    assert!(matches!(result, Err(EditorError::InvalidCommand { .. })));
    assert_eq!(session.revision(), revision(0));
    assert_eq!(session.canonical_projection(), before);
    let text_edit = session.execute(
        origin(mounted),
        command(
            0,
            EditorCommandKind::InsertText {
                at: position(4),
                text: "not a marker".into(),
            },
        ),
    );
    assert!(matches!(text_edit, Err(EditorError::InvalidCommand { .. })));
    assert_eq!(session.canonical_projection(), before);
}

#[test]
fn nested_lists_round_trip_and_depth_changes_are_revision_checked_and_undoable() {
    assert!(matches!(
        EditorCoreSession::open(load("<ul><ul><li>orphan</li></ul></ul>")),
        Err(EditorError::InvalidCommand { .. })
    ));
    let canonical = "<ul><li>one<ul><li><strong>two</strong></li></ul></li><li>three</li></ul>";
    let mounted = view(1);
    let mut session = EditorCoreSession::open(load(canonical)).expect("open nested list");
    session.attach_view(mounted).expect("attach view");
    assert_eq!(session.canonical_projection().body(), canonical);
    assert_eq!(
        session
            .canonical_projection()
            .semantic()
            .blocks()
            .iter()
            .map(SemanticBlock::list_depth)
            .collect::<Vec<_>>(),
        vec![0, 1, 0]
    );

    let mut flat = EditorCoreSession::open(load("<ul><li>one</li><li><em>two</em></li></ul>"))
        .expect("open flat list");
    flat.attach_view(mounted).expect("attach flat view");
    let indent = EditorCommandKind::AdjustListDepth {
        range: selection(4, 4),
        change: ListDepthChange::Indent,
    };
    flat.execute(origin(mounted), command(0, indent.clone()))
        .expect("indent second list item");
    assert_eq!(
        flat.canonical_projection().body(),
        "<ul><li>one<ul><li><em>two</em></li></ul></li></ul>"
    );
    assert!(matches!(
        flat.execute(origin(mounted), command(0, indent)),
        Err(EditorError::StaleCommand { .. })
    ));
    flat.execute(origin(mounted), command(1, EditorCommandKind::Undo))
        .expect("undo indent");
    assert_eq!(
        flat.canonical_projection().body(),
        "<ul><li>one</li><li><em>two</em></li></ul>"
    );
}

#[test]
fn enter_soft_break_and_list_reset_preserve_semantics() {
    let mounted = view(1);
    let mut session = EditorCoreSession::open(load("<h1 data-style-id=\"heading-1\">hello</h1>"))
        .expect("open heading");
    session.attach_view(mounted).expect("attach view");
    session
        .execute(
            origin(mounted),
            command(
                0,
                EditorCommandKind::SplitBlock {
                    selection: selection(2, 2),
                },
            ),
        )
        .expect("split heading");
    assert_eq!(
        session.canonical_projection().body(),
        "<h1 data-style-id=\"heading-1\">he</h1><p data-style-id=\"body\">llo</p>"
    );
    assert_eq!(session.selection(mounted).expect("view"), selection(3, 3));
    session
        .execute(
            origin(mounted),
            command(
                1,
                EditorCommandKind::InsertSoftBreak {
                    selection: selection(4, 4),
                },
            ),
        )
        .expect("insert soft break");
    assert!(session.canonical_projection().body().contains("l<br>lo"));

    let mut empty =
        EditorCoreSession::open(load("<ul><li></li></ul>")).expect("open empty list item");
    empty.attach_view(mounted).expect("attach empty list view");
    empty
        .execute(
            origin(mounted),
            command(
                0,
                EditorCommandKind::SplitBlock {
                    selection: selection(0, 0),
                },
            ),
        )
        .expect("reset empty list item");
    assert_eq!(
        empty.canonical_projection().body(),
        "<p data-style-id=\"body\"></p>"
    );
}

#[test]
fn semantic_fragment_insertion_preserves_blocks_marks_soft_breaks_and_undo() {
    let mounted = view(1);
    let mut session =
        EditorCoreSession::open(load("<p>before after</p>")).expect("open fragment destination");
    session.attach_view(mounted).expect("attach view");
    let fragment = SemanticFragment::new(vec![
        SemanticFragmentBlock::new(
            SemanticBlockKind::Paragraph,
            "one",
            vec![SemanticMarkRange::new(
                selection(0, 3),
                SemanticInlineMark::Bold,
            )],
        ),
        SemanticFragmentBlock::new(SemanticBlockKind::UnorderedListItem, "top", Vec::new()),
        SemanticFragmentBlock::new(SemanticBlockKind::UnorderedListItem, "nested", Vec::new())
            .with_list_depth(1),
        SemanticFragmentBlock::new(
            SemanticBlockKind::BlockQuote,
            "q\nx",
            vec![SemanticMarkRange::new(
                selection(0, 1),
                SemanticInlineMark::Link("https://e.test".into()),
            )],
        ),
    ]);
    session
        .execute(
            origin(mounted),
            command(
                0,
                EditorCommandKind::ReplaceRangeWithSemanticFragment {
                    range: selection(7, 7),
                    fragment,
                },
            ),
        )
        .expect("insert semantic fragment");
    assert_eq!(
        session.canonical_projection().body(),
        "<p>before </p><p><strong>one</strong></p><ul><li>top<ul><li>nested</li></ul></li></ul><blockquote><a href=\"https://e.test\">q</a><br>x</blockquote><p>after</p>"
    );
    assert_eq!(session.revision(), revision(1));
    assert_eq!(
        session.selection(mounted).expect("fragment caret"),
        selection(27, 27)
    );
    session
        .execute(origin(mounted), command(1, EditorCommandKind::Undo))
        .expect("undo semantic fragment");
    assert_eq!(session.canonical_projection().body(), "<p>before after</p>");
}

#[test]
fn semantic_fragment_replaces_cross_block_selection_and_rejects_invalid_input_atomically() {
    let mounted = view(1);
    let mut session =
        EditorCoreSession::open(load("<p>aa</p><p>bb</p>")).expect("open replacement destination");
    session.attach_view(mounted).expect("attach view");
    session
        .execute(
            origin(mounted),
            command(
                0,
                EditorCommandKind::ReplaceRangeWithSemanticFragment {
                    range: selection(1, 4),
                    fragment: SemanticFragment::new(vec![
                        SemanticFragmentBlock::new(SemanticBlockKind::Paragraph, "x", Vec::new()),
                        SemanticFragmentBlock::new(SemanticBlockKind::Paragraph, "y", Vec::new()),
                    ]),
                },
            ),
        )
        .expect("replace cross-block selection");
    assert_eq!(
        session.canonical_projection().body(),
        "<p>a</p><p>x</p><p>y</p><p>b</p>"
    );
    assert_eq!(
        session.selection(mounted).expect("replacement caret"),
        selection(6, 6)
    );
    let before = session.canonical_projection();
    assert!(matches!(
        session.execute(
            origin(mounted),
            command(
                0,
                EditorCommandKind::ReplaceRangeWithSemanticFragment {
                    range: selection(0, 0),
                    fragment: SemanticFragment::new(vec![SemanticFragmentBlock::new(
                        SemanticBlockKind::Paragraph,
                        "stale",
                        Vec::new(),
                    )]),
                },
            ),
        ),
        Err(EditorError::StaleCommand { .. })
    ));
    let invalid = SemanticFragment::new(vec![
        SemanticFragmentBlock::new(SemanticBlockKind::OrderedListItem, "orphan", Vec::new())
            .with_list_depth(1),
    ]);
    assert!(matches!(
        session.execute(
            origin(mounted),
            command(
                1,
                EditorCommandKind::ReplaceRangeWithSemanticFragment {
                    range: selection(0, 0),
                    fragment: invalid,
                },
            ),
        ),
        Err(EditorError::InvalidCommand { .. })
    ));
    assert_eq!(session.revision(), revision(1));
    assert_eq!(session.canonical_projection(), before);

    let oversized = SemanticFragment::new(
        (0..=MAX_SEMANTIC_FRAGMENT_BLOCKS)
            .map(|_| {
                SemanticFragmentBlock::new(SemanticBlockKind::Paragraph, String::new(), Vec::new())
            })
            .collect(),
    );
    assert!(
        session
            .execute(
                origin(mounted),
                command(
                    1,
                    EditorCommandKind::ReplaceRangeWithSemanticFragment {
                        range: selection(0, 0),
                        fragment: oversized,
                    },
                ),
            )
            .is_err()
    );
    assert_eq!(session.revision(), revision(1));
}

#[test]
fn atomic_blocks_parse_serialize_insert_split_and_undo_without_marker_text() {
    let canonical =
        "<p>before</p><hr data-kind=\"scene-break\"><hr data-kind=\"page-break\"><p>after</p>";
    let parsed = EditorCoreSession::open(load(canonical)).expect("parse atomic blocks");
    assert_eq!(parsed.canonical_projection().body(), canonical);
    assert_eq!(parsed.canonical_projection().word_count(), 2);
    assert_eq!(
        parsed.canonical_projection().semantic().blocks()[1].text(),
        ""
    );

    let mounted = view(1);
    let mut session = EditorCoreSession::open(load("<p>Hello <strong>world</strong></p>"))
        .expect("open marked paragraph");
    session.attach_view(mounted).expect("attach view");
    session
        .execute(
            origin(mounted),
            command(
                0,
                EditorCommandKind::InsertAtomicBlock {
                    selection: selection(6, 6),
                    kind: AtomicBlockKind::SceneBreak,
                },
            ),
        )
        .expect("split paragraph with scene break");
    let projection = session.canonical_projection();
    assert_eq!(
        projection.body(),
        "<p>Hello </p><hr data-kind=\"scene-break\"><p><strong>world</strong></p>"
    );
    assert_eq!(
        projection.semantic().plain_text(),
        "Hello \n\u{fffc}\nworld"
    );
    assert_eq!(projection.word_count(), 2);
    assert_eq!(projection.semantic().blocks().len(), 3);
    assert_ne!(
        projection.semantic().blocks()[0].id(),
        projection.semantic().blocks()[1].id()
    );

    session
        .execute(origin(mounted), command(1, EditorCommandKind::Undo))
        .expect("undo atomic insertion");
    assert_eq!(
        session.canonical_projection().body(),
        "<p>Hello <strong>world</strong></p>"
    );
    session
        .execute(origin(mounted), command(2, EditorCommandKind::Redo))
        .expect("redo atomic insertion");
    assert_eq!(
        session.canonical_projection().body(),
        "<p>Hello </p><hr data-kind=\"scene-break\"><p><strong>world</strong></p>"
    );
}

#[test]
fn atomic_insertion_requires_a_collapsed_selection_and_is_revision_safe() {
    let mounted = view(1);
    let mut session = EditorCoreSession::open(load("<p>text</p>")).expect("open paragraph");
    session.attach_view(mounted).expect("attach view");
    let before = session.canonical_projection();
    let invalid = session.execute(
        origin(mounted),
        command(
            0,
            EditorCommandKind::InsertAtomicBlock {
                selection: selection(0, 4),
                kind: AtomicBlockKind::PageBreak,
            },
        ),
    );
    assert!(matches!(invalid, Err(EditorError::InvalidCommand { .. })));
    assert_eq!(session.canonical_projection(), before);
    session
        .execute(
            origin(mounted),
            command(
                0,
                EditorCommandKind::InsertAtomicBlock {
                    selection: selection(4, 4),
                    kind: AtomicBlockKind::PageBreak,
                },
            ),
        )
        .expect("insert page break at caret");
    assert_eq!(
        session.canonical_projection().body(),
        "<p>text</p><hr data-kind=\"page-break\">"
    );
    let stale = session.execute(
        origin(mounted),
        command(
            0,
            EditorCommandKind::InsertAtomicBlock {
                selection: selection(4, 4),
                kind: AtomicBlockKind::SceneBreak,
            },
        ),
    );
    assert!(matches!(stale, Err(EditorError::StaleCommand { .. })));
    assert_eq!(session.revision(), revision(1));
}

#[test]
fn semantic_text_edit_serializes_valid_html_and_undo_restores_marks() {
    let mounted = view(1);
    let mut session = EditorCoreSession::open(load("<p>Hello <strong>world</strong></p>"))
        .expect("open semantic HTML");
    session.attach_view(mounted).expect("attach view");
    session
        .execute(
            origin(mounted),
            command(
                0,
                EditorCommandKind::ReplaceRange {
                    range: selection(7, 10),
                    text: "or".into(),
                },
            ),
        )
        .expect("edit marked text");
    assert_eq!(
        session.canonical_projection().body(),
        "<p>Hello <strong>word</strong></p>"
    );

    session
        .execute(origin(mounted), command(1, EditorCommandKind::Undo))
        .expect("undo semantic edit");
    assert_eq!(
        session.canonical_projection().body(),
        "<p>Hello <strong>world</strong></p>"
    );
}

#[test]
fn paragraph_style_and_bold_are_revision_checked_undoable_transactions() {
    let mounted = view(1);
    let mut session =
        EditorCoreSession::open(load("<p>Hello world</p>")).expect("open semantic HTML");
    session.attach_view(mounted).expect("attach view");

    session
        .execute(
            origin(mounted),
            command(
                0,
                EditorCommandKind::ApplyParagraphStyle {
                    range: selection(0, 0),
                    style: parchmint_editor_api::StyleCatalog::heading_1_id(),
                },
            ),
        )
        .expect("apply paragraph style");
    assert_eq!(
        session.canonical_projection().body(),
        "<p data-style-id=\"heading-1\">Hello world</p>"
    );
    session
        .execute(
            origin(mounted),
            command(
                1,
                EditorCommandKind::ToggleInlineMark {
                    range: selection(6, 11),
                    mark: InlineMarkKind::Bold,
                },
            ),
        )
        .expect("toggle bold");
    assert_eq!(session.revision(), revision(2));
    assert_eq!(
        session.canonical_projection().body(),
        "<p data-style-id=\"heading-1\">Hello <strong>world</strong></p>"
    );
    let stale = session.execute(
        origin(mounted),
        command(
            1,
            EditorCommandKind::ToggleInlineMark {
                range: selection(6, 11),
                mark: InlineMarkKind::Bold,
            },
        ),
    );
    assert!(matches!(stale, Err(EditorError::StaleCommand { .. })));
    assert_eq!(session.revision(), revision(2));

    session
        .execute(
            origin(mounted),
            command(
                2,
                EditorCommandKind::ToggleInlineMark {
                    range: selection(6, 11),
                    mark: InlineMarkKind::Bold,
                },
            ),
        )
        .expect("toggle bold off");
    assert_eq!(
        session.canonical_projection().body(),
        "<p data-style-id=\"heading-1\">Hello world</p>"
    );
    session
        .execute(origin(mounted), command(3, EditorCommandKind::Undo))
        .expect("undo bold removal");
    assert_eq!(
        session.canonical_projection().body(),
        "<p data-style-id=\"heading-1\">Hello <strong>world</strong></p>"
    );
}

#[test]
fn active_style_tracks_the_containing_caret_block_without_a_document_revision() {
    let mounted = view(1);
    let mut session = EditorCoreSession::open(load(
        "<p data-style-id=\"body\">a</p><h1 data-style-id=\"heading-1\">b</h1><blockquote>c</blockquote>",
    ))
    .expect("open styled blocks");
    session.attach_view(mounted).expect("attach view");
    assert_eq!(
        session.active_style(mounted).expect("body style"),
        StyleCatalog::body_id()
    );
    session
        .execute(
            origin(mounted),
            command(
                0,
                EditorCommandKind::SetSelection {
                    selection: selection(2, 2),
                },
            ),
        )
        .expect("move to heading");
    assert_eq!(
        session.active_style(mounted).expect("heading style"),
        StyleCatalog::heading_1_id()
    );
    session
        .execute(
            origin(mounted),
            command(
                0,
                EditorCommandKind::SetSelection {
                    selection: selection(4, 4),
                },
            ),
        )
        .expect("move to quote");
    assert_eq!(
        session.active_style(mounted).expect("quote style"),
        StyleCatalog::block_quote_id()
    );
    assert_eq!(session.revision(), revision(0));
}

#[test]
fn supported_inline_marks_parse_and_serialize_without_entering_rendered_text() {
    let body = "<p><em>italic</em> <u>under</u> <s>strike</s> <a href=\"https://example.com/a?x=1&amp;y=2\">link</a></p>";
    let session = EditorCoreSession::open(load(body)).expect("open supported inline marks");
    let projection = session.canonical_projection();

    assert_eq!(projection.body(), body);
    assert_eq!(
        projection.semantic().plain_text(),
        "italic under strike link"
    );
    assert!(!projection.semantic().plain_text().contains('<'));
    let marks = projection.semantic().blocks()[0].marks();
    assert!(
        marks
            .iter()
            .any(|mark| mark.mark() == &SemanticInlineMark::Italic)
    );
    assert!(
        marks
            .iter()
            .any(|mark| mark.mark() == &SemanticInlineMark::Underline)
    );
    assert!(
        marks
            .iter()
            .any(|mark| mark.mark() == &SemanticInlineMark::Strikethrough)
    );
    assert!(marks.iter().any(|mark| {
        mark.mark() == &SemanticInlineMark::Link("https://example.com/a?x=1&y=2".into())
    }));
}

#[test]
fn every_value_free_inline_mark_toggle_is_revision_safe_and_undoable() {
    for (mark, expected) in [
        (InlineMarkKind::Bold, "<p><strong>text</strong></p>"),
        (InlineMarkKind::Italic, "<p><em>text</em></p>"),
        (InlineMarkKind::Underline, "<p><u>text</u></p>"),
        (InlineMarkKind::Strikethrough, "<p><s>text</s></p>"),
        (
            InlineMarkKind::SmallCaps,
            "<p><span data-semantic=\"small-caps\">text</span></p>",
        ),
        (InlineMarkKind::Superscript, "<p><sup>text</sup></p>"),
        (InlineMarkKind::Subscript, "<p><sub>text</sub></p>"),
    ] {
        let mounted = view(1);
        let mut session = EditorCoreSession::open(load("<p>text</p>")).expect("open HTML");
        session.attach_view(mounted).expect("attach view");
        let toggle = EditorCommandKind::ToggleInlineMark {
            range: selection(0, 4),
            mark,
        };
        session
            .execute(origin(mounted), command(0, toggle.clone()))
            .expect("toggle mark");
        assert_eq!(session.canonical_projection().body(), expected);
        assert_eq!(session.revision(), revision(1));

        let stale = session.execute(origin(mounted), command(0, toggle));
        assert!(matches!(stale, Err(EditorError::StaleCommand { .. })));
        assert_eq!(session.revision(), revision(1));
        session
            .execute(origin(mounted), command(1, EditorCommandKind::Undo))
            .expect("undo mark");
        assert_eq!(session.canonical_projection().body(), "<p>text</p>");
    }
}

#[test]
fn overlapping_inline_marks_serialize_as_valid_deterministic_nesting() {
    let mounted = view(1);
    let mut session = EditorCoreSession::open(load("<p>abcd</p>")).expect("open HTML");
    session.attach_view(mounted).expect("attach view");
    session
        .execute(
            origin(mounted),
            command(
                0,
                EditorCommandKind::ToggleInlineMark {
                    range: selection(0, 3),
                    mark: InlineMarkKind::Bold,
                },
            ),
        )
        .expect("bold range");
    session
        .execute(
            origin(mounted),
            command(
                1,
                EditorCommandKind::ToggleInlineMark {
                    range: selection(1, 4),
                    mark: InlineMarkKind::Italic,
                },
            ),
        )
        .expect("italic overlap");
    let body = session.canonical_projection().body().to_owned();
    assert_eq!(
        body,
        "<p><strong>a</strong><strong><em>bc</em></strong><em>d</em></p>"
    );
    let reopened = EditorCoreSession::open(load(&body)).expect("reopen deterministic nesting");
    assert_eq!(reopened.canonical_projection().body(), body);
    assert_eq!(
        reopened.canonical_projection().semantic().plain_text(),
        "abcd"
    );
}

#[test]
fn links_apply_update_remove_and_reject_unsafe_or_collapsed_ranges() {
    assert!(matches!(
        EditorCoreSession::open(load("<p><a href=\"javascript:alert(1)\">bad</a></p>")),
        Err(EditorError::InvalidCommand { .. })
    ));
    let mounted = view(1);
    let mut session = EditorCoreSession::open(load("<p>hello world</p>")).expect("open HTML");
    session.attach_view(mounted).expect("attach view");
    session
        .execute(
            origin(mounted),
            command(
                0,
                EditorCommandKind::SetLink {
                    range: selection(0, 5),
                    target: Some("https://example.com".into()),
                },
            ),
        )
        .expect("apply link");
    assert_eq!(
        session.canonical_projection().body(),
        "<p><a href=\"https://example.com\">hello</a> world</p>"
    );
    session
        .execute(
            origin(mounted),
            command(
                1,
                EditorCommandKind::SetLink {
                    range: selection(0, 5),
                    target: Some("mailto:writer@example.com".into()),
                },
            ),
        )
        .expect("update link");
    assert_eq!(
        session.canonical_projection().body(),
        "<p><a href=\"mailto:writer@example.com\">hello</a> world</p>"
    );
    session
        .execute(
            origin(mounted),
            command(
                2,
                EditorCommandKind::SetLink {
                    range: selection(0, 5),
                    target: None,
                },
            ),
        )
        .expect("remove link");
    assert_eq!(session.canonical_projection().body(), "<p>hello world</p>");

    for (range, target) in [
        (selection(0, 5), Some("javascript:alert(1)".into())),
        (selection(2, 2), Some("https://example.com".into())),
        (selection(0, 5), None),
    ] {
        let before = session.canonical_projection();
        let result = session.execute(
            origin(mounted),
            command(3, EditorCommandKind::SetLink { range, target }),
        );
        assert!(matches!(result, Err(EditorError::InvalidCommand { .. })));
        assert_eq!(session.revision(), revision(3));
        assert_eq!(session.canonical_projection(), before);
    }

    session
        .execute(origin(mounted), command(3, EditorCommandKind::Undo))
        .expect("undo link removal");
    assert_eq!(
        session.canonical_projection().body(),
        "<p><a href=\"mailto:writer@example.com\">hello</a> world</p>"
    );
}

#[test]
fn partial_link_edits_split_ranges_without_creating_nested_anchors() {
    let mounted = view(1);
    let mut session =
        EditorCoreSession::open(load("<p><a href=\"https://old.example\">hello</a></p>"))
            .expect("open linked HTML");
    session.attach_view(mounted).expect("attach view");
    session
        .execute(
            origin(mounted),
            command(
                0,
                EditorCommandKind::SetLink {
                    range: selection(1, 4),
                    target: Some("https://new.example".into()),
                },
            ),
        )
        .expect("update link subset");
    assert_eq!(
        session.canonical_projection().body(),
        "<p><a href=\"https://old.example\">h</a><a href=\"https://new.example\">ell</a><a href=\"https://old.example\">o</a></p>"
    );
    session
        .execute(
            origin(mounted),
            command(
                1,
                EditorCommandKind::SetLink {
                    range: selection(2, 3),
                    target: None,
                },
            ),
        )
        .expect("remove link subset");
    assert_eq!(
        session.canonical_projection().body(),
        "<p><a href=\"https://old.example\">h</a><a href=\"https://new.example\">e</a>l<a href=\"https://new.example\">l</a><a href=\"https://old.example\">o</a></p>"
    );
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

#[test]
fn comment_mutations_are_revision_checked_undoable_and_reattachable() {
    let mounted = view(1);
    let mut session = EditorCoreSession::open(load("alpha beta")).unwrap();
    session.attach_view(mounted).unwrap();
    let thread = comment(41);
    let root = comment(42);
    let reply = comment(43);
    let created = CanonicalComment {
        id: thread,
        messages: vec![CanonicalCommentMessage {
            id: root,
            body: "Root note".into(),
            unknown_fields: BTreeMap::new(),
        }],
        resolved: false,
        anchor: CanonicalCommentAnchor::Text {
            block: block(9),
            range: selection(0, 5),
            quote: "alpha".into(),
            context_before: String::new(),
            context_after: " beta".into(),
            orphaned: false,
            unknown_fields: BTreeMap::new(),
        },
        unknown_fields: BTreeMap::new(),
    };

    session
        .execute(
            origin(mounted),
            command(0, EditorCommandKind::CreateComment { comment: created }),
        )
        .unwrap();
    session
        .execute(
            origin(mounted),
            command(
                1,
                EditorCommandKind::ReplyToComment {
                    thread,
                    message: CanonicalCommentMessage {
                        id: reply,
                        body: "Reply".into(),
                        unknown_fields: BTreeMap::new(),
                    },
                },
            ),
        )
        .unwrap();
    session
        .execute(
            origin(mounted),
            command(
                2,
                EditorCommandKind::SetCommentResolved {
                    thread,
                    resolved: true,
                },
            ),
        )
        .unwrap();
    assert!(session.canonical_projection().comments()[0].resolved);
    session
        .execute(
            origin(mounted),
            command(
                3,
                EditorCommandKind::SetCommentResolved {
                    thread,
                    resolved: false,
                },
            ),
        )
        .unwrap();
    session
        .execute(
            origin(mounted),
            command(
                4,
                EditorCommandKind::ReattachComment {
                    thread,
                    range: selection(6, 10),
                },
            ),
        )
        .unwrap();
    assert_eq!(session.comment_anchor(thread).unwrap().quote(), "beta");

    let stale = session.execute(
        origin(mounted),
        command(4, EditorCommandKind::DeleteCommentThread { thread }),
    );
    assert!(matches!(stale, Err(EditorError::StaleCommand { .. })));
    session
        .execute(
            origin(mounted),
            command(
                5,
                EditorCommandKind::DeleteCommentMessage {
                    thread,
                    message: reply,
                },
            ),
        )
        .unwrap();
    session
        .execute(
            origin(mounted),
            command(
                6,
                EditorCommandKind::DeleteCommentMessage {
                    thread,
                    message: root,
                },
            ),
        )
        .unwrap();
    assert!(session.canonical_projection().comments().is_empty());
    session
        .execute(origin(mounted), command(7, EditorCommandKind::Undo))
        .unwrap();
    assert_eq!(
        session.canonical_projection().comments()[0].messages.len(),
        1
    );
}

#[test]
fn comment_message_edits_are_revision_checked_validated_and_undoable() {
    let mounted = view(1);
    let thread = comment(10);
    let message = comment(11);
    let mut session = EditorCoreSession::open(load("alpha")).unwrap();
    session.attach_view(mounted).unwrap();
    let mut created = CanonicalComment::new(thread, selection(0, 5), "Original", block(9));
    created.messages[0].id = message;
    session
        .execute(
            origin(mounted),
            command(0, EditorCommandKind::CreateComment { comment: created }),
        )
        .unwrap();

    session
        .execute(
            origin(mounted),
            command(
                1,
                EditorCommandKind::EditCommentMessage {
                    thread,
                    message,
                    body: "Edited".into(),
                },
            ),
        )
        .unwrap();
    assert_eq!(
        session.canonical_projection().comments()[0].messages[0].body,
        "Edited"
    );

    assert!(matches!(
        session.execute(
            origin(mounted),
            command(
                1,
                EditorCommandKind::EditCommentMessage {
                    thread,
                    message,
                    body: "Stale".into(),
                },
            ),
        ),
        Err(EditorError::StaleCommand { .. })
    ));
    assert!(
        session
            .execute(
                origin(mounted),
                command(
                    2,
                    EditorCommandKind::EditCommentMessage {
                        thread,
                        message,
                        body: "  ".into(),
                    },
                ),
            )
            .is_err()
    );
    assert_eq!(session.revision(), revision(2));

    session
        .execute(origin(mounted), command(2, EditorCommandKind::Undo))
        .unwrap();
    assert_eq!(
        session.canonical_projection().comments()[0].messages[0].body,
        "Original"
    );
}

#[test]
fn orphan_conversion_is_revision_checked_and_undoable() {
    let mounted = view(1);
    let thread = comment(12);
    let mut canonical = CanonicalComment::new(thread, selection(0, 5), "Note", block(9));
    if let CanonicalCommentAnchor::Text { orphaned, .. } = &mut canonical.anchor {
        *orphaned = true;
    }
    let mut canonical_load = load("alpha");
    canonical_load.comments = vec![canonical];
    let mut session = EditorCoreSession::open(canonical_load).unwrap();
    session.attach_view(mounted).unwrap();

    session
        .execute(
            origin(mounted),
            command(0, EditorCommandKind::ConvertCommentToDocument { thread }),
        )
        .unwrap();
    assert!(matches!(
        session.canonical_projection().comments()[0].anchor,
        CanonicalCommentAnchor::Document { .. }
    ));
    assert!(matches!(
        session.execute(
            origin(mounted),
            command(0, EditorCommandKind::ConvertCommentToDocument { thread },),
        ),
        Err(EditorError::StaleCommand { .. })
    ));
    session
        .execute(origin(mounted), command(1, EditorCommandKind::Undo))
        .unwrap();
    assert!(matches!(
        session.canonical_projection().comments()[0].anchor,
        CanonicalCommentAnchor::Text { orphaned: true, .. }
    ));
}

#[test]
fn empty_comment_bodies_and_empty_reattachments_are_rejected_without_revision_change() {
    let mounted = view(1);
    let mut session = EditorCoreSession::open(load("alpha")).unwrap();
    session.attach_view(mounted).unwrap();
    let invalid = CanonicalComment::new(comment(9), selection(0, 0), "  ", block(9));
    assert!(
        session
            .execute(
                origin(mounted),
                command(0, EditorCommandKind::CreateComment { comment: invalid })
            )
            .is_err()
    );
    assert_eq!(session.revision(), revision(0));
}

#[test]
fn collapsed_and_document_comment_anchors_are_supported_but_empty_reattach_is_not() {
    let mounted = view(1);
    let mut session = EditorCoreSession::open(load("alpha")).unwrap();
    session.attach_view(mounted).unwrap();
    let position_thread = comment(51);
    session
        .execute(
            origin(mounted),
            command(
                0,
                EditorCommandKind::CreateComment {
                    comment: CanonicalComment::new(
                        position_thread,
                        selection(2, 2),
                        "Position note",
                        block(9),
                    ),
                },
            ),
        )
        .unwrap();
    assert!(
        session
            .comment_anchor(position_thread)
            .unwrap()
            .range()
            .is_collapsed()
    );

    let document_thread = comment(52);
    session
        .execute(
            origin(mounted),
            command(
                1,
                EditorCommandKind::CreateComment {
                    comment: CanonicalComment {
                        id: document_thread,
                        messages: vec![CanonicalCommentMessage {
                            id: comment(53),
                            body: "Document note".into(),
                            unknown_fields: BTreeMap::new(),
                        }],
                        resolved: false,
                        anchor: CanonicalCommentAnchor::Document {
                            unknown_fields: BTreeMap::new(),
                        },
                        unknown_fields: BTreeMap::new(),
                    },
                },
            ),
        )
        .unwrap();
    assert!(session.comment_anchor(document_thread).is_none());
    let before = session.revision();
    assert!(
        session
            .execute(
                origin(mounted),
                EditorCommand::new(
                    before,
                    EditorCommandKind::ReattachComment {
                        thread: document_thread,
                        range: selection(1, 1),
                    },
                ),
            )
            .is_err()
    );
    assert_eq!(session.revision(), before);
}
