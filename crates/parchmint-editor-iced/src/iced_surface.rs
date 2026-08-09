use iced::keyboard;
use iced::mouse;
use iced::widget::canvas::{self, Action, Canvas, Frame, Path, Text};
use iced::{Color, Element, Length, Point, Rectangle, Renderer, Size, Theme};
use parchmint_editor_api::{
    BlockId, DocumentPosition, EditorAdapter, EditorCommand, EditorCommandKind,
    EditorCommandOrigin, EditorSelection, SharedEditorSession, ViewId,
};
use std::sync::{Arc, Mutex};

use crate::adapter::EditorIcedAdapter;
use crate::layout::{BlockLayoutGeometry, EditorRectangle};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SurfaceMessage {
    Focus(DocumentPosition),
    Insert(String),
}

#[derive(Clone)]
struct EditorSurface {
    content: Arc<Mutex<SurfaceContent>>,
}

#[derive(Clone)]
struct SurfaceContent {
    geometry: BlockLayoutGeometry,
    selection: EditorSelection,
    focused: bool,
}

#[derive(Default)]
struct SurfaceState {
    focused: bool,
}

impl canvas::Program<SurfaceMessage> for EditorSurface {
    type State = SurfaceState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<SurfaceMessage>> {
        self.sync_focus(state);
        let content = self.content();
        match event {
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let position = cursor.position_in(bounds)?;
                let document = content.geometry.hit_test(position.x, position.y)?;
                state.focused = true;
                self.set_focus(true);
                Some(Action::publish(SurfaceMessage::Focus(document)).and_capture())
            }
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                text: Some(text), ..
            }) if state.focused && is_supported_en_us(text) => {
                Some(Action::publish(SurfaceMessage::Insert(text.to_string())).and_capture())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let content = self.content();
        let mut frame = Frame::new(renderer, bounds.size());
        let background = Path::rectangle(Point::ORIGIN, bounds.size());
        frame.fill(&background, Color::from_rgb8(252, 251, 247));

        for selection in content.geometry.selection_rectangles(content.selection) {
            fill_rectangle(&mut frame, selection, Color::from_rgba8(73, 162, 128, 0.28));
        }

        for scalar in content.geometry.draw_scalars() {
            if scalar.character == '\n' {
                continue;
            }
            frame.fill_text(Text {
                content: scalar.character.to_string(),
                position: Point::new(scalar.bounds.x, scalar.bounds.y),
                color: Color::from_rgb8(37, 42, 39),
                size: iced::Pixels::from(16.0),
                ..Text::default()
            });
        }

        if self.draws_focused_caret(state, &content)
            && let Some(caret) = content.geometry.caret(content.selection.head())
        {
            fill_rectangle(&mut frame, caret, Color::from_rgb8(44, 126, 94));
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if cursor.is_over(bounds) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::default()
        }
    }
}

impl EditorSurface {
    fn content(&self) -> SurfaceContent {
        self.content
            .lock()
            .expect("editor surface content lock")
            .clone()
    }

    fn set_focus(&self, focused: bool) {
        self.content
            .lock()
            .expect("editor surface content lock")
            .focused = focused;
    }

    fn sync_focus(&self, state: &mut SurfaceState) {
        state.focused = self.content().focused;
    }

    fn draws_focused_caret(&self, _state: &SurfaceState, content: &SurfaceContent) -> bool {
        content.focused
    }
}

/// A retained handle for refreshing the state observed by an existing Canvas.
#[derive(Clone)]
pub(crate) struct SurfaceHandle {
    content: Arc<Mutex<SurfaceContent>>,
}

impl SurfaceHandle {
    #[cfg(test)]
    fn is_focused(&self) -> bool {
        self.content
            .lock()
            .expect("editor surface content lock")
            .focused
    }

    pub(crate) fn refresh_from_adapter(
        &self,
        adapter: &EditorIcedAdapter,
        session: SharedEditorSession,
        view: ViewId,
        block: BlockId,
    ) -> Result<(), parchmint_editor_api::EditorError> {
        let presentation = adapter.view_snapshot(session.clone(), view)?.presentation;
        let geometry = adapter.geometry(session.clone(), view, block)?;
        let selection = adapter.selection(session, view)?;
        let mut content =
            self.content
                .lock()
                .map_err(|_| parchmint_editor_api::EditorError::InvalidCommand {
                    reason: "editor surface content lock is poisoned",
                })?;
        content.geometry = geometry;
        content.selection = selection;
        content.focused = presentation.focused;
        Ok(())
    }
}

fn editor_surface(
    content: Arc<Mutex<SurfaceContent>>,
    size: Size,
) -> Element<'static, SurfaceMessage> {
    Canvas::new(EditorSurface { content })
        .width(Length::Fixed(size.width))
        .height(Length::Fixed(size.height))
        .into()
}

