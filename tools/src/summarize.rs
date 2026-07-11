use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use crate::model::{CaptureResult, LineMeta, StreamKind};
use crate::util;

pub fn score_timeline(capture: &mut CaptureResult, keywords: &[String]) -> Result<(), String> {
    let total = capture.total_lines;
    let mut readers = capture.readers()?;
    for line in &mut capture.timeline {
        let clean = readers.read_display_line(line)?;
        let (score, reasons) = score_line(
            &clean,
            line.stream,
            line.line,
            total,
            capture.exit_code,
            keywords,
        );
        line.score = score;
        line.reasons = reasons;
    }
    let base_scores: Vec<i64> = capture.timeline.iter().map(|line| line.score).collect();
    for (index, &base_score) in base_scores.iter().enumerate() {
        if base_score < 100 {
            continue;
        }
        for nearby in [index.checked_sub(1), index.checked_add(1)] {
            if let Some(nearby) = nearby.filter(|&value| value < capture.timeline.len())
                && capture.timeline[nearby]
                    .line
                    .abs_diff(capture.timeline[index].line)
                    == 1
                && capture.timeline[nearby].score < base_score
            {
                capture.timeline[nearby].score += 10;
                capture.timeline[nearby]
                    .reasons
                    .push("adjacent diagnostic context".to_string());
            }
        }
    }
    Ok(())
}

pub fn select_important(lines: &[LineMeta], maximum: usize) -> Vec<usize> {
    if maximum == 0 {
        return Vec::new();
    }
    let mut selected = Vec::new();
    let mut selected_set = HashSet::new();
    let has_failure = lines.iter().any(|line| {
        line.reasons
            .iter()
            .any(|reason| reason == "outcome/failure")
    });
    let failure_budget = maximum.div_ceil(2);
    for index in (0..lines.len()).rev().filter(|&index| {
        lines[index]
            .reasons
            .iter()
            .any(|reason| reason == "outcome/failure")
    }) {
        for nearby in [index.checked_sub(1), Some(index), index.checked_add(1)] {
            if selected.len() >= failure_budget {
                break;
            }
            if let Some(nearby) = nearby.filter(|&value| value < lines.len())
                && selected_set.insert(nearby)
            {
                selected.push(nearby);
            }
        }
        if selected.len() >= failure_budget {
            break;
        }
    }
    if !has_failure {
        let latest_outcome = lines.iter().rposition(|line| {
            line.reasons
                .iter()
                .any(|reason| reason == "outcome/success")
        });
        let latest_check = lines.iter().rposition(|line| {
            line.reasons
                .iter()
                .any(|reason| reason == "successful check")
        });
        if let Some(index) = latest_outcome.or(latest_check) {
            selected_set.insert(index);
            selected.push(index);
        }
    }
    let mut order: Vec<usize> = (0..lines.len()).collect();
    order.sort_by_key(|&index| (Reverse(lines[index].score), lines[index].line));
    let mut templates: HashMap<String, usize> = HashMap::new();
    let mut successful_checks = selected
        .iter()
        .filter(|&&index| {
            lines[index]
                .reasons
                .iter()
                .any(|reason| reason == "successful check")
        })
        .count();
    for index in order {
        if selected.len() >= maximum {
            break;
        }
        if !selected_set.insert(index) {
            continue;
        }
        if has_failure
            && lines[index]
                .reasons
                .iter()
                .any(|reason| reason == "outcome/success")
        {
            continue;
        }
        if lines[index]
            .reasons
            .iter()
            .any(|reason| reason == "successful check")
        {
            if successful_checks >= 2 {
                continue;
            }
            successful_checks += 1;
        }
        let template = reason_template(&lines[index]);
        let repetitions = templates.entry(template).or_default();
        let repetition_limit = if lines[index]
            .reasons
            .iter()
            .any(|reason| reason == "severity/error")
        {
            3
        } else {
            2
        };
        if *repetitions >= repetition_limit {
            continue;
        }
        *repetitions += 1;
        selected.push(index);
    }
    selected.sort_by_key(|&index| lines[index].line);
    selected
}

