use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::sync::{Arc, Mutex, mpsc};

use parchmint_editor_api::{
    AsyncResult, BlockId, CanonicalDocumentLoad, CanonicalProjection, DocumentPosition,
    EditorAdapter, EditorCapabilities, EditorClipboardContent, EditorCommand, EditorCommandKind,
    EditorCommandOrigin, EditorError, EditorEvent, EditorRevision, EditorSelection,
    EditorViewState, EventStream, ProjectDocumentOperation, SearchDecoration, SelectionGeometry,
    SharedEditorSession, SpellcheckDecoration, StyleCatalogProjection, ViewHostCapability, ViewId,
};
use parchmint_editor_core::feasibility::{PasteSource, SanitizedPaste, sanitize_paste};
use parchmint_editor_core::{AppliedEditorChange, EditorCoreSession};
use parchmint_platform_api::{UntrustedClipboardContent, WindowCapability};

use crate::layout::{BlockLayoutGeometry, EditorLayoutMetrics, EditorViewport, VisibleEditorBlock};

/// Number of exact canonical revisions retained by the adapter for projections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectionBudget {
    pub retained_revisions: usize,
}

impl Default for ProjectionBudget {
    fn default() -> Self {
        Self {
            retained_revisions: 8,
        }
    }
}

/// Hard bounds for adapter-owned sessions, mounted views, layout caches, and paste input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorResourceLimits {
    pub max_sessions: usize,
    pub max_views_per_session: usize,
    pub max_visible_blocks_per_view: usize,
    pub max_clipboard_scalars: usize,
}

impl Default for EditorResourceLimits {
    fn default() -> Self {
        Self {
            max_sessions: 64,
            max_views_per_session: 2,
            max_visible_blocks_per_view: 64,
            max_clipboard_scalars: 1_000_000,
        }
    }
}

/// Construction settings expressed entirely in ParchMint-owned values.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EditorIcedConfig {
    pub projection_budget: ProjectionBudget,
    pub resource_limits: EditorResourceLimits,
    pub layout_metrics: EditorLayoutMetrics,
}

/// Invalid resource or layout settings detected before the adapter starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorStartupError {
    reason: &'static str,
}

impl fmt::Display for EditorStartupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason)
    }
}

impl Error for EditorStartupError {}

/// Pixel-local state owned by one mounted host, never by the shared editor core.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MountedViewPresentation {
    pub pixel_scroll_y: f32,
    pub focused: bool,
    pub viewport: EditorViewport,
}

impl MountedViewPresentation {
    pub fn new(viewport: EditorViewport) -> Self {
        Self {
            pixel_scroll_y: 0.0,
            focused: false,
            viewport,
        }
    }

    fn validate(self) -> Result<(), EditorError> {
        if !self.pixel_scroll_y.is_finite() || self.pixel_scroll_y < 0.0 {
            return Err(invalid("pixel scroll must be nonnegative and finite"));
        }
        EditorViewport::new(self.viewport.width, self.viewport.height).map_err(invalid)?;
        Ok(())
    }
}

/// Read-only ParchMint state for a mounted pane after its latest frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MountedViewSnapshot {
    pub window: WindowCapability,
    pub view: ViewId,
    pub presentation: MountedViewPresentation,
    pub rendered_revision: EditorRevision,
    pub visible_layouts: usize,
}

/// One block layout performed for one mounted view during a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockRelayout {
    pub view: ViewId,
    pub block: BlockId,
}

/// Deterministic work applied at one adapter frame boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorFrameReport {
    revision: EditorRevision,
    relayouts: Vec<BlockRelayout>,
}

impl EditorFrameReport {
    pub const fn revision(&self) -> EditorRevision {
        self.revision
    }

    pub fn relayouts(&self) -> &[BlockRelayout] {
        &self.relayouts
    }
}

#[derive(Clone)]
struct HostRecord {
    window: WindowCapability,
    view: ViewId,
    mounted: bool,
}

