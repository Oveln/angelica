use anyhow::Result;
use serde::Deserialize;
use tracing;

use crate::embedding::{self, EmbeddingConfig};
use crate::episode::Episode;
use crate::llm::types::ChatMessage;
use crate::llm::RequestOptions;
use crate::llm::LlmClient;
use crate::memory::MemoryManager;

/// Run Phase 2a: transition excess recent episodes to past, compute embeddings for past episodes that lack them.
pub async fn phase_transition_and_embed(
    memory: &MemoryManager,
    embed_config: &EmbeddingConfig,
) -> Vec<Episode> {
    // Transition recent → past
    let transitioned = memory.transition_to_past();
    if !transitioned.is_empty() {
        tracing::info!("Transitioned {} episodes from recent to past", transitioned.len());
    }

    // Compute embeddings for all past episodes that lack them
    let mut episodes = memory.read_episodes();
    let mut modified = false;

    for ep in &mut episodes {
        if ep.embedding.is_empty() {
            match embedding::embed(embed_config, &format!("{} {}", ep.date, ep.body)).await {
                Ok(vec) => {
                    tracing::debug!("Computed embedding for episode {} ({})", ep.id, ep.date);
                    ep.embedding = vec;
                    modified = true;
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to compute embedding for episode {} ({}): {}",
                        ep.id,
                        ep.date,
                        e
                    );
                }
            }
        }
    }

    if modified {
        if let Err(e) = memory.write_all_episodes(&episodes) {
            tracing::error!("Failed to write updated episodes: {}", e);
        }
    }

    transitioned
}

/// Consolidation result from LLM json_mode analysis of past episodes.
#[derive(Debug, Deserialize)]
struct ConsolidationResult {
    /// Insights about the agent's self (to append to SELF.md)
    #[serde(default)]
    self_insights: Vec<String>,
    /// Insights about the user (to append to profile)
    #[serde(default)]
    user_insights: Vec<String>,
}

/// Run Phase 2b: LLM analyzes past episodes and consolidates insights into SELF.md and profile.
pub async fn phase_consolidate(
    memory: &MemoryManager,
    llm: &LlmClient,
    transitioned: &[Episode],
) {
    if transitioned.is_empty() {
        tracing::info!("No episodes to consolidate");
        return;
    }

    // Build episode summaries for LLM
    let mut episode_text = String::new();
    for ep in transitioned {
        episode_text.push_str(&format!(
            "### {} (情感权重: {}/5)\n{}\n",
            ep.date, ep.emotional_weight, ep.body
        ));
        if !ep.afterglow.is_empty() {
            episode_text.push_str(&format!("余韵: {}\n", ep.afterglow));
        }
        episode_text.push('\n');
    }

    let current_self = memory.read_self();
    let current_profile = memory.read_user_profile();

    let system_msg = ChatMessage {
        role: "system".to_string(),
        content: Some(
            "你是一个记忆分析系统。分析以下过往情景记忆，提炼出两类认知：
1. 关于祈芷自身的认知（性格变化、新的自我理解、世界观调整等）
2. 关于用户的认知（用户的偏好、习惯、情感状态、关系变化等）

以 JSON 格式输出，格式如下：
{\"self_insights\": [\"洞察1\", \"洞察2\"], \"user_insights\": [\"洞察1\", \"洞察2\"]}

每条洞察应该是一句简洁的陈述句。如果某类没有新的认知，返回空数组。
只输出 JSON，不要输出其他内容。".to_string()
        ),
        reasoning_content: None,
        tool_calls: None,
        tool_call_id: None,
        name: None,
        usage: None,
    };

    let user_content = format!(
        "## 当前 SELF.md\n{}\n\n## 当前用户画像\n{}\n\n## 需要分析的过往情景\n{}",
        current_self, current_profile, episode_text
    );

    let user_msg = ChatMessage {
        role: "user".to_string(),
        content: Some(user_content),
        reasoning_content: None,
        tool_calls: None,
        tool_call_id: None,
        name: None,
        usage: None,
    };

    let options = RequestOptions {
        temperature: Some(0.3),
        thinking: Some(false),
        ..RequestOptions::new()
    };

    match llm.complete(&[system_msg, user_msg], options).await {
        Ok(response) => {
            if let Some(content) = &response.content {
                if let Err(e) =
                    apply_consolidation(memory, content, &current_self, &current_profile)
                {
                    tracing::error!("Failed to apply consolidation: {}", e);
                }
            }
        }
        Err(e) => {
            tracing::error!("Consolidation LLM call failed: {}", e);
        }
    }
}

fn apply_consolidation(
    memory: &MemoryManager,
    raw_json: &str,
    current_self: &str,
    current_profile: &str,
) -> Result<()> {
    // Try to extract JSON from the response (LLM may wrap it in markdown)
    let json_str = extract_json(raw_json);

    let result: ConsolidationResult = match serde_json::from_str(&json_str) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("Failed to parse consolidation JSON: {}\nRaw: {}", e, raw_json);
            return Err(anyhow::anyhow!("JSON parse error: {}", e));
        }
    };

    tracing::info!(
        "Consolidation: {} self insights, {} user insights",
        result.self_insights.len(),
        result.user_insights.len()
    );

    // Apply self insights
    if !result.self_insights.is_empty() {
        let date = chrono::Local::now().format("%Y-%m-%d").to_string();
        let mut addition = format!("\n## {} 沉淀\n", date);
        for insight in &result.self_insights {
            addition.push_str(&format!("- {}\n", insight));
        }
        let updated = format!("{}\n{}", current_self.trim_end(), addition);
        memory.write_self(&updated);
    }

    // Apply user insights
    if !result.user_insights.is_empty() {
        let date = chrono::Local::now().format("%Y-%m-%d").to_string();
        let mut addition = format!("\n## {} 沉淀\n", date);
        for insight in &result.user_insights {
            addition.push_str(&format!("- {}\n", insight));
        }
        let updated = format!("{}\n{}", current_profile.trim_end(), addition);
        memory.write_user_profile(&updated);
    }

    Ok(())
}

