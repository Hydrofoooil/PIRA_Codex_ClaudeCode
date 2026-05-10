#!/usr/bin/env bash
# Install optional Codex audio notifications for macOS.
#
# This script avoids Python and other non-default dependencies. It creates
# non-blocking Bash hooks, updates Codex config.toml so the direct user-facing
# agent can play finished and waiting audio while subagent turns stay silent,
# and optionally installs a zsh wrapper so starting Codex plays an online audio
# clip.
#
# Project default audio set: ~/agent/PIRA_Voice/Samantha
# Example:
#   bash ~/agent/assets/setup_codex_audio_mode.sh \
#     --config ~/.codex/config.toml
#
# Local custom audio example:
#   bash ~/agent/assets/setup_codex_audio_mode.sh \
#     --config ~/.codex/config.toml \
#     --audio-dir ~/agent/PIRA_Voice/Debbie

set -euo pipefail

START="# BEGIN PIRA Codex speech notifications"
END="# END PIRA Codex speech notifications"
PLAYER_CMD="/usr/bin/afplay"
AUDIO_DIR="$HOME/agent/PIRA_Voice/Samantha"
STARTUP_AUDIO=""
FINISHED_AUDIO=""
WAITING_AUDIO=""
INSTALL_STARTUP_WRAPPER=1
FORCE=0
CONFIG=""
ZSHRC="$HOME/.zshrc"
STARTUP_START="# BEGIN PIRA Codex startup audio"
STARTUP_END="# END PIRA Codex startup audio"

usage() {
  cat <<'EOF'
Usage: setup_codex_audio_mode.sh --config PATH [options]

Options:
  --config PATH           Path to Codex config.toml, usually ~/.codex/config.toml.
  --audio-dir PATH        Directory containing start_msg.m4a, complete_msg.m4a,
                          and waiting_msg.m4a. Default: ~/agent/PIRA_Voice/Samantha.
  --player-cmd PATH       Audio player command. Default: /usr/bin/afplay.
  --startup-audio PATH    Startup audio file. Default: AUDIO_DIR/start_msg.m4a.
  --finished-audio PATH   Completion audio file. Default: AUDIO_DIR/complete_msg.m4a.
  --waiting-audio PATH    Waiting/approval audio file. Default: AUDIO_DIR/waiting_msg.m4a.
  --zshrc PATH            Path to zsh config for the startup wrapper. Default: ~/.zshrc.
  --no-startup-wrapper    Install completion/waiting notifications only.
  --force                 Replace an existing top-level notify entry after backing up config.

Deprecated:
  --say-cmd PATH, --voice NAME, and --startup-message TEXT were used by the old
  TTS installer. This installer plays audio files instead.
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

shell_double_quote() {
  # Print a double-quoted zsh string literal.
  printf '"%s"' "$(printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g; s/\$/\\$/g; s/`/\\`/g')"
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --config)
      [ "$#" -ge 2 ] || { usage >&2; exit 2; }
      CONFIG="$(expand_path "$2")"
      shift 2
      ;;
    --audio-dir)
      [ "$#" -ge 2 ] || { usage >&2; exit 2; }
      AUDIO_DIR="$(expand_path "$2")"
      shift 2
      ;;
    --player-cmd)
      [ "$#" -ge 2 ] || { usage >&2; exit 2; }
      PLAYER_CMD="$(expand_path "$2")"
      shift 2
      ;;
    --startup-audio)
      [ "$#" -ge 2 ] || { usage >&2; exit 2; }
      STARTUP_AUDIO="$(expand_path "$2")"
      shift 2
      ;;
    --finished-audio)
      [ "$#" -ge 2 ] || { usage >&2; exit 2; }
      FINISHED_AUDIO="$(expand_path "$2")"
      shift 2
      ;;
    --waiting-audio)
      [ "$#" -ge 2 ] || { usage >&2; exit 2; }
      WAITING_AUDIO="$(expand_path "$2")"
      shift 2
      ;;
    --zshrc)
      [ "$#" -ge 2 ] || { usage >&2; exit 2; }
      ZSHRC="$(expand_path "$2")"
      shift 2
      ;;
    --no-startup-wrapper)
      INSTALL_STARTUP_WRAPPER=0
      shift
      ;;
    --force)
      FORCE=1
      shift
      ;;
    --say-cmd|--voice|--startup-message)
      printf '%s is no longer supported; use --player-cmd and --audio-dir/audio file options instead.\n' "$1" >&2
      exit 2
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

if [ -z "$CONFIG" ]; then
  usage >&2
  exit 2
