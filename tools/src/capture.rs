use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::SystemTime;

use sha2::{Digest, Sha256};

use crate::model::{CaptureResult, LineMeta, StreamKind, TempSpool};
use crate::{spawn_command, summarize, util};

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
    sha256: [u8; 32],
    binary: bool,
    non_utf8: bool,
}

const MAX_INDEXED_LINES: usize = 50_000;
const INDEX_HEAD_LINES: usize = MAX_INDEXED_LINES / 2;
const INDEX_TAIL_LINES: usize = MAX_INDEXED_LINES - INDEX_HEAD_LINES;

struct CollectedLines {
    timeline: Vec<LineMeta>,
    total: usize,
    stdout: usize,
    stderr: usize,
    truncated: bool,
}

pub fn capture_command(
    cmd: &[String],
    keywords: &[String],
) -> Result<Result<CaptureResult, i32>, String> {
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
    let start_ms = util::millis(start);
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
    let (sender, receiver) = mpsc::channel::<StreamLine>();
    let stdout_sender = sender.clone();
    let stdout_handle = thread::spawn(move || {
        read_stream(child_stdout, stdout_file, StreamKind::Stdout, stdout_sender)
    });
    let stderr_handle =
        thread::spawn(move || read_stream(child_stderr, stderr_file, StreamKind::Stderr, sender));
    let collector_handle = thread::spawn(move || collect_lines(receiver));

    let status = child.wait().map_err(|error| error.to_string())?;
    let end_ms = util::millis(SystemTime::now());
    let stdout_analysis = join_reader(stdout_handle, "stdout")?;
    let stderr_analysis = join_reader(stderr_handle, "stderr")?;
    let collected = collector_handle
        .join()
        .map_err(|_| "line collector panicked".to_string())?;
    let stdout = TempSpool {
        path: stdout_spool.disarm(),
        length: stdout_analysis.length,
        sha256: stdout_analysis.sha256,
        binary: stdout_analysis.binary,
        non_utf8: stdout_analysis.non_utf8,
    };
    let stderr = TempSpool {
        path: stderr_spool.disarm(),
        length: stderr_analysis.length,
        sha256: stderr_analysis.sha256,
        binary: stderr_analysis.binary,
        non_utf8: stderr_analysis.non_utf8,
    };
    let exit_code = util::status_code(status);
    let mut capture = CaptureResult {
        stdout,
        stderr,
        timeline: collected.timeline,
        total_lines: collected.total,
        stdout_lines: collected.stdout,
        stderr_lines: collected.stderr,
        timeline_truncated: collected.truncated,
        exit_code,
        start_ms,
        end_ms,
        duration_ms: end_ms.saturating_sub(start_ms),
        cwd,
    };
    summarize::score_timeline(&mut capture, keywords)?;
    Ok(Ok(capture))
}

fn collect_lines(receiver: mpsc::Receiver<StreamLine>) -> CollectedLines {
    let mut head = Vec::with_capacity(INDEX_HEAD_LINES);
    let mut tail = VecDeque::with_capacity(INDEX_TAIL_LINES);
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
            reasons: Vec::new(),
        };
        if head.len() < INDEX_HEAD_LINES {
            head.push(line);
        } else {
            if tail.len() == INDEX_TAIL_LINES {
                tail.pop_front();
            }
            tail.push_back(line);
        }
    }
    let truncated = total > MAX_INDEXED_LINES;
    head.extend(tail);
    CollectedLines {
        timeline: head,
        total,
        stdout,
        stderr,
        truncated,
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
    sender: mpsc::Sender<StreamLine>,
) -> io::Result<StreamAnalysis> {
    let mut buffer = [0_u8; 64 * 1024];
    let mut hasher = Sha256::new();
    let mut offset = 0_u64;
    let mut line_start = 0_u64;
    let mut null_seen = false;
    let mut controls = 0_u64;
    let mut validator = Utf8Validator::default();
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let chunk = &buffer[..count];
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
        offset += count as u64;
    }
    if line_start < offset {
        let _ = sender.send(StreamLine {
            stream,
            offset: line_start,
            length: offset - line_start,
        });
    }
    output.sync_all()?;
    let digest: [u8; 32] = hasher.finalize().into();
    Ok(StreamAnalysis {
        length: offset,
        sha256: digest,
        binary: null_seen || (offset > 0 && controls.saturating_mul(100) / offset > 30),
        non_utf8: validator.finish(),
    })
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
