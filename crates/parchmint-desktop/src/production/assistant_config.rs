//! Process-wide non-secret configuration and endpoint-bound credential storage.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{SystemTime, UNIX_EPOCH};

use parchmint_assistant_http::{HttpProvider, Protocol, ProviderConfig, ReqwestTransport};
use parchmint_platform_api::{CredentialSecret, CredentialStore};
use parchmint_preferences::{
    AssistantAuthentication, AssistantPreferences, AssistantProvider, PreferenceCommand,
    PreferenceService,
};
use parchmint_ui_api::PlatformServices;
use sha2::{Digest, Sha256};

use super::block_on;

static NEXT_REFERENCE: AtomicU64 = AtomicU64::new(0);

pub(super) fn provider(profile: &AssistantPreferences) -> Result<HttpProvider, String> {
    let (protocol, endpoint) = match profile.provider {
        AssistantProvider::OpenAi => (
            Protocol::OpenAiResponses,
            "https://api.openai.com/v1".to_owned(),
        ),
        AssistantProvider::OpenAiCompatible => (
            Protocol::OpenAiCompatible,
            profile.endpoint.trim().to_owned(),
        ),
        AssistantProvider::Anthropic => (
            Protocol::AnthropicMessages,
            "https://api.anthropic.com/v1".to_owned(),
        ),
    };
    let require_credential = profile.authentication == AssistantAuthentication::ApiKey;
    let transport = ReqwestTransport::new().map_err(|error| error.to_string())?;
    let provider = HttpProvider::new(
        ProviderConfig {
            protocol,
            base_url: endpoint,
            model: profile.model.trim().to_owned(),
            require_credential,
            compatible_function_tools: profile.compatible_function_tools,
            compatible_streaming: profile.compatible_streaming,
        },
        Arc::new(transport),
    )
    .map_err(|error| error.to_string())?;
    if !require_credential && !provider.is_local_endpoint() {
        return Err("No-authentication mode is available only for a local endpoint.".into());
    }
    Ok(provider)
}

fn store_for<'a>(
    platform: &'a PlatformServices,
    reference: &str,
) -> Result<&'a dyn CredentialStore, String> {
    let store = if reference.starts_with("session_") {
        &platform.session_credentials
    } else if reference.starts_with("os_") {
        &platform.credentials
    } else {
        return Err("The saved credential reference is invalid.".into());
    };
    store
        .as_deref()
        .ok_or_else(|| "Secure credential storage is unavailable.".into())
}

pub(super) fn read_credential(
    platform: &PlatformServices,
    profile: &AssistantPreferences,
    binding: &str,
) -> Result<Option<CredentialSecret>, String> {
    if profile.authentication == AssistantAuthentication::None {
        return Ok(None);
    }
    let Some(reference) = profile.credential_reference.as_deref() else {
        return Ok(None);
    };
    if profile.credential_binding.as_deref() != Some(binding) {
        return Ok(None);
    }
    store_for(platform, reference)?
        .read(reference)
        .map_err(|_| "Secure credential storage is unavailable.".into())
}

pub(super) fn configure(
    preferences: &dyn PreferenceService,
    platform: &PlatformServices,
    mut requested: AssistantPreferences,
    key: &str,
    session_only: bool,
    remove_key: bool,
) -> Result<AssistantPreferences, String> {
    if !(1..=28).contains(&requested.reset_day)
        || requested.monthly_budget_cents == 0
        || requested.warning_cents > requested.monthly_budget_cents
    {
        return Err(
            "Enter a valid monthly limit, warning amount, and reset day from 1 to 28.".into(),
        );
    }
    let provider = provider(&requested)?;
    let binding = provider.credential_binding();
    let current = block_on(preferences.load())
        .map_err(|_| "Could not load AI assistant settings.".to_owned())?;
    let previous = current.values.assistant;
    requested.credential_reference = None;
    requested.credential_binding = None;
    let mut created = None;
    if requested.authentication == AssistantAuthentication::ApiKey {
        if !key.is_empty() {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let sequence = NEXT_REFERENCE.fetch_add(1, Ordering::Relaxed);
            let digest = Sha256::digest(
                format!("{binding}:{timestamp}:{sequence}:{}", std::process::id()).as_bytes(),
            );
            let digest = digest
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            let reference = format!("{}_{digest}", if session_only { "session" } else { "os" });
            store_for(platform, &reference)?
                .write(&reference, &CredentialSecret::new(key))
                .map_err(|_| {
                    "Could not save the key in secure storage. Try session-only storage.".to_owned()
                })?;
            requested.credential_reference = Some(reference.clone());
            requested.credential_binding = Some(binding.clone());
            created = Some(reference);
        } else if !remove_key
            && previous.credential_binding.as_deref() == Some(binding.as_str())
            && let Some(reference) = previous.credential_reference.as_deref()
        {
            let existing = store_for(platform, reference)?
                .read(reference)
                .map_err(|_| "Secure credential storage is unavailable.".to_owned())?;
            if existing.is_some() {
                requested.credential_reference = previous.credential_reference.clone();
                requested.credential_binding = previous.credential_binding.clone();
            }
        }
    }
    if block_on(preferences.update(
        current.revision,
        PreferenceCommand::SetAssistant(requested.clone()),
    ))
    .is_err()
    {
        if let Some(reference) = created.as_deref()
            && let Ok(store) = store_for(platform, reference)
        {
            let _ = store.delete(reference);
        }
        return Err(
            "Could not save AI assistant settings. Your key entry has been retained.".into(),
        );
    }
    if let Some(previous_reference) = previous.credential_reference.as_deref()
        && requested.credential_reference.as_deref() != Some(previous_reference)
        && let Ok(store) = store_for(platform, previous_reference)
    {
        let _ = store.delete(previous_reference);
    }
    Ok(requested)
}

