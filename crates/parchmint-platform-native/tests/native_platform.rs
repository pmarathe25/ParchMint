//! Requirements-first native integration contracts.
//!
//! These tests exercise native services with the production capability
//! registry. The Iced crate separately tests its private registration adapter.

use std::{
    future::Future,
    sync::{Arc, Condvar, Mutex},
    task::{Context, Poll, Wake, Waker},
};

use parchmint_platform_api::{
    ClipboardContent, ClipboardFormats, MenuCommand, PathDialog, PlatformError, SemanticMenu,
    SemanticMenuEntry, SystemAppearance, UntrustedClipboardContent, ValidatedExternalIntent,
    WindowCapability,
};
#[cfg(target_os = "linux")]
use parchmint_platform_native::iced_adapter::IcedMenuAttachment;
use parchmint_platform_native::testing::NativeFixture;
#[cfg(target_os = "linux")]
use raw_window_handle::{RawDisplayHandle, RawWindowHandle, XlibDisplayHandle, XlibWindowHandle};

const WINDOW_ID: u64 = 27;
const LIVE_GENERATION: u64 = 41;
const STALE_GENERATION: u64 = 40;

fn live_window() -> WindowCapability {
    WindowCapability::new(WINDOW_ID, LIVE_GENERATION)
}

fn stale_window() -> WindowCapability {
    WindowCapability::new(WINDOW_ID, STALE_GENERATION)
}

fn menu() -> SemanticMenu {
    SemanticMenu::new(vec![
        SemanticMenuEntry::Command(MenuCommand::new("file.open", "Open")),
        SemanticMenuEntry::Command(MenuCommand::new("file.save", "Save")),
        SemanticMenuEntry::Separator,
        SemanticMenuEntry::Submenu {
            label: "Edit".to_owned(),
            entries: vec![
                SemanticMenuEntry::Command(MenuCommand::new("edit.paste", "Paste")),
                SemanticMenuEntry::Command(MenuCommand::disabled("edit.cut", "Cut")),
            ],
        },
    ])
}

fn fixture() -> NativeFixture {
    NativeFixture::new()
}

fn block_on<T>(future: impl Future<Output = T>) -> T {
    let parker = Arc::new(ThreadParker {
        ready: Mutex::new(false),
        wake: Condvar::new(),
    });
    let waker = Waker::from(Arc::clone(&parker));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
        let mut ready = parker.ready.lock().expect("parker lock");
        while !*ready {
            ready = parker.wake.wait(ready).expect("parker wait");
        }
        *ready = false;
    }
}

struct ThreadParker {
    ready: Mutex<bool>,
    wake: Condvar,
}

impl Wake for ThreadParker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        *self.ready.lock().expect("parker lock") = true;
        self.wake.notify_one();
    }
}

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
#[test]
fn native_desktop_uses_target_menu_and_dialog_conventions() {
    let native = fixture();
    let window = native.register_window(live_window());

    let semantic_menu = menu();
    block_on(native.menus().install(window, semantic_menu.clone())).expect("menu install");
    let snapshot = native.menu_snapshot(window).expect("live menu snapshot");

    assert_eq!(
        snapshot.commands(),
        &["file.open", "file.save", "edit.paste", "edit.cut"]
    );
    assert_eq!(snapshot.menu(), &semantic_menu);
    assert!(snapshot.contains_separator());
    #[cfg(target_os = "macos")]
    assert_eq!(snapshot.accelerator("file.open"), Some("Cmd+O"));
    #[cfg(not(target_os = "macos"))]
    assert_eq!(snapshot.accelerator("file.open"), Some("Ctrl+O"));

    let selection = block_on(native.dialogs().choose_path(window, PathDialog::default()))
        .expect("dialog dispatch")
        .into_value()
        .expect("fixture selection");
    #[cfg(target_os = "windows")]
    assert_eq!(
        selection.as_path(),
        std::path::Path::new(r"C:\outside\project.parchment")
    );
    #[cfg(not(target_os = "windows"))]
    assert_eq!(
        selection.as_path(),
        std::path::Path::new("/outside/project.parchment")
    );
}

