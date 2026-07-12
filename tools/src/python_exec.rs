use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::cli::Config;
use crate::model::StreamKind;
use crate::storage::StoredResult;

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

const BOOTSTRAP: &str = r#"import pathlib, sys
msg_path, stdout_path, stderr_path, msg_id, msg_exit, code_path, source_name = sys.argv[1:8]
MSG_BYTES = pathlib.Path(msg_path).read_bytes()
MSG = MSG_BYTES.decode("utf-8", "replace")
MSG_PATH = msg_path
MSG_STDOUT_PATH = stdout_path
MSG_STDERR_PATH = stderr_path
MSG_ID = msg_id
MSG_EXIT = int(msg_exit)
source = pathlib.Path(code_path).read_bytes()
scope = {
    "__name__": "__main__",
    "__file__": source_name,
    "MSG": MSG,
    "MSG_BYTES": MSG_BYTES,
    "MSG_PATH": MSG_PATH,
    "MSG_STDOUT_PATH": MSG_STDOUT_PATH,
    "MSG_STDERR_PATH": MSG_STDERR_PATH,
    "MSG_ID": MSG_ID,
    "MSG_EXIT": MSG_EXIT,
}
sys.argv = [source_name]
exec(compile(source, source_name, "exec"), scope, scope)
"#;

pub struct PreparedExec {
    _workspace: PrivateWorkspace,
    pub command: Vec<String>,
}

pub fn prepare(config: &Config, source: &StoredResult) -> Result<PreparedExec, String> {
    if source.metadata.timeline_truncated {
        return Err("cannot construct merged MSG from a result with a truncated line index".into());
    }
    let mut command = resolve_python(config)?;
    let workspace = PrivateWorkspace::create()?;
    let merged_path = workspace.path.join("merged.log");
    let stdout_path = workspace.path.join("stdout.log");
    let stderr_path = workspace.path.join("stderr.log");
    let code_path = workspace.path.join("analysis.py");
    materialize(source, &merged_path, &stdout_path, &stderr_path)?;

    let (code, source_name) = match (&config.exec_code, &config.exec_file) {
        (Some(code), None) => (code.as_bytes().to_vec(), "<pira_ctx-exec>".to_string()),
        (None, Some(path)) => (
            fs::read(path)
                .map_err(|error| format!("read analysis file {}: {error}", path.display()))?,
            path.display().to_string(),
        ),
        _ => return Err("choose exactly one --code CODE or --file PATH".into()),
    };
    write_private(&code_path, &code)?;

    command.extend([
        "-c".to_string(),
        BOOTSTRAP.to_string(),
        merged_path.display().to_string(),
        stdout_path.display().to_string(),
        stderr_path.display().to_string(),
        source.metadata.result_id.clone(),
        source.metadata.exit_code.to_string(),
        code_path.display().to_string(),
        source_name,
    ]);
    Ok(PreparedExec {
        _workspace: workspace,
        command,
    })
}

fn materialize(
    source: &StoredResult,
    merged_path: &Path,
    stdout_path: &Path,
    stderr_path: &Path,
) -> Result<(), String> {
    let mut reader = source.reader()?;
    let mut stdout = create_private(stdout_path)?;
    let mut stderr = create_private(stderr_path)?;
    reader.copy_section(StreamKind::Stdout, &mut stdout)?;
    reader.copy_section(StreamKind::Stderr, &mut stderr)?;

    let mut reader = source.reader()?;
    let mut merged = create_private(merged_path)?;
    for line in &source.metadata.line_timeline {
        reader.copy_line(line, &mut merged)?;
    }
    Ok(())
}

fn resolve_python(config: &Config) -> Result<Vec<String>, String> {
    if let Some(program) = config.python.as_deref() {
        let candidate = vec![program.to_string()];
        probe_python(&candidate).map_err(|error| format!("invalid --python PATH: {error}"))?;
        return Ok(candidate);
    }
    if let Some(program) = std::env::var_os("PIRA_CTX_PYTHON") {
        let candidate = vec![program.to_string_lossy().into_owned()];
        probe_python(&candidate).map_err(|error| format!("invalid PIRA_CTX_PYTHON: {error}"))?;
        return Ok(candidate);
    }

    let mut candidates = vec![vec!["python3".to_string()]];
    #[cfg(windows)]
    candidates.push(vec!["py".to_string(), "-3".to_string()]);
    candidates.push(vec!["python".to_string()]);
    for candidate in candidates {
        if probe_python(&candidate).is_ok() {
            return Ok(candidate);
        }
    }
    Err("Python 3 was not found; install it, pass --python PATH, or set PIRA_CTX_PYTHON".into())
}

fn probe_python(candidate: &[String]) -> Result<(), String> {
    let status = Command::new(&candidate[0])
        .args(&candidate[1..])
        .args([
            "-c",
            "import sys; raise SystemExit(0 if sys.version_info.major == 3 else 1)",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("cannot run {}: {error}", candidate[0]))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "{} is not a working Python 3 interpreter",
            candidate[0]
        ))
    }
}

struct PrivateWorkspace {
    path: PathBuf,
}

impl PrivateWorkspace {
    fn create() -> Result<Self, String> {
        let base = std::env::temp_dir();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for nonce in 0..100_u32 {
            let path = base.join(format!(
                ".pira_ctx-exec-{}-{now}-{nonce}",
                std::process::id()
            ));
            #[cfg(unix)]
            let mut builder = fs::DirBuilder::new();
            #[cfg(not(unix))]
            let builder = fs::DirBuilder::new();
            #[cfg(unix)]
            builder.mode(0o700);
            match builder.create(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(format!("create private analysis workspace: {error}")),
            }
        }
        Err("could not create a unique private analysis workspace".into())
    }
}

impl Drop for PrivateWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn create_private(path: &Path) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    options
        .open(path)
        .map_err(|error| format!("create private analysis file: {error}"))
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = create_private(path)?;
    file.write_all(bytes)
        .map_err(|error| format!("write private analysis file: {error}"))
}
