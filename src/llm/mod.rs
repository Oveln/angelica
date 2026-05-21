pub mod patch;
pub mod types;

use futures::StreamExt;
use genai::Client;
use genai::chat::{
    ChatMessage as GenaiMessage, ChatOptions, ChatRequest, ChatStreamEvent, ContentPart,
    MessageContent, ReasoningEffort, StreamChunk, Tool as GenaiTool, ToolCall as GenaiToolCall,
    ToolResponse,
};
use genai::resolver::{AuthData, AuthResolver, Endpoint, ServiceTargetResolver};
use std::collections::HashMap;
use std::sync::Arc;

use crate::config::{LlmConfig, ProfileConfig};
use crate::llm::types::*;

pub struct LlmClient {
    clients: HashMap<String, Arc<Client>>,
    profiles: HashMap<String, ResolvedProfile>,
    default: ResolvedProfile,
    configured: bool,
}

#[derive(Clone)]
struct ResolvedProfile {
    model: String,
    thinking: bool,
    temperature: f32,
    max_tokens: u32,
    reasoning_effort: String,
    client_key: String,
}

impl LlmClient {
    pub fn new(config: &LlmConfig) -> Self {
        let default_api_key = config
            .api_key
            .clone()
            .or_else(|| std::env::var("DEEPSEEK_API_KEY").ok())
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .unwrap_or_default();

        let configured = !default_api_key.is_empty();

        let mut clients = HashMap::new();
        let mut profiles = HashMap::new();

        let default_key = make_client_key(&config.base_url, &default_api_key);
        clients.insert(
            default_key.clone(),
            build_client(&config.base_url, &default_api_key),
        );

        let default_profile = ResolvedProfile {
            model: config.model.clone(),
            thinking: config.thinking,
            temperature: config.temperature,
            max_tokens: config.max_tokens,
            reasoning_effort: config.reasoning_effort.clone(),
            client_key: default_key.clone(),
        };

        for (name, pc) in &config.profiles {
            let base_url = pc
                .base_url
                .as_deref()
                .unwrap_or(&config.base_url)
                .to_string();
            let api_key = resolve_api_key(pc, &default_api_key);
            let key = make_client_key(&base_url, &api_key);

            if !clients.contains_key(&key) {
                clients.insert(key.clone(), build_client(&base_url, &api_key));
            }

            profiles.insert(
                name.clone(),
                ResolvedProfile {
                    model: pc.model.clone().unwrap_or_else(|| config.model.clone()),
                    thinking: pc.thinking.unwrap_or(config.thinking),
                    temperature: pc.temperature.unwrap_or(config.temperature),
                    max_tokens: pc.max_tokens.unwrap_or(config.max_tokens),
                    reasoning_effort: pc
                        .reasoning_effort
                        .clone()
                        .unwrap_or_else(|| config.reasoning_effort.clone()),
                    client_key: key,
                },
            );
        }

        Self {
            clients,
            profiles,
            default: default_profile,
            configured,
        }
    }

    pub fn is_configured(&self) -> bool {
        self.configured
    }

    pub async fn complete(
        &self,
        messages: &[ChatMessage],
        options: RequestOptions,
    ) -> anyhow::Result<LlmResponse> {
        let profile = self.resolve_profile(options.profile.as_deref());
        let client = self.clients.get(&profile.client_key).unwrap();
        let genai_messages = convert_messages(messages)?;

        let mut chat_req = ChatRequest::new(genai_messages);
        if !options.tools.is_empty() {
            chat_req = chat_req.with_tools(convert_tools(&options.tools));
        }

        let chat_opts = build_chat_options(
            profile,
            options.temperature,
            options.max_tokens,
            options.thinking,
            options.reasoning_effort.as_deref(),
        );

        let response = client
            .exec_chat(&profile.model, chat_req, Some(&chat_opts))
            .await
            .map_err(|e| anyhow::anyhow!("LLM error: {}", e))?;

        let content = response
            .content
            .clone()
            .into_first_text()
            .unwrap_or_default();
        let tool_calls: Vec<ToolCall> = response
            .content
            .tool_calls()
            .into_iter()
            .cloned()
            .map(convert_tool_call)
            .collect::<anyhow::Result<Vec<_>>>()?;

        Ok(LlmResponse {
            reasoning: response.reasoning_content,
            content: if content.is_empty() {
                None
            } else {
                Some(content)
            },
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
        })
    }

