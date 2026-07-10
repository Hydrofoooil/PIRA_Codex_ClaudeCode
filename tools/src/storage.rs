use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime};

use sha2::{Digest, Sha256};

use crate::model::{CaptureResult, ListedEntry, Metadata, StreamKind, StreamReaders};
use crate::{summarize, util};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

const MAGIC_V1: &[u8; 8] = b"PIRACTX1";
const MAGIC_V2: &[u8; 8] = b"PIRACTX2";
const FORMAT_VERSION: u32 = 2;
const HEADER_V2_BYTES: u64 = 8 + 4 + 4 + 8 + 8 + 8 + 32 + 32 + 32;
const MAX_METADATA_BYTES: u64 = 256 * 1024 * 1024;
const INDEX_COMPLETE: &str = ".complete-v2";

#[derive(Debug)]
pub struct StoredResult {
    pub metadata: Metadata,
    pub path: PathBuf,
    pub format_version: u32,
    stdout_offset: u64,
    stderr_offset: u64,
    stdout_hash: Option<[u8; 32]>,
    stderr_hash: Option<[u8; 32]>,
}

impl StoredResult {
    pub fn reader(&self) -> Result<StreamReaders, String> {
        StreamReaders::from_paths(
            &self.path,
            self.stdout_offset,
            self.metadata.stdout_bytes,
            &self.path,
            self.stderr_offset,
            self.metadata.stderr_bytes,
        )
    }

