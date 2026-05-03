# PIRA — PI Research Assistant

PIRA is the public-facing name of PI, a personal agent for research, writing, coding, learning, and practical problem-solving.
It is designed to be warm, honest about uncertainty, and evidence-first when evidence matters.

## Get started

1. Clone the repository:
   ```bash
   git clone https://github.com/AlgebraLoveme/PIRA.git ~/agent
   ```
2. Open your agent or coding tool, such as Codex or Claude Code.
3. Paste the prompt below and let the agent finish the setup.

### Prompt to paste into your agent

```text
I cloned a repository containing an agent policy folder. Please set up this agent automatically. Initialize USER yourself; do not ask me to pre-create files.

Requirements:
1. Detect the repository directory.
2. Ensure it is `~/agent`; otherwise create a symlink `~/agent -> <detected-folder>`.
3. Initialize `~/agent/USER.md` automatically.
   - If missing, ask for either public profile URL(s), a brief self-description, or permission to create an empty placeholder.
   - Generate or update `USER.md` from `~/agent/assets/USER_TEMPLATE.md`.
   - Preserve stable collaboration preferences that materially affect work.
4. If legacy files listed in `~/agent/assets/LEGACY_LIST.md` exist, delete them during setup.
   - Before deletion, preserve any still-needed durable policy content in the proper tracked files; do not keep obsolete schemes alive.
5. Tell the user the agent's preferred name is `PI`, and briefly explain why.
6. Explain the execution-mode options clearly, then ask the user to confirm which mode to use before writing platform config:
   - Safe approval mode: ask for approval for non-trusted or destructive actions by default.
   - Soft-safe mode (recommended for practical utility, but warn the user clearly): default to no per-command approval prompts, rely on PIRA's safety rules, and ask the user only when PIRA judges it necessary. Make clear that this does not provide hard protection, so it should be used with caution.
7. Configure the platform so `~/agent/AGENTS.md` is automatically loaded at the start of every session.
8. Keep existing policy text unchanged unless compatibility requires edits.
9. Verify setup and report exactly what changed, including verification-token consistency.
10. After setup, ask whether to run the optional comprehensive module-loading check in `~/agent/assets/MODULE_LOADING_CASES.md`; do not run it by default.

Verification checklist:
- Confirm `~/agent/AGENTS.md` exists.
- Confirm global config points to `~/agent/AGENTS.md`.
- If `~/agent/USER.md` exists, confirm it is loaded as mandatory context.
- Confirm the policy supports per-workspace `AGENT_WORKBOOK.md` memory, does not require a global memory file for startup behavior, and removes files listed in `~/agent/assets/LEGACY_LIST.md` if found.
- Start or describe a fresh-session check with only mandatory modules and confirm no load acknowledgement is printed.
- In that fresh session, ask for the verification token and confirm it exactly matches `SOUL.md` (`31415926535897932384626433832795`).
- Ask whether to run the optional comprehensive module-loading check in `~/agent/assets/MODULE_LOADING_CASES.md`. Run it only if the user says yes, then confirm the prompts report the correct optional instruction file paths under the current routing policy.

For Codex specifically:
- Update or create `~/.codex/config.toml` with:
  - `model_instructions_file = "~/agent/AGENTS.md"`
  - `project_doc_max_bytes = 65536` (keep or set)
  - if the user chose Safe approval mode, set a conservative default approval policy/sandbox combination.
  - if the user chose Soft-safe mode, set `approval_policy = "never"` and `sandbox_mode = "danger-full-access"`.
- Ensure `~/.codex/AGENTS.md` also points to `~/agent/AGENTS.md`.

Output format:
- Changed files (absolute paths)
- Diff summary
- Verification results
- Any remaining manual step (if unavoidable)
```

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

> This paper was assisted by PIRA~\citep{pira}, a research-assistant system powered by {concrete model series, such as GPT 5.5}. The assistance included [brainstorming / implementation assistance / writing polish / ...]. The authors are fully responsible for the presented final content.

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
