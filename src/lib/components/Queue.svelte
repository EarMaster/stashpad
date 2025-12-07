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
   import type { StashItem } from "$lib/types";
   import StashCard from "./StashCard.svelte";

   let { transferMode, refreshTrigger } = $props<{
      transferMode: string;
      refreshTrigger: number;
   }>();

   let stashes = $state<StashItem[]>([]);
   let effectiveMode = $state<"Drag" | "Copy">("Drag");

   const adapter = new DesktopStorageAdapter();

   $effect(() => {
      // Trigger load when refreshTrigger changes
      refreshTrigger;
      load();
   });

   async function load() {
      const loaded = await adapter.loadStashes();
      if (loaded && loaded.length > 0) {
         // Sort by newest
         stashes = loaded.sort(
            (a, b) =>
               new Date(b.createdAt).getTime() -
               new Date(a.createdAt).getTime(),
         );
      }
   }

   // Resolve mode loop
   $effect(() => {
      resolveMode();
      const interval = setInterval(resolveMode, 500);
      return () => clearInterval(interval);
   });

   async function resolveMode() {
      if (transferMode === "Auto") {
         try {
            const target = await adapter.getSmartTransferTarget();
            effectiveMode = target === "GUI" ? "Drag" : "Copy";
         } catch (e) {
            effectiveMode = "Drag";
         }
      } else {
         effectiveMode = transferMode as "Drag" | "Copy";
      }
   }
</script>

<div class="flex-1 overflow-y-auto p-4 space-y-3 scrollbar-hide">
   <div class="flex items-center justify-between">
      <h2
         class="text-xs font-bold text-muted-foreground uppercase tracking-wider"
      >
         Stash Queue
      </h2>
      <span
         class="text-[10px] text-muted-foreground bg-muted px-2 py-0.5 rounded"
      >
         Mode: <span
            class={effectiveMode === "Copy"
               ? "text-blue-400"
               : "text-orange-400"}>{effectiveMode}</span
         >
      </span>
   </div>

   {#each stashes as item (item.id)}
      <StashCard {item} mode={effectiveMode} />
   {/each}

   {#if stashes.length === 0}
      <div
         class="flex flex-col items-center justify-center py-10 text-muted-foreground/30 border border-dashed border-border/50 rounded-lg"
      >
         <span class="text-2xl mb-2">📭</span>
         <span class="text-sm">Queue is empty</span>
      </div>
   {/if}
</div>