fi
if [ ! -x "$PLAYER_CMD" ]; then
  printf 'audio player command is not executable: %s\n' "$PLAYER_CMD" >&2
  exit 1
fi

[ -n "$STARTUP_AUDIO" ] || STARTUP_AUDIO="$AUDIO_DIR/start_msg.m4a"
[ -n "$FINISHED_AUDIO" ] || FINISHED_AUDIO="$AUDIO_DIR/complete_msg.m4a"
[ -n "$WAITING_AUDIO" ] || WAITING_AUDIO="$AUDIO_DIR/waiting_msg.m4a"

for audio in "$STARTUP_AUDIO" "$FINISHED_AUDIO" "$WAITING_AUDIO"; do
  if [ ! -r "$audio" ]; then
    printf 'audio file is not readable: %s\n' "$audio" >&2
    exit 1
  fi
done

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
# Non-blocking Codex audio notification installed by PIRA.
set -euo pipefail
PLAYER_CMD=$(shell_quote "$PLAYER_CMD")
FINISHED_AUDIO=$(shell_quote "$FINISHED_AUDIO")
WAITING_AUDIO=$(shell_quote "$WAITING_AUDIO")

payload="\$*"
if [ -z "\$payload" ]; then
  payload="\$(cat || true)"
fi

frontmost_identity() {
  osascript <<'OSA' 2>/dev/null || true
tell application "System Events"
  set p to first application process whose frontmost is true
  set n to name of p
  set bid to ""
  try
    set bid to bundle identifier of p
  end try
  set uid to ""
  try
    set uid to unix id of p as text
  end try
  return n & linefeed & bid & linefeed & uid
end tell
OSA
}

codex_ui_seems_focused() {
  # Best-effort focus check: if a terminal/editor app is frontmost, assume the
  # user may already be looking at Codex and avoid redundant completion audio.
  # Some apps, especially VS Code-like Electron builds, may report their process
  # name as "Electron", so we also inspect bundle id and process command/path.
  identity="\$(frontmost_identity)"
  [ -n "\$identity" ] || return 1
  pid="\$(printf '%s\n' "\$identity" | sed -n '3p')"
  process_info=""
  case "\$pid" in
    ''|*[!0-9]*) ;;
    *) process_info="\$(ps -p "\$pid" -o comm= -o args= 2>/dev/null || true)" ;;
  esac
  haystack="\$(printf '%s\n%s' "\$identity" "\$process_info" | tr '[:upper:]' '[:lower:]')"
  case "\$haystack" in
    *terminal*|*iterm2*|*warp*|*wezterm*|*ghostty*|*alacritty*|*kitty*|*hyper*|*tabby*|*rio*|*black\ box*|*gnome\ terminal*|*konsole*|*tilix*|*xterm*|*mintty*|*cmder*|*conemu*|*mobaxterm*|*visual\ studio\ code*|*com.microsoft.vscode*|*code-insiders*|*vscodium*|*code\ -\ oss*|*cursor*|*todesktop*|*windsurf*|*codeium*|*zed*|*sublime\ text*|*textmate*|*bbedit*|*nova*|*macvim*|*neovide*|*emacs*|*xcode*|*android\ studio*|*intellij\ idea*|*idea*|*pycharm*|*webstorm*|*clion*|*goland*|*phpstorm*|*rider*|*rubymine*|*rustrover*|*datagrip*)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

play_unless_focused() {
  audio="\$1"
  if codex_ui_seems_focused; then
    exit 0
  fi
  "\$PLAYER_CMD" "\$audio" >/dev/null 2>&1 &
}

json_unescape_minimal() {
  sed -e 's/\\\\"/"/g' \\
      -e 's/\\\\n/ /g' \\
      -e 's/\\\\r/ /g' \\
      -e 's/\\\\t/ /g' \\
      -e 's/\\\\\\\\/\\\\/g'
}

extract_turn_id() {
  printf '%s' "\$payload" | tr '\n' ' ' | sed -nE \\
    's/.*"turn[-_]id"[[:space:]]*:[[:space:]]*"([^"\\\\]+)".*/\\1/p' | head -n 1
}

turn_is_from_subagent() {
  turn_id="\$(extract_turn_id)"
  [ -n "\$turn_id" ] || return 1
  sessions_dir="\${PIRA_CODEX_SESSIONS_DIR:-\$HOME/.codex/sessions}"
  [ -d "\$sessions_dir" ] || return 1
  session_file="\$(find "\$sessions_dir" -type f -name '*.jsonl' -print 2>/dev/null \\
    | xargs grep -l "\$turn_id" 2>/dev/null \\
    | head -n 1 || true)"
  [ -n "\$session_file" ] || return 1
  head -n 1 "\$session_file" 2>/dev/null | grep -q '"subagent"'
}

