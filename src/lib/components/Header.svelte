<!--
// SPDX-License-Identifier: AGPL-3.0-only

// Copyright (C) 2025 Nico Wiedemann
//
// This file is part of Stashpad.
// Stashpad is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License, version 3,
// as published by the Free Software Foundation.
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Affero General Public License for more details.
-->

<script lang="ts">
  import { DesktopStorageAdapter } from "$lib/services/desktop-adapter";
  import type { AppContext, Settings, Context } from "$lib/types";
  import type { SyncStatus } from "$lib/services/cloud-sync";
  import { _ } from "$lib/i18n";
  import { createSyncDisplay } from "$lib/utils/sync-display.svelte";
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import {
    Settings as SettingsIcon,
    Minus,
    X,
    Cloud,
    CloudOff,
    Check,
    Loader2,
    ArrowDownToLine,
  } from "lucide-svelte";
  import logoIcon from "../../../assets/stashpad/Icon-Darkmode.svg";
  import logoIconLight from "../../../assets/stashpad/Icon.svg";
  import logoTypo from "../../../assets/stashpad/Typo.svg";
  import { tooltip } from "$lib/actions/tooltip";

  let contextInfo = $state<AppContext>({
    windowTitle: $_("header.checking"),
    processName: "",
    detectedContextId: undefined,
  });

  let {
    transferMode = $bindable("Drag"),
    onOpenSettings,
    settings,
    contexts,
    currentContextId = $bindable(),
    onOpenContextSwitcher,
    autoDetectedWindowTitle = $bindable(),
    syncStatus = "idle",
    syncStatusMessage = "",
    updateAvailable = false,
    updateVersion,
    onShowUpdateNotice,
  } = $props<{
    transferMode: string;
    onOpenSettings: () => void;
    settings: Settings;
    contexts: Context[];
    currentContextId: string;
    onOpenContextSwitcher: () => void;
    /** Window title - only set when auto-detection matched a context */
    autoDetectedWindowTitle?: string;
    syncStatus?: SyncStatus;
    syncStatusMessage?: string;
    /** A newer version exists and the user has not chosen to sit it out */
    updateAvailable?: boolean;
    updateVersion?: string;
    onShowUpdateNotice?: () => void;
  }>();

  /** Sync state is only worth a slot in the header once the user has cloud sync on. */
  let showSyncStatus = $derived(
    !!settings.cloudConfig?.enabled && !!settings.cloudConfig?.userId,
  );

  // Displayed state, not the live one: a sync that finishes quickly never shows up,
  // so the icon stops flipping every few seconds. See createSyncDisplay().
  const syncDisplay = createSyncDisplay(() => syncStatus);

  let syncLabel = $derived.by(() => {
    switch (syncDisplay.current) {
      case "syncing":
        return $_("settings.cloudSync.auth.syncing");
      case "success":
        return $_("settings.cloudSync.auth.syncSuccess");
      case "error":
        return syncStatusMessage
          ? `${$_("settings.cloudSync.auth.syncError")} (${syncStatusMessage})`
          : $_("settings.cloudSync.auth.syncError");
      case "auth-error":
        return $_("settings.cloudSync.auth.loginAgain");
      default:
        return $_("settings.cloudSync.title");
    }
  });

  const adapter = new DesktopStorageAdapter();

  function updateEffectiveContext() {
    if (settings.autoContextDetection) {
      // Only switch context if Auto Detection found an actual match.
      // If no match (undefined), keep the last selected context.
      if (contextInfo.detectedContextId) {
        const detectedId = contextInfo.detectedContextId;
        currentContextId = detectedId;

        // Expose window title for AI enhancement when auto-detection matched
        autoDetectedWindowTitle = contextInfo.windowTitle;

        // Persist the detected context to settings so it's restored on app restart
        if (settings.activeContextId !== detectedId) {
          settings.activeContextId = detectedId;
          adapter.saveSettings(settings);

          // Update lastUsed timestamp for the detected context
          const ctx = contexts.find((c) => c.id === detectedId);
          if (ctx) {
            ctx.lastUsed = new Date().toISOString();
            adapter.saveContext(ctx);
          }
        }
      } else {
        // No match detected: keep the current context (don't switch)
        // Use the already set activeContextId or default if none is set yet
        currentContextId = settings.activeContextId || "default";
        // Clear window title when no auto-detection match
        autoDetectedWindowTitle = undefined;
      }
    } else {
      currentContextId = settings.activeContextId || "default";
      // Auto-detection is disabled, so don't expose window title
      autoDetectedWindowTitle = undefined;
    }
  }

  $effect(() => {
    // Re-run when dependencies change
    settings;
    contextInfo;
    updateEffectiveContext();
  });

  onMount(() => {
    const interval = setInterval(async () => {
      try {
        contextInfo = await adapter.getPreviousAppInfo();
      } catch (e) {
        console.error(e);
      }
    }, 1000);
    return () => clearInterval(interval);
  });

  function getContextName(id: string) {
    if (id === "default") return $_("common.default");
    return contexts.find((c) => c.id === id)?.name || $_("common.unknown");
  }
