//! Audited operating-system menu attachment.
//!
//! Raw handles enter this module only after Iced's event-loop-owned
//! `window::run` callback produced them. Linux deliberately does not pass a
//! winit X11/Wayland handle to muda's GTK-only API.

use parchmint_platform_api::{PlatformError, SemanticMenu, WindowCapability};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttachmentKind {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    Native,
    #[cfg(target_os = "linux")]
    InWindow,
}

fn failed(reason: impl Into<String>) -> PlatformError {
    PlatformError::Failed {
        operation: "attach native menu",
        reason: reason.into(),
    }
}

#[cfg(target_os = "linux")]
fn matching_handles(raw_window: RawWindowHandle, raw_display: RawDisplayHandle) -> bool {
    matches!(
        (raw_window, raw_display),
        (RawWindowHandle::Xlib(_), RawDisplayHandle::Xlib(_))
            | (RawWindowHandle::Xcb(_), RawDisplayHandle::Xcb(_))
            | (RawWindowHandle::Wayland(_), RawDisplayHandle::Wayland(_))
    )
}

#[cfg(target_os = "linux")]
pub(crate) fn attach(
    _window: WindowCapability,
    _binding: u64,
    _menu: &SemanticMenu,
    raw_window: RawWindowHandle,
    raw_display: RawDisplayHandle,
) -> Result<AttachmentKind, PlatformError> {
    if matching_handles(raw_window, raw_display) {
        Ok(AttachmentKind::InWindow)
    } else {
        Err(failed(
            "Iced did not provide a matching X11 or Wayland window/display handle",
        ))
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn detach(
    _window: WindowCapability,
    raw_window: RawWindowHandle,
    raw_display: RawDisplayHandle,
) -> Result<(), PlatformError> {
    if matching_handles(raw_window, raw_display) {
        Ok(())
    } else {
        Err(failed(
            "Iced did not provide a matching X11 or Wayland window/display handle",
        ))
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::{num::NonZeroU32, ptr::NonNull};

    use raw_window_handle::{
        WaylandDisplayHandle, WaylandWindowHandle, XcbDisplayHandle, XcbWindowHandle,
        XlibDisplayHandle, XlibWindowHandle,
    };

    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum HandleFamily {
        Xlib,
        Xcb,
        Wayland,
    }

    fn handles(
        window_family: HandleFamily,
        display_family: HandleFamily,
    ) -> (RawWindowHandle, RawDisplayHandle) {
        let raw_window = match window_family {
            HandleFamily::Xlib => RawWindowHandle::Xlib(XlibWindowHandle::new(1)),
            HandleFamily::Xcb => RawWindowHandle::Xcb(XcbWindowHandle::new(
                NonZeroU32::new(1).expect("nonzero Xcb window"),
            )),
            HandleFamily::Wayland => {
                RawWindowHandle::Wayland(WaylandWindowHandle::new(NonNull::dangling()))
            }
        };
        let raw_display = match display_family {
            HandleFamily::Xlib => RawDisplayHandle::Xlib(XlibDisplayHandle::new(None, 0)),
            HandleFamily::Xcb => RawDisplayHandle::Xcb(XcbDisplayHandle::new(None, 0)),
            HandleFamily::Wayland => {
                RawDisplayHandle::Wayland(WaylandDisplayHandle::new(NonNull::dangling()))
            }
        };
        (raw_window, raw_display)
    }

    #[test]
    fn linux_menu_lifecycle_accepts_only_matching_window_and_display_families() {
        let window = WindowCapability::new(1, 1);
        let menu = SemanticMenu::new(Vec::new());
        let families = [HandleFamily::Xlib, HandleFamily::Xcb, HandleFamily::Wayland];

        for window_family in families {
            for display_family in families {
                let matching = window_family == display_family;
                let (raw_window, raw_display) = handles(window_family, display_family);
                let attach_result = attach(window, 1, &menu, raw_window, raw_display);
                assert_eq!(
                    attach_result.is_ok(),
                    matching,
                    "attach {window_family:?}/{display_family:?}"
                );
                if !matching {
                    assert_eq!(
                        attach_result,
                        Err(failed(
                            "Iced did not provide a matching X11 or Wayland window/display handle"
                        ))
                    );
                }

                let (raw_window, raw_display) = handles(window_family, display_family);
                let detach_result = detach(window, raw_window, raw_display);
                assert_eq!(
                    detach_result.is_ok(),
                    matching,
                    "detach {window_family:?}/{display_family:?}"
                );
                if !matching {
                    assert_eq!(
                        detach_result,
                        Err(failed(
                            "Iced did not provide a matching X11 or Wayland window/display handle"
                        ))
                    );
                }
            }
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod native {
    #[cfg(target_os = "windows")]
    use std::collections::HashMap;
    use std::{
        cell::RefCell,
        sync::{Arc, Mutex, OnceLock, Weak},
    };

    use muda::{Menu, MenuItem, PredefinedMenuItem, Submenu, accelerator::Accelerator};
    use parchmint_platform_api::{SemanticMenuEntry, WindowCapability};
    use raw_window_handle::{RawDisplayHandle, RawWindowHandle};

    use super::{AttachmentKind, failed};
    use crate::{NativeServices, runtime::accelerator};

    const EVENT_PREFIX: &str = "parchmint-menu";

    static ACTIVATION_TARGETS: OnceLock<Mutex<Vec<Weak<NativeServices>>>> = OnceLock::new();

    pub(crate) fn register_activation_target(services: &Arc<NativeServices>) {
        ACTIVATION_TARGETS
            .get_or_init(|| {
                muda::MenuEvent::set_event_handler(Some(route_menu_event));
                Mutex::new(Vec::new())
            })
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(Arc::downgrade(services));
    }

    fn route_menu_event(event: muda::MenuEvent) {
        let Some((window, binding, command)) = decode_event_id(event.id.as_ref()) else {
            return;
        };
        let Some(targets) = ACTIVATION_TARGETS.get() else {
            return;
        };
        let mut targets = targets.lock().unwrap_or_else(|error| error.into_inner());
        targets.retain(|target| {
            let Some(target) = target.upgrade() else {
                return false;
            };
            let _ = target.publish_menu_activation(window, binding, command.clone());
            true
        });
    }

    fn event_id(window: WindowCapability, binding: u64, command: &str) -> String {
        format!(
            "{EVENT_PREFIX}|{}|{}|{binding}|{command}",
            window.window_id(),
            window.generation()
        )
    }

    fn decode_event_id(value: &str) -> Option<(WindowCapability, u64, String)> {
        let mut fields = value.splitn(5, '|');
        (fields.next()? == EVENT_PREFIX).then_some(())?;
        let window_id = fields.next()?.parse().ok()?;
        let generation = fields.next()?.parse().ok()?;
        let binding = fields.next()?.parse().ok()?;
        let command = fields.next()?.to_owned();
        Some((
            WindowCapability::new(window_id, generation),
            binding,
            command,
        ))
    }

    enum Parent<'a> {
        Root(&'a Menu),
        Submenu(&'a Submenu),
    }

    impl Parent<'_> {
        fn append(&self, item: &dyn muda::IsMenuItem) -> muda::Result<()> {
            match self {
                Self::Root(menu) => menu.append(item),
                Self::Submenu(menu) => menu.append(item),
            }
        }
    }

    fn build_menu(
        window: WindowCapability,
        binding: u64,
        semantic: &parchmint_platform_api::SemanticMenu,
    ) -> Result<Menu, parchmint_platform_api::PlatformError> {
        let menu = Menu::new();
        let mut submenu_id = 0_u64;
        append_entries(
            Parent::Root(&menu),
            semantic.entries(),
            window,
            binding,
            &mut submenu_id,
        )?;
        Ok(menu)
    }

    fn append_entries(
        parent: Parent<'_>,
        entries: &[SemanticMenuEntry],
        window: WindowCapability,
        binding: u64,
        submenu_id: &mut u64,
    ) -> Result<(), parchmint_platform_api::PlatformError> {
        for entry in entries {
            match entry {
                SemanticMenuEntry::Command(command) => {
                    let accelerator = accelerator(command.id())
                        .map(str::parse::<Accelerator>)
                        .transpose()
                        .map_err(|error| failed(error.to_string()))?;
                    let item = MenuItem::with_id(
                        event_id(window, binding, command.id()),
                        command.label(),
                        command.enabled(),
                        accelerator,
                    );
                    parent
                        .append(&item)
                        .map_err(|error| failed(error.to_string()))?;
                }
                SemanticMenuEntry::Separator => parent
                    .append(&PredefinedMenuItem::separator())
                    .map_err(|error| failed(error.to_string()))?,
                SemanticMenuEntry::Submenu { label, entries } => {
                    *submenu_id = submenu_id.saturating_add(1);
                    let submenu = Submenu::with_id(
                        format!(
                            "{EVENT_PREFIX}-submenu-{}-{}-{binding}-{}",
                            window.window_id(),
                            window.generation(),
                            *submenu_id
                        ),
                        label,
                        true,
                    );
                    append_entries(
                        Parent::Submenu(&submenu),
                        entries,
                        window,
                        binding,
                        submenu_id,
                    )?;
                    parent
                        .append(&submenu)
                        .map_err(|error| failed(error.to_string()))?;
                }
            }
        }
        Ok(())
    }

    #[cfg(target_os = "windows")]
    struct AttachedMenu {
        hwnd: isize,
        menu: Menu,
    }

    #[cfg(target_os = "windows")]
    thread_local! {
        static ATTACHED: RefCell<HashMap<WindowCapability, AttachedMenu>> =
            RefCell::new(HashMap::new());
    }

    #[cfg(target_os = "macos")]
    struct AttachedMenu {
        window: WindowCapability,
        menu: Menu,
    }

    #[cfg(target_os = "macos")]
    thread_local! {
        static ATTACHED: RefCell<Option<AttachedMenu>> = const { RefCell::new(None) };
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn attach(
        window: WindowCapability,
        binding: u64,
        semantic: &parchmint_platform_api::SemanticMenu,
        raw_window: RawWindowHandle,
        raw_display: RawDisplayHandle,
    ) -> Result<AttachmentKind, parchmint_platform_api::PlatformError> {
        let (RawWindowHandle::Win32(handle), RawDisplayHandle::Windows(_)) =
            (raw_window, raw_display)
        else {
            return Err(failed(
                "Iced did not provide matching Win32 window/display handles",
            ));
        };
        let hwnd = handle.hwnd.get();
        let menu = build_menu(window, binding, semantic)?;
        ATTACHED.with(|attached| {
            let mut attached = attached.borrow_mut();
            if let Some(previous) = attached.remove(&window) {
                // SAFETY: Both HWND values came from Iced's live `window::run`
                // callback. Removal occurs before replacement in that same
                // event-loop callback.
                unsafe { previous.menu.remove_for_hwnd(previous.hwnd) }
                    .map_err(|error| failed(error.to_string()))?;
            }
            // SAFETY: `hwnd` is the non-zero Win32 handle supplied by Iced for
            // the live window executing this callback. The Menu stays retained
            // in thread-local event-loop storage until rebind or close.
            unsafe { menu.init_for_hwnd(hwnd) }.map_err(|error| failed(error.to_string()))?;
            attached.insert(window, AttachedMenu { hwnd, menu });
            Ok(AttachmentKind::Native)
        })
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn detach(
        window: WindowCapability,
        raw_window: RawWindowHandle,
        raw_display: RawDisplayHandle,
    ) -> Result<(), parchmint_platform_api::PlatformError> {
        let (RawWindowHandle::Win32(handle), RawDisplayHandle::Windows(_)) =
            (raw_window, raw_display)
        else {
            return Err(failed(
                "Iced did not provide matching Win32 window/display handles",
            ));
        };
        ATTACHED.with(|attached| {
            let Some(previous) = attached.borrow_mut().remove(&window) else {
                return Ok(());
            };
            if previous.hwnd != handle.hwnd.get() {
                return Err(failed("the Win32 menu handle changed before detach"));
            }
            // SAFETY: the retained HWND is compared with the live handle from
            // this `window::run` callback immediately before removal.
            unsafe { previous.menu.remove_for_hwnd(previous.hwnd) }
                .map_err(|error| failed(error.to_string()))
        })
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn attach(
        window: WindowCapability,
        binding: u64,
        semantic: &parchmint_platform_api::SemanticMenu,
        raw_window: RawWindowHandle,
        raw_display: RawDisplayHandle,
    ) -> Result<AttachmentKind, parchmint_platform_api::PlatformError> {
        let (RawWindowHandle::AppKit(_), RawDisplayHandle::AppKit(_)) = (raw_window, raw_display)
        else {
            return Err(failed(
                "Iced did not provide matching AppKit window/display handles",
            ));
        };
        let menu = build_menu(window, binding, semantic)?;
        ATTACHED.with(|attached| {
            if let Some(previous) = attached.borrow_mut().take() {
                previous.menu.remove_for_nsapp();
            }
            // `window::run` executes this on the winit/AppKit event thread,
            // satisfying muda's main-thread requirement. A menu bar belongs to
            // NSApplication; the validated NSView handle establishes that this
            // request came from a live AppKit window.
            menu.init_for_nsapp();
            *attached.borrow_mut() = Some(AttachedMenu { window, menu });
            Ok(AttachmentKind::Native)
        })
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn detach(
        window: WindowCapability,
        raw_window: RawWindowHandle,
        raw_display: RawDisplayHandle,
    ) -> Result<(), parchmint_platform_api::PlatformError> {
        let (RawWindowHandle::AppKit(_), RawDisplayHandle::AppKit(_)) = (raw_window, raw_display)
        else {
            return Err(failed(
                "Iced did not provide matching AppKit window/display handles",
            ));
        };
        ATTACHED.with(|attached| {
            let mut attached = attached.borrow_mut();
            if attached.as_ref().is_some_and(|menu| menu.window == window)
                && let Some(previous) = attached.take()
            {
                previous.menu.remove_for_nsapp();
            }
        });
        Ok(())
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub(crate) use native::{attach, detach, register_activation_target};