extract_last_assistant_message() {
  # Best-effort extraction without Python or jq. We intentionally inspect only
  # the final assistant message, not the full payload, to avoid classifying the
  # user's prompt as a pending action request.
  printf '%s' "\$payload" | tr '\n' ' ' | sed -nE \\
    's/.*"last[-_]assistant[-_]message"[[:space:]]*:[[:space:]]*"(([^"\\\\]|\\\\.)*)".*/\\1/p' \\
    | json_unescape_minimal
}

if turn_is_from_subagent; then
  exit 0
fi

message="\$(extract_last_assistant_message | head -n 1)"

# If extraction fails, default to "finished" rather than guessing from the full
# JSON payload. Only a question mark in the final assistant message is treated
# as waiting. Waiting audio uses the same focus check as completion audio.
if [ -n "\$message" ] && printf '%s' "\$message" | grep -q '?'; then
  play_unless_focused "\$WAITING_AUDIO"
else
  play_unless_focused "\$FINISHED_AUDIO"
fi
SH_EOF
chmod +x "$NOTIFY_SCRIPT"

cat > "$WAITING_SCRIPT" <<SH_EOF
#!/usr/bin/env bash
# Play audio when the user-facing Codex agent is waiting for user action.
set -euo pipefail
PLAYER_CMD=$(shell_quote "$PLAYER_CMD")
WAITING_AUDIO=$(shell_quote "$WAITING_AUDIO")

payload="\$*"
if [ -z "\$payload" ] && [ ! -t 0 ]; then
  payload="\$(cat || true)"
fi

extract_turn_id() {
  printf '%s' "\$payload" | tr '\n' ' ' | sed -nE \\
    's/.*"turn[-_]id"[[:space:]]*:[[:space:]]*"([^"\\\\]+)".*/\\1/p' | head -n 1
}

turn_is_from_subagent() {
  turn_id="\$(extract_turn_id)"
  [ -n "\$turn_id" ] || return 1
  sessions_dir="\${PIRA_CODEX_SESSIONS_DIR:-\$HOME/.codex/sessions}"
  [ -d "\$sessions_dir" ] || return 1
  session_file="\$(find "\$sessions_dir" -type f -name '*.jsonl' -print 2>/dev/null \\
    | xargs grep -l "\$turn_id" 2>/dev/null \\
    | head -n 1 || true)"
  [ -n "\$session_file" ] || return 1
  head -n 1 "\$session_file" 2>/dev/null | grep -q '"subagent"'
}

frontmost_identity() {
  osascript <<'OSA' 2>/dev/null || true
tell application "System Events"
  set p to first application process whose frontmost is true
  set n to name of p
  set bid to ""
  try
    set bid to bundle identifier of p
  end try
  set uid to ""
  try
    set uid to unix id of p as text
  end try
  return n & linefeed & bid & linefeed & uid
end tell
OSA
}

codex_ui_seems_focused() {
  identity="\$(frontmost_identity)"
  [ -n "\$identity" ] || return 1
  pid="\$(printf '%s\n' "\$identity" | sed -n '3p')"
  process_info=""
  case "\$pid" in
    ''|*[!0-9]*) ;;
    *) process_info="\$(ps -p "\$pid" -o comm= -o args= 2>/dev/null || true)" ;;
  esac
  haystack="\$(printf '%s\n%s' "\$identity" "\$process_info" | tr '[:upper:]' '[:lower:]')"
  case "\$haystack" in
    *terminal*|*iterm2*|*warp*|*wezterm*|*ghostty*|*alacritty*|*kitty*|*hyper*|*tabby*|*rio*|*black\ box*|*gnome\ terminal*|*konsole*|*tilix*|*xterm*|*mintty*|*cmder*|*conemu*|*mobaxterm*|*visual\ studio\ code*|*com.microsoft.vscode*|*code-insiders*|*vscodium*|*code\ -\ oss*|*cursor*|*todesktop*|*windsurf*|*codeium*|*zed*|*sublime\ text*|*textmate*|*bbedit*|*nova*|*macvim*|*neovide*|*emacs*|*xcode*|*android\ studio*|*intellij\ idea*|*idea*|*pycharm*|*webstorm*|*clion*|*goland*|*phpstorm*|*rider*|*rubymine*|*rustrover*|*datagrip*)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

if turn_is_from_subagent; then
  printf '{}\n'
  exit 0
