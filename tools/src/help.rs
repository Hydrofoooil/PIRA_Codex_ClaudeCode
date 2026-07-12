pub const GLOBAL: &str = r#"pira_ctx keeps external-command output bounded while retaining exact local captures for recovery.

Choosing a command:
  Run external commands:
    auto       Return ordinary short output in full; store complete noteworthy output and summarize.
    check      Use when only trustworthy PASS/FAIL and exit status are needed.
    exact      Request original output; repetitive non-interactive output may be stored/summarized.
               If complete output is still needed, use the returned ID with raw.
    capture    Use when complete output must be retained regardless of size (`summary` is an alias).
    batch      Use for several independent intent-tagged commands.

  Inspect a stored capture:
    search     Start here to locate relevant evidence.
    range      Retrieve the smallest sufficient exact line range.
    transform  Use for supported deterministic filtering, counting, aggregation, or slicing.
    exec       Use for custom Python analysis that prints only decision-relevant output.
    raw        Use when the complete exact capture or stream is genuinely required.
               Prefer the targeted commands above for agent analysis.

  Continue or maintain:
    recap      Restore recent execution context after same-session compaction.
    stats      Inspect workspace totals or capture metadata.
    verify     Check capture integrity.
    list       Find stored captures.
    prune      Enforce capture-retention limits.
    forget     Remove a capture or event history.

Common forms:
  pira_ctx [auto|exact|check|capture] --intent TEXT [OPTIONS] -- PROGRAM [ARG...]
  pira_ctx SUBCOMMAND [OPTIONS] [RESULT]
  pira_ctx batch [--store-dir PATH] SPEC_FILE [--intent TEXT]

RESULT is --last, a result ID or unambiguous prefix, a .piractx filename, or a path. Each invocation
resolves it once. Prefer an explicit ID; --last selects the latest capture for the current workspace.
INTENT is a non-empty, single-line immediate purpose of at most 256 UTF-8 bytes.
Normal wrapper completion has two output routes: complete output is returned exactly, or complete
stdout/stderr are stored before compact output is printed. This includes nonzero child exits. If
pira_ctx itself fails with 125, retention is not guaranteed. Child status is otherwise preserved.

Scope: --last, recap, stats without RESULT, and `forget events` use the current workspace (nearest
Git root, otherwise current directory). list and prune cover all workspaces in the selected store
unless an option narrows them. An explicit RESULT path bypasses store lookup. The store comes from
--store-dir, PIRA_CTX_STORE_DIR, or the platform user-cache default.

SUBCOMMAND is a pira_ctx operation such as search, transform, exec, or raw. PROGRAM is the external
executable being wrapped. Help is side-effect free: it does not execute PROGRAM, resolve RESULT,
access the store, read a spec/script, or probe Python. Run `pira_ctx SUBCOMMAND --help` for details.
The `--` delimiter ends pira_ctx parsing; every following value belongs to PROGRAM unchanged.
pira_ctx preserves caller permissions and does not sandbox external programs or Python analysis.
pira_ctx --version prints the installed version."#;

const AUTO: &str = r#"pira_ctx auto — run a command with automatic context routing

WHEN TO USE
  Use for most non-interactive external commands when output size and importance are unknown.
  Use check when only status matters, exact to request original output, or capture when retention is
  mandatory. `auto` may be omitted; both forms are equivalent.

USAGE
  pira_ctx [auto] [--store-dir PATH] --intent TEXT [--keyword QUERY ...] -- PROGRAM [ARG...]

OPTIONS
  --intent TEXT       Immediate purpose; required, single-line, at most 256 UTF-8 bytes.
  --keyword QUERY     Additional ranking term; repeatable up to 16 times.
  --store-dir PATH    Override the private per-user capture store.

OUTPUT AND STORAGE
  pira_ctx does not allocate a terminal. With a caller-provided terminal, auto streams through exact
  mode and does not create a capture. Non-interactive short ordinary output is returned in full and
  is not persisted. When retention triggers, complete exact stdout/stderr are
  stored before a bounded synopsis and capture ID are printed. Retention triggers at 2 KiB, for
  binary/non-UTF-8 or diagnostic output, for an oversized line, or when a nonzero command produced
  output. Short retained text is normally shown in full; stored bytes remain authoritative. Use
  capture when every completed output must be persisted.

