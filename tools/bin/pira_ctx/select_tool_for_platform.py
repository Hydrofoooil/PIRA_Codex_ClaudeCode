#!/usr/bin/env python3
"""Select the bundled pira_ctx executable for this machine.

This script is intentionally small and deterministic: it directly maps the
current Python platform/architecture to a bundled binary path. It does not call
an agent, invoke a shell, install dependencies, or build from source.

Bundle layout:

    tools/bin/pira_ctx/
      select_tool_for_platform.py
      darwin-arm64/pira_ctx
      darwin-x64/pira_ctx
      linux-arm64/pira_ctx
      linux-x64/pira_ctx
      windows-x64/pira_ctx.exe

By default it prints the selected executable path. Setup scripts can later use
this path to copy or symlink the command into a user PATH location.
"""
from __future__ import annotations

import argparse
import platform
import sys
from pathlib import Path

BUNDLE_DIR = Path(__file__).resolve().parent

PLATFORM_BINARIES = {
    ("darwin", "arm64"): Path("darwin-arm64/pira_ctx"),
    ("darwin", "x64"): Path("darwin-x64/pira_ctx"),
    ("linux", "arm64"): Path("linux-arm64/pira_ctx"),
    ("linux", "x64"): Path("linux-x64/pira_ctx"),
    ("windows", "x64"): Path("windows-x64/pira_ctx.exe"),
}


def normalized_platform() -> tuple[str, str]:
    """Return normalized `(os_name, arch)` for selecting bundled binaries."""
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
    arch = {
        "x86_64": "x64",
        "amd64": "x64",
        "aarch64": "arm64",
        "arm64": "arm64",
    }.get(machine, machine)
    return os_name, arch


def supported_platforms() -> str:
    return ", ".join(f"{os_name}-{arch}" for os_name, arch in sorted(PLATFORM_BINARIES))


def select_binary(bundle_dir: Path = BUNDLE_DIR) -> Path:
    os_name, arch = normalized_platform()
    rel_path = PLATFORM_BINARIES.get((os_name, arch))
    if rel_path is None:
        raise SystemExit(f"unsupported platform {os_name}-{arch}; supported: {supported_platforms()}")

    binary = bundle_dir / rel_path
    if not binary.is_file():
        raise SystemExit(
            f"pira_ctx binary for {os_name}-{arch} is not bundled at {binary}\n"
            "Install a PIRA release that includes this platform binary."
        )
    return binary


def main() -> int:
    parser = argparse.ArgumentParser(description="Select bundled pira_ctx binary for this platform.")
    parser.add_argument("--print-platform", action="store_true", help="print normalized platform key before the path")
    args = parser.parse_args()

    os_name, arch = normalized_platform()
    if args.print_platform:
        print(f"{os_name}-{arch}")
    print(select_binary())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
