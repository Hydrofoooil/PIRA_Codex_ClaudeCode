#!/usr/bin/env python3
"""Install or refresh bundled PIRA tools in a per-user PATH directory."""

from __future__ import annotations

import argparse
import ctypes
import hashlib
import importlib.util
import os
import shutil
import subprocess
import sys
from datetime import datetime
from pathlib import Path
from types import ModuleType

REPO_ROOT = Path(__file__).resolve().parents[2]
SELECTOR_PATH = REPO_ROOT / "tools" / "select_tool_for_platform.py"
BLOCK_START = "# >>> PIRA tools PATH >>>"
BLOCK_END = "# <<< PIRA tools PATH <<<"


def default_install_dir() -> Path:
    if os.name == "nt":
        root = os.environ.get("LOCALAPPDATA")
        if not root:
            raise RuntimeError("LOCALAPPDATA is unset; pass --install-dir")
        return Path(root) / "PIRA" / "bin"
    return Path.home() / ".local" / "bin"


def load_selector() -> ModuleType:
    spec = importlib.util.spec_from_file_location("pira_tool_selector", SELECTOR_PATH)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load selector: {SELECTOR_PATH}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def executable_path(directory: Path) -> Path:
    return directory / ("pira_ctx.exe" if os.name == "nt" else "pira_ctx")


def shell_profiles() -> list[Path]:
    shell = Path(os.environ.get("SHELL", "")).name
    if shell == "zsh" or sys.platform == "darwin":
        return [Path.home() / ".zprofile", Path.home() / ".zshrc"]
    if shell == "bash" and (Path.home() / ".bash_profile").exists():
        return [Path.home() / ".bash_profile", Path.home() / ".bashrc"]
    if shell == "bash":
        return [Path.home() / ".profile", Path.home() / ".bashrc"]
    return [Path.home() / ".profile"]


def shell_path_line(directory: Path) -> str:
    home = Path.home()
    try:
        relative = directory.relative_to(home)
        value = f'$HOME/{relative.as_posix()}'
    except ValueError:
        value = "'" + str(directory).replace("'", "'\\''") + "'"
    return f'case ":$PATH:" in *":{value}:"*) ;; *) export PATH="{value}:$PATH" ;; esac'


def update_managed_block(path: Path, body: str, dry_run: bool) -> bool:
    old = path.read_text(encoding="utf-8") if path.exists() else ""
    block = f"{BLOCK_START}\n{body}\n{BLOCK_END}"
    if BLOCK_START in old:
        start = old.index(BLOCK_START)
        end_marker = old.find(BLOCK_END, start)
        if end_marker < 0:
            raise RuntimeError(f"incomplete PIRA PATH block in {path}")
        end = end_marker + len(BLOCK_END)
        prefix = old[:start].rstrip()
        suffix = old[end:].strip("\n")
        new = (prefix + "\n\n" if prefix else "") + block
        new += "\n\n" + suffix + "\n" if suffix else "\n"
    else:
        new = old.rstrip() + ("\n\n" if old.strip() else "") + block + "\n"
    if new == old:
        return False
    if dry_run:
        print(f"DRY-RUN: would update PATH block in {path}")
        return True
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        stamp = datetime.now().strftime("%Y%m%d%H%M%S%f")
        shutil.copy2(path, path.with_name(f"{path.name}.bak.{stamp}"))
    temporary = path.with_name(f".{path.name}.pira-tmp-{os.getpid()}")
    temporary.write_text(new, encoding="utf-8")
    os.replace(temporary, path)
    print(f"Updated PATH in {path}")
    return True


def windows_user_path(directory: Path, dry_run: bool) -> bool:
    import winreg

    with winreg.CreateKey(winreg.HKEY_CURRENT_USER, r"Environment") as key:
        try:
            current, kind = winreg.QueryValueEx(key, "Path")
        except FileNotFoundError:
            current, kind = "", winreg.REG_EXPAND_SZ
        parts = [part for part in current.split(";") if part]
        normalized = os.path.normcase(str(directory.resolve()))
        if any(os.path.normcase(os.path.expandvars(part)) == normalized for part in parts):
            return False
        updated = ";".join([str(directory), *parts])
        if dry_run:
            print(f"DRY-RUN: would prepend {directory} to the user PATH")
            return True
        winreg.SetValueEx(key, "Path", 0, kind, updated)
    try:
        ctypes.windll.user32.SendMessageTimeoutW(0xFFFF, 0x001A, 0, "Environment", 0x0002, 5000, None)
    except Exception:
        pass
    print(f"Updated Windows user PATH with {directory}")
    return True


