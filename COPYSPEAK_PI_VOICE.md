# CopySpeak Pi Extension

Project-local Pi extension: `.pi/extensions/copyspeak/`.

## What it does

- Speaks final Pi assistant responses through the running CopySpeak app by default.
- Optionally speaks agent/tool activity and thinking blocks.
- Triggers speech through CopySpeak's local control server, not the Windows clipboard.

## Setup

Set any API keys and start Pi from this repository:

```powershell
$env:CARTESIA_API_KEY="..."
$env:COPYSPEAK_PI_ENGINE="cartesia" # optional override; omit to use app settings
pi
```

Start CopySpeak yourself before using the extension. If `COPYSPEAK_PI_LAUNCH=1` is set, the extension looks for release/debug `copyspeak.exe` under `src-tauri/target/` and launches it when needed.

## Commands

```text
/copyspeak status
/copyspeak on
/copyspeak off
/copyspeak test hello from pi
/copyspeak engine cartesia|openai|elevenlabs|local
/copyspeak activity on|off
/copyspeak assistant on|off
/copyspeak thinking on|off
```

## Environment flags

- `COPYSPEAK_PI_ENABLED=0` disables voice on startup.
- `COPYSPEAK_PI_ENGINE=cartesia|openai|elevenlabs|local` overrides the running app engine.
- `COPYSPEAK_PI_EFFECT=walkie_talkie` overrides the running app effect.
- `COPYSPEAK_PI_ASSISTANT=0` disables final assistant-message speech.
- `COPYSPEAK_PI_ACTIVITY=1` enables thinking/tool announcements.
- `COPYSPEAK_PI_THINKING=0` disables spoken thinking blocks.
- `COPYSPEAK_PI_MAX_CHARS=700` limits final response speech length.
- `COPYSPEAK_PI_LAUNCH=1` enables auto-launching CopySpeak.
- `COPYSPEAK_CONTROL_URL=http://127.0.0.1:43117/speak` overrides the local control endpoint.
