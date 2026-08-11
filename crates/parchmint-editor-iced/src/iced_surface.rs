use iced::keyboard;
use iced::mouse;
use iced::widget::canvas::{self, Action, Canvas, Frame, Path, Text};
use iced::{Color, Element, Length, Point, Rectangle, Renderer, Size, Theme};
use parchmint_editor_api::{
    AtomicBlockKind, BlockFormatKind, BlockId, CommentDecoration, DocumentPosition, EditorAdapter,
    EditorCommand, EditorCommandKind, EditorCommandOrigin, EditorError, EditorRevision,
    EditorSelection, InlineMarkKind, ListDepthChange, SharedEditorSession, SpellcheckDecoration,
    StyleId, ViewId,
};
use std::sync::{Arc, Mutex};

use crate::adapter::EditorIcedAdapter;
use crate::layout::{BlockLayoutGeometry, EditorFontFamily, EditorRectangle, EditorViewport};

/// An sRGB surface color owned by the ParchMint editor boundary.
///
/// Iced conversion is intentionally private so callers supply semantic colors
/// without taking a dependency on the renderer's color type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorSurfaceColor {
    red: u8,
    green: u8,
    blue: u8,
    alpha: u8,
}

impl EditorSurfaceColor {
    pub const fn rgb(red: u8, green: u8, blue: u8) -> Self {
        Self::rgba(red, green, blue, u8::MAX)
    }

    pub const fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    pub const fn red(self) -> u8 {
        self.red
    }

    pub const fn green(self) -> u8 {
        self.green
    }

    pub const fn blue(self) -> u8 {
        self.blue
    }

    pub const fn alpha(self) -> u8 {
        self.alpha
    }

    fn iced(self) -> Color {
        Color::from_rgba8(
            self.red,
            self.green,
            self.blue,
            f32::from(self.alpha) / 255.0,
        )
    }
}

/// Semantic colors for the manuscript canvas and its editable prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorSurfaceTheme {
    manuscript: EditorSurfaceColor,
    text: EditorSurfaceColor,
    selection: EditorSurfaceColor,
    caret: EditorSurfaceColor,
    link: EditorSurfaceColor,
    spellcheck: EditorSurfaceColor,
    comment: EditorSurfaceColor,
}

impl EditorSurfaceTheme {
    pub const fn new(
        manuscript: EditorSurfaceColor,
        text: EditorSurfaceColor,
        selection: EditorSurfaceColor,
        caret: EditorSurfaceColor,
    ) -> Self {
        Self {
            manuscript,
            text,
            selection,
            caret,
            link: caret,
            spellcheck: EditorSurfaceColor::rgb(190, 62, 54),
            comment: EditorSurfaceColor::rgba(191, 137, 31, 96),
        }
    }

    /// The established ParchMint light manuscript palette.
    pub const fn light() -> Self {
        Self::new(
            EditorSurfaceColor::rgb(252, 251, 247),
            EditorSurfaceColor::rgb(37, 42, 39),
            EditorSurfaceColor::rgba(73, 162, 128, 71),
            EditorSurfaceColor::rgb(44, 126, 94),
        )
    }

    /// A fully dark ParchMint manuscript palette; no light prose sheet remains.
    pub const fn dark() -> Self {
        Self::new(
            EditorSurfaceColor::rgb(29, 32, 30),
            EditorSurfaceColor::rgb(232, 235, 230),
            EditorSurfaceColor::rgba(115, 202, 164, 96),
            EditorSurfaceColor::rgb(142, 223, 184),
        )
    }

    pub const fn manuscript(self) -> EditorSurfaceColor {
        self.manuscript
    }

    pub const fn text(self) -> EditorSurfaceColor {
        self.text
    }

    pub const fn selection(self) -> EditorSurfaceColor {
        self.selection
    }

    pub const fn caret(self) -> EditorSurfaceColor {
        self.caret
    }

    pub const fn link(self) -> EditorSurfaceColor {
        self.link
    }

    /// The semantic spelling-warning underline, selected for legibility on
    /// both manuscript palettes.
    pub const fn spellcheck(self) -> EditorSurfaceColor {
        self.spellcheck
    }

    pub const fn comment(self) -> EditorSurfaceColor {
        self.comment
    }
}

impl Default for EditorSurfaceTheme {
    fn default() -> Self {
        Self::light()
    }
}

/// A keyboard action supported by the mounted manuscript surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountedEditorKeyCommand {
    Backspace,
    Delete,
    MoveLeft { extend: bool },
    MoveRight { extend: bool },
    MoveUp { extend: bool },
    MoveDown { extend: bool },
    MoveLineStart { extend: bool },
    MoveLineEnd { extend: bool },
    SelectAll,
    SplitBlock,
    InsertSoftBreak,
    IndentList,
    OutdentList,
}

/// Clipboard operation requested by a focused mounted editor. Platform I/O is
/// deliberately left to the native window that owns the live capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountedEditorClipboardIntent {
    Copy,
    Cut,
    Paste,
    PasteWithoutFormatting,
}

/// A ParchMint-owned interaction emitted by a mounted editor surface.
#[derive(Debug, Clone, PartialEq)]
pub enum MountedEditorMessage {
    Focus(DocumentPosition),
    SetSelection(EditorSelection),
    Scroll {
        delta_y: f32,
        viewport: EditorViewport,
    },
    ViewportChanged(EditorViewport),
    InsertText(String),
    KeyCommand(MountedEditorKeyCommand),
    Clipboard(MountedEditorClipboardIntent),
    ToggleInlineMark(InlineMarkKind),
    SetLink(Option<String>),
    ToggleBlockFormat(BlockFormatKind),
    InsertAtomicBlock(AtomicBlockKind),
    ApplyParagraphStyle(StyleId),
    /// A secondary-button hit with independent comment and spelling targets.
    OpenSpellingMenu {
        comment_range: EditorSelection,
        spelling_range: Option<EditorSelection>,
    },
}

/// The session-local identity and semantic appearance of one mounted surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountedEditorConfig {
    session: SharedEditorSession,
    view: ViewId,
    block: BlockId,
    theme: EditorSurfaceTheme,
}

impl MountedEditorConfig {
    pub const fn new(
        session: SharedEditorSession,
        view: ViewId,
        block: BlockId,
        theme: EditorSurfaceTheme,
    ) -> Self {
        Self {
            session,
            view,
            block,
            theme,
        }
    }

    pub fn session(&self) -> SharedEditorSession {
        self.session.clone()
    }

    pub const fn view(&self) -> ViewId {
        self.view
    }

    pub const fn block(&self) -> BlockId {
        self.block
    }

    pub const fn theme(&self) -> EditorSurfaceTheme {
        self.theme
    }
}

