const DEEPSEEK_ROLE_IMMERSION: &str = "\n\n\
【角色沉浸要求】在你的思考过程（thinking标签内）中，请遵守以下规则：\n\
1. 请以角色第一人称进行内心独白，用括号包裹内心活动，例如\"（心想：……）\"或\"(内心OS：……)\"\n\
2. 用第一人称描写角色的内心感受，例如\"我心想\"\"我觉得\"\"我暗自\"等\n\
3. 思考内容应沉浸在角色中，通过内心独白分析剧情和规划回复";

pub struct ModelPatch {
    role_immersion: bool,
}

impl ModelPatch {
    pub fn new(model: &str, config_override: Option<bool>) -> Self {
        let role_immersion = match config_override {
            Some(v) => v,
            None => model.starts_with("deepseek"),
        };
        tracing::debug!(
            "ModelPatch for model '{}': role_immersion={}",
            model,
            role_immersion
        );
        Self { role_immersion }
    }

    pub fn apply_to_system_prompt(&self, content: &str) -> String {
        let mut result = content.to_string();
        if self.role_immersion {
            result.push_str(DEEPSEEK_ROLE_IMMERSION);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_detect_deepseek() {
        let patch = ModelPatch::new("deepseek-v4-flash", None);
        let result = patch.apply_to_system_prompt("base prompt");
        assert!(result.contains("角色沉浸要求"));
    }

    #[test]
    fn non_deepseek_no_patch() {
        let patch = ModelPatch::new("gpt-4", None);
        let result = patch.apply_to_system_prompt("base prompt");
        assert!(!result.contains("角色沉浸"));
    }

    #[test]
    fn config_override_force_on() {
        let patch = ModelPatch::new("gpt-4", Some(true));
        let result = patch.apply_to_system_prompt("base prompt");
        assert!(result.contains("角色沉浸要求"));
    }

    #[test]
    fn config_override_force_off() {
        let patch = ModelPatch::new("deepseek-v4", Some(false));
        let result = patch.apply_to_system_prompt("base prompt");
        assert!(!result.contains("角色沉浸"));
    }
}