def ensure_path(directory: Path, dry_run: bool) -> bool:
    if os.name == "nt":
        return windows_user_path(directory, dry_run)
    changed = False
    for path in shell_profiles():
        changed = update_managed_block(path, shell_path_line(directory), dry_run) or changed
    return changed


def path_is_configured(directory: Path) -> bool:
    active = {
        Path(value).expanduser().resolve(strict=False)
        for value in os.environ.get("PATH", "").split(os.pathsep)
        if value
    }
    if directory in active:
        return True
    if os.name == "nt":
        import winreg
        try:
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Environment") as key:
                current, _ = winreg.QueryValueEx(key, "Path")
        except (FileNotFoundError, OSError):
            return False
        return any(
            Path(os.path.expandvars(value)).resolve(strict=False) == directory
            for value in current.split(";") if value
        )
    profiles = shell_profiles()
    return all(
        path.exists()
        and BLOCK_START in path.read_text(encoding="utf-8")
        and shell_path_line(directory) in path.read_text(encoding="utf-8")
        for path in profiles
    )


def direct_version(binary: Path) -> str:
    result = subprocess.run(
        [str(binary), "--version"], check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True
    )
    return result.stdout.strip()


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Install or refresh bundled PIRA tools for this user.")
    parser.add_argument("--install-dir", type=Path, default=None, help="Per-user PATH directory.")
    parser.add_argument("--dry-run", action="store_true", help="Describe changes without writing.")
    parser.add_argument("--verify", action="store_true", help="Verify the installed tool without changing it.")
    parser.add_argument("--no-path", action="store_true", help="Do not persist the install directory in PATH.")
    parser.add_argument("--force", action="store_true", help="Refresh even when the installed hash already matches.")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    selector = load_selector()
    install_dir = (args.install_dir or default_install_dir()).expanduser().resolve(strict=False)
    manifest = selector.load_manifest()
    source, record = selector.select_binary(manifest=manifest)
    destination = executable_path(install_dir)
    expected = record["sha256"]
    existing_hash = sha256(destination) if destination.is_file() and not destination.is_symlink() else None
    action = "unchanged" if existing_hash == expected else ("refresh" if destination.exists() or destination.is_symlink() else "install")

    print(f"Platform: {selector.current_platform()}")
    print(f"Bundled:  {source}")
    print(f"Target:   {destination}")

    if args.verify:
        failures: list[str] = []
        if existing_hash != expected:
            failures.append("installed binary is missing or stale")
        if not args.no_path and not path_is_configured(install_dir):
            failures.append("install directory is not configured in the user PATH")
        if not failures:
            version = direct_version(destination)
            if manifest.get("tool_version") and manifest["tool_version"] not in version:
                failures.append(f"unexpected version: {version}")
        if failures:
            for failure in failures:
                print(f"FAIL: {failure}", file=sys.stderr)
            return 1
        print(f"OK: {direct_version(destination)}; SHA-256 verified")
        return 0

    if action == "unchanged" and not args.force:
        print("OK: installed tool already matches the bundled release")
    elif args.dry_run:
        print(f"DRY-RUN: would {action} {destination}")
    else:
        installed = selector.install_binary(source, record, install_dir)
        actual = sha256(installed)
        if actual != expected:
            raise RuntimeError("installed tool hash does not match bundle manifest")
        print(f"{action.capitalize()}ed: {installed}")

    if not args.no_path:
        ensure_path(install_dir, args.dry_run)

    if not args.dry_run:
        version = direct_version(destination)
        print(f"Verified: {version}; SHA-256 {expected}")
        resolved = shutil.which("pira_ctx")
        if resolved and Path(resolved).resolve() != destination.resolve():
            print(f"NOTE: current process still resolves pira_ctx to {resolved}; restart the shell to activate {destination}")
        elif not resolved:
            print("NOTE: restart the shell or agent process to activate the updated PATH")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (RuntimeError, OSError, subprocess.CalledProcessError) as error:
        print(f"setup_pira_tools.py: {error}", file=sys.stderr)
        raise SystemExit(1)
