mod capture;
mod cli;
mod model;
mod storage;
mod summarize;
mod util;

use std::io::{self, IsTerminal};
use std::process::{Command, Stdio};

use cli::{Config, Mode, RawStream};
use model::{CaptureResult, ListedEntry, Metadata, StreamKind};
use storage::{StoredResult, effective_store_dir};

const AUTO_SUMMARY_THRESHOLD: u64 = 3 * 1024;
const MAX_IMPORTANT_LINES: usize = 10;
const MAX_SEARCH_RESULTS: usize = 5;

pub fn run() -> i32 {
    match real_main() {
        Ok(code) => code,
        Err(error) if error == util::BROKEN_PIPE => 0,
        Err(error) => {
            eprintln!("pira_ctx: {error}");
            125
        }
    }
}

fn real_main() -> Result<i32, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let config = cli::parse_args(&args)?;
    match config.mode {
        Mode::Help => {
            util::stdout_line(cli::USAGE)?;
            Ok(0)
        }
        Mode::Version => {
            util::stdout_line(&format!("pira_ctx {}", env!("CARGO_PKG_VERSION")))?;
            Ok(0)
        }
        Mode::Exact => run_exact(&config.cmd),
        Mode::Auto => run_auto(&config),
        Mode::Summary => run_summary(&config),
        Mode::Search => run_search(&config),
        Mode::Range => run_range(&config),
        Mode::Raw => run_raw(&config),
        Mode::List => run_list(&config),
        Mode::Stats => run_stats(&config),
        Mode::Verify => run_verify(&config),
        Mode::Prune => run_prune(&config),
    }
}

fn run_exact(cmd: &[String]) -> Result<i32, String> {
    if cmd.is_empty() {
        return Err(cli::USAGE.to_string());
    }
    match Command::new(&cmd[0]).args(&cmd[1..]).status() {
        Ok(status) => Ok(util::status_code(status)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            eprintln!("pira_ctx: command not found: {}", cmd[0]);
            Ok(127)
        }
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
            eprintln!(
                "pira_ctx: command not executable/permission denied: {}",
                cmd[0]
            );
            Ok(126)
        }
        Err(error) => Err(format!("failed to spawn {}: {error}", cmd[0])),
    }
}

fn run_auto(config: &Config) -> Result<i32, String> {
    if io::stdout().is_terminal() || io::stderr().is_terminal() {
        return run_exact(&config.cmd);
    }
    let capture = match capture::capture_command(&config.cmd, &config.keywords)? {
        Ok(capture) => capture,
        Err(code) => return Ok(code),
    };
    if capture.total_bytes() < AUTO_SUMMARY_THRESHOLD {
        replay_capture(&capture)?;
        return Ok(capture.exit_code);
    }
    store_and_summarize(config, &capture)
}

fn run_summary(config: &Config) -> Result<i32, String> {
    let capture = match capture::capture_command(&config.cmd, &config.keywords)? {
        Ok(capture) => capture,
        Err(code) => return Ok(code),
    };
    store_and_summarize(config, &capture)
}

fn replay_capture(capture: &CaptureResult) -> Result<(), String> {
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();
    let mut readers = capture.readers()?;
    for line in &capture.timeline {
        match line.stream {
            StreamKind::Stdout => readers.copy_line(line, &mut stdout)?,
            StreamKind::Stderr => readers.copy_line(line, &mut stderr)?,
        }
    }
    Ok(())
}

fn store_and_summarize(config: &Config, capture: &CaptureResult) -> Result<i32, String> {
    let store_dir = effective_store_dir(config.store_dir.as_ref())?;
    let stored = storage::store_capture(&store_dir, &config.cmd, &config.keywords, capture)?;
    print_summary(&stored.metadata, &stored.path, capture)?;
    Ok(capture.exit_code)
}

