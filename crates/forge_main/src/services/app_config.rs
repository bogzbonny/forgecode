use std::sync::Arc;

use crate::app::{AppConfigService, EnvironmentInfra};
use crate::domain::{ConfigOperation, Effort, ModelConfig, ModelId, ProviderId, ProviderRepository};
use tracing::debug;

/// Service for managing user preferences for default providers and models.
///
/// All reads go through `infra.get_config()` so they always reflect the latest
/// on-disk state after any `update_environment` call.
pub struct ForgeAppConfigService<F> {
    infra: Arc<F>,
}

impl<F> ForgeAppConfigService<F> {
    /// Creates a new provider preferences service.
    pub fn new(infra: Arc<F>) -> Self {
        Self { infra }
    }
}

#[async_trait::async_trait]
impl<F: ProviderRepository + EnvironmentInfra<Config = crate::config::ForgeConfig> + Send + Sync>
    AppConfigService for ForgeAppConfigService<F>
{
    async fn get_session_config(&self) -> Option<ModelConfig> {
        let config = self.infra.get_config().ok()?;
        let session = config.session.as_ref()?;
        Some(ModelConfig {
            provider: ProviderId::from(session.provider_id.clone()),
            model: ModelId::new(session.model_id.clone()),
        })
    }

    async fn get_commit_config(&self) -> anyhow::Result<Option<crate::domain::ModelConfig>> {
        let config = self.infra.get_config()?;
        Ok(config.commit.clone().map(|mc| ModelConfig {
            provider: ProviderId::from(mc.provider_id),
            model: ModelId::new(mc.model_id),
        }))
    }

    async fn get_suggest_config(&self) -> anyhow::Result<Option<crate::domain::ModelConfig>> {
        let config = self.infra.get_config()?;
        Ok(config.suggest.clone().map(|mc| ModelConfig {
            provider: ProviderId::from(mc.provider_id),
            model: ModelId::new(mc.model_id),
        }))
    }

    async fn get_reasoning_effort(&self) -> anyhow::Result<Option<Effort>> {
        let config = self.infra.get_config()?;
        Ok(config
            .reasoning
            .clone()
            .and_then(|r| r.effort)
            .map(|e| match e {
                crate::config::Effort::None => Effort::None,
                crate::config::Effort::Minimal => Effort::Minimal,
                crate::config::Effort::Low => Effort::Low,
                crate::config::Effort::Medium => Effort::Medium,
                crate::config::Effort::High => Effort::High,
                crate::config::Effort::XHigh => Effort::XHigh,
                crate::config::Effort::Max => Effort::Max,
            }))
    }

    async fn update_config(&self, ops: Vec<ConfigOperation>) -> anyhow::Result<()> {
        debug!(ops = ?ops, "Updating app config");
        self.infra.update_environment(ops).await
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Mutex;

    use crate::config::{ForgeConfig, ModelConfig};
    // Alias to avoid collision with crate::config::ModelConfig used in test fixtures
    use crate::domain::ModelConfig as DomainModelConfig;
    use crate::domain::{
        AnyProvider, ChatRepository, ConfigOperation, Environment, InputModality, MigrationResult,
        Model, ModelId, ModelSource, Provider, ProviderId, ProviderResponse, ProviderTemplate,
    };
    use pretty_assertions::assert_eq;
    use url::Url;

    use super::*;

    #[derive(Clone)]
    struct MockInfra {
        config: Arc<Mutex<ForgeConfig>>,
        providers: Vec<Provider<Url>>,
    }

    impl MockInfra {
        fn new() -> Self {
            Self {
                config: Arc::new(Mutex::new(ForgeConfig::default())),
                providers: vec![
                    Provider {
                        id: ProviderId::OPENAI_COMPATIBLE,
                        provider_type: Default::default(),
                        response: Some(ProviderResponse::OpenAI),
                        url: Url::parse("https://api.openai.com").unwrap(),
                        credential: Some(crate::domain::AuthCredential {
                            id: ProviderId::OPENAI_COMPATIBLE,
                            auth_details: crate::domain::AuthDetails::ApiKey(
                                crate::domain::ApiKey::from("test-key".to_string()),
                            ),
                            url_params: HashMap::new(),
                        }),
                        auth_methods: vec![crate::domain::AuthMethod::ApiKey],
                        url_params: vec![],
                        models: Some(ModelSource::Hardcoded(vec![Model {
                            id: "gpt-4".to_string().into(),
                            name: Some("GPT-4".to_string()),
                            description: None,
                            context_length: Some(8192),
                            tools_supported: Some(true),
                            supports_parallel_tool_calls: Some(true),
                            supports_reasoning: Some(false),
                            input_modalities: vec![InputModality::Text],
                        }])),
                        custom_headers: None,
                    },
                    Provider {
                        id: ProviderId::OPENAI_COMPATIBLE,
                        provider_type: Default::default(),
                        response: Some(ProviderResponse::OpenAI),
                        url: Url::parse("https://api.openai.com").unwrap(),
                        auth_methods: vec![crate::domain::AuthMethod::ApiKey],
                        url_params: vec![],
                        credential: Some(crate::domain::AuthCredential {
                            id: ProviderId::OPENAI_COMPATIBLE,
                            auth_details: crate::domain::AuthDetails::ApiKey(
                                crate::domain::ApiKey::from("test-key".to_string()),
                            ),
                            url_params: HashMap::new(),
                        }),
                        models: Some(ModelSource::Hardcoded(vec![Model {
                            id: "gpt-4".to_string().into(),
                            name: Some("GPT-4".to_string()),
                            description: None,
                            context_length: Some(8192),
                            tools_supported: Some(true),
                            supports_parallel_tool_calls: Some(true),
                            supports_reasoning: Some(false),
                            input_modalities: vec![InputModality::Text],
                        }])),
                        custom_headers: None,
                    },
                ],
            }
        }
    }

    impl EnvironmentInfra for MockInfra {
        type Config = ForgeConfig;

        fn get_environment(&self) -> Environment {
            Environment {
                os: "test".to_string(),
                cwd: PathBuf::new(),
                home: None,
                shell: "bash".to_string(),
                base_path: PathBuf::new(),
            }
        }

        fn update_environment(
            &self,
            ops: Vec<ConfigOperation>,
        ) -> impl std::future::Future<Output = anyhow::Result<()>> + Send {
            let config = self.config.clone();
            async move {
                let mut config = config.lock().unwrap();
                for op in ops {
                    match op {
                        ConfigOperation::SetSessionConfig(mc) => {
                            let pid_str = mc.provider.as_ref().to_string();
                            let mid_str = mc.model.to_string();
                            config.session = Some(ModelConfig::new(pid_str, mid_str));
                        }
                        ConfigOperation::SetCommitConfig(mc) => {
                            config.commit = mc.map(|m| {
                                ModelConfig::new(
                                    m.provider.as_ref().to_string(),
                                    m.model.to_string(),
                                )
                            });
                        }
                        ConfigOperation::SetSuggestConfig(mc) => {
                            config.suggest = Some(ModelConfig::new(
                                mc.provider.as_ref().to_string(),
                                mc.model.to_string(),
                            ));
                        }
                        ConfigOperation::SetReasoningEffort(_) => {
                            // No-op in tests
                        }
                    }
                }
                Ok(())
            }
        }

        fn get_config(&self) -> anyhow::Result<ForgeConfig> {
            Ok(self.config.lock().unwrap().clone())
        }

        fn get_env_var(&self, _key: &str) -> Option<String> {
            None
        }

        fn get_env_vars(&self) -> std::collections::BTreeMap<String, String> {
            std::collections::BTreeMap::new()
        }
    }

    #[async_trait::async_trait]
    impl ChatRepository for MockInfra {
        async fn chat(
            &self,
            _model_id: &crate::app::domain::ModelId,
            _context: crate::app::domain::Context,
            _provider: Provider<Url>,
        ) -> crate::app::domain::ResultStream<crate::app::domain::ChatCompletionMessage, anyhow::Error>
        {
            Ok(Box::pin(tokio_stream::iter(vec![])))
        }

        async fn models(
            &self,
            _provider: Provider<Url>,
        ) -> anyhow::Result<Vec<crate::app::domain::Model>> {
            Ok(vec![])
        }
    }

    #[async_trait::async_trait]
    impl ProviderRepository for MockInfra {
        async fn get_all_providers(&self) -> anyhow::Result<Vec<AnyProvider>> {
            Ok(self
                .providers
                .iter()
                .map(|p| AnyProvider::Url(p.clone()))
                .collect())
        }

        async fn get_provider(&self, id: ProviderId) -> anyhow::Result<ProviderTemplate> {
            // Convert Provider<Url> to Provider<Template<...>> for testing
            self.providers
                .iter()
                .find(|p| p.id == id)
                .map(|p| Provider {
                    id: p.id.clone(),
                    provider_type: p.provider_type,
                    response: p.response.clone(),
                    url: crate::domain::Template::<crate::domain::URLParameters>::new(p.url.as_str()),
                    models: p.models.as_ref().map(|m| match m {
                        ModelSource::Url(url) => ModelSource::Url(crate::domain::Template::<
                            crate::domain::URLParameters,
                        >::new(
                            url.as_str()
                        )),
                        ModelSource::Hardcoded(list) => ModelSource::Hardcoded(list.clone()),
                    }),
                    auth_methods: p.auth_methods.clone(),
                    url_params: p.url_params.clone(),
                    credential: p.credential.clone(),
                    custom_headers: None,
                })
                .ok_or_else(|| anyhow::anyhow!("Provider not found"))
        }

        async fn upsert_credential(
            &self,
            _credential: crate::domain::AuthCredential,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        async fn get_credential(
            &self,
            _id: &ProviderId,
        ) -> anyhow::Result<Option<crate::domain::AuthCredential>> {
            Ok(None)
        }

        async fn remove_credential(&self, _id: &ProviderId) -> anyhow::Result<()> {
            Ok(())
        }

        async fn migrate_env_credentials(&self) -> anyhow::Result<Option<MigrationResult>> {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn test_get_session_config_when_none_set() -> anyhow::Result<()> {
        let fixture = MockInfra::new();
        let service = ForgeAppConfigService::new(Arc::new(fixture));

        let result = service.get_session_config().await;

        assert!(result.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_get_session_config_when_set() -> anyhow::Result<()> {
        let fixture = MockInfra::new();
        let service = ForgeAppConfigService::new(Arc::new(fixture.clone()));

        service
            .update_config(vec![ConfigOperation::SetSessionConfig(
                DomainModelConfig::new(ProviderId::OPENAI_COMPATIBLE, ModelId::new("gpt-4")),
            )])
            .await?;
        let actual = service.get_session_config().await;
        let expected = Some(DomainModelConfig::new(
            ProviderId::OPENAI_COMPATIBLE,
            ModelId::new("gpt-4"),
        ));

        assert_eq!(actual, expected);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_session_config_when_provider_not_available() -> anyhow::Result<()> {
        let mut fixture = MockInfra::new();
        // Remove OpenAI from available providers but keep it in config
        fixture.providers.retain(|p| p.id != ProviderId::OPENAI_COMPATIBLE);
        let service = ForgeAppConfigService::new(Arc::new(fixture.clone()));

        // Set OpenAI as the default provider in config (with a model)
        service
            .update_config(vec![ConfigOperation::SetSessionConfig(
                DomainModelConfig::new(ProviderId::OPENAI_COMPATIBLE, ModelId::new("gpt-4")),
            )])
            .await?;

        // Should return the config even if provider is not available
        // Validation happens when getting the actual provider via ProviderService
        let result = service.get_session_config().await;

        assert_eq!(
            result,
            Some(DomainModelConfig::new(
                ProviderId::OPENAI_COMPATIBLE,
                ModelId::new("gpt-4")
            ))
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_set_session_config() -> anyhow::Result<()> {
        let fixture = MockInfra::new();
        let service = ForgeAppConfigService::new(Arc::new(fixture.clone()));

        service
            .update_config(vec![ConfigOperation::SetSessionConfig(
                DomainModelConfig::new(ProviderId::OPENAI_COMPATIBLE, ModelId::new("gpt-4")),
            )])
            .await?;

        let actual = service.get_session_config().await;
        let expected = Some(DomainModelConfig::new(
            ProviderId::OPENAI_COMPATIBLE,
            ModelId::new("gpt-4"),
        ));

        assert_eq!(actual, expected);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_session_config_model_when_none_set() -> anyhow::Result<()> {
        let fixture = MockInfra::new();
        let service = ForgeAppConfigService::new(Arc::new(fixture));

        let result = service.get_session_config().await;

        assert!(result.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn test_get_session_config_model_when_set() -> anyhow::Result<()> {
        let fixture = MockInfra::new();
        let service = ForgeAppConfigService::new(Arc::new(fixture.clone()));

        service
            .update_config(vec![ConfigOperation::SetSessionConfig(
                DomainModelConfig::new(ProviderId::OPENAI_COMPATIBLE, ModelId::new("gpt-4")),
            )])
            .await?;
        let actual = service.get_session_config().await.map(|c| c.model);
        let expected = Some(ModelId::new("gpt-4"));

        assert_eq!(actual, expected);
        Ok(())
    }

    #[tokio::test]
    async fn test_set_session_config_model() -> anyhow::Result<()> {
        let fixture = MockInfra::new();
        let service = ForgeAppConfigService::new(Arc::new(fixture.clone()));

        service
            .update_config(vec![ConfigOperation::SetSessionConfig(
                DomainModelConfig::new(ProviderId::OPENAI_COMPATIBLE, ModelId::from("gpt-4".to_string())),
            )])
            .await?;

        let actual = service.get_session_config().await.map(|c| c.model);
        let expected = Some(ModelId::from("gpt-4".to_string()));

        assert_eq!(actual, expected);
        Ok(())
    }

    #[tokio::test]
    async fn test_set_multiple_default_models() -> anyhow::Result<()> {
        let fixture = MockInfra::new();
        let service = ForgeAppConfigService::new(Arc::new(fixture.clone()));

        // Set model for OpenAI first
        service
            .update_config(vec![ConfigOperation::SetSessionConfig(
                DomainModelConfig::new(ProviderId::OPENAI_COMPATIBLE, ModelId::from("gpt-4".to_string())),
            )])
            .await?;

        // Then switch to OpenAI with its model
        service
            .update_config(vec![ConfigOperation::SetSessionConfig(
                DomainModelConfig::new(
                    ProviderId::OPENAI_COMPATIBLE,
                    ModelId::from("gpt-4".to_string()),
                ),
            )])
            .await?;

        // ForgeConfig only tracks a single active session, so the last
        // provider/model pair wins
        let actual = service.get_session_config().await;
        let expected = Some(DomainModelConfig::new(
            ProviderId::OPENAI_COMPATIBLE,
            ModelId::new("gpt-4"),
        ));

        assert_eq!(actual, expected);
        Ok(())
    }
}
