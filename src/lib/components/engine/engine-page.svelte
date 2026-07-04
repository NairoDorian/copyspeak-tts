<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { toast } from "svelte-sonner";
  import { _ } from "svelte-i18n";
  import { ExternalLink, Key, Download, Loader2 } from "@lucide/svelte";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import { openExternal } from "$lib/utils/external-link";
  import type { AppConfig } from "$lib/types";

  type BadgeKind = "default" | "offline" | "free" | "cloud" | "paid" | "freemium";
  type CredentialKind = "none" | "api_key" | "api_key_endpoint";
  type CredentialTarget = "openai" | "elevenlabs" | "cartesia" | "google" | "microsoft";

  // ponytail: tts config fields for google/microsoft/edge exist at runtime
  // (backend + config.rs) but aren't on the TtsConfig TS type yet. Index through
  // a record until the type is widened.
  type TtsFields = Record<string, { api_key?: string; endpoint?: string }>;

  interface EngineTab {
    id: string;
    location: "local" | "cloud";
    badges: BadgeKind[];
    docsUrl: string;
    credential: CredentialKind;
    credentialTarget?: CredentialTarget;
    /** Engine id passed to test_tts_engine_config. */
    testEngine?: string;
    /** Installer id passed to install_engine (uv / engine id). */
    installer?: string;
    /** i18n placeholder key for the API key input. */
    placeholderKey?: string;
  }

  const ENGINE_TABS: EngineTab[] = [
    {
      id: "edge",
      location: "cloud",
      badges: ["default", "cloud", "free"],
      docsUrl: "https://github.com/rany2/edge-tts",
      credential: "none",
      testEngine: "edge",
      installer: "edge"
    },
    {
      id: "cartesia",
      location: "cloud",
      badges: ["cloud", "freemium"],
      docsUrl: "https://docs.cartesia.ai/api-reference/tts/bytes",
      credential: "api_key",
      credentialTarget: "cartesia",
      testEngine: "cartesia",
      placeholderKey: "placeholderCartesia"
    },
    {
      id: "elevenlabs",
      location: "cloud",
      badges: ["cloud", "freemium"],
      docsUrl: "https://elevenlabs.io/docs/api-reference/text-to-speech/convert",
      credential: "api_key",
      credentialTarget: "elevenlabs",
      testEngine: "elevenlabs",
      placeholderKey: "placeholderElevenlabs"
    },
    {
      id: "openai",
      location: "cloud",
      badges: ["cloud", "paid"],
      docsUrl: "https://platform.openai.com/docs/guides/text-to-speech",
      credential: "api_key",
      credentialTarget: "openai",
      testEngine: "openai",
      placeholderKey: "placeholderOpenai"
    },
    {
      id: "google",
      location: "cloud",
      badges: ["cloud", "freemium"],
      docsUrl: "https://ai.google.dev/gemini-api/docs/speech-generation",
      credential: "api_key",
      credentialTarget: "google",
      testEngine: "google",
      placeholderKey: "placeholderGoogle"
    },
    {
      id: "microsoft",
      location: "cloud",
      badges: ["cloud", "paid"],
      docsUrl:
        "https://learn.microsoft.com/en-us/azure/ai-services/speech-service/text-to-speech",
      credential: "api_key_endpoint",
      credentialTarget: "microsoft",
      testEngine: "microsoft",
      placeholderKey: "placeholderMicrosoft"
    },
    {
      id: "kitten",
      location: "local",
      badges: ["offline", "free"],
      docsUrl: "https://github.com/KittenML/KittenTTS",
      credential: "none",
      installer: "kitten"
    },
    {
      id: "piper",
      location: "local",
      badges: ["offline", "free"],
      docsUrl: "https://github.com/OHF-Voice/piper1-gpl",
      credential: "none",
      installer: "piper"
    },
    {
      id: "kokoro",
      location: "local",
      badges: ["offline", "free"],
      docsUrl: "https://github.com/hexgrad/kokoro",
      credential: "none",
      installer: "kokoro"
    },
    {
      id: "pocket",
      location: "local",
      badges: ["offline", "free"],
      docsUrl: "https://github.com/pocket-tts/pocket-tts",
      credential: "none",
      installer: "pocket"
    },
    {
      id: "chatterbox",
      location: "local",
      badges: ["offline", "free"],
      docsUrl: "https://github.com/resemble-ai/chatterbox",
      credential: "none",
      installer: "chatterbox"
    },
    {
      id: "http",
      location: "cloud",
      badges: ["cloud"],
      docsUrl: "https://github.com/CopySpeak/CopySpeak-TTS/blob/main/docs/profile-engine-settings.md",
      credential: "none"
    }
  ];

  const BADGE_STYLES: Record<BadgeKind, string> = {
    default: "bg-emerald-500/15 text-emerald-700 dark:text-emerald-400 ring-1 ring-emerald-500/30",
    offline: "bg-blue-500/15 text-blue-700 dark:text-blue-400 ring-1 ring-blue-500/30",
    free: "bg-green-500/10 text-green-700 dark:text-green-400 ring-1 ring-green-500/25",
    cloud: "bg-violet-500/15 text-violet-700 dark:text-violet-400 ring-1 ring-violet-500/30",
    paid: "bg-amber-500/15 text-amber-700 dark:text-amber-400 ring-1 ring-amber-500/30",
    freemium: "bg-yellow-500/15 text-yellow-700 dark:text-yellow-400 ring-1 ring-yellow-500/30"
  };

  const getBadgeLabel = (badge: BadgeKind): string => $_(`engine.badges.${badge}`);

  let localConfig = $state<AppConfig | null>(null);
  let originalConfig = $state<AppConfig | null>(null);
  let isLoading = $state(true);
  let isSaving = $state(false);
  let activeTab = $state<string>("edge");
  let uvAvailable = $state<boolean | null>(null);
  let installingFor = $state<string | null>(null);

  const active = $derived(ENGINE_TABS.find((t) => t.id === activeTab) ?? ENGINE_TABS[0]);

  const hasChanges = $derived(
    originalConfig !== null &&
      localConfig !== null &&
      JSON.stringify(localConfig) !== JSON.stringify(originalConfig)
  );

  function tts(): TtsFields | null {
    return localConfig ? (localConfig.tts as unknown as TtsFields) : null;
  }

  async function loadConfig() {
    isLoading = true;
    try {
      const config = await invoke<AppConfig>("get_config");
      localConfig = JSON.parse(JSON.stringify(config));
      originalConfig = JSON.parse(JSON.stringify(config));
      const uv = await invoke<{ available: boolean }>("check_command_exists", { command: "uv" });
      uvAvailable = uv.available;
    } catch (e) {
      console.error("Failed to load config:", e);
      toast.error("Failed to load configuration");
    } finally {
      isLoading = false;
    }
  }

  async function saveConfig() {
    if (!localConfig) return;
    isSaving = true;
    try {
      await invoke("set_config", { newConfig: localConfig });
      originalConfig = JSON.parse(JSON.stringify(localConfig));
      toast.success("Engine settings saved");
    } catch (e) {
      console.error("Failed to save config:", e);
      toast.error(`Failed to save settings: ${e}`);
    } finally {
      isSaving = false;
    }
  }

  function cancelChanges() {
    if (!originalConfig) return;
    localConfig = JSON.parse(JSON.stringify(originalConfig));
  }

  async function runInstaller(id: string) {
    installingFor = id;
    try {
      await invoke("install_engine", { engine: id });
      const uv = await invoke<{ available: boolean }>("check_command_exists", { command: "uv" });
      uvAvailable = uv.available;
      toast.success(`${id} installer launched`);
    } catch (e) {
      toast.error(`Install failed: ${e}`);
    } finally {
      installingFor = null;
    }
  }

  function handleExternalLinkClick(e: Event, url: string) {
    e.preventDefault();
    openExternal(url);
  }

  onMount(() => {
    loadConfig();
  });
