use std::borrow::Cow;

use derive_more::{AsRef, Deref, From};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumString};
use url::Url;

use crate::{ApiKey, AuthCredential, AuthDetails, Model, Template};

/// Distinguishes between different categories of providers
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display, EnumString, Default,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum ProviderType {
    /// LLM providers for chat completions (default for backward compatibility)
    #[default]
    Llm,
    /// Context engine providers for code indexing and search
    ContextEngine,
}

/// --- IMPORTANT ---
/// The order of providers is important because that would be order in which the
/// providers will be resolved
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    JsonSchema,
    AsRef,
    Deref,
    Serialize,
    Deserialize,
)]
#[schemars(with = "String")]
#[serde(from = "String")]
pub struct ProviderId(Cow<'static, str>);

impl ProviderId {
    // Built-in provider constants
    pub const FORGE: ProviderId = ProviderId(Cow::Borrowed("forge"));
    pub const OPENAI_COMPATIBLE: ProviderId = ProviderId(Cow::Borrowed("openai_compatible"));
    pub const OPENAI_RESPONSES_COMPATIBLE: ProviderId =
        ProviderId(Cow::Borrowed("openai_responses_compatible"));
    pub const FORGE_SERVICES: ProviderId = ProviderId(Cow::Borrowed("forge_services"));
    pub const LLAMA_CPP: ProviderId = ProviderId(Cow::Borrowed("llama_cpp"));
    pub const VLLM: ProviderId = ProviderId(Cow::Borrowed("vllm"));
    pub const OLLAMA: ProviderId = ProviderId(Cow::Borrowed("ollama"));

    /// Returns all built-in provider IDs
    ///
    /// This includes all providers defined as constants in this implementation.
    pub fn built_in_providers() -> &'static [ProviderId] {
        &[
            ProviderId::FORGE,
            ProviderId::OPENAI_COMPATIBLE,
            ProviderId::OPENAI_RESPONSES_COMPATIBLE,
            ProviderId::FORGE_SERVICES,
            ProviderId::LLAMA_CPP,
            ProviderId::VLLM,
            ProviderId::OLLAMA,
        ]
    }

    /// Returns the display name for UI (UpperCamelCase with special handling
    /// for acronyms).
    ///
    /// This converts snake_case IDs to proper display names:
    /// - "openai_compatible" -> "OpenAICompatible"
    fn display_name(&self) -> String {
        // Special cases for known providers with acronyms
        match self.0.as_ref() {
            "openai_compatible" => "OpenAICompatible".to_string(),
            "openai_responses_compatible" => "OpenAIResponsesCompatible".to_string(),
            "forge_services" => "ForgeServices".to_string(),
            _ => {
                // For other providers, use UpperCamelCase conversion
                use convert_case::{Case, Casing};
                self.0.to_case(Case::UpperCamel)
            }
        }
    }
}

impl std::fmt::Display for ProviderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

impl std::str::FromStr for ProviderId {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Check if it's a built-in provider first
        let provider = match s {
            "forge" => ProviderId::FORGE,
            "openai_compatible" => ProviderId::OPENAI_COMPATIBLE,
            "openai_responses_compatible" => ProviderId::OPENAI_RESPONSES_COMPATIBLE,
            "forge_services" => ProviderId::FORGE_SERVICES,
            "llama_cpp" => ProviderId::LLAMA_CPP,
            "vllm" => ProviderId::VLLM,
            "ollama" => ProviderId::OLLAMA,
            // For custom providers, use Cow::Owned to avoid memory leaks
            custom => ProviderId(Cow::Owned(custom.to_string())),
        };
        Ok(provider)
    }
}

