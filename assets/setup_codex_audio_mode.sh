#!/usr/bin/env bash
# Install optional Codex speech notifications for macOS.
#
# This script avoids Python and other non-default dependencies. It creates
# non-blocking Bash hooks and updates a Codex config.toml so future Codex turns
# say either "Pyra finished." or "Pyra waiting for action." using macOS `say`.
#
# Example:
#   bash ~/agent/assets/setup_codex_audio_mode.sh \
#     --say-cmd /usr/bin/say \
#     --config ~/.codex/config.toml

set -euo pipefail

START="# BEGIN PIRA Codex speech notifications"
END="# END PIRA Codex speech notifications"
VOICE="Samantha"
FORCE=0
SAY_CMD=""
CONFIG=""

usage() {
  cat <<'EOF'
Usage: setup_codex_audio_mode.sh --say-cmd PATH --config PATH [--voice NAME] [--force]

Options:
  --say-cmd PATH   Path to macOS say command, usually /usr/bin/say.
  --config PATH    Path to Codex config.toml, usually ~/.codex/config.toml.
  --voice NAME     macOS voice name to use. Default: Samantha.
  --force          Replace an existing top-level notify entry after backing up config.
EOF
}

expand_path() {
  case "$1" in
    ~) printf '%s\n' "$HOME" ;;
    ~/*) printf '%s/%s\n' "$HOME" "${1#~/}" ;;
    *) printf '%s\n' "$1" ;;
  esac
}

shell_quote() {
  # Print a single-quoted shell literal. Correctly handles embedded single quotes.
  printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"
}

toml_basic_string() {
  # Print a TOML basic string with the minimal escaping needed for paths/commands.
  printf '"%s"' "$(printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g')"
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --say-cmd)
      [ "$#" -ge 2 ] || { usage >&2; exit 2; }
      SAY_CMD="$(expand_path "$2")"
      shift 2
      ;;
    --config)
      [ "$#" -ge 2 ] || { usage >&2; exit 2; }
      CONFIG="$(expand_path "$2")"
      shift 2
      ;;
    --voice)
      [ "$#" -ge 2 ] || { usage >&2; exit 2; }
      VOICE="$2"
      shift 2
      ;;
    --force)
      FORCE=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf 'Unknown argument: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [ -z "$SAY_CMD" ] || [ -z "$CONFIG" ]; then
  usage >&2
  exit 2
fi
if [ ! -x "$SAY_CMD" ]; then
  printf 'say command is not executable: %s\n' "$SAY_CMD" >&2
  exit 1
fi

CONFIG_DIR="$(dirname "$CONFIG")"
HOOKS_DIR="$CONFIG_DIR/hooks"
NOTIFY_SCRIPT="$HOOKS_DIR/speak_notify.sh"
WAITING_SCRIPT="$HOOKS_DIR/speak_waiting.sh"
mkdir -p "$CONFIG_DIR"
: > /dev/null
[ -f "$CONFIG" ] || : > "$CONFIG"

TMP1="$(mktemp)"
TMP2="$(mktemp)"
TMP3="$(mktemp)"
trap 'rm -f "$TMP1" "$TMP2" "$TMP3"' EXIT

# Remove previous PIRA-managed blocks before checking for user-managed notify.
awk -v start="$START" -v end="$END" '
  index($0, start) { skip=1; next }
  index($0, end) { skip=0; next }
  !skip { print }
' "$CONFIG" > "$TMP1"

if grep -Eq '^notify[[:space:]]*=' "$TMP1" && [ "$FORCE" -ne 1 ]; then
  printf 'Refusing to replace an existing top-level `notify` entry without --force.\n' >&2
  printf 'Inspect the existing notify entry, then rerun with --force if replacement is acceptable.\n' >&2
  exit 1
fi

BACKUP=""
if [ -s "$CONFIG" ]; then
  BACKUP="$CONFIG.bak.$(date +%Y%m%d-%H%M%S)"
  cp "$CONFIG" "$BACKUP"
fi

mkdir -p "$HOOKS_DIR"

cat > "$NOTIFY_SCRIPT" <<SH_EOF
#!/usr/bin/env bash
# Non-blocking Codex speech notification installed by PIRA.
set -euo pipefail
SAY_CMD=$(shell_quote "$SAY_CMD")
VOICE=$(shell_quote "$VOICE")
FINISHED='Pyra finished.'
WAITING='Pyra waiting for action.'

payload="\$*"
if [ -z "\$payload" ]; then
  payload="\$(cat || true)"
fi

json_unescape_minimal() {
  sed -e 's/\\\\"/"/g' \\
      -e 's/\\\\n/ /g' \\
      -e 's/\\\\r/ /g' \\
      -e 's/\\\\t/ /g' \\
      -e 's/\\\\\\\\/\\\\/g'
}

extract_last_assistant_message() {
  # Best-effort extraction without Python or jq. We intentionally inspect only
  # the final assistant message, not the full payload, to avoid classifying the
  # user's prompt as a pending action request.
  printf '%s' "\$payload" | tr '\n' ' ' | sed -nE \\
    's/.*"last[-_]assistant[-_]message"[[:space:]]*:[[:space:]]*"(([^"\\\\]|\\\\.)*)".*/\\1/p' \\
    | json_unescape_minimal
}

