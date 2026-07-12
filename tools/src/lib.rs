mod capture;
mod cli;
mod events;
mod help;
mod model;
mod python_exec;
mod storage;
mod summarize;
mod transform;
mod util;

use std::collections::HashMap;
use std::io::{self, IsTerminal};
use std::process::{Command, Stdio};
use std::time::SystemTime;

use cli::{Config, Mode, RawStream};
use model::{CaptureResult, ListedEntry, Metadata, StreamKind};
use storage::{StoredResult, effective_store_dir};

const AUTO_SUMMARY_THRESHOLD: u64 = 2 * 1024;
const EXACT_GUARD_MIN_LINES: usize = 40;
const EXACT_GUARD_MAX_LINES: usize = 20_000;
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
            let text = config
                .help_topic
                .as_deref()
                .and_then(help::command)
                .unwrap_or(help::GLOBAL);
            util::stdout_line(text)?;
            Ok(0)
        }
        Mode::Version => {
            util::stdout_line(&format!("pira_ctx {}", env!("CARGO_PKG_VERSION")))?;
            Ok(0)
        }
        Mode::Exact => run_exact(&config),
        Mode::Check => run_check(&config),
        Mode::Auto => run_auto(&config),
        Mode::Capture => run_capture(&config),
        Mode::Search => run_search(&config),
        Mode::Range => run_range(&config),
        Mode::Raw => run_raw(&config),
        Mode::Exec => run_python_exec(&config),
        Mode::Transform => run_transform(&config),
        Mode::Recap => run_recap(&config),
        Mode::Batch => run_batch(&config),
        Mode::List => run_list(&config),
        Mode::Stats => run_stats(&config),
        Mode::Verify => run_verify(&config),
        Mode::Prune => run_prune(&config),
        Mode::Forget => run_forget(&config),
    }
}

fn run_python_exec(config: &Config) -> Result<i32, String> {
    let source = open_target(config)?;
    let prepared = python_exec::prepare(config, &source)?;
    let mut analysis_config = config.clone();
    analysis_config.cmd = vec![
        "pira_ctx".to_string(),
        "exec".to_string(),
        source.metadata.result_id.clone(),
    ];
    let ranking = ranking_terms(&analysis_config);
    let capture = match capture::capture_command(&prepared.command, &ranking)? {
        Ok(capture) => capture,
        Err(code) => {
            record_event(&analysis_config, code, 0, None);
            return Ok(code);
        }
    };
    if !should_capture(&capture) {
        replay_capture(&capture)?;
        record_event(
            &analysis_config,
            capture.exit_code,
            capture.duration_ms,
            None,
        );
        return Ok(capture.exit_code);
    }
    let compact = capture.total_bytes() < AUTO_SUMMARY_THRESHOLD
        && !capture.stdout.binary
        && !capture.stderr.binary
        && !capture.stdout.non_utf8
        && !capture.stderr.non_utf8;
    store_and_summarize(&analysis_config, &capture, compact)
}

fn run_exact(config: &Config) -> Result<i32, String> {
    if io::stdout().is_terminal() || io::stderr().is_terminal() {
        return run_streaming_exact(config);
    }
    let ranking = ranking_terms(config);
    let capture = match capture::capture_command(&config.cmd, &ranking)? {
        Ok(capture) => capture,
        Err(code) => {
            record_event(config, code, 0, None);
            return Ok(code);
        }
    };
    if should_guard_exact(&capture)? {
        let store_dir = effective_store_dir(config.store_dir.as_ref())?;
        let stored = storage::store_capture(&store_dir, &config.cmd, &ranking, &capture)?;
        util::stdout_line(&format!(
            "Auto-switched exact -> summary: non-interactive output was {} B/{} lines and highly repetitive; full capture retained.",
            capture.total_bytes(),
            capture.total_lines
        ))?;
        print_summary(&stored.metadata, &capture)?;
        record_event(
            config,
            capture.exit_code,
            capture.duration_ms,
            Some(&stored.metadata),
        );
    } else {
        replay_capture(&capture)?;
        record_event(config, capture.exit_code, capture.duration_ms, None);
    }
    Ok(capture.exit_code)
}

