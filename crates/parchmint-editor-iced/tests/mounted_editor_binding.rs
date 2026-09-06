use parchmint_editor_api::{
    CanonicalDocumentLoad, DocumentId, EditorAdapter, EditorRevision, ViewId,
};
use parchmint_editor_iced::{
    EditorIcedAdapter, EditorIcedConfig, EditorSurfaceTheme, EditorViewport, MountedEditorBinding,
    MountedEditorBindingConfig, MountedEditorMessage, MountedEditorSession,
};
use parchmint_platform_api::WindowCapability;

fn adapter() -> EditorIcedAdapter {
    EditorIcedAdapter::new(EditorIcedConfig::default()).expect("adapter")
}

/// Run with --release -- --ignored --nocapture on the target workstation.
#[test]
#[ignore = "opt-in workstation latency and memory measurement"]
fn chapter_authoring_performance() {
    use std::time::Instant;
    let adapter = adapter();
    let paragraph = format!(
        "<p>{}</p>",
        "The harbor lantern shines through the rain tonight. ".repeat(10)
    );
    let chapter = paragraph.repeat(250); // 20,000 words, many paragraphs.
    let mut sessions = Vec::new();
    let started = Instant::now();
    for id in 1..=8 {
        sessions.push(
            adapter
                .open_session(CanonicalDocumentLoad::new(
                    DocumentId::from_bytes([id; 16]),
                    &chapter,
                ))
                .unwrap(),
        );
    }
    let open_ms = started.elapsed().as_secs_f64() * 1000.0;
    let primary = MountedEditorBinding::mount(
        &adapter,
        config(
            MountedEditorSession::Reuse(sessions[0].clone()),
            30,
            EditorSurfaceTheme::light(),
        ),
    )
    .unwrap();
    let research = MountedEditorBinding::mount(
        &adapter,
        config(
            MountedEditorSession::Open(CanonicalDocumentLoad::new(
                DocumentId::from_bytes([20; 16]),
                paragraph.repeat(25),
            )),
            31,
            EditorSurfaceTheme::light(),
        ),
    )
    .unwrap();
    let rss_before = resident_kib();
    let mut typing = Vec::new();
    for _ in 0..512 {
        let started = Instant::now();
        primary
            .update(MountedEditorMessage::InsertText("x".into()))
            .unwrap();
        typing.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    let rss_after = resident_kib();
    let mut selecting = Vec::new();
    for offset in 0..100 {
        let started = Instant::now();
        primary
            .update(MountedEditorMessage::SetSelection(
                parchmint_editor_api::EditorSelection::new(offset.into(), (offset + 10).into()),
            ))
            .unwrap();
        selecting.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    let mut scrolling = Vec::new();
    for _ in 0..100 {
        let started = Instant::now();
        research
            .update(MountedEditorMessage::Scroll {
                delta_y: 24.0,
                viewport: EditorViewport::new(320.0, 240.0).unwrap(),
            })
            .unwrap();
        scrolling.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    let started = Instant::now();
    let body = iced::futures::executor::block_on(adapter.project(
        primary.session(),
        adapter.revision(primary.session()).unwrap(),
    ))
    .unwrap();
    let projection_ms = started.elapsed().as_secs_f64() * 1000.0;
    assert!(body.body().contains(&"x".repeat(512)));
    primary.detach().unwrap();
    let mut switches = Vec::new();
    for session in sessions {
        let started = Instant::now();
        let binding = MountedEditorBinding::mount(
            &adapter,
            config(
                MountedEditorSession::Reuse(session),
                30,
                EditorSurfaceTheme::light(),
            ),
        )
        .unwrap();
        switches.push(started.elapsed().as_secs_f64() * 1000.0);
        binding.detach().unwrap();
    }
    fn p95(samples: &mut [f64]) -> f64 {
        samples.sort_by(f64::total_cmp);
        samples[(samples.len() * 95 / 100).min(samples.len() - 1)]
    }
    eprintln!(
        "8 × 20k-word chapters + 2k Research: open={open_ms:.2}ms typing_p95={:.2}ms selection_p95={:.2}ms scroll_p95={:.2}ms switch_p95={:.2}ms projection={projection_ms:.2}ms RSS_before={rss_before:?}KiB RSS_after={rss_after:?}KiB",
        p95(&mut typing),
        p95(&mut selecting),
        p95(&mut scrolling),
        p95(&mut switches)
    );
}

fn resident_kib() -> Option<usize> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|line| {
            line.strip_prefix("VmRSS:")?
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        })
}

fn config(
    session: MountedEditorSession,
    view_byte: u8,
    theme: EditorSurfaceTheme,
) -> MountedEditorBindingConfig {
    MountedEditorBindingConfig::new(
        session,
        WindowCapability::new(u64::from(view_byte), 1),
        ViewId::from_bytes([view_byte; 16]),
        EditorViewport::new(320.0, 240.0).expect("viewport"),
        theme,
    )
}

#[test]
fn initial_mount_opens_session_initializes_primary_cache_and_detaches() {
    let adapter = adapter();
    let binding = MountedEditorBinding::mount(
        &adapter,
        config(
            MountedEditorSession::Open(CanonicalDocumentLoad::new(
                DocumentId::from_bytes([41; 16]),
                "alpha",
            )),
            42,
            EditorSurfaceTheme::light(),
        ),
    )
    .expect("initial mount");

    let snapshot = adapter
        .view_snapshot(binding.session(), binding.view())
        .expect("mounted view snapshot");
    assert_eq!(snapshot.presentation.viewport.width, 320.0);
    assert_eq!(snapshot.presentation.viewport.height, 240.0);
    assert_eq!(snapshot.visible_layouts, 1);
    assert_eq!(
        binding.host().config().block(),
        adapter
            .primary_visible_block(binding.session())
            .expect("primary")
            .block()
    );

    let session = binding.session();
    let view = binding.view();
    let detached = binding.detach().expect("detach binding");
    assert!(detached.selection().is_collapsed());
    assert!(matches!(
        adapter.view_snapshot(session, view),
        Err(parchmint_editor_api::EditorError::UnknownView { .. })
    ));
}

#[test]
fn same_document_two_view_bindings_share_changes_and_detach_independently() {
    let adapter = adapter();
    let left = MountedEditorBinding::mount(
        &adapter,
        config(
            MountedEditorSession::Open(CanonicalDocumentLoad::new(
                DocumentId::from_bytes([51; 16]),
                "alpha",
            )),
            52,
            EditorSurfaceTheme::light(),
        ),
    )
    .expect("left mount");
    let session = left.session();
    let right = MountedEditorBinding::mount(
        &adapter,
        config(
            MountedEditorSession::Reuse(session.clone()),
            53,
            EditorSurfaceTheme::dark(),
        ),
    )
    .expect("right mount");

    let update = left
        .update(MountedEditorMessage::InsertText("A".into()))
        .expect("shared input");
    assert!(update.document_changed());
    assert_eq!(update.revision(), EditorRevision::from(1));
    let frame = right.refresh().expect("refresh companion view");
    assert_eq!(frame.revision(), EditorRevision::from(1));
    assert_eq!(
        adapter
            .view_snapshot(session.clone(), right.view())
            .expect("companion snapshot")
            .rendered_revision,
        EditorRevision::from(1)
    );

    left.detach().expect("detach left");
    assert!(adapter.view_snapshot(session.clone(), right.view()).is_ok());
    right.detach().expect("detach right");
    assert!(matches!(
        adapter.view_snapshot(session, ViewId::from_bytes([53; 16])),
        Err(parchmint_editor_api::EditorError::UnknownView { .. })
    ));
}

#[test]
fn switching_between_shared_views_does_not_replace_the_prior_views_input() {
    let adapter = adapter();
    let primary = MountedEditorBinding::mount(
        &adapter,
        config(
            MountedEditorSession::Open(CanonicalDocumentLoad::new(
                DocumentId::from_bytes([54; 16]),
                "alpha alpha",
            )),
            55,
            EditorSurfaceTheme::light(),
        ),
    )
    .expect("primary mount");
    let session = primary.session();
    let companion = MountedEditorBinding::mount(
        &adapter,
        config(
            MountedEditorSession::Reuse(session.clone()),
            56,
            EditorSurfaceTheme::dark(),
        ),
    )
    .expect("companion mount");

    primary
        .update(MountedEditorMessage::Focus(3.into()))
        .expect("focus primary caret");
    primary
        .update(MountedEditorMessage::InsertText(" primary".into()))
        .expect("write primary prose");
    companion
        .update(MountedEditorMessage::Focus(3.into()))
        .expect("focus companion caret");
    companion
        .update(MountedEditorMessage::InsertText(" companion".into()))
        .expect("write companion prose");

    let revision = adapter.revision(session.clone()).expect("read revision");
    let body = iced::futures::executor::block_on(adapter.project(session, revision))
        .expect("complete synchronous adapter projection")
        .body()
        .to_owned();
    assert!(body.contains("primary"), "body was {body:?}");
    assert!(body.contains("companion"), "body was {body:?}");
}

#[test]
fn rebind_detaches_the_previous_view_before_mounting_its_replacement() {
    let adapter = adapter();
    let binding = MountedEditorBinding::mount(
        &adapter,
        config(
            MountedEditorSession::Open(CanonicalDocumentLoad::new(
                DocumentId::from_bytes([61; 16]),
                "alpha",
            )),
            62,
            EditorSurfaceTheme::light(),
        ),
    )
    .expect("initial binding");
    let session = binding.session();
    let previous_view = binding.view();

    let replacement = binding
        .rebind(config(
            MountedEditorSession::Reuse(session.clone()),
            63,
            EditorSurfaceTheme::dark(),
        ))
        .expect("replacement binding");

    assert!(matches!(
        adapter.view_snapshot(session.clone(), previous_view),
        Err(parchmint_editor_api::EditorError::UnknownView { .. })
    ));
    assert!(adapter.view_snapshot(session, replacement.view()).is_ok());
    replacement.detach().expect("detach replacement");
}