EXIT STATUS
  Preserves the child status. Missing/non-executable commands use 127/126; wrapper failures use 125.

EXAMPLE
  pira_ctx --intent "Inspect repository status" -- git status --short"#;

const EXACT: &str = r#"pira_ctx exact — request original output with a repetition guard

WHEN TO USE
  Use when original file/output content is needed or the child requires interactive terminal I/O.
  Non-interactive repetitive output may still auto-switch. If that happens and every byte must enter
  output, use the returned capture ID with raw. Use automatic mode otherwise.

USAGE
  pira_ctx exact [--store-dir PATH] --intent TEXT -- PROGRAM [ARG...]

BEHAVIOR
  pira_ctx does not allocate a terminal. With a caller-provided terminal, stdout/stderr stream
  unchanged. Without one, output is buffered and replayed exactly unless textual output is both at
  least 2 KiB and at least 40 eligible lines, with substantial repeated-form coverage and a dominant
  repeated form. An auto-switch stores the complete streams, prints a notice, synopsis, and capture
  ID, and preserves child status. Varied long output remains exact.

EXAMPLES
  pira_ctx exact --intent "Read source for editing" -- sed -n '1,160p' src/main.rs
  pira_ctx exact --intent "Run interactive debugger" -- rust-gdb target/debug/app
  pira_ctx raw CAPTURE_ID  # after an announced auto-switch, if complete output is still needed"#;

const CHECK: &str = r#"pira_ctx check — retain a completed job and print only trustworthy status

WHEN TO USE
  Use for builds, tests, lint, compilation, or validation when the immediate decision is pass/fail.

USAGE
  pira_ctx check [--store-dir PATH] --intent TEXT -- PROGRAM [ARG...]

OUTPUT AND STORAGE
  Every completed child is retained, including empty or short output. Active output is one line:
    PASS|FAIL | exit=CODE | duration=Nms | result=ID
  PASS/FAIL depends only on child exit status. Spawn failures print result=- and have no capture.

EXIT STATUS
  Preserves the child status. Missing/non-executable commands use 127/126; wrapper failures use 125.

EXAMPLE
  pira_ctx check --intent "Verify the Rust test suite" -- cargo test --locked"#;

const CAPTURE: &str = r#"pira_ctx capture — always retain completed command output and return a synopsis

WHEN TO USE
  Use when the complete output must remain available regardless of size or diagnostic content.
  Use automatic mode when unconditional retention is unnecessary. `summary` is an alias.

USAGE
  pira_ctx capture [--store-dir PATH] --intent TEXT [--keyword QUERY ...] -- PROGRAM [ARG...]

OUTPUT AND STORAGE
  Every completed child is stored with exact stdout/stderr, metadata, indexes, compression, and
  integrity hashes. A bounded extractive synopsis and capture ID are printed, even for empty output.
  Spawn failures have no capture. The child exit status is preserved.

EXAMPLE
  pira_ctx capture --intent "Retain deployment diagnostics" -- ./deploy --diagnose"#;

const BATCH: &str = r#"pira_ctx batch — run bounded groups of independent intent-tagged commands

USAGE
  pira_ctx batch [--store-dir PATH] SPEC_FILE [--intent TEXT]

SPECIFICATION
  JSON object with 1..64 commands and concurrency 0..8 (0 means sequential):
    {"concurrency":2,"commands":[
      {"intent":"Check crate A","argv":["cargo","test","-p","a"]},
      {"intent":"Check crate B","argv":["cargo","test","-p","b"]}
    ]}
  Each argv must be non-empty. Every child needs its own intent or the top-level --intent fallback.

OUTPUT AND STORAGE
  Every completed child is retained, including empty and short successful output. Prints one compact
  table row per child in specification order with status, duration, capture ID, and intent.
  Concurrency is bounded at eight. The overall status is the last nonzero child status in
  specification order, or 0 when all succeed. Missing/non-executable child programs use 127/126 and
  have no capture ID; other wrapper failures use 125.