fn run_streaming_exact(config: &Config) -> Result<i32, String> {
    let cmd = &config.cmd;
    if cmd.is_empty() {
        return Err(cli::USAGE.to_string());
    }
    let start = SystemTime::now();
    match Command::new(&cmd[0]).args(&cmd[1..]).status() {
        Ok(status) => {
            let code = util::status_code(status);
            let duration = util::millis(SystemTime::now()).saturating_sub(util::millis(start));
            record_event(config, code, duration, None);
            Ok(code)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            eprintln!("pira_ctx: command not found: {}", cmd[0]);
            let duration = util::millis(SystemTime::now()).saturating_sub(util::millis(start));
            record_event(config, 127, duration, None);
            Ok(127)
        }
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
            eprintln!(
                "pira_ctx: command not executable/permission denied: {}",
                cmd[0]
            );
            let duration = util::millis(SystemTime::now()).saturating_sub(util::millis(start));
            record_event(config, 126, duration, None);
            Ok(126)
        }
        Err(error) => Err(format!("failed to spawn {}: {error}", cmd[0])),
    }
}

fn should_guard_exact(capture: &CaptureResult) -> Result<bool, String> {
    if capture.total_bytes() < AUTO_SUMMARY_THRESHOLD
        || capture.total_lines < EXACT_GUARD_MIN_LINES
        || capture.stdout.binary
        || capture.stderr.binary
        || capture.stdout.non_utf8
        || capture.stderr.non_utf8
    {
        return Ok(false);
    }
    let mut readers = capture.readers()?;
    let mut counts = HashMap::<String, usize>::new();
    let mut eligible = 0_usize;
    for line in capture.timeline.iter().take(EXACT_GUARD_MAX_LINES) {
        if !(12..=4096).contains(&line.length) {
            continue;
        }
        let text = readers.read_display_line(line)?;
        let Some(key) = exact_repetition_key(&text) else {
            continue;
        };
        eligible += 1;
        *counts.entry(key).or_default() += 1;
    }
    if eligible < EXACT_GUARD_MIN_LINES {
        return Ok(false);
    }
    Ok(is_highly_repetitive(&counts, eligible))
}

fn exact_repetition_key(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.chars().count() < 12 {
        return None;
    }
    let mut key = String::new();
    let mut in_digits = false;
    let mut in_space = false;
    for character in trimmed.chars() {
        if character.is_ascii_digit() {
            if !in_digits {
                key.push('#');
            }
            in_digits = true;
            in_space = false;
        } else if character.is_whitespace() {
            if !in_space {
                key.push(' ');
            }
            in_space = true;
            in_digits = false;
        } else {
            key.push(character);
            in_digits = false;
            in_space = false;
        }
        if key.chars().count() >= 48 {
            break;
        }
    }
    Some(key)
}

fn is_highly_repetitive(counts: &HashMap<String, usize>, eligible: usize) -> bool {
    let repeated = counts.values().filter(|&&count| count >= 3).sum::<usize>();
    let dominant = counts.values().copied().max().unwrap_or(0);
    repeated.saturating_mul(100) >= eligible.saturating_mul(70)
        && dominant.saturating_mul(100) >= eligible.saturating_mul(25)
}

fn run_auto(config: &Config) -> Result<i32, String> {
    if io::stdout().is_terminal() || io::stderr().is_terminal() {
        return run_exact(config);
    }
    let ranking = ranking_terms(config);
    let capture = match capture::capture_command(&config.cmd, &ranking)? {
        Ok(capture) => capture,
        Err(code) => {
            record_event(config, code, 0, None);
            return Ok(code);
        }
    };
    if !should_capture(&capture) {
        replay_capture(&capture)?;
        record_event(config, capture.exit_code, capture.duration_ms, None);
        return Ok(capture.exit_code);
    }
    let compact = capture.total_bytes() < AUTO_SUMMARY_THRESHOLD
        && !capture.stdout.binary
        && !capture.stderr.binary
        && !capture.stdout.non_utf8
        && !capture.stderr.non_utf8;
    store_and_summarize(config, &capture, compact)
}