</script>

<header
  class="relative flex mt-1 h-12 w-full items-center justify-between border-b border-border bg-background/95 px-4 backdrop-blur supports-[backdrop-filter]:bg-background/60 z-50 select-none"
>
  <!-- Window Drag Area -->
  <div data-tauri-drag-region class="absolute inset-0 z-0"></div>

  <!-- Left side: Context Display -->
  <div
    class="relative z-10 flex items-center gap-3 overflow-hidden pointer-events-none"
  >
    <div
      class="flex h-2 w-2 shrink-0 rounded-full transition-colors {settings.autoContextDetection
        ? 'bg-primary dark:bg-[var(--amber)]'
        : 'bg-[#27272a] dark:bg-[#d8d8d9]'}"
      class:animate-pulse={settings.autoContextDetection}
    ></div>

    <div class="flex flex-col">
      <span
        class="text-[8px] font-semibold text-muted-foreground uppercase leading-none mb-0.5"
      >
        {settings.autoContextDetection
          ? $_("header.autoContext")
          : $_("header.manualContext")}:
      </span>

      <button
        class="flex items-center gap-1.5 text-sm font-medium text-foreground hover:bg-muted/50 rounded -ml-1 py-0.5 px-1 transition-colors text-left pointer-events-auto"
        onclick={onOpenContextSwitcher}
        title={settings.autoContextDetection
          ? contextInfo.windowTitle
          : $_("contextSwitcher.selectContext")}
        use:tooltip
      >
        <span class="truncate max-w-[150px] lg:max-w-[200px]">
          {getContextName(currentContextId || "default")}
        </span>
        <span class="text-muted-foreground text-xs">▼</span>
      </button>
    </div>
  </div>

  <!-- Center: Brand Logo (hidden automatically when narrow) -->
  <div
    class="z-10 absolute left-1/2 -translate-x-1/2 hidden sm:flex items-center gap-1.5 shrink-0 select-none cursor-default py-2 pointer-events-none"
  >
    <!-- Logo -->
    <img src={logoIcon} alt="{$_('app.name')} Icon" class="h-8 w-8 block" />
    <!-- Typo (Inverted in light mode) -->
    <img
      src={logoTypo}
      alt={$_("app.name")}
      class="h-7 invert dark:invert-0 transition-all"
    />
  </div>

  <!-- Right Side: Window Controls -->
  <div class="relative z-10 flex items-center gap-1 shrink-0">
    {#if updateAvailable}
      <!-- Deliberately quiet: an update is worth noticing, not worth interrupting. The
           details and every action live in the popover this opens. -->
      <button
        class="p-1.5 rounded-md transition-colors pointer-events-auto hover:bg-muted"
        onclick={onShowUpdateNotice}
        title={$_("header.updateAvailable", {
          values: { version: updateVersion ?? "" },
        })}
        aria-label={$_("header.updateAvailable", {
          values: { version: updateVersion ?? "" },
        })}
        use:tooltip
      >
        <ArrowDownToLine size={16} class="text-primary dark:text-[var(--amber)]" />
      </button>
    {/if}
    {#if showSyncStatus}
      <!-- Sync state, shown only once cloud sync is on. Opens Settings, which is where
           the details and the manual trigger live. -->
      <button
        class="p-1.5 rounded-md transition-colors pointer-events-auto hover:bg-muted"
        onclick={onOpenSettings}
        title={syncLabel}
        aria-label={syncLabel}
        use:tooltip
      >
        {#if syncDisplay.current === "syncing"}
          <Loader2 size={16} class="animate-spin text-blue-500" />
        {:else if syncDisplay.current === "success"}
          <Check size={16} class="text-green-500" />
        {:else if syncDisplay.current === "error"}
          <CloudOff size={16} class="text-red-500" />
        {:else if syncDisplay.current === "auth-error"}
          <CloudOff size={16} class="text-amber-500" />
        {:else}
          <Cloud size={16} class="text-muted-foreground" />
        {/if}
      </button>
    {/if}
    <button
      class="p-1.5 text-muted-foreground hover:text-foreground hover:bg-muted rounded-md transition-colors pointer-events-auto"
      onclick={onOpenSettings}
      title={$_("header.settings")}
      use:tooltip
    >
      <SettingsIcon size={16} />
    </button>
    <div class="w-px h-4 bg-border mx-1"></div>
    <button
      class="p-1.5 text-muted-foreground hover:text-foreground hover:bg-muted rounded-md transition-colors pointer-events-auto"
      onclick={() => getCurrentWindow().minimize()}
      title={$_("common.minimize")}
      use:tooltip
    >
      <Minus size={16} />
    </button>
    <button
      class="p-1.5 text-muted-foreground hover:text-destructive hover:bg-destructive/10 rounded-md transition-colors pointer-events-auto"
      onclick={() => getCurrentWindow().close()}
      title={$_("common.close")}
      use:tooltip
    >
      <X size={16} />
    </button>
  </div>
</header>
