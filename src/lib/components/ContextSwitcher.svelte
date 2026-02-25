<!--
// SPDX-License-Identifier: AGPL-3.0-only

// Copyright (C) 2025-2026 Nico Wiedemann
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
    import type { Context } from "$lib/types";
    import { _, date as formatDate } from "$lib/i18n";
    import { Search, ArrowDownUp, Clock, Plus } from "lucide-svelte";
    import fuzzysort from "fuzzysort";
    import { getRelativeTime } from "$lib/utils/date";
    import { tooltip } from "$lib/actions/tooltip";

    let {
        contexts,
        currentContextId,
        autoContextDetection = $bindable(false),
        mode = "switch",
        title = "",
        stashCounts = {},
        onSelect,
        onCreate,
        onAutoContextToggle,
        onManageContexts,
        onClose,
    } = $props<{
        contexts: Context[];
        currentContextId: string;
        autoContextDetection?: boolean;
        mode?: "switch" | "move";
        title?: string;
        stashCounts?: Record<string, number>;
        onSelect: (context: Context, shiftKey: boolean) => void;
        onCreate?: (name: string) => void;
        onAutoContextToggle?: (enabled: boolean) => void;
        onManageContexts?: () => void;
        onClose: () => void;
    }>();

    let searchQuery = $state("");
    let sortBy = $state<"lastUsed" | "alpha" | "alphaDesc">("lastUsed");
    let selectedIndex = $state(0);

    // Computed title based on mode
    let displayTitle = $derived(
        title ||
            (mode === "move"
                ? $_("contextSwitcher.moveStashTo")
                : $_("contextSwitcher.switchContext")),
    );

    // Derived state for the list
    let displayedContexts = $derived.by(() => {
        let list = contexts;

        // Filter
        if (searchQuery) {
            const results = fuzzysort.go(searchQuery, list, { key: "name" });
            return results.map((r) => r.obj);
        }

        // Sort - create a copy to avoid mutating state in $derived
        return [...list].sort((a, b) => {
            if (sortBy === "alpha") {
                return a.name.localeCompare(b.name);
            } else if (sortBy === "alphaDesc") {
                return b.name.localeCompare(a.name);
            } else {
                // lastUsed desc
                const tA = a.lastUsed ? new Date(a.lastUsed).getTime() : 0;
                const tB = b.lastUsed ? new Date(b.lastUsed).getTime() : 0;
                // If timestamps are equal (or both 0), maybe stable sort or alpha fallback?
                // Let's fallback to alpha for stability
                if (tA === tB) return a.name.localeCompare(b.name);
                return tB - tA; // Newest first
            }
        });
    });

    // Show a "Create" option when the search query doesn't exactly match any context
    let showCreateOption = $derived(
        searchQuery.trim().length > 0 &&
            onCreate !== undefined &&
            !contexts.some(
                (c) =>
                    c.name.toLowerCase() === searchQuery.trim().toLowerCase(),
            ),
    );

    // Total navigable items: contexts + optional create row
    let totalItems = $derived(
        displayedContexts.length + (showCreateOption ? 1 : 0),
    );

    $effect(() => {
        // Keep selectedIndex clamped within navigable items
        if (selectedIndex >= totalItems) {
            selectedIndex = Math.max(0, totalItems - 1);
        }
    });

    // Public API for parent
    export function next() {
        if (totalItems === 0) return;
        selectedIndex = (selectedIndex + 1) % totalItems;
    }

    export function prev() {
        if (totalItems === 0) return;
        selectedIndex = (selectedIndex - 1 + totalItems) % totalItems;
    }

    export function confirm() {
        // If the create-row is selected, trigger onCreate
        if (showCreateOption && selectedIndex === displayedContexts.length) {
            onCreate?.(searchQuery.trim());
            return;
        }
        const item = displayedContexts[selectedIndex];
        if (item) attemptSelection(item);
    }

    // Handle Alt+A for auto context detection toggle
    // Navigation keys (arrows, Enter, Escape) are handled by App.svelte
    // Note: On macOS, Option+A produces "å" instead of "a", so we check both
    function handleKeydown(e: KeyboardEvent) {
        if ((e.key === "a" || e.key === "å") && e.altKey && mode === "switch") {
            e.preventDefault();
            const newState = !autoContextDetection;
            onAutoContextToggle?.(newState);
            if (newState) {
                onClose();
            }
        }
    }

    function attemptSelection(ctx: Context, shiftKey = false) {
        if (!ctx) return;
        if (mode === "switch" && autoContextDetection) {
            onAutoContextToggle?.(false);
            setTimeout(() => onSelect(ctx, shiftKey), 200);
        } else {
            onSelect(ctx, shiftKey);
        }
    }

    import { trapFocus } from "$lib/actions/trapFocus";
