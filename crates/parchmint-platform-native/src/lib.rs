//! Native Windows, macOS, and Linux services for ParchMint's Iced desktop.
//!
//! This crate does not create windows. The Iced adapter privately registers a
//! ParchMint [`WindowCapability`](parchmint_platform_api::WindowCapability),
//! and every window-scoped operation authorizes that exact generation before
//! work starts and again immediately before its completion is delivered.

mod async_task;
mod native_menu;
mod registry;
mod runtime;
#[doc(hidden)]
pub mod testing;

use std::{
    collections::HashMap,
    error::Error,
    fmt,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
};

use async_task::dispatch;
use parchmint_platform_api::{
    ApplicationPathService, ApplicationPaths, AsyncResult, ClipboardContent, ClipboardFormats,
    ClipboardService, DialogService, ExternalOpenService, MenuActivation, MenuActivationService,
    MenuActivationStream, MenuBinding, MenuService, PathDialog, PlatformError, SemanticMenu,
    SemanticMenuEntry, SystemAppearance, SystemAppearanceEvent, SystemAppearanceEventService,
    SystemAppearanceEventStream, SystemAppearanceService, UntrustedClipboardContent,
    UntrustedPathSelection, ValidatedExternalIntent, WindowCapability, WindowResult,
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
#[cfg_attr(feature = "interaction-harness", derive(Clone))]
pub struct NativePlatform {
    pub dialogs: Arc<dyn DialogService>,
    pub menus: Arc<dyn MenuService>,
    pub menu_activations: Arc<dyn MenuActivationService>,
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
        Self::with_backend_and_before_window_work(backend, Arc::new(|| {}))
    }

    fn with_backend_and_before_window_work(
        backend: Arc<dyn NativeBackend>,
        before_window_work: Arc<dyn Fn() + Send + Sync>,
    ) -> (Self, Arc<NativeServices>) {
        let registry = CapabilityRegistry::new();
        let services = Arc::new(NativeServices::new(
            Arc::clone(&backend),
            registry.clone(),
            before_window_work,
        ));
        #[cfg(target_os = "macos")]
        native_menu::register_activation_target(&services);
        let platform = Self {
            dialogs: services.clone(),
            menus: services.clone(),
            menu_activations: services.clone(),
            clipboard: services.clone(),
            external_open: services.clone(),
            appearance: services.clone(),
            appearance_events: services.clone(),
            application_paths: services.clone(),
            iced_registry: iced_adapter::IcedWindowRegistry::new(Arc::clone(&services)),
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
    before_window_work: Arc<dyn Fn() + Send + Sync>,
    next_menu_binding: AtomicU64,
    installed_menus: Arc<Mutex<HashMap<WindowCapability, InstalledMenu>>>,
    menu_activation_listeners: Mutex<Vec<mpsc::Sender<MenuActivation>>>,
    next_appearance_generation: AtomicU64,
    appearance_listeners: Mutex<Vec<mpsc::Sender<SystemAppearanceEvent>>>,
    observed_appearance: Mutex<Option<SystemAppearance>>,
}

impl NativeServices {
    fn new(
        backend: Arc<dyn NativeBackend>,
        registry: CapabilityRegistry,
        before_window_work: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        Self {
            backend,
            registry,
            before_window_work,
            next_menu_binding: AtomicU64::new(1),
            installed_menus: Arc::new(Mutex::new(HashMap::new())),
            menu_activation_listeners: Mutex::new(Vec::new()),
            next_appearance_generation: AtomicU64::new(1),
            appearance_listeners: Mutex::new(Vec::new()),
            observed_appearance: Mutex::new(None),
        }
    }

    fn publish_appearance(&self, appearance: SystemAppearance) {
        let changed = self
            .observed_appearance
            .lock()
            .map(|mut observed| {
                if *observed == Some(appearance) {
                    false
                } else {
                    *observed = Some(appearance);
                    true
                }
            })
            .unwrap_or(false);
        if !changed {
            return;
        }
        let event = SystemAppearanceEvent {
            generation: self
                .next_appearance_generation
                .fetch_add(1, Ordering::Relaxed),
            appearance,
        };
        if let Ok(mut listeners) = self.appearance_listeners.lock() {
            listeners.retain(|listener| listener.send(event).is_ok());
        }
    }

    fn publish_menu_activation(
        &self,
        window: WindowCapability,
        binding: u64,
        command_id: impl Into<String>,
    ) -> Result<(), PlatformError> {
        self.registry.authorize(window)?;
        let command_id = command_id.into();
        let enabled = self
            .installed_menus
            .lock()
            .map_err(|_| PlatformError::Failed {
                operation: "activate menu command",
                reason: "installed menu registry is unavailable".into(),
            })?
            .get(&window)
            .is_some_and(|installed| {
                installed.binding == binding
                    && menu_command_enabled(installed.menu.entries(), &command_id)
            });
        if !enabled {
            return Err(PlatformError::Failed {
                operation: "activate menu command",
                reason: "menu binding is stale or the command is disabled".into(),
            });
        }
        let activation = MenuActivation {
            binding: WindowResult::new(window, binding),
            command_id,
        };
        self.menu_activation_listeners
            .lock()
            .map_err(|_| PlatformError::Failed {
                operation: "activate menu command",
                reason: "menu activation listener registry is unavailable".into(),
            })?
            .retain(|listener| listener.send(activation.clone()).is_ok());
        Ok(())
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
        let before_window_work = Arc::clone(&self.before_window_work);
        Box::pin(dispatch(move |sender| {
            before_window_work();
            let result = registry
                .authorize(window)
                .and_then(|()| work(backend).map(|value| WindowResult::new(window, value)));
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
        let installed_menu = menu.clone();
        let install = self.spawn_window(window, move |backend| {
            backend.install_menu(window, menu)?;
            Ok(binding)
        });
        let registry = self.registry.clone();
        let installed_menus = Arc::clone(&self.installed_menus);
        Box::pin(async move {
            let result = install.await?;
            registry.authorize(result.window())?;
            let mut installed_menus =
                installed_menus.lock().map_err(|_| PlatformError::Failed {
                    operation: "install menu",
                    reason: "installed menu registry is unavailable".into(),
                })?;
            if installed_menus
                .get(&result.window())
                .is_none_or(|current| current.binding < *result.value())
            {
                installed_menus.insert(
                    result.window(),
                    InstalledMenu {
                        binding: result.into_value(),
                        menu: installed_menu,
                    },
                );
            }
            Ok(WindowResult::new(window, binding))
        })
    }
}

#[derive(Clone)]
struct InstalledMenu {
    binding: u64,
    menu: SemanticMenu,
}

fn menu_command_enabled(entries: &[SemanticMenuEntry], command_id: &str) -> bool {
    entries.iter().any(|entry| match entry {
        SemanticMenuEntry::Command(command) => command.id() == command_id && command.enabled(),
        SemanticMenuEntry::Separator => false,
        SemanticMenuEntry::Submenu { entries, .. } => menu_command_enabled(entries, command_id),
    })
}

impl MenuActivationService for NativeServices {
    fn subscribe(&self) -> Result<MenuActivationStream, PlatformError> {
        let (sender, stream) = MenuActivationStream::channel();
        self.menu_activation_listeners
            .lock()
            .map_err(|_| PlatformError::Failed {
                operation: "subscribe to menu activations",
                reason: "menu activation listener registry is unavailable".into(),
            })?
            .push(sender);
        Ok(stream)
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
    use std::sync::Arc;

    use parchmint_platform_api::WindowCapability;
    use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

    use crate::{NativeServices, native_menu};

    /// Result of asking the platform adapter to attach an installed menu.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum IcedMenuAttachment {
        /// The operating system owns and displays the attached menu.
        Native,
        /// The Iced surface must display the semantic menu inside the window.
        InWindow,
    }

    #[derive(Clone)]
    pub struct IcedWindowRegistry {
        services: Arc<NativeServices>,
    }

    impl IcedWindowRegistry {
        pub(crate) fn new(services: Arc<NativeServices>) -> Self {
            Self { services }
        }

        pub fn register_window(&self, capability: WindowCapability) -> WindowCapability {
            self.services.registry.register(capability)
        }

        pub fn close_window(&self, capability: WindowCapability) {
            self.services.registry.unregister(capability);
            self.services
                .installed_menus
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .remove(&capability);
            self.services.backend.remove_menu(capability);
        }

        /// Attaches the exact installed binding while Iced guarantees that the
        /// supplied raw handles belong to a live event-loop window.
        pub fn attach_menu(
            &self,
            capability: WindowCapability,
            binding: u64,
            raw_window: RawWindowHandle,
            raw_display: RawDisplayHandle,
        ) -> Result<IcedMenuAttachment, parchmint_platform_api::PlatformError> {
            self.services.registry.authorize(capability)?;
            let semantic = self
                .services
                .installed_menus
                .lock()
                .map_err(|_| parchmint_platform_api::PlatformError::Failed {
                    operation: "attach native menu",
                    reason: "installed menu registry is unavailable".to_owned(),
                })?
                .get(&capability)
                .filter(|installed| installed.binding == binding)
                .map(|installed| installed.menu.clone())
                .ok_or_else(|| parchmint_platform_api::PlatformError::Failed {
                    operation: "attach native menu",
                    reason: "menu binding is stale or unavailable".to_owned(),
                })?;
            native_menu::attach(capability, binding, &semantic, raw_window, raw_display).map(
                |kind| match kind {
                    #[cfg(target_os = "macos")]
                    native_menu::AttachmentKind::Native => IcedMenuAttachment::Native,
                    #[cfg(any(target_os = "linux", target_os = "windows"))]
                    native_menu::AttachmentKind::InWindow => IcedMenuAttachment::InWindow,
                },
            )
        }

        /// Removes native menu state for a live Iced window before it closes.
        pub fn detach_menu(
            &self,
            capability: WindowCapability,
            raw_window: RawWindowHandle,
            raw_display: RawDisplayHandle,
        ) -> Result<(), parchmint_platform_api::PlatformError> {
            native_menu::detach(capability, raw_window, raw_display)
        }
    }
}
