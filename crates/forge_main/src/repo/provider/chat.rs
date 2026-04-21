use std::sync::Arc;

use crate::app::domain::{
    ChatCompletionMessage, Context, Model, ModelId, ProviderResponse, ResultStream,
};
use crate::app::{EnvironmentInfra, HttpInfra};
use crate::domain::{ChatRepository, Provider};
use crate::infra::CacacheStorage;
use tokio::task::AbortHandle;
use url::Url;

use super::openai::OpenAIResponseRepository;
use super::openai_responses::OpenAIResponsesResponseRepository;

/// Repository responsible for routing chat requests to the appropriate provider
/// implementation based on the provider's response type.
pub struct ForgeChatRepository<F> {
    router: Arc<ProviderRouter<F>>,
    model_cache: Arc<CacacheStorage>,
    bg_refresh: BgRefresh,
}

impl<F: EnvironmentInfra<Config = crate::config::ForgeConfig> + HttpInfra> ForgeChatRepository<F> {
    /// Creates a new ForgeChatRepository with the given infrastructure.
    ///
    /// # Arguments
    ///
    /// * `infra` - Infrastructure providing environment and HTTP capabilities
    pub fn new(infra: Arc<F>) -> Self {
        let env = infra.get_environment();
        let config = infra.get_config().unwrap_or_default();
        let model_cache_ttl_secs = config.model_cache_ttl_secs;

        let openai_repo = OpenAIResponseRepository::new(infra.clone());
        let openai_responses_repo = OpenAIResponsesResponseRepository::new(infra.clone());

        let model_cache = Arc::new(CacacheStorage::new(
            env.cache_dir().join("model_cache"),
            Some(model_cache_ttl_secs as u128),
        ));

        Self {
            router: Arc::new(ProviderRouter {
                openai_repo,
                openai_responses_repo,
            }),
            model_cache,
            bg_refresh: BgRefresh::default(),
        }
    }
}

#[async_trait::async_trait]
impl<F: EnvironmentInfra<Config = crate::config::ForgeConfig> + HttpInfra + Sync> ChatRepository
    for ForgeChatRepository<F>
{
    async fn chat(
        &self,
        model_id: &ModelId,
        context: Context,
        provider: Provider<Url>,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        self.router.chat(model_id, context, provider).await
    }

    async fn models(&self, provider: Provider<Url>) -> anyhow::Result<Vec<Model>> {
        use crate::app::KVStore;

        let cache_key = format!("models:{}", provider.id);

        if let Ok(Some(cached)) = self
            .model_cache
            .cache_get::<_, Vec<Model>>(&cache_key)
            .await
        {
            tracing::debug!(provider_id = %provider.id, "returning cached models; refreshing in background");

            // Spawn a background task to refresh the disk cache. The abort
            // handle is stored so the task is cancelled if the service is dropped.
            let cache = self.model_cache.clone();
            let router = self.router.clone();
            let key = cache_key;
            let handle = tokio::spawn(async move {
                match router.models(provider).await {
                    Ok(models) => {
                        if let Err(err) = cache.cache_set(&key, &models).await {
                            tracing::warn!(error = %err, "background refresh: failed to cache model list");
                        }
                    }
                    Err(err) => {
                        tracing::warn!(error = %err, "background refresh: failed to fetch models");
                    }
                }
            });
            self.bg_refresh.register(handle.abort_handle());

            return Ok(cached);
        }

        let models = self.router.models(provider).await?;

        if let Err(err) = self.model_cache.cache_set(&cache_key, &models).await {
            tracing::warn!(error = %err, "failed to cache model list");
        }

        Ok(models)
    }
}

/// Routes chat and model requests to the correct provider backend.
struct ProviderRouter<F> {
    openai_repo: OpenAIResponseRepository<F>,
    openai_responses_repo: OpenAIResponsesResponseRepository<F>,
}

impl<F: HttpInfra + EnvironmentInfra<Config = crate::config::ForgeConfig> + Sync> ProviderRouter<F> {
    async fn chat(
        &self,
        model_id: &ModelId,
        context: Context,
        provider: Provider<Url>,
    ) -> ResultStream<ChatCompletionMessage, anyhow::Error> {
        match provider.response {
            Some(ProviderResponse::OpenAI) => self.openai_repo.chat(model_id, context, provider).await,
            Some(ProviderResponse::OpenAIResponses) => {
                self.openai_responses_repo.chat(model_id, context, provider).await
            }
            None => Err(anyhow::anyhow!(
                "Provider response type not configured for provider: {}",
                provider.id
            )),
        }
    }

    async fn models(&self, provider: Provider<Url>) -> anyhow::Result<Vec<Model>> {
        match provider.response {
            Some(ProviderResponse::OpenAI) => self.openai_repo.models(provider).await,
            Some(ProviderResponse::OpenAIResponses) => self.openai_responses_repo.models(provider).await,
            None => Err(anyhow::anyhow!(
                "Provider response type not configured for provider: {}",
                provider.id
            )),
        }
    }
}

/// Tracks abort handles for background tasks and cancels them on drop.
#[derive(Default)]
struct BgRefresh(std::sync::Mutex<Vec<AbortHandle>>);

impl BgRefresh {
    /// Registers an abort handle to be cancelled when this guard is dropped.
    fn register(&self, handle: AbortHandle) {
        if let Ok(mut handles) = self.0.lock() {
            handles.push(handle);
        }
    }
}

impl Drop for BgRefresh {
    fn drop(&mut self) {
        if let Ok(mut handles) = self.0.lock() {
            for handle in handles.drain(..) {
                handle.abort();
            }
        }
    }
}
