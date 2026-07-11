# PIRA — PI Research Assistant

PIRA (pronounced "Pyra") is the public-facing name of PI: a plain-text, research-oriented personal agent for reasoning, writing, coding, learning, and practical problem-solving.

PIRA is designed to be warm, honest about uncertainty, evidence-first when evidence matters, and lightweight enough to inspect and customize.

## Tested compatibility

PIRA has been tested extensively with **Codex using GPT-5.4, GPT-5.5, and 5.6-sol, each with high reasoning effort**.

## Quick start

PIRA's default install lives at `~/agent`. The setup script is idempotent, backs up user-level Codex files before editing them, supports dry-run/verify modes, and is safe to rerun on an existing install. You need Git; the setup wrapper handles Python discovery and can offer platform-specific Python install help.

### Recommended one-line install or update

This command:
- uses the existing `~/agent` git checkout when present, otherwise clones PIRA into `~/agent`;
- enables **soft-safe** mode;
- keeps audio notifications **off**;
- links PIRA into Codex;
- moves old PIRA-managed legacy files into backup;
- creates a private `USER.md` placeholder only if `USER.md` is missing.

macOS/Linux:

```bash
if [ -d ~/agent/.git ]; then cd ~/agent && git pull --ff-only; else git clone https://github.com/AlgebraLoveme/PIRA.git ~/agent && cd ~/agent; fi && assets/scripts/setup_pira.sh --yes --execution-mode soft-safe --audio no --user-mode placeholder --global-agents link --legacy remove
```

Windows PowerShell:

```powershell
if (Test-Path "$HOME/agent/.git") { Set-Location "$HOME/agent"; git pull --ff-only; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE } } else { git clone https://github.com/AlgebraLoveme/PIRA.git "$HOME/agent"; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }; Set-Location "$HOME/agent" }; powershell.exe -ExecutionPolicy Bypass -File assets/scripts/setup_pira.ps1 --yes --execution-mode soft-safe --audio no --user-mode placeholder --global-agents link --legacy remove
```

If you are rerunning setup and want a missing `USER.md` to stay missing, use `--user-mode keep` instead.

macOS/Linux:

```bash
cd ~/agent && git pull --ff-only && assets/scripts/setup_pira.sh --yes --execution-mode soft-safe --audio no --user-mode keep --global-agents link --legacy remove
```

Windows PowerShell:

```powershell
Set-Location "$HOME/agent"; git pull --ff-only; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }; powershell.exe -ExecutionPolicy Bypass -File assets/scripts/setup_pira.ps1 --yes --execution-mode soft-safe --audio no --user-mode keep --global-agents link --legacy remove
```

`git pull --ff-only` updates an existing checkout only when Git can do so without a merge. If you have tracked local edits or a divergent branch, it stops for manual review.

> **Soft-safe is not a sandbox.** It sets Codex to no-approval/full-permission mode and relies on PIRA's explicit safety rules before state-changing commands.

### Inspect-first install

Use this path if you want to preview setup before writing anything:

```bash
git clone https://github.com/AlgebraLoveme/PIRA.git ~/agent
cd ~/agent
assets/scripts/setup_pira.sh --dry-run
assets/scripts/setup_pira.sh
assets/scripts/setup_pira.sh --verify
```

On Windows, run the same setup through `assets/scripts/setup_pira.ps1` from the repository directory:

```powershell
powershell.exe -ExecutionPolicy Bypass -File assets/scripts/setup_pira.ps1
```

Both platform wrappers forward the same options to `assets/scripts/setup_pira.py`. They share the Python bootstrap helpers in `assets/scripts/lib/`; setup can offer to install Python with Homebrew on macOS or winget on Windows.

## Setup options

The script asks before sensitive choices in interactive mode. For unattended setup, pass explicit flags.

### Execution mode

| Option | Codex settings | Use when |
| --- | --- | --- |
| `--execution-mode safe` | `approval_policy = "on-request"`, `sandbox_mode = "workspace-write"` | You want a real approval/sandbox boundary. |
| `--execution-mode soft-safe` | `approval_policy = "never"`, `sandbox_mode = "danger-full-access"` | You want convenience and accept full-permission risk. |
| `--execution-mode keep` | Leaves existing approval/sandbox settings unchanged. | You already manage Codex permissions yourself. |

