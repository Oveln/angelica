pub mod patch;
pub mod types;

use futures::StreamExt;
use genai::chat::{
    ChatMessage as GenaiMessage, ChatOptions, ChatRequest, ChatStreamEvent, ContentPart,
    MessageContent, ReasoningEffort, StreamChunk, Tool as GenaiTool, ToolCall as GenaiToolCall,
    ToolResponse,
};
use genai::resolver::{AuthData, AuthResolver, Endpoint, ServiceTargetResolver};
use std::sync::Arc;
use genai::Client;
use tokio::sync::mpsc;

use crate::config::LlmConfig;
use crate::llm::types::*;

pub struct LlmClient {
    client: Client,
    model: String,
    thinking: bool,
    temperature: f32,
    max_tokens: u32,
    reasoning_effort: String,
    configured: bool,
}

impl LlmClient {
    pub fn is_configured(&self) -> bool {
        self.configured
    }

    pub fn new(config: &LlmConfig) -> Self {
        let api_key = config
            .api_key
            .clone()
            .or_else(|| std::env::var("DEEPSEEK_API_KEY").ok())
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .unwrap_or_default();

        let configured = !api_key.is_empty();

        let key = api_key;
        let base_url: Arc<str> = config.base_url.clone().into();

        let client = Client::builder()
            .with_auth_resolver(AuthResolver::from_resolver_fn(move |_kind| {
                Ok(Some(AuthData::from_single(key.clone())))
            }))
            .with_service_target_resolver(
                ServiceTargetResolver::from_resolver_fn(
                    move |mut target: genai::ServiceTarget| {
                    target.endpoint = Endpoint::from_owned(base_url.clone());
                    Ok(target)
                }),
            )
            .build();

        Self {
            client,
            model: config.model.clone(),
            thinking: config.thinking,
            temperature: config.temperature,
            max_tokens: config.max_tokens,
            reasoning_effort: config.reasoning_effort.clone(),
            configured,
        }
    }