struct CachedLayout {
    input: VisibleEditorBlock,
    geometry: BlockLayoutGeometry,
}

struct MountedView {
    host: ViewHostCapability,
    window: WindowCapability,
    presentation: MountedViewPresentation,
    rendered_revision: EditorRevision,
    layouts: BTreeMap<BlockId, CachedLayout>,
    search: Vec<SearchDecoration>,
    spellcheck: Vec<SpellcheckDecoration>,
}

struct SessionRuntime {
    core: EditorCoreSession,
    views: BTreeMap<ViewId, MountedView>,
    pending_blocks: BTreeSet<BlockId>,
    projections: BTreeMap<EditorRevision, CanonicalProjection>,
    subscribers: Vec<mpsc::Sender<EditorEvent>>,
    closed: bool,
}

impl SessionRuntime {
    fn publish(&mut self, event: EditorEvent) {
        self.subscribers
            .retain(|sender| sender.send(event.clone()).is_ok());
    }

    fn require_open(&self) -> Result<(), EditorError> {
        if self.closed {
            Err(EditorError::Closed)
        } else {
            Ok(())
        }
    }
}

struct AdapterRuntime {
    next_session: u64,
    next_host: u64,
    hosts: BTreeMap<ViewHostCapability, HostRecord>,
    sessions: BTreeMap<SharedEditorSession, SessionRuntime>,
}

impl Default for AdapterRuntime {
    fn default() -> Self {
        Self {
            next_session: 1,
            next_host: 1,
            hosts: BTreeMap::new(),
            sessions: BTreeMap::new(),
        }
    }
}

/// The native Iced editor adapter. Its public surface contains no Iced or engine-native types.
#[derive(Clone)]
pub struct EditorIcedAdapter {
    config: EditorIcedConfig,
    runtime: Arc<Mutex<AdapterRuntime>>,
}

impl EditorIcedAdapter {
    pub fn new(config: EditorIcedConfig) -> Result<Self, EditorStartupError> {
        if config.projection_budget.retained_revisions == 0 {
            return Err(startup("at least one projection revision must be retained"));
        }
        if config.resource_limits.max_sessions == 0 {
            return Err(startup("at least one editor session must be allowed"));
        }
        if config.resource_limits.max_views_per_session < 2 {
            return Err(startup("the editor must allow two mounted views"));
        }
        if config.resource_limits.max_visible_blocks_per_view == 0 {
            return Err(startup("the visible-block cache must not be empty"));
        }
        if config.resource_limits.max_clipboard_scalars == 0 {
            return Err(startup("the clipboard scalar limit must not be empty"));
        }
        config.layout_metrics.validate().map_err(startup)?;
        Ok(Self {
            config,
            runtime: Arc::new(Mutex::new(AdapterRuntime::default())),
        })
    }

    pub fn create_view_host(
        &self,
        window: WindowCapability,
        view: ViewId,
    ) -> Result<ViewHostCapability, EditorError> {
        let mut runtime = self.lock()?;
        let token = runtime.next_host;
        runtime.next_host = runtime
            .next_host
            .checked_add(1)
            .ok_or_else(|| invalid("view-host capability space exhausted"))?;
        let capability = ViewHostCapability::new(token);
        runtime.hosts.insert(
            capability,
            HostRecord {
                window,
                view,
                mounted: false,
            },
        );
        Ok(capability)
    }

    /// Opens a canonical core session with fallible validation for direct callers.
    pub fn open_session(
        &self,
        load: CanonicalDocumentLoad,
    ) -> Result<SharedEditorSession, EditorError> {
        let core = EditorCoreSession::open(load)?;
        let initial = core.canonical_projection();
        let mut runtime = self.lock()?;
        let live_sessions = runtime
            .sessions
            .values()
            .filter(|session| !session.closed)
            .count();
        if live_sessions >= self.config.resource_limits.max_sessions {
            return Err(invalid("editor session resource limit reached"));
        }
        let token = runtime.next_session;
        runtime.next_session = runtime
            .next_session
            .checked_add(1)
            .ok_or_else(|| invalid("editor session capability space exhausted"))?;
        let capability = SharedEditorSession::new(token);
        runtime.sessions.insert(
            capability.clone(),
            SessionRuntime {
                core,
                views: BTreeMap::new(),
                pending_blocks: BTreeSet::new(),
                projections: BTreeMap::from([(initial.revision(), initial)]),
                subscribers: Vec::new(),
                closed: false,
            },
        );
        Ok(capability)
    }

