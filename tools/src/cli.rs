use std::path::PathBuf;

pub const USAGE: &str = "\
Usage:
  pira_ctx [--store-dir PATH] --intent TEXT [--keyword QUERY ...] -- <command> [args...]
  pira_ctx exact --intent TEXT [--store-dir PATH] -- <command> [args...]
  pira_ctx check --intent TEXT [--store-dir PATH] -- <command> [args...]
  pira_ctx capture|summary [--store-dir PATH] --intent TEXT [--keyword QUERY ...] -- <command> [args...]
  pira_ctx batch [--store-dir PATH] SPEC_FILE [--intent TEXT]
  pira_ctx search [--store-dir PATH] RESULT QUERY [--regex] [--context N]
  pira_ctx range [--store-dir PATH] RESULT START_LINE END_LINE
  pira_ctx raw [--store-dir PATH] RESULT [--stdout|--stderr]
  pira_ctx transform [--store-dir PATH] RESULT [--plan FILE] [--match REGEX] [--exclude REGEX]
                     [--unique] [--count] [--head N] [--tail N]
  pira_ctx recap [--store-dir PATH] [--limit N]
  pira_ctx stats [--store-dir PATH] [RESULT]
  pira_ctx verify [--store-dir PATH] RESULT
  pira_ctx list [--store-dir PATH] [--workspace current]
  pira_ctx prune [--store-dir PATH] [--max-age-days N] [--max-store-bytes N]
  pira_ctx forget [--store-dir PATH] RESULT|events
  pira_ctx --help | --version

RESULT may be --last, a result ID/prefix, filename, or path.
INTENT must be a non-empty, single-line purpose of at most 256 UTF-8 bytes.
Non-interactive exact mode retains and summarizes output only when it is both long and highly repetitive.";

pub const MAX_INTENT_BYTES: usize = 256;
pub const MAX_KEYWORDS: usize = 16;
pub const MAX_KEYWORD_BYTES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Auto,
    Exact,
    Check,
    Capture,
    Search,
    Range,
    Raw,
    Transform,
    Recap,
    Batch,
    List,
    Stats,
    Verify,
    Prune,
    Forget,
    Help,
    Version,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Default)]
pub struct TransformOptions {
    pub plan: Option<PathBuf>,
    pub matches: Vec<String>,
    pub excludes: Vec<String>,
    pub unique: bool,
    pub count: bool,
    pub head: Option<usize>,
    pub tail: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub mode: Mode,
    pub store_dir: Option<PathBuf>,
    pub intent: Option<String>,
    pub keywords: Vec<String>,
    pub cmd: Vec<String>,
    pub target: Option<String>,
    pub query: Option<String>,
    pub regex: bool,
    pub context: usize,
    pub start_line: Option<i64>,
    pub end_line: Option<i64>,
    pub workspace_current: bool,
    pub raw_stream: Option<RawStream>,
    pub max_age_days: Option<u64>,
    pub max_store_bytes: Option<u64>,
    pub transform: TransformOptions,
    pub limit: usize,
    pub batch_file: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: Mode::Auto,
            store_dir: None,
            intent: None,
            keywords: vec![],
            cmd: vec![],
            target: None,
            query: None,
            regex: false,
            context: 0,
            start_line: None,
            end_line: None,
            workspace_current: false,
            raw_stream: None,
            max_age_days: None,
            max_store_bytes: None,
            transform: TransformOptions::default(),
            limit: 20,
            batch_file: None,
        }
    }
}

