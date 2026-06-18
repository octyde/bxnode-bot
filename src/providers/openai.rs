//! OpenAI provider implementation

use async_trait::async_trait;
use futures_util::stream::BoxStream;

use super::openai_compatible::{OpenAICompatibleConfig, OpenAICompatibleProvider};
use super::{CompletionRequest, CompletionResponse, ModelInfo, Provider, StreamEvent};

/// OpenAI API configuration
#[derive(Debug, Clone)]
pub struct OpenAIConfig {
    pub api_key: String,
    pub base_url: String,
    pub organization: Option<String>,
}

impl Default for OpenAIConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.openai.com".to_string(),
            organization: None,
        }
    }
}

/// OpenAI provider (wraps the generic OpenAI-compatible provider)
pub struct OpenAIProvider(OpenAICompatibleProvider);

impl OpenAIProvider {
    pub fn new(config: OpenAIConfig) -> Self {
        let mut extra_headers = Vec::new();
        if let Some(org) = config.organization {
            extra_headers.push(("OpenAI-Organization".to_string(), org));
        }

        Self(OpenAICompatibleProvider::new(OpenAICompatibleConfig {
            provider_id: "openai".to_string(),
            api_key: config.api_key,
            base_url: config.base_url,
            models: openai_models(),
            completions_path: Some("/v1/chat/completions".to_string()),
            extra_headers,
            extra_body: vec![],
        }))
    }
}

fn openai_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: "gpt-4o".to_string(),
            name: "GPT-4o".to_string(),
            context_length: 128000,
            capabilities: vec![
                "chat".to_string(),
                "vision".to_string(),
                "tools".to_string(),
            ],
        },
        ModelInfo {
            id: "gpt-4o-mini".to_string(),
            name: "GPT-4o Mini".to_string(),
            context_length: 128000,
            capabilities: vec![
                "chat".to_string(),
                "vision".to_string(),
                "tools".to_string(),
            ],
        },
        ModelInfo {
            id: "gpt-4-turbo".to_string(),
            name: "GPT-4 Turbo".to_string(),
            context_length: 128000,
            capabilities: vec![
                "chat".to_string(),
                "vision".to_string(),
                "tools".to_string(),
            ],
        },
        ModelInfo {
            id: "gpt-4".to_string(),
            name: "GPT-4".to_string(),
            context_length: 8192,
            capabilities: vec!["chat".to_string(), "tools".to_string()],
        },
        ModelInfo {
            id: "gpt-3.5-turbo".to_string(),
            name: "GPT-3.5 Turbo".to_string(),
            context_length: 16385,
            capabilities: vec!["chat".to_string(), "tools".to_string()],
        },
    ]
}

#[async_trait]
impl Provider for OpenAIProvider {
    fn id(&self) -> &str {
        self.0.id()
    }

    fn models(&self) -> Vec<ModelInfo> {
        self.0.models()
    }

    async fn complete(&self, request: CompletionRequest) -> anyhow::Result<CompletionResponse> {
        self.0.complete(request).await
    }

    async fn complete_stream(
        &self,
        request: CompletionRequest,
    ) -> anyhow::Result<BoxStream<'static, anyhow::Result<StreamEvent>>> {
        self.0.complete_stream(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = OpenAIConfig::default();
        assert_eq!(config.base_url, "https://api.openai.com");
        assert!(config.api_key.is_empty());
        assert!(config.organization.is_none());
    }

    #[test]
    fn test_provider_id() {
        let config = OpenAIConfig::default();
        let provider = OpenAIProvider::new(config);
        assert_eq!(provider.id(), "openai");
    }

    #[test]
    fn test_models_list() {
        let config = OpenAIConfig::default();
        let provider = OpenAIProvider::new(config);
        let models = provider.models();

        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.id.contains("gpt-4")));
    }
}