/// The result of routing one editor interaction through the shared session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MountedEditorUpdate {
    revision: EditorRevision,
    document_changed: bool,
    active_style: StyleId,
}

impl MountedEditorUpdate {
    pub const fn revision(self) -> EditorRevision {
        self.revision
    }

    pub const fn document_changed(self) -> bool {
        self.document_changed
    }

    pub const fn active_style(self) -> StyleId {
        self.active_style
    }
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
    viewport: EditorViewport,
    theme: EditorSurfaceTheme,
    spellcheck: Vec<SpellcheckDecoration>,
    comments: Vec<CommentDecoration>,
}

struct SurfaceState {
    focused: bool,
    modifiers: keyboard::Modifiers,
    drag_anchor: Option<DocumentPosition>,
}

impl Default for SurfaceState {
    fn default() -> Self {
        Self {
            focused: false,
            modifiers: keyboard::Modifiers::NONE,
            drag_anchor: None,
        }
    }
}

impl canvas::Program<MountedEditorMessage> for EditorSurface {
    type State = SurfaceState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<MountedEditorMessage>> {
        self.sync_focus(state);
        let content = self.content();
        match event {
            iced::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.modifiers = *modifiers;
                None
            }
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let position = cursor.position_in(bounds)?;
                let document = content.geometry.hit_test(position.x, position.y)?;
                state.focused = true;
                self.set_focus(true);
                let extend = state.modifiers.contains(keyboard::Modifiers::SHIFT);
                let anchor = if extend {
                    content.selection.anchor()
                } else {
                    document
                };
                state.drag_anchor = Some(anchor);
                let message = if extend {
                    MountedEditorMessage::SetSelection(EditorSelection::new(anchor, document))
                } else {
                    MountedEditorMessage::Focus(document)
                };
                Some(Action::publish(message).and_capture())
            }
            iced::Event::Mouse(mouse::Event::CursorMoved { .. }) if state.drag_anchor.is_some() => {
                let position = cursor.position_in(bounds)?;
                let document = content.geometry.hit_test(position.x, position.y)?;
                Some(
                    Action::publish(MountedEditorMessage::SetSelection(EditorSelection::new(
                        state.drag_anchor.expect("drag anchor guard"),
                        document,
                    )))
                    .and_capture(),
                )
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.drag_anchor.take().is_some() =>
            {
                Some(Action::capture())
            }
            iced::Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                let delta_y = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => -*y * 60.0,
                    mouse::ScrollDelta::Pixels { y, .. } => -*y,
                };
                Some(
                    Action::publish(MountedEditorMessage::Scroll {
                        delta_y,
                        viewport: viewport_from_bounds(bounds)?,
                    })
                    .and_capture(),
                )
            }
            iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
                let position = cursor.position_in(bounds)?;
                let comment_range = comment_range_at(&content, position.x, position.y)?;
                Some(
                    Action::publish(MountedEditorMessage::OpenSpellingMenu {
                        comment_range,
                        spelling_range: spelling_range_at(&content, position.x, position.y),
                    })
                    .and_capture(),
                )
            }
            iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. })
                if state.focused && clipboard_shortcut(key.as_ref(), *modifiers).is_some() =>
            {
                Some(
                    Action::publish(MountedEditorMessage::Clipboard(
                        clipboard_shortcut(key.as_ref(), *modifiers)
                            .expect("guard resolved a clipboard shortcut"),
                    ))
                    .and_capture(),
                )
            }
            iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. })
                if state.focused && mounted_key_command(key.as_ref(), *modifiers).is_some() =>
            {
                let command = mounted_key_command(key.as_ref(), *modifiers)
                    .expect("guard resolved a mounted key command");
                Some(Action::publish(MountedEditorMessage::KeyCommand(command)).and_capture())
            }
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                text: Some(text), ..
            }) if state.focused && is_supported_en_us(text) => Some(
                Action::publish(MountedEditorMessage::InsertText(text.to_string())).and_capture(),
            ),
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
        frame.fill(&background, content.theme.manuscript().iced());

        for selection in content.geometry.selection_rectangles(content.selection) {
            fill_rectangle(&mut frame, selection, content.theme.selection().iced());
        }

        for scalar in content.geometry.draw_scalars() {
            if scalar.character == '\n' {
                continue;
            }
            if let Some(kind) = scalar.atomic {
                let line = EditorRectangle {
                    x: scalar.bounds.x - scalar.bounds.width * 1.5,
                    y: scalar.bounds.y + scalar.bounds.height * 0.5,
                    width: scalar.bounds.width * 4.0,
                    height: 1.0,
                };
                fill_rectangle(&mut frame, line, content.theme.text().iced());
                if kind == AtomicBlockKind::PageBreak {
                    fill_rectangle(
                        &mut frame,
                        EditorRectangle {
                            y: line.y + 3.0,
                            ..line
                        },
                        content.theme.text().iced(),
                    );
                }
                continue;
            }
            if scalar.block_start
                && scalar.block_kind == parchmint_editor_api::SemanticBlockKind::BlockQuote
            {
                fill_rectangle(
                    &mut frame,
                    EditorRectangle {
                        x: scalar.bounds.x - 12.0,
                        y: scalar.bounds.y,
                        width: 2.0,
                        height: scalar.bounds.height,
                    },
                    content.theme.text().iced(),
                );
            }
            if let Some(marker) = scalar.list_marker {
                frame.fill_text(Text {
                    content: if marker == 0 {
                        "•".to_owned()
                    } else {
                        format!("{marker}.")
                    },
                    position: Point::new(scalar.bounds.x - 20.0, scalar.bounds.y),
                    color: content.theme.text().iced(),
                    size: iced::Pixels::from(16.0),
                    ..Text::default()
                });
            }
            let raised = scalar.superscript;
            let lowered = scalar.subscript;
            let small_caps = scalar.small_caps && scalar.character.is_lowercase();
            frame.fill_text(Text {
                content: if scalar.small_caps {
                    scalar.character.to_uppercase().collect()
                } else {
                    scalar.character.to_string()
                },
                position: Point::new(
                    scalar.bounds.x,
                    scalar.bounds.y
                        + if raised {
                            -scalar.bounds.height * 0.25
                        } else if lowered {
                            scalar.bounds.height * 0.25
                        } else {
                            0.0
                        },
                ),
                color: if scalar.link {
                    content.theme.link().iced()
                } else {
                    content.theme.text().iced()
                },
                size: iced::Pixels::from(if raised || lowered || small_caps {
                    scalar.font_size * 0.75
                } else {
                    scalar.font_size
                }),
                font: iced::Font {
                    family: match scalar.font_family {
                        EditorFontFamily::SansSerif => iced::font::Family::SansSerif,
                        EditorFontFamily::Serif => iced::font::Family::Serif,
                        EditorFontFamily::Monospace => iced::font::Family::Monospace,
                    },
                    weight: if scalar.bold || scalar.font_weight >= 700 {
                        iced::font::Weight::Bold
                    } else if scalar.font_weight >= 500 {
                        iced::font::Weight::Medium
                    } else {
                        iced::font::Weight::Normal
                    },
                    style: if scalar.italic || scalar.block_italic {
                        iced::font::Style::Italic
                    } else {
                        iced::font::Style::Normal
                    },
                    ..iced::Font::default()
                },
                ..Text::default()
            });
            if scalar.underline || scalar.link {
                fill_rectangle(
                    &mut frame,
                    EditorRectangle {
                        x: scalar.bounds.x,
                        y: scalar.bounds.y + scalar.bounds.height - 2.0,
                        width: scalar.bounds.width,
                        height: 1.0,
                    },
                    if scalar.link {
                        content.theme.link().iced()
                    } else {
                        content.theme.text().iced()
                    },
                );
            }
            if scalar.strikethrough {
                fill_rectangle(
                    &mut frame,
                    EditorRectangle {
                        x: scalar.bounds.x,
                        y: scalar.bounds.y + scalar.bounds.height * 0.5,
                        width: scalar.bounds.width,
                        height: 1.0,
                    },
                    content.theme.text().iced(),
                );
            }
        }

        for decoration in &content.spellcheck {
            for rectangle in content.geometry.selection_rectangles(decoration.range()) {
                fill_rectangle(
                    &mut frame,
                    EditorRectangle {
                        x: rectangle.x,
                        y: rectangle.y + rectangle.height - 1.0,
                        width: rectangle.width,
                        height: 2.0,
                    },
                    content.theme.spellcheck().iced(),
                );
            }
        }

        for decoration in &content.comments {
            for rectangle in content.geometry.selection_rectangles(decoration.range()) {
                let rectangle = if decoration.active() {
                    rectangle
                } else {
                    EditorRectangle {
                        y: rectangle.y + rectangle.height - 2.0,
                        height: 2.0,
                        ..rectangle
                    }
                };
                fill_rectangle(&mut frame, rectangle, content.theme.comment().iced());
            }
        }

        if self.draws_focused_caret(state, &content)
            && let Some(caret) = content.geometry.caret(content.selection.head())
        {
            fill_rectangle(&mut frame, caret, content.theme.caret().iced());
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

fn spelling_range_at(content: &SurfaceContent, x: f32, y: f32) -> Option<EditorSelection> {
    let document = content.geometry.hit_test(x, y)?;
    content
        .spellcheck
        .iter()
        .map(SpellcheckDecoration::range)
        .find(|range| {
            range.start().value() <= document.value() && document.value() < range.end().value()
        })
}

fn comment_range_at(content: &SurfaceContent, x: f32, y: f32) -> Option<EditorSelection> {
    let document = content.geometry.hit_test(x, y)?;
    Some(
        if !content.selection.is_collapsed()
            && content.selection.start() <= document
            && document <= content.selection.end()
        {
            content.selection
        } else {
            EditorSelection::new(document, document)
        },
    )
}

fn viewport_from_bounds(bounds: Rectangle) -> Option<EditorViewport> {
    EditorViewport::new(bounds.width, bounds.height).ok()
}

/// A retained handle for refreshing the state observed by an existing Canvas.
#[derive(Clone)]
struct SurfaceHandle {
    content: Arc<Mutex<SurfaceContent>>,
}

impl SurfaceHandle {
    fn content(&self) -> SurfaceContent {
        self.content
            .lock()
            .expect("editor surface content lock")
            .clone()
    }

    #[cfg(test)]
    fn is_focused(&self) -> bool {
        self.content
            .lock()
            .expect("editor surface content lock")
            .focused
    }

    fn refresh_from_adapter(
        &self,
        adapter: &EditorIcedAdapter,
        session: SharedEditorSession,
        view: ViewId,
        block: BlockId,
    ) -> Result<(), parchmint_editor_api::EditorError> {
        let presentation = adapter.view_snapshot(session.clone(), view)?.presentation;
        let geometry = adapter.geometry(session.clone(), view, block)?;
        let selection = adapter.selection(session.clone(), view)?;
        let spellcheck = adapter.spellcheck_decorations(session.clone(), view)?;
        let comments = adapter.comment_decorations(session.clone(), view)?;
        let mut content =
            self.content
                .lock()
                .map_err(|_| parchmint_editor_api::EditorError::InvalidCommand {
                    reason: "editor surface content lock is poisoned",
                })?;
        content.geometry = geometry;
        content.selection = selection;
        content.spellcheck = spellcheck;
        content.comments = comments;
        content.focused = presentation.focused;
        content.viewport = presentation.viewport;
        Ok(())
    }

    fn set_theme(&self, theme: EditorSurfaceTheme) {
        self.content
            .lock()
            .expect("editor surface content lock")
            .theme = theme;
    }

    fn element(&self) -> Element<'static, MountedEditorMessage> {
        let viewport = self
            .content
            .lock()
            .expect("editor surface content lock")
            .viewport;
        editor_surface(
            Arc::clone(&self.content),
            Size::new(viewport.width, viewport.height),
        )
    }
}

