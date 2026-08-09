//! Deterministic native/Iced boundary fixture used by integration tests.
//!
//! The fixture substitutes operating-system calls but uses the production
//! capability registry, worker dispatch, service implementations, and final
//! callback authorization.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, SyncSender, sync_channel},
    },
    time::Duration,
};

use parchmint_platform_api::{
    ApplicationPaths, ClipboardContent, ClipboardFormats, ClipboardService, DialogService,
    ExternalOpenService, MenuService, PathDialog, PlatformError, SemanticMenu, SystemAppearance,
    SystemAppearanceService, UntrustedClipboardContent, UntrustedPathSelection,
    ValidatedExternalIntent, WindowCapability,
};

use crate::{
    NativePlatform, NativeServices,
    iced_adapter::IcedWindowRegistry,
    runtime::{MenuSnapshot, NativeBackend},
};

pub struct PausedMenuInstall {
    started: Receiver<()>,
    resume: Option<SyncSender<()>>,
}

impl PausedMenuInstall {
    pub fn wait_until_started(&self) {
        self.started
            .recv_timeout(Duration::from_secs(2))
            .expect("menu worker did not start");
    }

    pub fn release(mut self) {
        self.resume
            .take()
            .expect("menu worker was already released")
            .send(())
            .expect("menu worker stopped before release");
    }
}

pub struct NativeFixture {
    _platform: NativePlatform,
    backend: Arc<FixtureBackend>,
    services: Arc<NativeServices>,
    registry: IcedWindowRegistry,
}

impl NativeFixture {
    pub fn new() -> Self {
        let backend = Arc::new(FixtureBackend::default());
        let (platform, services) = NativePlatform::with_backend(backend.clone());
        let registry = platform.iced_window_registry();
        Self {
            _platform: platform,
            backend,
            services,
            registry,
        }
    }

    pub fn registry(&self) -> IcedWindowRegistry {
        self.registry.clone()
    }

    pub fn register_window(&self, capability: WindowCapability) -> WindowCapability {
        self.registry.register_window(capability)
    }

    pub fn close_window(&self, capability: WindowCapability) {
        self.registry.close_window(capability);
    }

    pub fn menus(&self) -> Arc<dyn MenuService> {
        self.services.clone()
    }

    pub fn dialogs(&self) -> Arc<dyn DialogService> {
        self.services.clone()
    }

    pub fn clipboard(&self) -> Arc<dyn ClipboardService> {
        self.services.clone()
    }

    pub fn external_open(&self) -> Arc<dyn ExternalOpenService> {
        self.services.clone()
    }

    pub fn appearance(&self) -> Arc<dyn SystemAppearanceService> {
        self.services.clone()
    }

    pub fn application_paths(&self) -> Arc<dyn parchmint_platform_api::ApplicationPathService> {
        self.services.clone()
    }

    pub fn menu_snapshot(
        &self,
        window: WindowCapability,
    ) -> Result<NativeMenuSnapshot, PlatformError> {
        self.services.registry.authorize(window)?;
        self.backend
            .menus
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(&window)
            .cloned()
            .map(NativeMenuSnapshot)
            .ok_or_else(|| PlatformError::Failed {
                operation: "menu snapshot",
                reason: "no semantic menu is installed".to_owned(),
            })
    }

    pub fn seed_external_clipboard(&self, content: UntrustedClipboardContent) {
        *self
            .backend
            .clipboard
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = content;
    }

    pub fn set_system_appearance(&self, appearance: SystemAppearance) {
        *self
            .backend
            .appearance
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = appearance;
    }

    pub fn registered_window(&self, window: WindowCapability) -> Option<WindowCapability> {
        self.services.registry.registered(window)
    }

    pub fn pause_next_menu_install(&self) -> PausedMenuInstall {
        let (started_sender, started) = sync_channel(0);
        let (resume, resume_receiver) = sync_channel(0);
        let previous = self
            .backend
            .next_menu_pause
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .replace(BackendMenuPause {
                started: started_sender,
                resume: resume_receiver,
            });
        assert!(previous.is_none(), "a menu worker is already paused");
        PausedMenuInstall {
            started,
            resume: Some(resume),
        }
    }

    pub fn opened_external_urls(&self) -> Vec<String> {
        self.backend
            .opened_external_urls
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }
}

impl Default for NativeFixture {
    fn default() -> Self {
        Self::new()
    }
}

