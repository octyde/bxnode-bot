//! Provider registry for managing LLM providers

use std::collections::HashMap;
use std::sync::Arc;

use super::{
    anthropic::{AnthropicConfig, AnthropicProvider},
    ollama::{OllamaConfig, OllamaProvider},
    openai::{OpenAIConfig, OpenAIProvider},
    openai_compatible::{
        deepseek_models, gemini_models, groq_models, mistral_models, qwen_models, venice_models,
        zai_models, OpenAICompatibleConfig, OpenAICompatibleProvider,
    },
    ModelInfo, Provider,
};
use crate::config::Config;

/// Provider registry for managing multiple LLM providers
pub struct ProviderRegistry {
    providers: HashMap<String, Arc<dyn Provider>>,
    model_to_provider: HashMap<String, String>,
}

impl ProviderRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            model_to_provider: HashMap::new(),
        }
    }

    /// Create a registry from configuration
    pub fn from_config(config: &Config) -> Self {
        let mut registry = Self::new();

        // Register Anthropic if configured
        if let Some(ref anthropic_config) = config.providers.anthropic {
            let provider_config = AnthropicConfig {
                api_key: anthropic_config.api_key.clone(),
                base_url: anthropic_config
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "https://api.anthropic.com".to_string()),
            };
            registry.register(Arc::new(AnthropicProvider::new(provider_config)));
        }

        // Register OpenAI if configured
        if let Some(ref openai_config) = config.providers.openai {
            let provider_config = OpenAIConfig {
                api_key: openai_config.api_key.clone(),
                base_url: openai_config
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "https://api.openai.com".to_string()),
                organization: openai_config.organization.clone(),
            };
            registry.register(Arc::new(OpenAIProvider::new(provider_config)));
        }

        // Register Ollama if configured
        if let Some(ref ollama_config) = config.providers.ollama {
            let provider_config = OllamaConfig {
                base_url: ollama_config.base_url.clone(),
            };
            registry.register(Arc::new(OllamaProvider::new(provider_config)));
        }

        // Register Z.AI if configured
        if let Some(ref zai_config) = config.providers.zai {
            registry.register(Arc::new(OpenAICompatibleProvider::new(
                OpenAICompatibleConfig {
                    provider_id: "zai".to_string(),
                    api_key: zai_config.api_key.clone(),
                    base_url: zai_config
                        .base_url
                        .clone()
                        .unwrap_or_else(|| "https://api.z.ai/api/coding/paas/v4".to_string()),
                    models: zai_models(),
                    completions_path: Some("/chat/completions".to_string()),
                    extra_headers: vec![("Accept-Language".to_string(), "en-US,en".to_string())],
                    extra_body: vec![
                        ("tool_stream".to_string(), serde_json::json!(true)),
                    ],
                },
            )));
        }

        // Register Groq if configured
        if let Some(ref groq_config) = config.providers.groq {
            registry.register(Arc::new(OpenAICompatibleProvider::new(
                OpenAICompatibleConfig {
                    provider_id: "groq".to_string(),
                    api_key: groq_config.api_key.clone(),
                    base_url: groq_config
                        .base_url
                        .clone()
                        .unwrap_or_else(|| "https://api.groq.com/openai/v1".to_string()),
                    models: groq_models(),
                    completions_path: Some("/chat/completions".to_string()),
                    extra_headers: vec![],
                    extra_body: vec![],
                },
            )));
        }

        // Register DeepSeek if configured
        if let Some(ref deepseek_config) = config.providers.deepseek {
            registry.register(Arc::new(OpenAICompatibleProvider::new(
                OpenAICompatibleConfig {
                    provider_id: "deepseek".to_string(),
                    api_key: deepseek_config.api_key.clone(),
                    base_url: deepseek_config
                        .base_url
                        .clone()
                        .unwrap_or_else(|| "https://api.deepseek.com".to_string()),
                    models: deepseek_models(),
                    completions_path: Some("/v1/chat/completions".to_string()),
                    extra_headers: vec![],
                    extra_body: vec![],
                },
            )));
        }

        // Register Mistral if configured
        if let Some(ref mistral_config) = config.providers.mistral {
            registry.register(Arc::new(OpenAICompatibleProvider::new(
                OpenAICompatibleConfig {
                    provider_id: "mistral".to_string(),
                    api_key: mistral_config.api_key.clone(),
                    base_url: mistral_config
                        .base_url
                        .clone()
                        .unwrap_or_else(|| "https://api.mistral.ai/v1".to_string()),
                    models: mistral_models(),
                    completions_path: Some("/chat/completions".to_string()),
                    extra_headers: vec![],
                    extra_body: vec![],
                },
            )));
        }

        // Register Venice.ai if configured
        if let Some(ref venice_config) = config.providers.venice {
            registry.register(Arc::new(OpenAICompatibleProvider::new(
                OpenAICompatibleConfig {
                    provider_id: "venice".to_string(),
                    api_key: venice_config.api_key.clone(),
                    base_url: venice_config
                        .base_url
                        .clone()
                        .unwrap_or_else(|| "https://api.venice.ai/api/v1".to_string()),
                    models: venice_models(),
                    completions_path: Some("/chat/completions".to_string()),
                    extra_headers: vec![],
                    extra_body: vec![],
                },
            )));
        }

        // Register Qwen if configured
        if let Some(ref qwen_config) = config.providers.qwen {
            registry.register(Arc::new(OpenAICompatibleProvider::new(
                OpenAICompatibleConfig {
                    provider_id: "qwen".to_string(),
                    api_key: qwen_config.api_key.clone(),
                    base_url: qwen_config.base_url.clone().unwrap_or_else(|| {
                        "https://dashscope-intl.aliyuncs.com/compatible-mode/v1".to_string()
                    }),
                    models: qwen_models(),
                    completions_path: Some("/chat/completions".to_string()),
                    extra_headers: vec![],
                    extra_body: vec![],
                },
            )));
        }

        // Register Google Gemini if configured
        if let Some(ref gemini_config) = config.providers.gemini {
            registry.register(Arc::new(OpenAICompatibleProvider::new(
                OpenAICompatibleConfig {
                    provider_id: "gemini".to_string(),
                    api_key: gemini_config.api_key.clone(),
                    base_url: gemini_config.base_url.clone().unwrap_or_else(|| {
                        "https://generativelanguage.googleapis.com/v1beta/openai".to_string()
                    }),
                    models: gemini_models(),
                    completions_path: Some("/chat/completions".to_string()),
                    extra_headers: vec![],
                    extra_body: vec![],
                },
            )));
        }

        registry
    }

    /// Register a provider
    pub fn register(&mut self, provider: Arc<dyn Provider>) {
        let id = provider.id().to_string();

        // Build model-to-provider mapping
        for model in provider.models() {
            // Map both full model ID and prefixed version
            self.model_to_provider
                .insert(model.id.clone(), id.clone());
            self.model_to_provider
                .insert(format!("{}/{}", id, model.id), id.clone());
        }

        self.providers.insert(id, provider);
    }

    /// Get a provider by ID
    pub fn get(&self, id: &str) -> Option<Arc<dyn Provider>> {
        self.providers.get(id).cloned()
    }

    /// Get a provider for a model ID
    /// Supports formats: "model-id" or "provider/model-id"
    pub fn get_for_model(&self, model_id: &str) -> Option<Arc<dyn Provider>> {
        // First check if it's a prefixed model (provider/model)
        if let Some((provider_id, _)) = model_id.split_once('/') {
            if let Some(provider) = self.providers.get(provider_id) {
                return Some(provider.clone());
            }
        }

        // Otherwise look up in the model-to-provider mapping
        self.model_to_provider
            .get(model_id)
            .and_then(|provider_id| self.providers.get(provider_id))
            .cloned()
    }

    /// Extract the model name from a potentially prefixed model ID
    pub fn extract_model_name(&self, model_id: &str) -> String {
        if let Some((_, model)) = model_id.split_once('/') {
            model.to_string()
        } else {
            model_id.to_string()
        }
    }

    /// List all registered provider IDs
    pub fn provider_ids(&self) -> Vec<String> {
        self.providers.keys().cloned().collect()
    }

    /// List all available models from all providers
    pub fn all_models(&self) -> Vec<ModelInfo> {
        let mut models = Vec::new();
        for provider in self.providers.values() {
            models.extend(provider.models());
        }
        models
    }

    /// List models with provider prefix
    pub fn all_models_prefixed(&self) -> Vec<PrefixedModel> {
        let mut models = Vec::new();
        for (provider_id, provider) in &self.providers {
            for model in provider.models() {
                models.push(PrefixedModel {
                    id: format!("{}/{}", provider_id, model.id),
                    provider: provider_id.clone(),
                    model,
                });
            }
        }
        models
    }

    /// Check if a provider is registered
    pub fn has_provider(&self, id: &str) -> bool {
        self.providers.contains_key(id)
    }

    /// Get provider count
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Model with provider prefix
#[derive(Debug, Clone)]
pub struct PrefixedModel {
    /// Full model ID (provider/model)
    pub id: String,
    /// Provider ID
    pub provider: String,
    /// Original model info
    pub model: ModelInfo,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_new() {
        let registry = ProviderRegistry::new();
        assert_eq!(registry.provider_count(), 0);
    }

