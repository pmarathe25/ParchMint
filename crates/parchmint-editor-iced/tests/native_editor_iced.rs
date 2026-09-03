//! Deterministic contracts for the custom Iced editor adapter.

mod fixtures;

use iced_test::futures::futures::executor::block_on;
use parchmint_editor_api::{
    DocumentPosition, EditorAdapter, EditorCommand, EditorCommandKind, EditorCommandOrigin,
    EditorRevision, EditorSelection,
};
use parchmint_editor_iced::{EditorViewport, MountedViewPresentation};
use parchmint_platform_api::UntrustedClipboardContent;
use parchmint_recovery_api::DocumentRevision;
use parchmint_save::{SaveGeneration, SaveState};

use fixtures::{
    Boundary, EditorSaveRecoveryHarness, adapter_with_cache_limit, block, document_id,
    durable_vector, mount, open, recovered_body, view, visible,
};

#[test]
fn mounts_two_views_on_one_core_session_with_independent_presentation_state() {
    let adapter = adapter_with_cache_limit(6);
    let session = open(&adapter, "alpha\nbeta");
    mount(&adapter, session.clone(), view(1));
    mount(&adapter, session.clone(), view(2));
    adapter
        .set_view_presentation(
            session.clone(),
            view(2),
            MountedViewPresentation {
                pixel_scroll_y: 160.0,
                focused: true,
                viewport: EditorViewport::new(320.0, 480.0).expect("viewport"),
            },
        )
        .expect("right presentation");

    let left = adapter
        .view_snapshot(session.clone(), view(1))
        .expect("left");
    let right = adapter.view_snapshot(session, view(2)).expect("right");
    assert_eq!(left.rendered_revision, EditorRevision::default());
    assert_eq!(left.presentation.pixel_scroll_y, 0.0);
    assert!(!left.presentation.focused);
    assert_eq!(right.presentation.pixel_scroll_y, 160.0);
    assert!(right.presentation.focused);
    assert_eq!(right.presentation.viewport.width, 320.0);
    assert_eq!(right.presentation.viewport.height, 480.0);
}

#[test]
fn unknown_view_presentation_update_is_side_effect_free() {
    let adapter = adapter_with_cache_limit(6);
    let session = open(&adapter, "alpha");
    mount(&adapter, session.clone(), view(1));
    adapter
        .set_view_presentation(
            session.clone(),
            view(1),
            MountedViewPresentation {
                pixel_scroll_y: 0.0,
                focused: true,
                viewport: EditorViewport::new(320.0, 480.0).expect("viewport"),
            },
        )
        .expect("focus the mounted view");

    let missing_view = view(2);
    let result = adapter.set_view_presentation(
        session.clone(),
        missing_view,
        MountedViewPresentation {
            pixel_scroll_y: 120.0,
            focused: true,
            viewport: EditorViewport::new(320.0, 480.0).expect("viewport"),
        },
    );

    assert!(matches!(
        result,
        Err(parchmint_editor_api::EditorError::UnknownView { view }) if view == missing_view
    ));
    assert!(
        adapter
            .view_snapshot(session, view(1))
            .expect("mounted view remains available")
            .presentation
            .focused
    );
}

#[test]
fn visible_block_cache_is_bounded_to_overscan_budget() {
    let adapter = adapter_with_cache_limit(6);
    let session = open(&adapter, "alpha");
    mount(&adapter, session.clone(), view(1));
    adapter
        .cache_visible_blocks(
            session.clone(),
            view(1),
            (1..=12).map(|value| visible(block(value), "block")),
        )
        .expect("cache visible blocks");

    assert_eq!(
        adapter
            .view_snapshot(session, view(1))
            .expect("snapshot")
            .visible_layouts,
        6
    );
}

