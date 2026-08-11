use parchmint_editor_api::{CanonicalDocumentLoad, DocumentId, EditorRevision, ViewId};
use parchmint_editor_iced::{
    EditorIcedAdapter, EditorIcedConfig, EditorSurfaceTheme, EditorViewport, MountedEditorBinding,
    MountedEditorBindingConfig, MountedEditorMessage, MountedEditorSession,
};
use parchmint_platform_api::WindowCapability;

fn adapter() -> EditorIcedAdapter {
    EditorIcedAdapter::new(EditorIcedConfig::default()).expect("adapter")
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
