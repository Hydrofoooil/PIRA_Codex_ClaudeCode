#!/usr/bin/env python3
"""Select the bundled pira_ctx executable for the current platform.

The install-facing bundle is rooted at this script's directory:

    tools/bin/pira_ctx/
      select_tool_for_platform.py
      darwin-arm64/pira_ctx
      darwin-x64/pira_ctx
      linux-x64/pira_ctx
      linux-arm64/pira_ctx
      windows-x64/pira_ctx.exe

By default the script prints the selected executable path. Use --copy-to PATH
or --symlink-to PATH from setup/install code to materialize a stable command
location without requiring Rust/Cargo on the user's machine.
"""
from __future__ import annotations

import argparse
import os
import platform
import shutil
import stat
import sys
from pathlib import Path

BUNDLE_DIR = Path(__file__).resolve().parent

PLATFORM_BINARIES = {
    ("darwin", "arm64"): Path("darwin-arm64/pira_ctx"),
    ("darwin", "x64"): Path("darwin-x64/pira_ctx"),
    ("linux", "x64"): Path("linux-x64/pira_ctx"),
    ("linux", "arm64"): Path("linux-arm64/pira_ctx"),
    ("windows", "x64"): Path("windows-x64/pira_ctx.exe"),
    ("windows", "arm64"): Path("windows-arm64/pira_ctx.exe"),
}


def normalized_platform() -> tuple[str, str]:
    sys_platform = sys.platform.lower()
    if sys_platform == "darwin":
        os_name = "darwin"
    elif sys_platform.startswith("linux"):
        os_name = "linux"
    elif sys_platform in {"win32", "cygwin", "msys"}:
        os_name = "windows"
    else:
        os_name = sys_platform

    machine = platform.machine().lower()
    arch_aliases = {
        "x86_64": "x64",
        "amd64": "x64",
        "aarch64": "arm64",
        "arm64": "arm64",
    }
    return os_name, arch_aliases.get(machine, machine)


def select_binary(bundle_dir: Path = BUNDLE_DIR) -> Path:
    key = normalized_platform()
    rel = PLATFORM_BINARIES.get(key)
    if rel is None:
        supported = ", ".join(f"{os_name}-{arch}" for os_name, arch in sorted(PLATFORM_BINARIES))
        raise SystemExit(f"unsupported platform {key[0]}-{key[1]}; supported: {supported}")
    binary = bundle_dir / rel
    if not binary.is_file():
        raise SystemExit(
            f"pira_ctx binary for {key[0]}-{key[1]} is not bundled at {binary}\n"
            "Install a PIRA release that includes this platform binary."
        )
    return binary


def make_executable(path: Path) -> None:
    if os.name == "nt":
        return
    mode = path.stat().st_mode
    path.chmod(mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)


def copy_to(src: Path, dst: Path) -> None:
    dst.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(src, dst)
    make_executable(dst)


def symlink_to(src: Path, dst: Path) -> None:
    dst.parent.mkdir(parents=True, exist_ok=True)
    tmp = dst.with_name(f".{dst.name}.tmp")
    try:
        tmp.unlink()
    except FileNotFoundError:
        pass
    os.symlink(src, tmp)
    os.replace(tmp, dst)


def main() -> int:
    parser = argparse.ArgumentParser(description="Select bundled pira_ctx binary for this platform.")
    parser.add_argument("--copy-to", type=Path, help="copy the selected binary to PATH")
    parser.add_argument("--symlink-to", type=Path, help="symlink PATH to the selected binary")
    parser.add_argument("--print-platform", action="store_true", help="print normalized platform key")
    args = parser.parse_args()

    os_name, arch = normalized_platform()
    if args.print_platform:
        print(f"{os_name}-{arch}")

    binary = select_binary()
    if args.copy_to and args.symlink_to:
        parser.error("choose at most one of --copy-to or --symlink-to")
    if args.copy_to:
        copy_to(binary, args.copy_to)
        print(args.copy_to)
    elif args.symlink_to:
        symlink_to(binary, args.symlink_to)
        print(args.symlink_to)
    else:
        print(binary)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
