pub mod patch;
pub mod types;

use futures::StreamExt;
use genai::Client;
use genai::adapter::AdapterKind;
use genai::chat::{
    ChatMessage as GenaiMessage, ChatOptions, ChatRequest, ChatStreamEvent, ContentPart,
    MessageContent, ReasoningEffort, StreamChunk, Tool as GenaiTool, ToolCall as GenaiToolCall,
    ToolResponse,
};
use genai::resolver::{AuthData, AuthResolver, Endpoint, ServiceTargetResolver};
use std::collections::HashMap;
use std::sync::Arc;

use crate::config::{LlmConfig, ProviderConfig};
use crate::llm::types::*;
use crate::usage::UsageMetrics;

/// Resolved profile: a concrete set of parameters for a single LLM provider.
#[derive(Clone)]
struct ResolvedProfile {
    model: String,
    thinking: bool,
    temperature: f32,
    max_tokens: u32,
    reasoning_effort: String,
    adapter_kind: AdapterKind,
    provider_name: String,
}

pub struct LlmClient {
    /// Map from provider name to its genai Client.
    clients: HashMap<String, Arc<Client>>,
    /// Map from provider name to its resolved profile.
    profiles: HashMap<String, ResolvedProfile>,
    /// Default profile (first provider, or the named default_provider).
    default: ResolvedProfile,
}

impl LlmClient {
    pub fn new(config: &LlmConfig) -> anyhow::Result<Self> {
        let mut clients = HashMap::new();
        let mut profiles = HashMap::new();

        // Build clients from configured providers.
        if !config.providers.is_empty() {
            for pc in &config.providers {
                let api_key = resolve_api_key(pc);
                let model = pc.model.clone().unwrap_or_else(|| "deepseek-v4-flash".to_string());

                if !clients.contains_key(&pc.name) {
                    let client = build_client(
                        &pc.adapter,
                        &api_key,
                        pc.base_url.as_deref(),
                    );
                    clients.insert(pc.name.clone(), client);
                }

                profiles.insert(
                    pc.name.clone(),
                    ResolvedProfile {
                        model,
                        thinking: pc.thinking.unwrap_or(true),
                        temperature: pc.temperature.unwrap_or(0.7),
                        max_tokens: pc.max_tokens.unwrap_or(4096),
                        reasoning_effort: pc.reasoning_effort.clone().unwrap_or_else(|| "high".to_string()),
                        adapter_kind: pc.adapter,
                        provider_name: pc.name.clone(),
                    },
                );
            }
        }

        // Select default profile: explicit default_provider > first provider.
        if profiles.is_empty() {
            anyhow::bail!(
                "No [[llm.providers]] configured. \
                 Please add at least one provider to config.toml."
            );
        }

        let first_name = config.providers[0].name.clone();
        let default_name = config.default_provider.as_deref().unwrap_or(&first_name);
        let profile = match profiles.get(default_name) {
            Some(p) => p,
            None => {
                tracing::warn!(
                    "default_provider '{}' not found, using first provider", default_name
                );
                profiles.get(&first_name).expect("first profile exists")
            }
        };
        let default_profile = profile.clone();

        Ok(Self {
            clients,
            profiles,
            default: default_profile,
        })
    }

