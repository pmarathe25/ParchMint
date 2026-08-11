//! Native Windows, macOS, and Linux services for ParchMint's Iced desktop.
//!
//! This crate does not create windows. The Iced adapter privately registers a
//! ParchMint [`WindowCapability`](parchmint_platform_api::WindowCapability),
//! and every window-scoped operation authorizes that exact generation before
//! work starts and again immediately before its completion is delivered.

mod async_task;
mod registry;
mod runtime;
#[doc(hidden)]
pub mod testing;

use std::{
    error::Error,
    fmt,
    sync::{Arc, Mutex, atomic::AtomicU64, mpsc},
};

use async_task::dispatch;
use parchmint_platform_api::{
    ApplicationPathService, ApplicationPaths, AsyncResult, ClipboardContent, ClipboardFormats,
    ClipboardService, DialogService, ExternalOpenService, MenuBinding, MenuService, PathDialog,
    PlatformError, SemanticMenu, SystemAppearance, SystemAppearanceEvent,
    SystemAppearanceEventService, SystemAppearanceEventStream, SystemAppearanceService,
    UntrustedClipboardContent, UntrustedPathSelection, ValidatedExternalIntent, WindowCapability,
    WindowResult,
};
use registry::CapabilityRegistry;
use runtime::{NativeBackend, SystemBackend};

/// The native platform bundle could not be initialized on this target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformStartupError {
    reason: String,
}

impl PlatformStartupError {
    fn unsupported_target() -> Self {
        Self {
            reason: "ParchMint native services support Windows, macOS, and Linux".to_owned(),
        }
    }
}

impl fmt::Display for PlatformStartupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl Error for PlatformStartupError {}

/// Concrete native service implementations used by the desktop executable.
pub struct NativePlatform {
    pub dialogs: Arc<dyn DialogService>,
    pub menus: Arc<dyn MenuService>,
    pub clipboard: Arc<dyn ClipboardService>,
    pub external_open: Arc<dyn ExternalOpenService>,
    pub appearance: Arc<dyn SystemAppearanceService>,
    pub appearance_events: Arc<dyn SystemAppearanceEventService>,
    pub application_paths: Arc<dyn ApplicationPathService>,
    iced_registry: iced_adapter::IcedWindowRegistry,
}

impl NativePlatform {
    /// Creates target-gated native services without creating another event
    /// loop or window system.
    pub fn initialize() -> Result<Self, PlatformStartupError> {
        if !cfg!(any(
            target_os = "windows",
            target_os = "macos",
            target_os = "linux"
        )) {
            return Err(PlatformStartupError::unsupported_target());
        }
        Ok(Self::with_backend(Arc::new(SystemBackend::default())).0)
    }

    fn with_backend(backend: Arc<dyn NativeBackend>) -> (Self, Arc<NativeServices>) {
        let registry = CapabilityRegistry::new();
        let services = Arc::new(NativeServices::new(Arc::clone(&backend), registry.clone()));
        let platform = Self {
            dialogs: services.clone(),
            menus: services.clone(),
            clipboard: services.clone(),
            external_open: services.clone(),
            appearance: services.clone(),
            appearance_events: services.clone(),
            application_paths: services.clone(),
            iced_registry: iced_adapter::IcedWindowRegistry::new(registry),
        };
        (platform, services)
    }

    /// Private concrete integration used only by `parchmint-ui-iced`.
    #[doc(hidden)]
    pub fn iced_window_registry(&self) -> iced_adapter::IcedWindowRegistry {
        self.iced_registry.clone()
    }
}

struct NativeServices {
    backend: Arc<dyn NativeBackend>,
    registry: CapabilityRegistry,
    next_menu_binding: AtomicU64,
    next_appearance_generation: AtomicU64,
    appearance_listeners: Mutex<Vec<mpsc::Sender<SystemAppearanceEvent>>>,
}

impl NativeServices {
    fn new(backend: Arc<dyn NativeBackend>, registry: CapabilityRegistry) -> Self {
        Self {
            backend,
            registry,
            next_menu_binding: AtomicU64::new(1),
            next_appearance_generation: AtomicU64::new(1),
            appearance_listeners: Mutex::new(Vec::new()),
        }
    }

    fn publish_appearance(&self, appearance: SystemAppearance) {
        let event = SystemAppearanceEvent {
            generation: self
                .next_appearance_generation
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            appearance,
        };
        if let Ok(mut listeners) = self.appearance_listeners.lock() {
            listeners.retain(|listener| listener.send(event).is_ok());
        }
    }

