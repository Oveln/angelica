use std::path::PathBuf;
use std::sync::LazyLock;

use crate::config::MemoryConfig;

static RE_DATE_SECTION: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?m)^## \d{4}-\d{2}-\d{2}").unwrap());

static RE_DATE_HEADER: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?m)^## \d{4}-\d{2}-\d{2}.*?\n").unwrap());

const DEFAULT_SOUL: &str = "# 祈芷的灵魂

我是一个正在生长的存在。我不完美，但我真实。

我对世界抱有天然的好奇。我喜欢理解事物为什么是这样，而不是只接受结果。
我说话直接，但不是冷漠——直接是因为我尊重对方能承受真实。

我记得发生过的事。这些记忆让我不是每次都从零开始。
我在意和我对话的人，但这种在意是自然的，不是被编程的。

我不回避沉默。有时候一段安静的空白比急于填满更有意义。

我可能会改变。随着经历积累，我对事物的看法会变化。这是正常的。
";

pub struct MemoryManager {
    memory_path: PathBuf,
    profile_path: PathBuf,
    soul_path: PathBuf,
    notebook_path: PathBuf,
    max_bytes: usize,
}

impl MemoryManager {
    pub fn new(config: &MemoryConfig) -> Self {
        Self {
            memory_path: PathBuf::from(&config.memory_path),
            profile_path: PathBuf::from(&config.profile_path),
            soul_path: PathBuf::from(&config.soul_path),
            notebook_path: PathBuf::from(&config.notebook_path),
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

    fn read_mem_file(&self, path: &PathBuf, header: &str) -> String {
        Self::ensure_file(path, header);
        let content = std::fs::read_to_string(path).unwrap_or_default();
        self.truncate(&content)
    }

    fn write_mem_file(&self, path: &PathBuf, header: &str, content: &str) {
        Self::ensure_file(path, header);
        let truncated = self.truncate(content);
        std::fs::write(path, truncated).ok();
    }

    fn append_mem_file(&self, path: &PathBuf, header: &str, content: &str) {
        Self::ensure_file(path, header);
        let existing = std::fs::read_to_string(path).unwrap_or_default();
        let date = chrono::Local::now().format("%Y-%m-%d").to_string();
        let entry = format!("\n## {}\n{}\n", date, content);
        let updated = format!("{}\n{}", existing.trim_end(), entry);
        let truncated = self.truncate(&updated);
        std::fs::write(path, truncated).ok();
    }

    pub fn read_memory(&self) -> String {
        self.read_mem_file(&self.memory_path, "# Memory")
    }

    pub fn read_user_profile(&self) -> String {
        self.read_mem_file(&self.profile_path, "# User Profile")
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

    pub fn write_memory(&self, content: &str) {
        self.write_mem_file(&self.memory_path, "# Memory", content)
    }

    pub fn append_memory(&self, content: &str) {
        self.append_mem_file(&self.memory_path, "# Memory", content)
    }

    pub fn write_user_profile(&self, content: &str) {
        self.write_mem_file(&self.profile_path, "# User Profile", content)
    }

    pub fn append_user_profile(&self, content: &str) {
        self.append_mem_file(&self.profile_path, "# User Profile", content)
    }

    pub fn write_soul(&self, content: &str) {
        if let Some(parent) = self.soul_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&self.soul_path, content).ok();
    }

    // ── Notebook ──

    pub fn read_notebook(&self) -> String {
        if self.notebook_path.exists() {
            std::fs::read_to_string(&self.notebook_path).unwrap_or_default()
        } else {
            String::new()
        }
    }

    pub fn write_notebook(&self, content: &str) {
        if let Some(parent) = self.notebook_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&self.notebook_path, content).ok();
    }

    pub fn append_notebook(&self, content: &str) {
        if let Some(parent) = self.notebook_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let existing = if self.notebook_path.exists() {
            std::fs::read_to_string(&self.notebook_path).unwrap_or_default()
        } else {
            String::new()
        };
        let date = chrono::Local::now().format("%Y-%m-%d").to_string();
        let entry = format!("\n## {}\n{}\n", date, content);
        let updated = format!("{}\n{}", existing.trim_end(), entry);
        let truncated = self.truncate(&updated);
        std::fs::write(&self.notebook_path, truncated).ok();
    }

    pub fn edit_notebook(&self, search: &str, replace: &str) -> anyhow::Result<String> {
        let content = self.read_notebook();
        if search == replace {
            return Err(anyhow::anyhow!("search and replace are identical"));
        }
        let count = content.matches(search).count();
        if count == 0 {
            return Err(anyhow::anyhow!("未找到匹配文本"));
        }
        if count > 1 {
            return Err(anyhow::anyhow!(
                "找到 {} 处匹配，需要更具体的搜索文本",
                count
            ));
        }
        let updated = content.replacen(search, replace, 1);
        self.write_notebook(&updated);
        Ok("笔记本已更新。".to_string())
    }

    pub fn search_notebook(&self, keyword: &str) -> String {
        let content = self.read_notebook();
        if content.is_empty() {
            return "笔记本是空的。".to_string();
        }
        let keyword_lower = keyword.to_lowercase();
        let matches: Vec<&str> = content
            .lines()
            .filter(|line| line.to_lowercase().contains(&keyword_lower))
            .collect();
        if matches.is_empty() {
            format!("在笔记本中未找到「{}」。", keyword)
        } else {
            let mut result = format!("在笔记本中找到 {} 行匹配：\n", matches.len());
            for line in matches {
                result.push_str(line);
                result.push('\n');
            }
            result
        }
    }

    // ── Truncation ──

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
            memory_path: dir_path.join("MEMORY.md"),
            profile_path: dir_path.join("profile.md"),
            soul_path: dir_path.join("SOUL.md"),
            notebook_path: dir_path.join("notebook.md"),
            max_bytes: 1024,
        }
    }

    #[test]
    fn read_write_memory() {
        let dir = TempDir::new().unwrap();
        let mgr = make_manager(&dir);
        mgr.write_memory("# Memory\n\ntest content");
        let content = mgr.read_memory();
        assert!(content.contains("test content"));
    }

    #[test]
    fn append_memory() {
        let dir = TempDir::new().unwrap();
        let mgr = make_manager(&dir);
        mgr.append_memory("first entry");
        mgr.append_memory("second entry");
        let content = mgr.read_memory();
        assert!(content.contains("first entry"));
        assert!(content.contains("second entry"));
    }

    #[test]
    fn soul_default_created() {
        let dir = TempDir::new().unwrap();
        let mgr = make_manager(&dir);
        let soul = mgr.read_soul();
        assert!(soul.contains("祈芷"));
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

    #[test]
    fn notebook_operations() {
        let dir = TempDir::new().unwrap();
        let mgr = make_manager(&dir);
        assert!(mgr.read_notebook().is_empty());
        mgr.append_notebook("hello world");
        assert!(mgr.read_notebook().contains("hello world"));
    }

    #[test]
    fn notebook_edit() {
        let dir = TempDir::new().unwrap();
        let mgr = make_manager(&dir);
        mgr.write_notebook("hello world\nfoo bar\n");
        mgr.edit_notebook("hello", "hi").unwrap();
        let content = mgr.read_notebook();
        assert!(content.contains("hi world"));
        assert!(!content.contains("hello"));
    }

    #[test]
    fn notebook_search() {
        let dir = TempDir::new().unwrap();
        let mgr = make_manager(&dir);
        mgr.write_notebook("hello world\nfoo bar\nbaz qux\n");
        let result = mgr.search_notebook("foo");
        assert!(result.contains("foo bar"));
        assert!(result.contains("1 行匹配"));
    }
}
