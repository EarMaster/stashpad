<!--
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2025 Nico Wiedemann
-->

<script lang="ts">
    import type { Context } from "$lib/types";

    let {
        contexts,
        currentContextId,
        selectedIndex = $bindable(0),
        autoContextDetection = $bindable(false),
        mode = "switch",
        title = "Select Context",
        onSelect,
        onAutoContextToggle,
        onManageContexts,
        onClose,
    } = $props<{
        contexts: Context[];
        currentContextId: string;
        selectedIndex: number;
        autoContextDetection?: boolean;
        mode?: "switch" | "move";
        title?: string;
        onSelect: (context: Context, shiftKey: boolean) => void;
        onAutoContextToggle?: (enabled: boolean) => void;
        onManageContexts?: () => void;
        onClose: () => void;
    }>();

    function getAvailableContexts() {
        return [{ id: "default", name: "Default", rules: [] }, ...contexts];
    }

    // We can handle arrow navigation here if the component is mounted
    function handleKeydown(e: KeyboardEvent) {
        const available = getAvailableContexts();
        if (available.length === 0) return;

        if (e.key === "ArrowDown") {
            e.preventDefault();
            selectedIndex = (selectedIndex + 1) % available.length;
        } else if (e.key === "ArrowUp") {
            e.preventDefault();
            selectedIndex =
                (selectedIndex - 1 + available.length) % available.length;
        } else if (e.key === "Enter") {
            e.preventDefault();
            attemptSelection(available[selectedIndex], e.shiftKey);
        } else if (e.key === "Escape") {
            e.preventDefault();
            onClose();
        } else if (e.key === "a" && e.altKey && mode === "switch") {
            // Alt+A to toggle auto context shortcut
            e.preventDefault();
            onAutoContextToggle?.(!autoContextDetection);
        }
    }

    function attemptSelection(ctx: Context, shiftKey = false) {
        if (!ctx) return;

        // If Auto Detection is ON and we are in switch mode, we first turn it off, wait, then select.
        if (mode === "switch" && autoContextDetection) {
            onAutoContextToggle?.(false); // Turn off visual toggle
            // Wait for visual confirmation
            setTimeout(() => {
                onSelect(ctx, shiftKey);
            }, 200); // 200ms delay
        } else {
            // Already off or in move mode, just select
            onSelect(ctx, shiftKey);
        }
    }
</script>

<svelte:window onkeydown={handleKeydown} />

<div
    class="absolute inset-0 z-50 bg-black/50 flex items-start justify-center pt-20"
    role="dialog"
    aria-modal="true"
    tabindex="-1"
    onclick={onClose}
    onkeydown={(e) => e.key === "Escape" && onClose()}
>
    <div
        class="bg-card border border-border rounded-lg shadow-xl w-[300px] overflow-hidden flex flex-col"
        onclick={(e) => e.stopPropagation()}
        role="document"
        onkeydown={() => {}}
    >
        <div
            class="p-2 border-b border-border bg-muted/50 text-xs font-semibold text-muted-foreground uppercase flex items-center justify-between"
        >
            <span>{title}</span>
        </div>
        <div class="max-h-[300px] overflow-y-auto">
            {#each getAvailableContexts() as ctx, i}
                <button
                    class="w-full text-left px-4 py-2 text-sm flex items-center justify-between hover:bg-accent/50 transition-colors {i ===
                    selectedIndex
                        ? 'bg-accent text-accent-foreground'
                        : ''}"
                    onmousedown={(e) => attemptSelection(ctx, e.shiftKey)}
                >
                    <span>{ctx.name}</span>
                    {#if ctx.id === currentContextId && mode === "switch"}
                        <span
                            class="text-[10px] bg-primary/20 text-primary px-1 rounded"
                            >Active</span
                        >
                    {/if}
                </button>
            {/each}
        </div>

        {#if mode === "switch"}
            <div
                class="p-2 border-t border-border bg-muted/30 flex flex-col gap-2"
            >
                <div class="flex items-center justify-between px-2 py-1">
                    <label
                        class="flex items-center gap-2 cursor-pointer select-none"
                        title="Automatically switch context based on active window"
                    >
                        <input
                            type="checkbox"
                            class="accent-primary h-3.5 w-3.5"
                            checked={autoContextDetection}
                            onchange={(e) =>
                                onAutoContextToggle?.(e.currentTarget.checked)}
                        />
                        <span class="text-xs text-muted-foreground leading-none"
                            >Auto Context Detection</span
                        >
                    </label>
                    <span class="text-[10px] text-muted-foreground/50 font-mono"
                        >Alt+A</span
                    >
                </div>

                <button
                    class="w-full text-center py-1.5 text-[11px] font-medium text-muted-foreground hover:text-foreground hover:bg-muted/50 rounded transition-colors"
                    onclick={() => {
                        onClose();
                        onManageContexts?.();
                    }}
                >
                    Manage Contexts...
                </button>
            </div>
        {:else}
            <div
                class="p-2 border-t border-border bg-muted/30 text-[10px] text-muted-foreground italic text-center"
            >
                Hold Shift to Copy instead of Move
            </div>
        {/if}
    </div>
</div>