    pub fn verify(&self) -> Result<(), String> {
        if let Some(expected) = self.stdout_hash {
            verify_section(
                &self.path,
                self.stdout_offset,
                self.metadata.stdout_bytes,
                &expected,
                "stdout",
            )?;
        }
        if let Some(expected) = self.stderr_hash {
            verify_section(
                &self.path,
                self.stderr_offset,
                self.metadata.stderr_bytes,
                &expected,
                "stderr",
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct PruneResult {
    pub removed_files: usize,
    pub removed_bytes: u64,
    pub remaining_files: usize,
    pub remaining_bytes: u64,
}

pub fn effective_store_dir(option: Option<&PathBuf>) -> Result<PathBuf, String> {
    if let Some(path) = option {
        return Ok(path.clone());
    }
    if let Some(path) = std::env::var_os("PIRA_CTX_STORE_DIR") {
        return Ok(PathBuf::from(path));
    }
    #[cfg(target_os = "windows")]
    if let Some(path) = std::env::var_os("LOCALAPPDATA") {
        return Ok(PathBuf::from(path).join("PIRA").join("ctx"));
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return Ok(PathBuf::from(home)
            .join("Library")
            .join("Caches")
            .join("PIRA")
            .join("ctx"));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(path) = std::env::var_os("XDG_CACHE_HOME") {
            return Ok(PathBuf::from(path).join("pira").join("ctx"));
        }
        if let Some(home) = std::env::var_os("HOME") {
            return Ok(PathBuf::from(home).join(".cache").join("pira").join("ctx"));
        }
    }
    Err(
        "cannot determine a per-user pira_ctx store; set PIRA_CTX_STORE_DIR or --store-dir"
            .to_string(),
    )
}

pub fn store_capture(
    store_dir: &Path,
    command: &[String],
    keywords: &[String],
    capture: &CaptureResult,
) -> Result<StoredResult, String> {
    ensure_private_dir(store_dir)?;
    ensure_private_dir(&store_dir.join("indexes"))?;
    let workspace_id = workspace_id()?;
    let workspace_hash = short_hash(workspace_id.as_bytes(), 16);
    let timestamp = format_utc_timestamp(capture.start_ms / 1000);
    let mut seed = Vec::new();
    seed.extend_from_slice(capture.cwd.as_bytes());
    for argument in command {
        seed.push(0);
        seed.extend_from_slice(argument.as_bytes());
    }
    seed.extend_from_slice(&capture.start_ms.to_le_bytes());
    seed.extend_from_slice(&std::process::id().to_le_bytes());
    let base_id = format!("{}-{}", timestamp, short_hash(&seed, 12));
    let (result_id, filename, path) = available_result_path(store_dir, &base_id);
    let detected_paths = summarize::detected_paths(capture)?;
    let suggested_keywords = summarize::suggested_keywords(capture, keywords)?;
    let metadata = Metadata {
        compat_version: FORMAT_VERSION,
        tool_version: format!("pira_ctx-{}", env!("CARGO_PKG_VERSION")),
        command_argv: command.to_vec(),
        cwd: capture.cwd.clone(),
        created_at: timestamp,
        start_unix_ms: capture.start_ms,
        end_unix_ms: capture.end_ms,
        duration_ms: capture.duration_ms,
        exit_code: capture.exit_code,
        stdout_bytes: capture.stdout.length,
        stderr_bytes: capture.stderr.length,
        total_bytes: capture.total_bytes(),
        stdout_lines: capture.stdout_lines,
        stderr_lines: capture.stderr_lines,
        total_lines: capture.total_lines,
        detected_paths,
        binary_stdout: capture.stdout.binary,
        binary_stderr: capture.stderr.binary,
        non_utf8_stdout: capture.stdout.non_utf8,
        non_utf8_stderr: capture.stderr.non_utf8,
        line_timeline: capture.timeline.clone(),
        suggested_keywords,
        store_dir: store_dir.display().to_string(),
        store_path: path.display().to_string(),
        filename: filename.clone(),
        result_id: result_id.clone(),
        workspace_id,
        workspace_hash: workspace_hash.clone(),
        stdout_sha256: util::hex(&capture.stdout.sha256),
        stderr_sha256: util::hex(&capture.stderr.sha256),
        timeline_truncated: capture.timeline_truncated,
    };
    write_container(&path, &metadata, capture)?;
    let entry = ListedEntry::from_metadata(&metadata, path.clone());
    if let Err(error) = update_index(store_dir, &entry) {
        eprintln!("pira_ctx: warning: stored result but could not update index: {error}");
    }
    read_result_path(&path)
}

fn available_result_path(store_dir: &Path, base_id: &str) -> (String, String, PathBuf) {
    for suffix in 0_u32.. {
        let id = if suffix == 0 {
            base_id.to_string()
        } else {
            format!("{base_id}-{suffix}")
        };
        let filename = format!("{id}.piractx");
        let path = store_dir.join(&filename);
        if !path.exists() {
            return (id, filename, path);
        }
    }
    unreachable!()
}

fn write_container(
    path: &Path,
    metadata: &Metadata,
    capture: &CaptureResult,
) -> Result<(), String> {
    let metadata_bytes = serde_json::to_vec(metadata).map_err(|error| error.to_string())?;
    if metadata_bytes.len() as u64 > MAX_METADATA_BYTES {
        return Err("capture metadata is too large to store safely".to_string());
    }
    let metadata_hash: [u8; 32] = Sha256::digest(&metadata_bytes).into();
    let temporary = path.with_extension(format!("piractx.tmp-{}", std::process::id()));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut output = options
            .open(&temporary)
            .map_err(|error| format!("create {}: {error}", temporary.display()))?;
        output
            .write_all(MAGIC_V2)
            .map_err(|error| error.to_string())?;
        output
            .write_all(&FORMAT_VERSION.to_le_bytes())
            .map_err(|error| error.to_string())?;
        output
            .write_all(&0_u32.to_le_bytes())
            .map_err(|error| error.to_string())?;
        write_u64(&mut output, metadata_bytes.len() as u64)?;
        write_u64(&mut output, capture.stdout.length)?;
        write_u64(&mut output, capture.stderr.length)?;
        output
            .write_all(&metadata_hash)
            .map_err(|error| error.to_string())?;
        output
            .write_all(&capture.stdout.sha256)
            .map_err(|error| error.to_string())?;
        output
            .write_all(&capture.stderr.sha256)
            .map_err(|error| error.to_string())?;
        output
            .write_all(&metadata_bytes)
            .map_err(|error| error.to_string())?;
        copy_file(&capture.stdout.path, &mut output)?;
        copy_file(&capture.stderr.path, &mut output)?;
        output.sync_all().map_err(|error| error.to_string())?;
        match fs::hard_link(&temporary, path) {
            Ok(()) => {
                let _ = fs::remove_file(&temporary);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                return Err(format!("result path already exists: {}", path.display()));
            }
            Err(_) if !path.exists() => {
                fs::rename(&temporary, path).map_err(|error| error.to_string())?;
            }
            Err(error) => return Err(format!("publish {}: {error}", path.display())),
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn copy_file(path: &Path, output: &mut File) -> Result<(), String> {
    let mut input = File::open(path).map_err(|error| error.to_string())?;
    io::copy(&mut input, output).map_err(|error| error.to_string())?;
    Ok(())
}

pub fn read_result_path(path: &Path) -> Result<StoredResult, String> {
    let mut file = File::open(path).map_err(|error| format!("open {}: {error}", path.display()))?;
    let file_length = file.metadata().map_err(|error| error.to_string())?.len();
    let mut magic = [0_u8; 8];
    file.read_exact(&mut magic)
        .map_err(|error| format!("corrupt result magic: {error}"))?;
    match &magic {
        MAGIC_V1 => read_v1(path, file, file_length),
        MAGIC_V2 => read_v2(path, file, file_length),
        _ => Err("corrupt result: bad magic".to_string()),
    }
}

fn read_v2(path: &Path, mut file: File, file_length: u64) -> Result<StoredResult, String> {
    let version = read_u32(&mut file)?;
    if version != FORMAT_VERSION {
        return Err(format!("unsupported pira_ctx format version {version}"));
    }
    let _flags = read_u32(&mut file)?;
    let metadata_length = read_u64(&mut file)?;
    let stdout_length = read_u64(&mut file)?;
    let stderr_length = read_u64(&mut file)?;
    validate_layout(
        file_length,
        HEADER_V2_BYTES,
        metadata_length,
        stdout_length,
        stderr_length,
    )?;
    let metadata_hash = read_hash(&mut file)?;
    let stdout_hash = read_hash(&mut file)?;
    let stderr_hash = read_hash(&mut file)?;
    let metadata_bytes = read_bounded_metadata(&mut file, metadata_length)?;
    let actual_metadata_hash: [u8; 32] = Sha256::digest(&metadata_bytes).into();
    if actual_metadata_hash != metadata_hash {
        return Err("corrupt result: metadata checksum mismatch".to_string());
    }
    let metadata: Metadata = serde_json::from_slice(&metadata_bytes)
        .map_err(|error| format!("invalid result metadata: {error}"))?;
    if metadata.compat_version != FORMAT_VERSION {
        return Err(format!(
            "unsupported metadata compatibility version {}",
            metadata.compat_version
        ));
    }
    validate_metadata(&metadata, stdout_length, stderr_length)?;
    if metadata.stdout_sha256 != util::hex(&stdout_hash)
        || metadata.stderr_sha256 != util::hex(&stderr_hash)
    {
        return Err("corrupt result: metadata stream checksums disagree with header".to_string());
    }
    let stdout_offset = HEADER_V2_BYTES + metadata_length;
    Ok(StoredResult {
        metadata,
        path: path.to_path_buf(),
        format_version: version,
        stdout_offset,
        stderr_offset: stdout_offset + stdout_length,
        stdout_hash: Some(stdout_hash),
        stderr_hash: Some(stderr_hash),
    })
}

fn read_v1(path: &Path, mut file: File, file_length: u64) -> Result<StoredResult, String> {
    let metadata_length = read_u64(&mut file)?;
    if metadata_length > MAX_METADATA_BYTES {
        return Err("corrupt result: metadata is too large".to_string());
    }
    let metadata_bytes = read_bounded_metadata(&mut file, metadata_length)?;
    let metadata: Metadata = serde_json::from_slice(&metadata_bytes)
        .map_err(|error| format!("invalid result metadata: {error}"))?;
    if metadata.compat_version != 1 {
        return Err(format!(
            "unsupported legacy metadata version {}",
            metadata.compat_version
        ));
    }
    let stdout_length = read_u64(&mut file)?;
    let stdout_offset = 8 + 8 + metadata_length + 8;
    let stderr_length_offset = stdout_offset
        .checked_add(stdout_length)
        .ok_or_else(|| "corrupt result: length overflow".to_string())?;
    if stderr_length_offset + 8 > file_length {
        return Err("corrupt result: stdout length exceeds file".to_string());
    }
    file.seek(SeekFrom::Start(stderr_length_offset))
        .map_err(|error| error.to_string())?;
    let stderr_length = read_u64(&mut file)?;
    let stderr_offset = stderr_length_offset + 8;
    let expected = stderr_offset
        .checked_add(stderr_length)
        .ok_or_else(|| "corrupt result: length overflow".to_string())?;
    if expected != file_length {
        return Err("corrupt result: inconsistent payload lengths".to_string());
    }
    validate_metadata(&metadata, stdout_length, stderr_length)?;
    Ok(StoredResult {
        metadata,
        path: path.to_path_buf(),
        format_version: 1,
        stdout_offset,
        stderr_offset,
        stdout_hash: None,
        stderr_hash: None,
    })
}

fn validate_layout(
    file_length: u64,
    header: u64,
    metadata: u64,
    stdout: u64,
    stderr: u64,
) -> Result<(), String> {
    if metadata > MAX_METADATA_BYTES {
        return Err("corrupt result: metadata is too large".to_string());
    }
    let expected = header
        .checked_add(metadata)
        .and_then(|value| value.checked_add(stdout))
        .and_then(|value| value.checked_add(stderr))
        .ok_or_else(|| "corrupt result: length overflow".to_string())?;
    if expected != file_length {
        return Err("corrupt result: inconsistent payload lengths".to_string());
    }
    Ok(())
}

fn validate_metadata(metadata: &Metadata, stdout: u64, stderr: u64) -> Result<(), String> {
    if metadata.stdout_bytes != stdout || metadata.stderr_bytes != stderr {
        return Err("corrupt result: metadata stream lengths disagree with container".to_string());
    }
    if metadata.total_bytes != stdout.saturating_add(stderr) {
        return Err("corrupt result: invalid total byte count".to_string());
    }
    if (!metadata.timeline_truncated && metadata.total_lines != metadata.line_timeline.len())
        || (metadata.timeline_truncated && metadata.total_lines < metadata.line_timeline.len())
    {
        return Err("corrupt result: invalid timeline line count".to_string());
    }
    let mut previous_line = 0;
    for line in &metadata.line_timeline {
        if line.line <= previous_line {
            return Err("corrupt result: non-increasing timeline".to_string());
        }
        previous_line = line.line;
        let section = match line.stream {
            StreamKind::Stdout => stdout,
            StreamKind::Stderr => stderr,
        };
        if line
            .offset
            .checked_add(line.length)
            .is_none_or(|end| end > section)
        {
            return Err(format!(
                "corrupt result: invalid timeline offset at L{}",
                line.line
            ));
        }
    }
    Ok(())
}

fn read_bounded_metadata(file: &mut File, length: u64) -> Result<Vec<u8>, String> {
    let size =
        usize::try_from(length).map_err(|_| "metadata does not fit this platform".to_string())?;
    let mut bytes = vec![0_u8; size];
    file.read_exact(&mut bytes)
        .map_err(|error| format!("corrupt result metadata: {error}"))?;
    Ok(bytes)
}

fn verify_section(
    path: &Path,
    offset: u64,
    length: u64,
    expected: &[u8; 32],
    name: &str,
) -> Result<(), String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| error.to_string())?;
    let mut limited = file.take(length);
    let mut buffer = [0_u8; 64 * 1024];
    let mut hasher = Sha256::new();
    loop {
        let count = limited
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let actual: [u8; 32] = hasher.finalize().into();
    if &actual != expected {
        return Err(format!("corrupt result: {name} checksum mismatch"));
    }
    Ok(())
}

pub fn scan_store(
    store_dir: &Path,
    workspace_filter: Option<&str>,
) -> Result<Vec<ListedEntry>, String> {
    if !store_dir.exists() {
        return Ok(Vec::new());
    }
    let indexes = store_dir.join("indexes");
    if indexes.join(INDEX_COMPLETE).is_file() {
        return read_indexes(&indexes, workspace_filter);
    }
    scan_result_headers(store_dir, workspace_filter)
}

fn scan_result_headers(
    store_dir: &Path,
    workspace_filter: Option<&str>,
) -> Result<Vec<ListedEntry>, String> {
    let mut entries = Vec::new();
    for item in
        fs::read_dir(store_dir).map_err(|error| format!("read {}: {error}", store_dir.display()))?
    {
        let path = item.map_err(|error| error.to_string())?.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("piractx") {
            continue;
        }
        if let Ok(stored) = read_result_path(&path)
            && workspace_filter.is_none_or(|filter| stored.metadata.workspace_hash == filter)
        {
            entries.push(ListedEntry::from_metadata(&stored.metadata, path));
        }
    }
    sort_entries(&mut entries);
    Ok(entries)
}

fn read_indexes(
    indexes: &Path,
    workspace_filter: Option<&str>,
) -> Result<Vec<ListedEntry>, String> {
    let mut entries = Vec::new();
    let paths: Vec<PathBuf> = if let Some(workspace) = workspace_filter {
        vec![indexes.join(format!("{workspace}.jsonl"))]
    } else {
        fs::read_dir(indexes)
            .map_err(|error| error.to_string())?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension().and_then(|extension| extension.to_str()) == Some("jsonl")
            })
            .collect()
    };
    let mut seen = HashSet::new();
    for path in paths {
        if !path.is_file() {
            continue;
        }
        let reader = BufReader::new(File::open(&path).map_err(|error| error.to_string())?);
        for line in reader.lines() {
            let line = line.map_err(|error| error.to_string())?;
            let entry: ListedEntry = match serde_json::from_str(&line) {
                Ok(entry) => entry,
                Err(_) => continue,
            };
            let mut entry = entry;
            if !entry.path.is_file()
                && let Some(store_dir) = indexes.parent()
            {
                entry.path = store_dir.join(&entry.filename);
            }
            if entry.path.is_file() && seen.insert(entry.path.clone()) {
                entries.push(entry);
            }
        }
    }
    sort_entries(&mut entries);
    Ok(entries)
}

fn sort_entries(entries: &mut [ListedEntry]) {
    entries.sort_by(|a, b| b.start_ms.cmp(&a.start_ms).then_with(|| b.id.cmp(&a.id)));
}

fn update_index(store_dir: &Path, entry: &ListedEntry) -> Result<(), String> {
    let indexes = store_dir.join("indexes");
    ensure_private_dir(&indexes)?;
    let _lock = StoreLock::acquire(&indexes.join(".index.lock"))?;
    if !indexes.join(INDEX_COMPLETE).is_file() {
        rebuild_indexes_locked(store_dir, &indexes)?;
        return Ok(());
    }
    let path = indexes.join(format!("{}.jsonl", entry.workspace_hash));
    append_index(&path, entry)
}

fn rebuild_indexes_locked(store_dir: &Path, indexes: &Path) -> Result<(), String> {
    for item in fs::read_dir(indexes).map_err(|error| error.to_string())? {
        let path = item.map_err(|error| error.to_string())?.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("jsonl") {
            fs::remove_file(path).map_err(|error| error.to_string())?;
        }
    }
    let entries = scan_result_headers(store_dir, None)?;
    let mut grouped: HashMap<String, Vec<ListedEntry>> = HashMap::new();
    for entry in entries {
        grouped
            .entry(entry.workspace_hash.clone())
            .or_default()
            .push(entry);
    }
    for (workspace, entries) in grouped {
        let path = indexes.join(format!("{workspace}.jsonl"));
        for entry in entries {
            append_index(&path, &entry)?;
        }
    }
    write_private_file(&indexes.join(INDEX_COMPLETE), b"2\n")?;
    Ok(())
}

fn append_index(path: &Path, entry: &ListedEntry) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(path).map_err(|error| error.to_string())?;
    serde_json::to_writer(&mut file, entry).map_err(|error| error.to_string())?;
    file.write_all(b"\n").map_err(|error| error.to_string())?;
    file.sync_data().map_err(|error| error.to_string())
}

pub fn resolve_result(store_dir: &Path, target: &str) -> Result<PathBuf, String> {
    if target == "--last" {
        let workspace = current_workspace_hash()?;
        return scan_store(store_dir, Some(&workspace))?
            .first()
            .map(|entry| entry.path.clone())
            .ok_or_else(|| "no stored pira_ctx result for current workspace".to_string());
    }
    let path = PathBuf::from(target);
    if path.is_absolute()
        || path.components().count() > 1
        || (target.ends_with(".piractx") && path.exists())
    {
        return Ok(path);
    }
    if target.ends_with(".piractx") {
        let candidate = store_dir.join(target);
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    let workspace = current_workspace_hash()?;
    let entries = scan_store(store_dir, Some(&workspace))?;
    let matches: Vec<_> = entries
        .iter()
        .filter(|entry| {
            entry.id == target
                || entry.id.starts_with(target)
                || entry.filename == target
                || entry.filename.starts_with(target)
        })
        .collect();
    match matches.as_slice() {
        [] => Err(format!("no result matches {target}")),
        [entry] => Ok(entry.path.clone()),
        _ => Err(format!("ambiguous result id/name {target}")),
    }
}

pub fn prune_store(
    store_dir: &Path,
    max_age_days: Option<u64>,
    max_store_bytes: Option<u64>,
) -> Result<PruneResult, String> {
    ensure_private_dir(store_dir)?;
    let mut entries = scan_store(store_dir, None)?;
    entries.sort_by_key(|entry| entry.start_ms);
    let now = util::millis(SystemTime::now());
    let cutoff = max_age_days.map(|days| now.saturating_sub(days as u128 * 86_400_000));
    let mut remove = HashSet::new();
    for entry in &entries {
        if cutoff.is_some_and(|cutoff| entry.start_ms < cutoff) {
            remove.insert(entry.path.clone());
        }
    }
    let mut remaining_bytes: u64 = entries
        .iter()
        .filter(|entry| !remove.contains(&entry.path))
        .map(entry_disk_size)
        .sum();
    if let Some(maximum) = max_store_bytes {
        for entry in &entries {
            if remaining_bytes <= maximum {
                break;
            }
            if remove.insert(entry.path.clone()) {
                remaining_bytes = remaining_bytes.saturating_sub(entry_disk_size(entry));
            }
        }
    }
    let mut result = PruneResult::default();
    for entry in &entries {
        if remove.contains(&entry.path) {
            let disk_size = entry_disk_size(entry);
            fs::remove_file(&entry.path)
                .map_err(|error| format!("remove {}: {error}", entry.path.display()))?;
            result.removed_files += 1;
            result.removed_bytes = result.removed_bytes.saturating_add(disk_size);
        } else {
            result.remaining_files += 1;
            result.remaining_bytes = result
                .remaining_bytes
                .saturating_add(entry_disk_size(entry));
        }
    }
    let indexes = store_dir.join("indexes");
    if indexes.exists() {
        let _lock = StoreLock::acquire(&indexes.join(".index.lock"))?;
        let _ = fs::remove_file(indexes.join(INDEX_COMPLETE));
        rebuild_indexes_locked(store_dir, &indexes)?;
    }
    Ok(result)
}

fn entry_disk_size(entry: &ListedEntry) -> u64 {
    entry
        .path
        .metadata()
        .map_or(entry.bytes, |metadata| metadata.len())
}

pub fn current_workspace_hash() -> Result<String, String> {
    Ok(short_hash(workspace_id()?.as_bytes(), 16))
}

fn workspace_id() -> Result<String, String> {
    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    let root = nearest_git_root(&cwd).unwrap_or(cwd);
    Ok(root.canonicalize().unwrap_or(root).display().to_string())
}

fn nearest_git_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

fn ensure_private_dir(path: &Path) -> Result<(), String> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "refusing symlinked store directory {}",
                path.display()
            ));
        }
        if !metadata.is_dir() {
            return Err(format!("store path is not a directory: {}", path.display()));
        }
    } else {
        fs::create_dir_all(path).map_err(|error| format!("create {}: {error}", path.display()))?;
    }
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("chmod {}: {error}", path.display()))?;
    Ok(())
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options
        .open(&temporary)
        .map_err(|error| error.to_string())?;
    file.write_all(bytes).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    fs::rename(&temporary, path).map_err(|error| error.to_string())
}

