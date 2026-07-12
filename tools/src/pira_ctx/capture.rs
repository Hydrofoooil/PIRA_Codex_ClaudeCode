use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Instant, SystemTime};

use sha2::{Digest, Sha256};

use crate::model::{CaptureResult, LineMeta, StreamKind, TempSpool};
use crate::{spawn_command, util};

const DEFAULT_MAX_RETAINED_BYTES: u64 = 512 * 1024 * 1024;
const DEFAULT_MAX_INDEXED_LINES: usize = 1_000_000;
const HARD_MAX_INDEXED_LINES: usize = 2_000_000;
const LINE_CHANNEL_CAPACITY: usize = 4096;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

#[derive(Debug)]
struct StreamLine {
    stream: StreamKind,
    offset: u64,
    length: u64,
}

#[derive(Debug)]
struct StreamAnalysis {
    length: u64,
    observed_length: u64,
    sha256: [u8; 32],
    binary: bool,
    non_utf8: bool,
}

struct CollectedLines {
    timeline: Vec<LineMeta>,
    total: usize,
    stdout: usize,
    stderr: usize,
    truncated: bool,
}

struct RetentionBudget {
    maximum: u64,
    used: AtomicU64,
}

impl RetentionBudget {
    fn reserve(&self, requested: usize) -> usize {
        let requested = requested as u64;
        let mut used = self.used.load(Ordering::Relaxed);
        loop {
            if used >= self.maximum {
                return 0;
            }
            let granted = requested.min(self.maximum - used);
            match self.used.compare_exchange_weak(
                used,
                used + granted,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return granted as usize,
                Err(actual) => used = actual,
            }
        }
    }
}

pub fn capture_command(cmd: &[String]) -> Result<Result<CaptureResult, i32>, String> {
    if cmd.is_empty() {
        return Err(crate::cli::USAGE.to_string());
    }
    let cwd_path = std::env::current_dir().map_err(|error| error.to_string())?;
    let cwd = cwd_path
        .canonicalize()
        .unwrap_or(cwd_path)
        .display()
        .to_string();
    let start = SystemTime::now();
    let elapsed = Instant::now();
    let start_ms = util::millis(start);
    let retained_limit = configured_u64(
        "PIRA_CTX_MAX_RETAINED_BYTES",
        DEFAULT_MAX_RETAINED_BYTES,
        4 * 1024,
    );
    let indexed_line_limit = configured_usize(
        "PIRA_CTX_MAX_INDEXED_LINES",
        DEFAULT_MAX_INDEXED_LINES,
        1_000,
        HARD_MAX_INDEXED_LINES,
    );
    let budget = Arc::new(RetentionBudget {
        maximum: retained_limit,
        used: AtomicU64::new(0),
    });
    let (mut stdout_spool, stdout_file) = create_spool("stdout", start_ms)?;
    let (mut stderr_spool, stderr_file) = create_spool("stderr", start_ms)?;
    let mut child = match spawn_command(cmd) {
        Ok(child) => child,
        Err(error) if error.starts_with("__EXIT127__ ") => {
            eprintln!("pira_ctx: {}", error.trim_start_matches("__EXIT127__ "));
            return Ok(Err(127));
        }
        Err(error) if error.starts_with("__EXIT126__ ") => {
            eprintln!("pira_ctx: {}", error.trim_start_matches("__EXIT126__ "));
            return Ok(Err(126));
        }
        Err(error) => return Err(error),
    };
    let child_stdout = child
        .stdout
        .take()
        .ok_or_else(|| "failed to capture stdout".to_string())?;
    let child_stderr = child
        .stderr
        .take()
        .ok_or_else(|| "failed to capture stderr".to_string())?;
    let (sender, receiver) = mpsc::sync_channel::<StreamLine>(LINE_CHANNEL_CAPACITY);
    let stdout_sender = sender.clone();
    let stdout_budget = Arc::clone(&budget);
    let stdout_handle = thread::spawn(move || {
        read_stream(
            child_stdout,
            stdout_file,
            StreamKind::Stdout,
            stdout_sender,
            &stdout_budget,
        )
    });
    let stderr_budget = Arc::clone(&budget);
    let stderr_handle = thread::spawn(move || {
        read_stream(
            child_stderr,
            stderr_file,
            StreamKind::Stderr,
            sender,
            &stderr_budget,
        )
    });
    let collector_handle = thread::spawn(move || collect_lines(receiver, indexed_line_limit));

    let status = child.wait().map_err(|error| error.to_string())?;
    let end_ms = util::millis(SystemTime::now());
    let stdout_analysis = join_reader(stdout_handle, "stdout")?;
    let stderr_analysis = join_reader(stderr_handle, "stderr")?;
    let collected = collector_handle
        .join()
        .map_err(|_| "line collector panicked".to_string())?;
    let retention_truncated = stdout_analysis.observed_length > stdout_analysis.length
        || stderr_analysis.observed_length > stderr_analysis.length;
    let stdout = TempSpool {
        path: stdout_spool.disarm(),
        length: stdout_analysis.length,
        observed_length: stdout_analysis.observed_length,
        sha256: stdout_analysis.sha256,
        binary: stdout_analysis.binary,
        non_utf8: stdout_analysis.non_utf8,
    };
    let stderr = TempSpool {
        path: stderr_spool.disarm(),
        length: stderr_analysis.length,
        observed_length: stderr_analysis.observed_length,
        sha256: stderr_analysis.sha256,
        binary: stderr_analysis.binary,
        non_utf8: stderr_analysis.non_utf8,
    };
    let exit_code = util::status_code(status);
    let capture = CaptureResult {
        stdout,
        stderr,
        timeline: collected.timeline,
        total_lines: collected.total,
        stdout_lines: collected.stdout,
        stderr_lines: collected.stderr,
        timeline_truncated: collected.truncated || retention_truncated,
        retention_truncated,
        exit_code,
        start_ms,
        end_ms,
        duration_ms: elapsed.elapsed().as_millis(),
        cwd,
    };
    Ok(Ok(capture))
}