    pub fn set_view_presentation(
        &self,
        session: SharedEditorSession,
        view: ViewId,
        presentation: MountedViewPresentation,
    ) -> Result<(), EditorError> {
        presentation.validate()?;
        self.with_session(session, |state| {
            state.require_open()?;
            if !state.views.contains_key(&view) {
                return Err(EditorError::UnknownView { view });
            }
            if presentation.focused {
                for mounted in state.views.values_mut() {
                    mounted.presentation.focused = false;
                }
            }
            let mounted = state
                .views
                .get_mut(&view)
                .ok_or(EditorError::UnknownView { view })?;
            mounted.presentation = presentation;
            relayout_all(mounted, self.config.layout_metrics)?;
            Ok(())
        })
    }

    pub fn view_snapshot(
        &self,
        session: SharedEditorSession,
        view: ViewId,
    ) -> Result<MountedViewSnapshot, EditorError> {
        self.with_session(session, |state| {
            state.require_open()?;
            let mounted = state
                .views
                .get(&view)
                .ok_or(EditorError::UnknownView { view })?;
            Ok(MountedViewSnapshot {
                window: mounted.window,
                view,
                presentation: mounted.presentation,
                rendered_revision: mounted.rendered_revision,
                visible_layouts: mounted.layouts.len(),
            })
        })
    }

    /// Replaces the visible-plus-overscan cache and immediately lays out newly visible blocks.
    pub fn cache_visible_blocks(
        &self,
        session: SharedEditorSession,
        view: ViewId,
        blocks: impl IntoIterator<Item = VisibleEditorBlock>,
    ) -> Result<Vec<BlockId>, EditorError> {
        let mut blocks = blocks.into_iter().collect::<Vec<_>>();
        let limit = self.config.resource_limits.max_visible_blocks_per_view;
        if blocks.len() > limit {
            blocks.drain(..blocks.len() - limit);
        }
        self.with_session(session, |state| {
            state.require_open()?;
            let mounted = state
                .views
                .get_mut(&view)
                .ok_or(EditorError::UnknownView { view })?;
            let retained = blocks
                .iter()
                .map(VisibleEditorBlock::block)
                .collect::<BTreeSet<_>>();
            mounted.layouts.retain(|block, _| retained.contains(block));
            let mut laid_out = Vec::new();
            for input in blocks {
                let block = input.block();
                let unchanged = mounted
                    .layouts
                    .get(&block)
                    .is_some_and(|cached| cached.input == input);
                if unchanged {
                    continue;
                }
                let geometry =
                    layout_block(&input, mounted.presentation, self.config.layout_metrics)?;
                mounted
                    .layouts
                    .insert(block, CachedLayout { input, geometry });
                laid_out.push(block);
            }
            Ok(laid_out)
        })
    }

    /// Applies pending shared edits to every mounted view at the next frame boundary.
    pub fn next_frame(
        &self,
        session: SharedEditorSession,
    ) -> Result<EditorFrameReport, EditorError> {
        self.with_session(session, |state| {
            state.require_open()?;
            let revision = state.core.revision();
            let primary = state.core.primary_block();
            let projection = state.core.canonical_projection();
            let pending = std::mem::take(&mut state.pending_blocks);
            let mut relayouts = Vec::new();
            for (view, mounted) in &mut state.views {
                for block in &pending {
                    let Some(cached) = mounted.layouts.get_mut(block) else {
                        continue;
                    };
                    if *block == primary {
                        cached.input = VisibleEditorBlock::from_semantic(
                            primary,
                            projection.semantic(),
                            cached.input.document_start(),
                        );
                    }
                    cached.geometry = layout_block(
                        &cached.input,
                        mounted.presentation,
                        self.config.layout_metrics,
                    )?;
                    relayouts.push(BlockRelayout {
                        view: *view,
                        block: *block,
                    });
                }
                mounted.rendered_revision = revision;
            }
            Ok(EditorFrameReport {
                revision,
                relayouts,
            })
        })
    }

