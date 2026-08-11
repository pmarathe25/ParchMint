use iced::{Point, Settings, Size, Theme};
use iced_test::Simulator;
use parchmint_editor_api::{BlockId, CanonicalDocumentLoad, DocumentId, EditorAdapter, ViewId};
use parchmint_editor_iced::{
    EditorIcedAdapter, EditorIcedConfig, EditorSurfaceTheme, EditorViewport, MountedEditorConfig,
    MountedEditorHost, VisibleEditorBlock,
};
use parchmint_platform_api::WindowCapability;

fn mounted_host(theme: EditorSurfaceTheme) -> (MountedEditorHost, EditorIcedAdapter) {
    let adapter = EditorIcedAdapter::new(EditorIcedConfig::default()).expect("adapter");
    let session = adapter
        .open_session(CanonicalDocumentLoad::new(
            DocumentId::from_bytes([71; 16]),
            "alpha",
        ))
        .expect("session");
    let view = ViewId::from_bytes([72; 16]);
    let capability = adapter
        .create_view_host(WindowCapability::new(71, 1), view)
        .expect("host capability");
    adapter
        .attach_view(session.clone(), view, capability)
        .expect("attached view");
    let block = BlockId::from_bytes([71; 16]);
    adapter
        .cache_visible_blocks(
            session.clone(),
            view,
            [VisibleEditorBlock::new(block, "alpha", 0.into())],
        )
        .expect("visible layout");
    let host = MountedEditorHost::mount(
        &adapter,
        MountedEditorConfig::new(session, view, block, theme),
    )
    .expect("public mounted host");
    (host, adapter)
}

#[test]
fn public_host_mounts_and_reports_document_updates() {
    let (host, adapter) = mounted_host(EditorSurfaceTheme::light());
    let mut simulator =
        Simulator::with_size(Settings::default(), Size::new(640.0, 480.0), host.element());

    simulator.point_at(Point::new(20.0, 20.0));
    assert!(
        simulator
            .simulate(iced_test::simulator::click())
            .iter()
            .any(|status| status == &iced::event::Status::Captured)
    );
    assert_eq!(simulator.typewrite("A"), iced::event::Status::Captured);

    let updates = simulator
        .into_messages()
        .map(|message| host.update(message).expect("route public message"))
        .collect::<Vec<_>>();
    assert_eq!(updates.len(), 2);
    assert!(!updates[0].document_changed());
    assert!(updates[1].document_changed());
    assert_eq!(updates[1].revision().value(), 1);
    assert_eq!(
        adapter
            .revision(host.config().session())
            .expect("revision")
            .value(),
        1
    );

    let deletion = host
        .update(parchmint_editor_iced::MountedEditorMessage::KeyCommand(
            parchmint_editor_iced::MountedEditorKeyCommand::Backspace,
        ))
        .expect("route backspace through public host");
    assert!(deletion.document_changed());
    assert_eq!(deletion.revision().value(), 2);
}

#[test]
fn light_and_dark_semantic_themes_render_headlessly() {
    let light = EditorSurfaceTheme::light();
    let dark = EditorSurfaceTheme::dark();
    assert_ne!(light, dark);
    assert_ne!(light.manuscript(), dark.manuscript());
    assert_ne!(light.text(), dark.text());
    assert_ne!(light.selection(), dark.selection());
    assert_ne!(light.caret(), dark.caret());

    for semantic_theme in [light, dark] {
        let (host, _) = mounted_host(semantic_theme);
        let mut simulator =
            Simulator::with_size(Settings::default(), Size::new(640.0, 480.0), host.element());
        let snapshot = simulator.snapshot(&Theme::Dark).expect("headless snapshot");
        assert!(format!("{snapshot:?}").contains("renderer: \"tiny-skia\""));
    }
}

#[test]
fn configured_viewport_controls_public_element_size() {
    let (host, adapter) = mounted_host(EditorSurfaceTheme::dark());
    let session = host.config().session();
    let view = host.config().view();
    let snapshot = adapter.view_snapshot(session, view).expect("view snapshot");
    assert_eq!(
        snapshot.presentation.viewport,
        EditorViewport::new(640.0, 480.0).unwrap()
    );
    host.refresh().expect("refresh mounted host");
}
