use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use regex::Regex;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

const MAGIC: &[u8; 8] = b"PIRACTX1";
const TOOL_VERSION: &str = "pira_ctx-0.1.0";
const COMPAT_VERSION: u32 = 1;
const AUTO_SUMMARY_THRESHOLD: usize = 3 * 1024;
const DISPLAY_CLIP_BYTES: usize = 1200;
const MAX_IMPORTANT_LINES: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Mode {
    Auto,
    Exact,
    Summary,
    Search,
    Range,
    Raw,
    List,
    Stats,
}

#[derive(Debug, Clone)]
struct Config {
    mode: Mode,
    store_dir: Option<PathBuf>,
    keywords: Vec<String>,
    cmd: Vec<String>,
    target: Option<String>,
    query: Option<String>,
    regex: bool,
    context: usize,
    start_line: Option<i64>,
    end_line: Option<i64>,
    workspace_current: bool,
}

#[derive(Debug, Clone)]
struct StreamLine {
    stream: &'static str,
    offset: usize,
    len: usize,
}

#[derive(Debug, Clone)]
struct LineMeta {
    no: usize,
    stream: String,
    offset: usize,
    len: usize,
    score: i64,
    reasons: Vec<String>,
}

#[derive(Debug, Clone)]
struct CaptureResult {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    timeline: Vec<LineMeta>,
    exit_code: i32,
    start_ms: u128,
    end_ms: u128,
    duration_ms: u128,
    cwd: String,
}

#[derive(Debug)]
struct StoredResult {
    metadata: String,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    path: PathBuf,
    timeline: Vec<LineMeta>,
}

fn main() {
    let code = match real_main() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("pira_ctx: {e}");
            125
        }
    };
    std::process::exit(code);
}

fn real_main() -> Result<i32, String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let cfg = parse_args(&args)?;
    match cfg.mode {
        Mode::Exact => run_exact(&cfg.cmd),
        Mode::Auto => run_auto(&cfg),
        Mode::Summary => run_summary(&cfg),
        Mode::Search => run_search(&cfg),
        Mode::Range => run_range(&cfg),
        Mode::Raw => run_raw(&cfg),
        Mode::List => run_list(&cfg),
        Mode::Stats => run_stats(&cfg),
    }
}

fn parse_args(args: &[String]) -> Result<Config, String> {
    if args.is_empty() {
        return usage_err();
    }
    let mut cfg = Config {
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
    };

    match args[0].as_str() {
        "exact" => {
            cfg.mode = Mode::Exact;
            let pos = args.iter().position(|a| a == "--").ok_or_else(usage_msg)?;
            if pos + 1 >= args.len() { return usage_err(); }
            cfg.cmd = args[pos + 1..].to_vec();
        }
        "summary" => {
            cfg.mode = Mode::Summary;
            let pos = parse_common_options(&mut cfg, args, 1)?;
            if pos >= args.len() || args[pos] != "--" || pos + 1 >= args.len() { return usage_err(); }
            cfg.cmd = args[pos + 1..].to_vec();
        }
        "search" => {
            cfg.mode = Mode::Search;
            let mut i = parse_store_only(&mut cfg, args, 1)?;
            if i >= args.len() { return usage_err(); }
            cfg.target = Some(args[i].clone());
            i += 1;
            if i >= args.len() { return usage_err(); }
            cfg.query = Some(args[i].clone());
            i += 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--regex" => { cfg.regex = true; i += 1; }
                    "--context" => {
                        i += 1;
                        if i >= args.len() { return usage_err(); }
                        cfg.context = args[i].parse::<usize>().map_err(|_| "invalid --context".to_string())?;
                        i += 1;
                    }
                    _ => return usage_err(),
                }
            }
        }
        "range" => {
            cfg.mode = Mode::Range;
            let mut i = parse_store_only(&mut cfg, args, 1)?;
            if i + 2 >= args.len() { return usage_err(); }
            cfg.target = Some(args[i].clone()); i += 1;
            cfg.start_line = Some(args[i].parse::<i64>().map_err(|_| "invalid start_line".to_string())?); i += 1;
            cfg.end_line = Some(args[i].parse::<i64>().map_err(|_| "invalid end_line".to_string())?); i += 1;
            if i != args.len() { return usage_err(); }
        }
        "raw" => {
            cfg.mode = Mode::Raw;
            let mut i = parse_store_only(&mut cfg, args, 1)?;
            if i >= args.len() { return usage_err(); }
            cfg.target = Some(args[i].clone()); i += 1;
            if i != args.len() { return usage_err(); }
        }
        "list" => {
            cfg.mode = Mode::List;
            let mut i = parse_store_only(&mut cfg, args, 1)?;
            while i < args.len() {
                match args[i].as_str() {
                    "--workspace" => {
                        i += 1;
                        if i >= args.len() || args[i] != "current" { return usage_err(); }
                        cfg.workspace_current = true;
                        i += 1;
                    }
                    _ => return usage_err(),
                }
            }
        }
        "stats" => {
            cfg.mode = Mode::Stats;
            let mut i = parse_store_only(&mut cfg, args, 1)?;
            if i >= args.len() { return usage_err(); }
            cfg.target = Some(args[i].clone()); i += 1;
            if i != args.len() { return usage_err(); }
        }
        _ => {
            cfg.mode = Mode::Auto;
            let pos = parse_common_options(&mut cfg, args, 0)?;
            if pos >= args.len() || args[pos] != "--" || pos + 1 >= args.len() { return usage_err(); }
            cfg.cmd = args[pos + 1..].to_vec();
        }
    }
    Ok(cfg)
}

fn parse_common_options(cfg: &mut Config, args: &[String], mut i: usize) -> Result<usize, String> {
    while i < args.len() {
        match args[i].as_str() {
            "--store-dir" => {
                i += 1;
                if i >= args.len() { return usage_err(); }
                cfg.store_dir = Some(PathBuf::from(&args[i]));
                i += 1;
            }
            "--keyword" => {
                i += 1;
                if i >= args.len() { return usage_err(); }
                cfg.keywords.push(args[i].clone());
                i += 1;
            }
            "--" => break,
            _ => return usage_err(),
        }
    }
    Ok(i)
}

fn parse_store_only(cfg: &mut Config, args: &[String], mut i: usize) -> Result<usize, String> {
    while i < args.len() {
        match args[i].as_str() {
            "--store-dir" => {
                i += 1;
                if i >= args.len() { return usage_err(); }
                cfg.store_dir = Some(PathBuf::from(&args[i]));
                i += 1;
            }
            _ => break,
        }
    }
    Ok(i)
}

fn usage_msg() -> String {
    "usage: pira_ctx [--store-dir PATH] [--keyword QUERY ...] -- <cmd> | pira_ctx exact -- <cmd> | pira_ctx summary ... | pira_ctx search/range/raw/list/stats ...".to_string()
}
fn usage_err<T>() -> Result<T, String> { Err(usage_msg()) }