pub fn has_high_confidence_signal(lines: &[LineMeta]) -> bool {
    lines.iter().any(|line| {
        line.reasons.iter().any(|reason| {
            matches!(
                reason.as_str(),
                "outcome/failure"
                    | "outcome/success"
                    | "severity/error"
                    | "failed test"
                    | "warning"
                    | "numeric anomaly"
            )
        })
    })
}

pub fn representative_groups(
    capture: &CaptureResult,
    maximum: usize,
) -> Result<Vec<(usize, String)>, String> {
    let number = regex::Regex::new(r"\b\d+(?:\.\d+)?\b").unwrap();
    let timestamp = regex::Regex::new(
        r"\b\d{2,4}[-/:T]\d{1,2}[-/:T]\d{1,2}(?:[T ]\d{1,2}:\d{2}:\d{2}(?:\.\d+)?)?\b",
    )
    .unwrap();
    let identifier = regex::Regex::new(r"\b[0-9a-fA-F]{8,}\b").unwrap();
    let mut reader = capture.readers()?;
    let mut groups: HashMap<String, (usize, String)> = HashMap::new();
    for line in &capture.timeline {
        let text = reader.read_display_line(line)?;
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }
        let normalized = identifier
            .replace_all(&timestamp.replace_all(trimmed, "<TIME>"), "<ID>")
            .into_owned();
        let normalized = number.replace_all(&normalized, "<N>").into_owned();
        let key = normalized.chars().take(160).collect::<String>();
        let entry = groups
            .entry(key)
            .or_insert_with(|| (0, trimmed.chars().take(240).collect()));
        entry.0 += 1;
    }
    let mut values: Vec<_> = groups.into_values().collect();
    values.retain(|(count, _)| *count > 1);
    values.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    values.truncate(maximum);
    Ok(values)
}

fn reason_template(line: &LineMeta) -> String {
    let mut stable_reasons: Vec<&str> = line
        .reasons
        .iter()
        .map(String::as_str)
        .filter(|reason| !reason.starts_with("position+") && *reason != "informativeness")
        .collect();
    stable_reasons.sort_unstable();
    format!("{}:{}", line.stream, stable_reasons.join("+"))
}

pub fn detected_paths(capture: &CaptureResult) -> Result<Vec<String>, String> {
    let mut readers = capture.readers()?;
    let mut lines: Vec<&LineMeta> = capture.timeline.iter().collect();
    lines.sort_by_key(|line| Reverse(line.score));
    let mut seen = HashSet::new();
    let mut output = Vec::new();
    for line in lines.into_iter().take(100) {
        let clean = readers.read_display_line(line)?;
        for token in clean.split_whitespace() {
            let candidate = token.trim_matches(|character: char| {
                matches!(character, ',' | ';' | ')' | '(' | '[' | ']' | '"' | '\'')
            });
            if is_path_like(candidate) && seen.insert(candidate.to_string()) {
                output.push(candidate.to_string());
                if output.len() == 20 {
                    return Ok(output);
                }
            }
        }
    }
    Ok(output)
}

pub fn suggested_keywords(
    capture: &CaptureResult,
    _user_keywords: &[String],
) -> Result<Vec<String>, String> {
    let mut seen = HashSet::new();
    let mut output = Vec::new();
    let mut lines: Vec<&LineMeta> = capture.timeline.iter().collect();
    lines.retain(|line| {
        line.reasons.iter().any(|reason| {
            matches!(
                reason.as_str(),
                "outcome/failure"
                    | "severity/error"
                    | "failed test"
                    | "warning"
                    | "numeric anomaly"
            )
        })
    });
    lines.sort_by_key(|line| Reverse(line.score));
    let mut readers = capture.readers()?;
    for line in lines.into_iter().take(12) {
        let clean = readers.read_display_line(line)?;
        for candidate in structured_search_terms(&clean) {
            let lower = candidate.to_lowercase();
            if seen.insert(lower) {
                output.push(candidate);
                if output.len() == 5 {
                    return Ok(output);
                }
            }
        }
    }
    Ok(output)
}