#[test]
fn menu_activations_retain_the_exact_binding_and_reject_stale_or_disabled_commands() {
    let native = fixture();
    let window = native.register_window(live_window());
    let binding = block_on(native.menus().install(window, menu()))
        .expect("menu install")
        .into_value();
    let stream = native
        .menu_activations()
        .subscribe()
        .expect("activation subscription");

    native
        .activate_menu(window, binding, "file.open")
        .expect("enabled activation");
    let activation = stream.try_next().expect("activation read").expect("event");
    assert_eq!(activation.binding.window(), window);
    assert_eq!(activation.binding.into_value(), binding);
    assert_eq!(activation.command_id, "file.open");

    assert!(
        native
            .activate_menu(window, binding + 1, "file.open")
            .is_err()
    );
    assert!(native.activate_menu(window, binding, "edit.cut").is_err());
    native.close_window(window);
    assert!(matches!(
        native.activate_menu(window, binding, "file.open"),
        Err(PlatformError::StaleCapability { .. })
    ));
}

#[cfg(target_os = "linux")]
#[test]
fn iced_menu_attachment_uses_the_linux_fallback_and_rejects_stale_lifecycle_requests() {
    let native = fixture();
    let window = native.register_window(live_window());
    let first = block_on(native.menus().install(window, menu()))
        .expect("first menu install")
        .into_value();
    let registry = native.registry();
    let raw_window = RawWindowHandle::Xlib(XlibWindowHandle::new(17));
    let raw_display = RawDisplayHandle::Xlib(XlibDisplayHandle::new(None, 0));

    assert_eq!(
        registry
            .attach_menu(window, first, raw_window, raw_display)
            .expect("matching live Xlib handles"),
        IcedMenuAttachment::InWindow
    );

    let second = block_on(native.menus().install(window, menu()))
        .expect("replacement menu install")
        .into_value();
    assert!(
        registry
            .attach_menu(window, first, raw_window, raw_display)
            .is_err()
    );
    assert_eq!(
        registry
            .attach_menu(window, second, raw_window, raw_display)
            .expect("current replacement binding"),
        IcedMenuAttachment::InWindow
    );

    native.close_window(window);
    assert!(
        registry
            .attach_menu(window, second, raw_window, raw_display)
            .is_err()
    );
}

#[test]
fn clipboard_formats_preserve_plain_text_and_untrusted_rich_text() {
    let native = fixture();
    let window = native.register_window(live_window());
    let html = "<script>discard me</script><p>paste</p>";

    block_on(
        native
            .clipboard()
            .write(window, ClipboardContent::plain_text("paste\n")),
    )
    .expect("clipboard write");
    let plain = block_on(
        native
            .clipboard()
            .read(window, ClipboardFormats::plain_text()),
    )
    .expect("plain clipboard read");
    assert_eq!(plain.value().plain_text(), Some("paste\n"));
    assert_eq!(plain.value().html(), None);

    native.seed_external_clipboard(UntrustedClipboardContent::empty().with_html(html));
    let rich = block_on(
        native
            .clipboard()
            .read(window, ClipboardFormats::plain_text_and_html()),
    )
    .expect("rich clipboard read");
    assert_eq!(rich.value().html(), Some(html));
    assert_eq!(rich.value().plain_text(), None);
}

#[test]
fn appearance_tracks_the_native_system_value() {
    let native = fixture();
    let window = native.register_window(live_window());

    native.set_system_appearance(SystemAppearance::Dark);
    assert_eq!(
        block_on(native.appearance().current_appearance()).expect("appearance"),
        SystemAppearance::Dark
    );
    native.set_system_appearance(SystemAppearance::Light);
    assert_eq!(
        block_on(native.appearance().current_appearance()).expect("appearance"),
        SystemAppearance::Light
    );
    assert_eq!(native.registered_window(window), Some(live_window()));
}

#[test]
fn injectable_appearance_stream_preserves_ordered_generations() {
    let native = fixture();
    let stream = native
        .appearance_events()
        .subscribe()
        .expect("appearance subscription");

    native.set_system_appearance(SystemAppearance::Dark);
    native.set_system_appearance(SystemAppearance::Light);

    let dark = stream.try_next().unwrap().expect("dark event");
    let light = stream.try_next().unwrap().expect("light event");
    assert_eq!(
        (dark.generation, dark.appearance),
        (1, SystemAppearance::Dark)
    );
    assert_eq!(
        (light.generation, light.appearance),
        (2, SystemAppearance::Light)
    );
    assert_eq!(stream.try_next(), Ok(None));
}