fn print_summary(
    metadata: &Metadata,
    path: &std::path::Path,
    capture: &CaptureResult,
) -> Result<(), String> {
    let shown = summarize::select_important(&capture.timeline, MAX_IMPORTANT_LINES);
    let shown_bytes: u64 = shown
        .iter()
        .map(|&index| capture.timeline[index].length)
        .sum();
    let omitted_lines = capture.total_lines.saturating_sub(shown.len());
    let omitted_bytes = capture.total_bytes().saturating_sub(shown_bytes);
    util::stdout_line(&format!(
        "Result: {} ({})",
        metadata.result_id, metadata.filename
    ))?;
    util::stdout_line(&format!(
        "Command: {}",
        util::argv_display(&metadata.command_argv)
    ))?;
    util::stdout_line(&format!("Exit: {}", capture.exit_code))?;
    util::stdout_line(&format!("Duration: {} ms", capture.duration_ms))?;
    util::stdout_line(&format!(
        "Size: stdout={} stderr={} total={} bytes; stdout_lines={} stderr_lines={} total_lines={}",
        capture.stdout.length,
        capture.stderr.length,
        capture.total_bytes(),
        capture.stdout_lines,
        capture.stderr_lines,
        capture.total_lines
    ))?;
    util::stdout_line(&format!(
        "Hidden: omitted_bytes={omitted_bytes} omitted_lines={omitted_lines} indexed_lines={} timeline_truncated={} binary_stdout={} binary_stderr={} non_utf8_stdout={} non_utf8_stderr={}",
        capture.timeline.len(),
        capture.timeline_truncated,
        capture.stdout.binary,
        capture.stderr.binary,
        capture.stdout.non_utf8,
        capture.stderr.non_utf8
    ))?;
    util::stdout_line(&format!("Store: {}", path.display()))?;
    util::stdout_line("Important lines:")?;
    if shown.is_empty() {
        util::stdout_line("  (none)")?;
    } else {
        let mut readers = capture.readers()?;
        for index in shown {
            let line = &capture.timeline[index];
            let text = readers.read_display_line(line)?;
            print_scored_line(line, line.score, &text)?;
        }
    }
    util::stdout_line("Anomalies:")?;
    let anomalies: Vec<_> = capture
        .timeline
        .iter()
        .filter(|line| {
            line.reasons
                .iter()
                .any(|reason| reason == "numeric anomaly")
        })
        .take(5)
        .collect();
    if anomalies.is_empty() {
        util::stdout_line("  (none detected)")?;
    } else {
        for line in anomalies {
            util::stdout_line(&format!(
                "  L{} {}: suspicious numeric token",
                line.line, line.stream
            ))?;
        }
    }
    util::stdout_line(&format!(
        "Suggested search keywords: {}",
        metadata.suggested_keywords.join(", ")
    ))?;
    util::stdout_line(
        "Retrieval: pira_ctx search --last <query> | pira_ctx range --last 1 20 | pira_ctx raw --last",
    )?;
    Ok(())
}

