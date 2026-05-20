use std::fs;
use std::path::Path;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::tools::{Tool, make_unified_diff};

pub struct EditFileTool;

#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn description(&self) -> &str {
        "Replace text in a file via exact search/replace. The search string must match exactly including whitespace and indentation. The search must be unique in the file — if there are multiple matches, the tool returns an error so you can provide a more specific search string. The user will be asked to review the diff before applying. You can return multiple edit_file calls for the same file in one response — they will be batched into a single combined preview."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to edit"
                },
                "search": {
                    "type": "string",
                    "description": "Exact text to search for, including whitespace and indentation"
                },
                "replace": {
                    "type": "string",
                    "description": "Text to replace with"
                }
            },
            "required": ["path", "search", "replace"]
        })
    }

    fn requires_approval(&self) -> bool {
        true
    }

    fn preview(&self, args: Value) -> anyhow::Result<Option<String>> {
        let v = validate_single_args(&args)?;
        let diff = make_unified_diff(&v.path_str, &v.original, &v.updated);
        Ok(Some(format!(
            "{}\nReplaced 1 occurrence in {}",
            diff,
            v.path.display()
        )))
    }

    async fn execute(&self, args: Value) -> anyhow::Result<String> {
        let v = validate_single_args(&args)?;
        let path_display = v.path.display().to_string();
        fs::write(&v.path, &v.updated)
            .map_err(|e| anyhow::anyhow!("Failed to write {}: {}", path_display, e))?;
        Ok(format!("Replaced 1 occurrence in {}", path_display))
    }
}

struct ValidatedSingle {
    path: std::path::PathBuf,
    path_str: String,
    original: String,
    updated: String,
}

