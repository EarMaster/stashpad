<!--
// SPDX-License-Identifier: AGPL-3.0-only

// Copyright (C) 2025 Nico Wiedemann
//
// This file is part of Stashpad.
// Stashpad is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Affero General Public License for more details.
-->

<script lang="ts">
  import { DesktopStorageAdapter } from "$lib/services/desktop-adapter";
  import type { SyncStatus } from "$lib/services/cloud-sync";
  import type { Settings, AIConfig } from "$lib/types";
  import {
    _,
    setLocale,
    SUPPORTED_LOCALES,
    LOCALE_DISPLAY_NAMES,
    type SupportedLocale,
  } from "$lib/i18n";
  import ShortcutInput from "./ShortcutInput.svelte";
  import CloudAuthModal from "./CloudAuthModal.svelte";
  import { fade } from "svelte/transition";
  import { onMount } from "svelte";
  import { APP_VERSION } from "$lib/utils/version";
  import {
    AI_PROVIDER_PRESETS,
    getPresetById,
    getDefaultAIConfig,
    isAppleIntelligencePreset,
    isLocalAIProvider,
  } from "$lib/utils/ai-presets";
  import { aiService } from "$lib/services/ai-service.svelte";
  import {
    Eye,
    EyeOff,
    Loader2,
    Sparkles,
    Check,
    X,
    AlertCircle,
    ExternalLink,
    FileText,
  } from "lucide-svelte";
  import TagBadge from "./TagBadge.svelte";
  import { tooltip } from "$lib/actions/tooltip";
  import { openUrl } from "@tauri-apps/plugin-opener";

  import { getRelativeTime } from "$lib/utils/date";

  let {
    settings = $bindable(),
    syncStatus,
    onBack,
    onOpenContexts,
  } = $props<{
    settings: Settings;
    syncStatus: SyncStatus;
    onBack: () => void;
    onOpenContexts: () => void;
  }>();

  const adapter = new DesktopStorageAdapter();

  async function save() {
    try {
      await adapter.saveSettings(settings);
    } catch (e) {
      console.error("Failed to save settings", e);
    }
  }

  /**
   * Parses text and splits by #tag patterns.
   * Returns array of { type: 'text' | 'tag', value: string }
   */
  function parseTextWithTags(
    text: string,
  ): Array<{ type: "text" | "tag"; value: string }> {
    const parts: Array<{ type: "text" | "tag"; value: string }> = [];
    const regex = /#[\w-]+/g;
    let lastIndex = 0;
    let match;

    while ((match = regex.exec(text)) !== null) {
      // Add text before the match
      if (match.index > lastIndex) {
        parts.push({ type: "text", value: text.slice(lastIndex, match.index) });
      }
      // Add the tag
      parts.push({ type: "tag", value: match[0] });
      lastIndex = regex.lastIndex;
    }

    // Add remaining text
    if (lastIndex < text.length) {
      parts.push({ type: "text", value: text.slice(lastIndex) });
    }

    return parts;
  }

  let isWin10 = $state(false);

  // macOS Screen Recording permission state
  const isMac = navigator.platform.includes("Mac");
  let hasScreenRecordingPermission = $state(true);

  onMount(async () => {
    isWin10 = await adapter.isWindows10();

    // Check macOS Screen Recording permission on mount
    if (isMac) {
      try {
        hasScreenRecordingPermission =
          await adapter.checkScreenRecordingPermission();
      } catch (e) {
        console.error("Failed to check screen recording permission:", e);
      }
    }

    // Load current autostart status
    try {
      const autostartEnabled = await adapter.getAutostartEnabled();
      if (settings.autostart !== autostartEnabled) {
        settings.autostart = autostartEnabled;
        save();
      }
    } catch (e) {
      console.error("Failed to get autostart status:", e);
    }
  });

  /**
   * Opens macOS System Settings to grant Screen Recording permission.
   */
  async function handleGrantScreenRecordingPermission() {
    try {
      await adapter.openMacosScreenRecordingSettings();
    } catch (e) {
      console.error("Failed to open screen recording settings:", e);
    }
  }

  // AI Enhancement state
  let showApiKey = $state(false);
  let testingConnection = $state(false);
  let connectionTestResult = $state<{
    success: boolean;
    message: string;
  } | null>(null);

  // Apple Intelligence availability state
  let appleIntelligenceAvailable = $state(false);

  // Initialize aiConfig if not present
  $effect(() => {
    if (!settings.aiConfig) {
      settings.aiConfig = getDefaultAIConfig();
    }
  });

  onMount(async () => {
    // Check if Apple Intelligence is available on this machine
    try {
      appleIntelligenceAvailable =
        await adapter.checkAppleIntelligenceAvailable();

      // If the user previously selected Apple Intelligence and it's no longer available
      if (
        settings.aiConfig &&
        isAppleIntelligencePreset(settings.aiConfig.presetId) &&
        !appleIntelligenceAvailable
      ) {
        settings.aiConfig.presetId = "openai"; // fallback
        const preset = getPresetById("openai");
        if (preset) {
          settings.aiConfig.endpoint = preset.endpoint;
          settings.aiConfig.model = preset.defaultModel;
        }
        save();
      }
    } catch (e) {
      console.error("Failed to check Apple Intelligence availability:", e);
    }
  });

  /**
   * Handle provider preset selection.
   * Auto-fills endpoint and model from the selected preset.
   */
  function handlePresetChange(presetId: string) {
    const preset = getPresetById(presetId);
    if (preset && settings.aiConfig) {
      settings.aiConfig.presetId = presetId;
      settings.aiConfig.endpoint = preset.endpoint;
      settings.aiConfig.model = preset.defaultModel;
      connectionTestResult = null;
      save();
    }
  }

  /**
   * Test the AI connection with current configuration.
   */
  async function handleTestConnection() {
    if (!settings.aiConfig) return;
    testingConnection = true;
    connectionTestResult = null;
    try {
      await aiService.testConnection(settings.aiConfig);
      connectionTestResult = {
        success: true,
        message: $_("settings.aiEnhancement.testSuccess"),
      };
    } catch (e) {
      let errorMsg = e instanceof Error ? e.message : String(e);

      // Specifically handle "Load failed" which is the common error when fetch is blocked by CORS
      // or the local server is not allowing the origin.
      if (
        errorMsg.includes("Load failed") &&
        isLocalAIProvider(settings.aiConfig)
      ) {
        errorMsg +=
          " (Check if 'Cross-Origin (CORS)' is enabled in LM Studio settings)";
      }

      connectionTestResult = {
        success: false,
        message: $_("settings.aiEnhancement.testFailed", {
          values: { error: errorMsg },
        }),
      };
    } finally {
      testingConnection = false;
    }
  }

  /** Controls visibility of the cloud authorization modal */
  let showAuthModal = $state(false);

  /**
   * Fetches fresh subscription status from the cloud after a successful login.
   */
  async function refreshSubscription() {
    try {
      const config = await adapter.fetchCloudAccount();
      settings.cloudConfig = config;
      save();
    } catch (e) {
      console.error("Failed to fetch subscription", e);
    }
  }

  /** Clears all cloud auth data and disables sync. */
  function handleCloudLogout() {
    if (settings.cloudConfig) {
      settings.cloudConfig.userId = undefined;
      settings.cloudConfig.userId = undefined;
      settings.cloudConfig.email = undefined;
      settings.cloudConfig.subscriptionTier = undefined;
      settings.cloudConfig.subscriptionStatus = undefined;
      settings.cloudConfig.subscriptionPeriodEnd = undefined;
      settings.cloudConfig.enabled = false;
      save();
    }
  }

  /** Opens the Stashpad account portal in the system browser. */
  function openAccountPortal() {
    const endpoint = settings.cloudConfig?.endpoint;
    if (!endpoint) return;
    openUrl(`${endpoint}/account/portal`).catch((err) => {
      console.error("Failed to open account portal:", err);
    });
  }

  /**
   * Called by CloudAuthModal when authorization succeeds.
   * Saves settings and refreshes subscription status.
   */
  async function handleAuthSuccess() {
    showAuthModal = false;
    save();
    await refreshSubscription();
  }