fn run_search(config: &Config) -> Result<i32, String> {
    let store = open_target(config)?;
    let query = config
        .query
        .as_ref()
        .ok_or_else(|| cli::USAGE.to_string())?;
    let regex = if config.regex {
        Some(regex::Regex::new(query).map_err(|error| format!("invalid regex: {error}"))?)
    } else {
        None
    };
    let mut reader = store.reader()?;
    let mut hits = Vec::new();
    for (index, line) in store.metadata.line_timeline.iter().enumerate() {
        let text = reader.read_search_line(line)?;
        let matched = regex.as_ref().map_or_else(
            || util::unicode_contains_ci(&text, query),
            |regex| regex.is_match(&text),
        );
        if matched {
            hits.push((index, line.score + if config.regex { 70 } else { 80 }));
        }
    }
    util::stdout_line(&format!("{} hits", hits.len()))?;
    if store.metadata.timeline_truncated {
        util::stdout_line(
            "Index: truncated; search covered the retained head and tail lines only",
        )?;
    }
    hits.sort_by(|a, b| {
        b.1.cmp(&a.1).then_with(|| {
            store.metadata.line_timeline[a.0]
                .line
                .cmp(&store.metadata.line_timeline[b.0].line)
        })
    });
    let mut selected = Vec::new();
    if config.context == 0 {
        selected.extend(hits.into_iter().take(MAX_SEARCH_RESULTS));
    } else {
        let mut seen = std::collections::HashSet::new();
        for (index, score) in hits.into_iter().take(MAX_SEARCH_RESULTS) {
            let start = index.saturating_sub(config.context);
            let end =
                (index + config.context).min(store.metadata.line_timeline.len().saturating_sub(1));
            for nearby in start..=end {
                if seen.insert(nearby) {
                    selected.push((
                        nearby,
                        if nearby == index {
                            score
                        } else {
                            store.metadata.line_timeline[nearby].score
                        },
                    ));
                }
            }
        }
        selected.sort_by_key(|(index, _)| *index);
    }
    for (index, score) in selected {
        let line = &store.metadata.line_timeline[index];
        let text = reader.read_display_line(line)?;
        print_scored_line(line, score, &text)?;
    }
    Ok(0)
}

fn run_range(config: &Config) -> Result<i32, String> {
    let store = open_target(config)?;
    if store.metadata.timeline_truncated {
        return Err(
            "result line index was truncated; use raw --stdout or raw --stderr".to_string(),
        );
    }
    let line_count = i64::try_from(store.metadata.line_timeline.len())
        .map_err(|_| "too many indexed lines".to_string())?;
    let start_raw = config.start_line.ok_or_else(|| cli::USAGE.to_string())?;
    let end_raw = config.end_line.ok_or_else(|| cli::USAGE.to_string())?;
    if start_raw == 0 || end_raw == 0 {
        return Err("line number 0 is invalid".to_string());
    }
    let mut start = if start_raw < 0 {
        line_count + start_raw + 1
    } else {
        start_raw
    };
    let mut end = if end_raw < 0 {
        line_count + end_raw + 1
    } else {
        end_raw
    };
    if start > end {
        return Err("start_line must be <= end_line after normalization".to_string());
    }
    if line_count == 0 || (start < 1 && end < 1) || (start > line_count && end > line_count) {
        return Ok(0);
    }
    start = start.max(1).min(line_count);
    end = end.max(1).min(line_count);
    let mut reader = store.reader()?;
    let mut output = io::stdout().lock();
    for number in start..=end {
        reader.copy_line(
            &store.metadata.line_timeline[(number - 1) as usize],
            &mut output,
        )?;
    }
    Ok(0)
}

fn run_raw(config: &Config) -> Result<i32, String> {
    let store = open_target(config)?;
    let mut reader = store.reader()?;
    let mut output = io::stdout().lock();
    match config.raw_stream {
        Some(RawStream::Stdout) => reader.copy_section(StreamKind::Stdout, &mut output)?,
        Some(RawStream::Stderr) => reader.copy_section(StreamKind::Stderr, &mut output)?,
        None => {
            if store.metadata.timeline_truncated {
                return Err(
                    "result line index was truncated; choose raw --stdout or raw --stderr"
                        .to_string(),
                );
            }
            for line in &store.metadata.line_timeline {
                reader.copy_line(line, &mut output)?;
            }
        }
    }
    Ok(0)
}

