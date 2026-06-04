# AZ_AgentZero — Source Export (Zip)

Creates a clean source-only `.zip` of the project excluding all build artifacts, temp files, and generated content.

## Usage

### PowerShell Script (recommended)

```powershell
.\export-zip.ps1
```

Custom output path:

```powershell
.\export-zip.ps1 -OutputPath "D:\backups\az-agentzero-v0.3.zip"
```

### Direct 7-Zip Command

```powershell
7z a -tzip "..\AZ_AgentZero.zip" ".\*" `
    -xr!".git" -xr!"models" -xr!"node_modules" -xr!"venv" -xr!".venv" `
    -xr!"target" -xr!"dist" -xr!"build" -xr!"__pycache__" -xr!"*.pyc" `
    -xr!".setup-cache" -xr!"*.log" -xr!"Cargo.lock" -xr!"package-lock.json" `
    -xr!"*.tar.gz" -xr!".DS_Store" -xr!"Thumbs.db" -xr!"debug" -xr!"release" `
    -xr!"*.exe" -xr!"*.dll" -xr!"*.pdb" -xr!"*.o" -xr!"*.so" -xr!"*.dylib"
```

### What's Excluded

| Pattern | Reason |
|---------|--------|
| `.git/` | Version control metadata |
| `models/` | Large STT/TTS model binaries (re-downloadable) |
| `node_modules/` | npm packages |
| `venv/`, `.venv/` | Python virtual environment |
| `target/` | Rust build artifacts |
| `dist/`, `build/` | Compiled frontend/build output |
| `__pycache__/`, `*.pyc` | Python bytecode cache |
| `.setup-cache/` | Setup completion markers |
| `Cargo.lock`, `package-lock.json` | Lock files (avoid merge conflicts in zips) |
| `*.exe`, `*.dll`, `*.pdb`, `*.o`, `*.so`, `*.dylib` | Native binaries/objects |
| `*.tar.gz`, `*.log` | Downloaded archives, logs |
| `debug/`, `release/` | Build profile output dirs |
| `.DS_Store`, `Thumbs.db` | OS metadata files |

### What's Included

All source code, configuration, documentation, and `.md` files across all 7 projects:

- `Agent_Zero/` — Main Tauri + SolidJS application
- `copyspeak-tts/`, `Handy/`, `parrot/`, `whispering/` — Helper projects
- `Parakeet-Realtime-Transcriber/` — STT engine
- `TranscriptionSuite/` — Transcription utilities
- Root-level: `*.md`, `*.json`, `repomix.config.json`