/// Extract JSON object from text that may contain markdown code fences.
fn extract_json(text: &str) -> String {
    let trimmed = text.trim();

    // If wrapped in markdown code fence
    if trimmed.starts_with("```") {
        let without_fence = trimmed
            .strip_prefix("```json")
            .or_else(|| trimmed.strip_prefix("```"))
            .unwrap_or(trimmed);
        if let Some(end) = without_fence.rfind("```") {
            return without_fence[..end].trim().to_string();
        }
        return without_fence.trim().to_string();
    }

    // If it starts with {, find the matching }
    if trimmed.starts_with('{') {
        if let Some(end) = trimmed.rfind('}') {
            return trimmed[..=end].to_string();
        }
    }

    trimmed.to_string()
}

/// Run Phase 3: compress SELF.md or profile if they exceed hard limits.
pub async fn phase_compress(memory: &MemoryManager, llm: &LlmClient) {
    if memory.self_hard_limit_reached() {
        match compress_file(llm, &memory.read_self(), "SELF.md", "祈芷的自我认知").await {
            Ok(compressed) => {
                tracing::info!("Compressed SELF.md: {} chars", compressed.len());
                memory.write_self(&compressed);
            }
            Err(e) => tracing::error!("Failed to compress SELF.md: {}", e),
        }
    }

    if memory.profile_hard_limit_reached() {
        match compress_file(
            llm,
            &memory.read_user_profile(),
            "用户画像",
            "关于用户的认知",
        )
        .await
        {
            Ok(compressed) => {
                tracing::info!("Compressed profile: {} chars", compressed.len());
                memory.write_user_profile(&compressed);
            }
            Err(e) => tracing::error!("Failed to compress profile: {}", e),
        }
    }
}

async fn compress_file(
    llm: &LlmClient,
    content: &str,
    _file_name: &str,
    description: &str,
) -> Result<String> {
    let system_msg = ChatMessage {
        role: "system".to_string(),
        content: Some(format!(
            "你是一个文本压缩系统。以下内容是{}，已经超出了大小限制。
请将其压缩到原来的一半左右，保留最核心和最重要的信息。
保持 markdown 格式。只输出压缩后的内容，不要解释。",
            description
        )),
        reasoning_content: None,
        tool_calls: None,
        tool_call_id: None,
        name: None,
        usage: None,
    };

    let user_msg = ChatMessage {
        role: "user".to_string(),
        content: Some(content.to_string()),
        reasoning_content: None,
        tool_calls: None,
        tool_call_id: None,
        name: None,
        usage: None,
    };

    let options = RequestOptions {
        temperature: Some(0.3),
        thinking: Some(false),
        ..RequestOptions::new()
    };

    let response = llm.complete(&[system_msg, user_msg], options).await?;
    Ok(response.content.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_json_plain() {
        let input = r#"{"self_insights": ["a"], "user_insights": []}"#;
        assert_eq!(extract_json(input), input);
    }

    #[test]
    fn extract_json_with_fence() {
        let input = "```json\n{\"self_insights\": [\"a\"], \"user_insights\": []}\n```";
        let expected = r#"{"self_insights": ["a"], "user_insights": []}"#;
        assert_eq!(extract_json(input), expected);
    }

    #[test]
    fn extract_json_with_fence_no_lang() {
        let input = "```\n{\"self_insights\": [], \"user_insights\": []}\n```";
        let expected = r#"{"self_insights": [], "user_insights": []}"#;
        assert_eq!(extract_json(input), expected);
    }

    #[test]
    fn extract_json_trailing_text() {
        let input = r#"{"self_insights": ["a"], "user_insights": ["b"]}
Here is some extra text"#;
        assert_eq!(
            extract_json(input),
            r#"{"self_insights": ["a"], "user_insights": ["b"]}"#
        );
    }
}
