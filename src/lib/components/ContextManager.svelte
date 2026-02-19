<!--
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2025 Nico Wiedemann
-->

<script lang="ts">
    import { DesktopStorageAdapter } from "$lib/services/desktop-adapter";
    import type { Settings, StashItem, Context } from "$lib/types";
    import { _ } from "$lib/i18n";
    import { tick } from "svelte";
    import ConfirmationDialog from "./ConfirmationDialog.svelte";
    import ExportDialog from "./ExportDialog.svelte";
    import ImportDialog from "./ImportDialog.svelte";
    import { ArrowDownAZ, ArrowUpAZ, Clock } from "lucide-svelte";
    import { calculateTotalAttachmentSize } from "$lib/utils/format";

    import ContextManagerItem from "./ContextManagerItem.svelte";

    let { onBack, onSelect } = $props<{
        onBack: () => void;
        onSelect?: (id: string) => void;
    }>();

    let contexts = $state<Context[]>([]);
    let stashCounts = $state<Record<string, number>>({});
    let contextSizes = $state<Record<string, number>>({});
    let allStashes = $state<StashItem[]>([]);
    let isLoading = $state(true);
    let contextToDelete = $state<string | null>(null);
    let deleteConfirmationOpen = $state(false);

    // Export dialog state
    let exportDialogOpen = $state(false);
    let exportContext = $state<Context | null>(null);

    // Import dialog state
    let importDialogOpen = $state(false);
    let importContext = $state<Context | null>(null);

    const adapter = new DesktopStorageAdapter();

    // Sorting state
    let sortBy = $state<"name" | "lastUsed">("name");
    let sortDirection = $state<"asc" | "desc">("asc");

    // Track newly created context for auto-focus
    let newlyCreatedContextId = $state<string | null>(null);

    // Separate default context from regular contexts
    let defaultContext = $derived(contexts.find((c) => c.id === "default"));

    let nonDefaultContexts = $derived(
        contexts.filter((c) => c.id !== "default"),
    );

    let sortedContexts = $derived(
        [...nonDefaultContexts].sort((a, b) => {
            if (sortBy === "name") {
                return sortDirection === "asc"
                    ? a.name.localeCompare(b.name)
                    : b.name.localeCompare(a.name);
            } else {
                const dateA = new Date(a.lastUsed || 0).getTime();
                const dateB = new Date(b.lastUsed || 0).getTime();
                // For dates, usually we want newest first (desc) as default "top",
                // but if user selects ASC, they want oldest first.
                return sortDirection === "asc" ? dateA - dateB : dateB - dateA;
            }
        }),
    );

    async function load() {
        try {
            contexts = await adapter.getContexts();

            // Load stashes and count per context
            const stashes = await adapter.loadStashes();
            allStashes = stashes;
            const counts: Record<string, number> = { default: 0 };
            const sizes: Record<string, number> = { default: 0 };
            contexts.forEach((ctx) => {
                counts[ctx.id] = 0;
                sizes[ctx.id] = 0;
            });

            stashes.forEach((stash: StashItem) => {
                const ctxId = stash.contextId || "default";
                if (!stash.completed) {
                    counts[ctxId] = (counts[ctxId] || 0) + 1;
                }
                // Include size of all stashes (active and completed) in context size
                // OR should it be only active? Usually storage management implies all.
                // Let's count all valid attachments.
                if (stash.attachments && stash.attachments.length > 0) {
                    sizes[ctxId] =
                        (sizes[ctxId] || 0) +
                        calculateTotalAttachmentSize(stash.attachments);
                }
            });
            stashCounts = counts;
            contextSizes = sizes;
        } catch (e) {
            console.error("Failed to load contexts", e);
        } finally {
            isLoading = false;
        }
    }

    async function saveContexts() {
        try {
            await adapter.saveContexts(contexts);
        } catch (e) {
            console.error("Failed to save contexts", e);
        }
    }

    $effect(() => {
        load();
    });

    async function saveContext(context: Context) {
        try {
            await adapter.saveContext(context);
        } catch (e) {
            console.error("Failed to save context", e);
        }
    }

    async function addContext() {
        const newContext = {
            id: crypto.randomUUID(),
            name: $_("contexts.newContext"),
            rules: [],
            lastUsed: new Date().toISOString(),
        };
        contexts = [...contexts, newContext];
        newlyCreatedContextId = newContext.id;
        await saveContext(newContext);
    }

    async function removeContext(id: string) {
        // Delete from database (marks as deleted = 1)
        try {
            await adapter.deleteContext(id);
            // Remove from local array after successful DB deletion
            contexts = contexts.filter((c) => c.id !== id);
        } catch (e) {
            console.error("Failed to delete context:", e);
        }
    }

    /**
     * Open export dialog for a context.
     */
    function openExportDialog(context: Context) {
        exportContext = context;
        exportDialogOpen = true;
    }

    /**
     * Get stashes for a specific context.
     */
    function getContextStashes(context: Context): StashItem[] {
        return allStashes.filter(
            (s) => (s.contextId || "default") === context.id,
        );
    }

    /**
     * Open import dialog for a context.
     */
    function openImportDialog(context: Context) {
        importContext = context;
        importDialogOpen = true;
    }
</script>