fn validate_single_args(args: &Value) -> anyhow::Result<ValidatedSingle> {
    let path_str = args["path"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'path' argument"))?;
    let search = args["search"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'search' argument"))?;
    let replace = args["replace"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing 'replace' argument"))?;

    let path = Path::new(path_str);
    let (original, updated) = validate_and_apply_one(path, search, replace)?;
    Ok(ValidatedSingle {
        path: path.to_path_buf(),
        path_str: path_str.to_string(),
        original,
        updated,
    })
}

fn validate_and_apply_one(path: &Path, search: &str, replace: &str) -> anyhow::Result<(String, String)> {
    if search == replace {
        return Err(anyhow::anyhow!("search and replace are identical"));
    }

    let contents = fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", path.display(), e))?;

    let count = contents.matches(search).count();
    if count == 0 {
        return Err(anyhow::anyhow!(
            "Search string not found in {}",
            path.display()
        ));
    }
    if count > 1 {
        let line_numbers: Vec<usize> = contents
            .lines()
            .enumerate()
            .filter(|(_, line)| line.contains(search))
            .map(|(i, _)| i + 1)
            .collect();
        return Err(anyhow::anyhow!(
            "Search string found {} times in {} (at lines {}). Provide a longer/more specific search string to uniquely identify the location.",
            count,
            path.display(),
            line_numbers
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    let updated = contents.replacen(search, replace, 1);
    Ok((contents, updated))
}

fn validate_and_apply_batched(
    path: &Path,
    edits: &[(String, String)],
    label: &str,
) -> anyhow::Result<(String, String)> {
    let mut current = fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", path.display(), e))?;

    for (i, (search, replace)) in edits.iter().enumerate() {
        if search == replace {
            return Err(anyhow::anyhow!(
                "Edit {} in {}: search and replace are identical",
                i + 1,
                label
            ));
        }
        let count = current.matches(search.as_str()).count();
        if count == 0 {
            return Err(anyhow::anyhow!(
                "Edit {} in {}: search string not found",
                i + 1,
                label
            ));
        }
        if count > 1 {
            return Err(anyhow::anyhow!(
                "Edit {} in {}: search string found {} times. Provide a longer/more specific search string.",
                i + 1,
                label,
                count
            ));
        }
        current = current.replacen(search.as_str(), replace.as_str(), 1);
    }

    let original = fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", path.display(), e))?;
    Ok((original, current))
}

/// Preview multiple edits to the same file as a single combined diff.
/// `edits` is a list of (search, replace) pairs applied sequentially.
pub fn preview_batched(path: &str, edits: &[(String, String)]) -> anyhow::Result<String> {
    if path.trim().is_empty() {
        return Err(anyhow::anyhow!("missing 'path' argument"));
    }
    let file_path = Path::new(path);
    let (original, updated) = validate_and_apply_batched(file_path, edits, &file_path.display().to_string())?;

    let diff = make_unified_diff(path, &original, &updated);
    let summary = format!("{} edit(s) to {}", edits.len(), file_path.display());
    Ok(format!("{}\n{}", diff, summary))
}

/// Execute multiple edits to the same file sequentially.
/// `edits` is a list of (search, replace) pairs applied in order.
pub fn execute_batched(path: &str, edits: &[(String, String)]) -> anyhow::Result<String> {
    if path.trim().is_empty() {
        return Err(anyhow::anyhow!("missing 'path' argument"));
    }
    let file_path = Path::new(path);
    let (_original, updated) = validate_and_apply_batched(file_path, edits, &file_path.display().to_string())?;

    fs::write(file_path, &updated)
        .map_err(|e| anyhow::anyhow!("Failed to write {}: {}", file_path.display(), e))?;

    Ok(format!(
        "Applied {} edit(s) to {}",
        edits.len(),
        file_path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn edit_single_match() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "hello world").unwrap();

        let tool = EditFileTool;
        let result = tool
            .execute(json!({
                "path": file.to_str().unwrap(),
                "search": "hello",
                "replace": "hi"
            }))
            .await
            .unwrap();

        assert!(result.contains("Replaced 1 occurrence"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "hi world");
    }

    #[tokio::test]
    async fn edit_multiple_matches_errors() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "hello hello").unwrap();

        let tool = EditFileTool;
        let result = tool
            .execute(json!({
                "path": file.to_str().unwrap(),
                "search": "hello",
                "replace": "hi"
            }))
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("found 2 times"));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello hello");
    }

    #[tokio::test]
    async fn edit_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "foo bar").unwrap();

        let tool = EditFileTool;
        let result = tool
            .execute(json!({
                "path": file.to_str().unwrap(),
                "search": "hello",
                "replace": "hi"
            }))
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn preview_shows_diff() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "hello world").unwrap();

        let tool = EditFileTool;
        let preview = tool
            .preview(json!({
                "path": file.to_str().unwrap(),
                "search": "hello",
                "replace": "hi"
            }))
            .unwrap()
            .unwrap();
        assert!(preview.contains("-hello world"));
        assert!(preview.contains("+hi world"));
        assert!(preview.contains("Replaced 1 occurrence"));
    }

    #[test]
    fn preview_batched_combines_edits() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "line one\nline two\nline three\n").unwrap();

        let preview = preview_batched(
            file.to_str().unwrap(),
            &[
                ("one".to_string(), "1".to_string()),
                ("two".to_string(), "2".to_string()),
            ],
        )
        .unwrap();
        assert!(preview.contains("-line one"));
        assert!(preview.contains("+line 1"));
        assert!(preview.contains("-line two"));
        assert!(preview.contains("+line 2"));
        assert!(preview.contains("2 edit(s)"));
    }

    #[test]
    fn execute_batched_applies_all_edits() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "hello world\nfoo bar\n").unwrap();

        let result = execute_batched(
            file.to_str().unwrap(),
            &[
                ("hello".to_string(), "hi".to_string()),
                ("foo".to_string(), "baz".to_string()),
            ],
        )
        .unwrap();
        assert!(result.contains("Applied 2 edit(s)"));
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "hi world\nbaz bar\n"
        );
    }

    #[test]
    fn preview_batched_search_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "hello world\n").unwrap();

        let result = preview_batched(
            file.to_str().unwrap(),
            &[
                ("hello".to_string(), "hi".to_string()),
                ("nonexistent".to_string(), "x".to_string()),
            ],
        );
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Edit 2"));
        assert!(err_msg.contains("not found"));
    }

    #[test]
    fn execute_batched_search_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "hello world\n").unwrap();

        let result = execute_batched(
            file.to_str().unwrap(),
            &[
                ("hello".to_string(), "hi".to_string()),
                ("nonexistent".to_string(), "x".to_string()),
            ],
        );
        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello world\n");
    }
}
