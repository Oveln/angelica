pub mod frontmatter;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct SkillDef {
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub path: PathBuf,
    pub scripts: Vec<PathBuf>,
    pub templates: Vec<PathBuf>,
    pub references: Vec<PathBuf>,
    pub enabled: bool,
}

pub struct SkillRegistry {
    skills: HashMap<String, SkillDef>,
    skills_dir: PathBuf,
}

impl SkillRegistry {
    pub fn new(skills_dir: &str) -> Self {
        Self {
            skills: HashMap::new(),
            skills_dir: PathBuf::from(skills_dir),
        }
    }

    pub fn discover(&mut self) {
        if !self.skills_dir.exists() {
            tracing::info!("Skills directory does not exist: {}", self.skills_dir.display());
            return;
        }

        let Ok(entries) = std::fs::read_dir(&self.skills_dir) else {
            tracing::warn!("Failed to read skills directory: {}", self.skills_dir.display());
            return;
        };

        let mut dirs: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .filter(|e| !e.file_name().to_string_lossy().starts_with('_'))
            .collect();
        dirs.sort_by_key(|e| e.file_name());

        for entry in dirs {
            let skill_md = entry.path().join("SKILL.md");
            if !skill_md.exists() {
                continue;
            }

            let Ok(content) = std::fs::read_to_string(&skill_md) else {
                continue;
            };

            let (meta, instructions) = frontmatter::parse_frontmatter(&content);
            let dir_name = entry.file_name().to_string_lossy().to_string();
            let name = meta
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(&dir_name)
                .to_string();
            let description = meta
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let enabled = meta
                .get("metadata")
                .and_then(|m| m.get("enabled"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            let scripts = Self::list_files(&entry.path(), "scripts");
            let templates = Self::list_files(&entry.path(), "templates");
            let references = Self::list_files(&entry.path(), "references");

            self.skills.insert(
                name.clone(),
                SkillDef {
                    name,
                    description,
                    instructions: instructions.trim().to_string(),
                    path: entry.path(),
                    scripts,
                    templates,
                    references,
                    enabled,
                },
            );
        }
    }

    fn list_files(base: &Path, subdir: &str) -> Vec<PathBuf> {
        let dir = base.join(subdir);
        if !dir.is_dir() {
            return Vec::new();
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        files.sort();
        files
    }

    pub fn get_all_skills(&self) -> Vec<&SkillDef> {
        self.skills.values().filter(|s| s.enabled).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn discover_skills() {
        let dir = TempDir::new().unwrap();
        let skill_dir = dir.path().join("my-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: test-skill\ndescription: A test\nmetadata:\n  enabled: true\n---\n\nDo the thing.",
        )
        .unwrap();

        let mut registry = SkillRegistry::new(dir.path().to_str().unwrap());
        registry.discover();

        let skills = registry.get_all_skills();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "test-skill");
        assert!(skills[0].instructions.contains("Do the thing"));
    }

    #[test]
    fn skip_disabled_skills() {
        let dir = TempDir::new().unwrap();
        let skill_dir = dir.path().join("disabled");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: off\nmetadata:\n  enabled: false\n---\n\nDisabled skill.",
        )
        .unwrap();

        let mut registry = SkillRegistry::new(dir.path().to_str().unwrap());
        registry.discover();
        assert!(registry.get_all_skills().is_empty());
    }
}
