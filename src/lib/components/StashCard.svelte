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
  import type { StashItem } from "$lib/types";
  import { DesktopStorageAdapter } from "$lib/services/desktop-adapter";
  import { fade, fly } from "svelte/transition";

  let { item, mode } = $props<{ item: StashItem; mode: "Drag" | "Copy" }>();

  const adapter = new DesktopStorageAdapter();
  let copied = $state(false);

  async function handleCopy() {
    const text = `${item.content}\n\nReference Paths:\n${item.files.join("\n")}`;
    await adapter.copyToClipboard(text);
    copied = true;
    setTimeout(() => (copied = false), 2000);
  }

  function handleDragStart(e: DragEvent) {
    if (mode === "Copy") return;

    e.preventDefault();
    // The prompt says: "Attach the list of absolute file paths to the native OS drag event."
    // Since browser cannot do this for local paths easily, we rely on backend.
    adapter.startDrag(item.content, item.files);
  }
</script>

<div
  class="group relative flex flex-col gap-2 rounded-lg border border-border bg-card p-3 shadow-sm hover:shadow-md transition-all hover:border-primary/50"
  transition:fly={{ y: 20, duration: 300 }}
>
  <div class="flex items-start gap-3">
    <!-- Handle -->
    <div
      class="mt-1 flex h-8 w-8 shrink-0 cursor-grab items-center justify-center rounded-md bg-muted text-muted-foreground transition-colors hover:bg-primary hover:text-primary-foreground active:cursor-grabbing"
      role="button"
      tabindex="0"
      draggable={mode === "Drag"}
      ondragstart={handleDragStart}
      onclick={mode === "Copy" ? handleCopy : undefined}
      onkeydown={() => {}}
      title={mode === "Drag" ? "Drag to target" : "Click to copy to clipboard"}
    >
      {#if copied}
        <span class="text-xs">✓</span>
      {:else}
        <span>{mode === "Drag" ? "✋" : "📋"}</span>
      {/if}
    </div>

    <div class="flex-1 min-w-0">
      <div
        class="prose prose-invert prose-xs max-w-none line-clamp-3 text-sm text-foreground/90 leading-relaxed font-sans"
      >
        {item.content || "Empty stash"}
      </div>

      {#if item.files.length > 0}
        <div class="mt-2 flex flex-wrap gap-1.5">
          {#each item.files as file}
            <span
              class="inline-flex items-center rounded-full border border-border bg-secondary/50 px-2 py-0.5 text-[10px] text-muted-foreground truncate max-w-[150px]"
            >
              {file.split(/[\\/]/).pop()}
            </span>
          {/each}
        </div>
      {/if}

      <div class="mt-2 text-[10px] text-muted-foreground/50">
        {new Date(item.createdAt).toLocaleTimeString()}
      </div>
    </div>
  </div>
</div>
