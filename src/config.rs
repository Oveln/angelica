use genai::adapter::AdapterKind;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::path::PathBuf;

use crate::permission::PermissionConfig;

const APP_NAME: &str = "angelica";

fn data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string())))
        .join(APP_NAME)
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string())))
        .join(APP_NAME)
        .join("config.toml")
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Config {
    #[serde(default)]
    pub llm: LlmConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
    #[serde(default)]
    pub embedding: EmbeddingConfig,
    #[serde(default)]
    pub mcp: McpConfig,
    #[serde(default)]
    pub skills: SkillsConfig,
    #[serde(default)]
    pub permission: PermissionConfig,
    #[serde(default)]
    pub state: StateConfig,
    #[serde(default)]
    pub fatigue: FatigueConfig,
}

impl Config {
    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&raw)?;
        Ok(config)
    }

    pub fn parse_toml(s: &str) -> anyhow::Result<Self> {
        Ok(toml::from_str(s)?)
    }

    pub fn load_or_create(cli_path: Option<PathBuf>) -> anyhow::Result<Self> {
        let config_path = match cli_path {
            Some(ref path) => {
                let abs = if path.is_absolute() {
                    path.clone()
                } else {
                    std::env::current_dir()?.join(path)
                };
                let mut cfg = Self::from_file(&abs)?;
                cfg.resolve_paths();
                return Ok(cfg);
            }
            None => config_path(),
        };

        if config_path.exists() {
            let mut cfg = Self::from_file(&config_path)?;
            cfg.resolve_paths();
            Ok(cfg)
        } else {
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut cfg = Self::default();
            std::fs::write(&config_path, toml::to_string_pretty(&cfg)?)?;
            tracing::info!("Created default config at {}", config_path.display());
            cfg.resolve_paths();
            Ok(cfg)
        }
    }

    pub fn resolve_paths(&mut self) {
        let base = data_dir();
        self.memory.episodes_path = Self::absolute_or(&base, &self.memory.episodes_path);
        self.memory.self_path = Self::absolute_or(&base, &self.memory.self_path);
        self.memory.profiles_dir = Self::absolute_or(&base, &self.memory.profiles_dir);
        self.memory.notebook_path = Self::absolute_or(&base, &self.memory.notebook_path);
        self.skills.directory = Self::absolute_or(&base, &self.skills.directory);
        self.permission.approved_path = Self::absolute_or(&base, &self.permission.approved_path);
        self.state.path = Self::absolute_or(&base, &self.state.path);
        self.state.conversation_path = Self::absolute_or(&base, &self.state.conversation_path);
    }

    fn absolute_or(base: &Path, path: &str) -> String {
        let p = Path::new(path);
        if p.is_absolute() {
            path.to_string()
        } else {
            base.join(p).to_string_lossy().to_string()
        }
    }
}

impl std::str::FromStr for Config {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_toml(s)
    }
}

// ── LLM ──

/// A single LLM provider configuration.
/// Each provider maps to a `genai::AdapterKind` which handles
/// endpoint resolution, auth, and protocol differences automatically.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProviderConfig {
    pub name: String,
    pub adapter: AdapterKind,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub thinking: Option<bool>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,
    #[serde(default)]
    pub role_immersion: Option<bool>,
    #[serde(default = "default_providers")]
    pub providers: Vec<ProviderConfig>,
    #[serde(default)]
    pub default_provider: Option<String>,
}

fn default_providers() -> Vec<ProviderConfig> {
    vec![ProviderConfig {
        name: "deepseek".to_string(),
        adapter: AdapterKind::DeepSeek,
        model: Some("deepseek-v4-flash".to_string()),
        base_url: None,
        api_key: None,
        max_tokens: None,
        temperature: None,
        thinking: None,
        reasoning_effort: None,
    }]
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            max_iterations: default_max_iterations(),
            role_immersion: None,
            providers: default_providers(),
            default_provider: None,
        }
    }
}