    pub async fn complete(
        &self,
        messages: &[ChatMessage],
        options: RequestOptions,
    ) -> anyhow::Result<LlmResponse> {
        let profile = self.resolve_profile(options.profile.as_deref());
        let client = self.clients.get(&profile.provider_name).unwrap();
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
            usage: Some(convert_usage(response.usage)),
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
        let client = self.clients.get(&profile.provider_name).unwrap().clone();
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
            let mut usage: Option<UsageMetrics> = None;

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
                        usage = end.captured_usage.map(convert_usage);
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
                usage,
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

/// Build a genai Client for a specific adapter.
/// If `base_url` is provided, it overrides the adapter's default endpoint.
fn build_client(
    adapter: &AdapterKind,
    api_key: &str,
    base_url: Option<&str>,
) -> Arc<Client> {
    let key = api_key.to_string();
    let owned_base_url = base_url.map(|s| s.to_string());

    let mut builder = Client::builder()
        .with_adapter_kind(*adapter)
        .with_auth_resolver(AuthResolver::from_resolver_fn(move |_kind| {
            Ok(Some(AuthData::from_single(key.clone())))
        }));

    if let Some(url) = owned_base_url {
        builder = builder.with_service_target_resolver(
            ServiceTargetResolver::from_resolver_fn(
                move |mut target: genai::ServiceTarget| {
                    target.endpoint = Endpoint::from_owned(url.clone());
                    Ok(target)
                },
            ),
        );
    }

    builder.build().into()
}

/// Resolve the API key for a provider: direct key > env var > empty.
fn resolve_api_key(pc: &ProviderConfig) -> String {
    pc.api_key
        .clone()
        .or_else(|| {
            pc.adapter
                .default_key_env_name()
                .and_then(|env| std::env::var(env).ok())
        })
        .unwrap_or_default()
}


/// Build ChatOptions, respecting provider-specific parameter requirements.
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
        .with_capture_usage(true)
        .with_capture_content(true)
        .with_capture_reasoning_content(true)
        .with_capture_tool_calls(true);

    if thinking {
        // Only DeepSeek supports reasoning_effort in ChatOptions.
        // Other providers (OpenAI, Groq, etc.) ignore it or handle it differently.
        if profile.adapter_kind == AdapterKind::DeepSeek {
            let effort = match reasoning_effort_override.unwrap_or(&profile.reasoning_effort) {
                "low" => ReasoningEffort::Low,
                "medium" => ReasoningEffort::Medium,
                _ => ReasoningEffort::High,
            };
            opts = opts.with_reasoning_effort(effort);
        }
    } else {
        // Only DeepSeek needs the "thinking: disabled" extra_body.
        // Other providers don't understand this parameter.
        if profile.adapter_kind == AdapterKind::DeepSeek {
            opts = opts.with_extra_body(serde_json::json!({ "thinking": { "type": "disabled" } }));
        }
        opts = opts.with_temperature(temperature as f64);
    }

    opts
}

fn convert_usage(usage: genai::chat::Usage) -> UsageMetrics {
    let prompt_tokens = usage.prompt_tokens.unwrap_or_default().max(0) as u64;
    let completion_tokens = usage.completion_tokens.unwrap_or_default().max(0) as u64;
    let total_tokens = usage
        .total_tokens
        .unwrap_or_else(|| {
            usage.prompt_tokens.unwrap_or_default() + usage.completion_tokens.unwrap_or_default()
        })
        .max(0) as u64;
    let reasoning_tokens = usage
        .completion_tokens_details
        .as_ref()
        .and_then(|d| d.reasoning_tokens)
        .unwrap_or_default()
        .max(0) as u64;
    let cache_hit_tokens = usage
        .prompt_tokens_details
        .as_ref()
        .and_then(|d| d.cached_tokens)
        .unwrap_or_default()
        .max(0) as u64;
    let cache_miss_tokens = prompt_tokens.saturating_sub(cache_hit_tokens);

    UsageMetrics {
        prompt_tokens,
        completion_tokens,
        total_tokens,
        reasoning_tokens,
        cache_hit_tokens,
        cache_miss_tokens,
    }
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
    match msg.role {
        Role::System => Ok(GenaiMessage::system(
            msg.content.clone().unwrap_or_default(),
        )),
        Role::User => Ok(GenaiMessage::user(msg.content.clone().unwrap_or_default())),
        Role::Assistant => {
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
        Role::Tool => {
            let response = ToolResponse::new(
                msg.tool_call_id.clone().unwrap_or_default(),
                msg.content.clone().unwrap_or_default(),
            );
            Ok(GenaiMessage::from(response))
        }
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
    pub usage: Option<UsageMetrics>,
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
