use std::collections::HashMap;
use std::sync::Arc;

use crate::{
    ApiKey, GeminiProvider, GenerationRequest, GenerationResult, OpenAiProvider, Provider,
    ProviderCapabilities, ProviderId, Result, SeedreamProvider,
};

#[derive(Clone)]
pub struct ProviderRegistry {
    providers: Arc<HashMap<ProviderId, Arc<dyn Provider>>>,
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new([
            Arc::new(SeedreamProvider::default()) as Arc<dyn Provider>,
            Arc::new(GeminiProvider::default()) as Arc<dyn Provider>,
            Arc::new(OpenAiProvider::default()) as Arc<dyn Provider>,
        ])
    }
}

impl ProviderRegistry {
    pub fn new(providers: impl IntoIterator<Item = Arc<dyn Provider>>) -> Self {
        let providers = providers
            .into_iter()
            .map(|provider| (provider.id(), provider))
            .collect();
        Self {
            providers: Arc::new(providers),
        }
    }

    pub fn capabilities(&self, provider: ProviderId) -> Option<ProviderCapabilities> {
        self.providers
            .get(&provider)
            .map(|provider| provider.capabilities())
    }

    pub fn generate(
        &self,
        provider: ProviderId,
        request: &GenerationRequest,
        api_key: &ApiKey,
    ) -> Result<GenerationResult> {
        self.providers
            .get(&provider)
            .ok_or(crate::AiError::ProviderUnavailable)?
            .generate(request, api_key)
    }
}
