# Customizing PIRA audio notifications

This guide is for **Codex audio notification mode on macOS or Windows**. It explains how to prepare a custom audio set, postprocess clips so they are consistent and unobtrusive, and ask PIRA to install or refine the result.

Use only audio you own, generated audio you are allowed to use, or voices used with clear consent. Keep clips short; these sounds may play while you are away from the terminal, so they should be recognizable without being distracting.

## Required files

A PIRA audio set is a folder containing these three event clips:

```text
start_msg.m4a
complete_msg.m4a
waiting_msg.m4a
```

Recommended meaning:

- `start_msg.m4a` — PIRA started / Codex launched.
- `complete_msg.m4a` — PIRA finished a turn.
- `waiting_msg.m4a` — PIRA is standing by for user confirmation, approval, or another action.

Recommended lengths:

- startup: 1--3 seconds;
- completion: 1--4 seconds;
- waiting: 2--6 seconds, gentle enough to repeat occasionally.

Recommended loudness target:

- integrated loudness around `-20` to `-18` LUFS;
- true peak at or below `-2 dBTP`;
- no clipping, harsh consonants, long silence, or abrupt cuts.

## Suggested folder layout

Keep raw source files separate from final notification files:

```text
~/agent/PIRA_Voice/MyVoice_raw/
  start_raw.wav
  complete_raw.wav
  waiting_raw.wav

~/agent/PIRA_Voice/MyVoice/
  start_msg.m4a
  complete_msg.m4a
  waiting_msg.m4a
```

The final folder can then be installed with the setup helper:

macOS:

```bash
bash ~/agent/assets/setup_codex_audio_mode.sh \
  --config ~/.codex/config.toml \
  --audio-dir ~/agent/PIRA_Voice/MyVoice
```

Windows PowerShell:

```powershell
powershell.exe -ExecutionPolicy Bypass -File "$HOME\agent\assets\setup_codex_audio_mode_windows.ps1" `
  -ConfigPath "$HOME\.codex\config.toml" `
  -AudioDir "$HOME\agent\PIRA_Voice\MyVoice"
```

Restart Codex after changing audio mode or replacing installed audio files.

## Postprocessing checklist

Ask PIRA to do these checks before installing the files:

1. Confirm the three final files exist and are readable.
2. Inspect duration, codec, sample rate, channels, and peak/loudness where tools are available.
3. Trim leading/trailing silence.
4. Add a tiny fade-in and fade-out to avoid clicks.
5. Normalize loudness consistently across all three clips.
6. Convert/export to `.m4a` using AAC.
7. Play or preview the clips locally when possible.
8. Install with the platform helper and verify Codex config still has top-level `notify`.

If `ffmpeg` and `ffprobe` are available, PIRA can usually handle the full postprocessing pipeline. If they are missing, ask PIRA to use built-in platform tools when feasible or tell you the exact missing dependency.

## Example postprocessing approach

For each raw clip, PIRA can use a filter chain like this as a starting point:

```bash
ffmpeg -y -i input.wav \
  -af "silenceremove=start_periods=1:start_duration=0.05:start_threshold=-45dB:stop_periods=1:stop_duration=0.12:stop_threshold=-45dB,afade=t=in:st=0:d=0.03,areverse,afade=t=in:st=0:d=0.08,areverse,loudnorm=I=-19:TP=-2:LRA=7" \
  -c:a aac -b:a 160k output.m4a
```

This trims quiet edges, smooths the start and end, normalizes loudness, and writes an AAC `.m4a`. PIRA should adjust thresholds and target loudness if the source audio is unusually quiet, noisy, or already mastered.

For especially important clips, ask PIRA for a two-pass `loudnorm` workflow. Two-pass normalization is more exact, but the one-pass command above is usually enough for short notification sounds.

## Concrete prompts for PIRA

### 1. Create a new custom audio set from raw clips

```text
Please create a custom PIRA Codex audio set from these raw clips:
- startup: ~/agent/PIRA_Voice/MyVoice_raw/start_raw.wav
- completion: ~/agent/PIRA_Voice/MyVoice_raw/complete_raw.wav
- waiting: ~/agent/PIRA_Voice/MyVoice_raw/waiting_raw.wav

Output folder:
~/agent/PIRA_Voice/MyVoice

Postprocess them into start_msg.m4a, complete_msg.m4a, and waiting_msg.m4a. Please trim leading/trailing silence, add very short fades to avoid clicks, normalize them to a consistent gentle notification loudness around -19 LUFS with true peak <= -2 dBTP, convert to AAC .m4a, and verify duration/codec afterward. Use ffmpeg/ffprobe if available. Do not change Codex config yet; just prepare the files and summarize the results.
```

### 2. Make an existing set quieter and less intrusive

```text
Please postprocess my existing PIRA audio set to make it quieter and less intrusive.

Input folder:
~/agent/PIRA_Voice/MyVoice

Create a backup folder first, then rewrite the three final files:
- start_msg.m4a
- complete_msg.m4a
- waiting_msg.m4a

Target a softer loudness around -22 LUFS, true peak <= -3 dBTP, and keep the clips short with smooth fade-in/fade-out. Preserve the same filenames. After processing, play or inspect the files if possible and tell me how to roll back from the backup.
```

### 3. Match three clips to a consistent style

```text
Please make these PIRA notification clips feel consistent as one audio set.

Input/output folder:
~/agent/PIRA_Voice/MyVoice

Keep the semantic mapping unchanged:
- start_msg.m4a = PIRA started
- complete_msg.m4a = PIRA finished
- waiting_msg.m4a = PIRA standing by

Postprocess only; do not rewrite the spoken content. Match loudness, remove distracting silence/noise where safely possible, smooth cuts with fades, and keep the waiting clip gentle because it may repeat. Verify the final files and summarize any quality issues you noticed.
```

### 4. Install a prepared custom set

```text
Please install this prepared PIRA audio set for Codex:
~/agent/PIRA_Voice/MyVoice

First verify that start_msg.m4a, complete_msg.m4a, and waiting_msg.m4a exist and are readable. Then use the bundled setup helper for my platform. Preserve existing Codex config where possible and back it up before editing. If config.toml already has a top-level notify entry, stop and show it to me before using --force or -Force. After installation, explain exactly what changed and how to disable or roll back the audio mode.
```

### 5. Diagnose why audio notifications sound wrong

```text
Please diagnose my PIRA Codex audio notifications.

Check the installed audio folder, file names, codec/duration/loudness if tools are available, Codex config notify placement, permission/waiting hooks, and startup wrapper. Do not overwrite config until you explain the likely problem. If a fix is safe and narrow, apply it after a safety review and verify the result.
```

## Voice-content suggestions

If generating or recording new spoken clips, keep wording brief:

- startup: "PIRA is ready." / "PI is online." / "Ready when you are."
- completion: "Done." / "Task complete." / "I finished that."
- waiting: "Standing by." / "I need your confirmation." / "Waiting for you."

Prefer calm, clear delivery over personality-heavy delivery. Notification audio should be easy to recognize once, then easy to ignore.

## Disabling or rolling back

To switch to a different audio set, rerun the relevant setup helper with a different `--audio-dir` or `-AudioDir`.

To disable startup audio only, remove the PIRA-managed startup wrapper block from `~/.zshrc` on macOS or from the PowerShell profile on Windows, or restore the helper-created backup.

To disable completion/waiting notifications, remove the PIRA-managed `notify` and hook blocks from `~/.codex/config.toml`, or restore the helper-created `config.toml.bak.*` backup.
