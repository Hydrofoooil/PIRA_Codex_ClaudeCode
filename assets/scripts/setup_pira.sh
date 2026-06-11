#!/usr/bin/env sh
# Bootstrap wrapper for PIRA setup.
# Ensures Python is available before delegating to setup_pira.py.
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
setup_py="$script_dir/setup_pira.py"

find_python() {
  if command -v python3 >/dev/null 2>&1; then
    command -v python3
    return 0
  fi
  if command -v python >/dev/null 2>&1; then
    py=$(command -v python)
    if "$py" - <<'PY' >/dev/null 2>&1
import sys
raise SystemExit(0 if sys.version_info[0] == 3 else 1)
PY
    then
      printf '%s\n' "$py"
      return 0
    fi
  fi
  return 1
}

install_python_hint() {
  os=$(uname -s 2>/dev/null || printf unknown)
  case "$os" in
    Darwin)
      if command -v brew >/dev/null 2>&1; then
        printf '%s\n' "Python 3 was not found. I can install it with Homebrew:" >&2
        printf '%s\n' "  brew install python" >&2
        if [ "${PIRA_SETUP_ASSUME_YES:-0}" = "1" ]; then
          brew install python
          return $?
        fi
        printf '%s' "Install Python 3 now with Homebrew? [y/N] " >&2
        read ans || ans=
        case "$ans" in
          y|Y|yes|YES) brew install python ;;
          *) return 1 ;;
        esac
      else
        printf '%s\n' \
          'Python 3 was not found, and Homebrew is not available.' \
          'Install Python 3 first, for example from https://www.python.org/downloads/ or Homebrew, then rerun this setup wrapper.' >&2
        return 1
      fi
      ;;
    Linux)
      printf '%s\n' \
        'Python 3 was not found.' \
        'Install Python 3 with your system package manager, for example:' \
        '  sudo apt-get update && sudo apt-get install -y python3' \
        '  sudo dnf install -y python3' \
        '  sudo pacman -S python' \
        'Then rerun this setup wrapper.' >&2
      return 1
      ;;
    *)
      printf '%s\n' \
        'Python 3 was not found.' \
        'Install Python 3 for this platform, then rerun this setup wrapper.' >&2
      return 1
      ;;
  esac
}

py=$(find_python || true)
if [ -z "${py:-}" ]; then
  install_python_hint
  py=$(find_python || true)
fi

if [ -z "${py:-}" ]; then
  printf '%s\n' 'ERROR: Python 3 is still unavailable; cannot run setup_pira.py.' >&2
  exit 1
fi

exec "$py" "$setup_py" "$@"