### `USER.md` mode

| Option | Behavior |
| --- | --- |
| `--user-mode placeholder` | Creates a private placeholder `USER.md` when it is missing. Existing `USER.md` is preserved. |
| `--user-mode keep` | Leaves `USER.md` exactly as-is; if it is missing, setup leaves it missing. |
| `--user-mode interactive` | Asks what to do when `USER.md` is missing. |

### Other useful flags

| Option | Behavior |
| --- | --- |
| `--yes` | Accepts setup confirmations. It does **not** enable audio unless `--audio yes` is also set. |
| `--audio yes\|no\|ask` | Controls optional Codex audio notifications. Use `--audio no` for a quiet install. |
| `--global-agents link\|copy\|skip\|ask` | Controls whether `~/.codex/AGENTS.md` points to PIRA by symlink, copy, or not at all. |
| `--legacy remove\|keep\|ask` | Controls paths listed in `assets/LEGACY_LIST.md`; `remove` moves active legacy files into `.backup/setup_pira_legacy/`. |
| `--agent-dir PATH` | Installs against a path other than `~/agent`. |
| `--skip-tools` | Skips installation or refresh of bundled native PIRA tools. |
| `--tools-install-dir PATH` | Overrides the per-user tools directory (`~/.local/bin` on macOS/Linux or `%LOCALAPPDATA%\PIRA\bin` on Windows). |
| `--verify` | Checks the current setup without writing. |
| `--dry-run` | Prints planned changes without applying them. |

## What setup changes

The setup script:

1. Detects the repository directory and ensures it is available as `~/agent`, unless another `--agent-dir` is given.
2. Initializes a private `USER.md` placeholder when needed.
3. Moves legacy files listed in `assets/LEGACY_LIST.md` into `.backup/setup_pira_legacy/` when approved.
4. Updates or creates Codex `config.toml` so the selected agent directory's `AGENTS.md` is loaded, with `project_doc_max_bytes = 65536`.
5. Optionally links or copies `~/.codex/AGENTS.md` for Codex's global AGENTS discovery path.
6. Selects and verifies the bundled native tool for the current platform, then installs or refreshes it in a per-user PATH directory. Existing stale copies are atomically replaced; matching copies are left unchanged.
7. Optionally delegates audio setup to the platform-specific audio helper.
8. Verifies the setup, including the PIRA verification token and installed native tool.

Tool setup can also be run independently:

```bash
python3 assets/scripts/setup_pira_tools.py
python3 assets/scripts/setup_pira_tools.py --verify
```

If setup cannot safely handle an existing conflicting file or Codex setting, it stops or skips that action with a warning instead of silently overwriting it.

## `pira_ctx`: lightweight command context

`pira_ctx` keeps large command output from overwhelming the model context without discarding the original data. Its default behavior is simple:

1. Ordinary short output is returned directly.
2. Long or diagnostic output is stored locally, while the model receives a bounded extractive synopsis and a capture ID.
3. If more detail is needed, the complete capture remains available for targeted search, line-range retrieval, transformation, or exact replay.

For compile, test, lint, or other validation jobs where only success or failure matters, `check` stores the complete log but prints one PASS/FAIL status line with the child exit code and capture ID.

Explicit `exact` mode streams unchanged when attached to a terminal. In non-interactive agent calls it buffers the result so that long, highly repetitive logs can be auto-switched to a retained summary instead of flooding context; genuinely varied output remains exact, and every switched response announces the decision.

Setup installs a verified native executable in the user's `PATH`. Normal use requires no Python, Rust toolchain, daemon, database, network service, or model call. Captures are private user-cache files with independently compressed blocks and integrity hashes. `pira_ctx` preserves the caller's permissions and does not sandbox commands. Run `pira_ctx --help` for the complete interface. The Rust source is under `tools/src`, and verified builds for macOS arm64/x64, Linux arm64/x64, and Windows x64 are under `tools/dist/pira_ctx`.

### Prospective held-out benchmark

The current `pira_ctx 0.5.2` source and release artifacts were frozen before collecting **43 new outputs from ten fixed public repositories**. The corpus was collected, sanitized, deduplicated against earlier private corpora, and evaluated once without subsequent implementation or threshold tuning. Every retained response was at least 2 KiB and entered full automatic-summary mode.

