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
    let mut order: Vec<usize> = (0..lines.len()).collect();
    order.sort_by_key(|&index| (Reverse(lines[index].score), lines[index].line));
    let mut selected = Vec::new();
    let mut templates: HashMap<String, usize> = HashMap::new();
    for index in order {
        if selected.len() >= maximum {
            break;
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
    user_keywords: &[String],
) -> Result<Vec<String>, String> {
    let mut seen = HashSet::new();
    let mut output = Vec::new();
    for keyword in user_keywords {
        if seen.insert(keyword.to_lowercase()) {
            output.push(keyword.clone());
        }
    }
    let mut lines: Vec<&LineMeta> = capture.timeline.iter().collect();
    lines.sort_by_key(|line| Reverse(line.score));
    let mut readers = capture.readers()?;
    for line in lines.into_iter().take(20) {
        let clean = readers.read_display_line(line)?;
        for raw in clean.split_whitespace() {
            let token = raw.trim_matches(|character: char| !is_keyword_char(character));
            if !(3..=120).contains(&token.len()) {
                continue;
            }
            let lower = token.to_lowercase();
            if is_stopword(&lower) || !seen.insert(lower) {
                continue;
            }
            output.push(token.to_string());
            if output.len() == 20 {
                return Ok(output);
            }
        }
    }
    Ok(output)
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
    let tokens = lexical_tokens(&lower);
    let mut score = 0_i64;
    let mut reasons = Vec::new();
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
    if severe.iter().any(|word| tokens.contains(*word))
        || [
            "permission denied",
            "no such file",
            "command not found",
            "timed out",
        ]
        .iter()
        .any(|phrase| lower.contains(phrase))
    {
        score += 100;
        reasons.push("severity/error".to_string());
    }
    if tokens.contains("test") && (tokens.contains("fail") || tokens.contains("failed")) {
        score += 70;
        reasons.push("failed test".to_string());
    }
    if tokens.contains("warning") || lower.contains("warn:") {
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
        if !keyword.is_empty() && util::unicode_contains_ci(clean, keyword) {
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
    if has_numeric_anomaly(&tokens) {
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

fn has_numeric_anomaly(tokens: &HashSet<String>) -> bool {
    ["nan", "inf", "infinity", "overflow", "underflow"]
        .iter()
        .any(|token| tokens.contains(*token))
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

fn is_keyword_char(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '_' | '-' | '/' | '.' | ':')
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
