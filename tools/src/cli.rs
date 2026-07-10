use std::path::PathBuf;

pub const USAGE: &str = "\
Usage:
  pira_ctx [--store-dir PATH] [--keyword QUERY ...] -- <command> [args...]
  pira_ctx exact -- <command> [args...]
  pira_ctx summary [--store-dir PATH] [--keyword QUERY ...] -- <command> [args...]
  pira_ctx search [--store-dir PATH] RESULT QUERY [--regex] [--context N]
  pira_ctx range [--store-dir PATH] RESULT START_LINE END_LINE
  pira_ctx raw [--store-dir PATH] RESULT [--stdout|--stderr]
  pira_ctx stats|verify [--store-dir PATH] RESULT
  pira_ctx list [--store-dir PATH] [--workspace current]
  pira_ctx prune [--store-dir PATH] [--max-age-days N] [--max-store-bytes N]
  pira_ctx --help | --version

RESULT may be --last, a result ID/prefix, filename, or path.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Auto,
    Exact,
    Summary,
    Search,
    Range,
    Raw,
    List,
    Stats,
    Verify,
    Prune,
    Help,
    Version,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub mode: Mode,
    pub store_dir: Option<PathBuf>,
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: Mode::Auto,
            store_dir: None,
            keywords: Vec::new(),
            cmd: Vec::new(),
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
        }
    }
}

pub fn parse_args(args: &[String]) -> Result<Config, String> {
    if args.is_empty() {
        return Err(USAGE.to_string());
    }
    if matches!(args[0].as_str(), "--help" | "-h" | "help") {
        return Ok(Config {
            mode: Mode::Help,
            ..Config::default()
        });
    }
    if matches!(args[0].as_str(), "--version" | "-V" | "version") {
        return Ok(Config {
            mode: Mode::Version,
            ..Config::default()
        });
    }

    let mut config = Config::default();
    match args[0].as_str() {
        "exact" => {
            config.mode = Mode::Exact;
            if args.get(1).map(String::as_str) != Some("--") || args.len() < 3 {
                return Err(USAGE.to_string());
            }
            config.cmd = args[2..].to_vec();
        }
        "summary" => {
            config.mode = Mode::Summary;
            let position = parse_common_options(&mut config, args, 1)?;
            parse_command(&mut config, args, position)?;
        }
        "search" => {
            config.mode = Mode::Search;
            let mut position = parse_store_only(&mut config, args, 1)?;
            config.target = args.get(position).cloned();
            position += 1;
            config.query = args.get(position).cloned();
            position += 1;
            if config.target.is_none() || config.query.is_none() {
                return Err(USAGE.to_string());
            }
            while position < args.len() {
                match args[position].as_str() {
                    "--regex" => {
                        config.regex = true;
                        position += 1;
                    }
                    "--context" => {
                        position += 1;
                        config.context = parse_value(args, &mut position, "--context")?;
                    }
                    _ => return Err(USAGE.to_string()),
                }
            }
        }
        "range" => {
            config.mode = Mode::Range;
            let mut position = parse_store_only(&mut config, args, 1)?;
            if position + 3 != args.len() {
                return Err(USAGE.to_string());
            }
            config.target = Some(args[position].clone());
            position += 1;
            config.start_line = Some(
                args[position]
                    .parse()
                    .map_err(|_| "invalid start_line".to_string())?,
            );
            position += 1;
            config.end_line = Some(
                args[position]
                    .parse()
                    .map_err(|_| "invalid end_line".to_string())?,
            );
        }
        "raw" => {
            config.mode = Mode::Raw;
            let mut position = parse_store_only(&mut config, args, 1)?;
            config.target = args.get(position).cloned();
            position += 1;
            if config.target.is_none() {
                return Err(USAGE.to_string());
            }
            while position < args.len() {
                let stream = match args[position].as_str() {
                    "--stdout" => RawStream::Stdout,
                    "--stderr" => RawStream::Stderr,
                    _ => return Err(USAGE.to_string()),
                };
                if config.raw_stream.replace(stream).is_some() {
                    return Err("choose only one of --stdout or --stderr".to_string());
                }
                position += 1;
            }
        }
        "list" => {
            config.mode = Mode::List;
            let mut position = parse_store_only(&mut config, args, 1)?;
            while position < args.len() {
                if args[position] != "--workspace"
                    || args.get(position + 1).map(String::as_str) != Some("current")
                {
                    return Err(USAGE.to_string());
                }
                config.workspace_current = true;
                position += 2;
            }
        }
        "stats" | "verify" => {
            config.mode = if args[0] == "stats" {
                Mode::Stats
            } else {
                Mode::Verify
            };
            let position = parse_store_only(&mut config, args, 1)?;
            if position + 1 != args.len() {
                return Err(USAGE.to_string());
            }
            config.target = Some(args[position].clone());
        }
        "prune" => {
            config.mode = Mode::Prune;
            let mut position = 1;
            while position < args.len() {
                match args[position].as_str() {
                    "--store-dir" => {
                        position += 1;
                        config.store_dir =
                            Some(PathBuf::from(take_arg(args, &mut position, "--store-dir")?));
                    }
                    "--max-age-days" => {
                        position += 1;
                        config.max_age_days =
                            Some(parse_value(args, &mut position, "--max-age-days")?);
                    }
                    "--max-store-bytes" => {
                        position += 1;
                        config.max_store_bytes =
                            Some(parse_value(args, &mut position, "--max-store-bytes")?);
                    }
                    _ => return Err(USAGE.to_string()),
                }
            }
            if config.max_age_days.is_none() && config.max_store_bytes.is_none() {
                return Err("prune requires --max-age-days and/or --max-store-bytes".to_string());
            }
        }
        _ => {
            let position = parse_common_options(&mut config, args, 0)?;
            parse_command(&mut config, args, position)?;
        }
    }
    Ok(config)
}