#[test]
fn silent_system_changes_do_not_create_events_or_background_reads() {
    let native = fixture();
    let stream = native
        .appearance_events()
        .subscribe()
        .expect("appearance subscription");

    native.set_system_appearance_silently(SystemAppearance::Dark);
    assert_eq!(stream.try_next(), Ok(None));
    assert_eq!(native.system_appearance_reads(), 0);

    assert_eq!(
        block_on(native.appearance().current_appearance()).expect("appearance"),
        SystemAppearance::Dark
    );
    assert_eq!(native.system_appearance_reads(), 1);
}

#[test]
fn stale_capabilities_reject_every_window_scoped_native_service() {
    macro_rules! assert_stale {
        ($window:expr, $future:expr) => {
            assert_eq!(
                block_on($future),
                Err(PlatformError::stale_capability($window))
            );
        };
    }

    let native = fixture();
    let live = native.register_window(live_window());
    let stale = stale_window();
    native.close_window(live);
    native.register_window(live_window());

    assert_stale!(stale, native.menus().install(stale, menu()));
    assert_stale!(
        stale,
        native
            .clipboard()
            .write(stale, ClipboardContent::plain_text("stale"))
    );
    assert_stale!(
        stale,
        native.dialogs().choose_path(stale, PathDialog::default())
    );
    assert_stale!(
        stale,
        native
            .clipboard()
            .read(stale, ClipboardFormats::plain_text())
    );
    let intent = ValidatedExternalIntent::https_url("https://parchmint.example/help")
        .expect("validated HTTPS intent");
    assert_stale!(stale, native.external_open().open(stale, intent));
}

#[test]
fn worker_dispatch_is_nonblocking_and_revalidates_before_completion() {
    let native = fixture();
    let window = native.register_window(live_window());
    let paused = native.pause_next_menu_install();

    let install = native.menus().install(window, menu());
    paused.wait_until_started();
    native.close_window(window);
    paused.release();

    assert_eq!(
        block_on(install),
        Err(PlatformError::stale_capability(window))
    );
}

#[test]
fn closing_a_window_before_dispatched_work_begins_prevents_the_dialog_side_effect() {
    let native = fixture();
    let window = native.register_window(live_window());
    let paused = native.pause_next_window_work();

    let choose_path = native.dialogs().choose_path(window, PathDialog::default());
    paused.wait_until_started();
    native.close_window(window);
    paused.release();

    assert_eq!(
        block_on(choose_path),
        Err(PlatformError::stale_capability(window))
    );
    assert_eq!(native.dialog_invocations(), 0);
}

#[test]
fn application_paths_follow_target_conventions_and_external_open_is_https_only() {
    let native = fixture();
    let window = native.register_window(live_window());
    let paths =
        block_on(native.application_paths().application_paths()).expect("application paths");

    #[cfg(target_os = "windows")]
    assert!(paths.cache().ends_with(r"ParchMint\Cache"));
    #[cfg(target_os = "macos")]
    assert!(paths.cache().ends_with("Library/Caches/ParchMint"));
    #[cfg(target_os = "linux")]
    assert!(paths.cache().ends_with(".cache/parchmint"));

    let intent = ValidatedExternalIntent::https_url("https://parchmint.example/help")
        .expect("validated HTTPS intent");
    block_on(native.external_open().open(window, intent)).expect("external open dispatch");
    assert_eq!(
        native.opened_external_urls(),
        ["https://parchmint.example/help"]
    );
    assert!(ValidatedExternalIntent::https_url("file:///tmp/project").is_err());
}

#[test]
fn replacing_a_generation_invalidates_the_old_capability_at_delivery() {
    let native = fixture();
    let old = native.register_window(stale_window());
    let paused = native.pause_next_menu_install();
    let install = native.menus().install(old, menu());
    paused.wait_until_started();
    native.register_window(live_window());
    paused.release();

    assert_eq!(block_on(install), Err(PlatformError::stale_capability(old)));
    assert_eq!(native.registered_window(live_window()), Some(live_window()));
}
