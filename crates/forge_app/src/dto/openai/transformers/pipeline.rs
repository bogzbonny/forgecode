use forge_domain::{DefaultTransformation, Provider, ProviderId, Transformer};
use url::Url;

use super::drop_tool_call::DropToolCalls;
use super::github_copilot_reasoning::GitHubCopilotReasoning;
use super::kimi_k2_reasoning::KimiK2Reasoning;
use super::make_openai_compat::MakeOpenAiCompat;
use super::minimax::SetMinimaxParams;
use super::normalize_tool_schema::{
    EnforceStrictResponseFormatSchema, EnforceStrictToolSchema, NormalizeToolSchema,
};
use super::set_cache::SetCache;
use super::set_reasoning_effort::SetReasoningEffort;
use super::strip_thought_signature::StripThoughtSignature;
use super::tool_choice::SetToolChoice;
use super::trim_tool_call_ids::TrimToolCallIds;
use super::when_model::when_model;
use crate::dto::openai::{Request, ToolChoice};

/// Pipeline for transforming requests based on the provider type
pub struct ProviderPipeline<'a>(&'a Provider<Url>);

impl<'a> ProviderPipeline<'a> {
    /// Creates a new provider pipeline for the given provider
    pub fn new(provider: &'a Provider<Url>) -> Self {
        Self(provider)
    }
}

impl Transformer for ProviderPipeline<'_> {
    type Value = Request;

    fn transform(&mut self, request: Self::Value) -> Self::Value {
        // Only Anthropic and Gemini requires cache configuration to be set.
        // ref: https://openrouter.ai/docs/features/prompt-caching
        let provider = self.0;

        let or_transformers = DefaultTransformation::<Request>::new()
            .pipe(SetMinimaxParams.when(when_model("minimax")))
            .pipe(DropToolCalls.when(when_model("mistral")))
            .pipe(SetToolChoice::new(ToolChoice::Auto).when(when_model("gemini")))
            .pipe(SetCache.when(when_model("gemini|anthropic|minimax")))
            .when(move |_| supports_open_router_params(provider));

        // Strip thought signatures for all models except gemini-3
        let strip_thought_signature =
            StripThoughtSignature.when(move |req: &Request| !is_gemini3_model(req));

        let open_ai_compat = MakeOpenAiCompat.when(move |_| !supports_open_router_params(provider));

        let set_reasoning_effort =
            SetReasoningEffort.when(move |_| provider.id == ProviderId::GITHUB_COPILOT);

        let github_copilot_reasoning =
            GitHubCopilotReasoning.when(move |_| provider.id == ProviderId::GITHUB_COPILOT);

        let kimi_k2_reasoning =
            KimiK2Reasoning.when(move |request: &Request| when_model("kimi")(request));

        let trim_tool_call_ids = TrimToolCallIds.when(move |_| provider.id == ProviderId::OPENAI);

        let strict_schema = EnforceStrictToolSchema
            .pipe(EnforceStrictResponseFormatSchema)
            .when(move |_| false);

        let mut combined = or_transformers
            .pipe(strip_thought_signature)
            .pipe(set_reasoning_effort)
            .pipe(open_ai_compat)
            .pipe(github_copilot_reasoning)
            .pipe(kimi_k2_reasoning)
            .pipe(trim_tool_call_ids)
            .pipe(strict_schema)
            .pipe(NormalizeToolSchema);
        combined.transform(request)
    }
}

/// Checks if the request model is a gemini-3 model (which supports thought
/// signatures)
fn is_gemini3_model(req: &Request) -> bool {
    req.model
        .as_ref()
        .map(|m| m.as_str().contains("gemini-3"))
        .unwrap_or(false)
}

