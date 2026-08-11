use iced::{Point, Settings, Size, Theme};
use iced_test::Simulator;
use parchmint_editor_api::{
    AtomicBlockKind, BlockFormatKind, BlockId, CanonicalDocumentLoad, DocumentId, EditorAdapter,
    EditorCommand, EditorCommandKind, EditorCommandOrigin, EditorSelection, InlineMarkKind,
    StyleCatalog, ViewId,
};
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
    assert_ne!(light.link(), light.text());
    assert_ne!(dark.link(), dark.text());

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

#[test]
fn semantic_html_mounts_without_tags_and_host_formats_the_selected_text() {
    let adapter = EditorIcedAdapter::new(EditorIcedConfig::default()).expect("adapter");
    let session = adapter
        .open_session(CanonicalDocumentLoad::new(
            DocumentId::from_bytes([81; 16]),
            "<p>Hello world</p>",
        ))
        .expect("semantic session");
    let view = ViewId::from_bytes([82; 16]);
    let capability = adapter
        .create_view_host(WindowCapability::new(81, 1), view)
        .expect("host capability");
    adapter
        .attach_view(session.clone(), view, capability)
        .expect("attached view");
    let visible = adapter
        .primary_visible_block(session.clone())
        .expect("semantic visible block");
    assert_eq!(visible.text(), "Hello world");
    assert!(!visible.text().contains('<'));
    let block = visible.block();
    adapter
        .cache_visible_blocks(session.clone(), view, [visible])
        .expect("visible layout");
    let host = MountedEditorHost::mount(
        &adapter,
        MountedEditorConfig::new(session.clone(), view, block, EditorSurfaceTheme::light()),
    )
    .expect("mounted host");
    adapter
        .execute(
            session.clone(),
            EditorCommandOrigin::new(view),
            EditorCommand::new(
                0.into(),
                EditorCommandKind::SetSelection {
                    selection: EditorSelection::new(6.into(), 11.into()),
                },
            ),
        )
        .expect("select world");

    let bold = host
        .update(parchmint_editor_iced::MountedEditorMessage::ToggleInlineMark(InlineMarkKind::Bold))
        .expect("toggle bold through mounted host");
    assert!(bold.document_changed());
    assert_eq!(bold.revision().value(), 1);
    adapter
        .next_frame(session.clone())
        .expect("advance bold frame");
    host.refresh().expect("refresh bold geometry");
    let geometry = adapter
        .geometry(session.clone(), view, block)
        .expect("bold geometry");
    assert!(
        geometry
            .draw_scalars()
            .iter()
            .filter(|scalar| (6..11).contains(&scalar.position.value()))
            .all(|scalar| scalar.bold)
    );
    assert!(
        geometry
            .draw_scalars()
            .iter()
            .filter(|scalar| scalar.position.value() < 6)
            .all(|scalar| !scalar.bold)
    );

    let styled = host
        .update(
            parchmint_editor_iced::MountedEditorMessage::ApplyParagraphStyle(
                StyleCatalog::heading_1_id(),
            ),
        )
        .expect("apply paragraph style through mounted host");
    assert!(styled.document_changed());
    assert_eq!(styled.revision().value(), 2);

    for (mark, expected_revision) in [
        (InlineMarkKind::Italic, 3),
        (InlineMarkKind::Underline, 4),
        (InlineMarkKind::Strikethrough, 5),
    ] {
        let update = host
            .update(parchmint_editor_iced::MountedEditorMessage::ToggleInlineMark(mark))
            .expect("toggle inline mark through mounted host");
        assert_eq!(update.revision().value(), expected_revision);
    }
    let linked = host
        .update(parchmint_editor_iced::MountedEditorMessage::SetLink(Some(
            "https://example.com".into(),
        )))
        .expect("set link through mounted host");
    assert_eq!(linked.revision().value(), 6);
    adapter
        .next_frame(session.clone())
        .expect("advance marked frame");
    host.refresh().expect("refresh marked geometry");
    let geometry = adapter
        .geometry(session, view, block)
        .expect("marked geometry");
    let marked = geometry
        .draw_scalars()
        .iter()
        .filter(|scalar| (6..11).contains(&scalar.position.value()))
        .collect::<Vec<_>>();
    assert!(marked.iter().all(|scalar| scalar.bold));
    assert!(marked.iter().all(|scalar| scalar.italic));
    assert!(marked.iter().all(|scalar| scalar.underline));
    assert!(marked.iter().all(|scalar| scalar.strikethrough));
    assert!(marked.iter().all(|scalar| scalar.link));

    let mut simulator =
        Simulator::with_size(Settings::default(), Size::new(640.0, 480.0), host.element());
    let snapshot = simulator
        .snapshot(&Theme::Light)
        .expect("formatted surface snapshot");
    assert!(format!("{snapshot:?}").contains("renderer: \"tiny-skia\""));
}

