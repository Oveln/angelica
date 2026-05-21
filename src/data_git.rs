use std::path::Path;
use std::process::Command;
use std::process::Stdio;

/// A version of a file from git history.
pub struct FileVersion {
    pub commit_hash: String,
    pub timestamp: i64,
    pub content: String,
}

fn git_cmd(dir: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

fn run_git(dir: &Path, args: &[&str]) -> anyhow::Result<std::process::Output> {
    let mut cmd = git_cmd(dir);
    cmd.args(args);
    let output = cmd.output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git {}: {}", args.join(" "), stderr.trim());
    }
    Ok(output)
}

/// Ensure `data_dir` is a git repo. Initializes one if missing.
pub fn ensure_repo(data_dir: &Path) -> anyhow::Result<()> {
    if !data_dir.exists() {
        std::fs::create_dir_all(data_dir)?;
    }

    if data_dir.join(".git").exists() {
        return Ok(());
    }

    run_git(data_dir, &["init"])?;
    run_git(data_dir, &["config", "user.email", "angelica@local"])?;
    run_git(data_dir, &["config", "user.name", "angelica"])?;

    // Initial commit — may be empty if data_dir has no files yet
    run_git(data_dir, &["add", "-A"])?;
    let output = Command::new("git")
        .current_dir(data_dir)
        .args(["commit", "--allow-empty", "-m", "initial"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // "nothing to commit" is fine
        if !stderr.contains("nothing to commit") {
            anyhow::bail!("git commit initial: {}", stderr.trim());
        }
    }

    Ok(())
}

/// Stage all changes and commit in `data_dir`. Skips if nothing changed.
pub fn commit_all(data_dir: &Path, message: &str) -> anyhow::Result<()> {
    let output = run_git(data_dir, &["status", "--porcelain"])?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        return Ok(());
    }

    run_git(data_dir, &["add", "-A"])?;
    run_git(data_dir, &["commit", "-m", message])?;
    Ok(())
}

/// Read historical versions of `file_path` (relative to data_dir) from git log.
/// Returns up to `limit` versions, newest first.
pub fn read_file_history(
    data_dir: &Path,
    file_path: &str,
    limit: usize,
) -> anyhow::Result<Vec<FileVersion>> {
    let log_output = run_git(
        data_dir,
        &[
            "log",
            "--format=%H %ct",
            &format!("-n {}", limit),
            "--",
            file_path,
        ],
    )?;
    let log_str = String::from_utf8_lossy(&log_output.stdout);
    let mut versions = Vec::new();

    for line in log_str.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.len() != 2 {
            continue;
        }
        let hash = parts[0].to_string();
        let timestamp: i64 = match parts[1].parse() {
            Ok(t) => t,
            Err(_) => continue,
        };

        // Retrieve file content at this commit
        let show_output = match run_git(data_dir, &["show", &format!("{}:{}", hash, file_path)]) {
            Ok(o) => o,
            Err(_) => continue, // file didn't exist at this commit
        };
        let content = String::from_utf8_lossy(&show_output.stdout).to_string();

        versions.push(FileVersion {
            commit_hash: hash,
            timestamp,
            content,
        });
    }

    Ok(versions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn ensure_repo_creates_git() {
        let dir = TempDir::new().unwrap();
        let data = dir.path().join("data");
        ensure_repo(&data).unwrap();
        assert!(data.join(".git").exists());
    }

    #[test]
    fn ensure_repo_idempotent() {
        let dir = TempDir::new().unwrap();
        let data = dir.path().join("data");
        ensure_repo(&data).unwrap();
        ensure_repo(&data).unwrap(); // should not fail
    }

    #[test]
    fn commit_all_tracks_changes() {
        let dir = TempDir::new().unwrap();
        let data = dir.path().join("data");
        ensure_repo(&data).unwrap();

        std::fs::write(data.join("test.txt"), "hello").unwrap();
        commit_all(&data, "add test").unwrap();

        let output = run_git(&data, &["status", "--porcelain"]).unwrap();
        assert!(String::from_utf8_lossy(&output.stdout).trim().is_empty());
    }

    #[test]
    fn commit_all_skips_when_clean() {
        let dir = TempDir::new().unwrap();
        let data = dir.path().join("data");
        ensure_repo(&data).unwrap();
        commit_all(&data, "should not appear").unwrap();

        // Only the initial commit should exist
        let output = run_git(&data, &["log", "--oneline"]).unwrap();
        let log = String::from_utf8_lossy(&output.stdout).to_string();
        assert!(!log.contains("should not appear"));
    }

    #[test]
    fn read_file_history_returns_versions() {
        let dir = TempDir::new().unwrap();
        let data = dir.path().join("data");
        ensure_repo(&data).unwrap();

        std::fs::write(data.join("memo.txt"), "v1").unwrap();
        commit_all(&data, "first").unwrap();

        std::fs::write(data.join("memo.txt"), "v2").unwrap();
        commit_all(&data, "second").unwrap();

        let versions = read_file_history(&data, "memo.txt", 10).unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].content, "v2");
        assert_eq!(versions[1].content, "v1");
    }
}
