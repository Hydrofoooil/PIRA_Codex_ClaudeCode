use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::{model::Metadata, storage, util};

const MAX_EVENT_BYTES: u64 = 64 * 1024;

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub timestamp_ms: u128,
    pub workspace_hash: String,
    pub intent: String,
    pub command: String,
    pub category: String,
    pub exit_code: i32,
    pub duration_ms: u128,
    pub observed: String,
    pub capture_id: Option<String>,
    #[serde(default)]
    pub files: Vec<String>,
}

pub fn record(
    store: &Path,
    intent: &str,
    command: &[String],
    exit: i32,
    duration: u128,
    metadata: Option<&Metadata>,
) -> Result<(), String> {
    let workspace_hash = storage::current_workspace_hash()?;
    let event = Event {
        timestamp_ms: util::millis(SystemTime::now()),
        workspace_hash: workspace_hash.clone(),
        intent: intent.trim().to_string(),
        command: redacted_command(command),
        category: category(command),
        exit_code: exit,
        duration_ms: duration,
        observed: observed(exit, metadata),
        capture_id: metadata.map(|m| m.result_id.clone()),
        files: metadata.map_or_else(Vec::new, |m| {
            m.detected_paths
                .iter()
                .take(16)
                .map(|path| util::single_line_clip(path, 512))
                .collect()
        }),
    };
    let dir = event_dir(store, &workspace_hash);
    private_dir(&dir)?;
    let name = format!(
        "{}-{}-{}.json",
        event.timestamp_ms,
        std::process::id(),
        short_nonce()
    );
    let final_path = dir.join(name);
    let tmp = final_path.with_extension("tmp");
    let bytes = serde_json::to_vec(&event).map_err(|e| e.to_string())?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(&tmp).map_err(|e| e.to_string())?;
    file.write_all(&bytes).map_err(|e| e.to_string())?;
    // Events are best-effort recap hints rather than checked captures. Rename
    // atomically, but do not add another disk barrier to every wrapped command.
    fs::rename(&tmp, &final_path).map_err(|e| e.to_string())?;
    cap_event_count(&dir, 2000)?;
    Ok(())
}

pub fn read_current(store: &Path, limit: usize) -> Result<Vec<Event>, String> {
    let hash = storage::current_workspace_hash()?;
    let dir = event_dir(store, &hash);
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();
    paths.sort();
    paths.reverse();
    let mut out = Vec::new();
    for path in paths.into_iter().take(limit) {
        if path
            .metadata()
            .is_ok_and(|metadata| metadata.len() <= MAX_EVENT_BYTES)
            && let Ok(bytes) = fs::read(path)
            && let Ok(event) = serde_json::from_slice(&bytes)
        {
            out.push(event)
        }
    }
    out.reverse();
    Ok(out)
}

pub fn select_recap(events: &[Event], maximum: usize) -> Vec<Event> {
    if maximum == 0 {
        return Vec::new();
    }
    let mut selected = std::collections::BTreeSet::new();
    if let Some(index) = events.len().checked_sub(1) {
        selected.insert(index);
    }
    if selected.len() < maximum
        && let Some((index, _)) = events
            .iter()
            .enumerate()
            .rev()
            .find(|(_, e)| e.exit_code != 0)
    {
        selected.insert(index);
    }
    if selected.len() < maximum
        && let Some((index, _)) = events
            .iter()
            .enumerate()
            .rev()
            .find(|(_, e)| e.exit_code == 0 && e.category == "build/test")
    {
        selected.insert(index);
    }
    for index in (0..events.len()).rev() {
        if selected.len() >= maximum {
            break;
        }
        selected.insert(index);
    }
    selected
        .into_iter()
        .map(|index| events[index].clone())
        .collect()
}

pub fn forget_current(store: &Path) -> Result<usize, String> {
    let hash = storage::current_workspace_hash()?;
    let dir = event_dir(store, &hash);
    if !dir.exists() {
        return Ok(0);
    }
    let count = fs::read_dir(&dir).map_err(|e| e.to_string())?.count();
    fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
    Ok(count)
}

pub fn prune(store: &Path, max_age_days: Option<u64>) -> Result<usize, String> {
    let root = store.join("events");
    if !root.exists() {
        return Ok(0);
    }
    let cutoff = max_age_days
        .map(|d| util::millis(SystemTime::now()).saturating_sub(u128::from(d) * 86_400_000));
    let mut removed = 0;
    for workspace in fs::read_dir(root)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
    {
        if !workspace.path().is_dir() {
            continue;
        }
        for entry in fs::read_dir(workspace.path())
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
        {
            let path = entry.path();
            let old = cutoff.is_some_and(|c| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .and_then(|s| s.split('-').next())
                    .and_then(|s| s.parse::<u128>().ok())
                    .is_some_and(|t| t < c)
            });
            if old {
                fs::remove_file(path).map_err(|e| e.to_string())?;
                removed += 1
            }
        }
    }
    Ok(removed)
}

