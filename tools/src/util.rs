use std::io::{self, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const BROKEN_PIPE: &str = "__PIRA_CTX_BROKEN_PIPE__";
pub const MAX_DISPLAY_READ_BYTES: u64 = 64 * 1024;
pub const MAX_SEARCH_LINE_BYTES: u64 = 8 * 1024 * 1024;
const DISPLAY_CLIP_BYTES: usize = 1200;

pub fn io_error(error: io::Error) -> String {
    if error.kind() == io::ErrorKind::BrokenPipe {
        BROKEN_PIPE.to_string()
    } else {
        error.to_string()
    }
}

pub fn stdout_line(text: &str) -> Result<(), String> {
    let mut output = io::stdout().lock();
    writeln!(output, "{}", sanitize_terminal(text)).map_err(io_error)
}

pub fn millis(time: SystemTime) -> u128 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis()
}

pub fn status_code(status: std::process::ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    125
}

pub fn argv_display(argv: &[String]) -> String {
    argv.iter()
        .map(|arg| shellish_quote(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shellish_quote(value: &str) -> String {
    if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "_+-=./:".contains(character))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

pub fn unicode_contains_ci(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

pub fn sanitize_terminal(value: &str) -> String {
    let mut output = String::new();
    let mut chars = value.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '\u{1b}' {
            match chars.peek().copied() {
                Some('[') => {
                    chars.next();
                    for next in chars.by_ref() {
                        if ('@'..='~').contains(&next) {
                            break;
                        }
                    }
                }
                Some(']') | Some('P') | Some('X') | Some('^') | Some('_') => {
                    chars.next();
                    let mut previous_escape = false;
                    for next in chars.by_ref() {
                        if next == '\u{7}' || (previous_escape && next == '\\') {
                            break;
                        }
                        previous_escape = next == '\u{1b}';
                    }
                }
                Some(_) => {
                    chars.next();
                }
                None => {}
            }
        } else if character == '\r' {
            // Carriage returns can rewrite terminal output; render them as line boundaries.
            output.push(' ');
        } else if character.is_control() && !matches!(character, '\n' | '\t') {
            // Suppress terminal controls while retaining readable whitespace.
        } else {
            output.push(character);
        }
    }
    output.trim_end_matches(['\n', '\r']).to_string()
}

pub fn clip_display(value: &str) -> String {
    if value.len() <= DISPLAY_CLIP_BYTES {
        return value.to_string();
    }
    let start = safe_prefix(value, 600);
    let end = safe_suffix(value, 300);
    let clipped = value.len().saturating_sub(start.len() + end.len());
    format!(
        "{start} … clipped {clipped} bytes (~{} words) … {end}",
        clipped / 6
    )
}

fn safe_prefix(value: &str, bytes: usize) -> &str {
    let mut end = bytes.min(value.len());
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

fn safe_suffix(value: &str, bytes: usize) -> &str {
    let mut start = value.len().saturating_sub(bytes);
    while !value.is_char_boundary(start) {
        start += 1;
    }
    &value[start..]
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_csi_osc_and_carriage_return() {
        let input = "\x1b[31mred\x1b[0m \x1b]8;;https://example.test\x07label\x1b]8;;\x07\rnext";
        assert_eq!(sanitize_terminal(input), "red label next");
    }

    #[test]
    fn unicode_search_is_case_insensitive() {
        assert!(unicode_contains_ci("CAFÉ diagnostic", "café"));
    }
}
