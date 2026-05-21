use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

use crate::permission::PermissionConfig;

#[derive(Debug, Deserialize, Clone, Default)]
pub struct Config {
    #[serde(default)]
    pub llm: LlmConfig,
    #[serde(default)]
    pub memory: MemoryConfig,
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
        self.memory.memory_path = Self::absolute_or(base, &self.memory.memory_path);
        self.memory.profile_path = Self::absolute_or(base, &self.memory.profile_path);
        self.memory.soul_path = Self::absolute_or(base, &self.memory.soul_path);
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
#[derive(Debug, Clone)]
pub struct MemoryConfig {
    pub memory_path: String,
    pub profile_path: String,
    pub soul_path: String,
    pub notebook_path: String,
    pub max_file_size_kb: usize,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            memory_path: default_memory_path(),
            profile_path: default_profile_path(),
            soul_path: default_soul_path(),
            notebook_path: default_notebook_path(),
            max_file_size_kb: default_max_file_size_kb(),
        }
    }
}

fn default_memory_path() -> String {
    "data/MEMORY.md".to_string()
}
fn default_profile_path() -> String {
    "data/profile.md".to_string()
}
fn default_soul_path() -> String {
    "data/SOUL.md".to_string()
}
fn default_notebook_path() -> String {
    "data/notebook.md".to_string()
}
fn default_max_file_size_kb() -> usize {
    32
}

// Custom Deserialize to support old config keys (agent_memory_path, user_profile_path)
impl<'de> serde::Deserialize<'de> for MemoryConfig {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct RawMemory {
            #[serde(default = "default_memory_path")]
            memory_path: String,
            #[serde(default = "default_profile_path")]
            profile_path: String,
            #[serde(default = "default_soul_path")]
            soul_path: String,
            #[serde(default = "default_notebook_path")]
            notebook_path: String,
            #[serde(default = "default_max_file_size_kb")]
            max_file_size_kb: usize,
            #[serde(default = "default_memory_path")]
            agent_memory_path: String,
            #[serde(default = "default_profile_path")]
            user_profile_path: String,
        }
        let raw = RawMemory::deserialize(d)?;
        Ok(MemoryConfig {
            memory_path: if raw.memory_path == default_memory_path()
                && raw.agent_memory_path != default_memory_path()
            {
                raw.agent_memory_path
            } else {
                raw.memory_path
            },
            profile_path: if raw.profile_path == default_profile_path()
                && raw.user_profile_path != default_profile_path()
            {
                raw.user_profile_path
            } else {
                raw.profile_path
            },
            soul_path: raw.soul_path,
            notebook_path: raw.notebook_path,
            max_file_size_kb: raw.max_file_size_kb,
        })
    }
}

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

#[derive(Debug, Deserialize, Clone)]
pub struct StateConfig {
    #[serde(default = "default_state_path")]
    pub path: String,
    #[serde(default = "default_conversation_path")]
    pub conversation_path: String,
    // Legacy fields kept for backward compat with old config files
    #[serde(default)]
    #[allow(dead_code)]
    archive_dir: String,
    #[serde(default)]
    #[allow(dead_code)]
    max_snapshots: usize,
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
memory_path = "data/mem.md"
max_file_size_kb = 64
"#;
        let config = Config::parse_toml(toml).unwrap();
        assert_eq!(config.llm.model, "deepseek-v4-pro");
        assert!(!config.llm.thinking);
        assert_eq!(config.memory.max_file_size_kb, 64);
    }

    #[test]
    fn parse_old_memory_keys() {
        let toml = r#"
[memory]
agent_memory_path = "data/old_memory.md"
user_profile_path = "data/old_profile.md"
"#;
        let config = Config::parse_toml(toml).unwrap();
        assert_eq!(config.memory.memory_path, "data/old_memory.md");
        assert_eq!(config.memory.profile_path, "data/old_profile.md");
    }
}