fn structured_search_terms(clean: &str) -> Vec<String> {
    let trimmed = clean.trim();
    let lower = trimmed.to_lowercase();
    let mut output = Vec::new();
    if lower.starts_with("not ok ")
        && let Some((_, description)) = trimmed.split_once(" - ")
    {
        let phrase = description.split(':').next().unwrap_or(description).trim();
        if !phrase.is_empty() {
            output.push(util::single_line_clip(phrase, 80));
        }
    }
    if lower.starts_with("failed ")
        && let Some(identifier) = trimmed.split_whitespace().nth(1)
    {
        output.push(identifier.trim_end_matches(':').to_string());
    }
    if lower.starts_with("diff in ") {
        let location = trimmed[8..].trim_end_matches(':');
        let path = location.split(':').next().unwrap_or(location);
        if let Some(name) = std::path::Path::new(path)
            .file_name()
            .and_then(|v| v.to_str())
        {
            output.push(name.to_string());
        }
    }
    if lower.starts_with("error: could not compile") {
        output.push("could not compile".into());
    }
    let code = regex::Regex::new(r"\b(?:E\d{4}|TS\d+|ERR_[A-Z0-9_]+)\b").unwrap();
    let lint = regex::Regex::new(r"\bclippy::[a-z0-9_]+\b").unwrap();
    let exception = regex::Regex::new(r"\b[A-Za-z][A-Za-z0-9]*(?:Error|Exception)\b").unwrap();
    let path = regex::Regex::new(
        r"\b(?:[A-Za-z0-9_.-]+/)+[A-Za-z0-9_.-]+\.(?:rs|py|js|jsx|ts|tsx|c|cc|cpp|h|hpp|go|java)(?::\d+(?::\d+)?)?\b",
    )
    .unwrap();
    let matchers = if lower.starts_with("diff in ") {
        vec![&code, &lint, &exception]
    } else {
        vec![&code, &lint, &exception, &path]
    };
    for re in matchers {
        for found in re.find_iter(trimmed) {
            output.push(found.as_str().to_string());
        }
    }
    output
}

fn score_line(
    clean: &str,
    stream: StreamKind,
    line_number: usize,
    total: usize,
    exit_code: i32,
    keywords: &[String],
) -> (i64, Vec<String>) {
    let lower = clean.to_lowercase();
    let trimmed = lower.trim_start();
    let tokens = lexical_tokens(&lower);
    let mut score = 0_i64;
    let mut reasons = Vec::new();
    let successful_check = (trimmed.starts_with("ok ") && trimmed.contains(" - "))
        || (trimmed.starts_with("test ") && trimmed.ends_with(" ... ok"));
    let successful_outcome = is_success_outcome(trimmed);
    let successful = successful_check || successful_outcome;
    let diff_content = matches!(trimmed.chars().next(), Some('+' | '-'))
        || trimmed.starts_with("b+")
        || trimmed.starts_with("b-");
    if is_failure_outcome(trimmed) {
        score += 220;
        reasons.push("outcome/failure".to_string());
    } else if successful_outcome {
        score += 180;
        reasons.push("outcome/success".to_string());
    } else if successful_check {
        score += 20;
        reasons.push("successful check".to_string());
    }
    let severe = [
        "fatal",
        "error",
        "failure",
        "failed",
        "panic",
        "exception",
        "traceback",
        "timeout",
    ];
    if !successful
        && !diff_content
        && (severe.iter().any(|word| tokens.contains(*word))
            || [
                "permission denied",
                "no such file",
                "command not found",
                "timed out",
            ]
            .iter()
            .any(|phrase| lower.contains(phrase)))
    {
        score += 100;
        reasons.push("severity/error".to_string());
    }
    if !successful
        && tokens.contains("test")
        && (tokens.contains("fail") || tokens.contains("failed"))
    {
        score += 70;
        reasons.push("failed test".to_string());
    }
    if !successful && !diff_content && (tokens.contains("warning") || lower.contains("warn:")) {
        score += 40;
        reasons.push("warning".to_string());
    }
    if score < 80
        && ["note", "remark", "info"]
            .iter()
            .any(|word| tokens.contains(*word))
    {
        score += 5;
        reasons.push("note/info".to_string());
    }
    for keyword in keywords {
        if !successful && !keyword.is_empty() && util::unicode_contains_ci(clean, keyword) {
            score += 80;
            reasons.push(format!("keyword:{keyword}"));
        }
    }
    if is_path_like(clean) {
        score += 30;
        reasons.push("file/path".to_string());
    }
    if is_metric_line(&lower) {
        score += 25;
        reasons.push("metric/table-like".to_string());
    }
    if tokens.contains("todo") || tokens.contains("pira") {
        score += 20;
        reasons.push("TODO/PIRA marker".to_string());
    }
    if lower.contains(" at ")
        || lower.trim_start().starts_with("at ")
        || lower.contains("stack backtrace")
    {
        score += 15;
        reasons.push("stack/frame".to_string());
    }
    if stream == StreamKind::Stderr && !is_progress_noise(&lower) {
        score += 10;
        reasons.push("stderr".to_string());
    }
    let position = position_boost(line_number, total);
    if position > 0 {
        score += position;
        reasons.push(format!("position+{position}"));
    }
    if exit_code != 0
        && line_number + 5 > total
        && (stream == StreamKind::Stderr
            || ["exit", "fail", "failed", "error"]
                .iter()
                .any(|word| tokens.contains(*word)))
    {
        score += 20;
        reasons.push("nonzero-exit tail".to_string());
    }
    if !successful && has_structured_numeric_anomaly(&tokens, &lower) {
        score += 35;
        reasons.push("numeric anomaly".to_string());
    }
    let token_bonus = tokens
        .iter()
        .filter(|token| !is_stopword(token))
        .count()
        .min(10) as i64;
    score += token_bonus;
    if token_bonus > 0 {
        reasons.push("informativeness".to_string());
    }
    (score, reasons)
}