#[test]
fn mounted_block_commands_and_atomic_nodes_have_distinct_semantic_geometry() {
    let adapter = EditorIcedAdapter::new(EditorIcedConfig::default()).expect("adapter");
    let session = adapter
        .open_session(CanonicalDocumentLoad::new(
            DocumentId::from_bytes([91; 16]),
            "<p>one</p><hr data-kind=\"scene-break\"><hr data-kind=\"page-break\"><p>two</p>",
        ))
        .expect("atomic session");
    let view = ViewId::from_bytes([92; 16]);
    let capability = adapter
        .create_view_host(WindowCapability::new(91, 1), view)
        .expect("host capability");
    adapter
        .attach_view(session.clone(), view, capability)
        .expect("attached view");
    let visible = adapter
        .primary_visible_block(session.clone())
        .expect("semantic visible block");
    assert_eq!(visible.text(), "one\n\u{fffc}\n\u{fffc}\ntwo");
    assert!(!visible.text().contains('<'));
    let block = visible.block();
    adapter
        .cache_visible_blocks(session.clone(), view, [visible])
        .expect("visible layout");
    let host = MountedEditorHost::mount(
        &adapter,
        MountedEditorConfig::new(session.clone(), view, block, EditorSurfaceTheme::light()),
    )
    .expect("mounted host");
    let geometry = adapter
        .geometry(session.clone(), view, block)
        .expect("atomic geometry");
    let atomic = geometry
        .draw_scalars()
        .iter()
        .filter_map(|scalar| scalar.atomic)
        .collect::<Vec<_>>();
    assert_eq!(
        atomic,
        vec![AtomicBlockKind::SceneBreak, AtomicBlockKind::PageBreak]
    );

    adapter
        .execute(
            session.clone(),
            EditorCommandOrigin::new(view),
            EditorCommand::new(
                0.into(),
                EditorCommandKind::SetSelection {
                    selection: EditorSelection::new(0.into(), 3.into()),
                },
            ),
        )
        .expect("select first block");
    let formatted = host
        .update(
            parchmint_editor_iced::MountedEditorMessage::ToggleBlockFormat(
                BlockFormatKind::BulletedList,
            ),
        )
        .expect("toggle block format through host");
    assert_eq!(formatted.revision().value(), 1);
    adapter
        .execute(
            session.clone(),
            EditorCommandOrigin::new(view),
            EditorCommand::new(
                1.into(),
                EditorCommandKind::SetSelection {
                    selection: EditorSelection::new(3.into(), 3.into()),
                },
            ),
        )
        .expect("collapse insertion caret");
    let inserted = host
        .update(
            parchmint_editor_iced::MountedEditorMessage::InsertAtomicBlock(
                AtomicBlockKind::PageBreak,
            ),
        )
        .expect("insert atomic block through host");
    assert_eq!(inserted.revision().value(), 2);
    adapter.next_frame(session).expect("advance atomic frame");
    host.refresh().expect("refresh atomic surface");
    let mut simulator =
        Simulator::with_size(Settings::default(), Size::new(640.0, 480.0), host.element());
    let snapshot = simulator
        .snapshot(&Theme::Light)
        .expect("atomic surface snapshot");
    assert!(format!("{snapshot:?}").contains("renderer: \"tiny-skia\""));
}