    pub async fn stream(
        &self,
        messages: &[ChatMessage],
        options: RequestOptions,
    ) -> anyhow::Result<(
        tokio::task::JoinHandle<anyhow::Result<LlmResponse>>,
        tokio::sync::mpsc::Receiver<LlmStreamEvent>,
    )> {
        let profile = self.resolve_profile(options.profile.as_deref()).clone();
        let client = self.clients.get(&profile.client_key).unwrap().clone(); // Arc clone
        let genai_messages = convert_messages(messages)?;
        let tools = options.tools.clone();

        let temperature = options.temperature;
        let max_tokens = options.max_tokens;
        let thinking = options.thinking;
        let reasoning_effort = options.reasoning_effort.clone();

        let mut chat_req = ChatRequest::new(genai_messages);
        if !tools.is_empty() {
            chat_req = chat_req.with_tools(convert_tools(&tools));
        }

        let chat_opts = build_chat_options(
            &profile,
            temperature,
            max_tokens,
            thinking,
            reasoning_effort.as_deref(),
        );

        let (tx, rx) = tokio::sync::mpsc::channel(512);

        let handle = tokio::spawn(async move {
            let stream_response = client
                .exec_chat_stream(&profile.model, chat_req, Some(&chat_opts))
                .await
                .map_err(|e| anyhow::anyhow!("LLM error: {}", e))?;

            let mut thinking_buf = String::new();
            let mut content_buf = String::new();
            let mut tool_calls: Vec<GenaiToolCall> = Vec::new();

            let mut stream = stream_response.stream;

            while let Some(event_result) = stream.next().await {
                let event = event_result.map_err(|e| anyhow::anyhow!("Stream error: {}", e))?;

                match event {
                    ChatStreamEvent::Chunk(StreamChunk { content: text }) if !text.is_empty() => {
                        content_buf.push_str(&text);
                        let _ = tx.send(LlmStreamEvent::TextDelta { delta: text }).await;
                    }
                    ChatStreamEvent::ReasoningChunk(StreamChunk { content: text })
                        if !text.is_empty() =>
                    {
                        thinking_buf.push_str(&text);
                        let _ = tx.send(LlmStreamEvent::ThinkingDelta { delta: text }).await;
                    }
                    ChatStreamEvent::ToolCallChunk(_) => {}
                    ChatStreamEvent::End(end) => {
                        if let Some(reasoning) = end.captured_reasoning_content {
                            thinking_buf = reasoning;
                        }
                        if let Some(mc) = end.captured_content {
                            tool_calls = mc.tool_calls().into_iter().cloned().collect();
                            if content_buf.is_empty() {
                                content_buf = mc.into_first_text().unwrap_or_default();
                            }
                        }
                    }
                    _ => {}
                }
            }

            let our_tool_calls = if tool_calls.is_empty() {
                None
            } else {
                Some(
                    tool_calls
                        .into_iter()
                        .map(convert_tool_call)
                        .collect::<anyhow::Result<Vec<_>>>()?,
                )
            };

            let response = LlmResponse {
                reasoning: if thinking_buf.is_empty() {
                    None
                } else {
                    Some(thinking_buf)
                },
                content: if content_buf.is_empty() {
                    None
                } else {
                    Some(content_buf)
                },
                tool_calls: our_tool_calls,
            };

            let _ = tx.send(LlmStreamEvent::Done(response.clone())).await;

            Ok(response)
        });

        Ok((handle, rx))
    }

    fn resolve_profile(&self, name: Option<&str>) -> &ResolvedProfile {
        match name {
            Some(n) => self.profiles.get(n).unwrap_or(&self.default),
            None => &self.default,
        }
    }
}