    pub fn geometry(
        &self,
        session: SharedEditorSession,
        view: ViewId,
        block: BlockId,
    ) -> Result<BlockLayoutGeometry, EditorError> {
        self.with_session(session, |state| {
            state.require_open()?;
            state
                .views
                .get(&view)
                .ok_or(EditorError::UnknownView { view })?
                .layouts
                .get(&block)
                .map(|cached| cached.geometry.clone())
                .ok_or_else(|| invalid("block is outside the visible layout cache"))
        })
    }

    /// Returns the current spelling ranges for one mounted view. The renderer
    /// uses this short-lived snapshot to draw decorations without exposing the
    /// adapter's retained view state.
    pub fn spellcheck_decorations(
        &self,
        session: SharedEditorSession,
        view: ViewId,
    ) -> Result<Vec<SpellcheckDecoration>, EditorError> {
        self.with_session(session, |state| {
            state.require_open()?;
            Ok(state
                .views
                .get(&view)
                .ok_or(EditorError::UnknownView { view })?
                .spellcheck
                .clone())
        })
    }

    pub fn input_en_us(
        &self,
        session: SharedEditorSession,
        view: ViewId,
        text: &str,
    ) -> Result<(), EditorError> {
        if text.is_empty() {
            return Err(invalid("text input must not be empty"));
        }
        if !text
            .chars()
            .all(|character| character.is_ascii_graphic() || matches!(character, ' ' | '\n' | '\t'))
        {
            return Err(invalid(
                "v1 editor input accepts en-US text, newline, and tab",
            ));
        }
        self.replace_selection(session, view, text)
    }

    /// Sanitizes caller-supplied untrusted clipboard data; it does not access an OS clipboard.
    pub fn paste_untrusted(
        &self,
        session: SharedEditorSession,
        view: ViewId,
        source: &UntrustedClipboardContent,
    ) -> Result<SanitizedPaste, EditorError> {
        let paste = self.sanitize_clipboard(source)?;
        if !paste.text().is_empty() {
            self.replace_selection(session, view, paste.text())?;
        }
        Ok(paste)
    }

    /// Applies untrusted clipboard data only if the session is still at the
    /// revision and selection captured before the asynchronous platform read.
    pub fn paste_untrusted_at(
        &self,
        session: SharedEditorSession,
        view: ViewId,
        selection: EditorSelection,
        expected_revision: EditorRevision,
        source: &UntrustedClipboardContent,
    ) -> Result<SanitizedPaste, EditorError> {
        let paste = self.sanitize_clipboard(source)?;
        if !paste.text().is_empty() {
            self.replace_selection_at(session, view, selection, expected_revision, paste.text())?;
        }
        Ok(paste)
    }

    pub fn revision(&self, session: SharedEditorSession) -> Result<EditorRevision, EditorError> {
        self.with_session(session, |state| {
            state.require_open()?;
            Ok(state.core.revision())
        })
    }

    /// Returns the complete initial cache entry for the session's primary block.
    ///
    /// Production hosts use this immediately after attaching a view, before a
    /// viewport is rendered. The editor core continues to own the primary-block
    /// identity and canonical body.
    pub fn primary_visible_block(
        &self,
        session: SharedEditorSession,
    ) -> Result<VisibleEditorBlock, EditorError> {
        self.with_session(session, |state| {
            state.require_open()?;
            let projection = state.core.canonical_projection();
            Ok(VisibleEditorBlock::from_semantic(
                state.core.primary_block(),
                projection.semantic(),
                DocumentPosition::default(),
            ))
        })
    }

