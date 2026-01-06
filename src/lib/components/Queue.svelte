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
   import { tick, untrack } from "svelte";
   import { dragHandleZone, TRIGGERS } from "svelte-dnd-action";
   import { getTagHue } from "$lib/utils/markdown";
   import { flip } from "svelte/animate";
   import { fade, fly } from "svelte/transition";
   import { DesktopStorageAdapter } from "$lib/services/desktop-adapter";
   import type { StashItem, Context } from "$lib/types";
   import { _ } from "$lib/i18n";
   import StashCard from "./StashCard.svelte";
   import ConfirmationDialog from "./ConfirmationDialog.svelte";
   import {
      Trash2,
      ArrowUp,
      ArrowDown,
      Check,
      MoreVertical,
      CalendarArrowUp,
      CalendarArrowDown,
      CheckCheck,
      RotateCcw,
      Filter,
   } from "lucide-svelte";

   let {
      transferMode,
      refreshTrigger,
      currentContextId,
      contexts,
      onMoveRequest,
      newStashId,
      onStashHandled,
      allTags = $bindable([]),
      stripTagsOnCopy = true,
   } = $props<{
      transferMode: string;
      refreshTrigger: number;
      currentContextId: string;
      contexts: Context[];
      onMoveRequest: (stash: StashItem) => void;
      newStashId?: string | null;
      onStashHandled?: () => void;
      allTags?: string[];
      stripTagsOnCopy?: boolean;
   }>();

   let stashes = $state<StashItem[]>([]);
   let effectiveMode = $state<"Drag" | "Copy">("Drag");

   // Flash state
   let showFlash = $state(false);
   let flashDirection = $state<"up" | "down">("down");
   let flashTargetId = $state<string | null>(null);
   let flashTimeout: NodeJS.Timeout;

   // Scroll state
   let showBackToTop = $state(false);

   // Separate arrays for dnd - these are the source of truth during drag
   let activeStashes = $state<StashItem[]>([]);
   let completedStashes = $state<StashItem[]>([]);
   let draggedItemId = $state<string | null>(null);

   // Confirmation state
   let stashToDelete = $state<string | null>(null);
   let showClearCompletedConfirm = $state(false);
   let showMenu = $state(false);
   let showCompleteAllConfirm = $state(false);
   let backupOrder = $state<StashItem[] | null>(null);
   let activeSort = $state<"asc" | "desc" | null>(null);
   let scrollContainer = $state<HTMLDivElement>();

   // Tag Filtering
   let selectedTags = $state<string[]>([]);
   let showFilterMenu = $state(false);
   let tagCounts = $state<Map<string, number>>(new Map());

   // Extract unique tags from BOTH active and completed stashes (for filter options)
   let availableTags = $derived.by(() => {
      const tags = new Set<string>();
      const regex = /#[\w-]+/g;
      // Include tags from active stashes
      activeStashes.forEach((stash) => {
         const matches = stash.content.match(regex);
         if (matches) {
            matches.forEach((tag) => tags.add(tag));
         }
      });
      // Include tags from completed stashes
      completedStashes.forEach((stash) => {
         const matches = stash.content.match(regex);
         if (matches) {
            matches.forEach((tag) => tags.add(tag));
         }
      });
      return Array.from(tags).sort();
   });

   // Filtered Active Stashes
   let filteredStashes = $derived.by(() => {
      if (selectedTags.length === 0) return activeStashes;

      // Using OR logic: Show stashes that contain ANY of the selected tags
      return activeStashes.filter((stash) => {
         return selectedTags.some((tag) => stash.content.includes(tag));
      });
   });

   // Filtered Completed Stashes
   let filteredCompletedStashes = $derived.by(() => {
      if (selectedTags.length === 0) return completedStashes;

      // Using OR logic: Show stashes that contain ANY of the selected tags
      return completedStashes.filter((stash) => {
         return selectedTags.some((tag) => stash.content.includes(tag));
      });
   });

   $effect(() => {
      // Clear filters if selected tags are no longer available in the current view
      if (selectedTags.length > 0) {
         // Check content from both active AND completed stashes
         const allContent = [
            ...activeStashes.map((s) => s.content),
            ...completedStashes.map((s) => s.content),
         ].join(" ");
         const validTags = selectedTags.filter((tag) =>
            allContent.includes(tag),
         );
         if (validTags.length !== selectedTags.length) {
            selectedTags = validTags;
         }
      }
   });

   const flipDurationMs = 200;

   const adapter = new DesktopStorageAdapter();

   function sortStashes(direction: "asc" | "desc") {
      activeSort = direction;
      if (!backupOrder) {
         backupOrder = [...activeStashes];
      }

      const sorted = [...activeStashes].sort((a, b) => {
         const dateA = new Date(a.createdAt).getTime();
         const dateB = new Date(b.createdAt).getTime();
         return direction === "asc" ? dateA - dateB : dateB - dateA;
      });
      activeStashes = sorted;

      // Reconstruct stashes: sorted active + everything else (completed or other context)
      // Note: activeStashes only contains current context items.
      const otherStashes = stashes.filter(
         (s) => !activeStashes.some((active) => active.id === s.id),
      );
      stashes = [...activeStashes, ...otherStashes];
      adapter.saveStashes(stashes);
      showMenu = false;
   }

   // Sync all tags (Current Context Only) - counts only from active stashes
   $effect(() => {
      const tags = new Set<string>();
      const counts = new Map<string, number>();
      const regex = /#[\w-]+/g;

      // Filter stashes by current context
      const contextStashes = stashes.filter(
         (s) => (s.contextId || "default") === (currentContextId || "default"),
      );

      // Collect all tags from all context stashes (for allTags prop)
      contextStashes.forEach((stash) => {
         const matches = stash.content.match(regex);
         if (matches) {
            matches.forEach((tag) => tags.add(tag));
         }
      });

      // Count occurrences only from ACTIVE (uncompleted) stashes
      activeStashes.forEach((stash) => {
         const matches = stash.content.match(regex);
         if (matches) {
            matches.forEach((tag) => {
               counts.set(tag, (counts.get(tag) || 0) + 1);
            });
         }
      });

      allTags = Array.from(tags).sort();
      tagCounts = counts;
   });

   function restoreOrder() {
      if (backupOrder) {
         activeStashes = [...backupOrder];
         // Reconstruct stashes
         const otherStashes = stashes.filter(
            (s) => !activeStashes.some((active) => active.id === s.id),
         );
         stashes = [...activeStashes, ...otherStashes];
         adapter.saveStashes(stashes);
         backupOrder = null;
         activeSort = null;
      }
      showMenu = false;
   }

   async function completeAllActive() {
      // Mark all active (in current context) as completed
      const activeIds = new Set(activeStashes.map((s) => s.id));

      const updatedStashes = stashes.map((s) => {
         if (activeIds.has(s.id)) {
            return { ...s, completed: true };
         }
         return s;
      });

      stashes = updatedStashes;
      await adapter.saveStashes(stashes);
      load(); // Reload to refresh view
      showCompleteAllConfirm = false;
      showMenu = false;
   }

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
   // Watch for changes to sync lists
   $effect(() => {
      stashes;
      currentContextId;
      syncLists();
   });

   // Clear backup order ONLY when context changes
   $effect(() => {
      currentContextId;
      untrack(() => {
         backupOrder = null;
         activeSort = null;
      });
   });

   $effect(() => {
      currentContextId;
      if (scrollContainer) {
         scrollContainer.scrollTop = 0;
      }
   });

   $effect(() => {
      // Trigger load when refreshTrigger changes
      refreshTrigger;
      load();
   });

   async function load() {
      const loaded = await adapter.loadStashes();
      if (loaded) {
         stashes = loaded;
         syncLists();
         if (newStashId) {
            checkNewStashVisibility();
         }
      }
   }

   async function checkNewStashVisibility() {
      if (!newStashId) return;
      await tick();

      const itemEl = scrollContainer?.querySelector(
         `[data-stash-id="${newStashId}"]`,
      ) as HTMLElement;

      if (!itemEl || !scrollContainer) {
         onStashHandled?.();
         return;
      }

      const containerRect = scrollContainer.getBoundingClientRect();
      const itemRect = itemEl.getBoundingClientRect();

      // Check visibility (with small margin)
      const isAbove = itemRect.bottom < containerRect.top + 50; // 50px buffer
      const isBelow = itemRect.top > containerRect.bottom - 50;

      if (isAbove || isBelow) {
         flashDirection = isAbove ? "up" : "down";
         flashTargetId = newStashId;
         showFlash = true;
         clearTimeout(flashTimeout);
         flashTimeout = setTimeout(() => (showFlash = false), 2500);
      }
      onStashHandled?.();
   }

   function scrollToNewStash() {
      if (!flashTargetId) return;
      const itemEl = scrollContainer?.querySelector(
         `[data-stash-id="${flashTargetId}"]`,
      ) as HTMLElement;

      if (itemEl) {
         itemEl.scrollIntoView({ behavior: "smooth", block: "center" });
         showFlash = false;
      }
   }

   // Resolve mode - only poll when in Auto mode
   $effect(() => {
      // Immediately resolve the mode based on current transferMode
      if (transferMode === "Auto") {
         // Only poll when in Auto mode - reduced frequency to lower CPU usage
         resolveMode();
         const interval = setInterval(resolveMode, 1000);
         return () => clearInterval(interval);
      } else {
         // For manual modes, set immediately without polling
         effectiveMode = transferMode as "Drag" | "Copy";
      }
   });

   async function resolveMode() {
      // Only called when transferMode is "Auto"
      try {
         const target = await adapter.getSmartTransferTarget();
         effectiveMode = target === "GUI" ? "Drag" : "Copy";
      } catch (e) {
         effectiveMode = "Drag";
      }
   }

   async function toggleComplete(item: StashItem) {
      const updated = { ...item, completed: !item.completed };
      await adapter.saveStash(updated);
      load();
   }

   async function deleteStash(id: string, skipConfirm = false) {
      if (skipConfirm) {
         await adapter.deleteStash(id);
         load();
      } else {
         stashToDelete = id;
      }
   }

   async function updateContent(
      item: StashItem,
      content: string,
      files: string[],
   ) {
      const updated = { ...item, content, files };
      await adapter.saveStash(updated);
      load();
   }

   /**
    * Move a stash to the top of the active list.
    */
   async function moveToTop(item: StashItem) {
      const index = activeStashes.findIndex((s) => s.id === item.id);
      if (index <= 0) return; // Already at top or not found

      // Remove from current position and add to beginning
      const newActive = activeStashes.filter((s) => s.id !== item.id);
      newActive.unshift(item);
      activeStashes = newActive;

      // Clear any sort/backup since user manually reordered
      backupOrder = null;
      activeSort = null;

      // Rebuild full stashes array and save
      const otherStashes = stashes.filter(
         (s) =>
            s.completed ||
            (s.contextId || "default") !== (currentContextId || "default"),
      );
      stashes = [...activeStashes, ...otherStashes];
      await adapter.saveStashes(stashes);
   }

   /**
    * Move a stash to the bottom of the active list.
    */
   async function moveToBottom(item: StashItem) {
      const index = activeStashes.findIndex((s) => s.id === item.id);
      if (index < 0 || index === activeStashes.length - 1) return; // Already at bottom or not found

      // Remove from current position and add to end
      const newActive = activeStashes.filter((s) => s.id !== item.id);
      newActive.push(item);
      activeStashes = newActive;

      // Clear any sort/backup since user manually reordered
      backupOrder = null;
      activeSort = null;

      // Rebuild full stashes array and save
      const otherStashes = stashes.filter(
         (s) =>
            s.completed ||
            (s.contextId || "default") !== (currentContextId || "default"),
      );
      stashes = [...activeStashes, ...otherStashes];
      await adapter.saveStashes(stashes);
   }

   async function clearCompleted() {
      await adapter.deleteCompletedStashes(currentContextId);
      load();
   }

   function handleDndConsider(e: CustomEvent) {
      console.log(
         "CONSIDER - items received:",
         e.detail.items.map((i) => ({
            id: i.id,
            isShadow: i.isDndShadowItem,
            content: i.content?.substring(0, 20),
         })),
      );

      // MUST keep shadows in array - library needs them to track dragged item
      activeStashes = e.detail.items;

      if (e.detail.info?.id) {
         draggedItemId = e.detail.info.id;
      }
   }

   function handleDndFinalize(e: CustomEvent) {
      console.log(
         "FINALIZE - items received:",
         e.detail.items.map((i) => ({
            id: i.id,
            isShadow: i.isDndShadowItem,
            content: i.content?.substring(0, 30),
         })),
      );

      activeStashes = e.detail.items;
      draggedItemId = null;
      backupOrder = null;
      activeSort = null;

      // Rebuild full stashes array with new order and save
      // Filter out any shadow items before saving (library should have cleaned them, but just in case)
      const cleanItems = activeStashes.filter((item) => !item.isDndShadowItem);
      const otherStashes = stashes.filter(
         (s) =>
            s.completed ||
            (s.contextId || "default") !== (currentContextId || "default"),
      );
      stashes = [...cleanItems, ...otherStashes];

      // Save asynchronously after state updates complete
      adapter.saveStashes(stashes);
   }

   function handleScroll() {
      if (!scrollContainer) return;
      showBackToTop = scrollContainer.scrollTop > 300;
   }

   function scrollToTop() {
      scrollContainer?.scrollTo({ top: 0, behavior: "smooth" });
   }
