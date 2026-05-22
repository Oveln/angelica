use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::skills::SkillRegistry;
use crate::tools::Tool;

pub struct SkillTool {
    skills: Arc<SkillRegistry>,
}

impl SkillTool {
    pub fn new(skills: Arc<SkillRegistry>) -> Self {
        Self { skills }
    }
}

#[async_trait]
impl Tool for SkillTool {
    fn name(&self) -> &str {
        "skill"
    }

    fn description(&self) -> &str {
        "加载一个技能来获取特定领域的指导。当你遇到匹配某个技能描述的任务时，先加载它。"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Name of the skill to load"
                }
            },
            "required": ["name"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let name = args["name"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'name'"))?;

        let skill = self.skills.get(name).ok_or_else(|| {
            let available: Vec<&str> = self
                .skills
                .get_all_skills()
                .iter()
                .map(|s| &*s.name)
                .collect();
            anyhow::anyhow!(
                "Skill \"{}\" not found. Available: {}",
                name,
                available.join(", ")
            )
        })?;

        let mut output = format!("# Skill: {}\n\n{}", skill.name, skill.instructions);

        if !skill.scripts.is_empty() || !skill.templates.is_empty() || !skill.references.is_empty()
        {
            output.push_str("\n\n## Files");
            for f in &skill.scripts {
                output.push_str(&format!("\n- script: {}", f.display()));
            }
            for f in &skill.templates {
                output.push_str(&format!("\n- template: {}", f.display()));
            }
            for f in &skill.references {
                output.push_str(&format!("\n- reference: {}", f.display()));
            }
        }

        Ok(output)
    }
}
