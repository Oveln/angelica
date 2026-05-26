#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "api-generated.ts")]
pub enum UsageScope {
    #[default]
    Awake,
    Sleep,
}

impl UsageScope {
    pub fn label(self) -> &'static str {
        match self {
            UsageScope::Awake => "awake",
            UsageScope::Sleep => "sleep",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "api-generated.ts")]
pub struct UsageMetrics {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub reasoning_tokens: u64,
    pub cache_hit_tokens: u64,
    pub cache_miss_tokens: u64,
}

impl UsageMetrics {
    pub fn accumulate(&mut self, other: &UsageMetrics) {
        self.prompt_tokens += other.prompt_tokens;
        self.completion_tokens += other.completion_tokens;
        self.total_tokens += other.total_tokens;
        self.reasoning_tokens += other.reasoning_tokens;
        self.cache_hit_tokens += other.cache_hit_tokens;
        self.cache_miss_tokens += other.cache_miss_tokens;
    }

    pub fn cache_total(&self) -> u64 {
        self.cache_hit_tokens + self.cache_miss_tokens
    }

    pub fn cache_hit_rate(&self) -> f64 {
        let total = self.cache_total();
        if total == 0 {
            0.0
        } else {
            self.cache_hit_tokens as f64 / total as f64
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UsageRecord {
    pub timestamp: String,
    pub scope: UsageScope,
    pub iteration: usize,
    pub context_messages: usize,
    pub metrics: UsageMetrics,
}

impl UsageRecord {
    pub fn new(
        scope: UsageScope,
        iteration: usize,
        context_messages: usize,
        metrics: UsageMetrics,
    ) -> Self {
        Self {
            timestamp: chrono::Local::now().to_rfc3339(),
            scope,
            iteration,
            context_messages,
            metrics,
        }
    }
}

/// A single session's aggregated usage (one awake or sleep cycle).
#[derive(Debug, Clone, Default, serde::Serialize, ts_rs::TS)]
#[ts(export_to = "api-generated.ts")]
pub struct SessionUsage {
    pub scope: UsageScope,
    pub start_time: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub reasoning_tokens: u64,
    pub cache_hit_tokens: u64,
    pub cache_miss_tokens: u64,
    pub iterations: usize,
}

impl SessionUsage {
    pub fn cache_hit_rate(&self) -> f64 {
        let total = self.cache_hit_tokens + self.cache_miss_tokens;
        if total == 0 {
            0.0
        } else {
            self.cache_hit_tokens as f64 / total as f64
        }
    }
}

/// Load usage records from JSONL and aggregate into per-session summaries.
/// Sessions are segmented by scope transitions: each Awake→Sleep or Sleep→Awake boundary
/// starts a new session.
pub fn load_session_summaries(path: &std::path::Path) -> Vec<SessionUsage> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let records: Vec<UsageRecord> = content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();

    if records.is_empty() {
        return Vec::new();
    }

    let mut sessions: Vec<SessionUsage> = Vec::new();
    let mut current = SessionUsage {
        scope: records[0].scope,
        start_time: records[0].timestamp.clone(),
        ..Default::default()
    };

    for record in &records {
        if record.scope != current.scope {
            sessions.push(std::mem::take(&mut current));
            current.scope = record.scope;
            current.start_time = record.timestamp.clone();
        }
        current.prompt_tokens += record.metrics.prompt_tokens;
        current.completion_tokens += record.metrics.completion_tokens;
        current.total_tokens += record.metrics.total_tokens;
        current.reasoning_tokens += record.metrics.reasoning_tokens;
        current.cache_hit_tokens += record.metrics.cache_hit_tokens;
        current.cache_miss_tokens += record.metrics.cache_miss_tokens;
        current.iterations += 1;
    }
    sessions.push(current);

    sessions
}

pub fn restore_current_usage(path: &std::path::Path) -> Option<UsageMetrics> {
    let sessions = load_session_summaries(path);
    let last = sessions.last()?;
    if last.scope != UsageScope::Awake {
        return None;
    }
    Some(UsageMetrics {
        prompt_tokens: last.prompt_tokens,
        completion_tokens: last.completion_tokens,
        total_tokens: last.total_tokens,
        reasoning_tokens: last.reasoning_tokens,
        cache_hit_tokens: last.cache_hit_tokens,
        cache_miss_tokens: last.cache_miss_tokens,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_accumulate_adds_fields() {
        let mut a = UsageMetrics {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
            reasoning_tokens: 10,
            cache_hit_tokens: 80,
            cache_miss_tokens: 20,
        };
        let b = UsageMetrics {
            prompt_tokens: 200,
            completion_tokens: 100,
            total_tokens: 300,
            reasoning_tokens: 30,
            cache_hit_tokens: 150,
            cache_miss_tokens: 50,
        };
        a.accumulate(&b);
        assert_eq!(a.prompt_tokens, 300);
        assert_eq!(a.completion_tokens, 150);
        assert_eq!(a.total_tokens, 450);
        assert_eq!(a.reasoning_tokens, 40);
        assert_eq!(a.cache_hit_tokens, 230);
        assert_eq!(a.cache_miss_tokens, 70);
    }

    #[test]
    fn cache_hit_rate_zero_when_no_cache() {
        let m = UsageMetrics::default();
        assert_eq!(m.cache_hit_rate(), 0.0);
    }

    #[test]
    fn cache_hit_rate_computed() {
        let m = UsageMetrics {
            prompt_tokens: 1000,
            cache_hit_tokens: 700,
            cache_miss_tokens: 300,
            ..Default::default()
        };
        assert!((m.cache_hit_rate() - 0.7).abs() < 0.001);
    }

    #[test]
    fn record_serialization_roundtrip() {
        let r = UsageRecord::new(
            UsageScope::Awake,
            3,
            10,
            UsageMetrics {
                prompt_tokens: 500,
                completion_tokens: 200,
                total_tokens: 700,
                ..Default::default()
            },
        );
        let json = serde_json::to_string(&r).unwrap();
        let parsed: UsageRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.scope, UsageScope::Awake);
        assert_eq!(parsed.iteration, 3);
        assert_eq!(parsed.metrics.prompt_tokens, 500);
    }

    #[test]
    fn load_session_summaries_from_jsonl() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("usage.jsonl");

        let records = vec![
            UsageRecord::new(
                UsageScope::Awake,
                0,
                5,
                UsageMetrics {
                    prompt_tokens: 100,
                    completion_tokens: 50,
                    total_tokens: 150,
                    ..Default::default()
                },
            ),
            UsageRecord::new(
                UsageScope::Awake,
                1,
                6,
                UsageMetrics {
                    prompt_tokens: 200,
                    completion_tokens: 100,
                    total_tokens: 300,
                    ..Default::default()
                },
            ),
            UsageRecord::new(
                UsageScope::Sleep,
                0,
                3,
                UsageMetrics {
                    prompt_tokens: 50,
                    completion_tokens: 20,
                    total_tokens: 70,
                    ..Default::default()
                },
            ),
            UsageRecord::new(
                UsageScope::Awake,
                0,
                4,
                UsageMetrics {
                    prompt_tokens: 300,
                    completion_tokens: 150,
                    total_tokens: 450,
                    ..Default::default()
                },
            ),
        ];

        use std::io::Write;
        let mut file = std::fs::File::create(&path).unwrap();
        for r in &records {
            writeln!(file, "{}", serde_json::to_string(r).unwrap()).unwrap();
        }

        let sessions = load_session_summaries(&path);
        assert_eq!(sessions.len(), 3);
        // First awake session
        assert_eq!(sessions[0].scope, UsageScope::Awake);
        assert_eq!(sessions[0].prompt_tokens, 300); // 100 + 200
        assert_eq!(sessions[0].iterations, 2);
        // Sleep session
        assert_eq!(sessions[1].scope, UsageScope::Sleep);
        assert_eq!(sessions[1].prompt_tokens, 50);
        // Second awake session
        assert_eq!(sessions[2].scope, UsageScope::Awake);
        assert_eq!(sessions[2].prompt_tokens, 300);
    }

    #[test]
    fn load_from_nonexistent_file() {
        let sessions = load_session_summaries(std::path::Path::new("/nonexistent/usage.jsonl"));
        assert!(sessions.is_empty());
    }
}