</script>

<div class="w-full">
  {#if isLoading}
    <div class="flex min-h-[60vh] items-center justify-center">
      <div class="text-center">
        <Loader2 class="text-primary mx-auto mb-4 h-8 w-8 animate-spin" />
        <p class="text-muted-foreground">{$_("engine.loading")}</p>
      </div>
    </div>
  {:else if localConfig}
    {#if uvAvailable === false}
      <div
        class="border-amber-500/30 bg-amber-500/10 mb-4 flex items-center justify-between gap-3 rounded-md border p-3"
      >
        <p class="text-amber-700 text-sm dark:text-amber-400">
          {$_("engine.setup.uvMissing")}
        </p>
        <Button variant="outline" size="sm" onclick={() => runInstaller("uv")}>
          <Download size={14} class="mr-2" />
          {$_("engine.setup.installUv")}
        </Button>
      </div>
    {/if}

    <div class="flex flex-row items-start gap-2">
      <aside class="w-28 shrink-0 self-stretch">
        <nav class="sticky top-0 space-y-0.5">
          {#each ENGINE_TABS as tab (tab.id)}
            <button
              class="w-full rounded-md px-2 py-1.5 text-left text-sm transition-colors {activeTab ===
              tab.id
                ? "bg-primary/10 text-primary border-primary border-l-2 font-medium"
                : "text-muted-foreground hover:text-foreground hover:bg-muted/50"}"
              onclick={() => (activeTab = tab.id)}
            >
              {$_(`engine.${tab.id}.title`)}
            </button>
          {/each}
        </nav>
      </aside>

      <main class="flex-1 space-y-6 pb-20">
        <section class="border-border overflow-hidden rounded-lg border">
          <div class="bg-muted/50 border-border border-b p-4">
            <div class="flex flex-wrap items-center gap-2">
              <h2 class="text-lg font-semibold">{$_(`engine.${active.id}.title`)}</h2>
              {#each active.badges as badge (badge)}
                <span
                  class="rounded-full px-2 py-0.5 text-[11px] font-medium {BADGE_STYLES[badge]}"
                >
                  {getBadgeLabel(badge)}
                </span>
              {/each}
            </div>
            <p class="text-muted-foreground mt-1 text-sm">
              {$_(`engine.${active.id}.description`)}
            </p>
            <button
              onclick={(e) => handleExternalLinkClick(e, active.docsUrl)}
              class="text-muted-foreground hover:text-foreground mt-2 flex cursor-pointer items-center gap-1 text-xs transition-colors"
            >
              <ExternalLink size={12} />
              {active.docsUrl}
            </button>
          </div>

          <div class="space-y-4 p-4">
            {#if active.credential !== "none" && active.credentialTarget}
              <div class="space-y-2">
                <Label for="api-key">{$_("engine.apiSetup.apiKey")}</Label>
                <div class="flex items-center gap-2">
                  <Key size={14} class="text-muted-foreground shrink-0" />
                  <Input
                    id="api-key"
                    type="password"
                    placeholder={active.placeholderKey
                      ? $_(`engine.apiSetup.${active.placeholderKey}`)
                      : ""}
                    value={tts()?.[active.credentialTarget]?.api_key ?? ""}
                    oninput={(e) => {
                      const t = tts();
                      if (t && active.credentialTarget) {
                        t[active.credentialTarget].api_key = e.currentTarget.value;
                      }
                    }}
                  />
                </div>
              </div>
            {/if}

            {#if active.credential === "api_key_endpoint" && active.credentialTarget}
              <div class="space-y-2">
                <Label for="endpoint">{$_("engine.apiSetup.endpointLabel")}</Label>
                <Input
                  id="endpoint"
                  type="text"
                  placeholder={$_("engine.apiSetup.endpointPlaceholder")}
                  value={tts()?.[active.credentialTarget]?.endpoint ?? ""}
                  oninput={(e) => {
                    const t = tts();
                    if (t && active.credentialTarget) {
                      t[active.credentialTarget].endpoint = e.currentTarget.value;
                    }
                  }}
                />
              </div>
            {/if}

            {#if active.installer}
              <div class="flex items-center gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  disabled={installingFor === active.installer}
                  onclick={() => runInstaller(active.installer!)}
                >
                  {#if installingFor === active.installer}
                    <Loader2 size={14} class="mr-2 animate-spin" />
                    {$_("engine.setup.installing")}
                  {:else}
                    <Download size={14} class="mr-2" />
                    {$_("engine.setup.install")}
                  {/if}
                </Button>
              </div>
            {/if}
          </div>
        </section>
      </main>
    </div>

    {#if hasChanges}
      <div
        class="border-border bg-card fixed right-4 bottom-12 z-60 flex items-center gap-3 border px-4 py-2.5 shadow-lg"
      >
        <Button
          size="sm"
          variant="ghost"
          onclick={cancelChanges}
          disabled={isSaving}
          class="h-8 px-3"
        >
          {$_("engine.saveBar.cancel")}
        </Button>
        <Button size="sm" onclick={saveConfig} disabled={isSaving} class="h-8 px-4">
          {isSaving ? $_("engine.saveBar.saving") : $_("engine.saveBar.saveChanges")}
        </Button>
      </div>
    {/if}
  {:else}
    <div class="flex min-h-[60vh] items-center justify-center px-6">
      <div class="mx-auto w-full max-w-sm text-center">
        <h2 class="mb-2 text-xl font-semibold">{$_("engine.error.loadFailed")}</h2>
        <p class="text-muted-foreground mb-4">{$_("engine.error.loadDescription")}</p>
        <Button onclick={loadConfig}>{$_("engine.setup.tryAgain")}</Button>
      </div>
    </div>
  {/if}
</div>