pub fn parse_args(args: &[String]) -> Result<Config, String> {
    if args.is_empty() {
        return Err(USAGE.into());
    }
    if matches!(args[0].as_str(), "--help" | "-h" | "help") {
        return Ok(Config {
            mode: Mode::Help,
            ..Default::default()
        });
    }
    if matches!(args[0].as_str(), "--version" | "-V" | "version") {
        return Ok(Config {
            mode: Mode::Version,
            ..Default::default()
        });
    }
    let mut c = Config::default();
    match args[0].as_str() {
        "exact" => {
            c.mode = Mode::Exact;
            let p = parse_exec_options(&mut c, args, 1, false)?;
            parse_command(&mut c, args, p)?;
            require_intent(&mut c)?;
        }
        "check" => {
            c.mode = Mode::Check;
            let p = parse_exec_options(&mut c, args, 1, false)?;
            parse_command(&mut c, args, p)?;
            require_intent(&mut c)?;
        }
        "capture" | "summary" => {
            c.mode = Mode::Capture;
            let p = parse_exec_options(&mut c, args, 1, true)?;
            parse_command(&mut c, args, p)?;
            require_intent(&mut c)?;
        }
        "search" => parse_search(&mut c, args)?,
        "range" => {
            c.mode = Mode::Range;
            let mut p = parse_store(&mut c, args, 1)?;
            if p + 3 != args.len() {
                return Err(USAGE.into());
            }
            c.target = Some(args[p].clone());
            p += 1;
            c.start_line = Some(args[p].parse().map_err(|_| "invalid start_line")?);
            p += 1;
            c.end_line = Some(args[p].parse().map_err(|_| "invalid end_line")?);
        }
        "raw" => parse_raw(&mut c, args)?,
        "transform" => parse_transform(&mut c, args)?,
        "recap" => {
            c.mode = Mode::Recap;
            let mut p = parse_store(&mut c, args, 1)?;
            while p < args.len() {
                if args[p] != "--limit" {
                    return Err(USAGE.into());
                }
                p += 1;
                c.limit = parse_value(args, &mut p, "--limit")?;
            }
        }
        "batch" => {
            c.mode = Mode::Batch;
            let mut p = parse_store(&mut c, args, 1)?;
            c.batch_file = Some(PathBuf::from(take(args, &mut p, "SPEC_FILE")?));
            while p < args.len() {
                if args[p] != "--intent" {
                    return Err(USAGE.into());
                }
                p += 1;
                c.intent = Some(take(args, &mut p, "--intent")?.into());
            }
            normalize_optional_intent(&mut c)?;
        }
        "list" => {
            c.mode = Mode::List;
            let mut p = parse_store(&mut c, args, 1)?;
            while p < args.len() {
                if args[p] != "--workspace"
                    || args.get(p + 1).map(String::as_str) != Some("current")
                {
                    return Err(USAGE.into());
                }
                c.workspace_current = true;
                p += 2;
            }
        }
        "stats" | "verify" => {
            c.mode = if args[0] == "stats" {
                Mode::Stats
            } else {
                Mode::Verify
            };
            let p = parse_store(&mut c, args, 1)?;
            if c.mode == Mode::Verify && p + 1 != args.len() {
                return Err(USAGE.into());
            }
            if p < args.len() {
                c.target = Some(args[p].clone());
                if p + 1 != args.len() {
                    return Err(USAGE.into());
                }
            }
        }
        "prune" => parse_prune(&mut c, args)?,
        "forget" => {
            c.mode = Mode::Forget;
            let p = parse_store(&mut c, args, 1)?;
            if p + 1 != args.len() {
                return Err(USAGE.into());
            }
            c.target = Some(args[p].clone());
        }
        _ => {
            let p = parse_exec_options(&mut c, args, 0, true)?;
            parse_command(&mut c, args, p)?;
            require_intent(&mut c)?;
        }
    }
    validate_keywords(&c.keywords)?;
    Ok(c)
}

fn validate_keywords(keywords: &[String]) -> Result<(), String> {
    if keywords.len() > MAX_KEYWORDS {
        return Err(format!(
            "at most {MAX_KEYWORDS} --keyword values are allowed"
        ));
    }
    for keyword in keywords {
        let trimmed = keyword.trim();
        if trimmed.is_empty()
            || trimmed.len() > MAX_KEYWORD_BYTES
            || trimmed.chars().any(char::is_control)
        {
            return Err(format!(
                "each --keyword must be non-empty, single-line, and at most {MAX_KEYWORD_BYTES} UTF-8 bytes"
            ));
        }
    }
    Ok(())
}