impl LlmConfig {
    pub fn default_model_name(&self) -> &str {
        let target_name = self.default_provider.as_deref();
        for p in &self.providers {
            if Some(p.name.as_str()) == target_name {
                return p.model.as_deref().unwrap_or("unknown");
            }
        }
        self.providers
            .first()
            .and_then(|p| p.model.as_deref())
            .unwrap_or("unknown")
    }
}

fn default_max_iterations() -> u32 {
    10
}

// ── Memory ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    #[serde(default = "default_episodes_path")]
    pub episodes_path: String,
    #[serde(default = "default_self_path")]
    pub self_path: String,
    #[serde(default = "default_profiles_dir")]
    pub profiles_dir: String,
    #[serde(default = "default_notebook_path")]
    pub notebook_path: String,
    #[serde(default = "default_max_file_size_kb")]
    pub max_file_size_kb: usize,
    #[serde(default = "default_recent_threshold")]
    pub recent_threshold: usize,
    #[serde(default = "default_episode_inject_budget")]
    pub episode_inject_budget: usize,
    #[serde(default = "default_recall_similarity_threshold")]
    pub recall_similarity_threshold: f32,
    #[serde(default = "default_recall_inject_threshold")]
    pub recall_inject_threshold: f32,
    #[serde(default = "default_recall_inject_probability")]
    pub recall_inject_probability: f32,
    #[serde(default = "default_self_hard_limit")]
    pub self_hard_limit: usize,
    #[serde(default = "default_profile_hard_limit")]
    pub profile_hard_limit: usize,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            episodes_path: default_episodes_path(),
            self_path: default_self_path(),
            profiles_dir: default_profiles_dir(),
            notebook_path: default_notebook_path(),
            max_file_size_kb: default_max_file_size_kb(),
            recent_threshold: default_recent_threshold(),
            episode_inject_budget: default_episode_inject_budget(),
            recall_similarity_threshold: default_recall_similarity_threshold(),
            recall_inject_threshold: default_recall_inject_threshold(),
            recall_inject_probability: default_recall_inject_probability(),
            self_hard_limit: default_self_hard_limit(),
            profile_hard_limit: default_profile_hard_limit(),
        }
    }
}

fn default_episodes_path() -> String {
    "episodes.jsonl".to_string()
}
fn default_self_path() -> String {
    "SELF.md".to_string()
}
fn default_profiles_dir() -> String {
    "profiles".to_string()
}
fn default_notebook_path() -> String {
    "notebook.md".to_string()
}
fn default_max_file_size_kb() -> usize {
    32
}
fn default_recent_threshold() -> usize {
    5
}
fn default_episode_inject_budget() -> usize {
    2
}
fn default_recall_similarity_threshold() -> f32 {
    0.6
}
fn default_recall_inject_threshold() -> f32 {
    0.7
}
fn default_recall_inject_probability() -> f32 {
    0.6
}
fn default_self_hard_limit() -> usize {
    8192
}
fn default_profile_hard_limit() -> usize {
    8192
}

// ── Embedding ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_embed_model")]
    pub model: String,
    #[serde(default = "default_embed_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub api_key_env: String,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            enabled: default_true(),
            model: default_embed_model(),
            base_url: default_embed_base_url(),
            api_key_env: String::new(),
        }
    }
}

fn default_embed_model() -> String {
    "qwen3-embedding".to_string()
}
fn default_true() -> bool {
    true
}
fn default_embed_base_url() -> String {
    "http://localhost:11434".to_string()
}

