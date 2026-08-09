//! Deterministic contracts for the custom Iced editor adapter.
//!
//! The ignored smoke tests need a real compositor, native keyboard, native
//! clipboard, and platform runner.

mod fixtures;

use iced_test::futures::futures::executor::block_on;
use parchmint_editor_api::{DocumentPosition, EditorAdapter, EditorRevision, EditorSelection};
use parchmint_editor_iced::{EditorViewport, MountedViewPresentation};
use parchmint_platform_api::UntrustedClipboardContent;

use fixtures::{adapter_with_cache_limit, block, mount, open, view, visible};

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
        block_on(adapter.project(session, EditorRevision::from(1))).body(),
        " betaalpha"
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
        block_on(adapter.project(session, EditorRevision::from(2))).body(),
        "Title\n\tBodyKeep"
    );
}

#[test]
#[ignore = "native smoke: requires a real Wayland compositor; Wayland is the priority Linux path"]
fn native_smoke_wayland_priority_linux() {}

#[test]
#[ignore = "native smoke: requires a real X11 server for Linux compatibility"]
fn native_smoke_x11_compatibility_linux() {}

#[test]
#[ignore = "native smoke: requires an ARM macOS window, keyboard, and clipboard runner"]
fn native_smoke_arm_macos() {}

#[test]
#[ignore = "native smoke: requires a Windows window, keyboard, and clipboard runner"]
fn native_smoke_windows() {}