fn run_exact(cmd: &[String]) -> Result<i32, String> {
    if cmd.is_empty() { return usage_err(); }
    let status = Command::new(&cmd[0]).args(&cmd[1..]).status();
    match status {
        Ok(s) => Ok(status_code(s)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            eprintln!("pira_ctx: command not found: {}", cmd[0]);
            Ok(127)
        }
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
            eprintln!("pira_ctx: command not executable/permission denied: {}", cmd[0]);
            Ok(126)
        }
        Err(e) => Err(format!("failed to spawn {}: {e}", cmd[0])),
    }
}

fn run_auto(cfg: &Config) -> Result<i32, String> {
    let cap = match capture_or_spawn_exit(&cfg.cmd, &cfg.keywords)? { Ok(c) => c, Err(code) => return Ok(code) };
    let total = cap.stdout.len() + cap.stderr.len();
    if total == 0 || total < AUTO_SUMMARY_THRESHOLD {
        io::stdout().write_all(&cap.stdout).map_err(|e| e.to_string())?;
        io::stderr().write_all(&cap.stderr).map_err(|e| e.to_string())?;
        return Ok(cap.exit_code);
    }
    let store_dir = effective_store_dir(&cfg.store_dir)?;
    let (path, result_id, filename, metadata) = store_capture(&store_dir, &cfg.cmd, &cfg.keywords, &cap)?;
    print_summary(&metadata, &path, &result_id, &filename, &cap, &cfg.keywords)?;
    Ok(cap.exit_code)
}

fn run_summary(cfg: &Config) -> Result<i32, String> {
    let cap = match capture_or_spawn_exit(&cfg.cmd, &cfg.keywords)? { Ok(c) => c, Err(code) => return Ok(code) };
    let store_dir = effective_store_dir(&cfg.store_dir)?;
    let (path, result_id, filename, metadata) = store_capture(&store_dir, &cfg.cmd, &cfg.keywords, &cap)?;
    print_summary(&metadata, &path, &result_id, &filename, &cap, &cfg.keywords)?;
    Ok(cap.exit_code)
}

fn capture_or_spawn_exit(cmd: &[String], keywords: &[String]) -> Result<Result<CaptureResult, i32>, String> {
    match capture_command(cmd, keywords) {
        Ok(c) => Ok(Ok(c)),
        Err(e) if e.starts_with("__EXIT127__ ") => {
            eprintln!("pira_ctx: {}", e.trim_start_matches("__EXIT127__ "));
            Ok(Err(127))
        }
        Err(e) if e.starts_with("__EXIT126__ ") => {
            eprintln!("pira_ctx: {}", e.trim_start_matches("__EXIT126__ "));
            Ok(Err(126))
        }
        Err(e) => Err(e),
    }
}

fn capture_command(cmd: &[String], keywords: &[String]) -> Result<CaptureResult, String> {
    if cmd.is_empty() { return usage_err(); }
    let cwd = env::current_dir().map_err(|e| e.to_string())?
        .canonicalize().unwrap_or_else(|_| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .display().to_string();
    let start = SystemTime::now();
    let start_ms = millis(start);
    let mut child = Command::new(&cmd[0])
        .args(&cmd[1..])
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound { format!("__EXIT127__ command not found: {}", cmd[0]) }
            else if e.kind() == io::ErrorKind::PermissionDenied { format!("__EXIT126__ permission denied/not executable: {}", cmd[0]) }
            else { format!("failed to spawn {}: {e}", cmd[0]) }
        })?;

    let stdout = child.stdout.take().ok_or_else(|| "failed to capture stdout".to_string())?;
    let stderr = child.stderr.take().ok_or_else(|| "failed to capture stderr".to_string())?;
    let (tx, rx) = mpsc::channel::<StreamLine>();
    let tx_out = tx.clone();
    let out_handle = thread::spawn(move || read_stream(stdout, "stdout", tx_out));
    let tx_err = tx.clone();
    let err_handle = thread::spawn(move || read_stream(stderr, "stderr", tx_err));
    drop(tx);

    let status = child.wait().map_err(|e| e.to_string())?;
    let end = SystemTime::now();
    let end_ms = millis(end);
    let mut events: Vec<StreamLine> = rx.into_iter().collect();
    let stdout = out_handle.join().map_err(|_| "stdout reader panicked".to_string())?.map_err(|e| e.to_string())?;
    let stderr = err_handle.join().map_err(|_| "stderr reader panicked".to_string())?.map_err(|e| e.to_string())?;
    if events.is_empty() && (!stdout.is_empty() || !stderr.is_empty()) {
        // Reader events should normally contain final unterminated segments. Rebuild as a safety net.
        events.extend(scan_lines(&stdout).into_iter().map(|(offset, len)| StreamLine { stream: "stdout", offset, len }));
        events.extend(scan_lines(&stderr).into_iter().map(|(offset, len)| StreamLine { stream: "stderr", offset, len }));
    }
    let mut timeline: Vec<LineMeta> = events.into_iter().enumerate().map(|(idx, ev)| LineMeta {
        no: idx + 1,
        stream: ev.stream.to_string(),
        offset: ev.offset,
        len: ev.len,
        score: 0,
        reasons: Vec::new(),
    }).collect();
    let exit_code = status_code(status);
    score_timeline(&mut timeline, &stdout, &stderr, exit_code, keywords);
    Ok(CaptureResult {
        stdout,
        stderr,
        timeline,
        exit_code,
        start_ms,
        end_ms,
        duration_ms: end_ms.saturating_sub(start_ms),
        cwd,
    })
}

fn read_stream<R: Read>(mut reader: R, stream: &'static str, tx: mpsc::Sender<StreamLine>) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut buf = [0u8; 8192];
    let mut line_start = 0usize;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 { break; }
        let old_len = bytes.len();
        bytes.extend_from_slice(&buf[..n]);
        let mut pos = old_len;
        while pos < bytes.len() {
            if bytes[pos] == b'\n' {
                let len = pos + 1 - line_start;
                let _ = tx.send(StreamLine { stream, offset: line_start, len });
                line_start = pos + 1;
            }
            pos += 1;
        }
    }
    if line_start < bytes.len() {
        let _ = tx.send(StreamLine { stream, offset: line_start, len: bytes.len() - line_start });
    }
    Ok(bytes)
}

fn scan_lines(bytes: &[u8]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'\n' {
            out.push((start, i + 1 - start));
            start = i + 1;
        }
    }
    if start < bytes.len() { out.push((start, bytes.len() - start)); }
    out
}

fn status_code(status: std::process::ExitStatus) -> i32 {
    if let Some(c) = status.code() { return c; }
    #[cfg(unix)]
    if let Some(sig) = status.signal() { return 128 + sig; }
    125
}

fn millis(t: SystemTime) -> u128 {
    t.duration_since(UNIX_EPOCH).unwrap_or(Duration::from_secs(0)).as_millis()
}