// ── MCP ──

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct McpConfig {
    #[serde(default)]
    pub servers: std::collections::HashMap<String, McpServerConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct McpServerConfig {
    #[serde(default = "default_stdio")]
    pub transport: String,
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    pub url: Option<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

fn default_stdio() -> String {
    "stdio".to_string()
}

// ── Skills ──

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SkillsConfig {
    #[serde(default = "default_skills_dir")]
    pub directory: String,
}

impl Default for SkillsConfig {
    fn default() -> Self {
        Self {
            directory: default_skills_dir(),
        }
    }
}

fn default_skills_dir() -> String {
    "skills".to_string()
}

// ── State ──

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StateConfig {
    #[serde(default = "default_state_path")]
    pub path: String,
    #[serde(default = "default_conversation_path")]
    pub conversation_path: String,
}

impl StateConfig {
    pub fn data_dir(&self) -> PathBuf {
        PathBuf::from(&self.conversation_path)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(data_dir)
    }
}

impl Default for StateConfig {
    fn default() -> Self {
        Self {
            path: default_state_path(),
            conversation_path: default_conversation_path(),
        }
    }
}

fn default_state_path() -> String {
    "state.json".to_string()
}
fn default_conversation_path() -> String {
    "conversation.jsonl".to_string()
}

// ── Fatigue ──

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FatigueConfig {
    #[serde(default = "default_max_context_tokens")]
    pub max_context_tokens: u64,
    #[serde(default = "default_curve_exponent")]
    pub curve_exponent: f64,
    #[serde(default = "default_sleep_threshold")]
    pub sleep_threshold: f64,
    #[serde(default = "default_can_sleep_threshold")]
    pub can_sleep_threshold: f64,
    #[serde(default = "default_groggy_turns")]
    pub groggy_turns: u32,
}

impl Default for FatigueConfig {
    fn default() -> Self {
        Self {
            max_context_tokens: default_max_context_tokens(),
            curve_exponent: default_curve_exponent(),
            sleep_threshold: default_sleep_threshold(),
            can_sleep_threshold: default_can_sleep_threshold(),
            groggy_turns: default_groggy_turns(),
        }
    }
}

fn default_max_context_tokens() -> u64 {
    120_000
}
fn default_curve_exponent() -> f64 {
    1.0
}
fn default_sleep_threshold() -> f64 {
    0.85
}
fn default_can_sleep_threshold() -> f64 {
    0.6
}
fn default_groggy_turns() -> u32 {
    3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_default_config() {
        let config = Config::default();
        assert_eq!(config.llm.providers.len(), 1);
        assert_eq!(config.llm.providers[0].name, "deepseek");
        assert_eq!(config.embedding.model, "qwen3-embedding");
    }

    #[test]
    fn parse_toml_config() {
        let toml = r#"
[llm]
max_iterations = 20

[memory]
recent_threshold = 3

[[llm.providers]]
name = "deepseek"
adapter = "DeepSeek"
model = "deepseek-v4-pro"
thinking = false
max_tokens = 8192
"#;
        let config = Config::parse_toml(toml).unwrap();
        assert_eq!(config.llm.providers.len(), 1);
        assert!(!config.llm.providers[0].thinking.unwrap());
        assert_eq!(config.llm.providers[0].max_tokens.unwrap(), 8192);
        assert_eq!(config.memory.recent_threshold, 3);
    }

    #[test]
    fn parse_multi_provider_config() {
        let toml = r#"
[[llm.providers]]
name = "deepseek"
adapter = "DeepSeek"
model = "deepseek-v4-flash"

[[llm.providers]]
name = "openai"
adapter = "OpenAI"
model = "gpt-4o"

[[llm.providers]]
name = "groq"
adapter = "Groq"
base_url = "https://api.groq.com/openai/v1"
model = "llama-3.1-70b-versatile"
"#;
        let config = Config::parse_toml(toml).unwrap();
        assert_eq!(config.llm.providers.len(), 3);
        assert_eq!(config.llm.providers[0].name, "deepseek");
        assert_eq!(config.llm.providers[1].name, "openai");
        assert_eq!(config.llm.providers[2].name, "groq");
    }

    #[test]
    fn default_model_name_resolution() {
        let toml = r#"
[llm]
default_provider = "openai"

[[llm.providers]]
name = "deepseek"
adapter = "DeepSeek"
model = "deepseek-v4-flash"

[[llm.providers]]
name = "openai"
adapter = "OpenAI"
model = "gpt-4o"
"#;
        let config = Config::parse_toml(toml).unwrap();
        assert_eq!(config.llm.default_model_name(), "gpt-4o");
    }
}