/// Builds the private Iced surface from the adapter's mounted cache and core selection.
///
/// Keeping this constructor here prevents callers from manufacturing a surface
/// from an unrelated geometry snapshot. The public adapter boundary continues
/// to expose only ParchMint values.
pub(crate) fn mounted_surface(
    adapter: &EditorIcedAdapter,
    session: SharedEditorSession,
    view: ViewId,
    block: BlockId,
    size: Size,
) -> Result<(Element<'static, SurfaceMessage>, SurfaceHandle), parchmint_editor_api::EditorError> {
    let presentation = adapter.view_snapshot(session.clone(), view)?.presentation;
    let geometry = adapter.geometry(session.clone(), view, block)?;
    let selection = adapter.selection(session, view)?;
    let content = Arc::new(Mutex::new(SurfaceContent {
        geometry,
        selection,
        focused: presentation.focused,
    }));
    let handle = SurfaceHandle {
        content: Arc::clone(&content),
    };
    Ok((editor_surface(content, size), handle))
}

/// Applies messages emitted by the mounted surface through the adapter boundary.
///
/// This is a synthetic/headless event bridge. It deliberately does not access
/// the operating-system clipboard or native compositor.
pub(crate) fn apply_surface_message(
    adapter: &EditorIcedAdapter,
    session: SharedEditorSession,
    view: ViewId,
    message: SurfaceMessage,
) -> Result<(), parchmint_editor_api::EditorError> {
    match message {
        SurfaceMessage::Focus(position) => {
            let revision = adapter.revision(session.clone())?;
            adapter.execute(
                session.clone(),
                EditorCommandOrigin::new(view),
                EditorCommand::new(
                    revision,
                    EditorCommandKind::SetSelection {
                        selection: EditorSelection::new(position, position),
                    },
                ),
            )?;
            let snapshot = adapter.view_snapshot(session.clone(), view)?;
            adapter.set_view_presentation(
                session,
                view,
                crate::MountedViewPresentation {
                    focused: true,
                    ..snapshot.presentation
                },
            )
        }
        SurfaceMessage::Insert(text) => adapter.input_en_us(session, view, &text),
    }
}

fn is_supported_en_us(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|character| character.is_ascii_graphic() || matches!(character, ' ' | '\n' | '\t'))
}

