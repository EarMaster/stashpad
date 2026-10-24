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
   import { tick, untrack } from "svelte";
   import { compareTimestamps } from "$lib/utils/time";
   import { dragHandleZone, TRIGGERS } from "svelte-dnd-action";
   import { flip } from "svelte/animate";
   import { fade, fly } from "svelte/transition";
   import { DesktopStorageAdapter } from "$lib/services/desktop-adapter";
   import type { StashItem, Context, Attachment, AIConfig } from "$lib/types";
   import { queueCache } from "$lib/stores/queue-cache";
   import { _ } from "$lib/i18n";
   import { isHexColor, extractTagsAndColors } from "$lib/utils/markdown";
   import StashCard from "./StashCard.svelte";
   import TagBadge from "./TagBadge.svelte";
   import ConfirmationDialog from "./ConfirmationDialog.svelte";
   import Fireworks from "./Fireworks.svelte";
   import { listen, type UnlistenFn } from "@tauri-apps/api/event";
   import { onMount, onDestroy } from "svelte";
   import {
      setHoveredStash,
      setDragging,
      findCardDropTarget,
   } from "$lib/stores/drag-state.svelte";
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
      ChevronDown,
      ChevronRight,
      Paperclip,
      Ban,
      Image,
      Video,
      FileText,
      File as FileIcon,
      Search,
      X,
      ChevronUp,
      Loader2,
   } from "lucide-svelte";
   import { tooltip } from "$lib/actions/tooltip";
   import {
      countAttachmentFilters,
      matchesFilters,
      type AttachmentFilter,
   } from "$lib/utils/stash-filters";
   import {
      parseRefIds,
      refLabel,
      resolveRef,
      type ResolvedRef,
   } from "$lib/utils/stash-refs";
   import {
      buildSearchIndex,
      searchStashes,
      segments,
      snippets,
      type SearchHits,
   } from "$lib/utils/stash-search";
   import { liveAttachments } from "$lib/utils/stash-filters";
   import { reportError } from "$lib/utils/error-reporting";
   import { log } from "$lib/utils/log";

   let {
      transferMode,
      refreshTrigger,
      currentContextId,
      contexts,
      onMoveRequest,
      newStashId,
      onStashHandled,
      allTags = $bindable([]),
      refCandidates = $bindable([]),
      onSwitchContext,
      stripTagsOnCopy = true,
      aiConfig,
      autoDetectedWindowTitle,
   } = $props<{
      transferMode: string;
      refreshTrigger: number;
      currentContextId: string;
      contexts: Context[];
      onMoveRequest: (stash: StashItem) => void;
      newStashId?: string | null;
      onStashHandled?: () => void;
      allTags?: string[];
      /** Live stashes of the current context, offered by the `>>` picker */
      refCandidates?: StashItem[];
      /** Switch the queue to another context, to follow a reference there */
      onSwitchContext?: (contextId: string) => void;
      stripTagsOnCopy?: boolean;
      aiConfig?: AIConfig;
      /** Window title from auto-detection (only set when auto-detection matched) */
      autoDetectedWindowTitle?: string;
   }>();

   // Derive the current context object for passing to StashCard
   let currentContext = $derived(
      contexts.find((c) => c.id === currentContextId) ??
         contexts.find((c) => c.id === "default"),
   );

   let stashes = $state<StashItem[]>(queueCache.stashes ?? []);
   let loaded = $state(queueCache.stashes !== null);
   // Every change goes through a reassignment of `stashes`, so this keeps the cache as
   // current as the screen, not just as current as the last load.
   $effect(() => {
      if (loaded) queueCache.stashes = stashes;
   });
   let effectiveMode = $state<"Drag" | "Copy">("Drag");

   // Flash state
   let showFlash = $state(false);
   let flashDirection = $state<"up" | "down">("down");
   let flashTargetId = $state<string | null>(null);
   let flashTimeout: NodeJS.Timeout;

   // Scroll state
   let showBackToTop = $state(false);

   // Separate arrays for dnd - these are the source of truth during drag
   // Seeded here rather than by the effect that keeps them in step, so a remount's first
   // frame already has its cards and the count effect below sees no empty-to-full jump.
   let activeStashes = $state<StashItem[]>(
      untrack(() => stashes.filter((s) => !s.completed && inContext(s))),
   );
   let completedStashes = $state<StashItem[]>(
      untrack(() => stashes.filter((s) => s.completed && inContext(s))),
   );
   let draggedItemId = $state<string | null>(null);

   // Confirmation state
   let stashToDelete = $state<string | null>(null);
   let showClearCompletedConfirm = $state(false);
   let showMenu = $state(false);
   let showCompleteAllConfirm = $state(false);
   let backupOrder = $state<StashItem[] | null>(null);
   let activeSort = $state<"asc" | "desc" | null>(null);
   let scrollContainer = $state<HTMLDivElement>();

   // Fireworks state
   let showFireworks = $state(false);
   let previousActiveCount = $state(0);

   // Collapsing state for completed section
   let completedCollapsed = $state(true);

   // Tag Filtering
   let selectedTags = $state<string[]>([]);
   let showFilterMenu = $state(false);
   let tagCounts = $state<Map<string, number>>(new Map());

   // Attachment Filtering
   let selectedAttachmentFilters = $state<AttachmentFilter[]>([]);

   /** Icon for each attachment filter chip, in render order. */
   const attachmentFilterMeta: {
      filter: AttachmentFilter;
      icon: typeof Paperclip;
   }[] = [
      { filter: "any", icon: Paperclip },
      { filter: "none", icon: Ban },
      { filter: "image", icon: Image },
      { filter: "video", icon: Video },
      { filter: "text", icon: FileText },
      { filter: "other", icon: FileIcon },
   ];

   // Counts per attachment filter across BOTH active and completed stashes, so the
   // chips have the same scope as availableTags. Filters nobody matches are absent.
   let attachmentFilterCounts = $derived.by(() =>
      countAttachmentFilters([...activeStashes, ...completedStashes]),
   );

   // "No attachments" alone is not worth a filter menu - only offer the chips once
   // something actually carries a file.
   let availableAttachmentFilters = $derived(
      attachmentFilterCounts.has("any")
         ? attachmentFilterMeta.filter((m) =>
              attachmentFilterCounts.has(m.filter),
           )
         : [],
   );

   function toggleAttachmentFilter(filter: AttachmentFilter) {
      selectedAttachmentFilters = selectedAttachmentFilters.includes(filter)
         ? selectedAttachmentFilters.filter((f) => f !== filter)
         : [...selectedAttachmentFilters, filter];
   }

   // Extract unique tags from BOTH active and completed stashes (for filter options)
   let availableTags = $derived.by(() => {
      const tags = new Set<string>();
      // Include tags from active stashes
      activeStashes.forEach((stash) => {
         const { tags: stashTags } = extractTagsAndColors(stash.content);
         stashTags.forEach((t) => tags.add(t));
      });
      // Include tags from completed stashes
      completedStashes.forEach((stash) => {
         const { tags: stashTags } = extractTagsAndColors(stash.content);
         stashTags.forEach((t) => tags.add(t));
      });
      return Array.from(tags).sort();
   });

   // OR within each filter group, AND between the groups - see matchesFilters().
   let hasActiveFilters = $derived(
      selectedTags.length > 0 || selectedAttachmentFilters.length > 0,
   );

   // Search. Cmd/Ctrl+F opens it on this context; pressing it again searches every
   // context. It looks through what the page cannot show - collapsed completed stashes,
   // the hidden part of long cards, AI-enhanced text, attachment names.
   const isMacPlatform = navigator.platform.includes("Mac");
   const searchShortcut = isMacPlatform ? "⌘F" : "Ctrl+F";
   let searchOpen = $state(false);
   let searchQuery = $state("");
   let searchScope = $state<"context" | "all">("context");
   let searchInput = $state<HTMLInputElement>();

   // `stashes` holds every context, so one index serves both scopes.
   let searchIndex = $derived(buildSearchIndex(stashes));
   // The query the search runs on: what was typed, once typing pauses. Every search
   // re-renders the cards it touches, so one per keystroke made typing stutter.
   let appliedQuery = $state("");
   $effect(() => {
      const query = searchQuery;
      const timer = setTimeout(() => (appliedQuery = query), 150);
      return () => clearTimeout(timer);
   });
   // Two letters at least: a single one prefix-matches nearly every stash, and marking
   // and unfolding all of them froze the window.
   const MIN_QUERY = 2;
   let searching = $derived(
      searchOpen && appliedQuery.trim().length >= MIN_QUERY,
   );
   let searchHits = $derived<SearchHits>(
      searching ? searchStashes(searchIndex, appliedQuery) : new Map(),
   );
   let searchingEverywhere = $derived(searching && searchScope === "all");

   /** Tag, attachment or search filters - anything that hides part of the queue. */
   let isFiltering = $derived(hasActiveFilters || searching);

   function passes(stash: StashItem): boolean {
      return (
         (!hasActiveFilters ||
            matchesFilters(stash, selectedTags, selectedAttachmentFilters)) &&
         (!searching || searchHits.has(stash.id))
      );
   }

   /**
    * Matches across every context, grouped by context, best first. Groups are ordered by
    * their best match, not with the current context first: opening a result switches the
    * context, and the list - and every position in it - must not reshuffle because of it.
    */
   let globalResults = $derived.by(() => {
      if (!searchingEverywhere) return [];
      const byId = new Map(stashes.map((s) => [s.id, s]));
      const groups = new Map<
         string,
         { contextId: string; items: { item: StashItem; terms: string[] }[] }
      >();
      for (const [id, terms] of searchHits) {
         const item = byId.get(id);
         if (!item || item.deleted) continue;
         const contextId = item.contextId || "default";
         if (!groups.has(contextId)) groups.set(contextId, { contextId, items: [] });
         groups.get(contextId)!.items.push({ item, terms });
      }
      return [...groups.values()];
   });

   /**
    * A result's lines of context, one per match: from the text, else the enhanced text,
    * else a file name - the same place its card shows, so the counts agree.
    */
   function resultSnippets(item: StashItem, terms: string[]) {
      const sources = [
         item.content,
         item.enhancedContent ?? "",
         liveAttachments(item)
            .map((a) => a.fileName)
            .join(", "),
      ];
      const text =
         sources.find((t) => segments(t, terms).some((seg) => seg.hit)) ??
         item.content;
      return snippets(text, terms);
   }

   async function openSearch(scope: "context" | "all" = "context") {
      searchOpen = true;
      searchScope = scope;
      await tick();
      searchInput?.focus();
      searchInput?.select();
   }

   function closeSearch() {
      searchOpen = false;
      searchQuery = "";
      appliedQuery = "";
      searchScope = "context";
   }

   /** Shared by every card the search did not match, so none of them re-renders. */
   const NO_TERMS: string[] = [];

   /** Whether a card in this context is hidden by the search. */
   function hiddenBySearch(stash: StashItem): boolean {
      return searching && !searchingEverywhere && !searchHits.has(stash.id);
   }

   // Previous/next, like a browser's find: through every marked word in this context's
   // cards - or the card itself, when its match is in something not shown, such as an
   // attachment name - and through the results when searching everywhere.
   let searchTargets = $state<HTMLElement[]>([]);
   let searchCurrent = $state(-1);

   function collectSearchTargets(): HTMLElement[] {
      if (!scrollContainer) return [];
      if (searchingEverywhere) {
         const targets: HTMLElement[] = [];
         for (const result of scrollContainer.querySelectorAll<HTMLElement>(
            "[data-search-result]",
         )) {
            const marks = result.querySelectorAll<HTMLElement>("mark.search-hit");
            if (marks.length > 0) targets.push(...marks);
            else targets.push(result);
         }
         return targets;
      }
      const targets: HTMLElement[] = [];
      // The list items only: the card inside each carries the id too.
      for (const card of scrollContainer.querySelectorAll<HTMLElement>(
         '[role="listitem"][data-stash-id]',
      )) {
         if (!searchHits.has(card.dataset.stashId ?? "")) continue;
         const marks = card.querySelectorAll<HTMLElement>("mark.search-hit");
         if (marks.length > 0) targets.push(...marks);
         else targets.push(card);
      }
      return targets;
   }

   function goToSearchTarget(index: number) {
      for (const el of scrollContainer?.querySelectorAll(".search-hit-current") ?? []) {
         el.classList.remove("search-hit-current");
      }
      const count = searchTargets.length;
      if (count === 0) {
         searchCurrent = -1;
         return;
      }
      searchCurrent = ((index % count) + count) % count;
      const el = searchTargets[searchCurrent];
      el.classList.add("search-hit-current");
      el.scrollIntoView({ behavior: "smooth", block: "center" });
   }

   /** Move to the next (1) or previous (-1) match, from wherever the last one was. */
   function stepSearch(delta: 1 | -1) {
      // Cards re-render as they are edited or synced, so the marks are found afresh and
      // the position kept by element where it still exists.
      const previous = searchTargets[searchCurrent];
      searchTargets = collectSearchTargets();
      const at = previous ? searchTargets.indexOf(previous) : -1;
      goToSearchTarget(
         at >= 0 ? at + delta : searchCurrent < 0 ? (delta > 0 ? 0 : -1) : searchCurrent,
      );
   }

   /** A stash opened from an all-contexts search: the count starts at its matches. */
   let searchFocusId: string | null = null;
   /** Where the all-contexts results were when a result was opened, to return to. */
   let savedEverywhere: { query: string; index: number } | null = null;

   // A new query, scope or context starts the count again - at the first match, or at
   // the stash just opened from a result - once the cards have marked theirs (they do
   // that a tick after rendering). The context matters on its own: opening a result in
   // another one switches the scope first, while the results are still on screen, and
   // the context only after.
   $effect(() => {
      searchHits;
      searchScope;
      currentContextId;
      if (!searching) {
         searchTargets = [];
         searchCurrent = -1;
         return;
      }
      let cancelled = false;
      tick()
         .then(() => tick())
         .then(() =>
            requestAnimationFrame(() => {
               if (cancelled) return;
               searchTargets = collectSearchTargets();
               const focus = searchFocusId;
               const at = focus
                  ? searchTargets.findIndex((el) =>
                       el.closest('[role="listitem"][data-stash-id]')?.getAttribute("data-stash-id") === focus,
                    )
                  : -1;
               if (at >= 0) searchFocusId = null;
               // Back on "All" for the same search: where it was left, not the top.
               const back =
                  searchingEverywhere && savedEverywhere?.query === appliedQuery
                     ? savedEverywhere.index
                     : -1;
               goToSearchTarget(back >= 0 ? back : Math.max(at, 0));
            }),
         );
      return () => {
         cancelled = true;
      };
   });

   /** Open the result the previous/next buttons are on, when searching everywhere. */
   function openCurrentResult() {
      const id = searchTargets[searchCurrent]
         ?.closest<HTMLElement>("[data-search-result]")
         ?.dataset.searchResult;
      const item = stashes.find((s) => s.id === id) ?? globalResults[0]?.items[0]?.item;
      if (item) openResult(item);
   }

   /**
    * Cmd/Ctrl+F: open on this context; again, while in the field, switch scope.
    * F3 and Cmd/Ctrl+G step through the matches, Shift going back.
    */
   function handleSearchShortcut(e: KeyboardEvent) {
      const mod = isMacPlatform ? e.metaKey : e.ctrlKey;
      if (
         searchOpen &&
         searching &&
         !e.altKey &&
         (e.key === "F3" || (mod && e.key.toLowerCase() === "g"))
      ) {
         e.preventDefault();
         stepSearch(e.shiftKey ? -1 : 1);
         return;
      }
      if (!mod || e.shiftKey || e.altKey || e.key.toLowerCase() !== "f") return;
      e.preventDefault();
      if (!searchOpen) {
         openSearch("context");
      } else if (document.activeElement !== searchInput) {
         searchInput?.focus();
         searchInput?.select();
      } else {
         searchScope = searchScope === "context" ? "all" : "context";
      }
   }

   function handleSearchKeydown(e: KeyboardEvent) {
      if (e.key === "Escape") {
         e.preventDefault();
         closeSearch();
      } else if (e.key === "Enter" && searchingEverywhere && !e.shiftKey) {
         e.preventDefault();
         openCurrentResult();
      } else if (e.key === "Enter") {
         e.preventDefault();
         stepSearch(e.shiftKey ? -1 : 1);
      } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
         e.preventDefault();
         stepSearch(e.key === "ArrowDown" ? 1 : -1);
      }
   }

   /** Go to a stash found by an all-contexts search, in its own context. */
   function openResult(item: StashItem) {
      // The result opened, clicked or stepped to: its first match is where to return.
      const current = searchTargets[searchCurrent]
         ?.closest<HTMLElement>("[data-search-result]")
         ?.dataset.searchResult;
      const index =
         current === item.id
            ? searchCurrent
            : searchTargets.findIndex(
                 (el) =>
                    el.closest<HTMLElement>("[data-search-result]")?.dataset
                       .searchResult === item.id,
              );
      savedEverywhere = { query: appliedQuery, index: Math.max(index, 0) };
      searchFocusId = item.id;
      searchScope = "context";
      const contextId = item.contextId || "default";
      if (contextId !== (currentContextId || "default")) {
         pendingRevealId = item.id;
         onSwitchContext?.(contextId);
         return;
      }
      revealStash(item.id);
   }

   // A search matching completed stashes shows them, rather than a count behind a fold,
   // and folds them away again when it ends - left open, closing the search rendered
   // every completed stash at once.
   let collapsedBeforeSearch: boolean | null = null;
   $effect(() => {
      if (searching && filteredCompletedStashes.length > 0) {
         untrack(() => {
            collapsedBeforeSearch ??= completedCollapsed;
            completedCollapsed = false;
         });
      } else if (!searching) {
         untrack(() => {
            if (collapsedBeforeSearch !== null) {
               completedCollapsed = collapsedBeforeSearch;
               collapsedBeforeSearch = null;
            }
         });
      }
   });

   let showFilterButton = $derived(
      availableTags.length > 0 || availableAttachmentFilters.length > 0,
   );

   // Filtered Active Stashes
   // Tag and attachment filters only. The search hides the active stashes it does not
   // match instead (see hiddenBySearch): unmounting and re-creating every card when it
   // closed is what froze the window.
   let filteredStashes = $derived.by(() => {
      if (!hasActiveFilters) return activeStashes;
      return activeStashes.filter((stash) =>
         matchesFilters(stash, selectedTags, selectedAttachmentFilters),
      );
   });

   let visibleActiveCount = $derived(
      filteredStashes.filter((s) => !s.isDndShadowItem && !hiddenBySearch(s))
         .length,
   );

   // Filtered Completed Stashes
   let filteredCompletedStashes = $derived.by(() => {
      if (!isFiltering) return completedStashes;
      return completedStashes.filter(passes);
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

      // Same for attachment filters: drop any that no longer match a stash, so
      // deleting the last image cannot leave the queue stuck on an empty result.
      if (selectedAttachmentFilters.length > 0) {
         const validFilters = selectedAttachmentFilters.filter((f) =>
            attachmentFilterCounts.has(f),
         );
         if (validFilters.length !== selectedAttachmentFilters.length) {
            selectedAttachmentFilters = validFilters;
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
         return direction === "asc"
            ? compareTimestamps(a.createdAt, b.createdAt)
            : compareTimestamps(b.createdAt, a.createdAt);
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

   // References between stashes. Resolved against every live stash this device has,
   // so a target moved to another context is still found and can be followed.
   const resolveStashRef = (id: string): ResolvedRef =>
      resolveRef(id, stashes, currentContextId);

   const contextName = (id: string): string =>
      contexts.find((c) => c.id === id)?.name ?? id;

   $effect(() => {
      refCandidates = [...activeStashes, ...completedStashes].filter(
         (s) => !s.isDndShadowItem,
      );
   });

   /** How many other live stashes reference the given one. */
   function referrerCount(id: string): number {
      return stashes.filter(
         (s) => s.id !== id && !s.deleted && parseRefIds(s.content).includes(id),
      ).length;
   }

   /** A stash to reveal once the context switched to for it has loaded. */
   let pendingRevealId = $state<string | null>(null);

   /** Follow a reference: bring its target into view, switching context if needed. */
   function followRef(ref: ResolvedRef) {
      if (!ref.stash) return;
      if (ref.state === "other_context") {
         pendingRevealId = ref.stash.id;
         onSwitchContext?.(ref.stash.contextId);
         return;
      }
      revealStash(ref.stash.id);
   }

   async function revealStash(id: string) {
      const target = stashes.find((s) => s.id === id);
      if (!target) return;

      // A filter hiding the target would make the click look broken.
      if (
         hasActiveFilters &&
         !matchesFilters(target, selectedTags, selectedAttachmentFilters)
      ) {
         selectedTags = [];
         selectedAttachmentFilters = [];
      }
      if (searching && !searchHits.has(id)) searchQuery = "";
      if (target.completed) completedCollapsed = false;
      await tick();

      const el = scrollContainer?.querySelector(
         `[data-stash-id="${id}"]`,
      ) as HTMLElement | null;
      if (!el) return;
      el.scrollIntoView({ behavior: "smooth", block: "center" });
      el.classList.add("stash-ref-target");
      setTimeout(() => el.classList.remove("stash-ref-target"), 1500);
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
         const { tags: stashTags } = extractTagsAndColors(stash.content);
         stashTags.forEach((t) => tags.add(t));
      });

      // Count occurrences only from ACTIVE (uncompleted) stashes
      activeStashes.forEach((stash) => {
         const { tags: stashTags } = extractTagsAndColors(stash.content);
         stashTags.forEach((t) => {
            counts.set(t, (counts.get(t) || 0) + 1);
         });
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

   function inContext(stash: StashItem): boolean {
      return (stash.contextId || "default") === (currentContextId || "default");
   }

   // Sync stashes to active/completed
   function syncLists() {
      activeStashes = stashes.filter((s) => !s.completed && inContext(s));
      completedStashes = stashes.filter((s) => s.completed && inContext(s));
   }

   // Watch for context changes
   // Watch for changes to sync lists
   $effect(() => {
      stashes;
      currentContextId;
      syncLists();
   });

   // Watch for queue transitions and completion
   $effect(() => {
      const currentActiveCount = activeStashes.length;

      if (currentActiveCount === 0) {
         // Auto-expand completed section when the active queue is empty
         completedCollapsed = false;

         // Trigger fireworks when the queue becomes empty
         if (previousActiveCount > 0) {
            showFireworks = true;
         }
      } else if (previousActiveCount === 0 && currentActiveCount > 0) {
         // Auto-collapse when transition from empty queue to having active stashes (collapsed by default)
         completedCollapsed = true;
      }

      // Update previous count for next comparison
      previousActiveCount = currentActiveCount;
   });

   // Clear context-specific state when switching
   $effect(() => {
      currentContextId;
      untrack(() => {
         backupOrder = null;
         activeSort = null;
         // Reset collapsing state based on whether active items exist
         completedCollapsed = activeStashes.length > 0;
         // Update previous count to prevent transition effects on context switch
         previousActiveCount = activeStashes.length;
      });
   });

   $effect(() => {
      currentContextId;
      if (scrollContainer) {
         scrollContainer.scrollTop = 0;
      }
   });

   // Declared after the two context effects above on purpose: a switch resets the
   // collapsed state and the scroll position, and the reveal has to come after both.
   $effect(() => {
      currentContextId;
      const id = untrack(() => pendingRevealId);
      if (!id) return;
      pendingRevealId = null;
      tick().then(() => revealStash(id));
   });

   $effect(() => {
      // Trigger load when refreshTrigger changes
      refreshTrigger;
      load();
   });

   async function load() {
      const result = await adapter.loadStashes();
      if (result) {
         stashes = result;
         loaded = true;
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
      attachments: Attachment[],
      enhancedContent?: string,
   ) {
      const updated = {
         ...item,
         content,
         attachments,
         // Only update enhancedContent if explicitly provided
         ...(enhancedContent !== undefined && { enhancedContent }),
      };
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
      // Ids only: this lands in the dev log file, and stash text does not belong there.
      log.debug(
         "CONSIDER - items received:",
         e.detail.items.map((i) => ({ id: i.id, isShadow: i.isDndShadowItem })),
      );

      // MUST keep shadows in array - library needs them to track dragged item
      activeStashes = e.detail.items;

      if (e.detail.info?.id) {
         draggedItemId = e.detail.info.id;
      }
   }

   function handleDndFinalize(e: CustomEvent) {
      // Ids only: this lands in the dev log file, and stash text does not belong there.
      log.debug(
         "FINALIZE - items received:",
         e.detail.items.map((i) => ({ id: i.id, isShadow: i.isDndShadowItem })),
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

   // Tauri drag-drop event listeners for StashCard targeting
   let unlistenDrop: UnlistenFn | null = null;
   let unlistenEnter: UnlistenFn | null = null;
   let unlistenOver: UnlistenFn | null = null;
   let unlistenLeave: UnlistenFn | null = null;

   onMount(async () => {
      window.addEventListener("keydown", handleSearchShortcut);

      // Listen for Tauri's native drag events to coordinate StashCard drop zones
      unlistenEnter = await listen<{ position: { x: number; y: number } }>(
         "tauri://drag-enter",
         (event) => {
            setDragging(true);
            const stashId = findCardDropTarget(
               event.payload.position.x,
               event.payload.position.y,
            );
            setHoveredStash(stashId);
         },
      );

      unlistenOver = await listen<{ position: { x: number; y: number } }>(
         "tauri://drag-over",
         (event) => {
            const stashId = findCardDropTarget(
               event.payload.position.x,
               event.payload.position.y,
            );
            setHoveredStash(stashId);
         },
      );

      unlistenLeave = await listen("tauri://drag-leave", () => {
         setDragging(false);
         setHoveredStash(null);
      });

      unlistenDrop = await listen<{
         paths: string[];
         position: { x: number; y: number };
      }>("tauri://drag-drop", async (event) => {
         const stashId = findCardDropTarget(
            event.payload.position.x,
            event.payload.position.y,
         );
         setDragging(false);
         setHoveredStash(null);

         if (stashId) {
            // Find the stash and add attachments to it
            const stash = stashes.find((s) => s.id === stashId);
            if (stash && !stash.completed) {
               // A browser drop carries no file paths; see Editor.svelte
               let paths = event.payload.paths;
               if (paths.length === 0) {
                  try {
                     paths = [await adapter.readDroppedImage()];
                  } catch (err) {
                     reportError("attachment", err, "Failed to read browser drop");
                     return;
                  }
               }
               let newAttachments = [...stash.attachments];
               for (const path of paths) {
                  try {
                     const attachment = await adapter.saveAssetFromPath(
                        path,
                        stash.contextId,
                        stash.id,
                     );
                     attachment.stashId = stash.id;
                     newAttachments.push(attachment);
                  } catch (err) {
                     reportError("attachment", err, "Failed to save dropped asset");
                  }
               }
               if (newAttachments.length > stash.attachments.length) {
                  const updated = { ...stash, attachments: newAttachments };
                  await adapter.saveStash(updated);
                  load();
               }
            }
         }
         // Note: Editor handles its own drops via separate listeners
      });
   });

   onDestroy(() => {
      window.removeEventListener("keydown", handleSearchShortcut);
      unlistenEnter?.();
      unlistenOver?.();
      unlistenLeave?.();
      unlistenDrop?.();
   });
</script>

<div class="relative flex-1 flex flex-col min-h-0 bg-[var(--background-queue)]">
   <!-- Above the scrolling list, not in a section header: a sticky header only sticks
        within its own section, so the controls scrolled away with the active stashes. -->
   {#if searchOpen}
      <div class="shrink-0 px-4 pt-2 pb-2">
         <div
            class="flex items-center gap-1.5"
            transition:fade={{ duration: 150 }}
         >
            <div class="relative flex-1 min-w-0">
               <Search
                  size={12}
                  class="absolute left-2 top-1/2 -translate-y-1/2 text-muted-foreground pointer-events-none"
               />
               <input
                  bind:this={searchInput}
                  bind:value={searchQuery}
                  type="text"
                  spellcheck="false"
                  autocomplete="off"
                  placeholder={searchScope === "all"
                     ? $_("queue.search.placeholderAll")
                     : $_("queue.search.placeholder")}
                  aria-label={$_("queue.search.label")}
                  class="w-full rounded-md border border-border bg-background pl-7 pr-2 py-1 text-xs text-foreground placeholder:text-muted-foreground/60 focus:outline-none focus:border-primary"
                  onkeydown={handleSearchKeydown}
               />
            </div>
            {#if searching}
               <span
                  class="shrink-0 text-[10px] tabular-nums text-muted-foreground"
                  aria-label={$_("queue.search.position", {
                     values: {
                        current: searchCurrent + 1,
                        total: searchTargets.length,
                     },
                  })}
               >
                  {searchTargets.length > 0
                     ? `${searchCurrent + 1}/${searchTargets.length}`
                     : "0/0"}
               </span>
               <div class="flex shrink-0 -mx-1">
                  <button
                     type="button"
                     class="p-1 rounded-md text-muted-foreground hover:text-foreground hover:bg-muted transition-colors disabled:opacity-40 disabled:pointer-events-none"
                     onclick={() => stepSearch(-1)}
                     disabled={searchTargets.length === 0}
                     title={$_("queue.search.previous")}
                     aria-label={$_("queue.search.previous")}
                  >
                     <ChevronUp size={12} />
                  </button>
                  <button
                     type="button"
                     class="p-1 rounded-md text-muted-foreground hover:text-foreground hover:bg-muted transition-colors disabled:opacity-40 disabled:pointer-events-none"
                     onclick={() => stepSearch(1)}
                     disabled={searchTargets.length === 0}
                     title={$_("queue.search.next")}
                     aria-label={$_("queue.search.next")}
                  >
                     <ChevronDown size={12} />
                  </button>
               </div>
            {/if}
            <div
               class="flex shrink-0 rounded-md border border-border overflow-hidden text-[10px] font-medium"
               role="group"
               aria-label={$_("queue.search.scope")}
            >
               {#each [["context", $_("queue.search.scopeContext")], ["all", $_("queue.search.scopeAll")]] as [scope, label] (scope)}
                  <button
                     type="button"
                     class="px-2 py-1 transition-colors {searchScope ===
                     scope
                        ? 'bg-primary/15 text-primary'
                        : 'text-muted-foreground hover:bg-muted'}"
                     aria-pressed={searchScope === scope}
                     onclick={() => {
                        searchScope = scope as "context" | "all";
                        searchInput?.focus();
                     }}
                  >
                     {label}
                  </button>
               {/each}
            </div>
            <button
               type="button"
               class="p-1 shrink-0 rounded-md text-muted-foreground hover:text-foreground hover:bg-muted transition-colors"
               onclick={closeSearch}
               title={$_("queue.search.close")}
               aria-label={$_("queue.search.close")}
            >
               <X size={12} />
            </button>
         </div>
      </div>
   {/if}
   <div
      bind:this={scrollContainer}
      onscroll={handleScroll}
      class="flex-1 overflow-y-auto overflow-x-hidden p-4 pt-0 pb-10 space-y-8 scrollbar-hide"
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


                  <button
                     class="p-1 rounded-md hover:bg-muted text-muted-foreground hover:text-primary transition-colors {searchOpen
                        ? 'text-primary bg-primary/10'
                        : ''}"
                     onclick={() => (searchOpen ? closeSearch() : openSearch())}
                     title={$_("queue.search.open", {
                        values: { shortcut: searchShortcut },
                     })}
                     aria-pressed={searchOpen}
                     use:tooltip
                  >
                     <Search size={14} />
                  </button>

                  {#if showFilterButton}
                     <button
                        class="p-1 rounded-md hover:bg-muted text-muted-foreground hover:text-primary transition-colors {showFilterMenu ||
                        hasActiveFilters
                           ? 'text-primary bg-primary/10'
                           : ''}"
                        onclick={() => (showFilterMenu = !showFilterMenu)}
                        title={$_("queue.filterStashes")}
                        use:tooltip
                     >
                        {#if hasActiveFilters}
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

            {#if showFilterMenu && showFilterButton}
               <div
                  class="flex flex-wrap gap-1.5 mt-2 pb-2 justify-end items-center"
                  transition:fade={{ duration: 150 }}
               >
                  {#each availableAttachmentFilters as { filter, icon: FilterIcon } (filter)}
                     <button
                        type="button"
                        class="inline-flex items-center gap-0.5 rounded-full border px-1.5 py-0.5 text-[10px] transition-all hover:opacity-80 {selectedAttachmentFilters.includes(
                           filter,
                        )
                           ? 'border-primary/40 bg-primary/15 text-primary'
                           : 'border-border bg-transparent text-muted-foreground'}"
                        onclick={() => toggleAttachmentFilter(filter)}
                     >
                        <FilterIcon size={10} />
                        <span class="font-medium"
                           >{$_(`queue.attachmentFilter.${filter}`)}</span
                        >
                        <span class="text-[9px] opacity-60"
                           >{attachmentFilterCounts.get(filter)}</span
                        >
                     </button>
                  {/each}

                  {#if availableAttachmentFilters.length > 0 && availableTags.length > 0}
                     <div class="h-3.5 w-px bg-border" aria-hidden="true"></div>
                  {/if}

                  {#each availableTags as tag}
                     <TagBadge
                        {tag}
                        selected={selectedTags.includes(tag)}
                        count={tagCounts.get(tag)}
                        onclick={() => {
                           if (selectedTags.includes(tag)) {
                              selectedTags = selectedTags.filter(
                                 (t) => t !== tag,
                              );
                           } else {
                              selectedTags = [...selectedTags, tag];
                           }
                        }}
                     />
                  {/each}
               </div>
            {/if}
         </div>

         {#if searchingEverywhere}
            <div class="flex flex-col gap-4">
               {#each globalResults as group (group.contextId)}
                  <div class="flex flex-col gap-1.5">
                     <h3
                        class="text-[10px] font-bold uppercase tracking-wider text-muted-foreground"
                     >
                        {contextName(group.contextId)} ({group.items.length})
                     </h3>
                     {#each group.items as { item, terms } (item.id)}
                        <button
                           type="button"
                           class="flex flex-col gap-1 rounded-lg border border-border bg-card p-2.5 text-left transition-colors hover:border-primary/50 focus-visible:border-primary focus-visible:outline-none"
                           data-search-result={item.id}
                           onclick={() => openResult(item)}
                        >
                           <span
                              class="flex items-center gap-1.5 text-xs font-medium {item.completed
                                 ? 'text-green-500'
                                 : 'text-foreground'}"
                           >
                              {#if item.completed}
                                 <Check size={12} class="shrink-0" />
                              {/if}
                              <span class="truncate"
                                 >{refLabel(item.content) ||
                                    item.id.slice(0, 8)}</span
                              >
                           </span>
                           {#each resultSnippets(item, terms) as line}
                              <span
                                 class="block text-[11px] leading-snug text-muted-foreground break-words"
                              >
                                 {#each line as seg}{#if seg.hit}<mark
                                          class="search-hit">{seg.text}</mark
                                       >{:else}{seg.text}{/if}{/each}
                              </span>
                           {/each}
                        </button>
                     {/each}
                  </div>
               {:else}
                  <div
                     class="flex flex-col items-center justify-center py-12 text-muted-foreground/30 border border-dashed border-border/50 rounded-lg"
                  >
                     <span class="text-sm">{$_("queue.search.noResults")}</span>
                  </div>
               {/each}
            </div>
         {:else}
         <div
            class="flex flex-col gap-3 min-h-[50px]"
            use:dragHandleZone={{
               items: filteredStashes,
               flipDurationMs,
               dragDisabled: isFiltering,
            }}
            onconsider={handleDndConsider}
            onfinalize={handleDndFinalize}
         >
            {#each filteredStashes as item, index (item.id)}
               <div
                  class="dnd-item {draggedItemId === item.id
                     ? 'is-dragging'
                     : ''} {hiddenBySearch(item) ? 'hidden' : ''}"
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
                        showReorderHandle={!isFiltering}
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
                        onUpdateContent={(
                           content,
                           attachments,
                           enhancedContent,
                        ) =>
                           updateContent(
                              item,
                              content,
                              attachments,
                              enhancedContent,
                           )}
                        availableTags={allTags}
                        {aiConfig}
                        {currentContext}
                        resolveRef={resolveStashRef}
                        {contextName}
                        onReferenceClick={followRef}
                        {refCandidates}
                        {autoDetectedWindowTitle}
                        searchTerms={searchHits.get(item.id) ?? NO_TERMS}
                     />
                  {/if}
               </div>
            {/each}

            {#if visibleActiveCount === 0}
               <div
                  class="flex flex-col items-center justify-center py-12 text-muted-foreground/30 border border-dashed border-border/50 rounded-lg"
               >
                  <span class="text-sm">
                     {searching && !hasActiveFilters
                        ? $_("queue.search.noActive")
                        : isFiltering
                          ? $_("queue.noActiveStashesFound")
                          : $_("queue.noActiveStashes")}
                  </span>
               </div>
            {/if}
         </div>
         {/if}
      </section>

      <!-- Completed Section -->
      {#if !searchingEverywhere && (filteredCompletedStashes.length > 0 || (completedStashes.length > 0 && !isFiltering))}
         <section class="space-y-4">
            <div
               class="flex items-center justify-between sticky top-0 py-3 z-10 -mx-4 px-4 mb-2 pointer-events-none"
               style="background: linear-gradient(to bottom, var(--background) 0%, var(--background) 80%, transparent 100%);"
            >
               <button
                  class="flex items-center gap-2 pointer-events-auto group"
                  onclick={() => (completedCollapsed = !completedCollapsed)}
               >
                  <div
                     class="text-muted-foreground/40 group-hover:text-muted-foreground transition-colors"
                  >
                     {#if completedCollapsed}
                        <ChevronRight size={12} />
                     {:else}
                        <ChevronDown size={12} />
                     {/if}
                  </div>
                  <h2
                     class="text-[10px] font-bold text-muted-foreground/60 uppercase tracking-wider group-hover:text-muted-foreground/80 transition-colors"
                  >
                     {$_("queue.completed")} ({isFiltering
                        ? filteredCompletedStashes.length
                        : completedStashes.length})
                  </h2>
               </button>

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
                     use:tooltip
                  >
                     <Trash2 size={10} />
                     {$_("queue.clearCompleted")}
                  </button>
               </div>
            </div>

               {#if !completedCollapsed}
                  <div
                     class="flex flex-col gap-3"
                     transition:fade={{ duration: 150 }}
                  >
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
                              onUpdateContent={(
                                 content,
                                 files,
                                 enhancedContent,
                              ) =>
                                 updateContent(
                                    item,
                                    content,
                                    files,
                                    enhancedContent,
                                 )}
                              {currentContext}
                              resolveRef={resolveStashRef}
                              {contextName}
                              onReferenceClick={followRef}
                              {refCandidates}
                              {autoDetectedWindowTitle}
                              searchTerms={searchHits.get(item.id) ?? NO_TERMS}
                           />
                        </div>
                     {/each}
                  </div>
               {/if}
            </section>
      {/if}

      {#if !loaded}
         <!-- Only on a cold start: every later mount begins from queueCache. -->
         <div
            class="flex flex-col items-center justify-center py-10 text-muted-foreground/60"
            role="status"
         >
            <Loader2 size={20} class="animate-spin mb-2" aria-hidden="true" />
            <span class="text-sm">{$_("queue.loadingStashes")}</span>
         </div>
      {:else if stashes.length === 0}
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
         description={stashToDelete && referrerCount(stashToDelete) > 0
            ? $_("stashRef.deleteReferenced", {
                 values: { count: referrerCount(stashToDelete) },
              })
            : $_("stashCard.deleteStashConfirm")}
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
         description={completedStashes.some((s) => referrerCount(s.id) > 0)
            ? `${$_("queue.deleteAllCompleted")} ${$_("stashRef.clearReferenced")}`
            : $_("queue.deleteAllCompleted")}
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

<!-- Fireworks celebration when queue is completed -->
<Fireworks bind:show={showFireworks} duration={4000} />

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
