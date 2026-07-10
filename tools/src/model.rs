use std::fmt;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::util;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StreamKind {
    Stdout,
    Stderr,
}

impl fmt::Display for StreamKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineMeta {
    #[serde(rename = "line")]
    pub line: usize,
    pub stream: StreamKind,
    pub offset: u64,
    #[serde(rename = "length")]
    pub length: u64,
    #[serde(default)]
    pub score: i64,
    #[serde(default)]
    pub reasons: Vec<String>,
}

#[derive(Debug)]
pub struct TempSpool {
    pub path: PathBuf,
    pub length: u64,
    pub sha256: [u8; 32],
    pub binary: bool,
    pub non_utf8: bool,
}

impl Drop for TempSpool {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[derive(Debug)]
pub struct CaptureResult {
    pub stdout: TempSpool,
    pub stderr: TempSpool,
    pub timeline: Vec<LineMeta>,
    pub total_lines: usize,
    pub stdout_lines: usize,
    pub stderr_lines: usize,
    pub timeline_truncated: bool,
    pub exit_code: i32,
    pub start_ms: u128,
    pub end_ms: u128,
    pub duration_ms: u128,
    pub cwd: String,
}

impl CaptureResult {
    pub fn total_bytes(&self) -> u64 {
        self.stdout.length.saturating_add(self.stderr.length)
    }