<div class="h-full flex flex-col bg-background">
    <!-- Header -->
    <div
        data-tauri-drag-region
        class="flex items-center gap-3 p-4 border-b border-border bg-muted/20 shrink-0"
    >
        <button
            class="p-2 hover:bg-muted rounded-md text-muted-foreground hover:text-foreground transition-colors"
            onclick={onBack}
            title={$_("contexts.back")}
        >
            ←
        </button>
        <h1 class="text-xl font-bold tracking-tight">{$_("contexts.title")}</h1>
    </div>

    <!-- Content -->
    <div class="flex-1 overflow-y-auto p-4 scrollbar-hide">
        <div class="space-y-6 max-w-2xl mx-auto">
            {#if isLoading}
                <div class="text-sm text-muted-foreground animate-pulse">
                    {$_("contexts.loadingContexts")}
                </div>
            {:else}
                <div class="flex items-center justify-between gap-4">
                    <p class="text-sm text-muted-foreground hidden sm:block">
                        {$_("contexts.manageDescription")}
                    </p>

                    <div class="flex items-center gap-2 ml-auto">
                        <!-- Sort Controls -->
                        <div
                            class="flex items-center bg-muted/50 rounded-md p-0.5 mr-2"
                        >
                            <button
                                class="p-1.5 rounded-sm transition-colors {sortBy ===
                                'name'
                                    ? 'bg-background shadow-sm text-foreground'
                                    : 'text-muted-foreground hover:text-foreground'}"
                                onclick={() => {
                                    if (sortBy === "name") {
                                        sortDirection =
                                            sortDirection === "asc"
                                                ? "desc"
                                                : "asc";
                                    } else {
                                        sortBy = "name";
                                        sortDirection = "asc";
                                    }
                                }}
                                title={$_("contextSwitcher.sortAlphabetically")}
                            >
                                {#if sortBy === "name" && sortDirection === "desc"}
                                    <ArrowUpAZ size={14} />
                                {:else}
                                    <ArrowDownAZ size={14} />
                                {/if}
                            </button>
                            <button
                                class="p-1.5 rounded-sm transition-colors {sortBy ===
                                'lastUsed'
                                    ? 'bg-background shadow-sm text-foreground'
                                    : 'text-muted-foreground hover:text-foreground'}"
                                onclick={() => {
                                    if (sortBy === "lastUsed") {
                                        sortDirection =
                                            sortDirection === "asc"
                                                ? "desc"
                                                : "asc";
                                    } else {
                                        sortBy = "lastUsed";
                                        sortDirection = "desc";
                                    }
                                }}
                                title={$_("contextSwitcher.sortByLastUsed")}
                            >
                                <Clock size={14} />
                            </button>
                        </div>

                        <button
                            class="text-xs bg-primary text-primary-foreground px-3 py-1.5 rounded-md hover:bg-primary/90 transition-colors shadow-sm font-medium"
                            onclick={addContext}
                            >{$_("contexts.addContext")}</button
                        >
                    </div>
                </div>

                <div class="space-y-4">
                    <!-- Default Context (from database) -->
                    {#if defaultContext}
                        {@const defaultIndex = contexts.findIndex(
                            (c) => c.id === "default",
                        )}
                        <ContextManagerItem
                            isDefault={true}
                            bind:context={contexts[defaultIndex]}
                            stats={{
                                count: stashCounts["default"] || 0,
                                size: contextSizes["default"] || 0,
                            }}
                            onSave={saveContext}
                            onExport={() => openExportDialog(defaultContext)}
                            onImport={() => openImportDialog(defaultContext)}
                            onSelect={onSelect
                                ? () => onSelect?.("default")
                                : undefined}
                        />
                    {/if}

                    {#each sortedContexts as context (context.id)}
                        {@const originalIndex = contexts.findIndex(
                            (c) => c.id === context.id,
                        )}
                        <ContextManagerItem
                            bind:context={contexts[originalIndex]}
                            autoFocus={context.id === newlyCreatedContextId}
                            stats={{
                                count: stashCounts[context.id] || 0,
                                size: contextSizes[context.id] || 0,
                            }}
                            onSave={saveContext}
                            onDelete={(shift) => {
                                if (shift) {
                                    removeContext(context.id);
                                } else {
                                    contextToDelete = context.id;
                                    deleteConfirmationOpen = true;
                                }
                            }}
                            onExport={() => openExportDialog(context)}
                            onImport={() => openImportDialog(context)}
                            onSelect={onSelect
                                ? () => onSelect?.(context.id)
                                : undefined}
                        />
                    {/each}
                </div>
            {/if}
        </div>
    </div>

    {#if contextToDelete !== null}
        <ConfirmationDialog
            bind:open={deleteConfirmationOpen}
            title={$_("contexts.deleteConfirm")}
            description=""
            confirmText={$_("common.delete")}
            variant="destructive"
            onConfirm={() => {
                if (contextToDelete !== null) {
                    removeContext(contextToDelete);
                    contextToDelete = null;
                }
            }}
            onCancel={() => {
                contextToDelete = null;
            }}
        />
    {/if}

    {#if exportDialogOpen && exportContext}
        <ExportDialog
            bind:open={exportDialogOpen}
            context={exportContext}
            stashes={getContextStashes(exportContext)}
            onClose={() => (exportContext = null)}
        />
    {/if}

    {#if importDialogOpen && importContext}
        <ImportDialog
            bind:open={importDialogOpen}
            context={importContext}
            existingStashes={getContextStashes(importContext)}
            onClose={() => {
                importContext = null;
                importDialogOpen = false;
            }}
            onImportComplete={() => {
                load();
            }}
        />
    {/if}
</div>