    fn replace_selection(
        &self,
        session: SharedEditorSession,
        view: ViewId,
        text: &str,
    ) -> Result<(), EditorError> {
        let selection = self.selection(session.clone(), view)?;
        let revision = self.revision(session.clone())?;
        self.replace_selection_at(session, view, selection, revision, text)
    }

    fn replace_selection_at(
        &self,
        session: SharedEditorSession,
        view: ViewId,
        selection: EditorSelection,
        revision: EditorRevision,
        text: &str,
    ) -> Result<(), EditorError> {
        let start = selection.start();
        let kind = if selection.is_collapsed() {
            EditorCommandKind::InsertText {
                at: start,
                text: text.to_owned(),
            }
        } else {
            EditorCommandKind::ReplaceRange {
                range: selection,
                text: text.to_owned(),
            }
        };
        self.execute(
            session.clone(),
            EditorCommandOrigin::new(view),
            EditorCommand::new(revision, kind),
        )?;
        let inserted = u64::try_from(text.chars().count())
            .map_err(|_| invalid("inserted text is too large"))?;
        let caret = start
            .value()
            .checked_add(inserted)
            .ok_or_else(|| invalid("inserted text position overflowed"))?;
        let revision = self.revision(session.clone())?;
        self.execute(
            session,
            EditorCommandOrigin::new(view),
            EditorCommand::new(
                revision,
                EditorCommandKind::SetSelection {
                    selection: EditorSelection::new(
                        DocumentPosition::from(caret),
                        DocumentPosition::from(caret),
                    ),
                },
            ),
        )
    }

    fn sanitize_clipboard(
        &self,
        source: &UntrustedClipboardContent,
    ) -> Result<SanitizedPaste, EditorError> {
        let paste = if let Some(html) = source.html() {
            sanitize_paste(PasteSource::RichHtml(html))
        } else {
            sanitize_paste(PasteSource::PlainText(
                source.plain_text().unwrap_or_default(),
            ))
        };
        if paste.text().chars().count() > self.config.resource_limits.max_clipboard_scalars {
            return Err(invalid(
                "sanitized clipboard text exceeds the editor resource limit",
            ));
        }
        Ok(paste)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, AdapterRuntime>, EditorError> {
        self.runtime
            .lock()
            .map_err(|_| invalid("editor adapter state is poisoned"))
    }

    fn with_session<T>(
        &self,
        session: SharedEditorSession,
        operation: impl FnOnce(&mut SessionRuntime) -> Result<T, EditorError>,
    ) -> Result<T, EditorError> {
        let mut runtime = self.lock()?;
        let state = runtime
            .sessions
            .get_mut(&session)
            .ok_or(EditorError::UnknownSession)?;
        operation(state)
    }

    fn record_change(&self, state: &mut SessionRuntime, applied: &AppliedEditorChange) {
        if !applied.document_changed() {
            return;
        }
        state
            .pending_blocks
            .extend(applied.changed_blocks().iter().copied());
        let projection = state.core.canonical_projection();
        state.projections.insert(projection.revision(), projection);
        while state.projections.len() > self.config.projection_budget.retained_revisions {
            let Some(oldest) = state.projections.keys().next().copied() else {
                break;
            };
            state.projections.remove(&oldest);
        }
        state.publish(EditorEvent::DocumentChanged {
            revision: applied.revision(),
        });
    }
}

impl EditorAdapter for EditorIcedAdapter {
    fn open(&self, load: CanonicalDocumentLoad) -> AsyncResult<SharedEditorSession> {
        let session = self
            .open_session(load)
            .expect("EditorAdapter::open requires a valid canonical load within configured limits");
        Box::pin(async move { session })
    }