fi

if ! codex_ui_seems_focused; then
  "\$PLAYER_CMD" "\$WAITING_AUDIO" >/dev/null 2>&1 &
fi
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
    print "# Non-blocking status audio for Codex on macOS."
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
      print "# Non-blocking status audio for Codex on macOS."
      print notify_line
      print end
    }
  }
' "$TMP2" > "$TMP3"
cp "$TMP3" "$TMP2"

# Ensure [features] contains hooks = true and remove deprecated codex_hooks.
if grep -Eq '^\[features\][[:space:]]*$' "$TMP2"; then
  awk '
    /^\[features\][[:space:]]*$/ { in_features=1; printed=0; print; next }
    in_features && /^\[/ {
      if (!printed) print "hooks = true"
      in_features=0
    }
    in_features && /^[[:space:]]*hooks[[:space:]]*=/ { print "hooks = true"; printed=1; next }
    in_features && /^[[:space:]]*codex_hooks[[:space:]]*=/ { next }
    { print }
    END { if (in_features && !printed) print "hooks = true" }
  ' "$TMP2" > "$TMP3"
else
  awk '
    BEGIN { inserted=0 }
    !inserted && /^\[/ { print "[features]"; print "hooks = true"; print ""; inserted=1 }
    { print }
    END { if (!inserted) { print "[features]"; print "hooks = true" } }
  ' "$TMP2" > "$TMP3"
fi
cp "$TMP3" "$TMP2"

waiting_command="/bin/bash $(shell_quote "$WAITING_SCRIPT")"
cat >> "$TMP2" <<HOOK_EOF

$START
# Play waiting audio only for the user-facing agent unless the coding UI is focused.
[[hooks.PermissionRequest]]
matcher = "*"

[[hooks.PermissionRequest.hooks]]
type = "command"
command = $(toml_basic_string "$waiting_command")
timeout = 1
statusMessage = "Checking waiting status audio"
$END
HOOK_EOF

cp "$TMP2" "$CONFIG"

ZSHRC_BACKUP=""
if [ "$INSTALL_STARTUP_WRAPPER" -eq 1 ]; then
  mkdir -p "$(dirname "$ZSHRC")"
  [ -f "$ZSHRC" ] || : > "$ZSHRC"
  ZSHRC_BACKUP="$ZSHRC.bak.$(date +%Y%m%d-%H%M%S)"
  cp "$ZSHRC" "$ZSHRC_BACKUP"
  STARTUP_TMP="$(mktemp)"
  trap 'rm -f "$TMP1" "$TMP2" "$TMP3" "$STARTUP_TMP"' EXIT
  awk -v start="$STARTUP_START" -v end="$STARTUP_END" '
    index($0, start) { skip=1; next }
    index($0, end) { skip=0; next }
    !skip { print }
  ' "$ZSHRC" > "$STARTUP_TMP"
  cat >> "$STARTUP_TMP" <<STARTUP_EOF

$STARTUP_START
# Play a short status audio clip when starting Codex from an interactive zsh shell.
codex() {
  { $(shell_quote "$PLAYER_CMD") $(shell_double_quote "$STARTUP_AUDIO") >/dev/null 2>&1 &! } 2>/dev/null
  command codex "\$@"
}
$STARTUP_END
STARTUP_EOF
  cp "$STARTUP_TMP" "$ZSHRC"
  zsh -n "$ZSHRC"
fi

printf 'Codex audio notification mode installed.\n'
printf 'Config: %s\n' "$CONFIG"
printf 'Audio player: %s\n' "$PLAYER_CMD"
printf 'Audio directory: %s\n' "$AUDIO_DIR"
printf 'Startup audio: %s\n' "$STARTUP_AUDIO"
printf 'Finished audio: %s\n' "$FINISHED_AUDIO"
printf 'Waiting audio: %s\n' "$WAITING_AUDIO"
printf 'Notify script: %s\n' "$NOTIFY_SCRIPT"
printf 'Waiting hook: %s\n' "$WAITING_SCRIPT"
if [ -n "$BACKUP" ]; then
  printf 'Config backup: %s\n' "$BACKUP"
fi
if [ "$INSTALL_STARTUP_WRAPPER" -eq 1 ]; then
  printf 'Startup wrapper installed in: %s\n' "$ZSHRC"
  printf 'zsh config backup: %s\n' "$ZSHRC_BACKUP"
  printf 'Run `source %s` or open a new terminal before starting Codex.\n' "$ZSHRC"
fi
printf 'Restart Codex to load the new notification settings.\n'
