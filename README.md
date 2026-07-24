# PIRA — PI Research Assistant

PIRA (pronounced "Pyra") is a research-oriented personal agent for reasoning, writing, coding, learning, and practical problem-solving.

PIRA is designed to be warm, honest about uncertainty, evidence-first when evidence matters, and easy to inspect and customize.

## Tested compatibility

PIRA has been tested extensively with **Codex on GPT-5.4, GPT-5.5, and 5.6-sol, each with high reasoning effort**.

PIRA also installs for **Claude Code** through a dedicated setup path; see [Claude Code](#claude-code). That integration is functional but newer and has received lighter testing than the Codex path.

## Quick start

PIRA installs to `~/agent` by default. Setup is idempotent, backs up user-level Codex files before editing them, supports dry-run and verification modes, and is safe to rerun. Git is required; the setup wrapper handles Python discovery and can offer platform-specific installation help.

### Recommended one-line install or update

This command:

- uses the existing `~/agent` git checkout when present, otherwise clones PIRA into `~/agent`;
- enables **soft-safe** mode;
- keeps audio notifications **off**;
- links PIRA into Codex;
- installs or refreshes bundled PIRA tools such as `pira_ctx` in the user's `PATH`;
- moves old PIRA-managed legacy files into backup;
- creates a private `USER.md` placeholder only if `USER.md` is missing.

macOS/Linux:

```bash
if [ -d ~/agent/.git ]; then cd ~/agent && git pull --ff-only; else git clone https://github.com/Hydrofoooil/PIRA_Codex_ClaudeCode.git ~/agent && cd ~/agent; fi && assets/scripts/setup_pira.sh --yes --execution-mode keep --audio no --user-mode placeholder --global-agents link --legacy remove
```

Windows PowerShell:

```powershell
if (Test-Path "$HOME/agent/.git") { Set-Location "$HOME/agent"; git pull --ff-only; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE } } else { git clone https://github.com/Hydrofoooil/PIRA_Codex_ClaudeCode.git "$HOME/agent"; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }; Set-Location "$HOME/agent" }; powershell.exe -ExecutionPolicy Bypass -File assets/scripts/setup_pira.ps1 --yes --execution-mode keep --audio no --user-mode placeholder --global-agents link --legacy remove
```

If you are rerunning setup and want a missing `USER.md` to stay missing, use `--user-mode keep` instead.

macOS/Linux:

```bash
cd ~/agent && git pull --ff-only && assets/scripts/setup_pira.sh --yes --execution-mode keep --audio no --user-mode keep --global-agents link --legacy remove
```

Windows PowerShell:

```powershell
Set-Location "$HOME/agent"; git pull --ff-only; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }; powershell.exe -ExecutionPolicy Bypass -File assets/scripts/setup_pira.ps1 --yes --execution-mode keep --audio no --user-mode keep --global-agents link --legacy remove
```

`git pull --ff-only` updates an existing checkout only when Git can do so without a merge. If you have tracked local edits or a divergent branch, it stops for manual review.

> **Soft-safe is not a sandbox.** It sets Codex to no-approval/full-permission mode and relies on PIRA's explicit safety rules before state-changing commands.

### Inspect-first install

Use this path if you want to preview setup before writing anything:

```bash
git clone https://github.com/Hydrofoooil/PIRA_Codex_ClaudeCode.git ~/agent
cd ~/agent
assets/scripts/setup_pira.sh --dry-run
assets/scripts/setup_pira.sh
assets/scripts/setup_pira.sh --verify
```

On Windows, invoke the same setup through `assets/scripts/setup_pira.ps1` from the repository directory:

```powershell
powershell.exe -ExecutionPolicy Bypass -File assets/scripts/setup_pira.ps1
```

Both platform wrappers forward the same options to `assets/scripts/setup_pira.py`. They share the Python bootstrap helpers in `assets/scripts/lib/`; setup can offer to install Python with Homebrew on macOS or winget on Windows.

## Setup options

<details>
<summary>Execution, user configuration, and tool-install options</summary>

The script asks before sensitive choices in interactive mode. For unattended setup, pass explicit flags.

### Execution mode

| Option                         | Codex settings                                                           | Use when                                              |
| ------------------------------ | ------------------------------------------------------------------------ | ----------------------------------------------------- |
| `--execution-mode safe`      | `approval_policy = "on-request"`, `sandbox_mode = "workspace-write"` | You want a real approval/sandbox boundary.            |
| `--execution-mode soft-safe` | `approval_policy = "never"`, `sandbox_mode = "danger-full-access"`   | You want convenience and accept full-permission risk. |
| `--execution-mode keep`      | Leaves existing approval/sandbox settings unchanged.                     | You already manage Codex permissions yourself.        |

### `USER.md` mode

| Option                      | Behavior                                                                                        |
| --------------------------- | ----------------------------------------------------------------------------------------------- |
| `--user-mode placeholder` | Creates a private placeholder`USER.md` when it is missing. Existing `USER.md` is preserved. |
| `--user-mode keep`        | Leaves`USER.md` exactly as-is; if it is missing, setup leaves it missing.                     |
| `--user-mode interactive` | Asks what to do when`USER.md` is missing.                                                     |

### Other useful flags

| Option                                 | Behavior                                                                                                                     |
| -------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `--yes`                              | Accepts setup confirmations. It does**not** enable audio unless `--audio yes` is also set.                           |
| `--audio yes\|no\|ask`                 | Controls optional Codex audio notifications. Use`--audio no` for a quiet install.                                          |
| `--global-agents link\|copy\|skip\|ask` | Controls whether`~/.codex/AGENTS.md` points to PIRA by symlink, copy, or not at all.                                       |
| `--legacy remove\|keep\|ask`           | Controls paths listed in`assets/LEGACY_LIST.md`; `remove` moves active legacy files into `.backup/setup_pira_legacy/`. |
| `--agent-dir PATH`                   | Installs against a path other than`~/agent`.                                                                               |
| `--skip-tools`                       | Skips installation or refresh of bundled native PIRA tools.                                                                  |
| `--tools-install-dir PATH`           | Overrides the per-user tools directory (`~/.local/bin` on macOS/Linux or `%LOCALAPPDATA%\PIRA\bin` on Windows).          |
| `--verify`                           | Checks the current setup without writing.                                                                                    |
| `--dry-run`                          | Prints planned changes without applying them.                                                                                |

### Install or refresh only the PIRA tools

If PIRA is already configured and you only need to install, update, or reinstall its bundled native tools, run the tools-only setup from the existing PIRA checkout. Update that checkout first when you want a newer bundled release. The normal command installs a missing tool, replaces a stale copy, and leaves an identical verified copy unchanged.

On macOS or Linux:

```bash
cd ~/agent
python3 assets/scripts/setup_pira_tools.py          # install or refresh
python3 assets/scripts/setup_pira_tools.py --force  # reinstall the same bundled release
python3 assets/scripts/setup_pira_tools.py --verify # verify without writing
```

On Windows PowerShell:

```powershell
cd $HOME\agent
py -3 assets/scripts/setup_pira_tools.py          # install or refresh
py -3 assets/scripts/setup_pira_tools.py --force  # reinstall the same bundled release
py -3 assets/scripts/setup_pira_tools.py --verify # verify without writing
```

Use `--force` to reinstall even when the installed hash already matches the bundled release. Use `--install-dir PATH` to override the tools-only default (`~/.local/bin` on macOS/Linux or `%LOCALAPPDATA%\PIRA\bin` on Windows), and `--no-path` when PATH persistence is managed separately. Restart the shell or agent process if setup reports that PATH changes are not yet active.

</details>

## What setup changes

<details>
<summary>Files, settings, tools, and verification performed by setup</summary>

The setup script:

1. Detects the repository directory and ensures it is available as `~/agent`, unless another `--agent-dir` is given.
2. Initializes a private `USER.md` placeholder when needed.
3. Moves legacy files listed in `assets/LEGACY_LIST.md` into `.backup/setup_pira_legacy/` when approved.
4. Updates or creates Codex `config.toml` so the selected agent directory's `AGENTS.md` is loaded, with `project_doc_max_bytes = 65536`.
5. Optionally links or copies `~/.codex/AGENTS.md` for Codex's global AGENTS discovery path.
6. Selects and verifies the bundled native tool for the current platform, then installs or refreshes it in a per-user PATH directory. Existing stale copies are atomically replaced; matching copies are left unchanged.
7. Optionally delegates audio setup to the platform-specific audio helper.
8. Verifies the setup, including the PIRA verification token and installed native tool.

If setup cannot safely handle an existing conflicting file or Codex setting, it stops or skips that action with a warning instead of silently overwriting it.

</details>

## Claude Code

PIRA's policy files are agent-agnostic, and Claude Code can load them every session through its memory-import mechanism. The Claude Code setup maintains a clearly marked, PIRA-managed block inside the user memory file `~/.claude/CLAUDE.md`. The block uses Claude Code's `@path` import syntax to load `AGENTS.md`, `SOUL.md`, `TOOLS.md`, and `USER.md` at session start; the optional modules keep loading on demand through the routing rules in `AGENTS.md`. Existing content in `~/.claude/CLAUDE.md` is preserved, only the marked block is created or replaced, and edited files are backed up first.

macOS/Linux:

```bash
if [ -d ~/agent/.git ]; then cd ~/agent && git pull --ff-only; else git clone https://github.com/Hydrofoooil/PIRA_Codex_ClaudeCode.git ~/agent && cd ~/agent; fi && assets/scripts/setup_pira_claude.sh --yes --execution-mode keep --user-mode placeholder --legacy remove
```

Windows PowerShell:

```powershell
if (Test-Path "$HOME/agent/.git") { Set-Location "$HOME/agent"; git pull --ff-only; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE } } else { git clone https://github.com/Hydrofoooil/PIRA_Codex_ClaudeCode.git "$HOME/agent"; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }; Set-Location "$HOME/agent" }; powershell.exe -ExecutionPolicy Bypass -File assets/scripts/setup_pira_claude.ps1 --yes --execution-mode keep --user-mode placeholder --legacy remove
```

<details>
<summary>Execution modes, verification, and differences from the Codex integration</summary>

### Execution mode

The Claude Code execution modes map onto `permissions.defaultMode` in `~/.claude/settings.json`. Other settings in that file are preserved; if the file is not valid JSON, permission settings are left unchanged with a warning.

| Option                         | Claude Code settings                              | Use when                                                                                                                                                                                                                      |
| ------------------------------ | ------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--execution-mode safe`      | `permissions.defaultMode = "default"`           | You want Claude Code to keep asking before sensitive actions.                                                                                                                                                                 |
| `--execution-mode soft-safe` | `permissions.defaultMode = "bypassPermissions"` | You want convenience and accept full-permission risk. Claude Code's own documentation recommends isolated environments for this mode; PIRA's explicit safety rules in`TOOLS.md` are the operating guardrail, not a sandbox. |
| `--execution-mode keep`      | Leaves permission settings unchanged.             | You already manage Claude Code permissions yourself.                                                                                                                                                                          |

### Shared behavior and options

`--verify`, `--dry-run`, `--yes`, `--agent-dir`, `--user-mode`, `--legacy`, `--skip-tools`, and `--tools-install-dir` behave exactly as in the Codex setup, and the agent directory, `USER.md` handling, legacy cleanup, and bundled `pira_ctx` installation are the same shared steps. Use `--claude-dir PATH` to target a non-default Claude Code configuration directory and `--skip-claude` to leave Claude Code configuration untouched.

### Differences from the Codex integration

- Claude Code does not read `AGENTS.md` natively, so the managed block in `~/.claude/CLAUDE.md` imports it explicitly. If `USER.md` is missing and `--user-mode keep` is selected, its import is omitted with a warning; rerun setup after creating it.
- Claude Code loads user memory, including the PIRA imports, into most subagents automatically; its built-in Explore and Plan agents intentionally skip memory files and therefore run without the PIRA bootstrap.
- Audio notifications are not supported for Claude Code; the `--audio` options exist only in the Codex setup.
- Both integrations can coexist on the same machine: they read the same `~/agent` policy files, and each is configured independently.

</details>

### Optional: auto-inject a project memory index (SessionStart hook)

If you keep a per-project memory index — for example a `WORKLOG/WORKLOG.md` task index at the repository root — a rule like "read the worklog before starting work" is followed only probabilistically, because instruction files are requests to the model, not triggers. A Claude Code `SessionStart` hook makes it deterministic: whatever the hook command prints is injected into the model's context at session startup, on resume, and right after context compaction, so the index is always in front of the model without relying on compliance.

Merge the following into `~/.claude/settings.json` (keep your existing keys; back the file up first):

```json
{
  "hooks": {
    "SessionStart": [
      {
        "matcher": "startup",
        "hooks": [
          {
            "type": "command",
            "command": "r=$(git rev-parse --show-toplevel 2>/dev/null) || r=$PWD; f=\"$r/WORKLOG/WORKLOG.md\"; if [ -f \"$f\" ]; then echo \"[SessionStart hook] Project WORKLOG index follows; per the logging rules, identify the current task and read its task log:\"; cat \"$f\"; fi; exit 0",
            "timeout": 10
          }
        ]
      },
      {
        "matcher": "resume",
        "hooks": [
          {
            "type": "command",
            "command": "r=$(git rev-parse --show-toplevel 2>/dev/null) || r=$PWD; f=\"$r/WORKLOG/WORKLOG.md\"; if [ -f \"$f\" ]; then echo \"[SessionStart hook] Project WORKLOG index follows; per the logging rules, identify the current task and read its task log:\"; cat \"$f\"; fi; exit 0",
            "timeout": 10
          }
        ]
      },
      {
        "matcher": "compact",
        "hooks": [
          {
            "type": "command",
            "command": "r=$(git rev-parse --show-toplevel 2>/dev/null) || r=$PWD; f=\"$r/WORKLOG/WORKLOG.md\"; if [ -f \"$f\" ]; then echo \"[SessionStart hook] Project WORKLOG index follows; per the logging rules, identify the current task and read its task log:\"; cat \"$f\"; fi; exit 0",
            "timeout": 10
          }
        ]
      }
    ]
  }
}
```

Notes:

- The command locates the repository root with git, prints the index only when `WORKLOG/WORKLOG.md` exists, and exits silently otherwise. It is read-only. Adjust the announcement line and the index path to your own memory layout.
- Hooks are snapshotted at session start: already-open sessions are unaffected; new sessions pick the hook up. Use `/hooks` inside Claude Code to review, edit, or disable it later.
- Verify end-to-end from a project that has an index: `claude -p "name the first task in the injected WORKLOG index"`.

## `pira_ctx`: lightweight command context

<details>
<summary>How it works, security design, Context Mode comparison, and benchmarks</summary>

`pira_ctx` keeps large command output from overwhelming active context while retaining it locally within configurable space limits. Automatic mode is the default and its name can be omitted:

1. Ordinary short output is returned directly.
2. Long or diagnostic output is stored locally, while the model receives a bounded extractive synopsis and a capture ID.
3. If more detail is needed, the retained capture remains available for targeted search, line-range retrieval, transformation, or exact replay.

For compile, test, lint, or other validation jobs where only success or failure matters, `check` stores the retained log but prints one PASS/FAIL status line with the child exit code and capture ID.

Explicit `exact` mode streams unchanged when attached to a terminal. In non-interactive calls it buffers the result so that highly repetitive output—or output exceeding retention or indexing bounds—can switch to a retained report instead of flooding or silently truncating context. Genuinely varied output remains exact, and every switch is announced.

For a non-interactive program still running after about 30 seconds, `pira_ctx` silently publishes a read-only checkpoint. Concurrent inspection uses a consistent snapshot without blocking the program; `exec` receives a private copy. Running captures are protected from verification, deletion, and pruning until completion. See `pira_ctx --help` for the exact contract.

Setup installs a verified native executable in the user's `PATH`. Normal use requires no Python, Rust toolchain, daemon, database, network service, or model call; the optional `exec` command uses an available Python 3 interpreter to analyze a stored capture with explicit code. Captures are private user-cache files with independently compressed blocks and integrity hashes. `pira_ctx` preserves the caller's permissions and does not sandbox commands. Run `pira_ctx --help` to choose a command and `pira_ctx SUBCOMMAND --help` for exact usage. Its Rust source is under `tools/src/pira_ctx`, and verified builds for macOS arm64/x64, Linux arm64/x64, and Windows x64 are under `tools/dist/pira_ctx`.

### Security design

`pira_ctx` treats PROGRAM output as untrusted data, but it is not a sandbox and does not make the wrapped program safe. Its security boundary covers harm introduced by capturing, storing, selecting, or displaying PROGRAM output:

- **Injection-aware display.** Agent-facing extracts are labeled as PROGRAM data, which PIRA rules treat as untrusted, and use trusted line and stream prefixes. Terminal escapes, Unicode line separators, bidirectional overrides, and invisible direction controls are sanitized so output cannot forge report structure or manipulate normal automatic display. A bounded heuristic scans the final displayed text for reserved role/wrapper markers and common **English** injection keywords, including English instructions split across displayed lines. Keyword detection is not multilingual; non-English text is detected only when it also contains a recognized marker or unsafe display control. When triggered, one warning appears before the evidence. Detection never suppresses or re-ranks evidence, and benign output pays no warning-token cost.
- **Explicit exactness.** Automatic mode retains short output matched by the advisory heuristic instead of replaying it directly; short `exec` output follows the same routing. `search` applies the same warning. Exact byte-replay paths in `exact`, `raw`, and `range` remain unsanitized because utility and faithful recovery take precedence; they remain untrusted data under PIRA's agent rules.
- **Bounded space, unbounded time.** Retention defaults to 512 MiB and 1,000,000 indexed lines, with a 2,000,000-line hard ceiling, while eager Python `exec` materialization defaults to 64 MiB. These ceilings are configurable within their safety bounds. Excess output is drained but not retained, and the command continues. `pira_ctx` imposes no runtime timeout or time-based termination, leaving cancellation to the agent or user.
- **Private, checked storage.** Captures use private user-cache files, independently compressed and SHA-256-checked blocks, validated offsets and lengths, and authenticated metadata/index tables. Common secret-bearing command arguments are redacted from metadata, and result IDs do not derive from raw arguments. Output may still contain secrets, and integrity hashes detect corruption rather than authenticate data against a same-user attacker.

Security checks are separate from ordinary functional tests and run as fixed, non-destructive fixtures in a deny-by-default sandbox with deliberately tiny configurable limits. Against 0.7.1 on 45 held-out benign real logs, 0.8.0 produced no false warnings, returned byte-identical responses, and showed no measurable median runtime regression in an alternating comparison. The live concurrency contract was also exercised with an inert delayed program in an isolated Linux Docker Sandbox. This is best-effort hardening, not a guarantee that every adversarial instruction will be detected; the primary boundary is the rule that PROGRAM output is data and cannot grant authority.

### Relationship to Context Mode

`pira_ctx` was informed by [Context Mode](https://github.com/mksglu/context-mode), especially its ideas of keeping raw tool output out of context, attaching intent to execution, retrieving indexed evidence after compaction, and analyzing stored output with small programs. We thank its contributors for publishing and explaining these ideas.

| Dimension           | `pira_ctx`                                                 | Context Mode                                                            |
| ------------------- | ------------------------------------------------------------ | ----------------------------------------------------------------------- |
| Integration         | Native wrapper for explicit external commands                | MCP server plus platform plugins and hooks                              |
| Runtime and storage | One Rust executable and self-contained checked capture files | Node/Bun integration with a SQLite FTS5 knowledge base                  |
| Reach               | Commands deliberately routed through the wrapper             | Broader shell, file, web, and MCP routing where integrations support it |
| Continuity          | Bounded same-session recap after compaction                  | Explicit session lifecycle and continuation support                     |
| Safety scope        | Preserves caller permissions; does not sandbox children      | Adds sandbox and permission-policy integration                          |

PIRA uses `pira_ctx` when a small single-binary wrapper and exact local fallback are preferable. Context Mode is the more comprehensive option when broader interception, hooks, sandboxing, or database-backed retrieval are needed.

### Comprehensive held-out benchmark

The fixed benchmark caps each category at five cases and contains **45 sanitized responses across ten categories**. Its individual fixture contents were not seen during development of the output-selection design and were not used to tune selection, scoring, thresholds, injection heuristics, or live checkpointing; the fixed runner served as a regression and final measurement gate. The table reports the final `pira_ctx 0.8.0` release candidate on that corpus:

| Suite                   | Cases | Holdout source                                                                           |
| ----------------------- | ----: | ---------------------------------------------------------------------------------------- |
| Public-repository core  |    25 | New outputs generated after the freeze from ten fixed Rust repositories                  |
| Remote status workloads |    15 | Previously unseen Codex outputs streamed from a remote machine after the freeze          |
| arXiv LaTeX supplement  |     5 | Isolated builds of seeded recent arXiv papers, including natural and controlled failures |

The remote importer scanned raw logs in memory and persisted only fixed-point sanitized, privacy-audited fixtures; unsanitized server output was not written locally. Final selection is independent of PIRA output: SHA-256 order with a five-case cap, while build and test categories prefer three successes and two failures. No output routing, scoring, threshold, or security behavior changed after the final reported replay.

| Mode on the same 2,248,456 raw bytes  |           Returned context |          Complete stored state |                    Median overhead | Immediate labeled evidence |
| ------------------------------------- | -------------------------: | -----------------------------: | ---------------------------------: | -------------------------: |
| `pira_ctx 0.8.0` automatic synopsis | 44,222 B (98.0% reduction) |    602,349 B (73.2% reduction) |                           +14.3 ms |                       5/13 |
| Context Mode generic passthrough      | 71,621 B (96.8% reduction) | 17,039,820 B (657.8% overhead) |                           +16.1 ms |                       9/13 |
| `pira_ctx 0.8.0 check`              |  3,064 B (99.9% reduction) |    602,484 B (73.2% reduction) |                           +13.2 ms |           N/A—status only |
| Context Mode`ctx_index` receipt     |  7,843 B (99.7% reduction) | 13,992,387 B (522.3% overhead) | N/A—no corresponding raw baseline |                       0/13 |

All 45 PIRA cases preserved child status, entered full automatic-summary mode, reconstructed every sanitized output exactly, and passed integrity verification. Suggestions correctly abstained in 32/32 successful unlabeled cases; immediate evidence covered 5/8 failure markers and 0/5 changed basenames. Version 0.8.0 does not change selection or scoring: the same fixed replay gives identical quality counts with 0.7.1. Context Mode generic passthrough classified all 45 recorded statuses correctly and immediately exposed 7/8 failure markers plus 2/5 changed basenames. These quality figures were not used for tuning.

<details>
<summary>Benchmark method, category results, Context Mode comparison, and limitations</summary>

#### Corpus and evaluation protocol

The prospective public core covers VCS patches, largest tracked Rust files, recursive declaration listings, 40-commit terminal logs, and GitHub pull-list responses. Exact and structural duplicates against earlier private corpora were excluded. Public changed basenames were preserved as sanitized metadata so suggestion labels remained observable. Five cases per category were selected by content SHA-256, producing 25 core cases.

The remote extension was fixed before inspecting output content. It reconstructed completed `exec_command` and `write_stdin` sessions from 2.73 GB of authorized Codex logs, streamed 683 category candidates through an in-memory sanitizer, retained 289 eligible unique responses, and selected cases by outcome, size bucket, session diversity, and content hash. The final five-case cap retained three successful and two failed builds, three successful and two failed tests, four setup/install responses, and one static-analysis response. The server contained no LaTeX response above the 2 KiB threshold.

LaTeX coverage therefore uses arXiv sources compiled inside the retained Docker Sandbox with TeX Live and shell escape disabled. Candidate papers came from a binary-seeded shuffle of the recent `cs.LG` API pool. Repeated transport interruptions caused the live recent-entry pool to drift, so the five already downloaded public identifiers were frozen before corpus persistence or PIRA evaluation. One paper compiled successfully; its fresh source also produced a controlled undefined-command failure. Three additional papers contributed natural compilation failures, yielding one pass and four failures. Raw paper sources were disposable and were not committed.

Each suite's output-quality labels were fixed during the original holdout evaluation and were not revised for 0.8.0. The visible aggregate performance figures come from the final no-tuning 0.8.0 replay of the selected 45 fixtures through one persistent automatic store and one persistent `check` store. Every call used an identical raw fixture-emitter baseline; overhead is `wrapped wall time - raw-operation wall time`, summarized by the per-case median. Stored state includes captures, indexes, and event history but excludes installed binaries and runtimes.

| Held-out category      | Cases |             Outcomes |            Immediate quality | Context reduction |
| ---------------------- | ----: | -------------------: | ---------------------------: | ----------------: |
| File reads             |     5 |            5 success |              5/5 abstentions |             99.2% |
| GitHub pull retrieval  |     5 |            5 success |              5/5 abstentions |             99.4% |
| Search and listing     |     5 |            5 success |              5/5 abstentions |             98.9% |
| Terminal logs          |     5 |            5 success |              5/5 abstentions |             95.5% |
| Version-control diffs  |     5 |            5 success |        0/5 changed basenames |             91.1% |
| Builds                 |     5 | 3 success, 2 failure | 3/3 abstentions; 0/2 markers |             75.4% |
| Test runs              |     5 | 3 success, 2 failure | 3/3 abstentions; 2/2 markers |             87.9% |
| Setup and installation |     4 |            4 success |              4/4 abstentions |             78.5% |
| Static analysis        |     1 |            1 success |               1/1 abstention |             92.5% |
| LaTeX compilation      |     5 | 1 success, 4 failure |  1/1 abstention; 3/4 markers |             94.6% |

#### Context Mode comparison on the final corpus

Context Mode 1.0.169 was installed inside the retained Docker Sandbox and rerun without errors on the exact final 45 sanitized fixtures. Generic passthrough used one persistent server, `ctx_execute_file`, the same category-level intent as PIRA, and JavaScript that printed each fixture while preserving its recorded exit status. Its direct Node emitter produced the same bytes and exit status as its raw baseline, so Docker startup and server initialization are excluded from overhead. It returned 71,621 bytes, classified all 45 statuses correctly, and immediately exposed 9/13 labeled outcomes: 7/8 failure markers and 2/5 changed basenames.

`ctx_index` used a separate persistent server and returned 7,843 bytes of indexing receipts. It exposed none of the 13 labels immediately, while exact content remained available through later search. Indexing has no equivalent raw operation, so no synthetic latency overhead is reported. Both Context Mode storage figures include its SQLite FTS5 retrieval state after shutdown; installed packages are excluded.

Generic passthrough is the closest automatic wrapper-level comparison, not Context Mode's recommended workflow. Context Mode normally asks the model to run task-specific analysis code and return only the derived answer. Its [published benchmark](https://github.com/mksglu/context-mode/blob/main/BENCHMARK.md) reports 98% reduction for task-specific execution, 82% for exact index-plus-search retrieval, and 96% overall. Returned-context measurements here count UTF-8 bytes rather than tokenizer-specific tokens, and immediate visibility does not measure evidence recoverable by later search.

#### Limitations

This remains a private implementation benchmark on one arm64 macOS evaluation host, not a universal performance claim. The remote suite is genuinely unseen and post-freeze imported, but its logs predate the freeze and are therefore not prospective outputs. Setup/install and static-analysis coverage remains below the five-case cap because no more eligible unique remote responses were available. Failure markers measure visibility of broad outcome evidence rather than complete diagnostic usefulness. arXiv selection required baseline build availability and includes one intentionally mutated source. Privacy sanitation changes path separators in LaTeX logs. Binary, non-UTF-8, and interactive-terminal behavior are covered by functional tests rather than this corpus. Web-search returns remain excluded because Codex built-in web output is not directly captured by the local command wrapper.

</details>

</details>

## Optional Codex audio notifications

<details>
<summary>Behavior, customization, and manual installation</summary>

Audio notifications are optional and are supported only for **Codex on macOS or Windows**. They are off by default and should not be presented as supported for Claude Code, other agent tools, Linux, or other systems.

When enabled, PIRA can play:

- `complete_msg.m4a` when the direct user-facing Codex agent finishes a turn; and
- `waiting_msg.m4a` when the direct user-facing Codex agent needs confirmation, approval, or another user action.

Startup audio is no longer installed. The helpers remove legacy PIRA-managed startup wrappers when found.

Focus detection is best-effort. On macOS, the helper checks the frontmost app with `osascript`; on Windows, it checks the foreground window process with built-in PowerShell/.NET calls. If a known terminal or editor is focused, including VS Code-like integrated-terminal hosts, the helper stays quiet. Subagent turns are suppressed by detecting Codex session metadata.

The default audio set lives in `~/agent/PIRA_Voice/Samantha`. A custom audio set is any folder containing:

```text
complete_msg.m4a
waiting_msg.m4a
```

For customization guidance, postprocessing steps, and ready-to-paste prompts for PIRA, see `~/agent/assets/AUDIO_CUSTOMIZATION_GUIDE.md`.

### Install audio manually

Prefer `assets/scripts/setup_pira.* --audio yes` when installing PIRA. If you only want to configure audio, use the dedicated helpers.

macOS:

```bash
bash ~/agent/assets/scripts/setup_codex_audio_mode.sh \
  --config ~/.codex/config.toml
```

Windows PowerShell:

```powershell
powershell.exe -ExecutionPolicy Bypass -File "$HOME\agent\assets\scripts\setup_codex_audio_mode_windows.ps1" `
  -ConfigPath "$HOME\.codex\config.toml"
```

Use `--audio-dir PATH` on macOS or `-AudioDir PATH` on Windows for a custom audio set. Restart Codex after installing or changing audio mode.

If `config.toml` already has a top-level `notify` entry, inspect it first and rerun the relevant helper with `--force` on macOS or `-Force` on Windows only after confirming it is acceptable to replace.

Keep `notify` at the top level of `config.toml`, before any `[section]` table, so it is not accidentally parsed as part of a nested table.

</details>

## What PIRA is for

PIRA is meant to help with work that benefits from both care and rigor:

- research planning and evidence-based analysis;
- scientific writing and paper polishing;
- coding, debugging, and repository work;
- learning and explanation;
- practical day-to-day guidance.

## Core principles

PIRA is built around a few simple commitments:

- **Be useful.** Prefer concrete next steps over vague advice.
- **Be honest.** Do not fabricate claims, citations, or results.
- **Be evidence-first.** Use primary sources when facts matter.
- **Be transparent.** Separate observation from interpretation and state uncertainty clearly.
- **Be kind.** Stay supportive, collaborative, and respectful.

## Why this design

PIRA is intentionally minimal:

- **Inspectable.** Behavior is organized in readable policy and module files that are easy to review and customize.
- **Lightweight.** Token overhead stays low; there is no heavy framework or rarely used abstraction layer.
- **Research-oriented.** Default workflows emphasize reasoning, writing, coding, evidence gathering, and careful iteration.
- **Lean by default.** Drawing on [Ponytail](https://github.com/DietrichGebert/ponytail) and general lessons from *Clean Code* and *Clean Architecture*, the coding style favors deletion, standard-library or platform features, the smallest safe implementation, readable names, and clear boundaries over speculative abstractions.
- **Tool-friendly.** The small, explicit design integrates naturally with official tools such as Codex.

## Safety model

<details>
<summary>Permission boundaries and operating rules</summary>

PIRA can run in soft-safe full-permission mode, but it is not a sandbox. Its safety depends on explicit operating rules in `TOOLS.md`, including:

- before any command that may write or change state, print a brief safety review covering action, scope, destructive risk, secrets/privacy impact, and rollback path when available;
- prefer narrow, reversible actions;
- avoid destructive commands without explicit permission;
- keep temporary artifacts in the platform temp directory unless the user wants them preserved.

Subagents should load the same bootstrap policy as the main agent. This is handled by Codex but has not been tested on other agents. On Claude Code, user memory including the PIRA imports reaches most subagents, but the built-in Explore and Plan agents skip memory files by design.

</details>

## Repository layout

<details>
<summary>Source, policy, setup, and bundled-tool files</summary>

- `AGENTS.md` — bootstrap instructions and module routing policy
- `SOUL.md` — PI's identity, tone, and non-negotiable behaviors
- `TOOLS.md` — tool-use and safety rules
- `USER.md` — user-specific knowledge and working preferences; keep this private
- `modules/` — optional task-specific modules for research, coding, writing, learning, guidance, and maintenance
- `assets/scripts/` — setup and helper scripts
- `tools/build/build_pira_ctx_platform_bins.py` — pinned, reproducibility-checking multi-platform release builder
- `tools/src/pira_ctx/` — public Rust implementation of `pira_ctx`; future tools use separate source directories
- `tools/dist/pira_ctx/` — verified prebuilt `pira_ctx` executables and bundle manifest
- `PIRA_Voice/Samantha/` — default audio clips for optional Codex notifications

</details>

## Public/private split

<details>
<summary>What belongs in the public repository and what stays local</summary>

The public repository contains the shared policy framework. Personal context should stay local:

- keep `USER.md` private;
- keep workspace-specific memory in local `AGENT_WORKBOOK.md` files;
- do not commit secrets or sensitive personal information.

</details>

## Why the name PIRA

PIRA stands for PI Research Assistant, giving PI a clear public-facing project name.

## Acknowledgement and citation

If PIRA materially assists a research project, disclose that assistance where appropriate, such as in an acknowledgement, LLM-use disclosure, or reproducibility checklist, and cite this repository. Adapt the scope of assistance to what was actually used, and include the actual model/version or reasoning setting if your venue asks for that level of detail.

Suggested disclosure text:

> This paper was assisted by PIRA~\citep{pira}, a research-assistant agent powered by {the model used, such as GPT-5.5}. The assistance included [brainstorming / implementation assistance / writing polish / ...]. The authors are fully responsible for the final content.

Suggested BibTeX entry:

```bibtex
@misc{pira,
  author = {{PIRA Project}},
  title = {{PIRA}: {PI} Research Assistant},
  year = {2026},
  howpublished = {\url{https://github.com/AlgebraLoveme/PIRA}}
}
```

PIRA should be acknowledged as tool assistance, not as scientific authorship.

## License

PIRA is available under the [Apache License 2.0](LICENSE).