    fn attach_view(
        &self,
        session: SharedEditorSession,
        view: ViewId,
        host: ViewHostCapability,
    ) -> Result<(), EditorError> {
        let mut runtime = self.lock()?;
        let host_record = runtime
            .hosts
            .get(&host)
            .cloned()
            .ok_or_else(|| invalid("unknown editor view-host capability"))?;
        if host_record.view != view {
            return Err(invalid("view-host capability belongs to a different view"));
        }
        if host_record.mounted {
            return Err(invalid("view-host capability is already mounted"));
        }
        {
            let state = runtime
                .sessions
                .get_mut(&session)
                .ok_or(EditorError::UnknownSession)?;
            state.require_open()?;
            if state.views.len() >= self.config.resource_limits.max_views_per_session {
                return Err(invalid("mounted view resource limit reached"));
            }
            state.core.attach_view(view)?;
            state.views.insert(
                view,
                MountedView {
                    host,
                    window: host_record.window,
                    presentation: MountedViewPresentation::new(
                        EditorViewport::new(640.0, 480.0).expect("default viewport is valid"),
                    ),
                    rendered_revision: state.core.revision(),
                    layouts: BTreeMap::new(),
                    search: Vec::new(),
                    spellcheck: Vec::new(),
                },
            );
            state.publish(EditorEvent::ViewAttached { view });
        }
        runtime
            .hosts
            .get_mut(&host)
            .expect("host was resolved above")
            .mounted = true;
        Ok(())
    }

    fn detach_view(
        &self,
        session: SharedEditorSession,
        view: ViewId,
    ) -> Result<EditorViewState, EditorError> {
        let mut runtime = self.lock()?;
        let (host, detached) = {
            let state = runtime
                .sessions
                .get_mut(&session)
                .ok_or(EditorError::UnknownSession)?;
            state.require_open()?;
            let mounted = state
                .views
                .remove(&view)
                .ok_or(EditorError::UnknownView { view })?;
            let detached = state.core.detach_view(view)?;
            state.publish(EditorEvent::ViewDetached { view });
            (mounted.host, detached)
        };
        if let Some(host) = runtime.hosts.get_mut(&host) {
            host.mounted = false;
        }
        Ok(detached)
    }

    fn execute(
        &self,
        session: SharedEditorSession,
        origin: EditorCommandOrigin,
        command: EditorCommand,
    ) -> Result<(), EditorError> {
        self.with_session(session, |state| {
            state.require_open()?;
            let applied = state.core.execute(origin, command)?;
            self.record_change(state, &applied);
            Ok(())
        })
    }

    fn selection(
        &self,
        session: SharedEditorSession,
        view: ViewId,
    ) -> Result<EditorSelection, EditorError> {
        self.with_session(session, |state| {
            state.require_open()?;
            state.core.selection(view)
        })
    }

    fn selection_clipboard(
        &self,
        session: SharedEditorSession,
        view: ViewId,
    ) -> Result<Option<EditorClipboardContent>, EditorError> {
        self.with_session(session, |state| {
            state.require_open()?;
            state.core.selection_clipboard(view)
        })
    }

    fn selection_geometry(
        &self,
        session: SharedEditorSession,
        view: ViewId,
    ) -> Result<Option<SelectionGeometry>, EditorError> {
        self.with_session(session, |state| {
            state.require_open()?;
            let selection = state.core.selection(view)?;
            if selection.is_collapsed() {
                return Ok(None);
            }
            let mounted = state
                .views
                .get(&view)
                .ok_or(EditorError::UnknownView { view })?;
            let rectangles = mounted
                .layouts
                .values()
                .flat_map(|cached| cached.geometry.selection_rectangles(selection))
                .map(Into::into)
                .collect::<Vec<_>>();
            Ok((!rectangles.is_empty()).then(|| SelectionGeometry::new(selection, rectangles)))
        })
    }

    fn set_style_catalog(
        &self,
        session: SharedEditorSession,
        styles: StyleCatalogProjection,
    ) -> Result<(), EditorError> {
        self.with_session(session, |state| {
            state.require_open()?;
            state.core.set_style_catalog(styles);
            Ok(())
        })
    }

