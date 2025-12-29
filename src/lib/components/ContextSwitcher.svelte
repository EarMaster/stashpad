<script lang="ts">
    import type { Context } from "$lib/types";
    import { Search, ArrowDownUp, Clock } from "lucide-svelte";
    import fuzzysort from "fuzzysort";

    let {
        contexts,
        currentContextId,
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
        autoContextDetection?: boolean;
        mode?: "switch" | "move";
        title?: string;
        onSelect: (context: Context, shiftKey: boolean) => void;
        onAutoContextToggle?: (enabled: boolean) => void;
        onManageContexts?: () => void;
        onClose: () => void;
    }>();

    let searchQuery = $state("");
    let sortBy = $state<"lastUsed" | "alpha">("lastUsed");
    let selectedIndex = $state(0);

    // Derived state for the list
    let displayedContexts = $derived.by(() => {
        let list = [
            {
                id: "default",
                name: "Default",
                rules: [] as any[],
                lastUsed: undefined,
            },
            ...contexts,
        ];

        // Filter
        if (searchQuery) {
            const results = fuzzysort.go(searchQuery, list, { key: "name" });
            return results.map((r) => r.obj);
        }

        // Sort
        list.sort((a, b) => {
            if (sortBy === "alpha") {
                return a.name.localeCompare(b.name);
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

        return list;
    });

    $effect(() => {
        // Reset index if list changes drastically?
        // Or keep it clamped.
        if (selectedIndex >= displayedContexts.length) {
            selectedIndex = Math.max(0, displayedContexts.length - 1);
        }
    });

    // Public API for parent
    export function next() {
        if (displayedContexts.length === 0) return;
        selectedIndex = (selectedIndex + 1) % displayedContexts.length;
    }

    export function prev() {
        if (displayedContexts.length === 0) return;
        selectedIndex =
            (selectedIndex - 1 + displayedContexts.length) %
            displayedContexts.length;
    }

    export function confirm() {
        const item = displayedContexts[selectedIndex];
        if (item) attemptSelection(item);
    }

    function handleKeydown(e: KeyboardEvent) {
        if (e.key === "ArrowDown") {
            e.preventDefault();
            next();
        } else if (e.key === "ArrowUp") {
            e.preventDefault();
            prev();
        } else if (e.key === "Enter") {
            e.preventDefault();
            confirm();
        } else if (e.key === "Escape") {
            e.preventDefault();
            onClose();
        } else if (e.key === "a" && e.altKey && mode === "switch") {
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

    function getRelativeTime(dateString: string) {
        if (!dateString) return "";
        const date = new Date(dateString);
        const now = new Date();
        const diffInSeconds = Math.floor(
            (now.getTime() - date.getTime()) / 1000,
        );

        if (diffInSeconds < 60) {
            return "just now";
        }

        const diffInMinutes = Math.floor(diffInSeconds / 60);
        if (diffInMinutes < 60) {
            return `${diffInMinutes} minute${diffInMinutes === 1 ? "" : "s"} ago`;
        }

        const diffInHours = Math.floor(diffInMinutes / 60);
        if (diffInHours < 24) {
            return `${diffInHours} hour${diffInHours === 1 ? "" : "s"} ago`;
        }

        const diffInDays = Math.floor(diffInHours / 24);
        if (diffInDays === 1) {
            return "Yesterday";
        }
        if (diffInDays < 7) {
            return `${diffInDays} days ago`;
        }

        // Check if same year
        if (date.getFullYear() === now.getFullYear()) {
            return date.toLocaleDateString(undefined, {
                month: "short",
                day: "numeric",
            });
        }

        return date.toLocaleDateString(undefined, {
            month: "short",
            year: "numeric",
        });
    }

    function focusAction(node: HTMLInputElement) {
        node.focus();
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
        class="bg-card border border-border rounded-lg shadow-xl w-[350px] overflow-hidden flex flex-col"
        onclick={(e) => e.stopPropagation()}
        role="document"
    >
        <div class="p-2 border-b border-border bg-muted/50 flex flex-col gap-2">
            <div
                class="flex items-center gap-2 bg-background border border-border rounded px-2 h-8"
            >
                <Search size={14} class="text-muted-foreground" />
                <input
                    class="flex-1 bg-transparent text-sm outline-none placeholder:text-muted-foreground/50"
                    placeholder="Search contexts..."
                    bind:value={searchQuery}
                    use:focusAction
                    onkeydown={(e) => {
                        if (
                            [
                                "ArrowUp",
                                "ArrowDown",
                                "Enter",
                                "Escape",
                            ].includes(e.key) ||
                            (e.key === "a" && e.altKey)
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
                    >{title}</span
                >

                <div class="flex items-center gap-1">
                    <button
                        class="p-1 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors {sortBy ===
                        'lastUsed'
                            ? 'text-primary'
                            : ''}"
                        title="Sort by Last Used"
                        onclick={() => (sortBy = "lastUsed")}
                    >
                        <Clock size={12} />
                    </button>
                    <button
                        class="p-1 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors {sortBy ===
                        'alpha'
                            ? 'text-primary'
                            : ''}"
                        title="Sort Alphabetically"
                        onclick={() => (sortBy = "alpha")}
                    >
                        <ArrowDownUp size={12} />
                    </button>
                </div>
            </div>
        </div>

        <div class="max-h-[300px] overflow-y-auto">
            {#each displayedContexts as ctx, i}
                <button
                    class="w-full text-left px-4 py-2 text-sm flex items-center justify-between hover:bg-accent/50 transition-colors {i ===
                    selectedIndex
                        ? 'bg-accent text-accent-foreground'
                        : ''}"
                    onclick={(e) => attemptSelection(ctx, e.shiftKey)}
                    onmouseenter={() => (selectedIndex = i)}
                >
                    <div class="flex flex-col">
                        <span>{ctx.name}</span>
                        {#if sortBy === "lastUsed" && ctx.lastUsed}
                            <span class="text-[9px] text-muted-foreground/60">
                                {getRelativeTime(ctx.lastUsed)}
                            </span>
                        {/if}
                    </div>

                    {#if ctx.id === currentContextId && mode === "switch"}
                        <span
                            class="text-[10px] bg-primary/20 text-primary px-1 rounded"
                            >Active</span
                        >
                    {/if}
                </button>
            {/each}
            {#if displayedContexts.length === 0}
                <div class="p-4 text-center text-sm text-muted-foreground">
                    No contexts found
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
                        title="Automatically switch context based on active window"
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