EXAMPLE
  pira_ctx batch checks.json"#;

const SEARCH: &str = r#"pira_ctx search — locate bounded evidence in a stored capture

WHEN TO USE
  Start here when relevant wording is known. Follow with a narrow range when exact nearby lines are
  needed. Use transform for systematic processing or exec for custom analysis.

USAGE
  pira_ctx search [--store-dir PATH] RESULT QUERY [--regex] [--context N]

OPTIONS AND OUTPUT
  Literal matching is Unicode case-insensitive. Only when it has no literal hits, a lexical fallback
  may return related lines. --regex uses Rust regex syntax and is case-sensitive unless the pattern
  requests otherwise. Up to five ranked hits are printed as line number, stream, score, and
  terminal-sanitized text. --context N (default 0) includes de-duplicated neighboring indexed lines,
  clipped at capture boundaries. Use range when exact unsanitized bytes are required.

EXIT STATUS
  Returns 0 even with no hits; invalid queries, missing results, or wrapper failures use 125.

EXAMPLE
  pira_ctx search 20260712-052432 'error|failed' --regex --context 2"#;

const RANGE: &str = r#"pira_ctx range — retrieve a small exact range from a capture timeline

WHEN TO USE
  Use after search identifies relevant line numbers. Request the smallest sufficient range; use raw
  only when the complete exact capture or stream is required.

USAGE
  pira_ctx range [--store-dir PATH] RESULT START_LINE END_LINE

BEHAVIOR
  Lines are 1-based and inclusive in merged stdout/stderr timeline order. Negative numbers count
  backward from the end; zero is invalid, and normalized start greater than end is an error.
  Out-of-bounds ranges are clipped without a separate notice. Exact stored bytes are written without
  display sanitization. A capture with a truncated legacy index cannot use range.

EXAMPLE
  pira_ctx range 20260712-052432 118 126"#;

const RAW: &str = r#"pira_ctx raw — reconstruct a complete capture or stream exactly

WHEN TO USE
  Use when complete exact bytes are required by the user or a downstream process. For agent
  analysis, prefer search, a narrow range, transform, or exec so the full capture does not re-enter
  active context.

USAGE
  pira_ctx raw [--store-dir PATH] RESULT [--stdout | --stderr]

BEHAVIOR
  Without a stream option, writes the complete merged stdout/stderr timeline to stdout. --stdout or
  --stderr writes only that complete stream, still to pira_ctx stdout. On success, stdout contains
  only the selected capture bytes—no receipt or metadata. Bytes are not decoded or terminal-
  sanitized. A truncated legacy timeline requires selecting one stream.

EXAMPLES
  pira_ctx raw 20260712-052432 --stderr
  pira_ctx raw 20260712-052432 --stdout >complete.stdout"#;

const TRANSFORM: &str = r#"pira_ctx transform — deterministically process stored capture lines

WHEN TO USE
  Use for filtering, deduplication, counting, grouping, sorting, numeric aggregation, JSONL fields,
  columns, streams, or bounded slicing. Use exec when custom Python or cross-line logic is clearer.

USAGE
  pira_ctx transform [--store-dir PATH] RESULT [--plan FILE] [--match REGEX ...]
                     [--exclude REGEX ...] [--unique] [--count] [--head N] [--tail N]

DIRECT OPTIONS
  Lines are replacement-decoded text with trailing CR/LF removed. Regexes use Rust syntax, are
  case-sensitive by default, and accept inline flags such as (?i). Repeated --match values are all
  required; any --exclude match removes a line. Operations apply as match, exclude, unique, head,
  tail, then count. unique compares resulting text and keeps first occurrence; count prints one
  decimal integer. Direct processing streams where possible and caps returned text at 64 KiB.

