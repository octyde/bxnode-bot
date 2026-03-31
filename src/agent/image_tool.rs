//! Image generation tool - generate images from text prompts.

use std::path::PathBuf;

use async_trait::async_trait;
use serde_json::json;

use super::tools::Tool;

/// Image generation tool configuration
#[derive(Debug, Clone)]
pub struct ImageGenConfig {
    pub api_key: String,
    pub base_url: String,
}

impl Default for ImageGenConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            base_url: "https://api.openai.com/v1".to_string(),
        }
    }
}

/// Image generation tool using OpenAI DALL-E API
pub struct ImageGenerateTool {
    config: ImageGenConfig,
    workspace: PathBuf,
    client: reqwest::Client,
}

impl ImageGenerateTool {
    pub fn new(config: ImageGenConfig, workspace: PathBuf) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
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
impl Tool for ImageGenerateTool {
    fn name(&self) -> &str {
        "image_generate"
    }

    fn description(&self) -> &str {
        "Generate images from text prompts using DALL-E. Returns the path to the saved image file."
    }

    fn input_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "A text description of the desired image (max 4000 characters)"
                },
                "size": {
                    "type": "string",
                    "description": "Image dimensions",
                    "enum": ["1024x1024", "1024x1792", "1792x1024"],
                    "default": "1024x1024"
                },
                "quality": {
                    "type": "string",
                    "description": "Image quality: 'standard' or 'hd'",
                    "enum": ["standard", "hd"],
                    "default": "standard"
                },
                "style": {
                    "type": "string",
                    "description": "Image style: 'vivid' (dramatic) or 'natural' (realistic)",
                    "enum": ["vivid", "natural"],
                    "default": "vivid"
                }
            },
            "required": ["prompt"]
        })
    }

    async fn execute(&self, input: serde_json::Value) -> anyhow::Result<String> {
        let prompt = input
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'prompt' parameter"))?;
        let size = input
            .get("size")
            .and_then(|v| v.as_str())
            .unwrap_or("1024x1024");
        let quality = input
            .get("quality")
            .and_then(|v| v.as_str())
            .unwrap_or("standard");
        let style = input
            .get("style")
            .and_then(|v| v.as_str())
            .unwrap_or("vivid");

        if prompt.len() > 4000 {
            anyhow::bail!("Prompt too long ({} chars, max 4000)", prompt.len());
        }

        // Validate enum values
        if !matches!(size, "1024x1024" | "1024x1792" | "1792x1024") {
            anyhow::bail!("Invalid size '{}'. Must be 1024x1024, 1024x1792, or 1792x1024", size);
        }
        if !matches!(quality, "standard" | "hd") {
            anyhow::bail!("Invalid quality '{}'. Must be 'standard' or 'hd'", quality);
        }
        if !matches!(style, "vivid" | "natural") {
            anyhow::bail!("Invalid style '{}'. Must be 'vivid' or 'natural'", style);
        }

        if self.config.api_key.is_empty() {
            anyhow::bail!(
                "Image generation API key not configured. Set tools.image_generation.api_key in config."
            );
        }

        let url = format!("{}/images/generations", self.config.base_url);

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .json(&json!({
                "model": "dall-e-3",
                "prompt": prompt,
                "n": 1,
                "size": size,
                "quality": quality,
                "style": style,
                "response_format": "url"
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Image generation API error ({}): {}", status, body);
        }

        let body: serde_json::Value = resp.json().await?;

        // Extract image URL and revised prompt from response
        let data = body
            .get("data")
            .and_then(|d| d.as_array())
            .and_then(|a| a.first())
            .ok_or_else(|| anyhow::anyhow!("No image data in API response"))?;

        let image_url = data
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("No image URL in API response"))?;

        let revised_prompt = data
            .get("revised_prompt")
            .and_then(|v| v.as_str())
            .unwrap_or(prompt);

        // Download the image
        let image_resp = self.client.get(image_url).send().await?;
        if !image_resp.status().is_success() {
            anyhow::bail!("Failed to download generated image");
        }
        let image_bytes = image_resp.bytes().await?;

        // Save to workspace temp directory
        let img_dir = self.workspace.join(".tmp");
        tokio::fs::create_dir_all(&img_dir).await?;

        let filename = format!("img_{}.png", uuid::Uuid::new_v4());
        let img_path = img_dir.join(&filename);
        tokio::fs::write(&img_path, &image_bytes).await?;

        let rel_path = format!(".tmp/{}", filename);

        Ok(json!({
            "generated": true,
            "path": rel_path,
            "prompt": prompt,
            "revised_prompt": revised_prompt,
            "size": size,
            "quality": quality,
            "style": style,
            "image_size_bytes": image_bytes.len()
        })
        .to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_tool() -> ImageGenerateTool {
        ImageGenerateTool::new(
            ImageGenConfig {
                api_key: "test-key".to_string(),
                ..Default::default()
            },
            PathBuf::from("/tmp/test-workspace"),
        )
    }

    #[tokio::test]
    async fn test_missing_prompt() {
        let tool = test_tool();
        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Missing 'prompt'"));
    }

    #[tokio::test]
    async fn test_prompt_too_long() {
        let tool = test_tool();
        let long_prompt = "x".repeat(4001);
        let result = tool.execute(json!({"prompt": long_prompt})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too long"));
    }

    #[tokio::test]
    async fn test_invalid_size() {
        let tool = test_tool();
        let result = tool.execute(json!({"prompt": "test", "size": "512x512"})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid size"));
    }

    #[tokio::test]
    async fn test_empty_api_key() {
        let tool = ImageGenerateTool::new(
            ImageGenConfig::default(),
            PathBuf::from("/tmp/test"),
        );
        let result = tool.execute(json!({"prompt": "test"})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not configured"));
    }

    #[test]
    fn test_input_schema() {
        let tool = test_tool();
        let schema = tool.input_schema();
        let required = schema.get("required").unwrap().as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "prompt");

        let props = schema.get("properties").unwrap();
        assert!(props.get("prompt").is_some());
        assert!(props.get("size").is_some());
        assert!(props.get("quality").is_some());
        assert!(props.get("style").is_some());
    }
}
