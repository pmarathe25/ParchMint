//! Platform-boundary contract tests.
//!
//! Native adapters retain handles privately. This public API uses ParchMint
//! values only, and receivers validate the originating window generation.

use std::{path::Path, sync::Arc};

use parchmint_platform_api::{
    ApplicationPathService, ApplicationPaths, AsyncResult, ClipboardContent, ClipboardFormats,
    ClipboardService, DialogService, ExternalOpenService, MenuActivationService, MenuBinding,
    MenuService, PathDialog, PlatformError, SemanticMenu, SystemAppearance,
    SystemAppearanceEventService, SystemAppearanceService, UntrustedClipboardContent,
    UntrustedPathSelection, ValidatedExternalIntent, WindowCapability, WindowResult,
};

fn live_window() -> WindowCapability {
    WindowCapability::new(41, 7)
}

fn stale_window() -> WindowCapability {
    WindowCapability::new(41, 6)
}

#[test]
fn platform_services_are_async_without_native_handles() {
    fn menu<S: MenuService + ?Sized>(service: &S, window: WindowCapability) {
        let _future: AsyncResult<MenuBinding> = service.install(window, SemanticMenu::default());
    }

    fn dialog<S: DialogService + ?Sized>(service: &S, window: WindowCapability) {
        let _future: AsyncResult<WindowResult<Option<UntrustedPathSelection>>> =
            service.choose_path(window, PathDialog::default());
    }

    fn clipboard_read<S: ClipboardService + ?Sized>(service: &S, window: WindowCapability) {
        let _future: AsyncResult<WindowResult<UntrustedClipboardContent>> =
            service.read(window, ClipboardFormats::plain_text());
    }

    fn clipboard_write<S: ClipboardService + ?Sized>(service: &S, window: WindowCapability) {
        let _future: AsyncResult<WindowResult<()>> =
            service.write(window, ClipboardContent::plain_text("text"));
    }

    fn external<S: ExternalOpenService + ?Sized>(
        service: &S,
        window: WindowCapability,
        intent: ValidatedExternalIntent,
    ) {
        let _future: AsyncResult<WindowResult<()>> = service.open(window, intent);
    }

    fn application_paths<S: ApplicationPathService + ?Sized>(service: &S) {
        let _future: AsyncResult<ApplicationPaths> = service.application_paths();
    }

    fn appearance<S: SystemAppearanceService + ?Sized>(service: &S) {
        let _future: AsyncResult<SystemAppearance> = service.current_appearance();
    }

    let _ = (
        menu::<dyn MenuService>,
        dialog::<dyn DialogService>,
        clipboard_read::<dyn ClipboardService>,
        clipboard_write::<dyn ClipboardService>,
        external::<dyn ExternalOpenService>,
        application_paths::<dyn ApplicationPathService>,
        appearance::<dyn SystemAppearanceService>,
    );

    fn assert_service<T: Send + Sync + ?Sized>() {}
    assert_service::<dyn MenuService>();
    assert_service::<dyn MenuActivationService>();
    assert_service::<dyn DialogService>();
    assert_service::<dyn ClipboardService>();
    assert_service::<dyn ExternalOpenService>();
    assert_service::<dyn ApplicationPathService>();
    assert_service::<dyn SystemAppearanceService>();
    assert_service::<dyn SystemAppearanceEventService>();
    let _services: Option<Arc<dyn MenuService>> = None;
}

fn receive<T>(live: WindowCapability, result: WindowResult<T>) -> Result<T, PlatformError> {
    if result.window() != live {
        return Err(PlatformError::stale_capability(result.window()));
    }
    Ok(result.into_value())
}

#[test]
fn every_window_result_preserves_its_exact_generation_for_receiving_validation() {
    let live = live_window();
    let stale = stale_window();
    assert_eq!(stale.window_id(), live.window_id());
    assert!(stale.generation() < live.generation());

    assert_eq!(receive(live, WindowResult::new(live, 9)), Ok(9));
    assert_eq!(
        receive(live, WindowResult::new(stale, ())),
        Err(PlatformError::StaleCapability {
            window_id: stale.window_id(),
            generation: stale.generation(),
        })
    );
}

#[test]
fn external_open_requires_a_validated_https_intent() {
    let https = ValidatedExternalIntent::https_url("https://parchmint.example/help")
        .expect("a checked HTTPS URL should be accepted");
    assert_eq!(https.scheme(), "https");

    assert!(ValidatedExternalIntent::https_url("javascript:alert(1)").is_err());
    assert!(ValidatedExternalIntent::https_url("file:///etc/passwd").is_err());
    assert!(ValidatedExternalIntent::https_url("https://").is_err());
    assert!(ValidatedExternalIntent::https_url("https://user@parchmint.example").is_err());
    assert!(ValidatedExternalIntent::https_url("https://parchmint.example/%zz").is_err());
}

#[test]
fn dialog_and_clipboard_results_remain_untrusted_after_generation_validation() {
    fn candidate_path(value: &UntrustedPathSelection) -> &Path {
        value.as_path()
    }

    fn untrusted_text(value: &UntrustedClipboardContent) -> Option<&str> {
        value.plain_text()
    }

    let live = live_window();
    let path = receive(
        live,
        WindowResult::new(
            live,
            UntrustedPathSelection::new("/outside/project.parchment"),
        ),
    )
    .expect("the receiving boundary accepts the exact live generation");
    let clipboard = receive(
        live,
        WindowResult::new(
            live,
            UntrustedClipboardContent::empty().with_plain_text("<p>paste</p>"),
        ),
    )
    .expect("the receiving boundary accepts the exact live generation");
    assert_eq!(
        candidate_path(&path),
        Path::new("/outside/project.parchment")
    );
    assert_eq!(untrusted_text(&clipboard), Some("<p>paste</p>"));

    // These values still need project-path validation or clipboard sanitizing;
    // wrapping them does not grant filesystem or editor authority.
}
