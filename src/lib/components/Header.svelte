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
  import type { AppContext } from "$lib/types";
  import { onMount } from "svelte";

  let context = $state<AppContext>({
    windowTitle: "Checking...",
    processName: "",
  });
  let { transferMode = $bindable("Drag") } = $props<{ transferMode: string }>();

  const adapter = new DesktopStorageAdapter();

  onMount(() => {
    const interval = setInterval(async () => {
      try {
        context = await adapter.getPreviousAppInfo();
      } catch (e) {
        console.error(e);
      }
    }, 1000);
    return () => clearInterval(interval);
  });
</script>

<header
  class="flex h-12 w-full items-center justify-between border-b border-border bg-background/95 px-4 backdrop-blur supports-[backdrop-filter]:bg-background/60"
>
  <div class="flex items-center gap-2 overflow-hidden">
    <span class="flex h-2 w-2 rounded-full bg-accent animate-pulse"></span>
    <span class="text-sm font-semibold text-muted-foreground whitespace-nowrap"
      >Active Context:</span
    >
    <span
      class="text-xs text-foreground truncate max-w-[200px]"
      title={context.windowTitle}
    >
      {context.windowTitle}
      <span class="opacity-50">({context.processName})</span>
    </span>
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
</header>
