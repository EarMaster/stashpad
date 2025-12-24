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
   import { dragHandleZone, TRIGGERS } from "svelte-dnd-action";
   import { flip } from "svelte/animate";
   import { DesktopStorageAdapter } from "$lib/services/desktop-adapter";
   import type { StashItem } from "$lib/types";
   import StashCard from "./StashCard.svelte";
   import { Trash2 } from "lucide-svelte";

   let {
      transferMode,
      refreshTrigger,
      currentContextId,
      contexts,
      onMoveRequest,
   } = $props<{
      transferMode: string;
      refreshTrigger: number;
      currentContextId: string;
      contexts: any[]; // Use any if exact type not imported yet, or import Context
      onMoveRequest: (stash: StashItem) => void;
   }>();

   let stashes = $state<StashItem[]>([]);
   let effectiveMode = $state<"Drag" | "Copy">("Drag");

   // Separate arrays for dnd - these are the source of truth during drag
   let activeStashes = $state<StashItem[]>([]);
   let completedStashes = $state<StashItem[]>([]);
   let draggedItemId = $state<string | null>(null);

   const flipDurationMs = 200;

   const adapter = new DesktopStorageAdapter();

   // Sync stashes to active/completed
   function syncLists() {
      activeStashes = stashes.filter(
         (s) =>
            !s.completed &&
            (s.contextId || "default") === (currentContextId || "default"),
      );
      completedStashes = stashes.filter(
         (s) =>
            s.completed &&
            (s.contextId || "default") === (currentContextId || "default"),
      );
   }

   // Watch for context changes
   $effect(() => {
      // Access currentContextId to track it
      currentContextId;
      syncLists();
   });

   $effect(() => {
      // Trigger load when refreshTrigger changes
      refreshTrigger;
      load();
   });

   async function load() {
      const loaded = await adapter.loadStashes();
      if (loaded) {
         // Filter out any shadow items that might have been persisted
         stashes = loaded.filter(s => !s.isDndShadowItem);
         syncLists();
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

   async function toggleComplete(item: StashItem) {
      const updated = { ...item, completed: !item.completed };
      await adapter.saveStash(updated);
      load();
   }

   async function deleteStash(id: string) {
      await adapter.deleteStash(id);
      load();
   }

   async function updateContent(item: StashItem, content: string) {
      const updated = { ...item, content };
      await adapter.saveStash(updated);
      load();
   }

   async function clearCompleted() {
      await adapter.deleteCompletedStashes();
      load();
   }

   function handleDndConsider(e: CustomEvent) {
      activeStashes = e.detail.items;
      if (e.detail.info?.id) {
         draggedItemId = e.detail.info.id;
      }
   }

   async function handleDndFinalize(e: CustomEvent) {
      activeStashes = e.detail.items;
      draggedItemId = null;

      // Filter out shadow items before saving
      const cleanActiveStashes = activeStashes.filter(s => !s.isDndShadowItem);

      // Rebuild full stashes array with new order and save
      const otherStashes = stashes.filter(
         (s) =>
            s.completed ||
            (s.contextId || "default") !== (currentContextId || "default"),
      );
      stashes = [...cleanActiveStashes, ...otherStashes];
      await adapter.saveStashes(stashes);
   }
</script>

<div
   class="flex-1 overflow-y-auto p-4 pt-0 pb-10 space-y-8 scrollbar-hide"
   role="list"
>
   <!-- Active Section -->
   <section class="space-y-4">
      <div
         class="flex items-center justify-between sticky top-0 py-3 z-10 -mx-4 px-4 mb-2 pointer-events-none"
         style="background: linear-gradient(to bottom, var(--background) 0%, var(--background) 80%, transparent 100%);"
      >
         <h2
            class="text-[10px] font-bold text-muted-foreground uppercase tracking-wider pointer-events-auto"
         >
            Stash Queue ({activeStashes.length})
         </h2>
      </div>

      <div
         class="flex flex-col gap-3 min-h-[50px]"
         use:dragHandleZone={{ items: activeStashes, flipDurationMs }}
         onconsider={handleDndConsider}
         onfinalize={handleDndFinalize}
      >
         {#each activeStashes as item (item.id)}
            <div
               class="dnd-item {draggedItemId === item.id && !item.isDndShadowItem ? 'is-dragging' : ''}"
               role="listitem"
               animate:flip={{ duration: flipDurationMs }}
            >
               <StashCard
                  {item}
                  mode={effectiveMode}
                  onMoveRequest={() => onMoveRequest(item)}
                  onToggleComplete={() => toggleComplete(item)}
                  onDelete={() => deleteStash(item.id)}
                  onUpdateContent={(content) => updateContent(item, content)}
               />
            </div>
         {/each}

         {#if activeStashes.length === 0}
            <div
               class="flex flex-col items-center justify-center py-12 text-muted-foreground/30 border border-dashed border-border/50 rounded-lg"
            >
               <span class="text-sm">No active stashes</span>
            </div>
         {/if}
      </div>
   </section>

   <!-- Completed Section -->
   {#if completedStashes.length > 0}
      <section class="space-y-4">
         <div
            class="flex items-center justify-between sticky top-0 py-3 z-10 -mx-4 px-4 mb-2 pointer-events-none"
            style="background: linear-gradient(to bottom, var(--background) 0%, var(--background) 80%, transparent 100%);"
         >
            <h2
               class="text-[10px] font-bold text-muted-foreground/60 uppercase tracking-wider pointer-events-auto"
            >
               Completed ({completedStashes.length})
            </h2>
            <button
               class="text-[9px] flex items-center gap-1 text-red-400/70 hover:text-red-500 transition-colors bg-red-400/5 px-1.5 py-0.5 rounded border border-red-400/10 pointer-events-auto"
               onclick={clearCompleted}
               title="Delete all completed stashes"
            >
               <Trash2 size={10} /> Clear Completed
            </button>
         </div>

         <div class="flex flex-col gap-3">
            {#each completedStashes as item (item.id)}
               <div role="listitem">
                  <StashCard
                     {item}
                     mode={effectiveMode}
                     showReorderHandle={false}
                     onMoveRequest={() => onMoveRequest(item)}
                     onToggleComplete={() => toggleComplete(item)}
                     onDelete={() => deleteStash(item.id)}
                     onUpdateContent={(content) => updateContent(item, content)}
                  />
               </div>
            {/each}
         </div>
      </section>
   {/if}

   {#if stashes.length === 0}
      <div
         class="flex flex-col items-center justify-center py-10 text-muted-foreground/30 border border-dashed border-border/50 rounded-lg"
      >
         <span class="text-2xl mb-2">📭</span>
         <span class="text-sm">Queue is empty</span>
      </div>
   {/if}
</div>

<style>
   /* svelte-dnd-action styles */
   :global(.dnd-item) {
      transition: transform 0.2s;
   }
   :global(.dnd-item.is-dragging) {
      /* Custom class for dragged items - customize as needed */
      opacity: 0.5;
   }
   :global([aria-grabbed="true"]) {
      opacity: 0.5;
   }
   :global([aria-dropeffect="move"]) {
      outline: 2px solid var(--electric-violet);
      outline-offset: 4px;
      border-radius: 0.5rem;
   }
   /* Style the shadow placeholder to show where item will drop */
   :global([data-is-dnd-shadow-item-internal]) {
      visibility: visible !important;
      opacity: 0.3;
      border: 2px dashed var(--electric-violet);
   }
</style>
