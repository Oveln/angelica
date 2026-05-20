use serde::Deserialize;
use std::path::Path;

use crate::permission::PermissionConfig;

#[derive(Debug, Deserialize)]
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
    pub session: SessionConfig,
    #[serde(default)]
    pub permission: PermissionConfig,
}

impl Config {
    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&raw)?;
        Ok(config)
    }

    pub fn from_str(s: &str) -> anyhow::Result<Self> {
        let config: Config = toml::from_str(s)?;
        Ok(config)
    }

    pub fn resolve_paths(&mut self, base: &Path) {
        self.memory.agent_memory_path = Self::absolute_or(base, &self.memory.agent_memory_path);
        self.memory.user_profile_path = Self::absolute_or(base, &self.memory.user_profile_path);
        self.memory.soul_path = Self::absolute_or(base, &self.memory.soul_path);
        self.skills.directory = Self::absolute_or(base, &self.skills.directory);
        self.session.directory = Self::absolute_or(base, &self.session.directory);
        self.permission.approved_path = Self::absolute_or(base, &self.permission.approved_path);
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

impl Default for Config {
    fn default() -> Self {
        Self {
            llm: LlmConfig::default(),
            memory: MemoryConfig::default(),
            mcp: McpConfig::default(),
            skills: SkillsConfig::default(),
            session: SessionConfig::default(),
            permission: PermissionConfig::default(),
        }
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

#[derive(Debug, Deserialize)]
pub struct MemoryConfig {
    #[serde(default = "default_agent_memory_path")]
    pub agent_memory_path: String,
    #[serde(default = "default_user_profile_path")]
    pub user_profile_path: String,
    #[serde(default = "default_soul_path")]
    pub soul_path: String,
    #[serde(default = "default_max_file_size_kb")]
    pub max_file_size_kb: usize,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            agent_memory_path: default_agent_memory_path(),
            user_profile_path: default_user_profile_path(),
            soul_path: default_soul_path(),
            max_file_size_kb: default_max_file_size_kb(),
        }
    }
}

fn default_agent_memory_path() -> String {
    "data/agent_memory.md".to_string()
}
fn default_user_profile_path() -> String {
    "data/user_profile.md".to_string()
}
fn default_soul_path() -> String {
    "data/SOUL.md".to_string()
}
fn default_max_file_size_kb() -> usize {
    32
}

#[derive(Debug, Deserialize, Default)]
pub struct McpConfig {
    #[serde(default)]
    pub servers: std::collections::HashMap<String, McpServerConfig>,
}

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
pub struct SessionConfig {
    #[serde(default = "default_session_dir")]
    pub directory: String,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            directory: default_session_dir(),
        }
    }
}

fn default_session_dir() -> String {
    "data/sessions".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_default_config() {
        let config = Config::default();
        assert_eq!(config.llm.model, "deepseek-v4-flash");
        assert_eq!(config.llm.thinking, true);
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
agent_memory_path = "data/mem.md"
max_file_size_kb = 64
"#;
        let config = Config::from_str(toml).unwrap();
        assert_eq!(config.llm.model, "deepseek-v4-pro");
        assert_eq!(config.llm.thinking, false);
        assert_eq!(config.memory.max_file_size_kb, 64);
    }
}