fn event_dir(store: &Path, hash: &str) -> PathBuf {
    store.join("events").join(hash)
}
fn category(command: &[String]) -> String {
    let first = command
        .first()
        .and_then(|value| Path::new(value).file_name())
        .and_then(|value| value.to_str())
        .unwrap_or("");
    if matches!(first, "cargo" | "rustc" | "make" | "cmake" | "ninja") {
        "build/test"
    } else if matches!(first, "git") {
        "git"
    } else if matches!(first, "cat" | "sed" | "grep" | "find" | "rg") {
        "inspection"
    } else {
        "command"
    }
    .into()
}
fn observed(exit: i32, metadata: Option<&Metadata>) -> String {
    if let Some(m) = metadata {
        format!(
            "command exited {exit}; {} lines and {} bytes captured",
            m.total_lines, m.total_bytes
        )
    } else if exit == 0 {
        "command exited 0; output was not captured".into()
    } else {
        format!("command exited {exit}; output was not captured")
    }
}

fn redacted_command(command: &[String]) -> String {
    let mut output = Vec::with_capacity(command.len());
    let mut redact_next = false;
    for argument in command {
        if redact_next {
            output.push("[REDACTED]".to_string());
            redact_next = false;
            continue;
        }
        let lower = argument.to_ascii_lowercase();
        if let Some((key, _)) = argument.split_once('=')
            && sensitive_name(key)
        {
            output.push(format!("{key}=[REDACTED]"));
            continue;
        }
        if sensitive_name(lower.trim_start_matches('-')) {
            output.push(argument.clone());
            redact_next = true;
        } else {
            output.push(argument.clone())
        }
    }
    util::single_line_clip(&util::argv_display(&output), 2048)
}

fn sensitive_name(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase().replace(['-', '_'], "");
    matches!(
        normalized.as_str(),
        "authorization"
            | "authtoken"
            | "accesstoken"
            | "refreshtoken"
            | "bearer"
            | "token"
            | "secret"
            | "password"
            | "passwd"
            | "pwd"
            | "apikey"
            | "cookie"
            | "setcookie"
            | "signature"
            | "privatekey"
            | "clientsecret"
    ) || normalized.ends_with("authtoken")
        || normalized.ends_with("accesstoken")
        || normalized.ends_with("refreshtoken")
        || normalized.ends_with("apikey")
        || normalized.ends_with("password")
        || normalized.ends_with("clientsecret")
}
static EVENT_COUNTER: AtomicU64 = AtomicU64::new(0);
fn short_nonce() -> u64 {
    (util::millis(SystemTime::now()) as u64)
        ^ u64::from(std::process::id())
        ^ EVENT_COUNTER.fetch_add(1, Ordering::Relaxed)
}
fn private_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    Ok(())
}

fn cap_event_count(dir: &Path, maximum: usize) -> Result<(), String> {
    let mut paths: Vec<_> = fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();
    if paths.len() <= maximum {
        return Ok(());
    }
    paths.sort();
    let remove = paths.len() - maximum;
    for path in paths.into_iter().take(remove) {
        fs::remove_file(path).map_err(|e| e.to_string())?
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_redacts_common_secret_arguments() {
        let command = vec![
            "curl".into(),
            "--token".into(),
            "secret-value".into(),
            "MY_API_KEY=also-secret".into(),
            "safe".into(),
        ];
        let rendered = redacted_command(&command);
        assert!(!rendered.contains("secret-value"));
        assert!(!rendered.contains("also-secret"));
        assert!(rendered.contains("safe"));
    }

    #[test]
    fn recap_selection_respects_requested_limit() {
        let make = |exit_code, category: &str| Event {
            timestamp_ms: 0,
            workspace_hash: "w".into(),
            intent: "i".into(),
            command: "c".into(),
            category: category.into(),
            exit_code,
            duration_ms: 0,
            observed: "o".into(),
            capture_id: None,
            files: vec![],
        };
        let events = vec![
            make(0, "build/test"),
            make(1, "command"),
            make(0, "command"),
        ];
        assert_eq!(select_recap(&events, 1).len(), 1);
        assert_eq!(select_recap(&events, 2).len(), 2);
    }
}