fn run_check(config: &Config) -> Result<i32, String> {
    let ranking = ranking_terms(config);
    let capture = match capture::capture_command(&config.cmd, &ranking)? {
        Ok(capture) => capture,
        Err(code) => {
            util::stdout_line(&format!(
                "{} | exit={code} | duration=0ms | result=-",
                check_label(code)
            ))?;
            record_event(config, code, 0, None);
            return Ok(code);
        }
    };
    let store_dir = effective_store_dir(config.store_dir.as_ref())?;
    let stored = storage::store_capture(&store_dir, &config.cmd, &ranking, &capture)?;
    util::stdout_line(&format!(
        "{} | exit={} | duration={}ms | result={}",
        check_label(capture.exit_code),
        capture.exit_code,
        capture.duration_ms,
        stored.metadata.result_id
    ))?;
    record_event(
        config,
        capture.exit_code,
        capture.duration_ms,
        Some(&stored.metadata),
    );
    Ok(capture.exit_code)
}

fn check_label(exit_code: i32) -> &'static str {
    if exit_code == 0 { "PASS" } else { "FAIL" }
}

fn should_capture(capture: &CaptureResult) -> bool {
    capture.total_bytes() >= AUTO_SUMMARY_THRESHOLD
        || capture.stdout.binary
        || capture.stderr.binary
        || capture.stdout.non_utf8
        || capture.stderr.non_utf8
        || capture.timeline.iter().any(|line| {
            line.length > 2048
                || line.reasons.iter().any(|reason| {
                    matches!(
                        reason.as_str(),
                        "outcome/failure" | "severity/error" | "failed test" | "warning"
                    )
                })
        })
        || (capture.exit_code != 0 && capture.total_bytes() > 0)
}