</script>

<div class="h-full flex flex-col bg-background">
  <div
    data-tauri-drag-region
    class="flex items-center gap-3 p-4 border-b border-border bg-muted/20 shrink-0"
  >
    <button
      class="p-2 hover:bg-muted rounded-md text-muted-foreground hover:text-foreground transition-colors"
      onclick={() => {
        // Only trigger cleanup for "after-n-days" strategy (not "on-close" which should only run on restart)
        if (settings.clearCompletedStrategy === "after-n-days") {
          adapter.triggerAutoCleanup();
        }
        onBack();
      }}
      title={$_("settings.backToStash")}
      use:tooltip
    >
      ←
    </button>
    <h1 class="text-xl font-bold tracking-tight">{$_("settings.title")}</h1>
  </div>

  <div class="flex-1 overflow-y-auto p-4 scrollbar-hide">
    <div class="space-y-6 max-w-2xl mx-auto">
      <!-- Navigation to Contexts -->
      <section class="space-y-4">
        <h2
          class="text-sm font-semibold uppercase tracking-wider text-muted-foreground"
        >
          {$_("settings.contextManagement.title")}
        </h2>

        <!-- Auto Context Detection -->
        <div
          class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
        >
          <div class="space-y-0.5">
            <div class="text-sm font-medium">
              {$_("settings.general.autoContextDetection.label")}
            </div>
            <div class="text-xs text-muted-foreground">
              {$_("settings.general.autoContextDetection.description")}
            </div>
          </div>
          <label class="relative inline-flex items-center cursor-pointer">
            <input
              type="checkbox"
              class="sr-only peer"
              bind:checked={settings.autoContextDetection}
              onchange={save}
            />
            <div
              class="w-11 h-6 bg-muted rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-primary peer-focus-visible:ring-2 peer-focus-visible:ring-primary peer-focus-visible:ring-offset-2 peer-focus-visible:ring-offset-background"
            ></div>
          </label>
        </div>

        {#if isMac && settings.autoContextDetection && !hasScreenRecordingPermission}
          <!-- macOS Screen Recording Permission Warning -->
          <div
            class="flex items-start gap-3 p-3 rounded-lg border border-amber-500/30 bg-amber-500/10 ml-4"
            transition:fade={{ duration: 150 }}
          >
            <span class="text-amber-500 text-lg shrink-0 mt-0.5">⚠️</span>
            <div class="flex-1 space-y-1.5">
              <div class="text-xs font-medium text-amber-500">
                {$_(
                  "settings.general.autoContextDetection.macPermissionWarning",
                )}
              </div>
              <div class="text-[10px] text-muted-foreground/80 italic">
                {$_("settings.general.autoContextDetection.restartNote")}
              </div>
              <button
                class="inline-flex items-center gap-1.5 px-3 py-1 rounded-md text-xs font-medium bg-amber-500/20 text-amber-500 hover:bg-amber-500/30 transition-colors"
                onclick={handleGrantScreenRecordingPermission}
              >
                {$_("settings.general.autoContextDetection.grantPermission")}
                <span class="text-[10px]">→</span>
              </button>
            </div>
          </div>
        {/if}

        <button
          class="w-full flex items-center justify-between p-3 rounded-lg border border-border bg-card hover:bg-muted/50 transition-colors group"
          onclick={onOpenContexts}
        >
          <div class="flex flex-col items-start gap-1">
            <span class="text-sm font-medium"
              >{$_("settings.contextManagement.manageContexts")}</span
            >
            <span class="text-xs text-muted-foreground"
              >{$_("settings.contextManagement.description")}</span
            >
          </div>
          <span class="text-muted-foreground group-hover:text-foreground"
            >→</span
          >
        </button>
      </section>

      <!-- Cloud Sync Section -->
      <section class="space-y-4">
        <h2
          class="text-sm font-semibold uppercase tracking-wider text-muted-foreground"
        >
          {$_("settings.cloudSync.title")}
        </h2>

        {#if settings.cloudConfig}
          <!-- Status indicator row -->
          <div
            class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
            transition:fade={{ duration: 150 }}
          >
            <div class="space-y-0.5 flex-1 mr-4">
              <div class="text-sm font-medium">
                {$_("settings.cloudSync.auth.status")}
              </div>
              <div class="text-xs text-muted-foreground">
                {#if settings.cloudConfig.enabled && settings.cloudConfig.userId && syncStatus !== "auth-error"}
                  <!-- Authenticated: green dot + email -->
                  <span class="inline-flex items-center gap-1.5">
                    <span
                      class="w-1.5 h-1.5 rounded-full bg-green-500 shrink-0"
                      aria-hidden="true"
                    ></span>
                    <span class="text-green-500 font-medium">
                      {$_("settings.cloudSync.auth.authenticated")}
                    </span>
                    {#if settings.cloudConfig.email}
                      <span class="text-muted-foreground">
                        — {settings.cloudConfig.email}
                      </span>
                    {/if}
                  </span>
                {:else if settings.cloudConfig.enabled && settings.cloudConfig.userId && syncStatus === "auth-error"}
                  <!-- Auth Error: session expired -->
                  <span class="inline-flex items-center gap-1.5">
                    <AlertCircle size={12} class="text-destructive shrink-0" />
                    <span class="text-destructive font-medium">
                      {$_("settings.cloudSync.auth.sessionExpired")}
                    </span>
                  </span>
                {:else}
                  <!-- Not authenticated: grey dot -->
                  <span class="inline-flex items-center gap-1.5">
                    <span
                      class="w-1.5 h-1.5 rounded-full bg-muted-foreground/50 shrink-0"
                      aria-hidden="true"
                    ></span>
                    <span class="text-muted-foreground">
                      {$_("settings.cloudSync.auth.notAuthenticated")}
                    </span>
                  </span>
                {/if}
              </div>
            </div>

            {#if settings.cloudConfig.enabled && settings.cloudConfig.userId && syncStatus !== "auth-error"}
              <!-- Logged in: logout button -->
              <button
                class="px-4 py-2 rounded-md text-sm font-medium border border-border hover:bg-muted transition-colors"
                onclick={handleCloudLogout}
              >
                {$_("settings.cloudSync.auth.logout")}
              </button>
            {:else if settings.cloudConfig.enabled && settings.cloudConfig.userId && syncStatus === "auth-error"}
              <!-- Logged in but expired: login again button -->
              <button
                class="inline-flex items-center gap-2 px-4 py-2 rounded-md text-sm font-medium bg-primary text-primary-foreground hover:bg-primary/90 transition-colors"
                onclick={() => {
                  showAuthModal = true;
                }}
              >
                {$_("settings.cloudSync.auth.loginAgain")}
              </button>
            {:else}
              <!-- Not logged in: open the auth modal -->
              <button
                class="inline-flex items-center gap-2 px-4 py-2 rounded-md text-sm font-medium bg-primary text-primary-foreground hover:bg-primary/90 transition-colors"
                onclick={() => {
                  showAuthModal = true;
                }}
              >
                {$_("settings.cloudSync.auth.enableButton")}
              </button>
            {/if}
          </div>

          <!-- Subscription Status (shown when logged in) -->
          {#if settings.cloudConfig.enabled && settings.cloudConfig.userId}
            <div
              class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
              transition:fade={{ duration: 150 }}
            >
              <div class="space-y-0.5 flex-1 mr-4">
                <div class="text-sm font-medium">Subscription</div>
                <div class="text-xs text-muted-foreground">
                  {#if settings.cloudConfig.subscriptionTier === "pro"}
                    <span class="text-blue-500 font-medium">Pro</span>
                    {#if settings.cloudConfig.subscriptionStatus === "active"}
                      — Active
                    {:else if settings.cloudConfig.subscriptionStatus}
                      — {settings.cloudConfig.subscriptionStatus}
                    {/if}
                  {:else if settings.cloudConfig.subscriptionTier === "enterprise"}
                    <span class="text-purple-500 font-medium">Enterprise</span>
                    — Active
                  {:else}
                    <span class="text-muted-foreground">Free</span>
                    — Cloud sync requires a subscription
                  {/if}
                </div>
              </div>
              <button
                class="px-4 py-2 rounded-md text-sm font-medium border border-border hover:bg-muted transition-colors"
                onclick={openAccountPortal}
              >
                {settings.cloudConfig.subscriptionTier === "free" ||
                !settings.cloudConfig.subscriptionTier
                  ? "Upgrade"
                  : "Manage"}
              </button>
            </div>

            <!-- Live Sync Status -->
            {#if settings.cloudConfig.enabled}
              <div
                class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
                transition:fade={{ duration: 150 }}
              >
                <div class="space-y-0.5 flex-1 mr-4">
                  <div class="text-sm font-medium">
                    {$_("settings.cloudSync.auth.status")}
                  </div>
                  <div
                    class="text-xs text-muted-foreground flex items-center gap-1.5 pt-0.5"
                  >
                    {#if syncStatus === "syncing"}
                      <Loader2 size={12} class="animate-spin text-blue-500" />
                      <span class="text-blue-500"
                        >{$_("settings.cloudSync.auth.syncing")}</span
                      >
                    {:else if syncStatus === "success"}
                      <Check size={12} class="text-green-500" />
                      <span class="text-green-500"
                        >{$_("settings.cloudSync.auth.syncSuccess")}</span
                      >
                    {:else if syncStatus === "error"}
                      <AlertCircle size={12} class="text-destructive" />
                      <span class="text-destructive"
                        >{$_("settings.cloudSync.auth.syncError")}</span
                      >
                    {:else}
                      <span
                        class="w-1.5 h-1.5 rounded-full bg-muted-foreground/50 shrink-0"
                        aria-hidden="true"
                      ></span>
                      <span class="text-muted-foreground">Idle</span>
                    {/if}
                  </div>
                </div>
                <!-- Optionally show last sync time on the right -->
                <div class="text-xs text-muted-foreground text-right mt-auto">
                  {#if settings.cloudConfig.lastSyncAt}
                    {$_("settings.cloudSync.auth.lastSync", {
                      values: {
                        time: getRelativeTime(
                          settings.cloudConfig.lastSyncAt,
                          $_,
                        ),
                      },
                    })}
                  {:else}
                    {$_("settings.cloudSync.auth.neverSynced")}
                  {/if}
                </div>
              </div>
            {/if}
          {/if}
        {/if}
      </section>

      <!-- General Section -->
      <section class="space-y-4">
        <h2
          class="text-sm font-semibold uppercase tracking-wider text-muted-foreground"
        >
          {$_("settings.general.title")}
        </h2>

        <!-- Language Selector -->
        <div
          class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
        >
          <div class="space-y-0.5">
            <div class="text-sm font-medium">
              {$_("settings.general.language.label")}
            </div>
            <div class="text-xs text-muted-foreground">
              {$_("settings.general.language.description")}
            </div>
          </div>
          <select
            class="bg-muted border border-border rounded-md px-3 py-1.5 text-sm font-medium cursor-pointer outline-none focus:border-primary focus:ring-1 focus:ring-primary transition-colors"
            value={settings.locale ?? "auto"}
            onchange={(e) => {
              const newLocale = e.currentTarget.value as
                | "auto"
                | SupportedLocale;
              settings.locale = newLocale;
              setLocale(newLocale);
              save();
            }}
          >
            <option value="auto">
              {$_("settings.general.language.automatic")}
            </option>
            {#each SUPPORTED_LOCALES as localeCode}
              <option value={localeCode}>
                {LOCALE_DISPLAY_NAMES[localeCode]}
              </option>
            {/each}
          </select>
        </div>

        <!-- New Stash Position Selector -->
        <div
          class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
        >
          <div class="space-y-0.5">
            <div class="text-sm font-medium">
              {$_("settings.general.newStashPosition.label")}
            </div>
            <div class="text-xs text-muted-foreground">
              {$_("settings.general.newStashPosition.description")}
            </div>
            <div class="text-[10px] text-muted-foreground/80 mt-1 italic">
              {$_("settings.general.newStashPosition.shiftModifier")}
            </div>
          </div>
          <div class="flex bg-muted p-1 rounded-lg border border-border">
            {#each ["top", "bottom"] as pos}
              <label
                class="flex items-center gap-2 px-4 py-1.5 rounded-md text-xs font-semibold cursor-pointer transition-all focus-within:ring-2 focus-within:ring-primary focus-within:ring-offset-2 focus-within:ring-offset-background {settings.newStashPosition ===
                pos
                  ? 'bg-primary text-primary-foreground shadow-sm ring-1 ring-primary/20'
                  : 'text-muted-foreground hover:text-foreground hover:bg-accent'}"
              >
                <input
                  type="radio"
                  name="newStashPosition"
                  value={pos}
                  class="sr-only"
                  checked={settings.newStashPosition === pos}
                  onchange={(e) => {
                    settings.newStashPosition = e.currentTarget.value as
                      | "top"
                      | "bottom";
                    save();
                  }}
                />
                {$_(`settings.general.newStashPosition.${pos}`)}
              </label>
            {/each}
          </div>
        </div>

        <!-- Strip Tags on Copy -->
        <div
          class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
        >
          <div class="space-y-0.5">
            <div class="text-sm font-medium">
              {$_("settings.general.stripTagsOnCopy.label")}
            </div>
            <div
              class="text-xs text-muted-foreground inline-flex items-center gap-0.5 flex-wrap"
            >
              {#each parseTextWithTags($_("settings.general.stripTagsOnCopy.description")) as part}
                {#if part.type === "tag"}
                  <TagBadge tag={part.value} size="xs" />
                {:else}
                  {part.value}
                {/if}
              {/each}
            </div>
            <div class="text-[10px] text-muted-foreground/80 mt-1 italic">
              {$_("settings.general.stripTagsOnCopy.shiftModifier")}
            </div>
          </div>
          <label class="relative inline-flex items-center cursor-pointer">
            <input
              type="checkbox"
              class="sr-only peer"
              bind:checked={settings.stripTagsOnCopy}
              onchange={save}
            />
            <div
              class="w-11 h-6 bg-muted rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-primary peer-focus-visible:ring-2 peer-focus-visible:ring-primary peer-focus-visible:ring-offset-2 peer-focus-visible:ring-offset-background"
            ></div>
          </label>
        </div>

        <!-- Paste as Attachment Threshold -->
        <div
          class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
        >
          <div class="space-y-0.5">
            <div class="text-sm font-medium">
              {$_("settings.general.pasteAsAttachment.label")}
            </div>
            <div class="text-xs text-muted-foreground">
              {$_("settings.general.pasteAsAttachment.description")}
            </div>
            <div class="text-[10px] text-muted-foreground/80 mt-1 italic">
              {$_("settings.general.pasteAsAttachment.zeroNote")}
            </div>
            <div class="text-[10px] text-muted-foreground/80 mt-1 italic">
              {$_("settings.general.pasteAsAttachment.shiftModifier")}
            </div>
          </div>
          <div class="flex items-center gap-3">
            <input
              type="number"
              min="0"
              max="1000"
              class="w-20 rounded-md border border-border bg-background px-3 py-1.5 text-sm outline-none focus:border-primary focus:ring-1 focus:ring-primary transition-colors"
              value={settings.pasteAsAttachmentThreshold ?? 8}
              oninput={(e) => {
                const val = parseInt(e.currentTarget.value);
                if (!isNaN(val) && val >= 0) {
                  settings.pasteAsAttachmentThreshold = val;
                  save();
                }
              }}
            />
            <span class="text-xs text-muted-foreground"
              >{$_("settings.general.pasteAsAttachment.unit")}</span
            >
          </div>
        </div>

        <!-- Resize Images Option -->
        <div
          class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
        >
          <div class="space-y-0.5">
            <div class="text-sm font-medium">Resize Images</div>
            <div class="text-xs text-muted-foreground">
              Automatically resize large images to save tokens.
            </div>
            {#if (settings.resizeImages ?? true) === false}
              <div class="text-[10px] text-amber-500 mt-1 font-medium">
                ⚠️ Strongly encouraged to keep enabled to avoid exceeding token
                limits.
              </div>
            {/if}
          </div>
          <label class="relative inline-flex items-center cursor-pointer">
            <input
              type="checkbox"
              class="sr-only peer"
              checked={settings.resizeImages ?? true}
              onchange={() => {
                settings.resizeImages = !(settings.resizeImages ?? true);
                save();
              }}
            />
            <div
              class="w-11 h-6 bg-muted rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-primary peer-focus-visible:ring-2 peer-focus-visible:ring-primary peer-focus-visible:ring-offset-2 peer-focus-visible:ring-offset-background"
            ></div>
          </label>
        </div>

        <!-- Auto Clear Completed -->
        <div
          class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
        >
          <div class="space-y-0.5">
            <div class="text-sm font-medium">
              {$_("settings.clearCompleted.label")}
            </div>
            <div class="text-xs text-muted-foreground">
              {$_("settings.clearCompleted.description")}
            </div>
          </div>
          <div class="flex bg-muted p-1 rounded-lg border border-border">
            {#each ["never", "on-close", "after-n-days"] as strategy}
              <label
                class="flex items-center gap-2 px-3 py-1.5 rounded-md text-xs font-semibold cursor-pointer transition-all focus-within:ring-2 focus-within:ring-primary focus-within:ring-offset-2 focus-within:ring-offset-background {(settings.clearCompletedStrategy ??
                  'never') === strategy
                  ? 'bg-primary text-primary-foreground shadow-sm ring-1 ring-primary/20'
                  : 'text-muted-foreground hover:text-foreground hover:bg-accent'}"
              >
                <input
                  type="radio"
                  name="clearCompletedStrategy"
                  value={strategy}
                  class="sr-only"
                  checked={(settings.clearCompletedStrategy ?? "never") ===
                    strategy}
                  onchange={(e) => {
                    settings.clearCompletedStrategy = e.currentTarget
                      .value as any;
                    save();
                  }}
                />
                {#if strategy === "on-close"}
                  {$_("settings.clearCompleted.onClose")}
                {:else if strategy === "after-n-days"}
                  {$_("settings.clearCompleted.afterNDays").replace(
                    "{days}",
                    (settings.clearCompletedDays ?? 7).toString(),
                  )}
                {:else}
                  {$_("settings.clearCompleted.never")}
                {/if}
              </label>
            {/each}
          </div>
        </div>

        <!-- Clear Completed Days (Conditional) -->
        {#if settings.clearCompletedStrategy === "after-n-days"}
          <div
            class="flex items-center justify-between p-3 rounded-lg border border-border bg-card ml-8"
            transition:fade={{ duration: 150 }}
          >
            <div class="space-y-0.5">
              <div class="text-sm font-medium">
                {$_("settings.clearCompletedDays.label")}
              </div>
              <div class="text-xs text-muted-foreground">
                {$_("settings.clearCompletedDays.description")}
              </div>
            </div>
            <div class="flex items-center gap-3">
              <input
                type="number"
                min="1"
                max="365"
                class="w-20 rounded-md border border-border bg-background px-3 py-1.5 text-sm outline-none focus:border-primary focus:ring-1 focus:ring-primary transition-colors"
                value={settings.clearCompletedDays ?? 7}
                oninput={(e) => {
                  const val = parseInt(e.currentTarget.value);
                  if (!isNaN(val)) {
                    settings.clearCompletedDays = val;
                    save();
                  }
                }}
              />
              <span class="text-xs text-muted-foreground">Days</span>
            </div>
          </div>
        {/if}

        <!-- Autostart -->
        <div
          class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
        >
          <div class="space-y-0.5">
            <div class="text-sm font-medium">
              {$_("settings.general.autostart.label")}
            </div>
            <div class="text-xs text-muted-foreground">
              {$_("settings.general.autostart.description")}
            </div>
          </div>
          <label class="relative inline-flex items-center cursor-pointer">
            <input
              type="checkbox"
              class="sr-only peer"
              bind:checked={settings.autostart}
              onchange={async () => {
                try {
                  await adapter.setAutostart(settings.autostart ?? false);
                  save();
                } catch (e) {
                  console.error("Failed to update autostart:", e);
                  // Revert the toggle if it failed
                  settings.autostart = !settings.autostart;
                }
              }}
            />
            <div
              class="w-11 h-6 bg-muted rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-primary peer-focus-visible:ring-2 peer-focus-visible:ring-primary peer-focus-visible:ring-offset-2 peer-focus-visible:ring-offset-background"
            ></div>
          </label>
        </div>
      </section>

      <!-- Shortcuts Section -->
      <section class="space-y-4">
        <h2
          class="text-sm font-semibold uppercase tracking-wider text-muted-foreground"
        >
          {$_("settings.shortcuts.title")}
        </h2>

        <div class="space-y-3">
          <!-- Local Switching -->
          <div
            class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
          >
            <div class="space-y-0.5">
              <div class="text-sm font-medium">
                {$_("settings.shortcuts.switchContext.label")}
              </div>
              <div class="text-xs text-muted-foreground">
                {$_("settings.shortcuts.switchContext.description")}
              </div>
            </div>
            <ShortcutInput
              value={settings.shortcuts?.["switch_context"] ||
                "CommandOrControl+P"}
              placeholder={$_("settings.shortcuts.clickToSet")}
              onchange={(shortcut) => {
                if (!settings.shortcuts) settings.shortcuts = {};
                settings.shortcuts["switch_context"] = shortcut;
                save();
              }}
            />
          </div>

          <!-- Global Toggle -->
          <div
            class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
          >
            <div class="space-y-0.5">
              <div class="text-sm font-medium">
                {$_("settings.shortcuts.toggleStashpad.label")}
              </div>
              <div class="text-xs text-muted-foreground">
                {$_("settings.shortcuts.toggleStashpad.description")}
              </div>
            </div>
            <ShortcutInput
              value={settings.shortcuts?.["global_toggle"] || ""}
              placeholder={$_("settings.shortcuts.clickToSet")}
              onchange={(shortcut) => {
                if (!settings.shortcuts) settings.shortcuts = {};
                settings.shortcuts["global_toggle"] = shortcut;
                save();
              }}
            />
          </div>
        </div>
      </section>

      <!-- Appearance Section -->
      <section class="space-y-4">
        <h2
          class="text-sm font-semibold uppercase tracking-wider text-muted-foreground"
        >
          {$_("settings.appearance.title")}
        </h2>

        <div
          class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
        >
          <div class="space-y-0.5">
            <div class="text-sm font-medium">
              {$_("settings.appearance.theme.label")}
            </div>
            <div class="text-xs text-muted-foreground">
              {$_("settings.appearance.theme.description")}
            </div>
          </div>
          <div class="flex bg-muted p-1 rounded-lg border border-border">
            {#each ["light", "dark", "system"] as theme}
              <label
                class="flex items-center gap-2 px-3 py-1.5 rounded-md text-xs font-semibold cursor-pointer transition-all focus-within:ring-2 focus-within:ring-primary focus-within:ring-offset-2 focus-within:ring-offset-background {(settings.theme ??
                  'system') === theme
                  ? 'bg-primary text-primary-foreground shadow-sm ring-1 ring-primary/20'
                  : 'text-muted-foreground hover:text-foreground hover:bg-accent'}"
              >
                <input
                  type="radio"
                  name="theme"
                  value={theme}
                  class="sr-only"
                  checked={(settings.theme ?? "system") === theme}
                  onchange={(e) => {
                    settings.theme = e.currentTarget.value as
                      | "light"
                      | "dark"
                      | "system";
                    save();
                  }}
                />
                {$_(`settings.appearance.theme.${theme}`)}
              </label>
            {/each}
          </div>
        </div>

        <div
          class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
        >
          <div class="space-y-0.5">
            <div class="text-sm font-medium">
              {$_("settings.appearance.uiScale.label")}
            </div>
            <div class="text-xs text-muted-foreground">
              {$_("settings.appearance.uiScale.description")}
            </div>
          </div>
          <div class="flex items-center gap-3">
            <input
              type="range"
              min="1"
              max="5"
              step="1"
              class="w-32 h-2 bg-muted rounded-lg appearance-none cursor-pointer accent-primary"
              value={settings.uiScale ?? 3}
              onchange={(e) => {
                const val = parseInt(e.currentTarget.value);
                settings.uiScale = val;
                save();
              }}
            />
          </div>
        </div>

        <div
          class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
        >
          <div class="space-y-0.5">
            <div class="text-sm font-medium">
              {$_("settings.appearance.visualEffects.label")}
            </div>
            <div class="text-xs text-muted-foreground">
              {$_("settings.appearance.visualEffects.description")}
            </div>
            {#if isWin10}
              <div class="text-[10px] text-muted-foreground/80 mt-1 italic">
                {$_("settings.appearance.visualEffects.windows10Note")}
              </div>
            {/if}
          </div>
          <label class="relative inline-flex items-center cursor-pointer">
            <input
              type="checkbox"
              class="sr-only peer"
              checked={settings.visualEffectsEnabled ??
                (isWin10
                  ? false
                  : !window.matchMedia("(prefers-reduced-transparency: reduce)")
                      .matches)}
              onchange={(e) => {
                settings.visualEffectsEnabled = e.currentTarget.checked;
                save();
              }}
            />
            <div
              class="w-11 h-6 bg-muted rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-primary peer-focus-visible:ring-2 peer-focus-visible:ring-primary peer-focus-visible:ring-offset-2 peer-focus-visible:ring-offset-background"
            ></div>
          </label>
        </div>
      </section>

      <!-- AI Enhancement Section -->
      <section class="space-y-4">
        <h2
          class="text-sm font-semibold uppercase tracking-wider text-muted-foreground"
        >
          {$_("settings.aiEnhancement.title")}
        </h2>

        <!-- Enable Toggle -->
        <div
          class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
        >
          <div class="space-y-0.5">
            <div class="text-sm font-medium">
              {$_("settings.aiEnhancement.enable.label")}
            </div>
            <div class="text-xs text-muted-foreground">
              {$_("settings.aiEnhancement.enable.description")}
            </div>
          </div>
          <label class="relative inline-flex items-center cursor-pointer">
            <input
              type="checkbox"
              class="sr-only peer"
              checked={settings.aiConfig?.enabled ?? false}
              onchange={(e) => {
                if (settings.aiConfig) {
                  settings.aiConfig.enabled = e.currentTarget.checked;
                  save();
                }
              }}
            />
            <div
              class="w-11 h-6 bg-muted rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-primary peer-focus-visible:ring-2 peer-focus-visible:ring-primary peer-focus-visible:ring-offset-2 peer-focus-visible:ring-offset-background"
            ></div>
          </label>
        </div>

        {#if settings.aiConfig?.enabled}
          <!-- Provider Selector -->
          <div
            class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
            transition:fade={{ duration: 150 }}
          >
            <div class="space-y-0.5">
              <div class="text-sm font-medium">
                {$_("settings.aiEnhancement.provider.label")}
              </div>
              <div class="text-xs text-muted-foreground">
                {$_("settings.aiEnhancement.provider.description")}
              </div>
            </div>
            <select
              class="bg-muted border border-border rounded-md px-3 py-1.5 text-sm font-medium cursor-pointer outline-none focus:border-primary focus:ring-1 focus:ring-primary transition-colors"
              value={settings.aiConfig?.presetId ?? ""}
              onchange={(e) => handlePresetChange(e.currentTarget.value)}
            >
              <option value="" disabled>{$_("common.select")}</option>
              {#each AI_PROVIDER_PRESETS as preset}
                <!-- Only show Apple Intelligence preset if available -->
                {#if !isAppleIntelligencePreset(preset.id) || appleIntelligenceAvailable}
                  <option value={preset.id}>
                    {preset.id === "custom"
                      ? $_("settings.aiEnhancement.provider.custom")
                      : preset.name}
                  </option>
                {/if}
              {/each}
            </select>
          </div>
          <div
            class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
            transition:fade={{ duration: 150 }}
          >
            <div class="space-y-0.5 flex-1 mr-4">
              <div class="text-sm font-medium">
                {$_("settings.aiEnhancement.systemPrompt.label")}
              </div>
              <div
                class="text-xs text-muted-foreground break-all font-mono opacity-70"
              >
                {aiService.systemPromptPath}
              </div>
              <div class="text-xs text-muted-foreground mt-1">
                {$_("settings.aiEnhancement.systemPrompt.description")}
              </div>
            </div>
            <button
              type="button"
              class="inline-flex items-center gap-2 px-3 py-1.5 rounded-md text-sm font-medium border border-border bg-muted hover:bg-accent hover:text-accent-foreground transition-colors"
              onclick={() => adapter.openSystemPromptFile()}
            >
              <FileText size={14} />
              {$_("settings.aiEnhancement.systemPrompt.edit")}
            </button>
          </div>

          {#if isAppleIntelligencePreset(settings.aiConfig?.presetId)}
            <!-- Apple Intelligence Info Note -->
            <div
              class="flex items-center gap-3 p-4 rounded-lg bg-primary/10 border border-primary/20 text-primary"
              transition:fade={{ duration: 150 }}
            >
              <Sparkles class="w-5 h-5 flex-shrink-0" />
              <div class="text-sm">
                {$_("settings.aiEnhancement.appleIntelligence.info")}
              </div>
            </div>
          {:else}
            <!-- API Endpoint -->
            <div
              class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
              transition:fade={{ duration: 150 }}
            >
              <div class="space-y-0.5 flex-1 mr-4">
                <div class="text-sm font-medium">
                  {$_("settings.aiEnhancement.endpoint.label")}
                </div>
                <div class="text-xs text-muted-foreground">
                  {$_("settings.aiEnhancement.endpoint.description")}
                </div>
              </div>
              <input
                type="text"
                class="w-64 rounded-md border border-border bg-background px-3 py-1.5 text-sm outline-none focus:border-primary focus:ring-1 focus:ring-primary transition-colors"
                placeholder="https://api.openai.com/v1"
                value={settings.aiConfig?.endpoint ?? ""}
                oninput={(e) => {
                  if (settings.aiConfig) {
                    settings.aiConfig.endpoint = e.currentTarget.value;
                    connectionTestResult = null;
                    save();
                  }
                }}
              />
            </div>

            <!-- API Key -->
            <div
              class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
              transition:fade={{ duration: 150 }}
            >
              <div class="space-y-0.5 flex-1 mr-4">
                <div class="text-sm font-medium">
                  {$_("settings.aiEnhancement.apiKey.label")}
                </div>
                <div class="text-xs text-muted-foreground">
                  {$_("settings.aiEnhancement.apiKey.description")}
                </div>
              </div>
              <div class="flex items-center gap-2">
                <input
                  type={showApiKey ? "text" : "password"}
                  class="w-52 rounded-md border border-border bg-background px-3 py-1.5 text-sm outline-none focus:border-primary focus:ring-1 focus:ring-primary transition-colors font-mono"
                  placeholder="sk-..."
                  value={settings.aiConfig?.apiKey ?? ""}
                  oninput={(e) => {
                    if (settings.aiConfig) {
                      settings.aiConfig.apiKey = e.currentTarget.value;
                      connectionTestResult = null;
                      save();
                    }
                  }}
                />
                <button
                  type="button"
                  class="p-1.5 rounded-md text-muted-foreground hover:text-foreground hover:bg-muted transition-colors"
                  onclick={() => (showApiKey = !showApiKey)}
                  title={showApiKey
                    ? $_("settings.aiEnhancement.apiKey.hide")
                    : $_("settings.aiEnhancement.apiKey.show")}
                  use:tooltip
                >
                  {#if showApiKey}
                    <EyeOff size={16} />
                  {:else}
                    <Eye size={16} />
                  {/if}
                </button>
              </div>
            </div>

            <!-- Model Name -->
            <div
              class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
              transition:fade={{ duration: 150 }}
            >
              <div class="space-y-0.5 flex-1 mr-4">
                <div class="text-sm font-medium">
                  {$_("settings.aiEnhancement.model.label")}
                </div>
                <div class="text-xs text-muted-foreground">
                  {$_("settings.aiEnhancement.model.description")}
                </div>
              </div>
              <input
                type="text"
                class="w-64 rounded-md border border-border bg-background px-3 py-1.5 text-sm outline-none focus:border-primary focus:ring-1 focus:ring-primary transition-colors"
                placeholder="gpt-4o-mini"
                value={settings.aiConfig?.model ?? ""}
                oninput={(e) => {
                  if (settings.aiConfig) {
                    settings.aiConfig.model = e.currentTarget.value;
                    connectionTestResult = null;
                    save();
                  }
                }}
              />
            </div>

            <!-- Test Connection -->
            <div
              class="flex items-center justify-end gap-3 p-3"
              transition:fade={{ duration: 150 }}
            >
              {#if connectionTestResult}
                <span
                  class="text-xs {connectionTestResult.success
                    ? 'text-green-500'
                    : 'text-destructive'}"
                  transition:fade
                >
                  {connectionTestResult.message}
                </span>
              {/if}
              <button
                type="button"
                class="inline-flex items-center gap-2 px-4 py-2 rounded-md text-sm font-medium bg-primary text-primary-foreground hover:bg-primary/90 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                onclick={handleTestConnection}
                disabled={testingConnection ||
                  !settings.aiConfig?.endpoint ||
                  !settings.aiConfig?.model}
              >
                {#if testingConnection}
                  <Loader2 size={14} class="animate-spin" />
                {/if}
                {$_("settings.aiEnhancement.testConnection")}
              </button>
            </div>
          {/if}
        {/if}
      </section>

      <!-- About / Footer -->
      <div class="pt-8 pb-4 text-center">
        <div class="text-xs text-muted-foreground space-y-2">
          <p class="font-medium text-foreground/80">
            {$_("app.name")}
            {APP_VERSION}
          </p>
          <p>{$_("app.copyright")}</p>
          <p>{$_("app.license")}</p>
          <div class="pt-2 opacity-50 text-[10px]">
            {$_("app.madeWith")}
          </div>
        </div>
      </div>
    </div>
  </div>
</div>

<!-- Cloud authorization modal – rendered at root level for full-window overlay -->
<!-- Cloud authorization modal – rendered at root level for full-window overlay -->
<CloudAuthModal
  bind:open={showAuthModal}
  bind:settings
  onSuccess={handleAuthSuccess}
  onCancel={() => {
    showAuthModal = false;
  }}
/>
