use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PermissionAction {
    Allow,
    #[default]
    Ask,
    Deny,
}

impl PermissionAction {
    fn default_ask() -> Self {
        PermissionAction::Ask
    }
}

#[derive(Debug, Clone)]
pub struct TargetRule {
    pub target: String,
    pub action: PermissionAction,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Rule {
    tool: String,
    target: String,
    action: PermissionAction,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ApprovedRules {
    rules: Vec<Rule>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolRuleEntry {
    pub tool: String,
    pub target: String,
    pub action: PermissionAction,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PermissionConfig {
    #[serde(default = "PermissionAction::default_ask")]
    pub default: PermissionAction,
    #[serde(default)]
    pub tools: Vec<ToolRuleEntry>,
    #[serde(default = "default_approved_path")]
    pub approved_path: String,
}

impl Default for PermissionConfig {
    fn default() -> Self {
        Self {
            default: PermissionAction::Ask,
            tools: Vec::new(),
            approved_path: default_approved_path(),
        }
    }
}

fn default_approved_path() -> String {
    "data/approved_rules.toml".to_string()
}

pub struct PermissionEvaluator {
    default: PermissionAction,
    builtin: Vec<Rule>,
    config: Vec<Rule>,
    approved: Vec<Rule>,
}

impl PermissionEvaluator {
    pub fn new(
        default: PermissionAction,
        builtin: Vec<(String, Vec<TargetRule>)>,
        config_tools: Vec<ToolRuleEntry>,
    ) -> Self {
        let builtin_rules = builtin
            .into_iter()
            .flat_map(|(tool, rules)| {
                rules.into_iter().map(move |r| Rule {
                    tool: tool.clone(),
                    target: r.target,
                    action: r.action,
                })
            })
            .collect();

        let config: Vec<Rule> = config_tools
            .into_iter()
            .map(|e| Rule {
                tool: e.tool,
                target: e.target,
                action: e.action,
            })
            .collect();

        Self {
            default,
            builtin: builtin_rules,
            config,
            approved: Vec::new(),
        }
    }

    pub fn set_mode_rules(&mut self, builtin: Vec<(String, Vec<TargetRule>)>) {
        self.builtin = builtin
            .into_iter()
            .flat_map(|(tool, rules)| {
                rules.into_iter().map(move |r| Rule {
                    tool: tool.clone(),
                    target: r.target,
                    action: r.action,
                })
            })
            .collect();
    }

    pub fn evaluate(&self, tool: &str, target: Option<&str>) -> PermissionAction {
        let target_str = target.unwrap_or("*");
        for ruleset in [&self.approved, &self.config, &self.builtin] {
            for rule in ruleset {
                if rule.tool == tool && matches_pattern(&rule.target, target_str) {
                    return rule.action;
                }
            }
        }
        self.default
    }

    pub fn approve_always(
        &mut self,
        tool: &str,
        target: String,
        path: &Path,
    ) -> anyhow::Result<()> {
        self.approve_session(tool, target);
        self.save_approved(path)
    }

    pub fn approve_session(&mut self, tool: &str, target: String) {
        if let Some(existing) = self
            .approved
            .iter_mut()
            .find(|r| r.tool == tool && r.target == target)
        {
            existing.action = PermissionAction::Allow;
        } else {
            self.approved.push(Rule {
                tool: tool.to_string(),
                target,
                action: PermissionAction::Allow,
            });
        }
    }

    pub fn load_approved(&mut self, path: &Path) {
        if !path.exists() {
            return;
        }
        let raw = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("Failed to read approved rules: {}", e);
                return;
            }
        };
        let approved: ApprovedRules = match toml::from_str(&raw) {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!("Failed to parse approved rules: {}", e);
                return;
            }
        };
        self.approved = approved.rules;
    }

    fn save_approved(&self, path: &Path) -> anyhow::Result<()> {
        let data = ApprovedRules {
            rules: self.approved.clone(),
        };
        let toml_str = toml::to_string_pretty(&data)?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let tmp_path = path.with_extension("toml.tmp");
        std::fs::write(&tmp_path, &toml_str)?;
        std::fs::rename(&tmp_path, path)?;

        Ok(())
    }
}

fn matches_pattern(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let mut pi = 0;
    let mut ti = 0;
    let mut star_pi = None;
    let mut star_ti = 0;

    while ti < t.len() {
        if pi < p.len() {
            match p[pi] {
                '*' => {
                    star_pi = Some(pi + 1);
                    star_ti = ti;
                    pi += 1;
                    continue;
                }
                '?' => {
                    pi += 1;
                    ti += 1;
                    continue;
                }
                _ if p[pi] == t[ti] => {
                    pi += 1;
                    ti += 1;
                    continue;
                }
                _ => {}
            }
        }
        if let Some(sp) = star_pi {
            pi = sp;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }

    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_exact() {
        assert!(matches_pattern("hello", "hello"));
        assert!(!matches_pattern("hello", "world"));
    }

    #[test]
    fn test_glob_star() {
        assert!(matches_pattern("*", "anything"));
        assert!(matches_pattern("*.rs", "main.rs"));
        assert!(!matches_pattern("*.rs", "main.go"));
        assert!(matches_pattern("cargo *", "cargo build"));
        assert!(matches_pattern("cargo *", "cargo test --release"));
    }

    #[test]
    fn test_glob_question() {
        assert!(matches_pattern("?", "a"));
        assert!(!matches_pattern("?", "ab"));
        assert!(matches_pattern("?.rs", "a.rs"));
        assert!(!matches_pattern("?.rs", "ab.rs"));
    }

    #[test]
    fn test_glob_star_question_combo() {
        assert!(matches_pattern("test?.txt", "test1.txt"));
        assert!(!matches_pattern("test?.txt", "test12.txt"));
    }

    #[test]
    fn test_glob_edge_cases() {
        assert!(matches_pattern("", ""));
        assert!(!matches_pattern("", "x"));
        assert!(matches_pattern("*", ""));
    }

    #[test]
    fn test_evaluate_builtin_default() {
        let builtin = vec![(
            "read_file".to_string(),
            vec![TargetRule {
                target: "*".to_string(),
                action: PermissionAction::Allow,
            }],
        )];
        let eval = PermissionEvaluator::new(PermissionAction::Ask, builtin, vec![]);
        assert_eq!(
            eval.evaluate("read_file", Some("/foo.rs")),
            PermissionAction::Allow
        );
        assert_eq!(
            eval.evaluate("write_file", Some("/foo.rs")),
            PermissionAction::Ask
        );
    }

    #[test]
    fn test_evaluate_config_overrides_builtin() {
        let builtin = vec![(
            "read_file".to_string(),
            vec![TargetRule {
                target: "*".to_string(),
                action: PermissionAction::Allow,
            }],
        )];
        let config = vec![ToolRuleEntry {
            tool: "read_file".to_string(),
            target: "*.env".to_string(),
            action: PermissionAction::Deny,
        }];
        let eval = PermissionEvaluator::new(PermissionAction::Ask, builtin, config);
        assert_eq!(
            eval.evaluate("read_file", Some("src/main.rs")),
            PermissionAction::Allow
        );
        assert_eq!(
            eval.evaluate("read_file", Some("secret.env")),
            PermissionAction::Deny
        );
    }

    #[test]
    fn test_evaluate_first_match_wins() {
        let builtin = vec![(
            "run_command".to_string(),
            vec![
                TargetRule {
                    target: "rm *".to_string(),
                    action: PermissionAction::Deny,
                },
                TargetRule {
                    target: "*".to_string(),
                    action: PermissionAction::Allow,
                },
            ],
        )];
        let eval = PermissionEvaluator::new(PermissionAction::Ask, builtin, vec![]);
        assert_eq!(
            eval.evaluate("run_command", Some("rm -rf /")),
            PermissionAction::Deny
        );
        assert_eq!(
            eval.evaluate("run_command", Some("cargo build")),
            PermissionAction::Allow
        );
    }

    #[test]
    fn test_evaluate_approved_overrides_all() {
        let builtin = vec![(
            "run_command".to_string(),
            vec![TargetRule {
                target: "*".to_string(),
                action: PermissionAction::Ask,
            }],
        )];
        let mut eval = PermissionEvaluator::new(PermissionAction::Ask, builtin, vec![]);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("approved.toml");
        eval.approve_always("run_command", "cargo *".to_string(), &path)
            .unwrap();
        assert_eq!(
            eval.evaluate("run_command", Some("cargo build")),
            PermissionAction::Allow
        );
        assert_eq!(
            eval.evaluate("run_command", Some("rm -rf /")),
            PermissionAction::Ask
        );
    }

    #[test]
    fn test_persist_and_load_approved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("approved.toml");
        let mut eval = PermissionEvaluator::new(PermissionAction::Ask, vec![], vec![]);
        eval.approve_always("run_command", "cargo *".to_string(), &path)
            .unwrap();
        eval.approve_always("edit_file", "src/*".to_string(), &path)
            .unwrap();

        let mut eval2 = PermissionEvaluator::new(PermissionAction::Ask, vec![], vec![]);
        eval2.load_approved(&path);
        assert_eq!(
            eval2.evaluate("run_command", Some("cargo build")),
            PermissionAction::Allow
        );
        assert_eq!(
            eval2.evaluate("edit_file", Some("src/main.rs")),
            PermissionAction::Allow
        );
        assert_eq!(
            eval2.evaluate("edit_file", Some("README.md")),
            PermissionAction::Ask
        );
    }

    #[test]
    fn test_approve_session_same_file_twice() {
        let builtin = vec![(
            "edit_file".to_string(),
            vec![TargetRule {
                target: "*".to_string(),
                action: PermissionAction::Ask,
            }],
        )];
        let mut eval = PermissionEvaluator::new(PermissionAction::Ask, builtin, vec![]);

        assert_eq!(
            eval.evaluate("edit_file", Some("data/a.txt")),
            PermissionAction::Ask
        );

        eval.approve_session("edit_file", "data/a.txt".to_string());

        assert_eq!(
            eval.evaluate("edit_file", Some("data/a.txt")),
            PermissionAction::Allow
        );

        assert_eq!(
            eval.evaluate("edit_file", Some("data/b.txt")),
            PermissionAction::Ask
        );
    }
}