fn require_intent(c: &mut Config) -> Result<(), String> {
    let value = c
        .intent
        .as_deref()
        .ok_or("external execution requires --intent TEXT")?;
    c.intent = Some(validate_intent(value)?.to_string());
    Ok(())
}

fn normalize_optional_intent(c: &mut Config) -> Result<(), String> {
    if let Some(value) = c.intent.as_deref() {
        c.intent = Some(validate_intent(value)?.to_string());
    }
    Ok(())
}

pub fn validate_intent(value: &str) -> Result<&str, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("external execution requires --intent TEXT".into());
    }
    let bytes = trimmed.len();
    if bytes > MAX_INTENT_BYTES {
        return Err(format!(
            "intent is too long: {} characters, {bytes} UTF-8 bytes; maximum {MAX_INTENT_BYTES} bytes. Rerun with a concise, single-line immediate purpose",
            trimmed.chars().count()
        ));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(
            "intent must be a concise, single-line immediate purpose without control characters. Rerun with a corrected intent"
                .into(),
        );
    }
    Ok(trimmed)
}
fn parse_command(c: &mut Config, args: &[String], p: usize) -> Result<(), String> {
    if args.get(p).map(String::as_str) != Some("--") || p + 1 >= args.len() {
        return Err(USAGE.into());
    }
    c.cmd = args[p + 1..].to_vec();
    Ok(())
}
fn parse_exec_options(
    c: &mut Config,
    args: &[String],
    mut p: usize,
    keywords: bool,
) -> Result<usize, String> {
    while p < args.len() {
        match args[p].as_str() {
            "--store-dir" => {
                p += 1;
                c.store_dir = Some(take(args, &mut p, "--store-dir")?.into())
            }
            "--intent" => {
                p += 1;
                c.intent = Some(take(args, &mut p, "--intent")?.into())
            }
            "--keyword" if keywords => {
                p += 1;
                c.keywords.push(take(args, &mut p, "--keyword")?.into())
            }
            "--" => break,
            _ => return Err(USAGE.into()),
        }
    }
    Ok(p)
}
fn parse_store(c: &mut Config, args: &[String], mut p: usize) -> Result<usize, String> {
    if args.get(p).map(String::as_str) == Some("--store-dir") {
        p += 1;
        c.store_dir = Some(take(args, &mut p, "--store-dir")?.into())
    }
    Ok(p)
}
fn parse_search(c: &mut Config, args: &[String]) -> Result<(), String> {
    c.mode = Mode::Search;
    let mut p = parse_store(c, args, 1)?;
    c.target = Some(take(args, &mut p, "RESULT")?.into());
    c.query = Some(take(args, &mut p, "QUERY")?.into());
    while p < args.len() {
        match args[p].as_str() {
            "--regex" => {
                c.regex = true;
                p += 1
            }
            "--context" => {
                p += 1;
                c.context = parse_value(args, &mut p, "--context")?
            }
            _ => return Err(USAGE.into()),
        }
    }
    Ok(())
}
fn parse_raw(c: &mut Config, args: &[String]) -> Result<(), String> {
    c.mode = Mode::Raw;
    let mut p = parse_store(c, args, 1)?;
    c.target = Some(take(args, &mut p, "RESULT")?.into());
    while p < args.len() {
        let s = match args[p].as_str() {
            "--stdout" => RawStream::Stdout,
            "--stderr" => RawStream::Stderr,
            _ => return Err(USAGE.into()),
        };
        if c.raw_stream.replace(s).is_some() {
            return Err("choose only one stream".into());
        }
        p += 1
    }
    Ok(())
}
fn parse_transform(c: &mut Config, args: &[String]) -> Result<(), String> {
    c.mode = Mode::Transform;
    let mut p = parse_store(c, args, 1)?;
    c.target = Some(take(args, &mut p, "RESULT")?.into());
    while p < args.len() {
        match args[p].as_str() {
            "--plan" => {
                p += 1;
                c.transform.plan = Some(take(args, &mut p, "--plan")?.into())
            }
            "--match" => {
                p += 1;
                c.transform
                    .matches
                    .push(take(args, &mut p, "--match")?.into())
            }
            "--exclude" => {
                p += 1;
                c.transform
                    .excludes
                    .push(take(args, &mut p, "--exclude")?.into())
            }
            "--unique" => {
                c.transform.unique = true;
                p += 1
            }
            "--count" => {
                c.transform.count = true;
                p += 1
            }
            "--head" => {
                p += 1;
                c.transform.head = Some(parse_value(args, &mut p, "--head")?)
            }
            "--tail" => {
                p += 1;
                c.transform.tail = Some(parse_value(args, &mut p, "--tail")?)
            }
            _ => return Err(USAGE.into()),
        }
    }
    Ok(())
}
fn parse_prune(c: &mut Config, args: &[String]) -> Result<(), String> {
    c.mode = Mode::Prune;
    let mut p = 1;
    while p < args.len() {
        match args[p].as_str() {
            "--store-dir" => {
                p += 1;
                c.store_dir = Some(take(args, &mut p, "--store-dir")?.into())
            }
            "--max-age-days" => {
                p += 1;
                c.max_age_days = Some(parse_value(args, &mut p, "--max-age-days")?)
            }
            "--max-store-bytes" => {
                p += 1;
                c.max_store_bytes = Some(parse_value(args, &mut p, "--max-store-bytes")?)
            }
            _ => return Err(USAGE.into()),
        }
    }
    if c.max_age_days.is_none() && c.max_store_bytes.is_none() {
        return Err("prune requires a limit".into());
    }
    Ok(())
}
fn take<'a>(args: &'a [String], p: &mut usize, name: &str) -> Result<&'a str, String> {
    let v = args.get(*p).ok_or_else(|| format!("missing {name}"))?;
    *p += 1;
    Ok(v)
}
fn parse_value<T: std::str::FromStr>(
    args: &[String],
    p: &mut usize,
    name: &str,
) -> Result<T, String> {
    take(args, p, name)?
        .parse()
        .map_err(|_| format!("invalid {name}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn a(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn intent_required() {
        assert!(parse_args(&a(&["--", "echo"])).is_err());
        assert!(parse_args(&a(&["--intent", "check", "--", "echo"])).is_ok())
    }
    #[test]
    fn summary_alias() {
        assert_eq!(
            parse_args(&a(&["summary", "--intent", "x", "--", "echo"]))
                .unwrap()
                .mode,
            Mode::Capture
        )
    }
    #[test]
    fn check_is_an_intent_required_execution_mode() {
        assert!(parse_args(&a(&["check", "--", "echo"])).is_err());
        assert_eq!(
            parse_args(&a(&["check", "--intent", "validate", "--", "echo"]))
                .unwrap()
                .mode,
            Mode::Check
        );
    }
    #[test]
    fn internal_needs_no_intent() {
        assert!(parse_args(&a(&["search", "--last", "x"])).is_ok())
    }

    #[test]
    fn intent_size_is_utf8_bytes() {
        assert!(validate_intent(&"a".repeat(256)).is_ok());
        assert!(validate_intent(&"a".repeat(257)).is_err());
        assert!(validate_intent(&"界".repeat(85)).is_ok());
        assert!(validate_intent(&"界".repeat(86)).is_err());
        assert!(validate_intent("one\ntwo").is_err());
    }

    #[test]
    fn keyword_count_and_size_are_bounded() {
        assert!(validate_keywords(&vec!["x".into(); MAX_KEYWORDS]).is_ok());
        assert!(validate_keywords(&vec!["x".into(); MAX_KEYWORDS + 1]).is_err());
        assert!(validate_keywords(&["x".repeat(MAX_KEYWORD_BYTES + 1)]).is_err());
    }
}