/// function checks if provider supports open-router parameters.
fn supports_open_router_params(provider: &Provider<Url>) -> bool {
    provider.id == ProviderId::FORGE
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use forge_domain::ModelId;
    use url::Url;

    use super::*;
    use crate::domain::{ModelSource, ProviderResponse};

    // Test helper functions
    fn make_credential(provider_id: ProviderId, key: &str) -> Option<forge_domain::AuthCredential> {
        Some(forge_domain::AuthCredential {
            id: provider_id,
            auth_details: forge_domain::AuthDetails::ApiKey(forge_domain::ApiKey::from(
                key.to_string(),
            )),
            url_params: HashMap::new(),
        })
    }

    fn forge(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::FORGE,
            provider_type: Default::default(),
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse("https://antinomy.ai/api/v1/chat/completions").unwrap(),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::FORGE, key),
            custom_headers: None,
            models: Some(ModelSource::Url(
                Url::parse("https://antinomy.ai/api/v1/models").unwrap(),
            )),
        }
    }

    fn openai(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::OPENAI,
            provider_type: Default::default(),
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse("https://api.openai.com/v1/chat/completions").unwrap(),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::OPENAI, key),
            custom_headers: None,
            models: Some(ModelSource::Url(
                Url::parse("https://api.openai.com/v1/models").unwrap(),
            )),
        }
    }

    #[test]
    fn test_openai_provider_trims_tool_call_ids() {
        let provider = openai("openai");
        let long_id = "call_12345678901234567890123456789012345678901234567890";

        let fixture = Request::default().messages(vec![crate::dto::openai::Message {
            role: crate::dto::openai::Role::Tool,
            content: None,
            name: None,
            tool_call_id: Some(forge_domain::ToolCallId::new(long_id)),
            tool_calls: None,
            reasoning_details: None,
            reasoning_text: None,
            reasoning_opaque: None,
            reasoning_content: None,
            extra_content: None,
        }]);

        let mut pipeline = ProviderPipeline::new(&provider);
        let actual = pipeline.transform(fixture);

        let expected_id = "call_12345678901234567890123456789012345";
        assert_eq!(expected_id.len(), 40);

        let messages = actual.messages.unwrap();
        assert_eq!(
            messages[0].tool_call_id.as_ref().unwrap().as_str(),
            expected_id
        );
    }

    #[test]
    fn test_non_openai_provider_does_not_trim_tool_call_ids() {
        let provider = Provider {
            id: ProviderId::FORGE_SERVICES,
            provider_type: Default::default(),
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse("https://api.forge.com/v1/chat/completions").unwrap(),
            auth_methods: vec![forge_domain::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::FORGE_SERVICES, "test-key"),
            custom_headers: None,
            models: Some(ModelSource::Url(
                Url::parse("https://api.forge.com/v1/models").unwrap(),
            )),
        };
        let long_id = "call_12345678901234567890123456789012345678901234567890";

        let fixture = Request::default().messages(vec![crate::dto::openai::Message {
            role: crate::dto::openai::Role::Tool,
            content: None,
            name: None,
            tool_call_id: Some(forge_domain::ToolCallId::new(long_id)),
            tool_calls: None,
            reasoning_details: None,
            reasoning_text: None,
            reasoning_opaque: None,
            reasoning_content: None,
            extra_content: None,
        }]);

        let mut pipeline = ProviderPipeline::new(&provider);
        let actual = pipeline.transform(fixture);

        // Non-OpenAI provider should not trim tool call IDs
        let messages = actual.messages.unwrap();
        assert_eq!(messages[0].tool_call_id.as_ref().unwrap().as_str(), long_id);
    }

    #[test]
    fn test_gemini3_model_preserves_thought_signature() {
        use crate::dto::openai::{ExtraContent, GoogleMetadata, Message, MessageContent, Role};

        let provider = forge("forge");
        let fixture = Request::default()
            .model(ModelId::new("google/gemini-3-pro-preview"))
            .messages(vec![Message {
                role: Role::Assistant,
                content: Some(MessageContent::Text("Hello".to_string())),
                name: None,
                tool_call_id: None,
                tool_calls: None,
                reasoning_details: None,
                reasoning_text: None,
                reasoning_opaque: None,
                reasoning_content: None,
                extra_content: Some(ExtraContent {
                    google: Some(GoogleMetadata { thought_signature: Some("sig123".to_string()) }),
                }),
            }]);

        let mut pipeline = ProviderPipeline::new(&provider);
        let actual = pipeline.transform(fixture);

        // Thought signature should be preserved for gemini-3 models
        let messages = actual.messages.unwrap();
        assert!(messages[0].extra_content.is_some());
        assert_eq!(
            messages[0]
                .extra_content
                .as_ref()
                .unwrap()
                .google
                .as_ref()
                .unwrap()
                .thought_signature,
            Some("sig123".to_string())
        );
    }

    #[test]
    fn test_non_gemini3_model_strips_thought_signature() {
        use crate::dto::openai::{ExtraContent, GoogleMetadata, Message, MessageContent, Role};

        let provider = forge("forge");
        let fixture = Request::default()
            .model(ModelId::new("anthropic/claude-sonnet-4"))
            .messages(vec![Message {
                role: Role::Assistant,
                content: Some(MessageContent::Text("Hello".to_string())),
                name: None,
                tool_call_id: None,
                tool_calls: None,
                reasoning_details: None,
                reasoning_text: None,
                reasoning_opaque: None,
                reasoning_content: None,
                extra_content: Some(ExtraContent {
                    google: Some(GoogleMetadata { thought_signature: Some("sig123".to_string()) }),
                }),
            }]);

        let mut pipeline = ProviderPipeline::new(&provider);
        let actual = pipeline.transform(fixture);

        // Thought signature should be stripped for non-gemini-3 models
        let messages = actual.messages.unwrap();
        assert!(messages[0].extra_content.is_none());
    }

    #[test]
    fn test_minimax_model_applies_cache_via_forge() {
        use crate::dto::openai::{Message, MessageContent, Role};

        let provider = forge("forge");
        let fixture = Request::default()
            .model(ModelId::new("minimax/minimax-m2"))
            .messages(vec![
                Message {
                    role: Role::User,
                    content: Some(MessageContent::Text("Hello".to_string())),
                    name: None,
                    tool_call_id: None,
                    tool_calls: None,
                    reasoning_details: None,
                    reasoning_text: None,
                    reasoning_opaque: None,
                    reasoning_content: None,
                    extra_content: None,
                },
                Message {
                    role: Role::Assistant,
                    content: Some(MessageContent::Text("Hi there".to_string())),
                    name: None,
                    tool_call_id: None,
                    tool_calls: None,
                    reasoning_details: None,
                    reasoning_text: None,
                    reasoning_opaque: None,
                    reasoning_content: None,
                    extra_content: None,
                },
            ]);

        let mut pipeline = ProviderPipeline::new(&provider);
        let actual = pipeline.transform(fixture);

        // Cache should be applied: first and last messages cached
        let messages = actual.messages.unwrap();
        assert!(messages
            .first()
            .unwrap()
            .content
            .as_ref()
            .unwrap()
            .is_cached());
        assert!(messages
            .last()
            .unwrap()
            .content
            .as_ref()
            .unwrap()
            .is_cached());
    }

    #[test]
    fn test_non_minimax_model_does_not_apply_cache_via_forge() {
        use crate::dto::openai::{Message, MessageContent, Role};

        let provider = forge("forge");
        let fixture = Request::default()
            .model(ModelId::new("openai/gpt-4o"))
            .messages(vec![
                Message {
                    role: Role::User,
                    content: Some(MessageContent::Text("Hello".to_string())),
                    name: None,
                    tool_call_id: None,
                    tool_calls: None,
                    reasoning_details: None,
                    reasoning_text: None,
                    reasoning_opaque: None,
                    reasoning_content: None,
                    extra_content: None,
                },
                Message {
                    role: Role::Assistant,
                    content: Some(MessageContent::Text("Hi there".to_string())),
                    name: None,
                    tool_call_id: None,
                    tool_calls: None,
                    reasoning_details: None,
                    reasoning_text: None,
                    reasoning_opaque: None,
                    reasoning_content: None,
                    extra_content: None,
                },
            ]);

        let mut pipeline = ProviderPipeline::new(&provider);
        let actual = pipeline.transform(fixture);

        // Cache should NOT be applied for non-minimax/gemini/anthropic models
        let messages = actual.messages.unwrap();
        assert!(!messages
            .first()
            .unwrap()
            .content
            .as_ref()
            .unwrap()
            .is_cached());
        assert!(!messages
            .last()
            .unwrap()
            .content
            .as_ref()
            .unwrap()
            .is_cached());
    }

    #[test]
    fn test_gemini2_model_strips_thought_signature() {
        use crate::dto::openai::{ExtraContent, GoogleMetadata, Message, MessageContent, Role};

        let provider = forge("forge");
        let fixture = Request::default()
            .model(ModelId::new("google/gemini-2.5-pro"))
            .messages(vec![Message {
                role: Role::Assistant,
                content: Some(MessageContent::Text("Hello".to_string())),
                name: None,
                tool_call_id: None,
                tool_calls: None,
                reasoning_details: None,
                reasoning_text: None,
                reasoning_opaque: None,
                reasoning_content: None,
                extra_content: Some(ExtraContent {
                    google: Some(GoogleMetadata { thought_signature: Some("sig123".to_string()) }),
                }),
            }]);

        let mut pipeline = ProviderPipeline::new(&provider);
        let actual = pipeline.transform(fixture);

        // Thought signature should be stripped for gemini-2 models (not gemini-3)
        let messages = actual.messages.unwrap();
        assert!(messages[0].extra_content.is_none());
    }

    #[test]
    fn test_openai_provider_does_not_enforce_strict_tool_schema() {
        let provider = openai("openai");
        let fixture = Request::default().tools(vec![crate::dto::openai::Tool {
            r#type: crate::dto::openai::FunctionType,
            function: crate::dto::openai::FunctionDescription {
                name: "fs_search".to_string(),
                description: Some("Search files".to_string()),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "output_mode": {
                            "description": "Output mode",
                            "nullable": true,
                            "type": "string",
                            "enum": ["content", "files_with_matches", "count", null]
                        }
                    }
                }),
            },
        }]);

        let mut pipeline = ProviderPipeline::new(&provider);
        let actual = pipeline.transform(fixture);

        let expected = serde_json::json!({
            "type": "object",
            "properties": {
                "output_mode": {
                    "description": "Output mode",
                    "nullable": true,
                    "type": "string",
                    "enum": ["content", "files_with_matches", "count", null]
                }
            }
        });

        assert_eq!(actual.tools.unwrap()[0].function.parameters, expected);
    }
}
