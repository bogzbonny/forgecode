use crate::domain::{DefaultTransformation, Provider, ProviderId, Transformer};
use url::Url;

use super::kimi_k2_reasoning::KimiK2Reasoning;
use super::make_openai_compat::MakeOpenAiCompat;
use super::minimax::SetMinimaxParams;
use super::normalize_tool_schema::{
    EnforceStrictResponseFormatSchema, EnforceStrictToolSchema, NormalizeToolSchema,
};
use super::set_cache::SetCache;
use super::trim_tool_call_ids::TrimToolCallIds;
use super::when_model::when_model;
use crate::app::dto::openai::Request;

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
        // Only Minimax requires cache configuration to be set.
        let provider = self.0;

        let or_transformers = DefaultTransformation::<Request>::new()
            .pipe(SetMinimaxParams.when(when_model("minimax")))
            .pipe(SetCache.when(when_model("minimax")))
            .when(move |_| supports_open_router_params(provider));

        let open_ai_compat = MakeOpenAiCompat.when(move |_| !supports_open_router_params(provider));

        let kimi_k2_reasoning =
            KimiK2Reasoning.when(move |request: &Request| when_model("kimi")(request));

        // TrimToolCallIds is no longer needed since we removed the direct OpenAI provider
        let trim_tool_call_ids = TrimToolCallIds.when(move |_| false);

        let strict_schema = EnforceStrictToolSchema
            .pipe(EnforceStrictResponseFormatSchema)
            .when(move |_| false);

        let mut combined = or_transformers
            .pipe(open_ai_compat)
            .pipe(kimi_k2_reasoning)
            .pipe(trim_tool_call_ids)
            .pipe(strict_schema)
            .pipe(NormalizeToolSchema);
        combined.transform(request)
    }
}

/// function checks if provider supports open-router parameters.
fn supports_open_router_params(provider: &Provider<Url>) -> bool {
    provider.id == ProviderId::FORGE
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::domain::ModelId;
    use url::Url;

    use super::*;
    use crate::domain::{ModelSource, ProviderResponse};

    // Test helper functions
    fn make_credential(provider_id: ProviderId, key: &str) -> Option<crate::domain::AuthCredential> {
        Some(crate::domain::AuthCredential {
            id: provider_id,
            auth_details: crate::domain::AuthDetails::ApiKey(crate::domain::ApiKey::from(
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
            auth_methods: vec![crate::domain::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::FORGE, key),
            custom_headers: None,
            models: Some(ModelSource::Url(
                Url::parse("https://antinomy.ai/api/v1/models").unwrap(),
            )),
        }
    }

    fn openai_compatible(key: &str) -> Provider<Url> {
        Provider {
            id: ProviderId::OPENAI_COMPATIBLE,
            provider_type: Default::default(),
            response: Some(ProviderResponse::OpenAI),
            url: Url::parse("https://api.openai.com/v1/chat/completions").unwrap(),
            auth_methods: vec![crate::domain::AuthMethod::ApiKey],
            url_params: vec![],
            credential: make_credential(ProviderId::OPENAI_COMPATIBLE, key),
            custom_headers: None,
            models: Some(ModelSource::Url(
                Url::parse("https://api.openai.com/v1/models").unwrap(),
            )),
        }
    }

    #[test]
    fn test_openai_compatible_provider_does_not_trim_tool_call_ids() {
        let provider = openai_compatible("openai");
        let long_id = "call_12345678901234567890123456789012345678901234567890";

        let fixture = Request::default().messages(vec![crate::app::dto::openai::Message {
            role: crate::app::dto::openai::Role::Tool,
            content: None,
            name: None,
            tool_call_id: Some(crate::domain::ToolCallId::new(long_id)),
            tool_calls: None,
            reasoning_details: None,
            reasoning_text: None,
            reasoning_opaque: None,
            reasoning_content: None,
            extra_content: None,
        }]);

        let mut pipeline = ProviderPipeline::new(&provider);
        let actual = pipeline.transform(fixture);

        // OpenAI-compatible provider should not trim tool call IDs
        let messages = actual.messages.unwrap();
        assert_eq!(messages[0].tool_call_id.as_ref().unwrap().as_str(), long_id);
    }

    #[test]
    fn test_minimax_model_applies_cache_via_forge() {
        use crate::app::dto::openai::{Message, MessageContent, Role};

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
        use crate::app::dto::openai::{Message, MessageContent, Role};

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

        // Cache should NOT be applied for non-minimax models
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
    fn test_openai_compatible_provider_does_not_enforce_strict_tool_schema() {
        let provider = openai_compatible("openai");
        let fixture = Request::default().tools(vec![crate::app::dto::openai::Tool {
            r#type: crate::app::dto::openai::FunctionType,
            function: crate::app::dto::openai::FunctionDescription {
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