fn parse_command(config: &mut Config, args: &[String], position: usize) -> Result<(), String> {
    if args.get(position).map(String::as_str) != Some("--") || position + 1 >= args.len() {
        return Err(USAGE.to_string());
    }
    config.cmd = args[position + 1..].to_vec();
    Ok(())
}

fn parse_common_options(
    config: &mut Config,
    args: &[String],
    mut position: usize,
) -> Result<usize, String> {
    while position < args.len() {
        match args[position].as_str() {
            "--store-dir" => {
                position += 1;
                config.store_dir =
                    Some(PathBuf::from(take_arg(args, &mut position, "--store-dir")?));
            }
            "--keyword" => {
                position += 1;
                config
                    .keywords
                    .push(take_arg(args, &mut position, "--keyword")?.to_string());
            }
            "--" => break,
            _ => return Err(USAGE.to_string()),
        }
    }
    Ok(position)
}

fn parse_store_only(
    config: &mut Config,
    args: &[String],
    mut position: usize,
) -> Result<usize, String> {
    if args.get(position).map(String::as_str) == Some("--store-dir") {
        position += 1;
        config.store_dir = Some(PathBuf::from(take_arg(args, &mut position, "--store-dir")?));
    }
    Ok(position)
}

fn take_arg<'a>(args: &'a [String], position: &mut usize, name: &str) -> Result<&'a str, String> {
    let value = args
        .get(*position)
        .ok_or_else(|| format!("missing value for {name}"))?;
    *position += 1;
    Ok(value)
}

fn parse_value<T: std::str::FromStr>(
    args: &[String],
    position: &mut usize,
    name: &str,
) -> Result<T, String> {
    take_arg(args, position, name)?
        .parse()
        .map_err(|_| format!("invalid {name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn exact_requires_separator_immediately() {
        assert!(parse_args(&args(&["exact", "junk", "--", "echo"])).is_err());
        assert_eq!(
            parse_args(&args(&["exact", "--", "echo", "hello"]))
                .unwrap()
                .cmd,
            args(&["echo", "hello"])
        );
    }

    #[test]
    fn raw_streams_are_mutually_exclusive() {
        assert!(parse_args(&args(&["raw", "--last", "--stdout", "--stderr"])).is_err());
    }
}
