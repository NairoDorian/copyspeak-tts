import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { svelteTesting } from "@testing-library/svelte/vite";
import path from "node:path";

export default defineConfig({
  plugins: [
    svelte({ hot: !process.env.VITEST, compilerOptions: { css: "injected" } }),
    svelteTesting()
  ],
  test: {
    include: ["src/**/*.{test,spec}.{js,ts}"],
    environment: "happy-dom",
    setupFiles: ["./src/test-setup.ts"],
    globals: true,
    ssr: {
      noExternal: true
    }
  },
  server: {
    deps: {
      inline: ["svelte", "@lucide/svelte"]
    }
  },
  resolve: {
    alias: {
      $lib: path.resolve("./src/lib"),
      "$lib/*": path.resolve("./src/lib/*"),
      "$app/navigation": path.resolve("./src/lib/mocks/app-navigation.ts"),
      "$app/state": path.resolve("./src/lib/mocks/app-state.ts")
    }
  }
});
