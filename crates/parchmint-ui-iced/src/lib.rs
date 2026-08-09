//! Iced desktop adapter.
//!
//! Window creation remains owned by this crate. Its concrete native-platform
//! integration registers ParchMint capabilities only; no raw window handle is
//! exposed through the platform API and no second event loop is created.

use parchmint_platform_api::WindowCapability;
use parchmint_platform_native::iced_adapter::IcedWindowRegistry;

#[cfg_attr(not(test), allow(dead_code))]
struct IcedWindowAdapter {
    native_windows: IcedWindowRegistry,
}

#[cfg_attr(not(test), allow(dead_code))]
impl IcedWindowAdapter {
    fn new(native_windows: IcedWindowRegistry) -> Self {
        Self { native_windows }
    }

    fn register_window(&self, capability: WindowCapability) -> WindowCapability {
        self.native_windows.register_window(capability)
    }

    fn close_window(&self, capability: WindowCapability) {
        self.native_windows.close_window(capability);
    }
}

#[cfg(test)]
mod tests {
    use parchmint_platform_native::testing::NativeFixture;

    use super::*;

    #[test]
    fn iced_adapter_owns_window_registration() {
        let native = NativeFixture::new();
        let adapter = IcedWindowAdapter::new(native.registry());
        let window = WindowCapability::new(27, 1);

        assert_eq!(adapter.register_window(window), window);
        assert_eq!(native.registered_window(window), Some(window));

        adapter.close_window(window);
        assert_eq!(native.registered_window(window), None);
    }
}