#[test]
fn shared_edits_relayout_changed_blocks_in_both_panes_on_the_next_frame() {
    let adapter = adapter_with_cache_limit(6);
    let session = open(&adapter, "alpha");
    mount(&adapter, session.clone(), view(1));
    mount(&adapter, session.clone(), view(2));
    adapter
        .cache_visible_blocks(
            session.clone(),
            view(1),
            [visible(block(9), "alpha"), visible(block(10), "unchanged")],
        )
        .expect("left layouts");
    adapter
        .cache_visible_blocks(
            session.clone(),
            view(2),
            [visible(block(9), "alpha"), visible(block(11), "unchanged")],
        )
        .expect("right layouts");
    adapter
        .input_en_us(session.clone(), view(1), " beta")
        .expect("input");

    assert_eq!(
        adapter
            .view_snapshot(session.clone(), view(1))
            .expect("left")
            .rendered_revision,
        EditorRevision::default()
    );
    assert_eq!(
        adapter
            .view_snapshot(session.clone(), view(2))
            .expect("right")
            .rendered_revision,
        EditorRevision::default()
    );
    let frame = adapter.next_frame(session.clone()).expect("frame");
    assert_eq!(frame.revision(), EditorRevision::from(1));
    assert_eq!(frame.relayouts().len(), 2);
    assert!(
        frame
            .relayouts()
            .iter()
            .all(|relayout| relayout.block == block(9))
    );
    assert_eq!(
        adapter
            .view_snapshot(session.clone(), view(1))
            .expect("left")
            .rendered_revision,
        EditorRevision::from(1)
    );
    assert_eq!(
        adapter
            .view_snapshot(session.clone(), view(2))
            .expect("right")
            .rendered_revision,
        EditorRevision::from(1)
    );
    assert_eq!(
        block_on(adapter.project(session, EditorRevision::from(1)))
            .expect("retained projection")
            .body(),
        " betaalpha"
    );
}

#[test]
fn typing_into_a_newly_split_paragraph_refreshes_the_primary_surface_geometry() {
    let adapter = adapter_with_cache_limit(6);
    let session = open(&adapter, "First");
    let mounted_view = view(1);
    mount(&adapter, session.clone(), mounted_view);
    let primary = adapter
        .primary_visible_block(session.clone())
        .expect("primary visible projection");
    let primary_block = primary.block();
    adapter
        .cache_visible_blocks(session.clone(), mounted_view, [primary])
        .expect("cache initial document");
    adapter
        .execute(
            session.clone(),
            EditorCommandOrigin::new(mounted_view),
            EditorCommand::new(
                EditorRevision::default(),
                EditorCommandKind::SplitBlock {
                    selection: EditorSelection::new(5.into(), 5.into()),
                },
            ),
        )
        .expect("split the paragraph");
    adapter.next_frame(session.clone()).expect("refresh split");
    adapter
        .input_en_us(session.clone(), mounted_view, "Second")
        .expect("type into the new paragraph");
    adapter
        .next_frame(session.clone())
        .expect("refresh typed prose");

    assert!(
        adapter
            .geometry(session, mounted_view, primary_block)
            .expect("primary geometry")
            .draw_scalars()
            .iter()
            .any(|scalar| scalar.position == DocumentPosition::from(6))
    );
}

#[test]
fn draw_hit_test_caret_and_selection_share_one_geometry_result() {
    let adapter = adapter_with_cache_limit(6);
    let session = open(&adapter, "alpha");
    mount(&adapter, session.clone(), view(1));
    adapter
        .cache_visible_blocks(session.clone(), view(1), [visible(block(9), "alpha")])
        .expect("layout");
    let geometry = adapter
        .geometry(session, view(1), block(9))
        .expect("geometry");
    let drawn = geometry.draw_scalars()[2];

    assert_eq!(
        geometry.hit_test(drawn.bounds.x, drawn.bounds.y + 10.0),
        Some(drawn.position)
    );
    assert_eq!(
        geometry.caret(drawn.position).expect("caret").x,
        drawn.bounds.x
    );
    assert_eq!(
        geometry.selection_rectangles(EditorSelection::new(
            drawn.position,
            DocumentPosition::from(drawn.position.value() + 1),
        )),
        vec![drawn.bounds]
    );
}

