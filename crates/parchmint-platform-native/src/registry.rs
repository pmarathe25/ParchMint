use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use parchmint_platform_api::{PlatformError, WindowCapability};

use crate::async_task::CompletionSender;

#[derive(Clone)]
pub(crate) struct CapabilityRegistry {
    state: Arc<Mutex<RegistryState>>,
}

#[derive(Default)]
struct RegistryState {
    windows: HashMap<u64, WindowCapability>,
}

impl CapabilityRegistry {
    pub(crate) fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(RegistryState::default())),
        }
    }

    pub(crate) fn register(&self, capability: WindowCapability) -> WindowCapability {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.windows.insert(capability.window_id(), capability);
        capability
    }

    pub(crate) fn unregister(&self, capability: WindowCapability) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.windows.get(&capability.window_id()) == Some(&capability) {
            state.windows.remove(&capability.window_id());
        }
    }

    pub(crate) fn authorize(&self, capability: WindowCapability) -> Result<(), PlatformError> {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        Self::authorize_locked(&state, capability)
    }

    pub(crate) fn complete<T>(
        &self,
        capability: WindowCapability,
        sender: CompletionSender<Result<T, PlatformError>>,
        result: Result<T, PlatformError>,
    ) {
        let waker = {
            let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            let delivered = match Self::authorize_locked(&state, capability) {
                Ok(()) => result,
                Err(error) => Err(error),
            };

            // Store while the exact generation is protected from a close or
            // replacement. Wake only after releasing the registry lock so a
            // synchronous executor cannot re-enter and deadlock here.
            sender.store(delivered)
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    pub(crate) fn registered(&self, capability: WindowCapability) -> Option<WindowCapability> {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state
            .windows
            .get(&capability.window_id())
            .copied()
            .filter(|registered| *registered == capability)
    }

    fn authorize_locked(
        state: &RegistryState,
        capability: WindowCapability,
    ) -> Result<(), PlatformError> {
        match state.windows.get(&capability.window_id()) {
            Some(registered) if *registered == capability => Ok(()),
            _ => Err(PlatformError::stale_capability(capability)),
        }
    }
}
