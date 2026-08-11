use iced::Element;
use parchmint_editor_api::{
    CanonicalDocumentLoad, EditorAdapter, EditorError, EditorViewState, SharedEditorSession, ViewId,
};
use parchmint_platform_api::WindowCapability;

use crate::{
    EditorFrameReport, EditorIcedAdapter, EditorSurfaceTheme, EditorViewport, MountedEditorConfig,
    MountedEditorHost, MountedEditorMessage, MountedEditorUpdate,
};

/// Whether a production manuscript view opens a new session or joins an
/// existing shared session.
#[derive(Debug, Clone, PartialEq)]
pub enum MountedEditorSession {
    Open(CanonicalDocumentLoad),
    Reuse(SharedEditorSession),
}

impl MountedEditorSession {
    fn resolve(self, adapter: &EditorIcedAdapter) -> Result<SharedEditorSession, EditorError> {
        match self {
            Self::Open(load) => adapter.open_session(load),
            Self::Reuse(session) => Ok(session),
        }
    }
}

/// ParchMint-owned settings for attaching one visible manuscript view.
#[derive(Debug, Clone, PartialEq)]
pub struct MountedEditorBindingConfig {
    session: MountedEditorSession,
    window: WindowCapability,
    view: ViewId,
    viewport: EditorViewport,
    theme: EditorSurfaceTheme,
}

impl MountedEditorBindingConfig {
    pub const fn new(
        session: MountedEditorSession,
        window: WindowCapability,
        view: ViewId,
        viewport: EditorViewport,
        theme: EditorSurfaceTheme,
    ) -> Self {
        Self {
            session,
            window,
            view,
            viewport,
            theme,
        }
    }

    pub fn session(&self) -> &MountedEditorSession {
        &self.session
    }

    pub const fn window(&self) -> WindowCapability {
        self.window
    }

    pub const fn view(&self) -> ViewId {
        self.view
    }

    pub const fn viewport(&self) -> EditorViewport {
        self.viewport
    }

    pub const fn theme(&self) -> EditorSurfaceTheme {
        self.theme
    }
}

/// Owns one live adapter attachment and its real Iced manuscript host.
///
/// This is the production lifecycle boundary: it opens or reuses a shared
/// session, attaches a view, initializes that view's viewport and primary-block
/// cache, and then creates the public mounted host. It can later rebind to a
/// different session/view or detach without exposing adapter host capabilities.
#[derive(Clone)]
pub struct MountedEditorBinding {
    adapter: EditorIcedAdapter,
    session: SharedEditorSession,
    view: ViewId,
    host: MountedEditorHost,
}

impl MountedEditorBinding {
    /// Performs one safe production mount.
    ///
    /// If initialization after attachment fails, the just-attached view is
    /// detached before the error is returned.
    pub fn mount(
        adapter: &EditorIcedAdapter,
        config: MountedEditorBindingConfig,
    ) -> Result<Self, EditorError> {
        let session = config.session.resolve(adapter)?;
        let view = config.view;
        let host_capability = adapter.create_view_host(config.window, view)?;
        adapter.attach_view(session.clone(), view, host_capability)?;

        let initialized = (|| {
            adapter.set_view_presentation(
                session.clone(),
                view,
                crate::MountedViewPresentation::new(config.viewport),
            )?;
            let primary = adapter.primary_visible_block(session.clone())?;
            let block = primary.block();
            adapter.cache_visible_blocks(session.clone(), view, [primary])?;
            let host = MountedEditorHost::mount(
                adapter,
                MountedEditorConfig::new(session.clone(), view, block, config.theme),
            )?;
            Ok(host)
        })();

        match initialized {
            Ok(host) => Ok(Self {
                adapter: adapter.clone(),
                session,
                view,
                host,
            }),
            Err(error) => {
                let _ = adapter.detach_view(session, view);
                Err(error)
            }
        }
    }

    pub fn session(&self) -> SharedEditorSession {
        self.session.clone()
    }

    pub const fn view(&self) -> ViewId {
        self.view
    }

    /// Exposes the narrow renderer/message host needed by the Iced UI crate.
    pub fn host(&self) -> &MountedEditorHost {
        &self.host
    }

    /// Builds the Iced implementation boundary for the outer production UI.
    pub fn element(&self) -> Element<'static, MountedEditorMessage> {
        self.host.element()
    }

    /// Routes one interaction, advances the shared-session frame, and refreshes
    /// this retained view. Other bindings for the same session can call
    /// [`Self::refresh`] to observe that shared frame.
    pub fn update(
        &self,
        message: MountedEditorMessage,
    ) -> Result<MountedEditorUpdate, EditorError> {
        let update = self.host.update(message)?;
        self.refresh()?;
        Ok(update)
    }

    /// Advances every mounted view's shared cache, then refreshes this host.
    pub fn refresh(&self) -> Result<EditorFrameReport, EditorError> {
        let frame = self.adapter.next_frame(self.session.clone())?;
        self.host.refresh()?;
        Ok(frame)
    }

    /// Restores editor focus after a native task without changing selection.
    pub fn restore_focus(&self) -> Result<(), EditorError> {
        self.host.restore_focus()
    }

    /// Rebinds semantic colors without remounting the editor session or view.
    pub fn set_theme(&mut self, theme: crate::EditorSurfaceTheme) {
        self.host.set_theme(theme);
    }

    /// Detaches this view and returns its adapter-owned presentation state.
    pub fn detach(self) -> Result<EditorViewState, EditorError> {
        self.adapter.detach_view(self.session, self.view)
    }

    /// Detaches this view before mounting a replacement binding.
    ///
    /// Rebinding intentionally has no rollback: after a successful detach the
    /// previous view is no longer live, even if replacement attachment fails.
    pub fn rebind(self, config: MountedEditorBindingConfig) -> Result<Self, EditorError> {
        let adapter = self.adapter.clone();
        self.detach()?;
        Self::mount(&adapter, config)
    }
}