fn run_capture(config: &Config) -> Result<i32, String> {
    let ranking = ranking_terms(config);
    let capture = match capture::capture_command(&config.cmd, &ranking)? {
        Ok(capture) => capture,
        Err(code) => {
            record_event(config, code, 0, None);
            return Ok(code);
        }
    };
    store_and_summarize(config, &capture, false)
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

fn store_and_summarize(
    config: &Config,
    capture: &CaptureResult,
    compact: bool,
) -> Result<i32, String> {
    let store_dir = effective_store_dir(config.store_dir.as_ref())?;
    let stored = storage::store_capture(&store_dir, &config.cmd, &ranking_terms(config), capture)?;
    if compact {
        print_compact_summary(&stored.metadata, capture)?;
    } else {
        print_summary(&stored.metadata, capture)?;
    }
    record_event(
        config,
        capture.exit_code,
        capture.duration_ms,
        Some(&stored.metadata),
    );
    Ok(capture.exit_code)
}

fn ranking_terms(config: &Config) -> Vec<String> {
    let mut terms = config.keywords.clone();
    if let Some(intent) = &config.intent {
        terms.extend(lexical_terms(intent));
    }
    terms.sort();
    terms.dedup();
    terms
}

fn record_event(config: &Config, exit: i32, duration: u128, metadata: Option<&Metadata>) {
    let Some(intent) = config.intent.as_deref() else {
        return;
    };
    let result = effective_store_dir(config.store_dir.as_ref())
        .and_then(|store| events::record(&store, intent, &config.cmd, exit, duration, metadata));
    if let Err(error) = result {
        eprintln!("pira_ctx: warning: command completed but event recording failed: {error}");
    }
}

fn print_summary(metadata: &Metadata, capture: &CaptureResult) -> Result<(), String> {
    let mut output = util::BoundedStdout::new(16 * 1024);
    let shown = summarize::select_important(&capture.timeline, MAX_IMPORTANT_LINES);
    let shown_bytes: u64 = shown
        .iter()
        .map(|&index| capture.timeline[index].length)
        .sum();
    let omitted_lines = capture.total_lines.saturating_sub(shown.len());
    let omitted_bytes = capture.total_bytes().saturating_sub(shown_bytes);
    output.line(&format!(
        "Result: {} | exit={} | {} B/{} lines | omitted={} B/{} lines",
        metadata.result_id,
        capture.exit_code,
        capture.total_bytes(),
        capture.total_lines,
        omitted_bytes,
        omitted_lines
    ))?;
    if capture.stderr.length > 0
        || capture.stdout.binary
        || capture.stderr.binary
        || capture.stdout.non_utf8
        || capture.stderr.non_utf8
    {
        output.line(&format!(
            "Streams: stdout={} B/{} lines; stderr={} B/{} lines; binary={}/{}; non_utf8={}/{}",
            capture.stdout.length,
            capture.stdout_lines,
            capture.stderr.length,
            capture.stderr_lines,
            capture.stdout.binary,
            capture.stderr.binary,
            capture.stdout.non_utf8,
            capture.stderr.non_utf8
        ))?;
    }
    output.line("Evidence:")?;
    if shown.is_empty() {
        output.line("  (none)")?;
    } else {
        let mut readers = capture.readers()?;
        for index in shown {
            let line = &capture.timeline[index];
            let text = readers.read_display_line(line)?;
            output.line(&format_evidence_line(line, &text))?;
        }
    }
    if !summarize::has_high_confidence_signal(&capture.timeline) {
        let groups = summarize::representative_groups(capture, 5)?;
        if !groups.is_empty() {
            output.line("Common line forms:")?;
            for (count, example) in groups {
                output.line(&format!("  {count}x {}", util::clip_display(&example)))?;
            }
        }
    }
    if !metadata.suggested_keywords.is_empty() {
        output.line(&format!(
            "Search terms: {}",
            util::single_line_clip(&metadata.suggested_keywords.join(" | "), 512)
        ))?;
    }
    output.line(&format!(
        "Retrieve: pira_ctx search {} <query>",
        metadata.result_id
    ))?;
    Ok(())
}

fn print_compact_summary(metadata: &Metadata, capture: &CaptureResult) -> Result<(), String> {
    let mut output = util::BoundedStdout::new(4 * 1024);
    output.line(&format!(
        "Captured: {} (exit {}){}",
        metadata.result_id,
        capture.exit_code,
        if capture.total_lines == 0 {
            "; no output"
        } else {
            ":"
        }
    ))?;
    if capture.total_lines > 0 {
        let mixed = capture.stdout.length > 0 && capture.stderr.length > 0;
        let mut readers = capture.readers()?;
        for line in &capture.timeline {
            let text = readers.read_display_line(line)?;
            if mixed {
                output.line(&format!("{}: {}", line.stream, text))?;
            } else {
                output.line(&text)?;
            }
        }
    }
    Ok(())
}

fn format_evidence_line(line: &crate::model::LineMeta, text: &str) -> String {
    format!(
        "L{} {}: {}",
        line.line,
        line.stream,
        util::clip_display(text)
    )
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
    let query_terms = lexical_terms(query);
    let mut lexical_hits = Vec::new();
    for (index, line) in store.metadata.line_timeline.iter().enumerate() {
        let text = reader.read_search_line(line)?;
        let matched = regex.as_ref().map_or_else(
            || util::unicode_contains_ci(&text, query),
            |regex| regex.is_match(&text),
        );
        if matched {
            hits.push((index, line.score + if config.regex { 70 } else { 80 }));
        } else if !config.regex {
            let score = lexical_score(&text, &query_terms);
            if score > 0 {
                lexical_hits.push((index, line.score + score));
            }
        }
    }
    let lexical = hits.is_empty() && !lexical_hits.is_empty();
    if lexical {
        hits = lexical_hits;
    }
    util::stdout_line(&format!(
        "{}{} hits",
        hits.len(),
        if lexical { " lexical" } else { "" }
    ))?;
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

fn lexical_terms(value: &str) -> Vec<String> {
    const STOPWORDS: &[&str] = &[
        "and", "are", "but", "did", "does", "for", "from", "has", "have", "how", "into", "not",
        "that", "the", "this", "use", "using", "was", "were", "what", "when", "where", "why",
        "with",
    ];
    let mut terms = value
        .split(|c: char| !c.is_alphanumeric())
        .filter(|v| v.chars().count() >= 3)
        .map(str::to_lowercase)
        .filter(|v| !STOPWORDS.contains(&v.as_str()))
        .map(|v| stem(&v))
        .collect::<Vec<_>>();
    terms.sort();
    terms.dedup();
    terms
}
fn stem(value: &str) -> String {
    for suffix in ["ing", "ed", "es", "s"] {
        if value.len() > suffix.len() + 3 && value.ends_with(suffix) {
            return value[..value.len() - suffix.len()].to_string();
        }
    }
    value.to_string()
}
fn lexical_score(text: &str, query: &[String]) -> i64 {
    if query.is_empty() {
        return 0;
    }
    let terms = lexical_terms(text);
    let matched = query
        .iter()
        .filter(|q| {
            terms
                .iter()
                .any(|t| t == *q || t.starts_with(q.as_str()) || q.starts_with(t.as_str()))
        })
        .count();
    let required = if query.len() == 1 {
        1
    } else {
        query.len().div_ceil(2)
    };
    if matched < required {
        0
    } else {
        40 + (matched as i64 * 20)
    }
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
    if config.target.is_none() {
        let dir = effective_store_dir(config.store_dir.as_ref())?;
        let workspace = storage::current_workspace_hash()?;
        let entries = storage::scan_store(&dir, Some(&workspace))?;
        let bytes: u64 = entries.iter().map(|e| e.bytes).sum();
        let events = events::read_current(&dir, usize::MAX)?.len();
        util::stdout_line(&format!("Workspace: {workspace}"))?;
        util::stdout_line(&format!("Captures: {}", entries.len()))?;
        util::stdout_line(&format!("CapturedBytes: {bytes}"))?;
        util::stdout_line(&format!("Events: {events}"))?;
        return Ok(0);
    }
    let store = open_target(config)?;
    let metadata = &store.metadata;
    util::stdout_line(&format!("Result: {}", metadata.result_id))?;
    util::stdout_line(&format!(
        "Command: {}",
        util::redacted_argv_display(&metadata.command_argv)
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
    let event_files = events::prune(&store_dir, config.max_age_days)?;
    util::stdout_line(&format!(
        "Pruned: files={} bytes={} events={}; remaining_files={} remaining_bytes={}",
        result.removed_files,
        result.removed_bytes,
        event_files,
        result.remaining_files,
        result.remaining_bytes
    ))?;
    Ok(0)
}

fn run_transform(config: &Config) -> Result<i32, String> {
    let store = open_target(config)?;
    let lines = transform::run(&store, &config.transform)?;
    const MAX_BYTES: usize = 64 * 1024;
    let mut used = 0_usize;
    for line in lines {
        let needed = line.len() + 1;
        if used + needed > MAX_BYTES {
            util::stdout_line("[transform output truncated]")?;
            break;
        }
        util::stdout_line(&line)?;
        used += needed
    }
    Ok(0)
}

fn run_recap(config: &Config) -> Result<i32, String> {
    let dir = effective_store_dir(config.store_dir.as_ref())?;
    let candidates = events::read_current(&dir, config.limit.saturating_mul(5).clamp(100, 2000))?;
    let events = events::select_recap(&candidates, config.limit);
    let mut output = util::BoundedStdout::new(8 * 1024 - 32);
    output.line("<pira_context_restore>")?;
    if events.is_empty() {
        output.line("No recent pira_ctx command events for this workspace.")?
    } else {
        for event in events {
            let files = event
                .files
                .iter()
                .take(5)
                .map(|v| util::xml_field(v, 256))
                .collect::<Vec<_>>()
                .join(", ");
            output.line(&format!(
                "- intent: {}; observed: {}; command: {}; files: {}; capture: {}",
                util::xml_field(&event.intent, 256),
                util::xml_field(&event.observed, 512),
                util::xml_field(&event.command, 1024),
                files,
                util::xml_field(event.capture_id.as_deref().unwrap_or("—"), 128)
            ))?;
        }
    }
    util::stdout_line("</pira_context_restore>")?;
    Ok(0)
}

fn run_forget(config: &Config) -> Result<i32, String> {
    let dir = effective_store_dir(config.store_dir.as_ref())?;
    let target = config
        .target
        .as_deref()
        .ok_or_else(|| cli::USAGE.to_string())?;
    if target == "events" {
        let count = events::forget_current(&dir)?;
        util::stdout_line(&format!("forgot {count} event files"))?;
        return Ok(0);
    }
    let path = storage::resolve_result(&dir, target)?;
    let capture = storage::read_result_path(&path)?;
    capture.verify()?;
    drop(capture);
    std::fs::remove_file(&path).map_err(|e| format!("remove {}: {e}", path.display()))?;
    util::stdout_line(&format!("forgot {}", path.display()))?;
    Ok(0)
}

#[derive(serde::Deserialize)]
struct BatchSpec {
    commands: Vec<BatchCommand>,
    #[serde(default)]
    concurrency: usize,
}
#[derive(serde::Deserialize)]
struct BatchCommand {
    intent: Option<String>,
    argv: Vec<String>,
}
fn run_batch(config: &Config) -> Result<i32, String> {
    let path = config
        .batch_file
        .as_ref()
        .ok_or_else(|| cli::USAGE.to_string())?;
    let spec: BatchSpec = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| format!("invalid batch spec: {e}"))?;
    if spec.commands.is_empty() || spec.commands.len() > 64 {
        return Err("batch requires 1..64 commands".into());
    }
    if spec.concurrency > 8 {
        return Err("batch concurrency must be 0..8".into());
    }
    let concurrency = spec.concurrency.max(1);
    let fallback = config.intent.clone();
    let mut validated = Vec::with_capacity(spec.commands.len());
    for item in spec.commands {
        let intent = item
            .intent
            .or_else(|| fallback.clone())
            .ok_or("each batch command requires intent")?;
        let intent = cli::validate_intent(&intent)?.to_string();
        if item.argv.is_empty() {
            return Err("batch argv cannot be empty".into());
        }
        validated.push((intent, item.argv));
    }
    let mut completed = Vec::new();
    let mut pending = validated.into_iter().enumerate();
    loop {
        let mut handles = Vec::new();
        for _ in 0..concurrency {
            let Some((index, (intent, argv))) = pending.next() else {
                break;
            };
            handles.push(std::thread::spawn(move || {
                let result = capture::capture_command(&argv, std::slice::from_ref(&intent));
                (index, intent, argv, result)
            }));
        }
        if handles.is_empty() {
            break;
        }
        for handle in handles {
            completed.push(handle.join().map_err(|_| "batch worker panicked")?)
        }
    }
    completed.sort_by_key(|r| r.0);
    util::stdout_line("index | exit | duration_ms | capture | intent")?;
    let mut overall = 0;
    let dir = effective_store_dir(config.store_dir.as_ref())?;
    for (index, intent, argv, result) in completed {
        let capture = match result? {
            Ok(c) => c,
            Err(code) => {
                overall = code;
                if let Err(error) = events::record(&dir, &intent, &argv, code, 0, None) {
                    eprintln!(
                        "pira_ctx: warning: batch child completed but event recording failed: {error}"
                    );
                }
                util::stdout_line(&format!("{} | {} | 0 | — | {}", index + 1, code, intent))?;
                continue;
            }
        };
        let stored = storage::store_capture(&dir, &argv, std::slice::from_ref(&intent), &capture)?;
        if let Err(error) = events::record(
            &dir,
            &intent,
            &argv,
            capture.exit_code,
            capture.duration_ms,
            Some(&stored.metadata),
        ) {
            eprintln!(
                "pira_ctx: warning: batch child completed but event recording failed: {error}"
            );
        }
        util::stdout_line(&format!(
            "{} | {} | {} | {} | {}",
            index + 1,
            capture.exit_code,
            capture.duration_ms,
            stored.metadata.result_id,
            intent
        ))?;
        if capture.exit_code != 0 {
            overall = capture.exit_code
        }
    }
    Ok(overall)
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
    util::stdout_line(&format_scored_line(line, score, text))
}

fn format_scored_line(line: &model::LineMeta, score: i64, text: &str) -> String {
    format!(
        "L{} {} score={}: {}",
        line.line,
        line.stream,
        score,
        util::clip_display(text)
    )
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

#[cfg(test)]
mod tests {
    use super::{exact_repetition_key, is_highly_repetitive};
    use std::collections::HashMap;

    #[test]
    fn exact_repetition_key_normalizes_dynamic_log_fields() {
        let first = r#"{"time":"2026-07-11T14:43:02.198528+08:00","level":"INFO","msg":"loading plugin","id":"alpha"}"#;
        let second = r#"{"time":"2026-07-12T09:04:51.777001+08:00","level":"INFO","msg":"loading plugin","id":"beta"}"#;
        assert_eq!(exact_repetition_key(first), exact_repetition_key(second));
    }

    #[test]
    fn exact_repetition_policy_requires_broad_and_dominant_repetition() {
        let repetitive = HashMap::from([
            ("common".to_string(), 60),
            ("secondary".to_string(), 20),
            ("unique-a".to_string(), 1),
            ("unique-b".to_string(), 19),
        ]);
        assert!(is_highly_repetitive(&repetitive, 100));

        let varied = (0..100)
            .map(|index| (format!("line-{index}"), 1))
            .collect::<HashMap<_, _>>();
        assert!(!is_highly_repetitive(&varied, 100));
    }
}