struct StoreLock {
    path: PathBuf,
}

impl StoreLock {
    fn acquire(path: &Path) -> Result<Self, String> {
        for attempt in 0..100 {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            options.mode(0o600);
            match options.open(path) {
                Ok(mut file) => {
                    writeln!(file, "{}", std::process::id()).map_err(|error| error.to_string())?;
                    return Ok(Self {
                        path: path.to_path_buf(),
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    if lock_is_stale(path) {
                        let _ = fs::remove_file(path);
                    } else {
                        thread::sleep(Duration::from_millis(20 + attempt));
                    }
                }
                Err(error) => return Err(format!("create index lock: {error}")),
            }
        }
        Err("timed out waiting for pira_ctx index lock".to_string())
    }
}

impl Drop for StoreLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn lock_is_stale(path: &Path) -> bool {
    path.metadata()
        .and_then(|metadata| metadata.modified())
        .and_then(|modified| modified.elapsed().map_err(io::Error::other))
        .is_ok_and(|age| age > Duration::from_secs(300))
}

fn read_u32(reader: &mut File) -> Result<u32, String> {
    let mut bytes = [0_u8; 4];
    reader
        .read_exact(&mut bytes)
        .map_err(|error| format!("corrupt header: {error}"))?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64(reader: &mut File) -> Result<u64, String> {
    let mut bytes = [0_u8; 8];
    reader
        .read_exact(&mut bytes)
        .map_err(|error| format!("corrupt length: {error}"))?;
    Ok(u64::from_le_bytes(bytes))
}

fn write_u64(writer: &mut File, value: u64) -> Result<(), String> {
    writer
        .write_all(&value.to_le_bytes())
        .map_err(|error| error.to_string())
}

fn read_hash(reader: &mut File) -> Result<[u8; 32], String> {
    let mut bytes = [0_u8; 32];
    reader
        .read_exact(&mut bytes)
        .map_err(|error| format!("corrupt checksum: {error}"))?;
    Ok(bytes)
}

fn short_hash(bytes: &[u8], characters: usize) -> String {
    let digest = Sha256::digest(bytes);
    util::hex(&digest)[..characters].to_string()
}

fn format_utc_timestamp(seconds: u128) -> String {
    let days = (seconds / 86_400) as i64;
    let seconds_of_day = (seconds % 86_400) as u32;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        seconds_of_day / 3600,
        (seconds_of_day % 3600) / 60,
        seconds_of_day % 60
    )
}

fn civil_from_days(days_since_epoch: i64) -> (i32, u32, u32) {
    let shifted = days_since_epoch + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    (
        (year + i64::from(month <= 2)) as i32,
        month as u32,
        day as u32,
    )
}