    #[test]
    fn test_registry_register() {
        let mut registry = ProviderRegistry::new();
        let provider = Arc::new(OllamaProvider::new(OllamaConfig::default()));

        registry.register(provider);

        assert!(registry.has_provider("ollama"));
        assert_eq!(registry.provider_count(), 1);
    }

    #[test]
    fn test_registry_get() {
        let mut registry = ProviderRegistry::new();
        let provider = Arc::new(OllamaProvider::new(OllamaConfig::default()));
        registry.register(provider);

        let retrieved = registry.get("ollama");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id(), "ollama");
    }

    #[test]
    fn test_registry_get_for_model_prefixed() {
        let mut registry = ProviderRegistry::new();
        registry.register(Arc::new(OllamaProvider::new(OllamaConfig::default())));

        let provider = registry.get_for_model("ollama/llama3.2");
        assert!(provider.is_some());
        assert_eq!(provider.unwrap().id(), "ollama");
    }

    #[test]
    fn test_registry_get_for_model_unprefixed() {
        let mut registry = ProviderRegistry::new();
        registry.register(Arc::new(OllamaProvider::new(OllamaConfig::default())));

        let provider = registry.get_for_model("llama3.2");
        assert!(provider.is_some());
        assert_eq!(provider.unwrap().id(), "ollama");
    }

    #[test]
    fn test_extract_model_name() {
        let registry = ProviderRegistry::new();

        assert_eq!(
            registry.extract_model_name("anthropic/claude-3-opus"),
            "claude-3-opus"
        );
        assert_eq!(registry.extract_model_name("llama3"), "llama3");
    }

    #[test]
    fn test_provider_ids() {
        let mut registry = ProviderRegistry::new();
        registry.register(Arc::new(OllamaProvider::new(OllamaConfig::default())));
        registry.register(Arc::new(AnthropicProvider::new(AnthropicConfig::default())));

        let ids = registry.provider_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"ollama".to_string()));
        assert!(ids.contains(&"anthropic".to_string()));
    }

    #[test]
    fn test_all_models() {
        let mut registry = ProviderRegistry::new();
        registry.register(Arc::new(OllamaProvider::new(OllamaConfig::default())));

        let models = registry.all_models();
        assert!(!models.is_empty());
    }

    #[test]
    fn test_all_models_prefixed() {
        let mut registry = ProviderRegistry::new();
        registry.register(Arc::new(OllamaProvider::new(OllamaConfig::default())));

        let models = registry.all_models_prefixed();
        assert!(!models.is_empty());
        assert!(models[0].id.starts_with("ollama/"));
    }

    #[test]
    fn test_from_config_empty() {
        let config = Config::default();
        let registry = ProviderRegistry::from_config(&config);

        // No providers configured
        assert_eq!(registry.provider_count(), 0);
    }

    #[test]
    fn test_from_config_with_ollama() {
        let mut config = Config::default();
        config.providers.ollama = Some(crate::config::OllamaConfig {
            base_url: "http://localhost:11434".to_string(),
            default_model: None,
        });

        let registry = ProviderRegistry::from_config(&config);

        assert!(registry.has_provider("ollama"));
    }
}