fn effective_store_dir(opt: &Option<PathBuf>) -> Result<PathBuf, String> {
    Ok(match opt {
        Some(p) => p.clone(),
        None => env::temp_dir().join("pira_ctx"),
    })
}

fn ensure_private_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| format!("create {}: {e}", path.display()))?;
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|e| format!("chmod {}: {e}", path.display()))?;
    Ok(())
}

fn store_capture(store_dir: &Path, cmd: &[String], keywords: &[String], cap: &CaptureResult) -> Result<(PathBuf, String, String, String), String> {
    ensure_private_dir(store_dir)?;
    let manifests_dir = store_dir.join("manifests");
    ensure_private_dir(&manifests_dir)?;
    let timestamp = format_utc_timestamp(cap.start_ms / 1000);
    let workspace_id = workspace_id()?;
    let workspace_hash = hex_sha256(workspace_id.as_bytes())[..16].to_string();
    let seed = format!("{}\0{}\0{}\0{}", cap.cwd, cmd.join("\0"), cap.start_ms, std::process::id());
    let short_id = hex_sha256(seed.as_bytes())[..12].to_string();
    let result_id = format!("{}-{}", timestamp, short_id);
    let mut filename = format!("{}.piractx", result_id);
    let mut path = store_dir.join(&filename);
    let mut retry = 0u32;
    while path.exists() {
        retry += 1;
        filename = format!("{}-{}.piractx", result_id, retry);
        path = store_dir.join(&filename);
    }
    let metadata = build_metadata(store_dir, &path, &filename, &result_id, &workspace_id, &workspace_hash, cmd, keywords, cap);
    write_container(&path, &metadata, &cap.stdout, &cap.stderr)?;
    update_manifest(store_dir, &workspace_hash)?;
    Ok((path, result_id, filename, metadata))
}

fn write_container(path: &Path, metadata: &str, stdout: &[u8], stderr: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("piractx.tmp");
    let mut f = OpenOptions::new().write(true).create_new(true).open(&tmp)
        .map_err(|e| format!("create {}: {e}", tmp.display()))?;
    #[cfg(unix)]
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600)).map_err(|e| format!("chmod {}: {e}", tmp.display()))?;
    f.write_all(MAGIC).map_err(|e| e.to_string())?;
    write_u64(&mut f, metadata.as_bytes().len() as u64)?;
    f.write_all(metadata.as_bytes()).map_err(|e| e.to_string())?;
    write_u64(&mut f, stdout.len() as u64)?;
    f.write_all(stdout).map_err(|e| e.to_string())?;
    write_u64(&mut f, stderr.len() as u64)?;
    f.write_all(stderr).map_err(|e| e.to_string())?;
    f.sync_all().map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| format!("rename {} -> {}: {e}", tmp.display(), path.display()))?;
    Ok(())
}

fn write_u64<W: Write>(w: &mut W, v: u64) -> Result<(), String> {
    w.write_all(&v.to_le_bytes()).map_err(|e| e.to_string())
}

fn build_metadata(
    store_dir: &Path,
    path: &Path,
    filename: &str,
    result_id: &str,
    workspace_id: &str,
    workspace_hash: &str,
    cmd: &[String],
    keywords: &[String],
    cap: &CaptureResult,
) -> String {
    let stdout_lines = cap.timeline.iter().filter(|l| l.stream == "stdout").count();
    let stderr_lines = cap.timeline.iter().filter(|l| l.stream == "stderr").count();
    let detected_paths = detected_paths(&cap.timeline, &cap.stdout, &cap.stderr);
    let suggested_keywords = suggested_keywords(&cap.timeline, &cap.stdout, &cap.stderr, keywords);
    let score_breakdowns: Vec<String> = cap.timeline.iter().map(|l| {
        format!("{{\"line\":{},\"score\":{},\"reasons\":[{}]}}", l.no, l.score, json_string_array(&l.reasons))
    }).collect();
    let timeline_json: Vec<String> = cap.timeline.iter().map(|l| {
        format!(
            "{{\"line\":{},\"stream\":\"{}\",\"offset\":{},\"length\":{},\"score\":{},\"reasons\":[{}]}}",
            l.no, l.stream, l.offset, l.len, l.score, json_string_array(&l.reasons)
        )
    }).collect();
    format!(
        concat!(
            "{{",
            "\"compat_version\":{},",
            "\"tool_version\":{},",
            "\"command_argv\":[{}],",
            "\"cwd\":{},",
            "\"created_at\":{},",
            "\"start_unix_ms\":{},",
            "\"end_unix_ms\":{},",
            "\"duration_ms\":{},",
            "\"exit_code\":{},",
            "\"stdout_bytes\":{},",
            "\"stderr_bytes\":{},",
            "\"total_bytes\":{},",
            "\"stdout_lines\":{},",
            "\"stderr_lines\":{},",
            "\"total_lines\":{},",
            "\"detected_paths\":[{}],",
            "\"binary_stdout\":{},",
            "\"binary_stderr\":{},",
            "\"non_utf8_stdout\":{},",
            "\"non_utf8_stderr\":{},",
            "\"line_timeline\":[{}],",
            "\"score_breakdowns\":[{}],",
            "\"suggested_keywords\":[{}],",
            "\"store_dir\":{},",
            "\"store_path\":{},",
            "\"filename\":{},",
            "\"result_id\":{},",
            "\"workspace_id\":{},",
            "\"workspace_hash\":{}",
            "}}"
        ),
        COMPAT_VERSION,
        json_string(TOOL_VERSION),
        json_string_array(cmd),
        json_string(&cap.cwd),
        json_string(&format_utc_timestamp(cap.start_ms / 1000)),
        cap.start_ms,
        cap.end_ms,
        cap.duration_ms,
        cap.exit_code,
        cap.stdout.len(),
        cap.stderr.len(),
        cap.stdout.len() + cap.stderr.len(),
        stdout_lines,
        stderr_lines,
        cap.timeline.len(),
        json_string_array(&detected_paths),
        is_binary_like(&cap.stdout),
        is_binary_like(&cap.stderr),
        std::str::from_utf8(&cap.stdout).is_err(),
        std::str::from_utf8(&cap.stderr).is_err(),
        timeline_json.join(","),
        score_breakdowns.join(","),
        json_string_array(&suggested_keywords),
        json_string(&store_dir.display().to_string()),
        json_string(&path.display().to_string()),
        json_string(filename),
        json_string(result_id),
        json_string(workspace_id),
        json_string(workspace_hash),
    )
}