#[test]
fn synthetic_input_contract_accepts_en_us_and_sanitizes_untrusted_clipboard_payload() {
    let adapter = adapter_with_cache_limit(6);
    let session = open(&adapter, "");
    mount(&adapter, session.clone(), view(1));
    adapter
        .input_en_us(session.clone(), view(1), "Title\n\tBody")
        .expect("en-US input");
    let paste = adapter
        .paste_untrusted(
            session.clone(),
            view(1),
            &UntrustedClipboardContent::empty()
                .with_html("<b>Keep</b><script>drop()</script><img src=x>"),
        )
        .expect("sanitized paste");

    assert_eq!(paste.text(), "Keep");
    assert!(paste.unsafe_content_removed());
    assert_eq!(paste.omitted_images(), 1);
    assert_eq!(
        block_on(adapter.project(session, EditorRevision::from(2)))
            .expect("retained projection")
            .body(),
        "Title\n\tBodyKeep"
    );
}

#[test]
fn rich_and_plain_paste_are_distinct_revision_checked_transactions() {
    let adapter = adapter_with_cache_limit(6);
    let session = open(&adapter, "<p></p>");
    mount(&adapter, session.clone(), view(1));
    let rich = UntrustedClipboardContent::empty()
        .with_plain_text("Keep")
        .with_html("<strong>Keep</strong><img src=x>");
    let paste = adapter
        .paste_untrusted_at(
            session.clone(),
            view(1),
            EditorSelection::default(),
            EditorRevision::default(),
            &rich,
        )
        .expect("rich paste");
    assert_eq!(paste.omitted_images(), 1);
    assert_eq!(
        block_on(adapter.project(session.clone(), EditorRevision::from(1)))
            .expect("rich projection")
            .body(),
        "<p><strong>Keep</strong></p>"
    );
    adapter
        .execute(
            session.clone(),
            EditorCommandOrigin::new(view(1)),
            EditorCommand::new(EditorRevision::from(1), EditorCommandKind::Undo),
        )
        .expect("undo rich paste");
    assert_eq!(
        block_on(adapter.project(session.clone(), EditorRevision::from(2)))
            .expect("undo projection")
            .body(),
        "<p></p>"
    );
    adapter
        .paste_untrusted_plain_at(
            session.clone(),
            view(1),
            EditorSelection::default(),
            EditorRevision::from(2),
            &rich,
        )
        .expect("plain paste");
    assert_eq!(
        block_on(adapter.project(session, EditorRevision::from(3)))
            .expect("plain projection")
            .body(),
        "<p>Keep</p>"
    );
}

#[test]
fn rich_paste_retains_multi_block_nested_structure_in_one_undoable_revision() {
    let adapter = adapter_with_cache_limit(6);
    let session = open(&adapter, "<p>before after</p>");
    mount(&adapter, session.clone(), view(1));
    let rich = UntrustedClipboardContent::empty().with_html(
        "<p><strong>one</strong></p><ul><li>top<ul><li><em>nested</em></li></ul></li></ul><blockquote><a href=\"https://e.test\">q</a><br>x</blockquote><img src=x>",
    );
    let paste = adapter
        .paste_untrusted_at(
            session.clone(),
            view(1),
            EditorSelection::new(7.into(), 7.into()),
            EditorRevision::default(),
            &rich,
        )
        .expect("structured rich paste");
    assert_eq!(paste.omitted_images(), 1);
    assert_eq!(
        adapter.revision(session.clone()).expect("revision"),
        1.into()
    );
    assert_eq!(
        block_on(adapter.project(session.clone(), 1.into()))
            .expect("structured projection")
            .body(),
        "<p>before </p><p><strong>one</strong></p><ul><li>top<ul><li><em>nested</em></li></ul></li></ul><blockquote><a href=\"https://e.test\">q</a><br>x</blockquote><p>after</p>"
    );
    adapter
        .execute(
            session.clone(),
            EditorCommandOrigin::new(view(1)),
            EditorCommand::new(1.into(), EditorCommandKind::Undo),
        )
        .expect("undo structured paste");
    assert_eq!(
        block_on(adapter.project(session, 2.into()))
            .expect("undo projection")
            .body(),
        "<p>before after</p>"
    );
}