</script>

<div class="relative flex-1 flex flex-col min-h-0 bg-[var(--background-queue)]">
   <div
      bind:this={scrollContainer}
      onscroll={handleScroll}
      class="flex-1 overflow-y-auto p-4 pt-0 pb-10 space-y-8 scrollbar-hide"
      role="list"
   >
      <!-- Active Section -->
      <section class="space-y-4">
         <div
            class="flex flex-col sticky top-0 py-3 z-10 -mx-4 px-4 mb-2"
            style="background: linear-gradient(to bottom, var(--background) 0%, var(--background) 80%, transparent 100%);"
         >
            <div class="flex items-center justify-between pointer-events-none">
               <h2
                  class="text-[10px] font-bold text-muted-foreground uppercase tracking-wider pointer-events-auto"
               >
                  {$_("queue.stashQueue")} ({activeStashes.filter(
                     (s) => !s.isDndShadowItem,
                  ).length})
               </h2>
               <div class="flex items-center gap-2 pointer-events-auto">
                  {#if showBackToTop}
                     <button
                        transition:fade={{ duration: 200 }}
                        class="flex items-center gap-1.5 px-2 py-1 rounded-full bg-background/50 hover:bg-background border border-border/40 text-[10px] font-medium text-muted-foreground hover:text-foreground transition-all shadow-sm backdrop-blur-md"
                        onclick={scrollToTop}
                     >
                        <ArrowUp size={10} />
                        {$_("queue.backToTop")}
                     </button>
                  {/if}

                  {#if availableTags.length > 0}
                     <button
                        class="p-1 rounded-md hover:bg-muted text-muted-foreground hover:text-primary transition-colors {showFilterMenu ||
                        selectedTags.length > 0
                           ? 'text-primary bg-primary/10'
                           : ''}"
                        onclick={() => (showFilterMenu = !showFilterMenu)}
                        title={$_("queue.filterByTags")}
                     >
                        {#if selectedTags.length > 0}
                           <svg
                              xmlns="http://www.w3.org/2000/svg"
                              width="14"
                              height="14"
                              viewBox="0 0 24 24"
                              fill="currentColor"
                              stroke="currentColor"
                              stroke-width="2"
                              stroke-linecap="round"
                              stroke-linejoin="round"
                              ><polygon
                                 points="22 3 2 3 10 12.46 10 19 14 21 14 12.46 22 3"
                              /></svg
                           >
                        {:else}
                           <Filter size={14} />
                        {/if}
                     </button>
                  {/if}

                  <div class="relative">
                     <button
                        class="p-1 rounded-md hover:bg-muted text-muted-foreground hover:text-foreground transition-colors"
                        onclick={() => (showMenu = !showMenu)}
                     >
                        <MoreVertical size={14} />
                     </button>

                     {#if showMenu}
                        <div
                           class="fixed inset-0 z-40"
                           onclick={() => (showMenu = false)}
                           aria-hidden="true"
                        ></div>
                        <div
                           class="absolute right-0 top-full mt-1 w-48 bg-popover border border-border rounded-md shadow-lg z-50 py-1 flex flex-col"
                           transition:fade={{ duration: 100 }}
                        >
                           <button
                              class="flex items-center gap-2 px-3 py-2 text-xs text-left w-full transition-colors {activeSort ===
                              'asc'
                                 ? 'bg-muted font-medium'
                                 : 'hover:bg-muted'}"
                              onclick={() => sortStashes("asc")}
                           >
                              <CalendarArrowUp
                                 size={14}
                                 class={activeSort === "asc"
                                    ? "text-primary"
                                    : ""}
                              />
                              {$_("queue.sortOldest")}
                           </button>
                           <button
                              class="flex items-center gap-2 px-3 py-2 text-xs text-left w-full transition-colors {activeSort ===
                              'desc'
                                 ? 'bg-muted font-medium'
                                 : 'hover:bg-muted'}"
                              onclick={() => sortStashes("desc")}
                           >
                              <CalendarArrowDown
                                 size={14}
                                 class={activeSort === "desc"
                                    ? "text-primary"
                                    : ""}
                              />
                              {$_("queue.sortNewest")}
                           </button>

                           {#if backupOrder}
                              <button
                                 class="flex items-center gap-2 px-3 py-2 text-xs hover:bg-muted text-left w-full transition-colors"
                                 onclick={restoreOrder}
                              >
                                 <RotateCcw size={14} />
                                 {$_("queue.restoreOrder")}
                              </button>
                           {/if}

                           <div class="h-px bg-border my-1"></div>
                           <button
                              class="flex items-center gap-2 px-3 py-2 text-xs hover:bg-muted text-left w-full transition-colors"
                              onclick={() => {
                                 showMenu = false;
                                 showCompleteAllConfirm = true;
                              }}
                           >
                              <CheckCheck size={14} />
                              {$_("queue.completeAll")}
                           </button>
                        </div>
                     {/if}
                  </div>
               </div>
            </div>

            {#if showFilterMenu && availableTags.length > 0}
               <div
                  class="flex flex-wrap gap-1.5 mt-2 pb-2 justify-end"
                  transition:fade={{ duration: 150 }}
               >
                  {#each availableTags as tag}
                     {@const hue = getTagHue(tag)}
                     {@const isSelected = selectedTags.includes(tag)}
                     <button
                        class="flex items-center gap-1 text-[10px] px-2 py-0.5 rounded-full border transition-all"
                        style="--tag-hue: {hue}; {isSelected
                           ? `background-color: hsla(${hue}, 100%, 75%, 0.2); border-color: hsla(${hue}, 80%, 40%, 0.3); color: hsla(${hue}, 90%, 25%, 1);`
                           : `background-color: transparent; border-color: var(--border); color: var(--muted-foreground);`}"
                        onclick={() => {
                           if (isSelected) {
                              selectedTags = selectedTags.filter(
                                 (t) => t !== tag,
                              );
                           } else {
                              selectedTags = [...selectedTags, tag];
                           }
                        }}
                     >
                        <svg
                           xmlns="http://www.w3.org/2000/svg"
                           width="12"
                           height="12"
                           viewBox="0 0 24 24"
                           fill="none"
                           stroke="currentColor"
                           stroke-width="2.5"
                           stroke-linecap="round"
                           stroke-linejoin="round"
                           class="flex-shrink-0 opacity-70"
                           ><line x1="4" x2="20" y1="9" y2="9" /><line
                              x1="4"
                              x2="20"
                              y1="15"
                              y2="15"
                           /><line x1="10" x2="8" y1="3" y2="21" /><line
                              x1="16"
                              x2="14"
                              y1="3"
                              y2="21"
                           /></svg
                        >
                        <span class="font-medium">{tag.slice(1)}</span>
                        {#if tagCounts.get(tag)}
                           <span class="text-[9px] opacity-60"
                              >{tagCounts.get(tag)}</span
                           >
                        {/if}
                     </button>
                  {/each}
               </div>
            {/if}
         </div>

         <div
            class="flex flex-col gap-3 min-h-[50px]"
            use:dragHandleZone={{
               items: filteredStashes,
               flipDurationMs,
               dragDisabled: selectedTags.length > 0,
            }}
            onconsider={handleDndConsider}
            onfinalize={handleDndFinalize}
         >
            {#each filteredStashes as item, index (item.id)}
               <div
                  class="dnd-item {draggedItemId === item.id
                     ? 'is-dragging'
                     : ''}"
                  role="listitem"
                  data-stash-id={item.id}
                  animate:flip={{ duration: flipDurationMs }}
               >
                  {#if item.isDndShadowItem}
                     <!-- Shadow placeholder for drop position -->
                     <div
                        class="h-20 rounded-lg border-2 border-dashed border-primary/50 bg-primary/5"
                     ></div>
                  {:else}
                     <StashCard
                        {item}
                        mode={effectiveMode}
                        showReorderHandle={selectedTags.length === 0}
                        {stripTagsOnCopy}
                        isFirst={index === 0}
                        isLast={index ===
                           filteredStashes.filter((s) => !s.isDndShadowItem)
                              .length -
                              1}
                        onMoveRequest={() => onMoveRequest(item)}
                        onMoveToTop={() => moveToTop(item)}
                        onMoveToBottom={() => moveToBottom(item)}
                        onToggleComplete={() => toggleComplete(item)}
                        onDelete={(skip) => deleteStash(item.id, skip)}
                        onUpdateContent={(content, files) =>
                           updateContent(item, content, files)}
                        availableTags={allTags}
                     />
                  {/if}
               </div>
            {/each}

            {#if filteredStashes.filter((s) => !s.isDndShadowItem).length === 0}
               <div
                  class="flex flex-col items-center justify-center py-12 text-muted-foreground/30 border border-dashed border-border/50 rounded-lg"
               >
                  <span class="text-sm">
                     {selectedTags.length > 0
                        ? $_("queue.noActiveStashesFound")
                        : $_("queue.noActiveStashes")}
                  </span>
               </div>
            {/if}
         </div>
      </section>

      <!-- Completed Section -->
      {#if filteredCompletedStashes.length > 0 || (completedStashes.length > 0 && selectedTags.length === 0)}
         <section class="space-y-4">
            <div
               class="flex items-center justify-between sticky top-0 py-3 z-10 -mx-4 px-4 mb-2 pointer-events-none"
               style="background: linear-gradient(to bottom, var(--background) 0%, var(--background) 80%, transparent 100%);"
            >
               <h2
                  class="text-[10px] font-bold text-muted-foreground/60 uppercase tracking-wider pointer-events-auto"
               >
                  {$_("queue.completed")} ({selectedTags.length > 0
                     ? filteredCompletedStashes.length
                     : completedStashes.length})
               </h2>

               <div class="flex items-center gap-2">
                  {#if showBackToTop}
                     <button
                        transition:fade={{ duration: 200 }}
                        class="pointer-events-auto flex items-center gap-1.5 px-2 py-1 rounded-full bg-background/50 hover:bg-background border border-border/40 text-[10px] font-medium text-muted-foreground hover:text-foreground transition-all shadow-sm backdrop-blur-md"
                        onclick={scrollToTop}
                     >
                        <ArrowUp size={10} />
                        {$_("queue.backToTop")}
                     </button>
                  {/if}
                  <button
                     class="text-[9px] flex items-center gap-1 text-red-500/70 dark:text-red-400/70 hover:text-red-600 dark:hover:text-red-500 transition-colors bg-red-500/5 dark:bg-red-400/5 px-1.5 py-0.5 rounded border border-red-500/10 dark:border-red-400/10 pointer-events-auto"
                     onclick={() => (showClearCompletedConfirm = true)}
                     title={$_("queue.deleteAllCompleted")}
                  >
                     <Trash2 size={10} />
                     {$_("queue.clearCompleted")}
                  </button>
               </div>
            </div>

            <div class="flex flex-col gap-3">
               {#each filteredCompletedStashes as item (item.id)}
                  <div role="listitem" data-stash-id={item.id}>
                     <StashCard
                        {item}
                        mode={effectiveMode}
                        showReorderHandle={false}
                        {stripTagsOnCopy}
                        onMoveRequest={() => onMoveRequest(item)}
                        onMoveToTop={() => {}}
                        onMoveToBottom={() => {}}
                        onToggleComplete={() => toggleComplete(item)}
                        onDelete={() => deleteStash(item.id)}
                        onUpdateContent={(content, files) =>
                           updateContent(item, content, files)}
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
            <span class="text-sm">{$_("queue.queueEmpty")}</span>
         </div>
      {/if}
      <!-- Confirmations -->
      <ConfirmationDialog
         open={!!stashToDelete}
         title={$_("stashCard.deleteStashConfirm")}
         description={$_("stashCard.deleteStashConfirm")}
         confirmText={$_("common.delete")}
         variant="destructive"
         onConfirm={() => {
            if (stashToDelete) {
               adapter.deleteStash(stashToDelete).then(() => load());
            }
            stashToDelete = null;
         }}
         onCancel={() => (stashToDelete = null)}
      />

      <ConfirmationDialog
         bind:open={showClearCompletedConfirm}
         title={$_("queue.clearCompleted")}
         description={$_("queue.deleteAllCompleted")}
         confirmText={$_("common.delete")}
         variant="destructive"
         onConfirm={() => {
            clearCompleted();
            showClearCompletedConfirm = false; // handled by dialog close but good to be explicit
         }}
      />

      <ConfirmationDialog
         bind:open={showCompleteAllConfirm}
         title={$_("queue.completeAll")}
         description={$_("queue.completeAllConfirm")}
         confirmText={$_("queue.completeAll")}
         variant="default"
         onConfirm={completeAllActive}
      />

      <!-- New Stash Flash Notification -->
      {#if showFlash}
         <div
            class="absolute left-1/2 -translate-x-1/2 z-50 {flashDirection ===
            'up'
               ? 'top-4'
               : 'bottom-4'}"
            transition:fly={{
               y: flashDirection === "up" ? -20 : 20,
               duration: 300,
            }}
         >
            <button
               class="bg-primary/95 hover:bg-primary text-primary-foreground backdrop-blur-md px-4 py-2.5 rounded-full shadow-lg flex items-center gap-2.5 text-xs font-medium border border-white/10 cursor-pointer transition-all hover:scale-105 active:scale-95"
               onclick={scrollToNewStash}
            >
               <div
                  class="bg-white/20 p-1 rounded-full w-5 h-5 flex items-center justify-center"
               >
                  <Check size={12} strokeWidth={3} />
               </div>
               <span>{$_("queue.stashAdded")}</span>
               <div class="w-px h-3 bg-white/20 mx-0.5"></div>
               {#if flashDirection === "up"}
                  <ArrowUp size={14} class="animate-bounce" />
               {:else}
                  <ArrowDown size={14} class="animate-bounce" />
               {/if}
            </button>
         </div>
      {/if}
   </div>
</div>

<style>
   /* svelte-dnd-action styles */
   :global(.dnd-item) {
      transition: transform 0.2s;
   }
   :global(.dnd-item.is-dragging) {
      opacity: 0.5;
   }
   :global([aria-grabbed="true"]) {
      opacity: 0.5;
   }
</style>