pub struct NativeMenuSnapshot(MenuSnapshot);

impl NativeMenuSnapshot {
    pub fn commands(&self) -> &[String] {
        self.0.commands()
    }

    pub const fn contains_separator(&self) -> bool {
        self.0.contains_separator()
    }

    pub fn accelerator(&self, command: &str) -> Option<&'static str> {
        self.0.accelerator(command)
    }
}

struct FixtureBackend {
    menus: Mutex<HashMap<WindowCapability, MenuSnapshot>>,
    clipboard: Mutex<UntrustedClipboardContent>,
    appearance: Mutex<SystemAppearance>,
    opened_external_urls: Mutex<Vec<String>>,
    next_menu_pause: Mutex<Option<BackendMenuPause>>,
}

struct BackendMenuPause {
    started: SyncSender<()>,
    resume: Receiver<()>,
}

impl Default for FixtureBackend {
    fn default() -> Self {
        Self {
            menus: Mutex::new(HashMap::new()),
            clipboard: Mutex::new(UntrustedClipboardContent::empty()),
            appearance: Mutex::new(SystemAppearance::Light),
            opened_external_urls: Mutex::new(Vec::new()),
            next_menu_pause: Mutex::new(None),
        }
    }
}

impl NativeBackend for FixtureBackend {
    fn install_menu(
        &self,
        window: WindowCapability,
        menu: SemanticMenu,
    ) -> Result<(), PlatformError> {
        let pause = self
            .next_menu_pause
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        if let Some(pause) = pause {
            let _ = pause.started.send(());
            let _ = pause.resume.recv();
        }

        let snapshot = MenuSnapshot::from_menu(&menu);
        self.menus
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(window, snapshot);
        Ok(())
    }

    fn choose_path(
        &self,
        _window: WindowCapability,
        _request: PathDialog,
    ) -> Result<Option<UntrustedPathSelection>, PlatformError> {
        #[cfg(target_os = "windows")]
        let path = PathBuf::from(r"C:\outside\project.parchment");
        #[cfg(not(target_os = "windows"))]
        let path = PathBuf::from("/outside/project.parchment");
        Ok(Some(UntrustedPathSelection::new(path)))
    }

    fn read_clipboard(
        &self,
        _window: WindowCapability,
        formats: ClipboardFormats,
    ) -> Result<UntrustedClipboardContent, PlatformError> {
        let stored = self
            .clipboard
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let mut selected = UntrustedClipboardContent::empty();
        if formats.accepts_plain_text()
            && let Some(text) = stored.plain_text()
        {
            selected = selected.with_plain_text(text);
        }
        if formats.accepts_html()
            && let Some(html) = stored.html()
        {
            selected = selected.with_html(html);
        }
        Ok(selected)
    }

    fn write_clipboard(
        &self,
        _window: WindowCapability,
        content: ClipboardContent,
    ) -> Result<(), PlatformError> {
        *self
            .clipboard
            .lock()
            .unwrap_or_else(|error| error.into_inner()) =
            UntrustedClipboardContent::empty().with_plain_text(content.as_plain_text());
        Ok(())
    }

    fn open_external(
        &self,
        _window: WindowCapability,
        intent: ValidatedExternalIntent,
    ) -> Result<(), PlatformError> {
        self.opened_external_urls
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(intent.as_url().to_owned());
        Ok(())
    }

    fn application_paths(&self) -> Result<ApplicationPaths, PlatformError> {
        #[cfg(target_os = "windows")]
        let paths = ApplicationPaths::new(
            r"C:\Users\tester\AppData\Roaming\ParchMint",
            r"C:\Users\tester\AppData\Local\ParchMint\Data",
            r"C:\Users\tester\AppData\Local\ParchMint\Cache",
        );
        #[cfg(target_os = "macos")]
        let paths = ApplicationPaths::new(
            "/Users/tester/Library/Application Support/ParchMint",
            "/Users/tester/Library/Application Support/ParchMint",
            "/Users/tester/Library/Caches/ParchMint",
        );
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let paths = ApplicationPaths::new(
            "/home/tester/.config/parchmint",
            "/home/tester/.local/share/parchmint",
            "/home/tester/.cache/parchmint",
        );
        Ok(paths)
    }

    fn appearance(&self) -> Result<SystemAppearance, PlatformError> {
        Ok(*self
            .appearance
            .lock()
            .unwrap_or_else(|error| error.into_inner()))
    }
}