| Mode on the same 3,436,926 raw bytes | Returned context | Complete stored state | Median overhead | Immediate changed-file visibility |
|---|---:|---:|---:|---:|
| `pira_ctx` automatic synopsis | 40,272 B (98.8% reduction) | 724,318 B (78.9% reduction) | +15.0 ms | 1/8 |
| Context Mode generic passthrough | 44,835 B (98.7% reduction) | 23,437,772 B (581.9% overhead) | +26.5 ms | 3/8 |
| `pira_ctx check` | 2,924 B (99.9% reduction) | 724,447 B (78.9% reduction) | +14.0 ms | N/A—status only |
| Context Mode `ctx_index` receipt | 7,323 B (99.8% reduction) | 18,436,546 B (436.4% overhead) | N/A—no corresponding raw baseline | 0/8 |

All 43 PIRA captures preserved child status, reconstructed the exact sanitized stdout, and passed integrity verification. Automatic suggestions correctly abstained in 35/35 unlabeled cases, but changed-filename recall was only 1/8. This is a current generalization weakness, deliberately reported without tuning it against the held-out corpus.

<details>
<summary>Benchmark method, category results, comparison scope, and limitations</summary>

#### Corpus and PIRA protocol

The five categories use recent public VCS patches, largest tracked Rust files, recursive declaration listings, 40-commit terminal logs, and GitHub pull-list responses. Candidate outputs were generated only after the binary and source freeze. Fixed-point sanitation removed private values and paths; exact and structural duplicates against 226 earlier private cases were rejected. Public changed basenames were preserved as explicit sanitized diff metadata so the predeclared suggestion labels remained observable.

A preceding candidate collection was not reported after audit found that sanitation had replaced every changed-filename label with a generic placeholder. The implementation was not changed after seeing that invalid measurement. The final protocol fixed the label-preservation rule and repository list before collecting an entirely new corpus.

Automatic and `check` modes used one persistent store each. Every call was compared with the identical raw fixture-emitter command. Overhead is `wrapped wall time - raw-operation wall time`, summarized by the per-case median. Complete stored state includes captures, indexes, and event history after all calls; installed binaries and build/runtime dependencies are excluded.

| Held-out category | Cases | Changed-basename recall | Correct abstentions | Context reduction |
|---|---:|---:|---:|---:|
| File reads | 9 | — | 9/9 | 98.8% |
| GitHub pull retrieval | 8 | — | 8/8 | 99.4% |
| Search and listing | 9 | — | 9/9 | 99.0% |
| Terminal logs | 9 | — | 9/9 | 95.7% |
| Version-control diffs | 8 | 1/8 | — | 90.3% |
| **Overall** | **43** | **1/8** | **35/35** | **98.8%** |

#### Context Mode comparison

Context Mode 1.0.169 was installed inside the retained Docker Sandbox and run without errors on the identical sanitized fixtures, with one persistent server per mode. Generic passthrough used `ctx_execute_file` to print each fixture unchanged and supplied the same category-level intent as PIRA. Its direct Node file emitter provided its own raw baseline, so Docker startup and server initialization are excluded from the overhead figure. `ctx_index` has no equivalent raw indexing operation, so no synthetic overhead is reported.

Generic passthrough is the closest wrapper-level comparison, but it is not Context Mode's recommended workflow. Context Mode normally asks the model to write task-specific analysis code and return only the derived answer. Its [published benchmark](https://github.com/mksglu/context-mode/blob/main/BENCHMARK.md) reports 98% reduction for task-specific execution, 82% for exact index-plus-search retrieval, and 96% overall. The table therefore compares concrete wrapper behaviors on one corpus, not the full capability or preferred workflow of either system.

Returned-context measurements count UTF-8 bytes, not tokenizer-specific tokens. Immediate visibility checks only whether a preserved changed basename appears in the first response; they do not measure evidence recoverable through later search. Context Mode storage includes its richer SQLite FTS5 retrieval indexes, and fixed database overhead is material on this 3.44 MB corpus.

#### Limitations

This is a private implementation benchmark on one arm64 macOS host, not a universal performance claim. It covers successful outputs only and contains no build, test, static-analysis, LaTeX, binary, non-UTF-8, or interactive-terminal cases. GitHub/API availability and conservative privacy filtering reduced category counts unevenly. Diff labels measure changed-basename recall, not complete diagnostic usefulness. Web-search returns are intentionally excluded because Codex built-in web output is not directly captured by the local command wrapper.

