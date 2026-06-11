#!/usr/bin/env sh
# Bootstrap wrapper for PIRA setup.
# Ensures Python is available before delegating to setup_pira.py.
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
setup_py="$script_dir/setup_pira.py"
# shellcheck source=assets/scripts/lib/pira_python_bootstrap.sh
. "$script_dir/lib/pira_python_bootstrap.sh"

py=$(pira_bootstrap_python3 "$@")
exec "$py" "$setup_py" "$@"
