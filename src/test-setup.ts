import { vi } from "vitest";

// Create a global mock for $app/state before any imports
let mockPathname = "/";

const pageMock = {
  get url() {
    return { pathname: mockPathname };
  }
};

// Stub global for use in tests
vi.stubGlobal("__setMockPathname", (pathname: string) => {
  mockPathname = pathname;
});

// Mock the module
vi.mock("$app/state", () => ({
  page: pageMock
}));

// Initialize locale for svelte-i18n
import { init, addMessages } from "svelte-i18n";
import enTranslations from "./lib/locales/en.json";

addMessages("en", enTranslations);
init({
  fallbackLocale: "en",
  initialLocale: "en"
});

// Mock Tooltip component to avoid context providers in unit tests
vi.mock("$lib/components/ui/info-tooltip.svelte", () => {
  return import("./lib/components/ui/mock-info-tooltip.svelte");
});
