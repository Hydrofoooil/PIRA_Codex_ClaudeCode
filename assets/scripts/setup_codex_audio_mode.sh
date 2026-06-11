#!/usr/bin/env sh
# Thin macOS wrapper for the shared PIRA Codex audio setup helper.
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
setup_py="$script_dir/setup_codex_audio_mode.py"
# shellcheck source=assets/scripts/lib/pira_python_bootstrap.sh
. "$script_dir/lib/pira_python_bootstrap.sh"

py=$(pira_require_python3)
exec "$py" "$setup_py" --platform macos "$@"
