#!/usr/bin/env python3
"""Deterministic setup helper for PIRA on Claude Code.

The script configures Claude Code to load the PIRA policy files every session
by maintaining a clearly marked, PIRA-managed import block inside the user
memory file ``~/.claude/CLAUDE.md``. Existing user content in that file is
preserved; only the marked block is created or replaced, and edits are backed
up first. Shared setup behavior (agent directory, USER.md, legacy files,
bundled tools) is reused from ``setup_pira.py``.

Codex audio notifications are not ported: Claude Code notification hooks are
not configured by this script.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import subprocess
import sys
from pathlib import Path
from types import ModuleType
from typing import Literal

SCRIPT_DIR = Path(__file__).resolve().parent
BLOCK_START = "<!-- >>> PIRA bootstrap >>> -->"
BLOCK_END = "<!-- <<< PIRA bootstrap <<< -->"
ALWAYS_LOADED_FILES = ["AGENTS.md", "SOUL.md", "TOOLS.md", "USER.md"]
EXECUTION_MODES: dict[str, str] = {"safe": "default", "soft-safe": "bypassPermissions"}


def load_common() -> ModuleType:
    """Load setup_pira.py as a module to reuse its setup helpers."""
    spec = importlib.util.spec_from_file_location("pira_setup_common", SCRIPT_DIR / "setup_pira.py")
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load shared setup helpers: {SCRIPT_DIR / 'setup_pira.py'}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def bootstrap_block_body(common: ModuleType, state, import_paths: list[Path]) -> str:
    lines = [
        "PIRA global agent bootstrap for Claude Code. The imports below load the",
        "always-on PIRA policy files every session. `AGENTS.md` routes the optional",
        "task modules, which are loaded on demand by reading the referenced module files.",
        "",
    ]
    lines.extend(f"@{common.config_path_string(path)}" for path in import_paths)
    return "\n".join(lines)


def managed_block_text(body: str) -> str:
    return f"{BLOCK_START}\n{body}\n{BLOCK_END}"


def upsert_managed_block(existing: str, body: str) -> str:
    block = managed_block_text(body)
    if BLOCK_START in existing:
        start = existing.index(BLOCK_START)
        end_marker = existing.find(BLOCK_END, start)
        if end_marker < 0:
            raise RuntimeError("incomplete PIRA bootstrap block in Claude Code memory file; fix or remove the markers manually")
        end = end_marker + len(BLOCK_END)
        prefix = existing[:start].rstrip()
        suffix = existing[end:].strip("\n")
        new = (prefix + "\n\n" if prefix else "") + block
        new += "\n\n" + suffix + "\n" if suffix else "\n"
        return new
    return existing.rstrip() + ("\n\n" if existing.strip() else "") + block + "\n"


def bootstrap_import_paths(common: ModuleType, state, *, warn_missing: bool) -> list[Path]:
    source_root = common.pira_source_root(state)
    paths: list[Path] = []
    for name in ALWAYS_LOADED_FILES:
        target = state.agent_dir / name
        if (source_root / name).exists() or target.exists():
            paths.append(target)
        elif warn_missing:
            state.warn(
                f"{name} is missing, so it is not imported; create it and rerun this setup to load it every session"
            )
    return paths


def configure_claude_memory(common: ModuleType, state, memory_path: Path) -> None:
    import_paths = bootstrap_import_paths(common, state, warn_missing=True)
    body = bootstrap_block_body(common, state, import_paths)
    existing = memory_path.read_text(encoding="utf-8") if memory_path.exists() else ""
    new_text = upsert_managed_block(existing, body)
    common.write_text(state, memory_path, new_text, "Claude Code user memory PIRA bootstrap block")


def configure_claude_settings(
    common: ModuleType,
    state,
    settings_path: Path,
    execution_mode: Literal["ask", "safe", "soft-safe", "keep"],
) -> None:
    if execution_mode == "ask":
        if state.yes or not sys.stdin.isatty():
            execution_mode = "keep"
            state.warn(
                "Execution mode left unchanged; pass --execution-mode safe or soft-safe to set it non-interactively"
            )
        else:
            print("Execution modes:")
            print('  1. safe: permissions.defaultMode="default" (Claude Code asks before sensitive actions)')
            print('  2. soft-safe: permissions.defaultMode="bypassPermissions" (no approval prompts; full-permission risk)')
            print("  3. keep: do not change Claude Code permission settings")
            choice = input("Choose execution mode [1/2/3, default 1]: ").strip()
            execution_mode = {"": "safe", "1": "safe", "2": "soft-safe", "3": "keep"}.get(choice, "safe")  # type: ignore[assignment]
    if execution_mode == "keep":
        print("OK: Claude Code permission settings left unchanged")
        return

    existing = settings_path.read_text(encoding="utf-8") if settings_path.exists() else ""
    if existing.strip():
        try:
            data = json.loads(existing)
        except json.JSONDecodeError as exc:
            state.warn(
                f"Could not parse {common.display_path(settings_path)} as JSON ({exc}); permission settings were not changed"
            )
            return
        if not isinstance(data, dict):
            state.warn(
                f"{common.display_path(settings_path)} does not contain a JSON object; permission settings were not changed"
            )
            return
    else:
        data = {}
    permissions = data.setdefault("permissions", {})
    if not isinstance(permissions, dict):
        state.warn(
            f'"permissions" in {common.display_path(settings_path)} is not an object; permission settings were not changed'
        )
        return
    permissions["defaultMode"] = EXECUTION_MODES[execution_mode]
    new_text = json.dumps(data, indent=2, ensure_ascii=False) + "\n"
    common.write_text(state, settings_path, new_text, "Claude Code settings.json permission mode")


def verify_claude(common: ModuleType, state, memory_path: Path) -> None:
    def add(name: str, passed: bool, detail: str) -> None:
        state.verification.append((name, passed, detail))
        label = "PASS" if passed else "FAIL"
        print(f"{label}: {name} — {detail}")

    if not memory_path.exists():
        add("Claude Code memory imports PIRA", False, f"{common.display_path(memory_path)} is missing")
        return
    text = memory_path.read_text(encoding="utf-8")
    if BLOCK_START not in text or BLOCK_END not in text:
        add("Claude Code memory imports PIRA", False, f"no PIRA bootstrap block in {common.display_path(memory_path)}")
        return
    expected = [f"@{common.config_path_string(path)}" for path in bootstrap_import_paths(common, state, warn_missing=False)]
    block = text[text.index(BLOCK_START): text.find(BLOCK_END) + len(BLOCK_END)]
    missing = [line for line in expected if line not in block.splitlines()]
    add(
        "Claude Code memory imports PIRA",
        not missing,
        f"{common.display_path(memory_path)} imports {', '.join(expected) or 'nothing'}"
        + (f"; missing: {', '.join(missing)}" if missing else ""),
    )


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Set up PIRA for Claude Code on the current machine.")
    parser.add_argument("--agent-dir", default="~/agent", help="Global PIRA path to configure (default: ~/agent).")
    parser.add_argument("--claude-dir", default="~/.claude", help="Claude Code user configuration directory.")
    parser.add_argument("--skip-claude", action="store_true", help="Do not edit Claude Code configuration.")
    parser.add_argument("--skip-tools", action="store_true", help="Do not install or refresh bundled PIRA tools.")
    parser.add_argument("--tools-install-dir", default=None, help="Override the per-user PIRA tools PATH directory.")
    parser.add_argument("--execution-mode", choices=["ask", "safe", "soft-safe", "keep"], default="ask")
    parser.add_argument("--user-mode", choices=["interactive", "placeholder", "keep"], default="interactive")
    parser.add_argument("--legacy", choices=["ask", "remove", "keep"], default="ask", help="How to handle paths listed in assets/LEGACY_LIST.md.")
    parser.add_argument("--force-agent-link", action="store_true", help="Move a conflicting --agent-dir aside and symlink this repo there.")
    parser.add_argument("--verify", action="store_true", help="Only verify the current setup; do not write.")
    parser.add_argument("--dry-run", action="store_true", help="Print planned changes without writing.")
    parser.add_argument("--yes", action="store_true", help="Assume yes for setup confirmations.")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    common = load_common()
    repo_root = Path(__file__).resolve().parents[2]
    state = common.SetupState(
        repo_root=repo_root,
        agent_dir=common.expand_path(args.agent_dir),
        dry_run=args.dry_run or args.verify,
        yes=args.yes,
    )
    claude_dir = common.expand_path(args.claude_dir)
    memory_path = claude_dir / "CLAUDE.md"
    settings_path = claude_dir / "settings.json"

    print("PIRA setup for Claude Code")
    print(f"Repository: {common.display_path(repo_root)}")
    print(f"Agent dir:  {common.display_path(state.agent_dir)}")
    print(f"Claude dir: {common.display_path(claude_dir)}")
    print(f"Dry run:    {state.dry_run}")

    try:
        if not args.verify:
            common.ensure_agent_dir(state, force_agent_link=args.force_agent_link)
            common.ensure_user_md(state, args.user_mode)
            common.remove_legacy_files(state, args.legacy)
            if not args.skip_claude:
                configure_claude_memory(common, state, memory_path)
                configure_claude_settings(common, state, settings_path, args.execution_mode)
            if not args.skip_tools:
                sys.stdout.flush()
                common.configure_tools(state, args.tools_install_dir, verify_only=False)
        if args.dry_run and not args.verify:
            print("DRY-RUN: verification skipped because planned changes were not applied")
        else:
            common.verify(state, settings_path, skip_codex=True)
            if not args.skip_claude:
                verify_claude(common, state, memory_path)
            if args.verify and not args.skip_tools:
                sys.stdout.flush()
                common.configure_tools(state, args.tools_install_dir, verify_only=True)
    except (RuntimeError, subprocess.CalledProcessError, OSError) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1

    print("\nSummary")
    if state.changed:
        for item in state.changed:
            print(f"- {item}")
    else:
        print("- No changes")
    if state.warnings:
        print("Warnings:")
        for item in state.warnings:
            print(f"- {item}")
    failed = [name for name, passed, _ in state.verification if not passed]
    if failed:
        print("Verification failed:")
        for item in failed:
            print(f"- {item}")
        return 1
    if state.verification:
        print("Verification passed.")
    else:
        print("Verification skipped.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