PLAN FILE
  JSON object {"steps":[STEP,...]}; steps run in order after CLI filters. Valid STEP objects:
    {"op":"match|exclude","regex":"..."}
    {"op":"context","regex":"...","before":N,"after":N}
    {"op":"head|tail|top","n":N} | {"op":"sort","numeric":true|false}
    {"op":"json_field","field":"name"} | {"op":"json_eq","field":"name","value":JSON}
    {"op":"column","index":N,"delimiter":"..."} | {"op":"stream","stream":"stdout|stderr"}
    {"op":"unique|count|group_count|sum|min|max|mean|diagnostic"}
  Numeric reductions use Rust f64 parsing/formatting, fail on nonnumeric text, and preserve accepted
  non-finite values. Malformed JSONL is an error for json_field; strings emit their contents, other
  JSON values emit compact JSON, and absent fields emit an empty string. json_eq treats malformed
  JSONL as nonmatching. column index is zero-based and delimiter defaults to tab. Plans materialize
  at most 1,000,000 rows and 128 MiB of exact uncompressed line bytes; parse/limit failures exit 125.

EXAMPLES
  pira_ctx transform RESULT --match 'FAILED|ERROR' --count
  pira_ctx transform RESULT --plan analysis.json
  analysis.json: {"steps":[{"op":"json_field","field":"value"},{"op":"sum"}]}"#;

const EXEC: &str = r#"pira_ctx exec — analyze a stored capture with explicit Python 3 code

WHEN TO USE
  Use for substantial or custom analysis not covered clearly by transform. Print only the result
  needed for the current decision; analysis output itself follows non-interactive automatic routing.

USAGE
  pira_ctx exec [--store-dir PATH] RESULT --intent TEXT
                (--code CODE | --file PATH) [--python PATH]

BINDINGS
  MSG                 Merged text with invalid UTF-8 replaced by U+FFFD.
  MSG_BYTES           Exact merged bytes.
  MSG_PATH            Private temporary merged-capture path.
  MSG_STDOUT_PATH     Private temporary exact-stdout path.
  MSG_STDERR_PATH     Private temporary exact-stderr path.
  MSG_ID              Resolved source capture ID.
  MSG_EXIT            Source command exit code.

BEHAVIOR
  --last resolves once before execution. Choose exactly one code source. Interpreter order is
  --python PATH, PIRA_CTX_PYTHON, python3, Windows `py -3`, then python. Python is optional for all
  other commands. MSG_BYTES and MSG eagerly load the complete merged capture into Python memory;
  there is no separate exec size cap, so prefer transform for very large inputs when sufficient.
  Temporary paths exist only during execution. Analysis status is preserved; retained analysis
  metadata links to the source ID through its command. Code runs with caller permissions and is not
  sandboxed.

EXAMPLES
  pira_ctx exec --last --intent "Count failures" --code 'print(MSG.count("FAILED"))'
  pira_ctx exec RESULT --intent "Extract errors" --file analysis.py"#;

const RECAP: &str = r#"pira_ctx recap — restore recent same-session execution context after compaction

WHEN TO USE
  Run immediately after model context compaction before further substantive shell/exec work. It is
  not intended to reconstruct a separate new session.

USAGE
  pira_ctx recap [--store-dir PATH] [--limit N]

OUTPUT
  Prints a bounded <pira_context_restore> block containing selected recent intents, observed status,
  redacted commands, detected files, and capture IDs for the current workspace. Default limit is 20;
  total output is bounded below 8 KiB. Recap reads event hints and does not rerun commands.

EXAMPLE
  pira_ctx recap --limit 10"#;

const STATS: &str = r#"pira_ctx stats — show workspace totals or capture metadata

USAGE
  pira_ctx stats [--store-dir PATH] [RESULT]

OUTPUT
  Without RESULT, prints current-workspace capture count, captured bytes, event count, and workspace
  hash. With RESULT, prints command, cwd, status, duration, stream sizes/lines, store path, format,
  index state, binary/non-UTF-8 flags, detected paths, and suggested keywords. It does not print
  captured content.

EXAMPLES
  pira_ctx stats
  pira_ctx stats --last"#;

const VERIFY: &str = r#"pira_ctx verify — verify a stored capture's structure and stream integrity

USAGE
  pira_ctx verify [--store-dir PATH] RESULT

BEHAVIOR
  Validates the container layout, authenticated metadata/index/block tables, and exact stdout/stderr
  hashes supported by its format. Prints the verified path on success and does not modify the capture.
  Corruption, missing results, or wrapper failures use exit 125.