fn run_stats(config: &Config) -> Result<i32, String> {
    let store = open_target(config)?;
    let metadata = &store.metadata;
    util::stdout_line(&format!("Result: {}", metadata.result_id))?;
    util::stdout_line(&format!(
        "Command: {}",
        util::argv_display(&metadata.command_argv)
    ))?;
    util::stdout_line(&format!("Cwd: {}", metadata.cwd))?;
    util::stdout_line(&format!("Exit: {}", metadata.exit_code))?;
    util::stdout_line(&format!("Duration: {} ms", metadata.duration_ms))?;
    util::stdout_line(&format!(
        "Size: stdout={} stderr={} total={} bytes",
        metadata.stdout_bytes, metadata.stderr_bytes, metadata.total_bytes
    ))?;
    util::stdout_line(&format!(
        "Lines: stdout={} stderr={} total={}",
        metadata.stdout_lines, metadata.stderr_lines, metadata.total_lines
    ))?;
    util::stdout_line(&format!("Store: {}", store.path.display()))?;
    util::stdout_line(&format!("Created: {}", metadata.created_at))?;
    util::stdout_line(&format!("Tool: {}", metadata.tool_version))?;
    util::stdout_line(&format!("Format: {}", store.format_version))?;
    util::stdout_line(&format!(
        "Index: indexed_lines={} truncated={}",
        metadata.line_timeline.len(),
        metadata.timeline_truncated
    ))?;
    util::stdout_line(&format!(
        "Binary: stdout={} stderr={} non_utf8_stdout={} non_utf8_stderr={}",
        metadata.binary_stdout,
        metadata.binary_stderr,
        metadata.non_utf8_stdout,
        metadata.non_utf8_stderr
    ))?;
    util::stdout_line(&format!(
        "DetectedPaths: {}",
        metadata.detected_paths.join(", ")
    ))?;
    util::stdout_line(&format!(
        "Keywords: {}",
        metadata.suggested_keywords.join(", ")
    ))?;
    Ok(0)
}

fn run_list(config: &Config) -> Result<i32, String> {
    let store_dir = effective_store_dir(config.store_dir.as_ref())?;
    let filter = if config.workspace_current {
        Some(storage::current_workspace_hash()?)
    } else {
        None
    };
    let entries = storage::scan_store(&store_dir, filter.as_deref())?;
    util::stdout_line("id | timestamp | exit | bytes | lines | command")?;
    for entry in entries {
        print_listed_entry(&entry)?;
    }
    Ok(0)
}

fn print_listed_entry(entry: &ListedEntry) -> Result<(), String> {
    util::stdout_line(&format!(
        "{} | {} | {} | {} | {} | {}",
        entry.id, entry.timestamp, entry.exit, entry.bytes, entry.lines, entry.command
    ))
}

fn run_verify(config: &Config) -> Result<i32, String> {
    let store = open_target(config)?;
    store.verify()?;
    util::stdout_line(&format!("verified {}", store.path.display()))?;
    Ok(0)
}

fn run_prune(config: &Config) -> Result<i32, String> {
    let store_dir = effective_store_dir(config.store_dir.as_ref())?;
    let result = storage::prune_store(&store_dir, config.max_age_days, config.max_store_bytes)?;
    util::stdout_line(&format!(
        "Pruned: files={} bytes={}; remaining_files={} remaining_bytes={}",
        result.removed_files, result.removed_bytes, result.remaining_files, result.remaining_bytes
    ))?;
    Ok(0)
}

fn open_target(config: &Config) -> Result<StoredResult, String> {
    let store_dir = effective_store_dir(config.store_dir.as_ref())?;
    let target = config
        .target
        .as_ref()
        .ok_or_else(|| cli::USAGE.to_string())?;
    let path = storage::resolve_result(&store_dir, target)?;
    storage::read_result_path(&path)
}

fn print_scored_line(line: &model::LineMeta, score: i64, text: &str) -> Result<(), String> {
    util::stdout_line(&format!(
        "L{} {} score={}: {}",
        line.line,
        line.stream,
        score,
        util::clip_display(text)
    ))
}

pub(crate) fn spawn_command(cmd: &[String]) -> Result<std::process::Child, String> {
    Command::new(&cmd[0])
        .args(&cmd[1..])
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                format!("__EXIT127__ command not found: {}", cmd[0])
            } else if error.kind() == io::ErrorKind::PermissionDenied {
                format!("__EXIT126__ permission denied/not executable: {}", cmd[0])
            } else {
                format!("failed to spawn {}: {error}", cmd[0])
            }
        })
}
