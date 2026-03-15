//! Text-to-speech tool - generate speech audio from text.

use std::path::PathBuf;

use async_trait::async_trait;
use serde_json::json;

use super::tools::Tool;

/// TTS tool configuration
#[derive(Debug, Clone)]
pub struct TtsConfig {
    pub api_key: String,
    pub base_url: String,
}

impl Default for TtsConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.openai.com/v1".to_string(),
        }
    }
}

/// Text-to-speech tool
pub struct TextToSpeechTool {
    config: TtsConfig,
    workspace: PathBuf,
    client: reqwest::Client,
}

impl TextToSpeechTool {
    pub fn new(config: TtsConfig, workspace: PathBuf) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap_or_default();
        Self {
            config,
            workspace,
            client,
        }
    }
}

#[async_trait]
impl Tool for TextToSpeechTool {
    fn name(&self) -> &str {
        "text_to_speech"
    }

    fn description(&self) -> &str {
        "Generate speech audio from text using a TTS provider. Returns the path to the generated audio file. The audio can be sent as a voice message on channels like Telegram."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "The text to convert to speech (max 4096 characters)"
                },
                "voice": {
                    "type": "string",
                    "description": "Voice to use: 'alloy', 'echo', 'fable', 'onyx', 'nova', 'shimmer' (default: 'alloy')",
                    "enum": ["alloy", "echo", "fable", "onyx", "nova", "shimmer"],
                    "default": "alloy"
                },
                "speed": {
                    "type": "number",
                    "description": "Speech speed multiplier (0.25-4.0, default: 1.0)",
                    "minimum": 0.25,
                    "maximum": 4.0,
                    "default": 1.0
                }
            },
            "required": ["text"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let text = input
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'text' parameter"))?;
        let voice = input
            .get("voice")
            .and_then(|v| v.as_str())
            .unwrap_or("alloy");
        let speed = input
            .get("speed")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0)
            .clamp(0.25, 4.0);

        if text.len() > 4096 {
            anyhow::bail!("Text too long ({} chars, max 4096)", text.len());
        }

        if self.config.api_key.is_empty() {
            anyhow::bail!("TTS API key not configured. Set tools.tts.api_key in config.");
        }

        let url = format!("{}/audio/speech", self.config.base_url);

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .json(&json!({
                "model": "tts-1",
                "input": text,
                "voice": voice,
                "speed": speed,
                "response_format": "opus"
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("TTS API error ({}): {}", status, body);
        }

        let bytes = resp.bytes().await?;

        // Save to workspace temp directory
        let audio_dir = self.workspace.join(".tmp");
        tokio::fs::create_dir_all(&audio_dir).await?;

        let filename = format!("tts_{}.opus", uuid::Uuid::new_v4());
        let audio_path = audio_dir.join(&filename);
        tokio::fs::write(&audio_path, &bytes).await?;

        let rel_path = format!(".tmp/{}", filename);

        Ok(json!({
            "generated": true,
            "path": rel_path,
            "format": "opus",
            "voice": voice,
            "speed": speed,
            "text_length": text.len(),
            "audio_size": bytes.len()
        })
        .to_string())
    }
}
