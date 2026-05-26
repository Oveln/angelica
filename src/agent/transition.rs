use std::path::PathBuf;

use crate::llm::types::ChatMessage;

use super::Agent;
use super::history::History;
use super::modes::{AwakeMode, RunMode, SleepingMode};

impl Agent<AwakeMode> {
    /// Transition into sleeping mode. Consumes self, returns Agent<SleepingMode>.
    /// Archives conversation, creates sleep history, resets per-turn state.
    pub fn transition_to_sleeping(self, snapshot_ts: String) -> Agent<SleepingMode> {
        self.save_state();

        let data_dir = self.config.state.data_dir();
        let history_folder = data_dir.join("history").join(&snapshot_ts);

        // Archive conversation before swapping history
        if let Err(e) = std::fs::create_dir_all(&history_folder) {
            tracing::error!("Failed to create history folder: {}", e);
        }
        if !self.history.messages().is_empty() {
            let archive_path = history_folder.join("conversation.jsonl");
            let now_ts = chrono::Local::now().to_rfc3339();
            if let Ok(mut file) = std::fs::File::create(&archive_path) {
                use std::io::Write;
                for (i, msg) in self.history.messages().iter().enumerate() {
                    let tm = crate::agent::history::TimedMessage {
                        ts: format!("{} (msg {})", now_ts, i),
                        message: msg.clone(),
                    };
                    let _ = writeln!(file, "{}", serde_json::to_string(&tm).unwrap_or_default());
                }
            }
        }

        // Build sleeping mode
        let (conversation_text, turns, tool_calls, fatigue_desc, fatigue_val) = {
            let awake = &self.run_state;
            let (t, tc, f) = awake.fatigue_info();
            let text = crate::sleep::build_conversation_text(self.history.messages());
            (text, t, tc, awake.fatigue_desc().to_string(), f)
        };

        let sleeping = SleepingMode::new(
            self.memory.clone(),
            conversation_text,
            turns,
            tool_calls,
            fatigue_desc,
            fatigue_val,
        );

        let sleep_history_path = history_folder.join("sleep.jsonl");
        let mut new_history = History::new(sleep_history_path);

        let system_msg = sleeping.build_system_message(&self.memory, &self.skills);
        new_history.push(system_msg);

        self.into_mode(sleeping, new_history)
    }
}

impl Agent<SleepingMode> {
    pub async fn run_sleep_cycle(
        mut self,
        event_tx: &tokio::sync::mpsc::Sender<crate::agent::events::AppEvent>,
        snapshot_ts: String,
    ) -> Agent<AwakeMode> {
        use crate::agent::events::AppEvent;

        use crate::sleep::consolidation;

        let data_dir = self.config.state.data_dir();
        let history_folder = data_dir.join("history").join(&snapshot_ts);
        self.history.push(ChatMessage::user(" "));
        let _ = event_tx.send(AppEvent::Sleeping).await;

        loop {
            if event_tx.is_closed() {
                break;
            }
            let continued = self.step(event_tx).await;
            if self.is_finished() {
                break;
            }
            if !continued {
                tracing::warn!("Sleep step ended without finishing; breaking loop");
                break;
            }
        }

        let embed_config = &self.config.embedding;
        let transitioned =
            consolidation::phase_transition_and_embed(&self.memory, embed_config).await;

        consolidation::phase_consolidate(&self.memory, &self.llm, &transitioned).await;

        consolidation::phase_compress(&self.memory, &self.llm).await;

        let dream = self.run_state.take_dream();
        let (turns, tool_calls, fatigue_val) = self.run_state.pre_sleep_stats();
        let dream_for_event = dream.clone();
        let sleep_record = serde_json::json!({
            "timestamp": snapshot_ts,
            "turns": turns,
            "tool_calls": tool_calls,
            "fatigue": fatigue_val,
            "dream": dream,
        });
        let sleep_path = history_folder.join("sleep.json");
        if let Err(e) = std::fs::write(
            &sleep_path,
            serde_json::to_string_pretty(&sleep_record).unwrap_or_default(),
        ) {
            tracing::error!("Failed to write sleep record: {}", e);
        }

        if let Err(e) =
            crate::data_git::commit_all(&data_dir, &format!("sleep done: {}", snapshot_ts))
        {
            tracing::error!("Failed to commit sleep data: {}", e);
        }

        let mut awake = self.transition_to_awake(dream);

        if let Err(e) = awake.initialize().await {
            tracing::error!("Failed to initialize agent after sleep: {}", e);
            let _ = event_tx
                .send(crate::agent::events::AppEvent::Error {
                    message: format!("Post-sleep initialization failed: {}", e),
                })
                .await;
        }

        if let Some(d) = dream_for_event {
            let _ = event_tx.send(AppEvent::WakingUp { dream: d }).await;
        }
        if let Some(evt) = awake.run_state.fatigue_update_event() {
            let _ = event_tx.try_send(evt);
        }

        awake
    }

    fn transition_to_awake(self, dream: Option<String>) -> Agent<AwakeMode> {
        {
            let state_path = std::path::PathBuf::from(&self.config.state.path);
            if state_path.exists()
                && let Err(e) = std::fs::remove_file(&state_path)
            {
                tracing::error!("Failed to remove state file: {}", e);
            }

            let conversation_path = std::path::PathBuf::from(&self.config.state.conversation_path);
            if conversation_path.exists()
                && let Err(e) = std::fs::write(&conversation_path, "")
            {
                tracing::error!("Failed to clear conversation history: {}", e);
            }
        }

        let awake = AwakeMode::build(
            &self.config,
            self.memory.clone(),
            self.skills.clone(),
            dream,
        );

        let conversation_path = PathBuf::from(&self.config.state.conversation_path);
        let mut new_history = History::new(conversation_path);

        let system_msg = awake.build_system_message(&self.memory, &self.skills);
        new_history.push(system_msg);

        let new_agent = self.into_mode(awake, new_history);

        new_agent.save_state();

        new_agent
    }
}
