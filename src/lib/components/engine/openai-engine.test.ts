import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/svelte";
import OpenAiEngine from "./openai-engine.svelte";

describe("OpenAiEngine", () => {
  const mockConfig = {
    tts: {
      openai: { model: "tts-1", voice: "Alloy" }
    }
  } as any;

  it("renders model and voice select elements", () => {
    render(OpenAiEngine, { localConfig: mockConfig });
    expect(screen.getByLabelText("Model")).toBeTruthy();
    expect(screen.getByLabelText("Voice")).toBeTruthy();
  });
});