</script>

<svelte:window onkeydown={handleKeydown} />

<div
    class="absolute inset-0 z-50 bg-black/50 flex items-start justify-center pt-20"
    role="dialog"
    aria-modal="true"
    tabindex="-1"
    onclick={(e) => {
        if (e.target === e.currentTarget) onClose();
    }}
    onkeydown={(e) => e.key === "Escape" && onClose()}
>
    <div
        class="bg-card border border-border rounded-lg shadow-xl w-[350px] overflow-hidden flex flex-col"
        role="document"
        use:trapFocus
    >
        <div class="p-2 border-b border-border bg-muted/50 flex flex-col gap-2">
            <div
                class="flex items-center gap-2 bg-background border border-border rounded px-2 h-8"
            >
                <Search size={14} class="text-muted-foreground" />
                <input
                    class="flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground/50"
                    placeholder={$_("contextSwitcher.searchPlaceholder")}
                    bind:value={searchQuery}
                    onkeydown={(e) => {
                        // On macOS, Option+A produces "å" — prevent it from
                        // being inserted and let the window handler toggle the
                        // auto-context-detection setting instead.
                        if ((e.key === "a" || e.key === "å") && e.altKey) {
                            e.preventDefault();
                            return;
                        }
                        if (
                            [
                                "ArrowUp",
                                "ArrowDown",
                                "Enter",
                                "Escape",
                            ].includes(e.key) ||
                            e.ctrlKey ||
                            e.metaKey
                        ) {
                            return;
                        }
                        e.stopPropagation();
                    }}
                />
                <!-- Stop prop logic for arrows if focused? 
                      Actually we want up/down to scroll list even if input focused.
                      So we should NOT stop prop for arrows.
                      But we should stop prop for other keys to avoid shortcuts firing? 
                      Wait, the window handler handles arrows. 
                 -->
            </div>

            <div class="flex items-center justify-between px-1">
                <span
                    class="text-[10px] font-semibold text-muted-foreground uppercase"
                    >{displayTitle}</span
                >

                <div class="flex items-center gap-1">
                    <button
                        class="p-1 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors {sortBy ===
                        'lastUsed'
                            ? 'text-primary'
                            : ''}"
                        title={$_("contextSwitcher.sortByLastUsed")}
                        use:tooltip
                        onclick={() => (sortBy = "lastUsed")}
                    >
                        <Clock size={12} />
                    </button>
                    <button
                        class="p-1 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors {sortBy.startsWith(
                            'alpha',
                        )
                            ? 'text-primary'
                            : ''}"
                        title={$_("contextSwitcher.sortAlphabetically")}
                        use:tooltip
                        onclick={() =>
                            (sortBy =
                                sortBy === "alpha" ? "alphaDesc" : "alpha")}
                    >
                        <ArrowDownUp size={12} />
                    </button>
                </div>
            </div>
        </div>

        <div class="max-h-[300px] overflow-y-auto">
            {#each displayedContexts as ctx, i}
                {@const isSelected = i === selectedIndex}
                <button
                    class="w-full text-left px-4 py-2 text-sm flex items-center justify-between transition-colors {isSelected
                        ? 'bg-primary/20 text-foreground'
                        : ''}"
                    onclick={(e) => attemptSelection(ctx, e.shiftKey)}
                    onmouseenter={() => (selectedIndex = i)}
                >
                    <div class="flex flex-col">
                        <span>{ctx.name}</span>
                        <span class="text-[9px] text-muted-foreground/60">
                            {#if ctx.lastUsed}
                                {getRelativeTime(ctx.lastUsed, $_)}
                            {:else if ctx.id === "default"}
                                {$_("contextSwitcher.alwaysThereForYou")}
                            {/if}
                        </span>
                    </div>

                    <div class="flex items-center gap-2">
                        {#if ctx.id === currentContextId && mode === "switch"}
                            <span
                                class="text-[10px] bg-primary/20 text-primary px-1 rounded"
                                >{$_("common.active")}</span
                            >
                        {/if}
                        {#if stashCounts[ctx.id] !== undefined}
                            <span
                                class="text-[10px] text-muted-foreground tabular-nums"
                            >
                                {stashCounts[ctx.id]}
                            </span>
                        {/if}
                    </div>
                </button>
            {/each}
            {#if showCreateOption}
                {@const isCreateSelected =
                    selectedIndex === displayedContexts.length}
                <button
                    class="w-full text-left px-4 py-2 text-sm flex items-center gap-2 transition-colors {isCreateSelected
                        ? 'bg-primary/20 text-foreground'
                        : 'text-muted-foreground hover:text-foreground'}"
                    onclick={() => onCreate?.(searchQuery.trim())}
                    onmouseenter={() =>
                        (selectedIndex = displayedContexts.length)}
                >
                    <Plus size={14} class="shrink-0" />
                    <span
                        >{$_("contextSwitcher.createContext", {
                            values: { name: searchQuery.trim() },
                        })}</span
                    >
                </button>
            {/if}
            {#if displayedContexts.length === 0 && !showCreateOption}
                <div class="p-4 text-center text-sm text-muted-foreground">
                    {$_("contextSwitcher.noContextsFound")}
                </div>
            {/if}
        </div>

        {#if mode === "switch"}
            <div
                class="p-2 border-t border-border bg-muted/30 flex flex-col gap-2"
            >
                <div class="flex items-center justify-between px-2 py-1">
                    <label
                        class="flex items-center gap-2 cursor-pointer select-none"
                        title={$_(
                            "settings.general.autoContextDetection.description",
                        )}
                    >
                        <input
                            type="checkbox"
                            class="accent-primary h-3.5 w-3.5"
                            checked={autoContextDetection}
                            onchange={(e) => {
                                const newState = e.currentTarget.checked;
                                onAutoContextToggle?.(newState);
                                if (newState) onClose();
                            }}
                        />
                        <span class="text-xs text-muted-foreground leading-none"
                            >{$_("contextSwitcher.autoContextDetection")}</span
                        >
                    </label>
                    <span class="text-[10px] text-muted-foreground/50 font-mono"
                        >{navigator.platform.includes("Mac")
                            ? "⌥A"
                            : "Alt+A"}</span
                    >
                </div>

                <button
                    class="w-full text-center py-1.5 text-[11px] font-medium text-muted-foreground hover:text-foreground hover:bg-muted/50 rounded transition-colors"
                    onclick={() => {
                        onClose();
                        onManageContexts?.();
                    }}
                >
                    {$_("contextSwitcher.manageContexts")}
                </button>
            </div>
        {:else}
            <div
                class="p-2 border-t border-border bg-muted/30 text-[10px] text-muted-foreground italic text-center"
            >
                {$_("contextSwitcher.holdShiftToCopy")}
            </div>
        {/if}
    </div>
</div>