</details>

### Relationship to Context Mode

`pira_ctx` was informed by [Context Mode](https://github.com/mksglu/context-mode), especially its ideas of keeping raw tool output out of context, attaching intent to execution, retrieving indexed evidence after compaction, and analyzing stored output with small programs. We thank its contributors for publishing and explaining these ideas.

| Dimension | `pira_ctx` | Context Mode |
|---|---|---|
| Integration | Native wrapper for explicit external commands | MCP server plus platform plugins and hooks |
| Runtime and storage | One Rust executable and self-contained checked capture files | Node/Bun integration with a SQLite FTS5 knowledge base |
| Reach | Commands deliberately routed through the wrapper | Broader shell, file, web, and MCP routing where integrations support it |
| Continuity | Bounded same-session recap after compaction | Explicit session lifecycle and continuation support |
| Safety scope | Preserves caller permissions; does not sandbox children | Adds sandbox and permission-policy integration |

PIRA uses `pira_ctx` when a small dependency-free command wrapper and exact local fallback are preferable. Context Mode is the more comprehensive option when broader interception, hooks, sandboxing, or database-backed retrieval are needed.

## Optional Codex audio notifications

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

- **Plain-text controlled.** Behavior is defined in readable Markdown files, so it is easy to inspect, edit, and customize.
- **Lightweight.** Token overhead stays low; there is no heavy framework or rarely used abstraction layer.
- **Research-oriented.** The default workflows emphasize reasoning, writing, coding, evidence gathering, and careful iteration.
- **Lean by default.** The coding style incorporates useful minimalism principles from [Ponytail](https://github.com/DietrichGebert/ponytail) and general, non-language-specific lessons from Robert C. Martin's *Clean Code* and *Clean Architecture*: prefer deletion, standard-library or platform features, the smallest safe implementation, readable names, behavior-preserving refactors, and clear boundaries over speculative code or architecture ceremony.
- **Tool-friendly.** Because the system is simple and text-based, it works naturally with official tools such as Codex.

## Safety model

PIRA can run in soft-safe full-permission mode, but it is not a sandbox. Its safety depends on explicit operating rules in `TOOLS.md`, including:

- before any command that may write or change state, print a brief safety review covering action, scope, destructive risk, secrets/privacy impact, and rollback path when available;
- prefer narrow, reversible actions;
- avoid destructive commands without explicit permission;
- keep temporary artifacts in the platform temp directory unless the user wants them preserved.

Subagents should load the same bootstrap policy as the main agent. This is handled by Codex but has not been tested on other agents.

## Repository layout

- `AGENTS.md` — bootstrap instructions and module routing policy
- `SOUL.md` — PI's identity, tone, and non-negotiable behaviors
- `TOOLS.md` — tool-use and safety rules
- `USER.md` — user-specific knowledge and working preferences; keep this private
- `modules/` — optional task-specific modules for research, coding, writing, learning, guidance, and maintenance
- `assets/scripts/` — setup and helper scripts
- `tools/src/` — public Rust implementation of `pira_ctx`
- `tools/dist/pira_ctx/` — verified prebuilt `pira_ctx` executables and bundle manifest
- `PIRA_Voice/Samantha/` — default audio clips for optional Codex notifications

## Public/private split

The public repository contains the shared policy framework. Personal context should stay local:

- keep `USER.md` private;
- keep workspace-specific memory in local `AGENT_WORKBOOK.md` files;
- do not commit secrets or sensitive personal information.

## Why the name PIRA

PIRA stands for PI Research Assistant. It preserves the identity of PI while giving the project a clearer public-facing name.

## Acknowledgement and citation

If PIRA materially assists a research project, disclose that assistance where appropriate, such as in an acknowledgement, LLM-use disclosure, or reproducibility checklist, and cite this repository. Adapt the scope of assistance to what was actually used, and include the actual model/version or reasoning setting if your venue asks for that level of detail.

Suggested disclosure text:

> This paper was assisted by PIRA~\citep{pira}, a research-assistant agent powered by {concrete model series, such as GPT 5.5}. The assistance included [brainstorming / implementation assistance / writing polish / ...]. The authors are fully responsible for the presented final content.

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
