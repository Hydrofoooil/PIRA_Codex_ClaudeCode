# PIRA — PI Research Assistant

PIRA (pronounced "Pyra") is the public-facing name of PI, a personal agent for research, writing, coding, learning, and practical problem-solving.
It is designed to be warm, honest about uncertainty, and evidence-first when evidence matters.

## Get started

PIRA's default setup is a global machine install centered on `~/agent`. The setup script is idempotent, backs up user-level Codex files before editing them, and can be run in dry-run or verification-only mode.

1. Clone the repository:
   ```bash
   git clone https://github.com/AlgebraLoveme/PIRA.git ~/agent
   cd ~/agent
   ```
2. Preview the planned changes:
   ```bash
   assets/scripts/setup_pira.sh --dry-run
   ```
3. Run setup:
   ```bash
   assets/scripts/setup_pira.sh
   ```
4. Verify later without writing:
   ```bash
   assets/scripts/setup_pira.sh --verify
   ```

On Windows, use `powershell.exe -ExecutionPolicy Bypass -File assets/scripts/setup_pira.ps1` from the repository directory. On macOS/Linux, use `assets/scripts/setup_pira.sh`. The wrappers share the Python bootstrap helpers in `assets/scripts/lib/`; setup can offer to install Python with Homebrew on macOS or winget on Windows.

### Setup choices

The script asks before potentially sensitive choices in interactive mode. For unattended setup, pass explicit flags.

Execution mode:
- `--execution-mode safe` sets `approval_policy = "on-request"` and `sandbox_mode = "workspace-write"`.
- `--execution-mode soft-safe` sets `approval_policy = "never"` and `sandbox_mode = "danger-full-access"`; this is convenient but not a sandbox.
- `--execution-mode keep` leaves existing approval and sandbox settings unchanged.

Other useful options:
- `--yes` accepts setup confirmations, but does not enable audio unless `--audio yes` is also set.
- `--audio yes|no|ask` controls optional Codex audio notifications.
- `--user-mode placeholder|interactive|keep` controls `USER.md` initialization.
- `--global-agents link|copy|skip|ask` controls whether `~/.codex/AGENTS.md` points to PIRA by symlink, copy, or not at all.
- `--legacy remove|keep|ask` controls files listed in `assets/LEGACY_LIST.md`.
- `--agent-dir PATH` installs against a path other than `~/agent`.

Example non-interactive soft-safe setup without audio:

```bash
assets/scripts/setup_pira.sh \
  --yes \
  --execution-mode soft-safe \
  --audio no \
  --global-agents link \
  --legacy remove
```

### What setup configures

The script:
1. Detects the repository directory and ensures it is available as `~/agent` unless another `--agent-dir` is given.
2. Initializes a private `USER.md` placeholder when needed.
3. Removes legacy files listed in `assets/LEGACY_LIST.md` when approved.
4. Updates or creates Codex `config.toml` so the selected agent directory's `AGENTS.md` is loaded, with `project_doc_max_bytes = 65536`.
5. Optionally links or copies `~/.codex/AGENTS.md` for Codex's global AGENTS discovery path.
6. Optionally delegates audio setup to the existing platform-specific audio helper.
7. Verifies the setup, including the PIRA verification token.

If the script cannot safely handle an existing conflicting file or Codex setting, it stops or skips that action with a warning instead of silently overwriting it.

## Optional Codex audio notifications

This audio notification guide is only for **Codex running on macOS or Windows**. It should not be presented as supported for Claude Code, other agent tools, Linux, or other systems.

During installation, the setup script should ask whether to enable audio notification mode only when the detected platform is Codex on macOS or Windows. This is optional and should remain off unless the user explicitly opts in.

Behavior:
- for the direct user-facing Codex agent only, play `complete_msg.m4a` when a turn completes normally and Codex does not appear to be the focused app;
- for the direct user-facing Codex agent only, play `waiting_msg.m4a` when Codex needs user confirmation, approval, or another user action and Codex does not appear to be focused.

Startup audio is no longer installed. The helpers remove legacy PIRA-managed startup wrappers when found.

Focus detection is best-effort. On macOS the helper checks the frontmost app with `osascript`; on Windows it checks the foreground window process with built-in PowerShell/.NET calls. If the frontmost app is a known terminal or editor, including VS Code-like integrated-terminal hosts, the helper assumes the user may already be looking at Codex and stays quiet. Subagent turns are suppressed by detecting Codex session metadata, so delegated agents do not produce completion or waiting audio.

The default audio set lives in `~/agent/PIRA_Voice/Samantha`. A custom audio set is any folder with these two files:

```text
complete_msg.m4a
waiting_msg.m4a
```