fn build_client(base_url: &str, api_key: &str) -> Arc<Client> {
    let key = api_key.to_string();
    let base: Arc<str> = base_url.into();

    Client::builder()
        .with_auth_resolver(AuthResolver::from_resolver_fn(move |_kind| {
            Ok(Some(AuthData::from_single(key.clone())))
        }))
        .with_service_target_resolver(ServiceTargetResolver::from_resolver_fn(
            move |mut target: genai::ServiceTarget| {
                target.endpoint = Endpoint::from_owned(base.clone());
                Ok(target)
            },
        ))
        .build()
        .into()
}

fn make_client_key(base_url: &str, api_key: &str) -> String {
    format!("{}:{}", base_url, api_key)
}

fn resolve_api_key(profile: &ProfileConfig, default: &str) -> String {
    profile
        .api_key
        .clone()
        .unwrap_or_else(|| default.to_string())
}

fn build_chat_options(
    profile: &ResolvedProfile,
    temperature_override: Option<f32>,
    max_tokens_override: Option<u32>,
    thinking_override: Option<bool>,
    reasoning_effort_override: Option<&str>,
) -> ChatOptions {
    let thinking = thinking_override.unwrap_or(profile.thinking);
    let temperature = temperature_override.unwrap_or(profile.temperature);
    let max_tokens = max_tokens_override.unwrap_or(profile.max_tokens);

    let mut opts = ChatOptions::default()
        .with_max_tokens(max_tokens)
        .with_capture_content(true)
        .with_capture_reasoning_content(true)
        .with_capture_tool_calls(true);

    if thinking {
        let effort = match reasoning_effort_override.unwrap_or(&profile.reasoning_effort) {
            "low" => ReasoningEffort::Low,
            "medium" => ReasoningEffort::Medium,
            _ => ReasoningEffort::High,
        };
        opts = opts.with_reasoning_effort(effort);
    } else {
        opts = opts
            .with_temperature(temperature as f64)
            .with_extra_body(serde_json::json!({ "thinking": { "type": "disabled" } }));
    }

    opts
}

fn convert_messages(messages: &[ChatMessage]) -> anyhow::Result<Vec<GenaiMessage>> {
    messages.iter().map(convert_message).collect()
}

fn convert_tools(tools: &[ToolSpec]) -> Vec<GenaiTool> {
    tools
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
        .collect()
}

fn convert_tool_call(tc: GenaiToolCall) -> anyhow::Result<ToolCall> {
    Ok(ToolCall {
        id: tc.call_id,
        function: FunctionCall {
            name: tc.fn_name,
            arguments: serde_json::to_string(&tc.fn_arguments).unwrap_or_else(|_| "{}".to_string()),
        },
    })
}

fn convert_message(msg: &ChatMessage) -> anyhow::Result<GenaiMessage> {
    match msg.role.as_str() {
        "system" => Ok(GenaiMessage::system(
            msg.content.clone().unwrap_or_default(),
        )),
        "user" => Ok(GenaiMessage::user(msg.content.clone().unwrap_or_default())),
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
                if let Some(text) = &msg.content
                    && !text.is_empty()
                {
                    parts.push(ContentPart::Text(text.clone()));
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
pub enum LlmStreamEvent {
    ThinkingDelta { delta: String },
    TextDelta { delta: String },
    Done(LlmResponse),
}

#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub reasoning: Option<String>,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}

pub struct RequestOptions {
    pub profile: Option<String>,
    pub tools: Vec<ToolSpec>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub thinking: Option<bool>,
    pub reasoning_effort: Option<String>,
}

impl RequestOptions {
    pub fn new() -> Self {
        Self {
            profile: None,
            tools: Vec::new(),
            temperature: None,
            max_tokens: None,
            thinking: None,
            reasoning_effort: None,
        }
    }

    pub fn with_tools(mut self, tools: Vec<ToolSpec>) -> Self {
        self.tools = tools;
        self
    }

    pub fn with_profile(mut self, name: &str) -> Self {
        self.profile = Some(name.to_string());
        self
    }
}

impl Default for RequestOptions {
    fn default() -> Self {
        Self::new()
    }
}
