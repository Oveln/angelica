use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

use crate::permission::PermissionConfig;

#[derive(Debug, Deserialize, Clone, Default)]
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

    pub fn resolve_paths(&mut self, base: &Path) {
        self.memory.episodes_path = Self::absolute_or(base, &self.memory.episodes_path);
        self.memory.self_path = Self::absolute_or(base, &self.memory.self_path);
        self.memory.profiles_dir = Self::absolute_or(base, &self.memory.profiles_dir);
        self.memory.notebook_path = Self::absolute_or(base, &self.memory.notebook_path);
        self.skills.directory = Self::absolute_or(base, &self.skills.directory);
        self.permission.approved_path = Self::absolute_or(base, &self.permission.approved_path);
        self.state.path = Self::absolute_or(base, &self.state.path);
        self.state.conversation_path = Self::absolute_or(base, &self.state.conversation_path);
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

#[derive(Debug, Deserialize, Clone)]
pub struct LlmConfig {
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_true")]
    pub thinking: bool,
    #[serde(default = "default_reasoning_effort")]
    pub reasoning_effort: String,
    pub api_key: Option<String>,
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,
    #[serde(default)]
    pub role_immersion: Option<bool>,
    #[serde(default)]
    pub profiles: HashMap<String, ProfileConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProfileConfig {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
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

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            model: default_model(),
            base_url: default_base_url(),
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            thinking: true,
            reasoning_effort: default_reasoning_effort(),
            api_key: None,
            max_iterations: default_max_iterations(),
            role_immersion: None,
            profiles: HashMap::new(),
        }
    }
}

fn default_model() -> String {
    "deepseek-v4-flash".to_string()
}
fn default_base_url() -> String {
    "https://api.deepseek.com".to_string()
}
fn default_max_tokens() -> u32 {
    4096
}
fn default_temperature() -> f32 {
    0.7
}
fn default_reasoning_effort() -> String {
    "high".to_string()
}
fn default_true() -> bool {
    true
}
fn default_max_iterations() -> u32 {
    10
}

// ── Memory ──

#[derive(Debug, Clone, Deserialize)]
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
    "data/episodes.jsonl".to_string()
}
fn default_self_path() -> String {
    "data/SELF.md".to_string()
}
fn default_profiles_dir() -> String {
    "data/profiles".to_string()
}
fn default_notebook_path() -> String {
    "data/notebook.md".to_string()
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

#[derive(Debug, Clone, Deserialize)]
pub struct EmbeddingConfig {
    #[serde(default = "default_embed_provider")]
    pub provider: String,
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
            provider: default_embed_provider(),
            model: default_embed_model(),
            base_url: default_embed_base_url(),
            api_key_env: String::new(),
        }
    }
}

fn default_embed_provider() -> String {
    "ollama".to_string()
}
fn default_embed_model() -> String {
    "qwen3-embedding".to_string()
}
fn default_embed_base_url() -> String {
    "http://localhost:11434".to_string()
}

// ── MCP ──

#[derive(Debug, Deserialize, Default, Clone)]
pub struct McpConfig {
    #[serde(default)]
    pub servers: std::collections::HashMap<String, McpServerConfig>,
}

#[derive(Debug, Deserialize, Clone)]
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

#[derive(Debug, Deserialize, Clone)]
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

#[derive(Debug, Deserialize, Clone)]
pub struct StateConfig {
    #[serde(default = "default_state_path")]
    pub path: String,
    #[serde(default = "default_conversation_path")]
    pub conversation_path: String,
    #[serde(default)]
    #[allow(dead_code)]
    archive_dir: String,
    #[serde(default)]
    #[allow(dead_code)]
    max_snapshots: usize,
}

impl StateConfig {
    /// Parent directory of `conversation_path`, i.e. the data root.
    pub fn data_dir(&self) -> PathBuf {
        PathBuf::from(&self.conversation_path)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("data"))
    }
}

impl Default for StateConfig {
    fn default() -> Self {
        Self {
            path: default_state_path(),
            conversation_path: default_conversation_path(),
            archive_dir: String::new(),
            max_snapshots: 0,
        }
    }
}

fn default_state_path() -> String {
    "data/state.json".to_string()
}
fn default_conversation_path() -> String {
    "data/conversation.jsonl".to_string()
}

// ── Fatigue ──

#[derive(Debug, Deserialize, Clone)]
pub struct FatigueConfig {
    #[serde(default = "default_per_turn_base")]
    pub per_turn_base: f64,
    #[serde(default = "default_per_tool_call_ratio")]
    pub per_tool_call_ratio: f64,
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
            per_turn_base: default_per_turn_base(),
            per_tool_call_ratio: default_per_tool_call_ratio(),
            sleep_threshold: default_sleep_threshold(),
            can_sleep_threshold: default_can_sleep_threshold(),
            groggy_turns: default_groggy_turns(),
        }
    }
}

fn default_per_turn_base() -> f64 {
    0.015
}
fn default_per_tool_call_ratio() -> f64 {
    0.333
}
fn default_sleep_threshold() -> f64 {
    1.0
}
fn default_can_sleep_threshold() -> f64 {
    0.8
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
        assert_eq!(config.llm.model, "deepseek-v4-flash");
        assert!(config.llm.thinking);
        assert_eq!(config.embedding.provider, "ollama");
        assert_eq!(config.embedding.model, "qwen3-embedding");
    }

    #[test]
    fn parse_toml_config() {
        let toml = r#"
[llm]
model = "deepseek-v4-pro"
base_url = "https://api.deepseek.com"
max_tokens = 8192
thinking = false

[memory]
recent_threshold = 3

[embedding]
provider = "ollama"
model = "qwen3-embedding"
"#;
        let config = Config::parse_toml(toml).unwrap();
        assert_eq!(config.llm.model, "deepseek-v4-pro");
        assert!(!config.llm.thinking);
        assert_eq!(config.memory.recent_threshold, 3);
    }
}