fn update_manifest(store_dir: &Path, workspace_hash: &str) -> Result<(), String> {
    let entries = scan_store(store_dir, Some(workspace_hash))?;
    let manifest_path = store_dir.join("manifests").join(format!("{}.json", workspace_hash));
    let tmp = manifest_path.with_extension("json.tmp");
    let mut body = String::new();
    body.push_str("{\"manifest_version\":1,\"entries\":[");
    let mut first = true;
    for e in entries {
        if !first { body.push(','); }
        first = false;
        body.push_str(&format!(
            "{{\"id\":{},\"filename\":{},\"path\":{},\"timestamp\":{},\"exit\":{},\"bytes\":{},\"lines\":{},\"command\":{}}}",
            json_string(&e.id), json_string(&e.filename), json_string(&e.path.display().to_string()), json_string(&e.timestamp), e.exit, e.bytes, e.lines, json_string(&e.command)
        ));
    }
    body.push_str("]}");
    fs::write(&tmp, body).map_err(|e| format!("write manifest: {e}"))?;
    #[cfg(unix)]
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600)).map_err(|e| format!("chmod manifest: {e}"))?;
    fs::rename(&tmp, &manifest_path).map_err(|e| format!("rename manifest: {e}"))?;
    Ok(())
}

#[derive(Debug, Clone)]
struct ListedEntry {
    id: String,
    filename: String,
    timestamp: String,
    start_ms: u64,
    exit: i64,
    bytes: u64,
    lines: u64,
    command: String,
    path: PathBuf,
}

fn scan_store(store_dir: &Path, workspace_filter: Option<&str>) -> Result<Vec<ListedEntry>, String> {
    let mut entries = Vec::new();
    if !store_dir.exists() { return Ok(entries); }
    let rd = fs::read_dir(store_dir).map_err(|e| format!("read {}: {e}", store_dir.display()))?;
    for item in rd {
        let item = item.map_err(|e| e.to_string())?;
        let path = item.path();
        if path.extension().and_then(|s| s.to_str()) != Some("piractx") { continue; }
        if let Ok(stored) = read_result_path(&path) {
            let m = &stored.metadata;
            let wh = json_get_string(m, "workspace_hash").unwrap_or_default();
            if let Some(wf) = workspace_filter {
                if wh != wf { continue; }
            }
            let id = json_get_string(m, "result_id").unwrap_or_else(|| path.file_stem().unwrap_or_default().to_string_lossy().to_string());
            let filename = json_get_string(m, "filename").unwrap_or_else(|| path.file_name().unwrap_or_default().to_string_lossy().to_string());
            let timestamp = json_get_string(m, "created_at").unwrap_or_default();
            let start_ms = json_get_u64(m, "start_unix_ms").unwrap_or(0);
            let exit = json_get_i64(m, "exit_code").unwrap_or(125);
            let bytes = json_get_u64(m, "total_bytes").unwrap_or(0);
            let lines = json_get_u64(m, "total_lines").unwrap_or(0);
            let command = json_get_string_array(m, "command_argv").map(|v| argv_display(&v)).unwrap_or_default();
            entries.push(ListedEntry { id, filename, timestamp, start_ms, exit, bytes, lines, command, path });
        }
    }
    entries.sort_by(|a, b| b.start_ms.cmp(&a.start_ms).then_with(|| b.id.cmp(&a.id)));
    Ok(entries)
}

fn resolve_result(store_dir: &Path, target: &str) -> Result<PathBuf, String> {
    if target == "--last" {
        let wh = current_workspace_hash()?;
        let entries = scan_store(store_dir, Some(&wh))?;
        return entries.first().map(|e| e.path.clone()).ok_or_else(|| "no stored pira_ctx result for current workspace".to_string());
    }
    let p = PathBuf::from(target);
    if target.contains(std::path::MAIN_SEPARATOR) || p.is_absolute() || target.ends_with(".piractx") && p.exists() {
        return Ok(p);
    }
    if target.ends_with(".piractx") {
        let candidate = store_dir.join(target);
        if candidate.exists() { return Ok(candidate); }
    }
    let wh = current_workspace_hash()?;
    let entries = scan_store(store_dir, Some(&wh))?;
    let matches: Vec<&ListedEntry> = entries.iter().filter(|e| e.id == target || e.id.starts_with(target) || e.filename == target || e.filename.starts_with(target)).collect();
    if matches.is_empty() { return Err(format!("no result matches {target}")); }
    if matches.len() > 1 { return Err(format!("ambiguous result id/name {target}")); }
    Ok(matches[0].path.clone())
}

fn read_result_path(path: &Path) -> Result<StoredResult, String> {
    let mut f = File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let mut magic = [0u8; 8];
    f.read_exact(&mut magic).map_err(|e| format!("corrupt result magic: {e}"))?;
    if &magic != MAGIC { return Err("corrupt result: bad magic".to_string()); }
    let meta_len = read_u64(&mut f)? as usize;
    let mut meta_bytes = vec![0u8; meta_len];
    f.read_exact(&mut meta_bytes).map_err(|e| format!("corrupt result metadata: {e}"))?;
    let metadata = String::from_utf8(meta_bytes).map_err(|_| "metadata is not UTF-8".to_string())?;
    let stdout_len = read_u64(&mut f)? as usize;
    let mut stdout = vec![0u8; stdout_len];
    f.read_exact(&mut stdout).map_err(|e| format!("corrupt stdout bytes: {e}"))?;
    let stderr_len = read_u64(&mut f)? as usize;
    let mut stderr = vec![0u8; stderr_len];
    f.read_exact(&mut stderr).map_err(|e| format!("corrupt stderr bytes: {e}"))?;
    let timeline = parse_timeline(&metadata)?;
    validate_timeline(&timeline, &stdout, &stderr)?;
    Ok(StoredResult { metadata, stdout, stderr, path: path.to_path_buf(), timeline })
}

fn read_u64<R: Read>(r: &mut R) -> Result<u64, String> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b).map_err(|e| format!("corrupt length: {e}"))?;
    Ok(u64::from_le_bytes(b))
}

fn validate_timeline(lines: &[LineMeta], stdout: &[u8], stderr: &[u8]) -> Result<(), String> {
    for l in lines {
        let stream = if l.stream == "stdout" { stdout } else if l.stream == "stderr" { stderr } else { return Err("invalid timeline stream".to_string()); };
        if l.offset.checked_add(l.len).map_or(true, |end| end > stream.len()) {
            return Err(format!("invalid timeline offset at L{}", l.no));
        }
    }
    Ok(())
}

fn run_search(cfg: &Config) -> Result<i32, String> {
    let store_dir = effective_store_dir(&cfg.store_dir)?;
    let target = cfg.target.as_ref().ok_or_else(usage_msg)?;
    let query = cfg.query.as_ref().ok_or_else(usage_msg)?;
    let path = resolve_result(&store_dir, target)?;
    let stored = read_result_path(&path)?;
    let regex = if cfg.regex { Some(Regex::new(query).map_err(|e| format!("invalid regex: {e}"))?) } else { None };
    let mut hits: Vec<(usize, i64)> = Vec::new();
    for (idx, line) in stored.timeline.iter().enumerate() {
        let clean = clean_display(line_bytes(line, &stored.stdout, &stored.stderr));
        let matched = if let Some(re) = &regex { re.is_match(&clean) } else { ascii_contains_ci(&clean, query) };
        if matched { hits.push((idx, line.score + if cfg.regex { 70 } else { 80 })); }
    }
    println!("{} hits", hits.len());
    hits.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| stored.timeline[a.0].no.cmp(&stored.timeline[b.0].no)));
    let mut selected: Vec<(usize, i64)> = Vec::new();
    if cfg.context == 0 {
        selected.extend(hits.into_iter().take(5));
    } else {
        let mut seen = HashSet::new();
        for (idx, score) in hits.into_iter().take(5) {
            let start = idx.saturating_sub(cfg.context);
            let end = (idx + cfg.context).min(stored.timeline.len().saturating_sub(1));
            for j in start..=end {
                if seen.insert(j) {
                    selected.push((j, if j == idx { score } else { stored.timeline[j].score }));
                }
            }
        }
        selected.sort_by_key(|(idx, _)| *idx);
    }
    for (idx, score) in selected {
        print_line_display(&stored.timeline[idx], &stored.stdout, &stored.stderr, score);
    }
    Ok(0)
}

