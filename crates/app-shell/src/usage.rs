use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use serde_json::Value;

#[derive(Clone, Debug, Default)]
pub struct UsageSnapshot {
    pub source_path: PathBuf,
    pub status: UsageStatus,
    pub models: Vec<ModelUsage>,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost_usd: f64,
    pub event_count: usize,
    pub session_count: usize,
    pub last_timestamp: Option<String>,
    pub parse_errors: usize,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ModelUsage {
    pub provider: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
    pub event_count: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum UsageStatus {
    #[default]
    AwaitingFile,
    AwaitingEvents,
    Ready,
    ParseWarning,
    Error,
}

pub fn load_snapshot(root_hint: &Path) -> UsageSnapshot {
    let source_path = usage_log_path(root_hint);
    load_snapshot_from_path(&source_path)
}

fn load_snapshot_from_path(source_path: &Path) -> UsageSnapshot {
    let mut snapshot = UsageSnapshot {
        source_path: source_path.to_path_buf(),
        status: UsageStatus::AwaitingFile,
        ..Default::default()
    };

    let contents = match fs::read_to_string(source_path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == ErrorKind::NotFound => return snapshot,
        Err(error) => {
            snapshot.status = UsageStatus::Error;
            snapshot.error = Some(error.to_string());
            return snapshot;
        }
    };

    parse_snapshot_contents(snapshot, &contents)
}

fn parse_snapshot_contents(mut snapshot: UsageSnapshot, contents: &str) -> UsageSnapshot {
    if contents.lines().all(|line| line.trim().is_empty()) {
        snapshot.status = UsageStatus::AwaitingEvents;
        return snapshot;
    }

    let mut grouped = BTreeMap::<(String, String), ModelUsage>::new();
    let mut sessions = BTreeSet::new();

    for line in contents.lines().filter(|line| !line.trim().is_empty()) {
        let value = match serde_json::from_str::<Value>(line) {
            Ok(value) => value,
            Err(_) => {
                snapshot.parse_errors += 1;
                continue;
            }
        };

        let provider =
            string_field(&value, &["provider", "vendor"]).unwrap_or_else(|| "unknown".into());
        let model =
            string_field(&value, &["model", "model_name"]).unwrap_or_else(|| "unknown".into());
        let input_tokens = integer_field(&value, &["input_tokens", "prompt_tokens", "input"]);
        let output_tokens =
            integer_field(&value, &["output_tokens", "completion_tokens", "output"]);
        let cost_usd = float_field(&value, &["cost_usd", "cost"]);

        if let Some(session) = string_field(&value, &["session", "session_id"]) {
            sessions.insert(session);
        }

        if let Some(timestamp) = string_field(&value, &["timestamp", "created_at"]) {
            snapshot.last_timestamp = Some(timestamp);
        }

        snapshot.total_input_tokens += input_tokens;
        snapshot.total_output_tokens += output_tokens;
        snapshot.total_cost_usd += cost_usd;
        snapshot.event_count += 1;

        let entry = grouped
            .entry((provider.clone(), model.clone()))
            .or_insert_with(|| ModelUsage {
                provider,
                model,
                ..Default::default()
            });

        entry.input_tokens += input_tokens;
        entry.output_tokens += output_tokens;
        entry.cost_usd += cost_usd;
        entry.event_count += 1;
    }

    snapshot.session_count = sessions.len();
    snapshot.models = grouped.into_values().collect();
    snapshot.models.sort_by(|left, right| {
        right
            .cost_usd
            .partial_cmp(&left.cost_usd)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                (right.input_tokens + right.output_tokens)
                    .cmp(&(left.input_tokens + left.output_tokens))
            })
    });

    snapshot.status = if !snapshot.models.is_empty() && snapshot.parse_errors == 0 {
        UsageStatus::Ready
    } else if !snapshot.models.is_empty() {
        UsageStatus::ParseWarning
    } else if snapshot.parse_errors > 0 {
        UsageStatus::Error
    } else {
        UsageStatus::AwaitingEvents
    };

    snapshot
}

fn usage_log_path(root_hint: &Path) -> PathBuf {
    env::var_os("GHOSTTY_SHELL_USAGE_LOG")
        .map(PathBuf::from)
        .unwrap_or_else(|| root_hint.join(".ghostty-shell/usage-events.jsonl"))
}

fn string_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn integer_field(value: &Value, keys: &[&str]) -> u64 {
    keys.iter()
        .find_map(|key| value.get(*key))
        .and_then(|field| {
            field
                .as_u64()
                .or_else(|| field.as_i64().map(|number| number.max(0) as u64))
        })
        .unwrap_or_default()
}

