import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/svelte";
import ElevenLabsEngine from "./elevenlabs-engine.svelte";

describe("ElevenLabsEngine", () => {
  const mockConfig = {
    tts: {
      elevenlabs: {
        api_key: "xi-test",
        voice_id: "21m00Tcm4TlvDq8ikWAM",
        model_id: "eleven_turbo_v2_5",
        output_format: "mp3_44100_128",
        voice_stability: 0.5,
        voice_similarity_boost: 0.75,
        voice_style: 0.0
      }
    }
  } as any;

  it("renders model and format select elements", () => {
    render(ElevenLabsEngine, { localConfig: mockConfig });
    expect(screen.getByLabelText("Model")).toBeTruthy();
    expect(screen.getByLabelText("Format")).toBeTruthy();
  });
});