fn run_range(cfg: &Config) -> Result<i32, String> {
    let store_dir = effective_store_dir(&cfg.store_dir)?;
    let target = cfg.target.as_ref().ok_or_else(usage_msg)?;
    let path = resolve_result(&store_dir, target)?;
    let stored = read_result_path(&path)?;
    let n = stored.timeline.len() as i64;
    let start_raw = cfg.start_line.ok_or_else(usage_msg)?;
    let end_raw = cfg.end_line.ok_or_else(usage_msg)?;
    if start_raw == 0 || end_raw == 0 { return Err("line number 0 is invalid".to_string()); }
    let mut start = if start_raw < 0 { n + start_raw + 1 } else { start_raw };
    let mut end = if end_raw < 0 { n + end_raw + 1 } else { end_raw };
    if start > end { return Err("start_line must be <= end_line after normalization".to_string()); }
    if n == 0 || (start < 1 && end < 1) || (start > n && end > n) { return Ok(0); }
    start = start.max(1).min(n);
    end = end.max(1).min(n);
    let mut out = io::stdout().lock();
    for i in start..=end {
        let line = &stored.timeline[(i - 1) as usize];
        out.write_all(line_bytes(line, &stored.stdout, &stored.stderr)).map_err(|e| e.to_string())?;
    }
    Ok(0)
}

fn run_raw(cfg: &Config) -> Result<i32, String> {
    let store_dir = effective_store_dir(&cfg.store_dir)?;
    let target = cfg.target.as_ref().ok_or_else(usage_msg)?;
    let path = resolve_result(&store_dir, target)?;
    let stored = read_result_path(&path)?;
    let mut out = io::stdout().lock();
    for line in &stored.timeline {
        out.write_all(line_bytes(line, &stored.stdout, &stored.stderr)).map_err(|e| e.to_string())?;
    }
    Ok(0)
}

fn run_stats(cfg: &Config) -> Result<i32, String> {
    let store_dir = effective_store_dir(&cfg.store_dir)?;
    let target = cfg.target.as_ref().ok_or_else(usage_msg)?;
    let path = resolve_result(&store_dir, target)?;
    let stored = read_result_path(&path)?;
    let m = &stored.metadata;
    let argv = json_get_string_array(m, "command_argv").unwrap_or_default();
    println!("Result: {}", json_get_string(m, "result_id").unwrap_or_default());
    println!("Command: {}", argv_display(&argv));
    println!("Cwd: {}", json_get_string(m, "cwd").unwrap_or_default());
    println!("Exit: {}", json_get_i64(m, "exit_code").unwrap_or(125));
    println!("Duration: {} ms", json_get_u64(m, "duration_ms").unwrap_or(0));
    println!("Size: stdout={} stderr={} total={} bytes", json_get_u64(m, "stdout_bytes").unwrap_or(0), json_get_u64(m, "stderr_bytes").unwrap_or(0), json_get_u64(m, "total_bytes").unwrap_or(0));
    println!("Lines: stdout={} stderr={} total={}", json_get_u64(m, "stdout_lines").unwrap_or(0), json_get_u64(m, "stderr_lines").unwrap_or(0), json_get_u64(m, "total_lines").unwrap_or(0));
    println!("Store: {}", json_get_string(m, "store_path").unwrap_or_else(|| stored.path.display().to_string()));
    println!("Created: {}", json_get_string(m, "created_at").unwrap_or_default());
    println!("Tool: {}", json_get_string(m, "tool_version").unwrap_or_default());
    println!("Binary: stdout={} stderr={} non_utf8_stdout={} non_utf8_stderr={}", json_get_bool(m, "binary_stdout").unwrap_or(false), json_get_bool(m, "binary_stderr").unwrap_or(false), json_get_bool(m, "non_utf8_stdout").unwrap_or(false), json_get_bool(m, "non_utf8_stderr").unwrap_or(false));
    println!("DetectedPaths: {}", json_get_string_array(m, "detected_paths").unwrap_or_default().join(", "));
    println!("Keywords: {}", json_get_string_array(m, "suggested_keywords").unwrap_or_default().join(", "));
    Ok(0)
}

fn run_list(cfg: &Config) -> Result<i32, String> {
    let store_dir = effective_store_dir(&cfg.store_dir)?;
    let filter = if cfg.workspace_current { Some(current_workspace_hash()?) } else { None };
    let entries = scan_store(&store_dir, filter.as_deref())?;
    println!("id | timestamp | exit | bytes | lines | command");
    for e in entries {
        println!("{} | {} | {} | {} | {} | {}", e.id, e.timestamp, e.exit, e.bytes, e.lines, e.command);
    }
    Ok(0)
}

fn print_summary(metadata: &str, path: &Path, result_id: &str, filename: &str, cap: &CaptureResult, _keywords: &[String]) -> Result<(), String> {
    let argv = json_get_string_array(metadata, "command_argv").unwrap_or_default();
    let stdout_lines = cap.timeline.iter().filter(|l| l.stream == "stdout").count();
    let stderr_lines = cap.timeline.iter().filter(|l| l.stream == "stderr").count();
    let total_bytes = cap.stdout.len() + cap.stderr.len();
    let shown = select_important(&cap.timeline, MAX_IMPORTANT_LINES);
    let shown_bytes: usize = shown.iter().map(|&i| cap.timeline[i].len).sum();
    let omitted_lines = cap.timeline.len().saturating_sub(shown.len());
    let omitted_bytes = total_bytes.saturating_sub(shown_bytes);
    println!("Result: {} ({})", result_id, filename);
    println!("Command: {}", argv_display(&argv));
    println!("Exit: {}", cap.exit_code);
    println!("Duration: {} ms", cap.duration_ms);
    println!("Size: stdout={} stderr={} total={} bytes; stdout_lines={} stderr_lines={} total_lines={}", cap.stdout.len(), cap.stderr.len(), total_bytes, stdout_lines, stderr_lines, cap.timeline.len());
    println!("Hidden: omitted_bytes={} omitted_lines={} binary_stdout={} binary_stderr={} non_utf8_stdout={} non_utf8_stderr={}", omitted_bytes, omitted_lines, is_binary_like(&cap.stdout), is_binary_like(&cap.stderr), std::str::from_utf8(&cap.stdout).is_err(), std::str::from_utf8(&cap.stderr).is_err());
    println!("Store: {}", path.display());
    println!("Important lines:");
    if cap.timeline.is_empty() {
        println!("  (none)");
    } else {
        for idx in shown {
            print_line_display(&cap.timeline[idx], &cap.stdout, &cap.stderr, cap.timeline[idx].score);
        }
    }
    println!("Anomalies:");
    let anomalies = anomalies(&cap.timeline, &cap.stdout, &cap.stderr);
    if anomalies.is_empty() { println!("  (none detected)"); }
    else { for a in anomalies { println!("  {a}"); } }
    println!("Suggested search keywords: {}", json_get_string_array(metadata, "suggested_keywords").unwrap_or_default().join(", "));
    println!("Retrieval: pira_ctx search --last <query> | pira_ctx range --last 1 20 | pira_ctx raw --last");
    Ok(())
}

