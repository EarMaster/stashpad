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
  import type { AppContext, Settings } from "$lib/types";
  import { onMount } from "svelte";

  let contextInfo = $state<AppContext>({
    windowTitle: "Checking...",
    processName: "",
    detectedContextId: undefined,
  });

  let {
    transferMode = $bindable("Drag"),
    onOpenSettings,
    settings,
    currentContextId = $bindable(),
    onOpenContextSwitcher,
  } = $props<{
    transferMode: string;
    onOpenSettings: () => void;
    settings: Settings;
    currentContextId: string;
    onOpenContextSwitcher: () => void;
  }>();

  const adapter = new DesktopStorageAdapter();

  function updateEffectiveContext() {
    if (settings.autoContextDetection) {
      currentContextId = contextInfo.detectedContextId || "default";
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
    if (id === "default") return "Default";
    return settings.contexts.find((c) => c.id === id)?.name || "Unknown";
  }
</script>

<header
  class="flex h-12 w-full items-center justify-between border-b border-border bg-background/95 px-4 backdrop-blur supports-[backdrop-filter]:bg-background/60"
>
  <div class="flex items-center gap-2 overflow-hidden min-w-[200px]">
    <div
      class="flex h-2 w-2 shrink-0 rounded-full bg-accent"
      class:animate-pulse={settings.autoContextDetection}
    ></div>

    <div class="flex flex-col">
      <span
        class="text-[10px] font-semibold text-muted-foreground uppercase leading-none mb-0.5"
      >
        {settings.autoContextDetection ? "Auto Context" : "Manual Context"}
      </span>

      <button
        class="flex items-center gap-1.5 text-sm font-medium text-foreground hover:bg-muted/50 rounded -ml-1 py-0.5 px-1 transition-colors text-left"
        onclick={onOpenContextSwitcher}
        title={contextInfo.windowTitle}
      >
        <span class="truncate max-w-[150px]"
          >{getContextName(currentContextId || "default")}</span
        >
        <span class="text-muted-foreground text-xs">▼</span>
      </button>
    </div>
  </div>

  <div class="flex items-center rounded-lg border border-input bg-muted p-1">
    {#each ["Drag", "Copy", "Auto"] as mode}
      <button
        class={transferMode === mode
          ? "rounded-md bg-background px-3 py-1 text-xs font-medium shadow-sm transition-all text-primary"
          : "px-3 py-1 text-xs font-medium text-muted-foreground hover:text-foreground transition-all"}
        onclick={() => (transferMode = mode)}
      >
        <span class="mr-1"
          >{mode === "Drag" ? "✋" : mode === "Copy" ? "📋" : "🤖"}</span
        >
        {mode}
      </button>
    {/each}
  </div>

  <button
    class="ml-2 p-1.5 text-muted-foreground hover:text-foreground hover:bg-muted rounded-md transition-colors"
    onclick={onOpenSettings}
    title="Settings"
  >
    ⚙️
  </button>
</header>