fn fill_rectangle(frame: &mut Frame, rectangle: EditorRectangle, color: Color) {
    let path = Path::rectangle(
        Point::new(rectangle.x, rectangle.y),
        Size::new(rectangle.width, rectangle.height),
    );
    frame.fill(&path, color);
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use iced::{Settings, Size, Theme};
    use iced_test::{Simulator, simulator::Snapshot};
    use parchmint_editor_api::{CanonicalDocumentLoad, DocumentId};

    use super::*;
    use crate::layout::{EditorViewport, VisibleEditorBlock};
    use crate::{EditorIcedConfig, EditorResourceLimits};

    fn assert_tiny_skia_golden(snapshot: &Snapshot, stem: &str) {
        let renderer = format!("{snapshot:?}");
        assert!(
            renderer.contains("renderer: \"tiny-skia\""),
            "headless snapshot requires the pinned tiny-skia renderer: {renderer}"
        );
        let golden = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(format!("{stem}.sha256"));
        let checked_in_golden = golden.with_file_name(format!("{stem}-tiny-skia.sha256"));
        assert!(
            checked_in_golden.is_file(),
            "checked-in tiny-skia snapshot hash is required"
        );
        assert!(
            snapshot
                .matches_hash(&golden)
                .expect("compare checked-in tiny-skia snapshot hash")
        );
    }

    #[test]
    fn retained_surface_focus_handoff_clears_state_and_input() {
        let adapter = EditorIcedAdapter::new(EditorIcedConfig::default()).expect("adapter");
        let session = adapter
            .open_session(CanonicalDocumentLoad::new(
                DocumentId::from_bytes([35; 16]),
                "alpha",
            ))
            .expect("session");
        let left = ViewId::from_bytes([3; 16]);
        let right = ViewId::from_bytes([4; 16]);
        for (number, view) in [(3, left), (4, right)] {
            let host = adapter
                .create_view_host(
                    parchmint_platform_api::WindowCapability::new(number, 1),
                    view,
                )
                .expect("host");
            adapter
                .attach_view(session.clone(), view, host)
                .expect("mount");
        }
        let viewport = EditorViewport::new(240.0, 100.0).expect("viewport");
        adapter
            .set_view_presentation(
                session.clone(),
                left,
                crate::MountedViewPresentation {
                    focused: true,
                    pixel_scroll_y: 0.0,
                    viewport,
                },
            )
            .expect("focus left view");
        let block = BlockId::from_bytes([35; 16]);
        adapter
            .cache_visible_blocks(
                session.clone(),
                left,
                [VisibleEditorBlock::new(
                    block,
                    "alpha",
                    DocumentPosition::default(),
                )],
            )
            .expect("layout");
        let (element, surface) = mounted_surface(
            &adapter,
            session.clone(),
            left,
            block,
            Size::new(viewport.width, viewport.height),
        )
        .expect("surface");
        let mut simulator = Simulator::with_size(
            Settings::default(),
            Size::new(viewport.width, viewport.height),
            element,
        );
        simulator.snapshot(&Theme::Light).expect("focused snapshot");
        assert!(surface.is_focused());

        adapter
            .set_view_presentation(
                session.clone(),
                right,
                crate::MountedViewPresentation {
                    focused: true,
                    pixel_scroll_y: 0.0,
                    viewport,
                },
            )
            .expect("focus right view");
        surface
            .refresh_from_adapter(&adapter, session.clone(), left, block)
            .expect("refresh handed-off surface");
        assert!(!surface.is_focused());
        simulator
            .snapshot(&Theme::Light)
            .expect("unfocused snapshot");
        assert_eq!(simulator.typewrite("A"), iced::event::Status::Ignored);
    }

    #[test]
    fn unknown_surface_focus_does_not_mutate_other_view_focus() {
        let adapter = EditorIcedAdapter::new(EditorIcedConfig::default()).expect("adapter");
        let session = adapter
            .open_session(CanonicalDocumentLoad::new(
                DocumentId::from_bytes([36; 16]),
                "alpha",
            ))
            .expect("session");
        let mounted = ViewId::from_bytes([5; 16]);
        let missing = ViewId::from_bytes([6; 16]);
        let host = adapter
            .create_view_host(parchmint_platform_api::WindowCapability::new(5, 1), mounted)
            .expect("host");
        adapter
            .attach_view(session.clone(), mounted, host)
            .expect("mount");
        adapter
            .set_view_presentation(
                session.clone(),
                mounted,
                crate::MountedViewPresentation {
                    focused: true,
                    pixel_scroll_y: 0.0,
                    viewport: EditorViewport::new(240.0, 100.0).expect("viewport"),
                },
            )
            .expect("focus mounted view");

        let result = apply_surface_message(
            &adapter,
            session.clone(),
            missing,
            SurfaceMessage::Focus(DocumentPosition::default()),
        );
        assert!(matches!(
            result,
            Err(parchmint_editor_api::EditorError::UnknownView { view }) if view == missing
        ));
        assert!(
            adapter
                .view_snapshot(session, mounted)
                .expect("mounted view")
                .presentation
                .focused
        );
    }

    #[test]
    fn simulator_mounts_adapter_surface_and_propagates_focus_scroll_input() {
        let adapter = EditorIcedAdapter::new(EditorIcedConfig {
            resource_limits: EditorResourceLimits {
                max_visible_blocks_per_view: 6,
                ..EditorResourceLimits::default()
            },
            ..EditorIcedConfig::default()
        })
        .expect("adapter");
        let session = adapter
            .open_session(CanonicalDocumentLoad::new(
                DocumentId::from_bytes([33; 16]),
                "Title\nBody",
            ))
            .expect("session");
        let view = ViewId::from_bytes([1; 16]);
        let host = adapter
            .create_view_host(parchmint_platform_api::WindowCapability::new(1, 1), view)
            .expect("host");
        adapter
            .attach_view(session.clone(), view, host)
            .expect("mount");
        let viewport = EditorViewport::new(240.0, 100.0).expect("viewport");
        adapter
            .set_view_presentation(
                session.clone(),
                view,
                crate::MountedViewPresentation {
                    pixel_scroll_y: 4.0,
                    focused: false,
                    viewport,
                },
            )
            .expect("presentation");
        let block = BlockId::from_bytes([33; 16]);
        adapter
            .cache_visible_blocks(
                session.clone(),
                view,
                [VisibleEditorBlock::new(
                    block,
                    "Title\nBody",
                    DocumentPosition::default(),
                )],
            )
            .expect("layout");
        assert_eq!(
            adapter
                .geometry(session.clone(), view, block)
                .expect("adapter geometry")
                .draw_scalars()[0]
                .bounds
                .y,
            12.0
        );
        let (element, surface) = mounted_surface(
            &adapter,
            session.clone(),
            view,
            block,
            Size::new(viewport.width, viewport.height),
        )
        .expect("mounted surface");
        let mut simulator = Simulator::with_size(
            Settings::default(),
            Size::new(viewport.width, viewport.height),
            element,
        );

        simulator.point_at(Point::new(24.0, 24.0));
        let statuses = simulator.simulate(iced_test::simulator::click());
        assert!(
            statuses
                .iter()
                .any(|status| status == &iced::event::Status::Captured)
        );
        assert_eq!(simulator.typewrite("A\n\t"), iced::event::Status::Captured);

        let focus = SurfaceMessage::Focus(
            adapter
                .geometry(session.clone(), view, block)
                .expect("focus geometry")
                .hit_test(24.0, 24.0)
                .expect("focus position"),
        );
        for message in [
            focus,
            SurfaceMessage::Insert("A".into()),
            SurfaceMessage::Insert("\n".into()),
            SurfaceMessage::Insert("\t".into()),
        ] {
            apply_surface_message(&adapter, session.clone(), view, message)
                .expect("surface message reaches adapter");
        }
        assert_eq!(
            adapter.revision(session.clone()).expect("revision"),
            3.into()
        );
        assert!(
            adapter
                .view_snapshot(session.clone(), view)
                .expect("focused mounted view")
                .presentation
                .focused
        );

        let frame = adapter.next_frame(session.clone()).expect("next frame");
        assert_eq!(frame.revision(), 3.into());
        assert_eq!(frame.relayouts().len(), 1);
        let rendered = adapter
            .view_snapshot(session.clone(), view)
            .expect("updated mounted view");
        assert_eq!(rendered.rendered_revision, 3.into());
        let updated_geometry = adapter
            .geometry(session.clone(), view, block)
            .expect("updated geometry");
        assert!(
            updated_geometry
                .draw_scalars()
                .iter()
                .any(|scalar| scalar.character == 'A')
        );
        surface
            .refresh_from_adapter(&adapter, session.clone(), view, block)
            .expect("refresh retained surface");
        let updated_snapshot = simulator
            .snapshot(&Theme::Light)
            .expect("updated retained surface snapshot");
        assert_tiny_skia_golden(&updated_snapshot, "post_edit_surface");

        let messages = simulator.into_messages().collect::<Vec<_>>();
        assert!(matches!(messages.first(), Some(SurfaceMessage::Focus(_))));
        assert_eq!(
            &messages[1..],
            &[
                SurfaceMessage::Insert("A".into()),
                SurfaceMessage::Insert("\n".into()),
                SurfaceMessage::Insert("\t".into()),
            ]
        );
    }

    #[test]
    fn prefocused_mounted_surface_initializes_canvas_focus_before_input() {
        let adapter = EditorIcedAdapter::new(EditorIcedConfig::default()).expect("adapter");
        let session = adapter
            .open_session(CanonicalDocumentLoad::new(
                DocumentId::from_bytes([34; 16]),
                "prefocused",
            ))
            .expect("session");
        let view = ViewId::from_bytes([2; 16]);
        let host = adapter
            .create_view_host(parchmint_platform_api::WindowCapability::new(2, 1), view)
            .expect("host");
        adapter
            .attach_view(session.clone(), view, host)
            .expect("mount");
        adapter
            .set_view_presentation(
                session.clone(),
                view,
                crate::MountedViewPresentation {
                    focused: true,
                    pixel_scroll_y: 4.0,
                    viewport: EditorViewport::new(240.0, 100.0).expect("viewport"),
                },
            )
            .expect("pre-focus");
        let block = BlockId::from_bytes([34; 16]);
        adapter
            .cache_visible_blocks(
                session.clone(),
                view,
                [VisibleEditorBlock::new(
                    block,
                    "prefocused",
                    DocumentPosition::default(),
                )],
            )
            .expect("layout");
        let (element, _surface) = mounted_surface(
            &adapter,
            session.clone(),
            view,
            block,
            Size::new(240.0, 100.0),
        )
        .expect("surface");
        let mut simulator =
            Simulator::with_size(Settings::default(), Size::new(240.0, 100.0), element);

        // The first snapshot is the initial render contract: it must contain
        // the caret from the mounted presentation, before input is accepted.
        let initial = simulator
            .snapshot(&Theme::Light)
            .expect("initial focused snapshot");
        let surface = EditorSurface {
            content: Arc::new(Mutex::new(SurfaceContent {
                geometry: adapter
                    .geometry(session.clone(), view, block)
                    .expect("initial geometry"),
                selection: adapter
                    .selection(session.clone(), view)
                    .expect("initial selection"),
                focused: true,
            })),
        };
        assert!(surface.draws_focused_caret(&SurfaceState::default(), &surface.content()));

        assert_tiny_skia_golden(&initial, "prefocused_surface");

        assert_eq!(simulator.typewrite("A"), iced::event::Status::Captured);
        let messages = simulator.into_messages().collect::<Vec<_>>();
        assert_eq!(messages, vec![SurfaceMessage::Insert("A".into())]);
    }
}