fn select_important(lines: &[LineMeta], n: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..lines.len()).collect();
    order.sort_by(|&a, &b| lines[b].score.cmp(&lines[a].score).then_with(|| lines[a].no.cmp(&lines[b].no)));
    let mut selected = Vec::new();
    let mut templates = HashSet::new();
    for idx in order {
        if selected.len() >= n { break; }
        let key = format!("{}:{}", lines[idx].stream, lines[idx].score / 20);
        if templates.contains(&key) && lines[idx].score < 80 { continue; }
        templates.insert(key);
        selected.push(idx);
    }
    selected.sort_by_key(|&i| lines[i].no);
    selected
}

fn print_line_display(line: &LineMeta, stdout: &[u8], stderr: &[u8], score: i64) {
    let clean = clean_display(line_bytes(line, stdout, stderr));
    println!("L{} {} score={}: {}", line.no, line.stream, score, clip_display(&clean));
}

fn line_bytes<'a>(line: &LineMeta, stdout: &'a [u8], stderr: &'a [u8]) -> &'a [u8] {
    let s = if line.stream == "stdout" { stdout } else { stderr };
    &s[line.offset..line.offset + line.len]
}

fn score_timeline(lines: &mut [LineMeta], stdout: &[u8], stderr: &[u8], exit_code: i32, keywords: &[String]) {
    let total = lines.len();
    let mut base_scores: Vec<i64> = Vec::with_capacity(total);
    for line in lines.iter_mut() {
        let clean = clean_display(line_bytes(line, stdout, stderr));
        let (score, reasons) = score_line(&clean, &line.stream, line.no, total, exit_code, keywords);
        line.score = score;
        line.reasons = reasons;
        base_scores.push(score);
    }
    let line_tokens: Vec<HashSet<String>> = lines.iter().map(|line| {
        purified_tokens(&clean_display(line_bytes(line, stdout, stderr))).into_iter().collect()
    }).collect();
    let mut df: HashMap<String, usize> = HashMap::new();
    for toks in &line_tokens {
        for tok in toks { *df.entry(tok.clone()).or_insert(0) += 1; }
    }
    let rare_cutoff = (total / 10).max(1);
    for (line, toks) in lines.iter_mut().zip(line_tokens.iter()) {
        let rare_bonus = toks.iter()
            .filter(|tok| df.get(*tok).copied().unwrap_or(total + 1) <= rare_cutoff)
            .count()
            .min(5) as i64 * 3;
        if rare_bonus > 0 {
            line.score += rare_bonus;
            line.reasons.push(format!("idf-density+{rare_bonus}"));
        }
    }
    for i in 0..lines.len() {
        if base_scores[i] >= 100 {
            if i > 0 && lines[i - 1].score < base_scores[i] { lines[i - 1].score += 10; lines[i - 1].reasons.push("adjacent diagnostic context".to_string()); }
            if i + 1 < lines.len() && lines[i + 1].score < base_scores[i] { lines[i + 1].score += 10; lines[i + 1].reasons.push("adjacent diagnostic context".to_string()); }
        }
    }
}

fn score_line(clean: &str, stream: &str, no: usize, total: usize, exit_code: i32, keywords: &[String]) -> (i64, Vec<String>) {
    let lower = clean.to_ascii_lowercase();
    let mut score = 0i64;
    let mut reasons = Vec::new();
    let severe = ["fatal", "error", "failure", "failed", "panic", "exception", "traceback", "permission denied", "no such file", "command not found", "timeout", "timed out"];
    if severe.iter().any(|s| lower.contains(s)) { score += 100; reasons.push("severity/error".to_string()); }
    if lower.contains("test") && (lower.contains("fail") || lower.contains("failed")) { score += 70; reasons.push("failed test".to_string()); }
    if lower.contains("warning") || lower.contains("warn:") { score += 40; reasons.push("warning".to_string()); }
    if (lower.contains("note") || lower.contains("remark") || lower.contains("info")) && score < 80 { score += 5; reasons.push("note/info".to_string()); }
    for kw in keywords {
        if !kw.is_empty() && lower.contains(&kw.to_ascii_lowercase()) { score += 80; reasons.push(format!("keyword:{kw}")); }
    }
    if has_path_colon_line(clean) || has_path_like(clean) { score += 30; reasons.push("file/path".to_string()); }
    if is_metric_line(&lower) { score += 25; reasons.push("metric/table-like".to_string()); }
    if lower.contains("todo") || lower.contains("pira") { score += 20; reasons.push("TODO/PIRA marker".to_string()); }
    if lower.contains(" at ") || lower.trim_start().starts_with("at ") || lower.contains("stack backtrace") { score += 15; reasons.push("stack/frame".to_string()); }
    if stream == "stderr" && !is_progress_noise(&lower) { score += 10; reasons.push("stderr".to_string()); }
    let pos_boost = position_boost(no, total);
    if pos_boost > 0 { score += pos_boost; reasons.push(format!("position+{pos_boost}")); }
    if exit_code != 0 && no + 5 > total && (stream == "stderr" || lower.contains("exit") || lower.contains("fail") || lower.contains("error")) {
        score += 20; reasons.push("nonzero-exit tail".to_string());
    }
    if lower.contains("nan") || lower.contains("inf") || lower.contains("overflow") || lower.contains("underflow") {
        score += 35; reasons.push("numeric anomaly".to_string());
    }
    let token_bonus = purified_tokens(clean).into_iter().collect::<HashSet<_>>().len().min(10) as i64;
    score += token_bonus;
    if token_bonus > 0 { reasons.push("informativeness".to_string()); }
    (score, reasons)
}

fn position_boost(no: usize, total: usize) -> i64 {
    let boosts = [15, 12, 9, 6, 3];
    if no >= 1 && no <= 5 { return boosts[no - 1]; }
    if total >= no && total - no < 5 { return boosts[total - no]; }
    0
}