fn float_field(value: &Value, keys: &[&str]) -> f64 {
    keys.iter()
        .find_map(|key| value.get(*key))
        .and_then(|field| {
            field
                .as_f64()
                .or_else(|| field.as_i64().map(|number| number as f64))
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn parse_snapshot_contents_aggregates_usage_by_model() {
        let snapshot = parse_snapshot_contents(
            UsageSnapshot {
                source_path: PathBuf::from("usage.jsonl"),
                ..Default::default()
            },
            r#"{"timestamp":"2026-04-14T15:10:00Z","provider":"OpenAI","model":"gpt-5.4","input_tokens":1200,"output_tokens":340,"cost_usd":0.09,"session":"shell-01"}
{"timestamp":"2026-04-14T15:12:00Z","vendor":"OpenAI","model_name":"gpt-5.4","prompt_tokens":300,"completion_tokens":30,"cost":0.02,"session_id":"shell-01"}
{"timestamp":"2026-04-14T15:13:00Z","provider":"Anthropic","model":"claude-sonnet","input_tokens":980,"output_tokens":210,"cost_usd":0.05,"session":"shell-02"}"#,
        );

        assert_eq!(snapshot.status, UsageStatus::Ready);
        assert_eq!(snapshot.total_input_tokens, 2_480);
        assert_eq!(snapshot.total_output_tokens, 580);
        assert!((snapshot.total_cost_usd - 0.16).abs() < f64::EPSILON);
        assert_eq!(snapshot.event_count, 3);
        assert_eq!(snapshot.session_count, 2);
        assert_eq!(
            snapshot.last_timestamp.as_deref(),
            Some("2026-04-14T15:13:00Z")
        );
        assert_eq!(snapshot.models.len(), 2);
        assert_eq!(snapshot.models[0].provider, "OpenAI");
        assert_eq!(snapshot.models[0].model, "gpt-5.4");
        assert_eq!(snapshot.models[0].input_tokens, 1_500);
        assert_eq!(snapshot.models[0].output_tokens, 370);
        assert_eq!(snapshot.models[0].event_count, 2);
    }

    #[test]
    fn parse_snapshot_contents_reports_partial_parse_failures() {
        let snapshot = parse_snapshot_contents(
            UsageSnapshot {
                source_path: PathBuf::from("usage.jsonl"),
                ..Default::default()
            },
            r#"{"provider":"OpenAI","model":"gpt-5.4","input_tokens":10,"output_tokens":5,"cost_usd":0.01}
not json at all"#,
        );

        assert_eq!(snapshot.status, UsageStatus::ParseWarning);
        assert_eq!(snapshot.parse_errors, 1);
        assert_eq!(snapshot.event_count, 1);
        assert_eq!(snapshot.models.len(), 1);
    }

    #[test]
    fn parse_snapshot_contents_reports_error_when_all_lines_are_broken() {
        let snapshot = parse_snapshot_contents(
            UsageSnapshot {
                source_path: PathBuf::from("usage.jsonl"),
                ..Default::default()
            },
            "}{\nnot-json\n",
        );

        assert_eq!(snapshot.status, UsageStatus::Error);
        assert_eq!(snapshot.parse_errors, 2);
        assert_eq!(snapshot.event_count, 0);
        assert!(snapshot.models.is_empty());
    }

    #[test]
    fn load_snapshot_from_path_handles_missing_and_empty_files() {
        let temp = TempDir::new().expect("temp dir");
        let missing = temp.path().join("missing.jsonl");
        let empty = temp.path().join("empty.jsonl");
        fs::write(&empty, "\n  \n").expect("empty usage log");

        let missing_snapshot = load_snapshot_from_path(&missing);
        let empty_snapshot = load_snapshot_from_path(&empty);

        assert_eq!(missing_snapshot.status, UsageStatus::AwaitingFile);
        assert_eq!(empty_snapshot.status, UsageStatus::AwaitingEvents);
    }

    #[test]
    fn numeric_fields_clamp_negative_and_accept_integer_costs() {
        let value: Value = serde_json::from_str(
            r#"{"input_tokens":-10,"output_tokens":2,"cost":3,"provider":"x","model":"y"}"#,
        )
        .expect("json");

        assert_eq!(integer_field(&value, &["input_tokens"]), 0);
        assert_eq!(integer_field(&value, &["output_tokens"]), 2);
        assert_eq!(float_field(&value, &["cost"]), 3.0);
    }
}