    pub fn readers(&self) -> Result<StreamReaders, String> {
        StreamReaders::from_paths(
            &self.stdout.path,
            0,
            self.stdout.length,
            &self.stderr.path,
            0,
            self.stderr.length,
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    #[serde(default = "default_compat_version")]
    pub compat_version: u32,
    #[serde(default)]
    pub tool_version: String,
    #[serde(default)]
    pub command_argv: Vec<String>,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub start_unix_ms: u128,
    #[serde(default)]
    pub end_unix_ms: u128,
    #[serde(default)]
    pub duration_ms: u128,
    #[serde(default)]
    pub exit_code: i32,
    #[serde(default)]
    pub stdout_bytes: u64,
    #[serde(default)]
    pub stderr_bytes: u64,
    #[serde(default)]
    pub total_bytes: u64,
    #[serde(default)]
    pub stdout_lines: usize,
    #[serde(default)]
    pub stderr_lines: usize,
    #[serde(default)]
    pub total_lines: usize,
    #[serde(default)]
    pub detected_paths: Vec<String>,
    #[serde(default)]
    pub binary_stdout: bool,
    #[serde(default)]
    pub binary_stderr: bool,
    #[serde(default)]
    pub non_utf8_stdout: bool,
    #[serde(default)]
    pub non_utf8_stderr: bool,
    #[serde(default)]
    pub line_timeline: Vec<LineMeta>,
    #[serde(default)]
    pub suggested_keywords: Vec<String>,
    #[serde(default)]
    pub store_dir: String,
    #[serde(default)]
    pub store_path: String,
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub result_id: String,
    #[serde(default)]
    pub workspace_id: String,
    #[serde(default)]
    pub workspace_hash: String,
    #[serde(default)]
    pub stdout_sha256: String,
    #[serde(default)]
    pub stderr_sha256: String,
    #[serde(default)]
    pub timeline_truncated: bool,
}

fn default_compat_version() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListedEntry {
    pub id: String,
    pub filename: String,
    pub timestamp: String,
    pub start_ms: u128,
    pub exit: i32,
    pub bytes: u64,
    pub lines: usize,
    pub command: String,
    pub path: PathBuf,
    pub workspace_hash: String,
}

impl ListedEntry {
    pub fn from_metadata(metadata: &Metadata, path: PathBuf) -> Self {
        Self {
            id: metadata.result_id.clone(),
            filename: metadata.filename.clone(),
            timestamp: metadata.created_at.clone(),
            start_ms: metadata.start_unix_ms,
            exit: metadata.exit_code,
            bytes: metadata.total_bytes,
            lines: metadata.total_lines,
            command: util::argv_display(&metadata.command_argv),
            path,
            workspace_hash: metadata.workspace_hash.clone(),
        }
    }
}

#[derive(Debug)]
pub struct StreamReaders {
    stdout: BufReader<File>,
    stderr: BufReader<File>,
    stdout_base: u64,
    stderr_base: u64,
    stdout_length: u64,
    stderr_length: u64,
}

impl StreamReaders {
    pub fn from_paths(
        stdout_path: &std::path::Path,
        stdout_base: u64,
        stdout_length: u64,
        stderr_path: &std::path::Path,
        stderr_base: u64,
        stderr_length: u64,
    ) -> Result<Self, String> {
        Ok(Self {
            stdout: BufReader::new(File::open(stdout_path).map_err(|error| error.to_string())?),
            stderr: BufReader::new(File::open(stderr_path).map_err(|error| error.to_string())?),
            stdout_base,
            stderr_base,
            stdout_length,
            stderr_length,
        })
    }

    pub fn read_display_line(&mut self, line: &LineMeta) -> Result<String, String> {
        let bytes = self.read_bounded(line, util::MAX_DISPLAY_READ_BYTES)?;
        Ok(util::sanitize_terminal(&String::from_utf8_lossy(&bytes)))
    }

    pub fn read_search_line(&mut self, line: &LineMeta) -> Result<String, String> {
        let bytes = self.read_bounded(line, util::MAX_SEARCH_LINE_BYTES)?;
        Ok(util::sanitize_terminal(&String::from_utf8_lossy(&bytes)))
    }

    fn read_bounded(&mut self, line: &LineMeta, maximum: u64) -> Result<Vec<u8>, String> {
        let (reader, base, section_length) = self.parts_mut(line.stream);
        validate_line(line, section_length)?;
        reader
            .seek(SeekFrom::Start(base + line.offset))
            .map_err(|error| error.to_string())?;
        if line.length <= maximum {
            let size = usize::try_from(line.length).map_err(|_| "line is too large".to_string())?;
            let mut bytes = vec![0; size];
            reader
                .read_exact(&mut bytes)
                .map_err(|error| error.to_string())?;
            return Ok(bytes);
        }
        let prefix_length = maximum / 2;
        let suffix_length = maximum - prefix_length;
        let mut bytes =
            vec![0; usize::try_from(prefix_length).map_err(|_| "line is too large".to_string())?];
        reader
            .read_exact(&mut bytes)
            .map_err(|error| error.to_string())?;
        bytes.extend_from_slice(b" ... [line read truncated] ... ");
        reader
            .seek(SeekFrom::Start(
                base + line.offset + line.length - suffix_length,
            ))
            .map_err(|error| error.to_string())?;
        let old_length = bytes.len();
        bytes.resize(
            old_length
                + usize::try_from(suffix_length).map_err(|_| "line is too large".to_string())?,
            0,
        );
        reader
            .read_exact(&mut bytes[old_length..])
            .map_err(|error| error.to_string())?;
        Ok(bytes)
    }

    pub fn copy_line<W: Write>(&mut self, line: &LineMeta, output: &mut W) -> Result<(), String> {
        let (reader, base, section_length) = self.parts_mut(line.stream);
        validate_line(line, section_length)?;
        reader
            .seek(SeekFrom::Start(base + line.offset))
            .map_err(|error| error.to_string())?;
        let mut limited = reader.take(line.length);
        std::io::copy(&mut limited, output).map_err(util::io_error)?;
        Ok(())
    }

    pub fn copy_section<W: Write>(
        &mut self,
        stream: StreamKind,
        output: &mut W,
    ) -> Result<(), String> {
        let (reader, base, section_length) = self.parts_mut(stream);
        reader
            .seek(SeekFrom::Start(base))
            .map_err(|error| error.to_string())?;
        let mut limited = reader.take(section_length);
        std::io::copy(&mut limited, output).map_err(util::io_error)?;
        Ok(())
    }

    fn parts_mut(&mut self, stream: StreamKind) -> (&mut BufReader<File>, u64, u64) {
        match stream {
            StreamKind::Stdout => (&mut self.stdout, self.stdout_base, self.stdout_length),
            StreamKind::Stderr => (&mut self.stderr, self.stderr_base, self.stderr_length),
        }
    }
}

fn validate_line(line: &LineMeta, section_length: u64) -> Result<(), String> {
    if line
        .offset
        .checked_add(line.length)
        .is_none_or(|end| end > section_length)
    {
        return Err(format!("invalid timeline offset at L{}", line.line));
    }
    Ok(())
}
