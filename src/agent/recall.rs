use super::Agent;
use super::modes::RunMode;
use crate::embedding;

impl<S: RunMode> Agent<S> {
    /// After a turn completes, compute embedding from user input + assistant response
    /// and search past episodes. Store results + top score for probabilistic injection.
    pub(super) async fn recall_past_episodes(&mut self, assistant_content: Option<&str>) {
        let budget = self.config.memory.episode_inject_budget;
        if budget == 0 {
            return;
        }

        // Get the last user message and assistant content
        let user_msg = self
            .history
            .messages()
            .iter()
            .rev()
            .find(|m| m.role == "user");
        let user_text = match user_msg.and_then(|m| m.content.as_ref()) {
            Some(t) => t.clone(),
            None => return,
        };
        let assistant_text = assistant_content.unwrap_or("").to_string();

        let embed_config = &self.config.embedding;
        let result = embedding::embed_turn(&embed_config, &user_text, &assistant_text).await;

        match result {
            Ok(query_emb) => {
                let (recalled, top_score) = self.memory.search_past_episodes(&query_emb, budget);
                if !recalled.is_empty() && top_score > 0.0 {
                    tracing::info!(
                        "Recalled {} past episode(s), top score: {:.3}",
                        recalled.lines().filter(|l| l.starts_with("事情发生的时间")).count(),
                        top_score
                    );
                    self.recall_text = recalled;
                    self.recall_top_score = top_score;
                } else {
                    self.recall_text.clear();
                    self.recall_top_score = 0.0;
                }
            }
            Err(e) => {
                tracing::debug!("Embedding search skipped: {}", e);
            }
        }
    }
}
