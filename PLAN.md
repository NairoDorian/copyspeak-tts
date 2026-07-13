# CopySpeak Fork Refinement Plan

This plan documents the changes to align the CopySpeak fork after merging upstream updates and addressing the final set of performance, testing, and lifecycle gaps.

## 1. Unified Playback Streaming (H7 / speak_now Integration)
- **Goal**: Enable direct streaming playback for all long texts (including those initiated from the Play page UI).
- **Implementation**: Refactor `speak_now_internal` so that when pagination criteria are met and file output is disabled, it drops its locks and routes directly to a new `speak_queued_internal` helper (bypassing redundant LLM post-processing), mimicking the global hotkey streaming path.

## 2. Fast Health Check Probe (H5 / Stopped State Optimization)
- **Goal**: Prevent health checks from loading models (which takes several seconds and allocates RAM/VRAM) when the Piper server is stopped.
- **Implementation**: Instead of running a test synthesis on the Stopped path, execute a fast python check `python -c "import piper"` (or local equivalent with `uv run`) to verify python and module availability.

## 3. Core Component Test Suite Restoration (H3)
- **Goal**: Restore Svelte component tests for OpenAI and ElevenLabs to verify proper UI rendering, key validation, and state logic.
- **Implementation**: Port the 28 tests originally dropped during the merge back into `openai-engine.test.ts` and `elevenlabs-engine.test.ts` using Svelte 5 syntax and the updated vitest mock environment.

## 4. Audio Quality and Formatting Guards (M1 / M2 / C5)
- **Goal**: Add regression unit tests for WAV formatting limits.
- **Implementation**: Add tests validating that format mismatches are rejected on WAV concatenation, and verify robust parsing and duration estimation under truncated/corrupted inputs.

## 5. Voice List Deduplication (Q2)
- **Goal**: Remove hardcoded voice name lists in `cli.rs` and consolidate them into a single source of truth constant.
