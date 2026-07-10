#!/usr/bin/env python3
"""Select and verify the bundled ``pira_ctx`` executable for this machine.

Selection is local and deterministic. The script does not invoke an agent or a
shell, install dependencies, or build code. Setup code should normally consume
the default absolute path output and copy that binary into the installation.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import sys
from pathlib import Path
from typing import Any

BUNDLE_DIR = Path(__file__).resolve().parent
MANIFEST_PATH = BUNDLE_DIR / "bundle.json"

OS_ALIASES = {
    "darwin": "darwin",
    "linux": "linux",
    "win32": "windows",
    "cygwin": "windows",
    "msys": "windows",
}

ARCH_ALIASES = {
    "x86_64": "x64",
    "amd64": "x64",
    "aarch64": "arm64",
    "arm64": "arm64",
}


class SelectionError(RuntimeError):
    """Raised when no safe bundled executable can be selected."""


def normalize_platform(sys_platform: str, machine: str) -> str:
    """Return the canonical ``os-arch`` key for explicit platform values."""
    system = sys_platform.lower()
    os_name = next(
        (normalized for prefix, normalized in OS_ALIASES.items() if system == prefix or system.startswith(prefix)),
        system,
    )
    architecture = ARCH_ALIASES.get(machine.lower(), machine.lower())
    return f"{os_name}-{architecture}"


def current_platform() -> str:
    """Return the canonical key for the current Python process."""
    return normalize_platform(sys.platform, platform.machine())


def load_manifest(path: Path = MANIFEST_PATH) -> dict[str, Any]:
    try:
        manifest = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as error:
        raise SelectionError(f"pira_ctx bundle manifest is missing: {path}") from error
    except (OSError, json.JSONDecodeError) as error:
        raise SelectionError(f"cannot read pira_ctx bundle manifest {path}: {error}") from error
    if manifest.get("schema_version") != 1 or not isinstance(manifest.get("binaries"), dict):
        raise SelectionError(f"unsupported pira_ctx bundle manifest: {path}")
    return manifest


def select_binary(
    platform_key: str | None = None,
    *,
    bundle_dir: Path = BUNDLE_DIR,
    manifest: dict[str, Any] | None = None,
    verify: bool = True,
) -> tuple[Path, dict[str, Any]]:
    """Select a bundled binary and optionally verify its recorded SHA-256."""
    manifest = load_manifest(bundle_dir / "bundle.json") if manifest is None else manifest
    platform_key = current_platform() if platform_key is None else platform_key
    binaries = manifest["binaries"]
    record = binaries.get(platform_key)
    if not isinstance(record, dict):
        supported = ", ".join(sorted(binaries))
        raise SelectionError(f"unsupported platform {platform_key}; supported: {supported}")

    relative = Path(str(record.get("path", "")))
    if relative.is_absolute() or ".." in relative.parts:
        raise SelectionError(f"unsafe binary path in bundle manifest for {platform_key}")
    binary = (bundle_dir / relative).resolve()
    try:
        binary.relative_to(bundle_dir.resolve())
    except ValueError as error:
        raise SelectionError(f"binary path escapes bundle directory for {platform_key}") from error
    if not binary.is_file():
        raise SelectionError(
            f"pira_ctx binary for {platform_key} is not bundled at {binary}; "
            "install a PIRA release that includes this platform binary"
        )
    if os.name != "nt" and not os.access(binary, os.X_OK):
        raise SelectionError(f"pira_ctx binary is not executable: {binary}")
    if verify:
        expected = record.get("sha256")
        if not isinstance(expected, str) or len(expected) != 64:
            raise SelectionError(f"missing SHA-256 for {platform_key} in bundle manifest")
        actual = sha256_file(binary)
        if actual != expected.lower():
            raise SelectionError(
                f"pira_ctx checksum mismatch for {platform_key}: expected {expected}, got {actual}"
            )
    return binary, record


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Select the bundled pira_ctx binary for this platform.")
    output = parser.add_mutually_exclusive_group()
    output.add_argument("--platform", "--print-platform", action="store_true", help="print only the normalized platform key")
    output.add_argument("--json", action="store_true", help="print selection details as JSON")
    parser.add_argument("--no-verify", action="store_true", help="skip SHA-256 verification")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    key = current_platform()
    binary, record = select_binary(key, verify=not args.no_verify)
    if args.platform:
        print(key)
    elif args.json:
        print(json.dumps({**record, "platform": key, "path": str(binary)}, sort_keys=True))
    else:
        print(binary)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except SelectionError as error:
        print(f"select_tool_for_platform.py: {error}", file=sys.stderr)
        raise SystemExit(1)
