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
  import type { AppContext, Settings, Context } from "$lib/types";
  import { _ } from "$lib/i18n";
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import logoIcon from "../../../assets/stashpad/Icon-Darkmode.svg";
  import logoIconLight from "../../../assets/stashpad/Icon.svg";
  import logoTypo from "../../../assets/stashpad/Typo.svg";

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
  } = $props<{
    transferMode: string;
    onOpenSettings: () => void;
    settings: Settings;
    contexts: Context[];
    currentContextId: string;
    onOpenContextSwitcher: () => void;
  }>();

  const adapter = new DesktopStorageAdapter();

  function updateEffectiveContext() {
    if (settings.autoContextDetection) {
      const detectedId = contextInfo.detectedContextId || "default";
      currentContextId = detectedId;

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
      currentContextId = settings.activeContextId || "default";
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
        title={contextInfo.windowTitle}
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
    <button
      class="p-1.5 text-muted-foreground hover:text-foreground hover:bg-muted rounded-md transition-colors pointer-events-auto"
      onclick={onOpenSettings}
      title={$_("header.settings")}
    >
      ⚙️
    </button>
    <div class="w-px h-4 bg-border mx-1"></div>
    <button
      class="p-1.5 text-muted-foreground hover:text-foreground hover:bg-muted rounded-md transition-colors pointer-events-auto"
      onclick={() => getCurrentWindow().minimize()}
      title={$_("common.minimize")}
    >
      ─
    </button>
    <button
      class="p-1.5 text-muted-foreground hover:text-destructive hover:bg-destructive/10 rounded-md transition-colors pointer-events-auto"
      onclick={() => getCurrentWindow().close()}
      title={$_("common.close")}
    >
      ✕
    </button>
  </div>
</header>