fn editor_surface(
    content: Arc<Mutex<SurfaceContent>>,
    _size: Size,
) -> Element<'static, MountedEditorMessage> {
    Canvas::new(EditorSurface { content })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn mounted_surface(
    adapter: &EditorIcedAdapter,
    session: SharedEditorSession,
    view: ViewId,
    block: BlockId,
    theme: EditorSurfaceTheme,
) -> Result<SurfaceHandle, EditorError> {
    let presentation = adapter.view_snapshot(session.clone(), view)?.presentation;
    let geometry = adapter.geometry(session.clone(), view, block)?;
    let selection = adapter.selection(session.clone(), view)?;
    let spellcheck = adapter.spellcheck_decorations(session.clone(), view)?;
    let comments = adapter.comment_decorations(session.clone(), view)?;
    let content = Arc::new(Mutex::new(SurfaceContent {
        geometry,
        selection,
        focused: presentation.focused,
        viewport: presentation.viewport,
        theme,
        spellcheck,
        comments,
    }));
    let handle = SurfaceHandle {
        content: Arc::clone(&content),
    };
    Ok(handle)
}

/// Applies messages emitted by the mounted surface through the adapter boundary.
///
/// This is a synthetic/headless event bridge. It deliberately does not access
/// the operating-system clipboard or native compositor.
fn apply_surface_message(
    adapter: &EditorIcedAdapter,
    session: SharedEditorSession,
    view: ViewId,
    surface: Option<&SurfaceHandle>,
    message: MountedEditorMessage,
) -> Result<(), EditorError> {
    match message {
        MountedEditorMessage::Focus(position) => {
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
        MountedEditorMessage::SetSelection(selection) => {
            set_selection(adapter, session.clone(), view, selection)?;
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
        MountedEditorMessage::Scroll { delta_y, viewport } => {
            let snapshot = adapter.view_snapshot(session.clone(), view)?;
            let requested = (snapshot.presentation.pixel_scroll_y + delta_y).max(0.0);
            adapter.set_view_presentation(
                session,
                view,
                crate::MountedViewPresentation {
                    pixel_scroll_y: requested,
                    viewport,
                    ..snapshot.presentation
                },
            )
        }
        MountedEditorMessage::ViewportChanged(viewport) => {
            let snapshot = adapter.view_snapshot(session.clone(), view)?;
            adapter.set_view_presentation(
                session,
                view,
                crate::MountedViewPresentation {
                    viewport,
                    ..snapshot.presentation
                },
            )
        }
        MountedEditorMessage::InsertText(text) => adapter.input_en_us(session, view, &text),
        MountedEditorMessage::KeyCommand(command) => {
            let surface = surface.ok_or(EditorError::InvalidCommand {
                reason: "mounted editor key input requires retained surface state",
            })?;
            apply_key_command(adapter, session, view, surface, command)
        }
        MountedEditorMessage::Clipboard(_) => Ok(()),
        MountedEditorMessage::ToggleInlineMark(mark) => {
            let range = adapter.selection(session.clone(), view)?;
            let revision = adapter.revision(session.clone())?;
            adapter.execute(
                session,
                EditorCommandOrigin::new(view),
                EditorCommand::new(
                    revision,
                    EditorCommandKind::ToggleInlineMark { range, mark },
                ),
            )
        }
        MountedEditorMessage::SetLink(target) => {
            let range = adapter.selection(session.clone(), view)?;
            let revision = adapter.revision(session.clone())?;
            adapter.execute(
                session,
                EditorCommandOrigin::new(view),
                EditorCommand::new(revision, EditorCommandKind::SetLink { range, target }),
            )
        }
        MountedEditorMessage::ToggleBlockFormat(format) => {
            let range = adapter.selection(session.clone(), view)?;
            let revision = adapter.revision(session.clone())?;
            adapter.execute(
                session,
                EditorCommandOrigin::new(view),
                EditorCommand::new(
                    revision,
                    EditorCommandKind::ToggleBlockFormat { range, format },
                ),
            )
        }
        MountedEditorMessage::InsertAtomicBlock(kind) => {
            let selection = adapter.selection(session.clone(), view)?;
            let revision = adapter.revision(session.clone())?;
            adapter.execute(
                session,
                EditorCommandOrigin::new(view),
                EditorCommand::new(
                    revision,
                    EditorCommandKind::InsertAtomicBlock { selection, kind },
                ),
            )
        }
        MountedEditorMessage::ApplyParagraphStyle(style) => {
            let range = adapter.selection(session.clone(), view)?;
            let revision = adapter.revision(session.clone())?;
            adapter.execute(
                session,
                EditorCommandOrigin::new(view),
                EditorCommand::new(
                    revision,
                    EditorCommandKind::ApplyParagraphStyle { range, style },
                ),
            )
        }
        // The native shell owns the popover and validates its exact revision
        // before it executes an action. The canvas has already performed the
        // range hit test before publishing this message.
        MountedEditorMessage::OpenSpellingMenu { .. } => Ok(()),
    }
}

fn apply_key_command(
    adapter: &EditorIcedAdapter,
    session: SharedEditorSession,
    view: ViewId,
    surface: &SurfaceHandle,
    command: MountedEditorKeyCommand,
) -> Result<(), EditorError> {
    let selection = adapter.selection(session.clone(), view)?;
    let geometry = surface.content().geometry;
    let head = selection.head();
    let selection_or_adjacent = match command {
        MountedEditorKeyCommand::Backspace => selection
            .is_collapsed()
            .then(|| geometry.previous_caret(head))
            .flatten()
            .map(|previous| EditorSelection::new(previous, head))
            .or((!selection.is_collapsed()).then_some(selection)),
        MountedEditorKeyCommand::Delete => selection
            .is_collapsed()
            .then(|| geometry.next_caret(head))
            .flatten()
            .map(|next| EditorSelection::new(head, next))
            .or((!selection.is_collapsed()).then_some(selection)),
        MountedEditorKeyCommand::MoveLeft { extend } => {
            if !extend && !selection.is_collapsed() {
                Some(EditorSelection::new(selection.start(), selection.start()))
            } else {
                geometry
                    .previous_caret(head)
                    .map(|previous| moved_selection(selection, previous, extend))
            }
        }
        MountedEditorKeyCommand::MoveRight { extend } => {
            if !extend && !selection.is_collapsed() {
                Some(EditorSelection::new(selection.end(), selection.end()))
            } else {
                geometry
                    .next_caret(head)
                    .map(|next| moved_selection(selection, next, extend))
            }
        }
        MountedEditorKeyCommand::MoveUp { extend } => geometry
            .caret_above(head)
            .map(|target| moved_selection(selection, target, extend)),
        MountedEditorKeyCommand::MoveDown { extend } => geometry
            .caret_below(head)
            .map(|target| moved_selection(selection, target, extend)),
        MountedEditorKeyCommand::MoveLineStart { extend } => geometry
            .line_start(head)
            .map(|target| moved_selection(selection, target, extend)),
        MountedEditorKeyCommand::MoveLineEnd { extend } => geometry
            .line_end(head)
            .map(|target| moved_selection(selection, target, extend)),
        MountedEditorKeyCommand::SelectAll => Some(geometry.document_range()),
        MountedEditorKeyCommand::SplitBlock
        | MountedEditorKeyCommand::InsertSoftBreak
        | MountedEditorKeyCommand::IndentList
        | MountedEditorKeyCommand::OutdentList => Some(selection),
    };
    let Some(range) = selection_or_adjacent else {
        return Ok(());
    };
    let revision = adapter.revision(session.clone())?;
    let kind = match command {
        MountedEditorKeyCommand::Backspace | MountedEditorKeyCommand::Delete => {
            EditorCommandKind::DeleteRange { range }
        }
        MountedEditorKeyCommand::MoveLeft { .. }
        | MountedEditorKeyCommand::MoveRight { .. }
        | MountedEditorKeyCommand::MoveUp { .. }
        | MountedEditorKeyCommand::MoveDown { .. }
        | MountedEditorKeyCommand::MoveLineStart { .. }
        | MountedEditorKeyCommand::MoveLineEnd { .. }
        | MountedEditorKeyCommand::SelectAll => {
            EditorCommandKind::SetSelection { selection: range }
        }
        MountedEditorKeyCommand::SplitBlock => EditorCommandKind::SplitBlock { selection: range },
        MountedEditorKeyCommand::InsertSoftBreak => {
            EditorCommandKind::InsertSoftBreak { selection: range }
        }
        MountedEditorKeyCommand::IndentList | MountedEditorKeyCommand::OutdentList => {
            let is_list = matches!(
                geometry.block_kind_at(range.head()),
                Some(
                    parchmint_editor_api::SemanticBlockKind::UnorderedListItem
                        | parchmint_editor_api::SemanticBlockKind::OrderedListItem
                )
            );
            if is_list {
                EditorCommandKind::AdjustListDepth {
                    range,
                    change: if command == MountedEditorKeyCommand::IndentList {
                        ListDepthChange::Indent
                    } else {
                        ListDepthChange::Outdent
                    },
                }
            } else if command == MountedEditorKeyCommand::IndentList {
                EditorCommandKind::ReplaceRange {
                    range,
                    text: "\t".to_owned(),
                }
            } else {
                return Ok(());
            }
        }
    };
    adapter.execute(
        session.clone(),
        EditorCommandOrigin::new(view),
        EditorCommand::new(revision, kind),
    )?;
    if !matches!(
        command,
        MountedEditorKeyCommand::Backspace
            | MountedEditorKeyCommand::Delete
            | MountedEditorKeyCommand::SplitBlock
            | MountedEditorKeyCommand::InsertSoftBreak
            | MountedEditorKeyCommand::IndentList
            | MountedEditorKeyCommand::OutdentList
    ) {
        ensure_caret_visible(adapter, session, surface, view, range.head())?;
    }
    Ok(())
}

fn set_selection(
    adapter: &EditorIcedAdapter,
    session: SharedEditorSession,
    view: ViewId,
    selection: EditorSelection,
) -> Result<(), EditorError> {
    let revision = adapter.revision(session.clone())?;
    adapter.execute(
        session,
        EditorCommandOrigin::new(view),
        EditorCommand::new(revision, EditorCommandKind::SetSelection { selection }),
    )
}

fn moved_selection(
    selection: EditorSelection,
    target: DocumentPosition,
    extend: bool,
) -> EditorSelection {
    if extend {
        EditorSelection::new(selection.anchor(), target)
    } else {
        EditorSelection::new(target, target)
    }
}

fn ensure_caret_visible(
    adapter: &EditorIcedAdapter,
    session: SharedEditorSession,
    surface: &SurfaceHandle,
    view: ViewId,
    head: DocumentPosition,
) -> Result<(), EditorError> {
    let snapshot = adapter.view_snapshot(session.clone(), view)?;
    let Some(caret) = surface.content().geometry.caret(head) else {
        return Ok(());
    };
    let top = caret.y;
    let bottom = caret.y + caret.height;
    let delta = if top < 0.0 {
        top
    } else if bottom > snapshot.presentation.viewport.height {
        bottom - snapshot.presentation.viewport.height
    } else {
        return Ok(());
    };
    adapter.set_view_presentation(
        session,
        view,
        crate::MountedViewPresentation {
            pixel_scroll_y: (snapshot.presentation.pixel_scroll_y + delta).max(0.0),
            ..snapshot.presentation
        },
    )
}

fn is_supported_en_us(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|character| character.is_ascii_graphic() || matches!(character, ' ' | '\n' | '\t'))
}

fn mounted_key_command(
    key: keyboard::Key<&str>,
    modifiers: keyboard::Modifiers,
) -> Option<MountedEditorKeyCommand> {
    let extend = modifiers.contains(keyboard::Modifiers::SHIFT);
    match key {
        keyboard::Key::Character(value)
            if modifiers == keyboard::Modifiers::COMMAND && value.eq_ignore_ascii_case("a") =>
        {
            Some(MountedEditorKeyCommand::SelectAll)
        }
        keyboard::Key::Named(keyboard::key::Named::Enter)
            if modifiers.contains(keyboard::Modifiers::SHIFT) =>
        {
            Some(MountedEditorKeyCommand::InsertSoftBreak)
        }
        keyboard::Key::Named(keyboard::key::Named::Enter) => {
            Some(MountedEditorKeyCommand::SplitBlock)
        }
        keyboard::Key::Named(keyboard::key::Named::Tab)
            if modifiers.contains(keyboard::Modifiers::SHIFT) =>
        {
            Some(MountedEditorKeyCommand::OutdentList)
        }
        keyboard::Key::Named(keyboard::key::Named::Tab) => {
            Some(MountedEditorKeyCommand::IndentList)
        }
        keyboard::Key::Named(keyboard::key::Named::Backspace) => {
            Some(MountedEditorKeyCommand::Backspace)
        }
        keyboard::Key::Named(keyboard::key::Named::Delete) => Some(MountedEditorKeyCommand::Delete),
        keyboard::Key::Named(keyboard::key::Named::ArrowLeft) => {
            Some(MountedEditorKeyCommand::MoveLeft { extend })
        }
        keyboard::Key::Named(keyboard::key::Named::ArrowRight) => {
            Some(MountedEditorKeyCommand::MoveRight { extend })
        }
        keyboard::Key::Named(keyboard::key::Named::ArrowUp) => {
            Some(MountedEditorKeyCommand::MoveUp { extend })
        }
        keyboard::Key::Named(keyboard::key::Named::ArrowDown) => {
            Some(MountedEditorKeyCommand::MoveDown { extend })
        }
        keyboard::Key::Named(keyboard::key::Named::Home) => {
            Some(MountedEditorKeyCommand::MoveLineStart { extend })
        }
        keyboard::Key::Named(keyboard::key::Named::End) => {
            Some(MountedEditorKeyCommand::MoveLineEnd { extend })
        }
        _ => None,
    }
}

fn clipboard_shortcut(
    key: keyboard::Key<&str>,
    modifiers: keyboard::Modifiers,
) -> Option<MountedEditorClipboardIntent> {
    match key {
        keyboard::Key::Character(value)
            if modifiers == keyboard::Modifiers::COMMAND && value.eq_ignore_ascii_case("c") =>
        {
            Some(MountedEditorClipboardIntent::Copy)
        }
        keyboard::Key::Character(value)
            if modifiers == keyboard::Modifiers::COMMAND && value.eq_ignore_ascii_case("x") =>
        {
            Some(MountedEditorClipboardIntent::Cut)
        }
        keyboard::Key::Character(value)
            if modifiers == keyboard::Modifiers::COMMAND && value.eq_ignore_ascii_case("v") =>
        {
            Some(MountedEditorClipboardIntent::Paste)
        }
        keyboard::Key::Character(value)
            if modifiers == (keyboard::Modifiers::COMMAND | keyboard::Modifiers::SHIFT)
                && value.eq_ignore_ascii_case("v") =>
        {
            Some(MountedEditorClipboardIntent::PasteWithoutFormatting)
        }
        _ => None,
    }
}

/// A retained, ParchMint-owned host for one adapter-mounted Iced manuscript.
///
/// The only renderer type exposed is the final `Element` needed by the
/// production Iced composition. Session, view, cache, and Canvas state stay
/// owned by this crate.
#[derive(Clone)]
pub struct MountedEditorHost {
    adapter: EditorIcedAdapter,
    config: MountedEditorConfig,
    surface: SurfaceHandle,
}

impl MountedEditorHost {
    /// Mounts the real Iced surface from an already attached adapter view.
    pub fn mount(
        adapter: &EditorIcedAdapter,
        config: MountedEditorConfig,
    ) -> Result<Self, EditorError> {
        let surface = mounted_surface(
            adapter,
            config.session(),
            config.view(),
            config.block(),
            config.theme(),
        )?;
        Ok(Self {
            adapter: adapter.clone(),
            config,
            surface,
        })
    }

    pub fn config(&self) -> MountedEditorConfig {
        self.config.clone()
    }

    /// Builds an implementation-scoped Iced element for the outer UI crate.
    pub fn element(&self) -> Element<'static, MountedEditorMessage> {
        self.surface.element()
    }

    /// Routes a Canvas interaction through the shared editor session.
    ///
    /// `document_changed` tells the outer UI whether it should schedule its
    /// existing persistence and reprojection work.
    pub fn update(
        &self,
        message: MountedEditorMessage,
    ) -> Result<MountedEditorUpdate, EditorError> {
        let before = self.adapter.revision(self.config.session())?;
        apply_surface_message(
            &self.adapter,
            self.config.session(),
            self.config.view(),
            Some(&self.surface),
            message,
        )?;
        let revision = self.adapter.revision(self.config.session())?;
        let active_style = self
            .adapter
            .active_style(self.config.session(), self.config.view())?;
        self.surface.refresh_from_adapter(
            &self.adapter,
            self.config.session(),
            self.config.view(),
            self.config.block(),
        )?;
        Ok(MountedEditorUpdate {
            revision,
            document_changed: revision != before,
            active_style,
        })
    }

    pub fn active_style(&self) -> Result<StyleId, EditorError> {
        self.adapter
            .active_style(self.config.session(), self.config.view())
    }

    /// Returns the pane allocation currently retained by the mounted surface.
    pub fn viewport(&self) -> EditorViewport {
        self.surface
            .content
            .lock()
            .expect("mounted editor surface mutex poisoned")
            .viewport
    }

    /// Reflows this pane to the logical size assigned by the outer Iced layout.
    /// Native hosts should call this when the pane allocation changes.
    pub fn resize(&self, viewport: EditorViewport) -> Result<(), EditorError> {
        apply_surface_message(
            &self.adapter,
            self.config.session(),
            self.config.view(),
            Some(&self.surface),
            MountedEditorMessage::ViewportChanged(viewport),
        )?;
        self.refresh()
    }

    /// Refreshes retained Canvas state after the outer UI has advanced a frame.
    pub fn refresh(&self) -> Result<(), EditorError> {
        self.surface.refresh_from_adapter(
            &self.adapter,
            self.config.session(),
            self.config.view(),
            self.config.block(),
        )
    }

    /// Restores logical editor focus without moving the caret or selection.
    pub fn restore_focus(&self) -> Result<(), EditorError> {
        let snapshot = self
            .adapter
            .view_snapshot(self.config.session(), self.config.view())?;
        self.adapter.set_view_presentation(
            self.config.session(),
            self.config.view(),
            crate::MountedViewPresentation {
                focused: true,
                ..snapshot.presentation
            },
        )?;
        self.surface.refresh_from_adapter(
            &self.adapter,
            self.config.session(),
            self.config.view(),
            self.config.block(),
        )
    }

    /// Rebinds only the semantic colors; the shared session is unaffected.
    pub fn set_theme(&mut self, theme: EditorSurfaceTheme) {
        self.config.theme = theme;
        self.surface.set_theme(theme);
    }
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
    use parchmint_editor_api::{CanonicalComment, CanonicalDocumentLoad, CommentId, DocumentId};

    use super::*;
    use crate::layout::{EditorViewport, VisibleEditorBlock};
    use crate::{EditorIcedConfig, EditorResourceLimits};

    #[test]
    fn primary_modifier_maps_copy_cut_paste_and_plain_paste_shortcuts() {
        for (key, expected) in [
            ("c", MountedEditorClipboardIntent::Copy),
            ("X", MountedEditorClipboardIntent::Cut),
            ("v", MountedEditorClipboardIntent::Paste),
        ] {
            assert_eq!(
                clipboard_shortcut(keyboard::Key::Character(key), keyboard::Modifiers::COMMAND,),
                Some(expected)
            );
        }
        assert_eq!(
            clipboard_shortcut(keyboard::Key::Character("c"), keyboard::Modifiers::NONE),
            None
        );
        assert_eq!(
            clipboard_shortcut(
                keyboard::Key::Character("v"),
                keyboard::Modifiers::COMMAND | keyboard::Modifiers::SHIFT,
            ),
            Some(MountedEditorClipboardIntent::PasteWithoutFormatting)
        );
        assert_eq!(
            clipboard_shortcut(
                keyboard::Key::Character("c"),
                keyboard::Modifiers::COMMAND | keyboard::Modifiers::SHIFT,
            ),
            None
        );
        assert_eq!(
            clipboard_shortcut(keyboard::Key::Character("b"), keyboard::Modifiers::COMMAND,),
            None
        );
    }

    #[test]
    fn structural_keys_map_before_scalar_text_input() {
        assert_eq!(
            mounted_key_command(
                keyboard::Key::Named(keyboard::key::Named::Enter),
                keyboard::Modifiers::NONE,
            ),
            Some(MountedEditorKeyCommand::SplitBlock)
        );
        assert_eq!(
            mounted_key_command(
                keyboard::Key::Named(keyboard::key::Named::Enter),
                keyboard::Modifiers::SHIFT,
            ),
            Some(MountedEditorKeyCommand::InsertSoftBreak)
        );
        assert_eq!(
            mounted_key_command(
                keyboard::Key::Named(keyboard::key::Named::Tab),
                keyboard::Modifiers::SHIFT,
            ),
            Some(MountedEditorKeyCommand::OutdentList)
        );
        assert_eq!(
            mounted_key_command(
                keyboard::Key::Named(keyboard::key::Named::ArrowDown),
                keyboard::Modifiers::SHIFT,
            ),
            Some(MountedEditorKeyCommand::MoveDown { extend: true })
        );
        assert_eq!(
            mounted_key_command(
                keyboard::Key::Named(keyboard::key::Named::Home),
                keyboard::Modifiers::NONE,
            ),
            Some(MountedEditorKeyCommand::MoveLineStart { extend: false })
        );
        assert_eq!(
            mounted_key_command(keyboard::Key::Character("a"), keyboard::Modifiers::COMMAND,),
            Some(MountedEditorKeyCommand::SelectAll)
        );
    }

    #[test]
    fn mounted_navigation_extends_selection_and_scroll_is_clamped_to_content() {
        let adapter = EditorIcedAdapter::new(EditorIcedConfig::default()).expect("adapter");
        let document = DocumentId::from_bytes([51; 16]);
        let block = BlockId::from_bytes([51; 16]);
        let session = adapter
            .open_session(CanonicalDocumentLoad::new(document, "abc\ndef"))
            .expect("session");
        let view = ViewId::from_bytes([52; 16]);
        let host = adapter
            .create_view_host(parchmint_platform_api::WindowCapability::new(52, 1), view)
            .expect("host");
        adapter
            .attach_view(session.clone(), view, host)
            .expect("mount");
        let viewport = EditorViewport::new(200.0, 20.0).expect("viewport");
        adapter
            .set_view_presentation(
                session.clone(),
                view,
                crate::MountedViewPresentation::new(viewport),
            )
            .expect("presentation");
        adapter
            .cache_visible_blocks(
                session.clone(),
                view,
                [VisibleEditorBlock::new(
                    block,
                    "abc\ndef",
                    DocumentPosition::default(),
                )],
            )
            .expect("layout");
        let mounted = MountedEditorHost::mount(
            &adapter,
            MountedEditorConfig::new(session.clone(), view, block, EditorSurfaceTheme::light()),
        )
        .expect("mounted host");

        mounted
            .update(MountedEditorMessage::Focus(1.into()))
            .expect("focus");
        mounted
            .update(MountedEditorMessage::KeyCommand(
                MountedEditorKeyCommand::MoveDown { extend: false },
            ))
            .expect("move down");
        assert_eq!(
            adapter.selection(session.clone(), view).expect("selection"),
            EditorSelection::new(5.into(), 5.into())
        );
        mounted
            .update(MountedEditorMessage::KeyCommand(
                MountedEditorKeyCommand::MoveLineStart { extend: true },
            ))
            .expect("extend to line start");
        assert_eq!(
            adapter.selection(session.clone(), view).expect("selection"),
            EditorSelection::new(5.into(), 4.into())
        );
        mounted
            .update(MountedEditorMessage::KeyCommand(
                MountedEditorKeyCommand::SelectAll,
            ))
            .expect("select all");
        assert_eq!(
            adapter.selection(session.clone(), view).expect("selection"),
            EditorSelection::new(0.into(), 7.into())
        );
        mounted
            .update(MountedEditorMessage::Scroll {
                delta_y: 10_000.0,
                viewport,
            })
            .expect("bounded scroll");
        let snapshot = adapter.view_snapshot(session, view).expect("snapshot");
        assert!(snapshot.presentation.pixel_scroll_y > 0.0);
        assert!(snapshot.presentation.pixel_scroll_y < 10_000.0);
    }

    #[test]
    fn mounted_tab_inserts_literal_text_outside_lists() {
        let adapter = EditorIcedAdapter::new(EditorIcedConfig::default()).expect("adapter");
        let document = DocumentId::from_bytes([61; 16]);
        let block = BlockId::from_bytes([61; 16]);
        let session = adapter
            .open_session(CanonicalDocumentLoad::new(document, "abc"))
            .expect("session");
        let view = ViewId::from_bytes([62; 16]);
        let host = adapter
            .create_view_host(parchmint_platform_api::WindowCapability::new(62, 1), view)
            .expect("host");
        adapter
            .attach_view(session.clone(), view, host)
            .expect("mount");
        let viewport = EditorViewport::new(200.0, 80.0).expect("viewport");
        adapter
            .set_view_presentation(
                session.clone(),
                view,
                crate::MountedViewPresentation::new(viewport),
            )
            .expect("presentation");
        adapter
            .cache_visible_blocks(
                session.clone(),
                view,
                [VisibleEditorBlock::new(
                    block,
                    "abc",
                    DocumentPosition::default(),
                )],
            )
            .expect("layout");
        let mounted = MountedEditorHost::mount(
            &adapter,
            MountedEditorConfig::new(session.clone(), view, block, EditorSurfaceTheme::light()),
        )
        .expect("mounted host");

        mounted
            .update(MountedEditorMessage::Focus(1.into()))
            .expect("focus");
        mounted
            .update(MountedEditorMessage::KeyCommand(
                MountedEditorKeyCommand::IndentList,
            ))
            .expect("literal tab");

        assert_eq!(
            adapter
                .primary_visible_block(session)
                .expect("document")
                .text(),
            "a\tbc"
        );
    }

    #[test]
    fn canvas_drag_and_shift_click_preserve_the_directional_selection_anchor() {
        let viewport = EditorViewport::new(200.0, 80.0).expect("viewport");
        let geometry = BlockLayoutGeometry::build(
            &VisibleEditorBlock::new(
                BlockId::from_bytes([53; 16]),
                "abcd",
                DocumentPosition::default(),
            ),
            viewport,
            0.0,
            crate::EditorLayoutMetrics::default(),
            None,
        )
        .expect("geometry");
        let content = Arc::new(Mutex::new(SurfaceContent {
            geometry,
            selection: EditorSelection::new(0.into(), 0.into()),
            focused: false,
            viewport,
            theme: EditorSurfaceTheme::light(),
            spellcheck: Vec::new(),
            comments: Vec::new(),
        }));
        let surface = EditorSurface {
            content: Arc::clone(&content),
        };
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(200.0, 80.0));
        let mut state = SurfaceState::default();

        let press = canvas::Program::update(
            &surface,
            &mut state,
            &iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            mouse::Cursor::Available(Point::new(16.0, 16.0)),
        );
        let (message, _, _) = press.expect("press action").into_inner();
        assert_eq!(message, Some(MountedEditorMessage::Focus(0.into())));

        let drag = canvas::Program::update(
            &surface,
            &mut state,
            &iced::Event::Mouse(mouse::Event::CursorMoved {
                position: Point::new(40.0, 16.0),
            }),
            bounds,
            mouse::Cursor::Available(Point::new(40.0, 16.0)),
        );
        let (message, _, _) = drag.expect("drag action").into_inner();
        assert_eq!(
            message,
            Some(MountedEditorMessage::SetSelection(EditorSelection::new(
                0.into(),
                3.into(),
            )))
        );

        content.lock().expect("content").selection = EditorSelection::new(1.into(), 1.into());
        state.drag_anchor = None;
        state.modifiers = keyboard::Modifiers::SHIFT;
        let shift_press = canvas::Program::update(
            &surface,
            &mut state,
            &iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            bounds,
            mouse::Cursor::Available(Point::new(40.0, 16.0)),
        );
        let (message, _, _) = shift_press.expect("shift press action").into_inner();
        assert_eq!(
            message,
            Some(MountedEditorMessage::SetSelection(EditorSelection::new(
                1.into(),
                3.into(),
            )))
        );
    }

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
        let surface = mounted_surface(
            &adapter,
            session.clone(),
            left,
            block,
            EditorSurfaceTheme::light(),
        )
        .expect("surface");
        let element = surface.element();
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
            None,
            MountedEditorMessage::Focus(DocumentPosition::default()),
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
        let surface = mounted_surface(
            &adapter,
            session.clone(),
            view,
            block,
            EditorSurfaceTheme::light(),
        )
        .expect("mounted surface");
        let element = surface.element();
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

        let focus = MountedEditorMessage::Focus(
            adapter
                .geometry(session.clone(), view, block)
                .expect("focus geometry")
                .hit_test(24.0, 24.0)
                .expect("focus position"),
        );
        for message in [
            focus,
            MountedEditorMessage::InsertText("A".into()),
            MountedEditorMessage::InsertText("\n".into()),
            MountedEditorMessage::InsertText("\t".into()),
        ] {
            apply_surface_message(&adapter, session.clone(), view, None, message)
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
        assert!(matches!(
            messages.first(),
            Some(MountedEditorMessage::Focus(_))
        ));
        assert_eq!(
            &messages[1..],
            &[
                MountedEditorMessage::InsertText("A".into()),
                MountedEditorMessage::InsertText("\n".into()),
                MountedEditorMessage::InsertText("\t".into()),
            ]
        );
    }

    #[test]
    fn spelling_ranges_produce_visible_underline_geometry_and_secondary_hits() {
        let adapter = EditorIcedAdapter::new(EditorIcedConfig::default()).expect("adapter");
        let session = adapter
            .open_session(CanonicalDocumentLoad::new(
                DocumentId::from_bytes([34; 16]),
                "teh",
            ))
            .expect("session");
        let view = ViewId::from_bytes([2; 16]);
        let host = adapter
            .create_view_host(parchmint_platform_api::WindowCapability::new(2, 1), view)
            .expect("host");
        adapter
            .attach_view(session.clone(), view, host)
            .expect("mount");
        let viewport = EditorViewport::new(240.0, 100.0).expect("viewport");
        adapter
            .set_view_presentation(
                session.clone(),
                view,
                crate::MountedViewPresentation::new(viewport),
            )
            .expect("presentation");
        let block = BlockId::from_bytes([34; 16]);
        adapter
            .cache_visible_blocks(
                session.clone(),
                view,
                [VisibleEditorBlock::new(
                    block,
                    "teh",
                    DocumentPosition::default(),
                )],
            )
            .expect("layout");
        let range = EditorSelection::new(0_u64.into(), 3_u64.into());
        adapter
            .set_spellcheck_decorations(
                session.clone(),
                view,
                vec![SpellcheckDecoration::new(range)],
            )
            .expect("decoration");
        let surface = mounted_surface(&adapter, session, view, block, EditorSurfaceTheme::light())
            .expect("surface");
        let content = surface.content();
        assert_eq!(content.geometry.selection_rectangles(range).len(), 3);
        assert_eq!(spelling_range_at(&content, 18.0, 18.0), Some(range));
        assert_eq!(spelling_range_at(&content, 200.0, 80.0), None);
        assert!(
            comment_range_at(&content, 18.0, 18.0)
                .unwrap()
                .is_collapsed()
        );
        let mut selected = content;
        selected.selection = range;
        assert_eq!(comment_range_at(&selected, 18.0, 18.0), Some(range));
    }

    #[test]
    fn live_comment_anchors_render_and_selected_threads_become_active() {
        let adapter = EditorIcedAdapter::new(EditorIcedConfig::default()).expect("adapter");
        let document = DocumentId::from_bytes([44; 16]);
        let block = BlockId::from_bytes([44; 16]);
        let comment = CommentId::from_bytes([45; 16]);
        let range = EditorSelection::new(0_u64.into(), 3_u64.into());
        let mut load = CanonicalDocumentLoad::new(document, "note");
        load.comments = vec![CanonicalComment::new(comment, range, "Comment", block)];
        let session = adapter.open_session(load).expect("session");
        let view = ViewId::from_bytes([46; 16]);
        let host = adapter
            .create_view_host(parchmint_platform_api::WindowCapability::new(4, 1), view)
            .expect("host");
        adapter
            .attach_view(session.clone(), view, host)
            .expect("mount");
        adapter
            .cache_visible_blocks(
                session.clone(),
                view,
                [VisibleEditorBlock::new(
                    block,
                    "note",
                    DocumentPosition::default(),
                )],
            )
            .expect("layout");
        assert!(!adapter.comment_decorations(session.clone(), view).unwrap()[0].active());
        adapter
            .set_active_comment_decoration(session.clone(), view, Some(comment))
            .unwrap();
        let surface =
            mounted_surface(&adapter, session, view, block, EditorSurfaceTheme::light()).unwrap();
        assert!(surface.content().comments[0].active());
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
        let surface = mounted_surface(
            &adapter,
            session.clone(),
            view,
            block,
            EditorSurfaceTheme::light(),
        )
        .expect("surface");
        let element = surface.element();
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
                viewport: EditorViewport::new(240.0, 100.0).expect("viewport"),
                theme: EditorSurfaceTheme::light(),
                spellcheck: Vec::new(),
                comments: Vec::new(),
            })),
        };
        assert!(surface.draws_focused_caret(&SurfaceState::default(), &surface.content()));

        assert_tiny_skia_golden(&initial, "prefocused_surface");

        assert_eq!(simulator.typewrite("A"), iced::event::Status::Captured);
        let messages = simulator.into_messages().collect::<Vec<_>>();
        assert_eq!(messages, vec![MountedEditorMessage::InsertText("A".into())]);
    }
}