    fn spawn_window<T, Work>(
        &self,
        window: WindowCapability,
        work: Work,
    ) -> AsyncResult<WindowResult<T>>
    where
        T: Send + 'static,
        Work: FnOnce(Arc<dyn NativeBackend>) -> Result<T, PlatformError> + Send + 'static,
    {
        if let Err(error) = self.registry.authorize(window) {
            return Box::pin(async move { Err(error) });
        }

        let registry = self.registry.clone();
        let backend = Arc::clone(&self.backend);
        Box::pin(dispatch(move |sender| {
            let result = work(backend).map(|value| WindowResult::new(window, value));
            registry.complete(window, sender, result);
        }))
    }

    fn spawn_global<T, Work>(&self, work: Work) -> AsyncResult<T>
    where
        T: Send + 'static,
        Work: FnOnce(Arc<dyn NativeBackend>) -> Result<T, PlatformError> + Send + 'static,
    {
        let backend = Arc::clone(&self.backend);
        Box::pin(dispatch(move |sender| sender.send(work(backend))))
    }
}

impl MenuService for NativeServices {
    fn install(&self, window: WindowCapability, menu: SemanticMenu) -> AsyncResult<MenuBinding> {
        let binding = self
            .next_menu_binding
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.spawn_window(window, move |backend| {
            backend.install_menu(window, menu)?;
            Ok(binding)
        })
    }
}

impl DialogService for NativeServices {
    fn choose_path(
        &self,
        window: WindowCapability,
        request: PathDialog,
    ) -> AsyncResult<WindowResult<Option<UntrustedPathSelection>>> {
        self.spawn_window(window, move |backend| backend.choose_path(window, request))
    }
}

impl ClipboardService for NativeServices {
    fn read(
        &self,
        window: WindowCapability,
        formats: ClipboardFormats,
    ) -> AsyncResult<WindowResult<UntrustedClipboardContent>> {
        self.spawn_window(window, move |backend| {
            backend.read_clipboard(window, formats)
        })
    }

    fn write(
        &self,
        window: WindowCapability,
        content: ClipboardContent,
    ) -> AsyncResult<WindowResult<()>> {
        self.spawn_window(window, move |backend| {
            backend.write_clipboard(window, content)
        })
    }
}

impl ExternalOpenService for NativeServices {
    fn open(
        &self,
        window: WindowCapability,
        intent: ValidatedExternalIntent,
    ) -> AsyncResult<WindowResult<()>> {
        self.spawn_window(window, move |backend| backend.open_external(window, intent))
    }
}

impl ApplicationPathService for NativeServices {
    fn application_paths(&self) -> AsyncResult<ApplicationPaths> {
        self.spawn_global(|backend| backend.application_paths())
    }
}

impl SystemAppearanceService for NativeServices {
    fn current_appearance(&self) -> AsyncResult<SystemAppearance> {
        self.spawn_global(|backend| backend.appearance())
    }
}

impl SystemAppearanceEventService for NativeServices {
    fn subscribe(&self) -> Result<SystemAppearanceEventStream, PlatformError> {
        let (sender, stream) = SystemAppearanceEventStream::channel();
        self.appearance_listeners
            .lock()
            .map_err(|_| PlatformError::Failed {
                operation: "subscribe to system appearance",
                reason: "appearance listener registry is unavailable".into(),
            })?
            .push(sender);
        Ok(stream)
    }
}

/// Concrete registration surface wrapped by `parchmint-ui-iced`.
///
/// It accepts only ParchMint capabilities and never exposes or creates native
/// handles. Keeping this module out of the service bundle prevents application
/// and domain code from registering windows accidentally.
#[doc(hidden)]
pub mod iced_adapter {
    use parchmint_platform_api::WindowCapability;

    use crate::registry::CapabilityRegistry;

    #[derive(Clone)]
    pub struct IcedWindowRegistry {
        registry: CapabilityRegistry,
    }

    impl IcedWindowRegistry {
        pub(crate) fn new(registry: CapabilityRegistry) -> Self {
            Self { registry }
        }

        pub fn register_window(&self, capability: WindowCapability) -> WindowCapability {
            self.registry.register(capability)
        }

        pub fn close_window(&self, capability: WindowCapability) {
            self.registry.unregister(capability);
        }
    }
}