    pub async fn stream_complete(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
        event_tx: &mpsc::Sender<AppStreamEvent>,
    ) -> anyhow::Result<StreamFinal> {
        let genai_messages: Vec<GenaiMessage> = messages
            .iter()
            .map(convert_message)
            .collect::<anyhow::Result<Vec<_>>>()?;

        let mut chat_req = ChatRequest::new(genai_messages);

        if !tools.is_empty() {
            let genai_tools: Vec<GenaiTool> = tools
                .iter()
                .map(|t| {
                    let mut tool = GenaiTool::new(&t.function.name);
                    if let Some(desc) = &t.function.description {
                        tool = tool.with_description(desc);
                    }
                    if let Some(params) = &t.function.parameters {
                        tool = tool.with_schema(params.clone());
                    }
                    tool
                })
                .collect();
            chat_req = chat_req.with_tools(genai_tools);
        }

        let mut options = ChatOptions::default()
            .with_max_tokens(self.max_tokens)
            .with_capture_content(true)
            .with_capture_reasoning_content(true)
            .with_capture_tool_calls(true);

        if self.thinking {
            let effort = match self.reasoning_effort.as_str() {
                "low" => ReasoningEffort::Low,
                "medium" => ReasoningEffort::Medium,
                "high" => ReasoningEffort::High,
                _ => ReasoningEffort::High,
            };
            options = options.with_reasoning_effort(effort);
        } else {
            options = options
                .with_temperature(self.temperature as f64)
                .with_extra_body(serde_json::json!({ "thinking": { "type": "disabled" } }));
        }

        let stream_response = self
            .client
            .exec_chat_stream(&self.model, chat_req, Some(&options))
            .await
            .map_err(|e| anyhow::anyhow!("LLM error: {}", e))?;

        let mut thinking = String::new();
        let mut content = String::new();
        let mut tool_calls: Vec<GenaiToolCall> = Vec::new();

        let mut stream = stream_response.stream;

        while let Some(event_result) = stream.next().await {
            let event = event_result.map_err(|e| anyhow::anyhow!("Stream error: {}", e))?;

            match event {
                ChatStreamEvent::Chunk(StreamChunk { content: text }) if !text.is_empty() => {
                    content.push_str(&text);
                    let _ = event_tx
                        .send(AppStreamEvent::TextDelta { delta: text })
                        .await;
                }
                ChatStreamEvent::ReasoningChunk(StreamChunk { content: text }) if !text.is_empty() => {
                    thinking.push_str(&text);
                    let _ = event_tx
                        .send(AppStreamEvent::ThinkingDelta { delta: text })
                        .await;
                }
                ChatStreamEvent::ToolCallChunk(_chunk) => {
                    // Tool calls are captured via capture_tool_calls in StreamEnd
                }
                ChatStreamEvent::End(end) => {
                    // Prefer captured_reasoning_content over stream-accumulated chunks
                    // as it's the canonical complete version from the API.
                    if let Some(reasoning) = end.captured_reasoning_content {
                        thinking = reasoning;
                    }
                    if let Some(mc) = end.captured_content {
                        tool_calls = mc.tool_calls().into_iter().cloned().collect();
                        if content.is_empty() {
                            content = mc.into_first_text().unwrap_or_default();
                        }
                    }
                }
                _ => {}
            }
        }

        let _ = event_tx.send(AppStreamEvent::Done).await;

        let our_tool_calls = if tool_calls.is_empty() {
            None
        } else {
            Some(
                tool_calls
                    .into_iter()
                    .map(|tc| ToolCall {
                        id: tc.call_id,
                        call_type: "function".to_string(),
                        function: FunctionCall {
                            name: tc.fn_name,
                            arguments: serde_json::to_string(&tc.fn_arguments)
                                .unwrap_or_else(|_| "{}".to_string()),
                        },
                    })
                    .collect(),
            )
        };

        Ok(StreamFinal {
            reasoning: if thinking.is_empty() {
                None
            } else {
                Some(thinking)
            },
            content: if content.is_empty() {
                None
            } else {
                Some(content)
            },
            tool_calls: our_tool_calls,
        })
    }
}

fn convert_message(msg: &ChatMessage) -> anyhow::Result<GenaiMessage> {
    match msg.role.as_str() {
        "system" => Ok(GenaiMessage::system(
            msg.content.clone().unwrap_or_default(),
        )),
        "user" => Ok(GenaiMessage::user(
            msg.content.clone().unwrap_or_default(),
        )),
        "assistant" => {
            if let Some(tcs) = &msg.tool_calls {
                let genai_tcs: Vec<GenaiToolCall> = tcs
                    .iter()
                    .map(|tc| {
                        Ok(GenaiToolCall {
                            call_id: tc.id.clone(),
                            fn_name: tc.function.name.clone(),
                            fn_arguments: serde_json::from_str(&tc.function.arguments)?,
                            thought_signatures: None,
                        })
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?;

                let mut parts: Vec<ContentPart> = Vec::new();
                if let Some(text) = &msg.content {
                    if !text.is_empty() {
                        parts.push(ContentPart::Text(text.clone()));
                    }
                }
                parts.extend(genai_tcs.into_iter().map(ContentPart::ToolCall));

                let message = GenaiMessage::assistant(MessageContent::from_parts(parts))
                    .with_reasoning_content(msg.reasoning_content.clone());
                Ok(message)
            } else {
                Ok(GenaiMessage::assistant(
                    msg.content.clone().unwrap_or_default(),
                ))
            }
        }
        "tool" => {
            let response = ToolResponse::new(
                msg.tool_call_id.clone().unwrap_or_default(),
                msg.content.clone().unwrap_or_default(),
            );
            Ok(GenaiMessage::from(response))
        }
        _ => Err(anyhow::anyhow!("Unknown message role: {}", msg.role)),
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
    Done,
}

pub struct StreamFinal {
    pub reasoning: Option<String>,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}