fn is_progress_noise(lower: &str) -> bool {
    lower.contains("%") && (lower.contains("download") || lower.contains("progress"))
}

fn is_metric_line(lower: &str) -> bool {
    let keys = ["accuracy", "loss", "metric", "score", "auc", "f1", "precision", "recall", "passed", "failed", "result"];
    keys.iter().any(|k| lower.contains(k)) && lower.chars().any(|c| c.is_ascii_digit()) || lower.contains('|') || lower.contains('=')
}

fn has_path_like(s: &str) -> bool {
    s.contains('/') && (s.contains(".rs") || s.contains(".py") || s.contains(".md") || s.contains(".txt") || s.contains(".sh") || s.contains(".c") || s.contains(".h"))
}

fn has_path_colon_line(s: &str) -> bool {
    let bytes = s.as_bytes();
    for i in 0..bytes.len().saturating_sub(3) {
        if bytes[i] == b':' && i > 0 && bytes[i + 1].is_ascii_digit() {
            return true;
        }
    }
    false
}

fn anomalies(lines: &[LineMeta], stdout: &[u8], stderr: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    for l in lines {
        let clean = clean_display(line_bytes(l, stdout, stderr));
        let lower = clean.to_ascii_lowercase();
        if lower.contains("nan") || lower.contains("inf") || lower.contains("overflow") || lower.contains("underflow") {
            out.push(format!("L{} {}: suspicious numeric token", l.no, l.stream));
        }
        if out.len() >= 5 { break; }
    }
    out
}

fn detected_paths(lines: &[LineMeta], stdout: &[u8], stderr: &[u8]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for l in lines {
        let clean = clean_display(line_bytes(l, stdout, stderr));
        for tok in clean.split_whitespace() {
            let t = tok.trim_matches(|c: char| c == ',' || c == ';' || c == ')' || c == '(' || c == '[' || c == ']' || c == '"' || c == '\'');
            if (has_path_like(t) || has_path_colon_line(t)) && seen.insert(t.to_string()) {
                out.push(t.to_string());
                if out.len() >= 20 { return out; }
            }
        }
    }
    out
}

fn suggested_keywords(lines: &[LineMeta], stdout: &[u8], stderr: &[u8], user_keywords: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for kw in user_keywords {
        if seen.insert(kw.to_ascii_lowercase()) { out.push(kw.clone()); }
    }
    let mut sorted: Vec<&LineMeta> = lines.iter().collect();
    sorted.sort_by(|a, b| b.score.cmp(&a.score));
    for l in sorted.into_iter().take(20) {
        let clean = clean_display(line_bytes(l, stdout, stderr));
        for tok in clean.split_whitespace() {
            let t = tok.trim_matches(|c: char| !is_keyword_char(c));
            if t.len() < 3 || t.len() > 120 { continue; }
            let lower = t.to_ascii_lowercase();
            if stopwords().contains(lower.as_str()) { continue; }
            if seen.insert(lower) {
                out.push(t.to_string());
                if out.len() >= 20 { return out; }
            }
        }
    }
    out
}

fn is_keyword_char(c: char) -> bool { c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '/' || c == '.' || c == ':' }

fn purified_tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in s.split(|c: char| !is_keyword_char(c)) {
        if raw.len() < 2 { continue; }
        let t = raw.to_ascii_lowercase();
        if stopwords().contains(t.as_str()) { continue; }
        out.push(t);
    }
    out
}

fn stopwords() -> HashSet<&'static str> {
    ["the", "and", "for", "with", "this", "that", "from", "into", "have", "has", "are", "was", "were", "you", "your", "but", "not", "all", "out", "line", "lines", "noise", "more", "done", "begin", "end", "build", "word"].into_iter().collect()
}

fn clean_display(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(bytes);
    strip_ansi(&s).trim_end_matches(['\n', '\r']).to_string()
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for ch in chars.by_ref() {
                    if ('@'..='~').contains(&ch) { break; }
                }
            }
        } else if c.is_control() && c != '\n' && c != '\r' && c != '\t' {
            // Drop display control characters.
        } else {
            out.push(c);
        }
    }
    out
}

fn clip_display(s: &str) -> String {
    if s.len() <= DISPLAY_CLIP_BYTES { return s.to_string(); }
    let clipped = s.len() - 900;
    let approx_words = clipped / 6;
    let start = safe_prefix(s, 600);
    let end = safe_suffix(s, 300);
    format!("{} … clipped {} bytes (~{} words) … {}", start, clipped, approx_words, end)
}

fn safe_prefix(s: &str, n: usize) -> &str {
    let mut end = n.min(s.len());
    while !s.is_char_boundary(end) { end -= 1; }
    &s[..end]
}
fn safe_suffix(s: &str, n: usize) -> &str {
    let mut start = s.len().saturating_sub(n);
    while !s.is_char_boundary(start) { start += 1; }
    &s[start..]
}

fn ascii_contains_ci(hay: &str, needle: &str) -> bool {
    hay.to_ascii_lowercase().contains(&needle.to_ascii_lowercase())
}


fn is_binary_like(bytes: &[u8]) -> bool {
    if bytes.contains(&0) { return true; }
    if bytes.is_empty() { return false; }
    let control = bytes.iter().filter(|&&b| b < 0x20 && b != b'\n' && b != b'\r' && b != b'\t' && b != 0x1b).count();
    control * 100 / bytes.len() > 30
}

fn workspace_id() -> Result<String, String> {
    let cwd = env::current_dir().map_err(|e| e.to_string())?;
    let root = nearest_git_root(&cwd).unwrap_or(cwd);
    Ok(root.canonicalize().unwrap_or(root).display().to_string())
}
fn current_workspace_hash() -> Result<String, String> {
    Ok(hex_sha256(workspace_id()?.as_bytes())[..16].to_string())
}
fn nearest_git_root(start: &Path) -> Option<PathBuf> {
    let mut cur = start.to_path_buf();
    loop {
        if cur.join(".git").exists() { return Some(cur); }
        if !cur.pop() { return None; }
    }
}

fn format_utc_timestamp(secs: u128) -> String {
    let days = (secs / 86400) as i64;
    let sod = (secs % 86400) as u32;
    let (year, month, day) = civil_from_days(days);
    let hour = sod / 3600;
    let minute = (sod % 3600) / 60;
    let second = sod % 60;
    format!("{:04}{:02}{:02}-{:02}{:02}{:02}", year, month, day, hour, minute, second)
}

fn civil_from_days(days_since_epoch: i64) -> (i32, u32, u32) {
    // Howard Hinnant's civil_from_days, with z relative to 1970-01-01.
    let z = days_since_epoch + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (year as i32, m as u32, d as u32)
}

fn argv_display(argv: &[String]) -> String {
    argv.iter().map(|a| shellish_quote(a)).collect::<Vec<_>>().join(" ")
}
fn shellish_quote(s: &str) -> String {
    if s.chars().all(|c| c.is_ascii_alphanumeric() || "_+-=./:".contains(c)) { s.to_string() }
    else { format!("'{}'", s.replace('\'', "'\\''")) }
}

fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_string_array(items: &[String]) -> String {
    items.iter().map(|s| json_string(s)).collect::<Vec<_>>().join(",")
}

fn json_get_string(meta: &str, key: &str) -> Option<String> {
    let idx = meta.find(&format!("\"{}\":", key))?;
    let rest = &meta[idx + key.len() + 3..];
    parse_json_string(rest.trim_start()).map(|(s, _)| s)
}

fn json_get_i64(meta: &str, key: &str) -> Option<i64> {
    let idx = meta.find(&format!("\"{}\":", key))?;
    let rest = meta[idx + key.len() + 3..].trim_start();
    let end = rest.find(|c: char| !(c.is_ascii_digit() || c == '-')).unwrap_or(rest.len());
    rest[..end].parse().ok()
}
fn json_get_u64(meta: &str, key: &str) -> Option<u64> { json_get_i64(meta, key).and_then(|v| if v >= 0 { Some(v as u64) } else { None }) }
fn json_get_bool(meta: &str, key: &str) -> Option<bool> {
    let idx = meta.find(&format!("\"{}\":", key))?;
    let rest = meta[idx + key.len() + 3..].trim_start();
    if rest.starts_with("true") { Some(true) } else if rest.starts_with("false") { Some(false) } else { None }
}

fn json_get_string_array(meta: &str, key: &str) -> Option<Vec<String>> {
    let idx = meta.find(&format!("\"{}\":[", key))?;
    let mut rest = &meta[idx + key.len() + 4..];
    let mut out = Vec::new();
    loop {
        rest = rest.trim_start();
        if rest.starts_with(']') { break; }
        let (s, used) = parse_json_string(rest)?;
        out.push(s);
        rest = rest[used..].trim_start();
        if rest.starts_with(',') { rest = &rest[1..]; }
        else if rest.starts_with(']') { break; }
        else { return None; }
    }
    Some(out)
}

fn parse_json_string(s: &str) -> Option<(String, usize)> {
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'"') { return None; }
    let mut out = String::new();
    let mut i = 1usize;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' { return Some((out, i + 1)); }
        if b == b'\\' {
            i += 1;
            if i >= bytes.len() { return None; }
            match bytes[i] {
                b'"' => out.push('"'),
                b'\\' => out.push('\\'),
                b'n' => out.push('\n'),
                b'r' => out.push('\r'),
                b't' => out.push('\t'),
                b'u' => {
                    if i + 4 >= bytes.len() { return None; }
                    let hex = std::str::from_utf8(&bytes[i + 1..i + 5]).ok()?;
                    let code = u32::from_str_radix(hex, 16).ok()?;
                    out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                    i += 4;
                }
                other => out.push(other as char),
            }
        } else {
            let tail = std::str::from_utf8(&bytes[i..]).ok()?;
            let ch = tail.chars().next()?;
            out.push(ch);
            i += ch.len_utf8() - 1;
        }
        i += 1;
    }
    None
}

fn parse_timeline(meta: &str) -> Result<Vec<LineMeta>, String> {
    let key = "\"line_timeline\":[";
    let start = meta.find(key).ok_or_else(|| "metadata missing line_timeline".to_string())? + key.len();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    let mut end = None;
    for (rel, ch) in meta[start..].char_indices() {
        if in_str {
            if esc { esc = false; }
            else if ch == '\\' { esc = true; }
            else if ch == '"' { in_str = false; }
            continue;
        }
        match ch {
            '"' => in_str = true,
            '[' => depth += 1,
            ']' => {
                if depth == 0 { end = Some(start + rel); break; }
                depth -= 1;
            }
            _ => {}
        }
    }
    let body = &meta[start..end.ok_or_else(|| "unterminated line_timeline".to_string())?];
    let mut objs = Vec::new();
    let mut obj_start = None;
    depth = 0; in_str = false; esc = false;
    for (i, ch) in body.char_indices() {
        if in_str {
            if esc { esc = false; }
            else if ch == '\\' { esc = true; }
            else if ch == '"' { in_str = false; }
            continue;
        }
        match ch {
            '"' => in_str = true,
            '{' => { if depth == 0 { obj_start = Some(i); } depth += 1; }
            '}' => { depth -= 1; if depth == 0 { if let Some(s) = obj_start { objs.push(&body[s..=i]); } } }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for (idx, obj) in objs.iter().enumerate() {
        out.push(LineMeta {
            no: json_get_u64(obj, "line").unwrap_or((idx + 1) as u64) as usize,
            stream: json_get_string(obj, "stream").ok_or_else(|| "timeline line missing stream".to_string())?,
            offset: json_get_u64(obj, "offset").ok_or_else(|| "timeline line missing offset".to_string())? as usize,
            len: json_get_u64(obj, "length").ok_or_else(|| "timeline line missing length".to_string())? as usize,
            score: json_get_i64(obj, "score").unwrap_or(0),
            reasons: json_get_string_array(obj, "reasons").unwrap_or_default(),
        });
    }
    Ok(out)
}

// Minimal SHA-256 implementation for manifest workspace hashes and short ids.
fn hex_sha256(data: &[u8]) -> String {
    let digest = sha256(data);
    digest.iter().map(|b| format!("{:02x}", b)).collect()
}

fn sha256(data: &[u8]) -> [u8; 32] {
    const H0: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    const K: [u32; 64] = [
        0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
        0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
        0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
        0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
        0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
        0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
        0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
        0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2,
    ];
    let mut h = H0;
    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 { msg.push(0); }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[4*i], chunk[4*i+1], chunk[4*i+2], chunk[4*i+3]]);
        }
        for i in 16..64 {
            let s0 = w[i-15].rotate_right(7) ^ w[i-15].rotate_right(18) ^ (w[i-15] >> 3);
            let s1 = w[i-2].rotate_right(17) ^ w[i-2].rotate_right(19) ^ (w[i-2] >> 10);
            w[i] = w[i-16].wrapping_add(s0).wrapping_add(w[i-7]).wrapping_add(s1);
        }
        let mut a=h[0]; let mut b=h[1]; let mut c=h[2]; let mut d=h[3];
        let mut e=h[4]; let mut f=h[5]; let mut g=h[6]; let mut hh=h[7];
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g; g = f; f = e; e = d.wrapping_add(temp1); d = c; c = b; b = a; a = temp1.wrapping_add(temp2);
        }
        h[0]=h[0].wrapping_add(a); h[1]=h[1].wrapping_add(b); h[2]=h[2].wrapping_add(c); h[3]=h[3].wrapping_add(d);
        h[4]=h[4].wrapping_add(e); h[5]=h[5].wrapping_add(f); h[6]=h[6].wrapping_add(g); h[7]=h[7].wrapping_add(hh);
    }
    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() { out[4*i..4*i+4].copy_from_slice(&word.to_be_bytes()); }
    out
}
