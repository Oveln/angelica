use std::time::Duration;

use anyhow::Result;
use serde::Deserialize;

use crate::config::EmbeddingConfig;

#[derive(Deserialize)]
struct EmbedResponse {
    #[serde(default)]
    embeddings: Vec<Vec<f32>>,
}

/// Compute embedding for a single text input.
pub async fn embed(config: &EmbeddingConfig, text: &str) -> Result<Vec<f32>> {
    let base_url = config.base_url.trim_end_matches('/');
    let url = format!("{}/api/embed", base_url);
    let body = serde_json::json!({
        "model": config.model,
        "input": text
    });


    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    let mut req = client.post(&url).json(&body);
    if !config.api_key_env.is_empty() {
        if let Ok(key) = std::env::var(&config.api_key_env) {
            req = req.bearer_auth(&key);
        }
    }
    let resp = req.send().await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("embedding request failed ({}): {}", status, body);
    }

    let parsed: EmbedResponse = resp.json().await?;
    let embedding = parsed
        .embeddings
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no embedding in response"))?;

    Ok(embedding)
}

/// Compute embedding for concatenated user + assistant text (for recall search).
pub async fn embed_turn(config: &EmbeddingConfig, user: &str, assistant: &str) -> Result<Vec<f32>> {
    let combined = format!("{}\n{}", user, assistant);
    embed(config, &combined).await
}