#[test]
fn acknowledged_vector_survives_exactly_while_newer_mounted_input_remains_dirty() {
    let mut harness = EditorSaveRecoveryHarness::new("");
    let paused = [
        Boundary::BeforeProjection,
        Boundary::AfterProjection,
        Boundary::BeforeRecoveryAppend,
        Boundary::AfterRecoveryAppend,
        Boundary::BeforeSave,
        Boundary::AfterCanonicalCommit,
        Boundary::BeforeSaveAcknowledgement,
    ];
    for boundary in paused {
        harness.boundaries().pause_at(boundary);
    }

    let acknowledged = harness.type_text("one", true);
    harness.boundaries().wait_until(Boundary::BeforeProjection);
    let newer = harness.type_text(" two", false);
    assert_eq!(acknowledged, EditorRevision::from(1));
    assert_eq!(newer, EditorRevision::from(2));

    for (index, boundary) in paused.iter().copied().enumerate() {
        harness.boundaries().release(boundary);
        if let Some(next) = paused.get(index + 1).copied() {
            harness.boundaries().wait_until(next);
            assert!(harness.acknowledgements().is_empty());
        }
    }
    harness.wait_until_idle();

    let acknowledgements = harness.acknowledgements();
    assert_eq!(acknowledgements.len(), 1);
    let acknowledgement = &acknowledgements[0];
    assert_eq!(
        acknowledgement.requested_revisions.open_documents[&document_id()],
        DocumentRevision::from(1)
    );
    assert_eq!(
        acknowledgement.requested_revisions.generation,
        SaveGeneration::from(1)
    );
    assert!(
        acknowledgement
            .written_revisions
            .covers(&acknowledgement.requested_revisions)
    );
    let status = harness.status();
    assert_eq!(status.state, SaveState::Dirty);
    assert_eq!(
        status.saved_through.unwrap().open_documents[&document_id()],
        DocumentRevision::from(1)
    );
    assert_eq!(harness.committed_bodies(), ["one"]);
    assert_eq!(recovered_body(&harness.replay()), Some("one two"));

    for boundary in paused {
        assert!(
            harness.boundaries().count(boundary) > 0,
            "missing named boundary {boundary:?}"
        );
    }
    harness.force_terminate();
}

#[test]
fn forced_termination_reopens_and_replays_acknowledged_recovery_once_in_order() {
    let mut harness = EditorSaveRecoveryHarness::new("");
    harness.type_text("one", false);
    harness.wait_until_idle();
    harness.type_text(" two", false);
    harness.wait_until_idle();
    harness.force_terminate();

    let replay = harness.replay_after_reopen("");
    assert_eq!(replay.accepted.len(), 2);
    assert!(replay.isolated.is_empty());
    assert_eq!(
        replay
            .accepted
            .iter()
            .map(|batch| batch.documents[&document_id()].last)
            .collect::<Vec<_>>(),
        [DocumentRevision::from(1), DocumentRevision::from(2)]
    );
    assert_eq!(recovered_body(&replay), Some("one two"));
    assert!(durable_vector(&replay).is_some());
    assert_eq!(harness.boundaries().count(Boundary::ForcedTermination), 1);
}

#[test]
fn continuous_typing_keeps_projection_and_recovery_backlog_bounded() {
    let harness = EditorSaveRecoveryHarness::new("");
    harness.boundaries().pause_at(Boundary::BeforeProjection);
    harness.type_text("x", false);
    harness.boundaries().wait_until(Boundary::BeforeProjection);
    for _ in 0..64 {
        harness.type_text("x", false);
    }

    assert!(harness.max_backlog() <= 2);
    harness.boundaries().release(Boundary::BeforeProjection);
    harness.wait_until_idle();

    assert!(harness.max_backlog() <= 2);
    assert_eq!(harness.projected_count(), 1);
    assert_eq!(harness.recovery_batch_count(), 1);
    let replay = harness.replay();
    assert_eq!(replay.accepted.len(), 1);
    let range = replay.accepted[0].documents[&document_id()];
    assert_eq!(range.first, DocumentRevision::from(1));
    assert_eq!(range.last, DocumentRevision::from(65));
    assert_eq!(recovered_body(&replay).map(str::len), Some(65));
}