For detailed customization, audio postprocessing steps, and ready-to-paste prompts for PIRA, see `~/agent/assets/AUDIO_CUSTOMIZATION_GUIDE.md`.

The setup helpers preserve existing `notify` or hook configuration when possible and back up `~/.codex/config.toml` before editing it.

Use the repository helper scripts rather than reconstructing the setup manually. For macOS:

```bash
bash ~/agent/assets/scripts/setup_codex_audio_mode.sh \
  --config ~/.codex/config.toml
```

For Windows PowerShell:

```powershell
powershell.exe -ExecutionPolicy Bypass -File "$HOME\agent\assets\scripts\setup_codex_audio_mode_windows.ps1" `
  -ConfigPath "$HOME\.codex\config.toml"
```

Use `--audio-dir PATH` on macOS or `-AudioDir PATH` on Windows for a custom audio set. Restart Codex after installing or changing audio mode.

If `config.toml` already has a top-level `notify` entry, inspect it first and rerun the relevant helper with `--force` on macOS or `-Force` on Windows only after confirming it is acceptable to replace.

Keep `notify` at the top level of `config.toml`, before any `[section]` table, so it is not accidentally parsed as part of a nested table. After changing Codex config, restart Codex to load the new notification settings. The macOS helper uses `afplay`; the Windows helper uses Windows media playback from PowerShell.

## What PIRA is for

PIRA is meant to help with work that benefits from both care and rigor, including:
- research planning and evidence-based analysis
- scientific writing and paper polishing
- coding, debugging, and repository work
- learning and explanation
- practical day-to-day guidance

## Core principles

PIRA is built around a few simple commitments:
- **Be useful.** Prefer concrete next steps over vague advice.
- **Be honest.** Do not fabricate claims, citations, or results.
- **Be evidence-first.** Use primary sources when facts matter.
- **Be transparent.** Separate observation from interpretation and state uncertainty clearly.
- **Be kind.** Stay supportive, collaborative, and respectful.

## Why this design

PIRA is intentionally minimal by design.

- **Plain-text controlled.** Its behavior is defined in readable Markdown files, so it is easy to inspect, edit, and customize.
- **Lightweight.** It keeps token overhead low instead of relying on a heavy framework or many layers of rarely used abstractions.
- **Research-oriented.** It focuses on the workflows that matter most in research: reasoning, writing, coding, evidence gathering, and careful iteration.
- **Practical.** It avoids complex features that are impressive in principle but often unnecessary in everyday research use.
- **Lean by default.** Its coding style incorporates useful minimalism principles from [Ponytail](https://github.com/DietrichGebert/ponytail): prefer deletion, standard-library or platform features, and the smallest safe implementation over speculative code.
- **Tool-friendly.** Because the system is simple and text-based, it works naturally with official tools such as Codex.

## Safety model

PIRA can be used in a soft-safe full-permission mode, but it is not a sandbox. Its safety depends on explicit operating rules in `TOOLS.md`, including:

- before any command that may write or change state, print a brief safety review covering action, scope, destructive risk, secrets/privacy impact, and rollback path when available;
- prefer narrow, reversible actions;
- avoid destructive commands without explicit permission;
- keep temporary artifacts in the platform temp directory unless the user wants them preserved.

Subagents should load the same bootstrap policy as the main agent (automatically handled by Codex but not tested on other agents).

## Tested compatibility

PIRA has been tested extensively with **Codex using GPT-5.4/5.5 on high reasoning effort**.

## What is in this repository

- `AGENTS.md` — the bootstrap instructions and module routing policy
- `SOUL.md` — PI's identity, tone, and non-negotiable behaviors
- `TOOLS.md` — tool-use and safety rules
- `USER.md` — user-specific knowledge and working preferences
- `modules/` — optional task-specific modules such as research, coding, writing, learning, guidance, and maintenance
- `PIRA_Voice/Samantha/` — default audio clips for optional Codex notifications

## How to use it

This repository is intended to live at `~/agent` and be loaded automatically at the start of each session by your coding or agent tool.

The setup philosophy is intentionally simple: keep the system text-first, keep personal context local, and rely on small task-specific modules instead of a heavy framework.

## Public/private split

The public repository contains the shared policy framework.
Personal context should stay local:
- `USER.md` should remain private
- each workspace can keep a local `AGENT_WORKBOOK.md`

## Why the name PIRA

PIRA stands for PI Research Assistant. It keeps the identity of PI while giving the project a clearer and more public-facing name.

## Acknowledgement and citation

If PIRA materially assists a research project, please disclose that assistance where appropriate, such as in an acknowledgement, LLM-use disclosure, or reproducibility checklist, and cite this repository. Adapt the scope of assistance to what was actually used, and include the actual model/version or reasoning setting if your venue asks for that level of detail.

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