    fn set_search_decorations(
        &self,
        session: SharedEditorSession,
        view: ViewId,
        decorations: Vec<SearchDecoration>,
    ) -> Result<(), EditorError> {
        self.with_session(session, |state| {
            state.require_open()?;
            state
                .views
                .get_mut(&view)
                .ok_or(EditorError::UnknownView { view })?
                .search = decorations;
            Ok(())
        })
    }

    fn set_spellcheck_decorations(
        &self,
        session: SharedEditorSession,
        view: ViewId,
        decorations: Vec<SpellcheckDecoration>,
    ) -> Result<(), EditorError> {
        self.with_session(session, |state| {
            state.require_open()?;
            state
                .views
                .get_mut(&view)
                .ok_or(EditorError::UnknownView { view })?
                .spellcheck = decorations;
            Ok(())
        })
    }

    fn apply_composite_project_edit(
        &self,
        session: SharedEditorSession,
        _operation: ProjectDocumentOperation,
    ) -> Result<(), EditorError> {
        self.with_session(session, |state| {
            state.require_open()?;
            Err(invalid(
                "composite project edits are not available in the Iced adapter",
            ))
        })
    }

    fn project(
        &self,
        session: SharedEditorSession,
        through: EditorRevision,
    ) -> AsyncResult<Result<CanonicalProjection, EditorError>> {
        let projection = self.with_session(session, |state| {
            state.require_open()?;
            state.projections.get(&through).cloned().ok_or_else(|| {
                invalid("requested projection revision is outside the retained budget")
            })
        });
        Box::pin(async move { projection })
    }

    fn events(&self, session: SharedEditorSession) -> EventStream<EditorEvent> {
        let (sender, receiver) = mpsc::channel();
        if let Ok(mut runtime) = self.runtime.lock()
            && let Some(state) = runtime.sessions.get_mut(&session)
        {
            if state.closed {
                let _ = sender.send(EditorEvent::Closed);
            } else {
                state.subscribers.push(sender);
            }
        }
        EventStream::from_receiver(receiver)
    }

    fn close(&self, session: SharedEditorSession) -> AsyncResult<()> {
        if let Ok(mut runtime) = self.runtime.lock() {
            let hosts = if let Some(state) = runtime.sessions.get_mut(&session) {
                if state.closed {
                    Vec::new()
                } else {
                    state.closed = true;
                    let hosts = state
                        .views
                        .values()
                        .map(|view| view.host)
                        .collect::<Vec<_>>();
                    state.views.clear();
                    state.publish(EditorEvent::Closed);
                    state.subscribers.clear();
                    hosts
                }
            } else {
                Vec::new()
            };
            for host in hosts {
                if let Some(record) = runtime.hosts.get_mut(&host) {
                    record.mounted = false;
                }
            }
        }
        Box::pin(async {})
    }

    fn capabilities(&self) -> EditorCapabilities {
        EditorCapabilities::default()
    }
}

fn layout_block(
    input: &VisibleEditorBlock,
    presentation: MountedViewPresentation,
    metrics: EditorLayoutMetrics,
) -> Result<BlockLayoutGeometry, EditorError> {
    BlockLayoutGeometry::build(
        input,
        presentation.viewport,
        presentation.pixel_scroll_y,
        metrics,
    )
    .map_err(invalid)
}

fn relayout_all(
    mounted: &mut MountedView,
    metrics: EditorLayoutMetrics,
) -> Result<(), EditorError> {
    for cached in mounted.layouts.values_mut() {
        cached.geometry = layout_block(&cached.input, mounted.presentation, metrics)?;
    }
    Ok(())
}

const fn startup(reason: &'static str) -> EditorStartupError {
    EditorStartupError { reason }
}

const fn invalid(reason: &'static str) -> EditorError {
    EditorError::InvalidCommand { reason }
}
