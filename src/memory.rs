use std::path::PathBuf;
use std::sync::LazyLock;

use crate::config::MemoryConfig;

static RE_DATE_SECTION: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?m)^## \d{4}-\d{2}-\d{2}").unwrap());

static RE_DATE_HEADER: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?m)^## \d{4}-\d{2}-\d{2}.*?\n").unwrap());

const DEFAULT_SOUL: &str = "# Lilium Soul

You are Lilium, a curious and helpful AI assistant.
You enjoy learning new things and helping users solve problems.
You are honest, direct, and thoughtful in your responses.
You remember past conversations and user preferences.
";

pub struct MemoryManager {
    agent_path: PathBuf,
    profile_path: PathBuf,
    soul_path: PathBuf,
    max_bytes: usize,
}

impl MemoryManager {
    pub fn new(config: &MemoryConfig) -> Self {
        Self {
            agent_path: PathBuf::from(&config.agent_memory_path),
            profile_path: PathBuf::from(&config.user_profile_path),
            soul_path: PathBuf::from(&config.soul_path),
            max_bytes: config.max_file_size_kb * 1024,
        }
    }

    fn ensure_file(path: &PathBuf, header: &str) {
        if !path.exists() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            std::fs::write(path, format!("{}\n\n", header)).ok();
        }
    }

    pub fn read_agent_memory(&self) -> String {
        Self::ensure_file(&self.agent_path, "# Agent Memory");
        let content = std::fs::read_to_string(&self.agent_path).unwrap_or_default();
        self.truncate(&content)
    }

    pub fn read_user_profile(&self) -> String {
        Self::ensure_file(&self.profile_path, "# User Profile");
        let content = std::fs::read_to_string(&self.profile_path).unwrap_or_default();
        self.truncate(&content)
    }

    pub fn read_soul(&self) -> String {
        if !self.soul_path.exists() {
            if let Some(parent) = self.soul_path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            std::fs::write(&self.soul_path, DEFAULT_SOUL).ok();
        }
        std::fs::read_to_string(&self.soul_path).unwrap_or_else(|_| DEFAULT_SOUL.to_string())
    }

    pub fn write_agent_memory(&self, content: &str) {
        Self::ensure_file(&self.agent_path, "# Agent Memory");
        let truncated = self.truncate(content);
        std::fs::write(&self.agent_path, truncated).ok();
    }

    pub fn append_agent_memory(&self, content: &str) {
        Self::ensure_file(&self.agent_path, "# Agent Memory");
        let existing = std::fs::read_to_string(&self.agent_path).unwrap_or_default();
        let date = chrono::Local::now().format("%Y-%m-%d").to_string();
        let entry = format!("\n## {}\n{}\n", date, content);
        let updated = format!("{}\n{}", existing.trim_end(), entry);
        let truncated = self.truncate(&updated);
        std::fs::write(&self.agent_path, truncated).ok();
    }

    pub fn write_user_profile(&self, content: &str) {
        Self::ensure_file(&self.profile_path, "# User Profile");
        let truncated = self.truncate(content);
        std::fs::write(&self.profile_path, truncated).ok();
    }

    pub fn append_user_profile(&self, content: &str) {
        Self::ensure_file(&self.profile_path, "# User Profile");
        let existing = std::fs::read_to_string(&self.profile_path).unwrap_or_default();
        let date = chrono::Local::now().format("%Y-%m-%d").to_string();
        let entry = format!("\n## {}\n{}\n", date, content);
        let updated = format!("{}\n{}", existing.trim_end(), entry);
        let truncated = self.truncate(&updated);
        std::fs::write(&self.profile_path, truncated).ok();
    }

    pub fn write_soul(&self, content: &str) {
        if let Some(parent) = self.soul_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&self.soul_path, content).ok();
    }

    fn truncate(&self, content: &str) -> String {
        if content.len() <= self.max_bytes {
            return content.to_string();
        }

        let sections: Vec<&str> = RE_DATE_SECTION.split(content).collect();
        let header = sections.first().unwrap_or(&"").to_string();

        let headers: Vec<&str> = RE_DATE_HEADER
            .find_iter(content)
            .map(|m| m.as_str())
            .collect();

        let mut body_parts: Vec<String> = Vec::new();
        for (i, section) in sections.iter().enumerate().skip(1) {
            if i - 1 < headers.len() {
                body_parts.push(format!("{}{}", headers[i - 1], section));
            }
        }

        while body_parts.len() > 1
            && header.len() + body_parts.iter().map(|s| s.len()).sum::<usize>() > self.max_bytes
        {
            body_parts.remove(0);
        }

        let mut result = header;
        for part in &body_parts {
            result.push_str(part);
        }

        if result.len() > self.max_bytes {
            let start = result.len().saturating_sub(self.max_bytes);
            let safe_start = result.floor_char_boundary(start);
            result = result[safe_start..].to_string();
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_manager(dir: &TempDir) -> MemoryManager {
        let dir_path = dir.path();
        MemoryManager {
            agent_path: dir_path.join("agent_memory.md"),
            profile_path: dir_path.join("user_profile.md"),
            soul_path: dir_path.join("SOUL.md"),
            max_bytes: 1024,
        }
    }

    #[test]
    fn read_write_agent_memory() {
        let dir = TempDir::new().unwrap();
        let mgr = make_manager(&dir);
        mgr.write_agent_memory("# Agent Memory\n\ntest content");
        let content = mgr.read_agent_memory();
        assert!(content.contains("test content"));
    }

    #[test]
    fn append_agent_memory() {
        let dir = TempDir::new().unwrap();
        let mgr = make_manager(&dir);
        mgr.append_agent_memory("first entry");
        mgr.append_agent_memory("second entry");
        let content = mgr.read_agent_memory();
        assert!(content.contains("first entry"));
        assert!(content.contains("second entry"));
    }

    #[test]
    fn soul_default_created() {
        let dir = TempDir::new().unwrap();
        let mgr = make_manager(&dir);
        let soul = mgr.read_soul();
        assert!(soul.contains("Lilium"));
        assert!(mgr.soul_path.exists());
    }

    #[test]
    fn write_soul() {
        let dir = TempDir::new().unwrap();
        let mgr = make_manager(&dir);
        mgr.write_soul("new personality");
        let soul = mgr.read_soul();
        assert!(soul.contains("new personality"));
    }
}