message="\$(extract_last_assistant_message | head -n 1)"

# If extraction fails, default to "finished" rather than guessing from the full
# JSON payload. This avoids false "waiting" notifications from user prompt text.
if [ -n "\$message" ] && printf '%s' "\$message" | grep -Eiq '\?|confirm|confirmation|approve|approval|permission|do you want|would you like|should i|shall i|may i|please confirm|please approve|waiting for|need your|needs your|reply|respond|choose|select|pick|can i|could i'; then
  "\$SAY_CMD" -v "\$VOICE" "\$WAITING" >/dev/null 2>&1 &
else
  "\$SAY_CMD" -v "\$VOICE" "\$FINISHED" >/dev/null 2>&1 &
fi
SH_EOF
chmod +x "$NOTIFY_SCRIPT"

cat > "$WAITING_SCRIPT" <<SH_EOF
#!/usr/bin/env bash
# Speak when Codex is waiting for user action, without blocking.
set -euo pipefail
SAY_CMD=$(shell_quote "$SAY_CMD")
VOICE=$(shell_quote "$VOICE")
"\$SAY_CMD" -v "\$VOICE" 'Pyra waiting for action.' >/dev/null 2>&1 &
printf '{}\n'
SH_EOF
chmod +x "$WAITING_SCRIPT"

if grep -Eq '^notify[[:space:]]*=' "$TMP1"; then
  grep -Ev '^notify[[:space:]]*=' "$TMP1" > "$TMP2"
else
  cp "$TMP1" "$TMP2"
fi

# Insert top-level notify before the first TOML table, so it cannot be parsed as
# part of a previous table.
notify_line="notify = [\"/bin/bash\", $(toml_basic_string "$NOTIFY_SCRIPT")]"
awk -v start="$START" -v end="$END" -v notify_line="$notify_line" '
  BEGIN { inserted=0 }
  !inserted && $0 ~ /^\[/ {
    print start
    print "# Non-blocking status speech for Codex on macOS."
    print notify_line
    print end
    print ""
    inserted=1
  }
  { print }
  END {
    if (!inserted) {
      print ""
      print start
      print "# Non-blocking status speech for Codex on macOS."
      print notify_line
      print end
    }
  }
' "$TMP2" > "$TMP3"
cp "$TMP3" "$TMP2"

# Ensure [features] contains codex_hooks = true.
if grep -Eq '^\[features\][[:space:]]*$' "$TMP2"; then
  awk '
    /^\[features\][[:space:]]*$/ { in_features=1; printed=0; print; next }
    in_features && /^\[/ {
      if (!printed) print "codex_hooks = true"
      in_features=0
    }
    in_features && /^[[:space:]]*codex_hooks[[:space:]]*=/ { print "codex_hooks = true"; printed=1; next }
    { print }
    END { if (in_features && !printed) print "codex_hooks = true" }
  ' "$TMP2" > "$TMP3"
else
  awk '
    BEGIN { inserted=0 }
    !inserted && /^\[/ { print "[features]"; print "codex_hooks = true"; print ""; inserted=1 }
    { print }
    END { if (!inserted) { print "[features]"; print "codex_hooks = true" } }
  ' "$TMP2" > "$TMP3"
fi
cp "$TMP3" "$TMP2"

waiting_command="/bin/bash $(shell_quote "$WAITING_SCRIPT")"
cat >> "$TMP2" <<HOOK_EOF

$START
# Speak when Codex is waiting for approval/action.
[[hooks.PermissionRequest]]
matcher = "*"

[[hooks.PermissionRequest.hooks]]
type = "command"
command = $(toml_basic_string "$waiting_command")
timeout = 1
statusMessage = "Speaking waiting status"
$END
HOOK_EOF

cp "$TMP2" "$CONFIG"

printf 'Codex speech notification mode installed.\n'
printf 'Config: %s\n' "$CONFIG"
printf 'Notify script: %s\n' "$NOTIFY_SCRIPT"
printf 'Waiting hook: %s\n' "$WAITING_SCRIPT"
if [ -n "$BACKUP" ]; then
  printf 'Backup: %s\n' "$BACKUP"
fi
printf 'Restart Codex to load the new notification settings.\n'