fn lexical_tokens(value: &str) -> HashSet<String> {
    value
        .split(|character: char| !character.is_alphanumeric() && character != '_')
        .filter(|token| !token.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn has_structured_numeric_anomaly(tokens: &HashSet<String>, lower: &str) -> bool {
    ["nan", "inf", "infinity", "overflow", "underflow"]
        .iter()
        .any(|token| tokens.contains(*token))
        && (is_metric_line(lower)
            || lower.contains("=nan")
            || lower.contains(": nan")
            || lower.contains("overflowerror")
            || lower.contains("arithmetic overflow"))
}

fn is_failure_outcome(trimmed: &str) -> bool {
    trimmed.starts_with("not ok ")
        || trimmed.starts_with("failed ")
        || trimmed.starts_with("error: could not")
        || trimmed.starts_with("error: aborting")
        || trimmed.starts_with("fatal:")
        || trimmed.starts_with("traceback ")
        || trimmed.starts_with("diff in ")
        || trimmed.contains("test result: failed")
        || trimmed.contains("process completed with exit code")
        || trimmed.contains("tests failed")
}

fn is_success_outcome(trimmed: &str) -> bool {
    trimmed.starts_with("test result: ok")
        || trimmed.starts_with("result: ok")
        || trimmed.starts_with("ok:")
        || trimmed == "verification passed."
        || trimmed == "verification passed"
        || trimmed.ends_with("all tests passed")
}

fn position_boost(line: usize, total: usize) -> i64 {
    let boosts = [15, 12, 9, 6, 3];
    if (1..=5).contains(&line) {
        return boosts[line - 1];
    }
    if total >= line && total - line < 5 {
        return boosts[total - line];
    }
    0
}

fn is_progress_noise(lower: &str) -> bool {
    lower.contains('%') && (lower.contains("download") || lower.contains("progress"))
}

fn is_metric_line(lower: &str) -> bool {
    let keys = [
        "accuracy",
        "loss",
        "metric",
        "score",
        "auc",
        "f1",
        "precision",
        "recall",
        "passed",
        "failed",
        "result",
    ];
    let has_key = keys.iter().any(|key| {
        lower
            .split(|c: char| !c.is_alphanumeric())
            .any(|token| token == *key)
    });
    has_key && lower.chars().any(|character| character.is_ascii_digit())
}

fn is_path_like(value: &str) -> bool {
    value.split_whitespace().any(|raw| {
        let token = raw.trim_matches(|character: char| {
            matches!(character, ',' | ';' | ')' | '(' | '[' | ']' | '"' | '\'')
        });
        let path_part = token.split(':').next().unwrap_or(token);
        let extension = std::path::Path::new(path_part)
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase);
        let recognized = matches!(
            extension.as_deref(),
            Some(
                "rs" | "py"
                    | "md"
                    | "txt"
                    | "sh"
                    | "c"
                    | "cc"
                    | "cpp"
                    | "h"
                    | "hpp"
                    | "js"
                    | "jsx"
                    | "ts"
                    | "tsx"
                    | "go"
                    | "java"
                    | "kt"
                    | "swift"
                    | "toml"
                    | "yaml"
                    | "yml"
                    | "json"
            )
        );
        recognized && (token.contains('/') || token.contains('\\') || token.contains(':'))
    })
}

fn is_stopword(word: &str) -> bool {
    matches!(
        word,
        "the"
            | "and"
            | "for"
            | "with"
            | "this"
            | "that"
            | "from"
            | "into"
            | "have"
            | "has"
            | "are"
            | "was"
            | "were"
            | "you"
            | "your"
            | "but"
            | "not"
            | "all"
            | "out"
            | "line"
            | "lines"
            | "noise"
            | "more"
            | "done"
            | "begin"
            | "end"
            | "build"
            | "word"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_test_name_is_not_a_diagnostic() {
        let (_, reasons) = score_line(
            "ok 39 - error beats noise",
            StreamKind::Stdout,
            39,
            100,
            1,
            &[],
        );
        assert!(reasons.iter().any(|reason| reason == "successful check"));
        assert!(!reasons.iter().any(|reason| reason == "severity/error"));
        assert!(!reasons.iter().any(|reason| reason == "failed test"));
    }

    #[test]
    fn actual_test_failure_is_an_outcome() {
        let (_, reasons) = score_line(
            "not ok 72 - direct transform count mismatch",
            StreamKind::Stdout,
            72,
            100,
            1,
            &[],
        );
        assert!(reasons.iter().any(|reason| reason == "outcome/failure"));
    }

    #[test]
    fn successful_result_does_not_trigger_failed_test() {
        let (_, reasons) = score_line(
            "test result: ok. 11 passed; 0 failed",
            StreamKind::Stdout,
            10,
            10,
            0,
            &[],
        );
        assert!(reasons.iter().any(|reason| reason == "outcome/success"));
        assert!(!reasons.iter().any(|reason| reason == "failed test"));
    }

    #[test]
    fn rust_test_harness_success_is_not_a_diagnostic() {
        let (_, reasons) = score_line(
            "test parser::failed_input_is_rejected ... ok",
            StreamKind::Stdout,
            8,
            10,
            0,
            &[],
        );
        assert!(reasons.iter().any(|reason| reason == "successful check"));
        assert!(!reasons.iter().any(|reason| reason == "severity/error"));
    }

    #[test]
    fn search_terms_use_diagnostic_identifiers_and_outcome_phrases() {
        assert_eq!(
            structured_search_terms(
                "not ok 72 - direct transform count mismatch: CompletedProcess(--store-dir /tmp/x)"
            ),
            vec!["direct transform count mismatch"]
        );
        let terms = structured_search_terms(
            "error[E0425]: cannot find value; clippy::collapsible_if at src/lib.rs:42:7",
        );
        assert!(terms.contains(&"E0425".to_string()));
        assert!(terms.contains(&"clippy::collapsible_if".to_string()));
        assert!(terms.contains(&"src/lib.rs:42:7".to_string()));
    }

    #[test]
    fn successful_summary_reserves_latest_check() {
        let lines = (1..=5)
            .map(|line| LineMeta {
                line,
                stream: StreamKind::Stdout,
                offset: 0,
                length: 1,
                score: if line == 1 { 100 } else { 20 },
                reasons: vec!["successful check".into()],
            })
            .collect::<Vec<_>>();
        assert!(select_important(&lines, 2).contains(&4));
    }
}