EXAMPLE
  pira_ctx verify 20260712-052432"#;

const LIST: &str = r#"pira_ctx list — list stored captures

USAGE
  pira_ctx list [--store-dir PATH] [--workspace current]

OUTPUT
  Prints newest-first rows with ID, timestamp, exit status, bytes, lines, and redacted command.
  Without --workspace current, entries from every workspace in the selected store are listed.

EXAMPLE
  pira_ctx list --workspace current"#;

const PRUNE: &str = r#"pira_ctx prune — enforce capture age or total-storage limits

USAGE
  pira_ctx prune [--store-dir PATH] [--max-age-days N] [--max-store-bytes N]

BEHAVIOR
  At least one limit is required. prune covers every workspace in the selected store. Captures whose
  start time is strictly older than N*24 hours are removed first; if remaining capture-container file
  bytes exceed the limit, oldest captures are removed until within budget. Age pruning also removes
  old event files across the store. Prints removed and remaining capture-file counts/bytes. Deletion
  is immediate; use list or stats before pruning when the scope needs inspection.

EXAMPLE
  pira_ctx prune --max-age-days 30 --max-store-bytes 1073741824"#;

const FORGET: &str = r#"pira_ctx forget — remove one capture or current-workspace event history

USAGE
  pira_ctx forget [--store-dir PATH] RESULT
  pira_ctx forget [--store-dir PATH] events

BEHAVIOR
  RESULT resolves using normal ID/prefix/filename/path rules. An explicit path bypasses store lookup
  and may identify a valid capture outside --store-dir. The target must pass capture structure and
  integrity verification before removal. `events` is reserved here and removes only recap event
  files for the current workspace, not captures. Deletion is immediate; this operation is not
  transactional across filesystem failures. The removed path or event count is printed.

EXAMPLES
  pira_ctx forget 20260712-052432
  pira_ctx forget events"#;

const VERSION: &str = r#"pira_ctx version — print the installed pira_ctx version

USAGE
  pira_ctx --version
  pira_ctx version

OUTPUT
  Prints `pira_ctx MAJOR.MINOR.PATCH` and exits 0."#;

pub fn canonical_topic(topic: &str) -> Option<&'static str> {
    Some(match topic {
        "auto" | "default" => "auto",
        "capture" | "summary" => "capture",
        "exact" => "exact",
        "check" => "check",
        "batch" => "batch",
        "search" => "search",
        "range" => "range",
        "raw" => "raw",
        "transform" => "transform",
        "exec" => "exec",
        "recap" => "recap",
        "stats" => "stats",
        "verify" => "verify",
        "list" => "list",
        "prune" => "prune",
        "forget" => "forget",
        "version" | "--version" | "-V" => "version",
        _ => return None,
    })
}

pub fn command(topic: &str) -> Option<&'static str> {
    Some(match canonical_topic(topic)? {
        "auto" => AUTO,
        "exact" => EXACT,
        "check" => CHECK,
        "capture" => CAPTURE,
        "batch" => BATCH,
        "search" => SEARCH,
        "range" => RANGE,
        "raw" => RAW,
        "transform" => TRANSFORM,
        "exec" => EXEC,
        "recap" => RECAP,
        "stats" => STATS,
        "verify" => VERIFY,
        "list" => LIST,
        "prune" => PRUNE,
        "forget" => FORGET,
        "version" => VERSION,
        _ => unreachable!(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_global_commands_have_detailed_help() {
        for topic in [
            "auto",
            "exact",
            "check",
            "capture",
            "batch",
            "search",
            "range",
            "transform",
            "exec",
            "raw",
            "recap",
            "stats",
            "verify",
            "list",
            "prune",
            "forget",
        ] {
            let text = command(topic).unwrap();
            assert!(text.contains("USAGE"), "missing usage for {topic}");
            assert!(text.len() < 3_500, "help too long for {topic}");
        }
        assert!(GLOBAL.len() < 3_500);
        assert!(RAW.contains("prefer search, a narrow range, transform, or exec"));
    }
}
