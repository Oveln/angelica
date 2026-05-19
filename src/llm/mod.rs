pub mod types;

use futures::StreamExt;
use reqwest::Client;
use tokio::sync::mpsc;

use crate::config::LlmConfig;
use crate::llm::types::*;

pub struct LlmClient {
    client: Client,
    config: LlmConfig,
    api_key: String,
}

impl LlmClient {
    pub fn is_configured(&self) -> bool {
        !self.api_key.is_empty()
    }

    pub fn new(config: &LlmConfig) -> Self {
        let api_key = config
            .api_key
            .clone()
            .or_else(|| std::env::var("DEEPSEEK_API_KEY").ok())
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .unwrap_or_default();
        Self {
            client: Client::new(),
            config: config.clone(),
            api_key,
        }
    }

    pub async fn stream_complete(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
        event_tx: &mpsc::Sender<AppStreamEvent>,
    ) -> anyhow::Result<StreamFinal> {
        let mut payload = serde_json::json!({
            "model": self.config.model,
            "messages": messages,
            "max_tokens": self.config.max_tokens,
            "stream": true,
        });

        if self.config.thinking {
            payload["thinking"] = serde_json::json!({ "type": "enabled" });
            payload["reasoning_effort"] = serde_json::json!(self.config.reasoning_effort);
        } else {
            payload["temperature"] = serde_json::json!(self.config.temperature);
        }

        if !tools.is_empty() {
            payload["tools"] = serde_json::json!(tools);
        }

        let url = format!(
            "{}/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&payload)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("API error {}: {}", status, body));
        }

        let mut accumulator = StreamAccumulator::new();
        let mut stream = response.bytes_stream();
        let mut line_buf = String::new();

        loop {
            let chunk = match stream.next().await {
                Some(Ok(data)) => data,
                Some(Err(e)) => return Err(anyhow::anyhow!("Stream error: {}", e)),
                None => break,
            };

            line_buf.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(pos) = line_buf.find('\n') {
                let line = line_buf[..pos].trim_end_matches('\r').trim().to_string();
                line_buf.drain(..=pos);

                if !line.starts_with("data: ") {
                    continue;
                }

                let data = &line[6..];
                if data == "[DONE]" {
                    let final_result = accumulator.finalize();
                    let _ = event_tx.send(AppStreamEvent::Done).await;
                    return Ok(final_result);
                }

                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(choices) = parsed.get("choices").and_then(|c| c.as_array()) {
                        if let Some(choice) = choices.first() {
                            if let Some(delta) = choice.get("delta") {
                                let events = accumulator.process_delta(delta);
                                for evt in events {
                                    let _ = event_tx.send(evt).await;
                                }
                            }
                        }
                    }
                }
            }
        }

        if !line_buf.trim().is_empty() {
            tracing::warn!(
                "SSE stream ended with partial line: {} bytes remaining",
                line_buf.len()
            );
        }

        let final_result = accumulator.finalize();
        let _ = event_tx.send(AppStreamEvent::Done).await;
        Ok(final_result)
    }
}

#[derive(Debug, Clone)]
pub enum AppStreamEvent {
    ThinkingDelta {
        delta: String,
    },
    TextDelta {
        delta: String,
    },
    ToolCallStart {
        index: usize,
        id: String,
        name: String,
    },
    ToolCallArgsDelta {
        index: usize,
        delta: String,
    },
    Done,
}

pub struct StreamFinal {
    pub reasoning: Option<String>,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}

struct StreamAccumulator {
    thinking: String,
    content: String,
    tool_calls: Vec<PartialToolCall>,
}

#[derive(Default)]
struct PartialToolCall {
    id: Option<String>,
    function_name: String,
    function_arguments: String,
}

impl StreamAccumulator {
    fn new() -> Self {
        Self {
            thinking: String::new(),
            content: String::new(),
            tool_calls: Vec::new(),
        }
    }

    fn process_delta(&mut self, delta: &serde_json::Value) -> Vec<AppStreamEvent> {
        let mut events = Vec::new();

        if let Some(rc) = delta.get("reasoning_content").and_then(|v| v.as_str()) {
            self.thinking.push_str(rc);
            events.push(AppStreamEvent::ThinkingDelta {
                delta: rc.to_string(),
            });
        }

        if let Some(c) = delta.get("content").and_then(|v| v.as_str()) {
            self.content.push_str(c);
            events.push(AppStreamEvent::TextDelta {
                delta: c.to_string(),
            });
        }

        if let Some(tcs) = delta.get("tool_calls").and_then(|v| v.as_array()) {
            for tc in tcs {
                let idx = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                while self.tool_calls.len() <= idx {
                    self.tool_calls.push(PartialToolCall::default());
                }
                let partial = &mut self.tool_calls[idx];

                let mut new_id = None;
                let mut new_name = None;

                if let Some(id) = tc.get("id").and_then(|v| v.as_str()) {
                    partial.id = Some(id.to_string());
                    new_id = Some(id.to_string());
                }
                if let Some(f) = tc.get("function") {
                    if let Some(name) = f.get("name").and_then(|v| v.as_str()) {
                        partial.function_name.push_str(name);
                        new_name = Some(name.to_string());
                    }
                    if let Some(args_val) = f.get("arguments") {
                        match args_val {
                            serde_json::Value::String(s) => {
                                partial.function_arguments.push_str(s);
                                events.push(AppStreamEvent::ToolCallArgsDelta {
                                    index: idx,
                                    delta: s.clone(),
                                });
                            }
                            other => {
                                let serialized = other.to_string();
                                partial.function_arguments =
                                    partial.function_arguments.clone() + &serialized;
                                events.push(AppStreamEvent::ToolCallArgsDelta {
                                    index: idx,
                                    delta: serialized,
                                });
                            }
                        }
                    }
                }

                if let (Some(id), Some(name)) = (new_id, new_name) {
                    events.push(AppStreamEvent::ToolCallStart {
                        index: idx,
                        id,
                        name,
                    });
                }
            }
        }

        events
    }

    fn finalize(self) -> StreamFinal {
        let reasoning = if self.thinking.is_empty() {
            None
        } else {
            Some(self.thinking)
        };
        let content = if self.content.is_empty() {
            None
        } else {
            Some(self.content)
        };
        let tool_calls = if self.tool_calls.is_empty() {
            None
        } else {
            Some(
                self.tool_calls
                    .into_iter()
                    .map(|tc| ToolCall {
                        id: tc.id.unwrap_or_default(),
                        call_type: "function".to_string(),
                        function: FunctionCall {
                            name: tc.function_name,
                            arguments: tc.function_arguments,
                        },
                    })
                    .collect(),
            )
        };
        StreamFinal {
            reasoning,
            content,
            tool_calls,
        }
    }
}