fn collect_lines(receiver: mpsc::Receiver<StreamLine>, maximum: usize) -> CollectedLines {
    let mut timeline = Vec::new();
    let mut total = 0_usize;
    let mut stdout = 0_usize;
    let mut stderr = 0_usize;
    for event in receiver {
        total += 1;
        match event.stream {
            StreamKind::Stdout => stdout += 1,
            StreamKind::Stderr => stderr += 1,
        }
        let line = LineMeta {
            line: total,
            stream: event.stream,
            offset: event.offset,
            length: event.length,
            score: 0,
            flags: 0,
        };
        if timeline.len() < maximum {
            timeline.push(line);
        }
    }
    CollectedLines {
        timeline,
        total,
        stdout,
        stderr,
        truncated: total > maximum,
    }
}

fn join_reader(
    handle: thread::JoinHandle<io::Result<StreamAnalysis>>,
    name: &str,
) -> Result<StreamAnalysis, String> {
    handle
        .join()
        .map_err(|_| format!("{name} reader panicked"))?
        .map_err(|error| format!("{name} reader failed: {error}"))
}

fn create_spool(stream: &str, start_ms: u128) -> Result<(SpoolGuard, File), String> {
    let directory = std::env::temp_dir();
    for nonce in 0..100_u32 {
        let filename = format!(
            ".pira_ctx-spool-{}-{start_ms}-{stream}-{nonce}",
            std::process::id()
        );
        let path = directory.join(filename);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        match options.open(&path) {
            Ok(file) => return Ok((SpoolGuard { path: Some(path) }, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("create temporary capture: {error}")),
        }
    }
    Err("could not create unique temporary capture file".to_string())
}

struct SpoolGuard {
    path: Option<PathBuf>,
}

impl SpoolGuard {
    fn disarm(&mut self) -> PathBuf {
        self.path.take().expect("spool guard already disarmed")
    }
}

impl Drop for SpoolGuard {
    fn drop(&mut self) {
        if let Some(path) = &self.path {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn read_stream<R: Read>(
    mut input: R,
    mut output: File,
    stream: StreamKind,
    sender: mpsc::SyncSender<StreamLine>,
    budget: &RetentionBudget,
) -> io::Result<StreamAnalysis> {
    let mut buffer = [0_u8; 64 * 1024];
    let mut hasher = Sha256::new();
    let mut offset = 0_u64;
    let mut observed = 0_u64;
    let mut line_start = 0_u64;
    let mut null_seen = false;
    let mut controls = 0_u64;
    let mut validator = Utf8Validator::default();
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        observed = observed.saturating_add(count as u64);
        let retained = budget.reserve(count);
        if retained == 0 {
            continue;
        }
        let chunk = &buffer[..retained];
        output.write_all(chunk)?;
        hasher.update(chunk);
        validator.feed(chunk);
        null_seen |= chunk.contains(&0);
        controls += chunk
            .iter()
            .filter(|&&byte| byte < 0x20 && !matches!(byte, b'\n' | b'\r' | b'\t' | 0x1b))
            .count() as u64;
        for (index, byte) in chunk.iter().enumerate() {
            if *byte == b'\n' {
                let end = offset + index as u64 + 1;
                let _ = sender.send(StreamLine {
                    stream,
                    offset: line_start,
                    length: end - line_start,
                });
                line_start = end;
            }
        }
        offset += retained as u64;
    }
    if line_start < offset {
        let _ = sender.send(StreamLine {
            stream,
            offset: line_start,
            length: offset - line_start,
        });
    }
    // This is an ephemeral spool. Closing the writer makes its bytes visible to
    // the later reader; durable synchronization belongs to the final capture.
    let digest: [u8; 32] = hasher.finalize().into();
    Ok(StreamAnalysis {
        length: offset,
        observed_length: observed,
        sha256: digest,
        binary: null_seen || (offset > 0 && controls.saturating_mul(100) / offset > 30),
        non_utf8: validator.finish(),
    })
}

fn configured_u64(name: &str, default: u64, minimum: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map_or(default, |value| value.max(minimum))
}

fn configured_usize(name: &str, default: usize, minimum: usize, maximum: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .map_or(default, |value| value.clamp(minimum, maximum))
}

#[derive(Default)]
struct Utf8Validator {
    tail: Vec<u8>,
    invalid: bool,
}

impl Utf8Validator {
    fn feed(&mut self, chunk: &[u8]) {
        if self.invalid {
            return;
        }
        let mut bytes = std::mem::take(&mut self.tail);
        bytes.extend_from_slice(chunk);
        match std::str::from_utf8(&bytes) {
            Ok(_) => {}
            Err(error) if error.error_len().is_some() => self.invalid = true,
            Err(error) => self.tail.extend_from_slice(&bytes[error.valid_up_to()..]),
        }
    }

    fn finish(self) -> bool {
        self.invalid || !self.tail.is_empty()
    }
}
