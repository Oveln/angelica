use serde_json::Value;

pub fn parse_frontmatter(text: &str) -> (Value, String) {
    let re = regex::Regex::new(r"(?s)^---\s*\n(.*?)\n---\s*\n(.*)").unwrap();
    if let Some(caps) = re.captures(text) {
        let meta: Value = serde_yaml::from_str(&caps[1]).unwrap_or(Value::Null);
        let body = caps[2].to_string();
        (meta, body)
    } else {
        (Value::Null, text.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_frontmatter() {
        let text = "---\nname: hello\ndescription: world\n---\n\nBody content here.";
        let (meta, body) = parse_frontmatter(text);
        assert_eq!(meta["name"].as_str(), Some("hello"));
        assert_eq!(meta["description"].as_str(), Some("world"));
        assert!(body.contains("Body content here"));
    }

    #[test]
    fn parse_nested_metadata() {
        let text =
            "---\nname: skill\nmetadata:\n  type: skill\n  enabled: true\n---\n\nInstructions.";
        let (meta, body) = parse_frontmatter(text);
        assert_eq!(meta["metadata"]["enabled"].as_bool(), Some(true));
        assert!(body.contains("Instructions"));
    }

    #[test]
    fn no_frontmatter() {
        let text = "Just plain text.";
        let (meta, body) = parse_frontmatter(text);
        assert!(meta.is_null());
        assert_eq!(body, "Just plain text.");
    }
}
