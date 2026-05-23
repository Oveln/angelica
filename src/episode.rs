use serde::{Deserialize, Serialize};
use std::io::Write;

/// Status of an episode in its lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EpisodeStatus {
    Recent,
    Past,
}

/// A single episodic memory entry. Stored as one JSON line in episodes.jsonl.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    pub id: String,
    pub date: String,
    pub body: String,
    #[serde(default = "default_weight")]
    pub emotional_weight: u8,
    #[serde(default)]
    pub unresolved: bool,
    #[serde(default)]
    pub afterglow: String,
    #[serde(default = "default_status")]
    pub status: EpisodeStatus,
    #[serde(default)]
    pub embedding: Vec<f32>,
    #[serde(default)]
    pub created_at: String,
}

fn default_weight() -> u8 {
    3
}

fn default_status() -> EpisodeStatus {
    EpisodeStatus::Recent
}

impl Episode {
    pub fn new(date: String, body: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            date,
            body,
            emotional_weight: 3,
            unresolved: false,
            afterglow: String::new(),
            status: EpisodeStatus::Recent,
            embedding: Vec::new(),
            created_at: chrono::Local::now().to_rfc3339(),
        }
    }

    pub fn with_weight(mut self, w: u8) -> Self {
        self.emotional_weight = w.clamp(1, 5);
        self
    }

    pub fn with_afterglow(mut self, a: String) -> Self {
        self.afterglow = a;
        self
    }

    pub fn with_unresolved(mut self, u: bool) -> Self {
        self.unresolved = u;
        self
    }
}

/// Read all episodes from a JSONL file.
pub fn read_episodes(path: &std::path::Path) -> Vec<Episode> {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<Episode>(line).ok())
        .collect()
}

/// Append a single episode to a JSONL file.
pub fn append_episode(path: &std::path::Path, episode: &Episode) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let json = serde_json::to_string(episode)?;
    use std::io::Write;
    writeln!(file, "{}", json)?;
    Ok(())
}

/// Rewrite all episodes to a JSONL file (used after status changes or embedding updates).
pub fn write_all_episodes(path: &std::path::Path, episodes: &[Episode]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::File::create(path)?;
    for ep in episodes {
        let json = serde_json::to_string(ep)?;
        writeln!(file, "{}", json)?;
    }
    Ok(())
}

/// Cosine similarity between two vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

/// Search past episodes by embedding similarity. Returns top-K results with scores.
pub fn search_by_embedding(
    episodes: &[Episode],
    query_embedding: &[f32],
    top_k: usize,
    min_similarity: f32,
) -> Vec<(usize, f32)> {
    let mut scored: Vec<(usize, f32)> = episodes
        .iter()
        .enumerate()
        .filter(|(_, ep)| ep.status == EpisodeStatus::Past && !ep.embedding.is_empty())
        .map(|(i, ep)| (i, cosine_similarity(query_embedding, &ep.embedding)))
        .filter(|(_, score)| *score >= min_similarity)
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_k);
    scored
}

/// Format episodes as markdown for injection into system prompt.
pub fn format_episodes_for_prompt(episodes: &[&Episode]) -> String {
    let mut out = String::new();
    for ep in episodes {
        out.push_str(&format!("### {}\n", ep.date));
        if ep.emotional_weight != 3 || !ep.afterglow.is_empty() {
            out.push_str(&format!("> 情感权重：{}/5\n", ep.emotional_weight));
            if !ep.afterglow.is_empty() {
                out.push_str(&format!("> 余韵：{}\n", ep.afterglow));
            }
        }
        out.push('\n');
        out.push_str(&ep.body);
        out.push_str("\n\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn episode_serialization_roundtrip() {
        let ep = Episode::new("2026-05-22".to_string(), "test body".to_string())
            .with_weight(4)
            .with_afterglow("轻盈".to_string());
        let json = serde_json::to_string(&ep).unwrap();
        let parsed: Episode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.date, "2026-05-22");
        assert_eq!(parsed.body, "test body");
        assert_eq!(parsed.emotional_weight, 4);
        assert_eq!(parsed.afterglow, "轻盈");
        assert_eq!(parsed.status, EpisodeStatus::Recent);
    }

    #[test]
    fn cosine_similarity_identical() {
        let v = vec![1.0, 0.0, 1.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 0.001);
    }

    #[test]
    fn cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!((cosine_similarity(&a, &b)).abs() < 0.001);
    }

    #[test]
    fn search_finds_top_k() {
        let query = vec![1.0, 0.0];
        let episodes = vec![
            Episode {
                embedding: vec![0.9, 0.1],
                status: EpisodeStatus::Past,
                ..Episode::new("1".into(), "a".into())
            },
            Episode {
                embedding: vec![0.1, 0.9],
                status: EpisodeStatus::Past,
                ..Episode::new("2".into(), "b".into())
            },
            Episode {
                embedding: vec![0.8, 0.2],
                status: EpisodeStatus::Recent, // should be excluded
                ..Episode::new("3".into(), "c".into())
            },
        ];
        let results = search_by_embedding(&episodes, &query, 2, 0.5);
        assert_eq!(results.len(), 1); // only episode 0 passes threshold
        assert_eq!(results[0].0, 0);
    }
}