impl From<String> for ProviderId {
    fn from(s: String) -> Self {
        std::str::FromStr::from_str(&s).unwrap()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProviderResponse {
    /// OpenAI-compatible chat completions API
    OpenAI,
    /// OpenAI Responses API
    OpenAIResponses,
}

/// Represents the source of models for a provider
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ModelSource<T> {
    /// Can be a `Url` or a `Template`
    Url(T),
    Hardcoded(Vec<Model>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Provider<T> {
    pub id: ProviderId,
    #[serde(default)]
    pub provider_type: ProviderType,
    pub response: Option<ProviderResponse>,
    pub url: T,
    pub models: Option<ModelSource<T>>,
    pub auth_methods: Vec<crate::AuthMethod>,
    #[serde(default)]
    pub url_params: Vec<crate::URLParamSpec>,
    pub credential: Option<AuthCredential>,
    /// Custom HTTP headers to include in API requests for this provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_headers: Option<std::collections::HashMap<String, String>>,
}

/// Type alias for a provider with template URLs (not yet rendered)
pub type ProviderTemplate = Provider<Template<crate::URLParameters>>;

impl<T> Provider<T> {
    pub fn is_configured(&self) -> bool {
        self.credential.is_some()
    }
    pub fn models(&self) -> Option<&ModelSource<T>> {
        self.models.as_ref()
    }
}

impl Provider<Url> {
    pub fn url(&self) -> &Url {
        &self.url
    }

    pub fn api_key(&self) -> Option<&ApiKey> {
        self.credential
            .as_ref()
            .and_then(|c| match &c.auth_details {
                AuthDetails::ApiKey(key) => Some(key),
                _ => None,
            })
    }
}

/// Enum for viewing providers in listings where both configured and
/// unconfigured.
#[derive(Debug, Clone, PartialEq, From)]
pub enum AnyProvider {
    Url(Provider<Url>),
    Template(ProviderTemplate),
}

impl AnyProvider {
    /// Returns whether this provider is configured
    pub fn is_configured(&self) -> bool {
        match self {
            AnyProvider::Url(p) => p.is_configured(),
            AnyProvider::Template(p) => p.is_configured(),
        }
    }

    pub fn provider_type(&self) -> &ProviderType {
        match self {
            AnyProvider::Url(p) => &p.provider_type,
            AnyProvider::Template(t) => &t.provider_type,
        }
    }

    pub fn id(&self) -> ProviderId {
        match self {
            AnyProvider::Url(p) => p.id.clone(),
            AnyProvider::Template(p) => p.id.clone(),
        }
    }

    /// Gets the response type
    pub fn response(&self) -> Option<&ProviderResponse> {
        match self {
            AnyProvider::Url(p) => p.response.as_ref(),
            AnyProvider::Template(p) => p.response.as_ref(),
        }
    }

    /// Gets the URL for this provider.
    ///
    /// For configured providers, returns the resolved URL. For template
    /// providers with no URL parameters (i.e. a hardcoded default URL in
    /// provider.json), parses and returns the template string as a URL.
    /// Returns `None` for template providers that require user-supplied URL
    /// parameters.
    pub fn url(&self) -> Option<Url> {
        match self {
            AnyProvider::Url(p) => Some(p.url().clone()),
            AnyProvider::Template(t) if t.url_params.is_empty() => Url::parse(&t.url.template).ok(),
            AnyProvider::Template(_) => None,
        }
    }
    pub fn url_params(&self) -> &[crate::URLParamSpec] {
        match self {
            AnyProvider::Url(p) => &p.url_params,
            AnyProvider::Template(p) => &p.url_params,
        }
    }

    /// Gets the authentication methods supported by this provider
    pub fn auth_methods(&self) -> &[crate::AuthMethod] {
        match self {
            AnyProvider::Url(p) => &p.auth_methods,
            AnyProvider::Template(p) => &p.auth_methods,
        }
    }

    /// Consumes self and returns the configured provider if this is a URL
    /// provider with credentials
    pub fn into_configured(self) -> Option<Provider<Url>> {
        match self {
            AnyProvider::Url(p) if p.is_configured() => Some(p),
            _ => None,
        }
    }
}

/// Represents a provider with its available models
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderModels {
    /// The provider identifier
    pub provider_id: ProviderId,
    /// Available models from this provider
    pub models: Vec<Model>,
}

#[cfg(test)]
mod test_helpers {
    use std::collections::HashMap;

    use super::*;

    fn make_credential(provider_id: ProviderId, key: &str) -> Option<AuthCredential> {
        Some(AuthCredential {
            id: provider_id,
            auth_details: AuthDetails::ApiKey(ApiKey::from(key.to_string())),
            url_params: HashMap::new(),
        })
    }

    /// Test helper for creating an OpenAI-compatible provider
    pub(super) fn openai_compatible(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::OPENAI_COMPATIBLE,
            provider_type: Default::default(),
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse("https://api.openai.com/v1/chat/completions").unwrap(),
            auth_methods: vec![crate::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::OPENAI_COMPATIBLE, key),
            custom_headers: None,
            models: Some(ModelSource::Url(
                Url::parse("https://api.openai.com/v1/models").unwrap(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::str::FromStr;

    use pretty_assertions::assert_eq;

    use super::test_helpers::*;
    use super::*;

    #[test]
    fn test_provider_id_display_name() {
        assert_eq!(
            ProviderId::OPENAI_COMPATIBLE.to_string(),
            "OpenAICompatible"
        );
        assert_eq!(
            ProviderId::OPENAI_RESPONSES_COMPATIBLE.to_string(),
            "OpenAIResponsesCompatible"
        );
        assert_eq!(ProviderId::FORGE_SERVICES.to_string(), "ForgeServices");
        assert_eq!(ProviderId::LLAMA_CPP.to_string(), "LlamaCpp");
        assert_eq!(ProviderId::VLLM.to_string(), "Vllm");
        assert_eq!(ProviderId::OLLAMA.to_string(), "Ollama");
    }

    #[test]
    fn test_openai_compatible_from_str() {
        let actual = ProviderId::from_str("openai_compatible").unwrap();
        let expected = ProviderId::OPENAI_COMPATIBLE;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_llama_cpp_from_str() {
        let actual = ProviderId::from_str("llama_cpp").unwrap();
        let expected = ProviderId::LLAMA_CPP;
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_built_in_providers_contains_expected() {
        let built_in = ProviderId::built_in_providers();
        assert!(built_in.contains(&ProviderId::OPENAI_COMPATIBLE));
        assert!(built_in.contains(&ProviderId::OPENAI_RESPONSES_COMPATIBLE));
        assert!(built_in.contains(&ProviderId::LLAMA_CPP));
        assert!(built_in.contains(&ProviderId::OLLAMA));
    }

    #[test]
    fn test_openai_compatible_fixture() {
        let fixture = "test_key";
        let actual = openai_compatible(fixture);
        let expected = Provider {
            id: ProviderId::OPENAI_COMPATIBLE,
            provider_type: Default::default(),
            response: Some(ProviderResponse::OpenAI),
            url: Url::from_str("https://api.openai.com/v1/chat/completions").unwrap(),
            credential: Some(AuthCredential {
                id: ProviderId::OPENAI_COMPATIBLE,
                auth_details: AuthDetails::ApiKey(ApiKey::from(fixture.to_string())),
                url_params: HashMap::new(),
            }),
            auth_methods: vec![crate::AuthMethod::ApiKey],
            url_params: vec![],
            models: Some(ModelSource::Url(
                Url::from_str("https://api.openai.com/v1/models").unwrap(),
            )),
            custom_headers: None,
        };
        assert_eq!(actual, expected);
    }
}