#[cfg(test)]
mod tests {
    use super::*;
    use parchmint_platform_native::{SessionCredentialStore, testing::NativeFixture};
    use parchmint_preferences::{FilePreferenceStore, PreferenceCoordinator};

    fn fixture_platform(store: Arc<dyn CredentialStore>) -> PlatformServices {
        let fixture = NativeFixture::new();
        PlatformServices::new(
            fixture.menus(),
            fixture.dialogs(),
            fixture.clipboard(),
            fixture.external_open(),
            fixture.application_paths(),
            fixture.appearance(),
        )
        .with_session_credentials(store)
    }

    #[test]
    fn session_key_is_bound_to_the_endpoint_and_not_written_to_preferences() {
        let path = std::env::temp_dir().join(format!(
            "parchmint-ai-settings-{}-{}.json",
            std::process::id(),
            NEXT_REFERENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let service = PreferenceCoordinator::new(Arc::new(FilePreferenceStore::new(&path)));
        let platform = fixture_platform(Arc::new(SessionCredentialStore::default()));
        let profile = AssistantPreferences {
            provider: AssistantProvider::OpenAiCompatible,
            model: "model".into(),
            endpoint: "https://provider-a.example/v1".into(),
            ..Default::default()
        };
        let saved = configure(
            &service,
            &platform,
            profile,
            "test-private-value",
            true,
            false,
        )
        .unwrap();
        let binding = provider(&saved).unwrap().credential_binding();
        assert_eq!(
            read_credential(&platform, &saved, &binding)
                .unwrap()
                .unwrap()
                .expose(),
            "test-private-value"
        );
        assert!(
            !std::fs::read_to_string(&path)
                .unwrap()
                .contains("test-private-value")
        );
        let mut changed = saved.clone();
        changed.endpoint = "https://provider-b.example/v1".into();
        let changed_binding = provider(&changed).unwrap().credential_binding();
        assert!(
            read_credential(&platform, &saved, &changed_binding)
                .unwrap()
                .is_none()
        );
        let changed = configure(&service, &platform, changed, "", true, false).unwrap();
        assert!(changed.credential_reference.is_none());
        assert!(
            read_credential(&platform, &changed, &changed_binding)
                .unwrap()
                .is_none()
        );
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn no_authentication_is_restricted_to_local_endpoints() {
        let mut profile = AssistantPreferences {
            provider: AssistantProvider::OpenAiCompatible,
            model: "model".into(),
            endpoint: "https://remote.example/v1".into(),
            authentication: AssistantAuthentication::None,
            ..Default::default()
        };
        assert!(provider(&profile).is_err());
        profile.endpoint = "http://localhost:8080/v1".into();
        assert!(provider(&profile).is_ok());
    }

    #[test]
    fn anthropic_key_is_bound_to_its_native_provider() {
        let path = std::env::temp_dir().join(format!(
            "parchmint-anthropic-settings-{}-{}.json",
            std::process::id(),
            NEXT_REFERENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let service = PreferenceCoordinator::new(Arc::new(FilePreferenceStore::new(&path)));
        let platform = fixture_platform(Arc::new(SessionCredentialStore::default()));
        let saved = configure(
            &service,
            &platform,
            AssistantPreferences {
                provider: AssistantProvider::Anthropic,
                model: "claude-sonnet-4-6".into(),
                ..Default::default()
            },
            "test-anthropic-key",
            true,
            false,
        )
        .unwrap();
        let anthropic_binding = provider(&saved).unwrap().credential_binding();
        assert!(
            read_credential(&platform, &saved, &anthropic_binding)
                .unwrap()
                .is_some()
        );
        let other = AssistantPreferences {
            provider: AssistantProvider::OpenAi,
            ..saved.clone()
        };
        let other_binding = provider(&other).unwrap().credential_binding();
        assert_ne!(anthropic_binding, other_binding);
        assert!(
            read_credential(&platform, &saved, &other_binding)
                .unwrap()
                .is_none()
        );
        assert!(
            !std::fs::read_to_string(&path)
                .unwrap()
                .contains("test-anthropic-key")
        );
        std::fs::remove_file(path).unwrap();
    }
}
