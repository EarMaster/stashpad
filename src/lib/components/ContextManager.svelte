<!--
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2025 Nico Wiedemann
-->

<script lang="ts">
    import { DesktopStorageAdapter } from "$lib/services/desktop-adapter";
    import type { Settings, StashItem, Context } from "$lib/types";
    import { _ } from "$lib/i18n";
    import ConfirmationDialog from "./ConfirmationDialog.svelte";
    import ExportDialog from "./ExportDialog.svelte";
    import ImportDialog from "./ImportDialog.svelte";
    import { Download, Upload } from "lucide-svelte";

    let { onBack } = $props<{ onBack: () => void }>();

    let contexts = $state<Context[]>([]);
    let stashCounts = $state<Record<string, number>>({});
    let allStashes = $state<StashItem[]>([]);
    let isLoading = $state(true);
    let contextToDelete = $state<number | null>(null);

    // Export dialog state
    let exportDialogOpen = $state(false);
    let exportContext = $state<Context | null>(null);

    // Import dialog state
    let importDialogOpen = $state(false);
    let importContext = $state<Context | null>(null);

    const adapter = new DesktopStorageAdapter();

    async function load() {
        try {
            contexts = await adapter.getContexts();

            // Load stashes and count per context
            const stashes = await adapter.loadStashes();
            allStashes = stashes;
            const counts: Record<string, number> = { default: 0 };
            contexts.forEach((ctx) => (counts[ctx.id] = 0));

            stashes.forEach((stash: StashItem) => {
                if (!stash.completed) {
                    const ctxId = stash.contextId || "default";
                    counts[ctxId] = (counts[ctxId] || 0) + 1;
                }
            });
            stashCounts = counts;
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

    function addContext() {
        contexts = [
            ...contexts,
            {
                id: crypto.randomUUID(),
                name: $_("contexts.newContext"),
                rules: [],
                lastUsed: new Date().toISOString(),
            },
        ];
        saveContexts();
    }

    function removeContext(index: number) {
        contexts.splice(index, 1);
        saveContexts();
    }

    function addRule(contextIndex: number) {
        contexts[contextIndex].rules = [
            ...contexts[contextIndex].rules,
            {
                ruleType: "process",
                matchType: "contains",
                value: "",
            },
        ];
        saveContexts();
    }

    function removeRule(contextIndex: number, ruleIndex: number) {
        contexts[contextIndex].rules.splice(ruleIndex, 1);
        saveContexts();
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
                <div class="flex items-center justify-between">
                    <p class="text-sm text-muted-foreground">
                        {$_("contexts.manageDescription")}
                    </p>
                    <button
                        class="text-xs bg-primary text-primary-foreground px-3 py-1.5 rounded-md hover:bg-primary/90 transition-colors shadow-sm font-medium"
                        onclick={addContext}>{$_("contexts.addContext")}</button
                    >
                </div>

                <div class="space-y-4">
                    {#each contexts as context, i}
                        <div
                            class="rounded-lg border border-border bg-card p-4 space-y-3 shadow-sm"
                        >
                            <div class="flex items-center gap-2">
                                <input
                                    class="flex-1 bg-transparent font-medium focus:outline-none border-b border-transparent focus:border-primary/50 text-sm py-1"
                                    bind:value={context.name}
                                    onchange={saveContexts}
                                    placeholder={$_(
                                        "contexts.contextNamePlaceholder",
                                    )}
                                />
                                {#if stashCounts[context.id] !== undefined}
                                    <span
                                        class="text-[10px] text-muted-foreground tabular-nums bg-muted px-1.5 py-0.5 rounded"
                                    >
                                        {stashCounts[context.id]}
                                    </span>
                                {/if}
                                <button
                                    class="text-muted-foreground hover:text-primary text-xs px-2 py-1 rounded hover:bg-muted flex items-center gap-1 transition-colors"
                                    onclick={() => openExportDialog(context)}
                                    disabled={(stashCounts[context.id] ?? 0) ===
                                        0}
                                    title={$_("contexts.exportContext")}
                                >
                                    <Download size={12} />
                                    {$_("contexts.export")}
                                </button>
                                <button
                                    class="text-muted-foreground hover:text-primary text-xs px-2 py-1 rounded hover:bg-muted flex items-center gap-1 transition-colors"
                                    onclick={() => openImportDialog(context)}
                                    title={$_("contexts.importContext")}
                                >
                                    <Upload size={12} />
                                    {$_("contexts.import")}
                                </button>
                                <button
                                    class="text-muted-foreground hover:text-destructive text-xs px-2 py-1 rounded hover:bg-muted"
                                    onclick={(e) => {
                                        if (e.shiftKey) {
                                            removeContext(i);
                                        } else {
                                            contextToDelete = i;
                                        }
                                    }}
                                    title={$_("contexts.shiftClickToSkip")}
                                    >{$_("contexts.removeContext")}</button
                                >
                            </div>

                            <!-- Rules -->
                            <div
                                class="pl-2 border-l-2 border-muted space-y-2 mt-2"
                            >
                                <div
                                    class="text-[10px] text-muted-foreground font-medium flex justify-between uppercase tracking-wider"
                                >
                                    <span>{$_("contexts.autoSwitchRules")}</span
                                    >
                                    <button
                                        class="text-xs text-primary hover:underline"
                                        onclick={() => addRule(i)}
                                        >{$_("contexts.addRule")}</button
                                    >
                                </div>

                                {#each context.rules as rule, j}
                                    <div
                                        class="flex items-center gap-2 text-xs group"
                                    >
                                        <select
                                            class="bg-muted/50 rounded px-2 py-1 border border-transparent focus:border-primary/50 outline-none"
                                            bind:value={rule.ruleType}
                                            onchange={saveContexts}
                                        >
                                            <option value="process"
                                                >{$_(
                                                    "contexts.rules.process",
                                                )}</option
                                            >
                                            <option value="title"
                                                >{$_(
                                                    "contexts.rules.title",
                                                )}</option
                                            >
                                        </select>
                                        <select
                                            class="bg-muted/50 rounded px-2 py-1 border border-transparent focus:border-primary/50 outline-none"
                                            bind:value={rule.matchType}
                                            onchange={saveContexts}
                                        >
                                            <option value="contains"
                                                >{$_(
                                                    "contexts.rules.contains",
                                                )}</option
                                            >
                                            <option value="exact"
                                                >{$_(
                                                    "contexts.rules.exact",
                                                )}</option
                                            >
                                        </select>
                                        <input
                                            class="flex-1 bg-muted/50 px-2 py-1 rounded border border-transparent focus:border-primary/50 outline-none"
                                            bind:value={rule.value}
                                            onchange={saveContexts}
                                            placeholder={$_(
                                                "contexts.rules.valuePlaceholder",
                                            )}
                                        />
                                        <button
                                            class="text-muted-foreground hover:text-destructive px-1.5 opacity-0 group-hover:opacity-100 transition-opacity"
                                            onclick={() => removeRule(i, j)}
                                            >×</button
                                        >
                                    </div>
                                {/each}
                                {#if context.rules.length === 0}
                                    <div
                                        class="text-[10px] text-muted-foreground/50 italic py-1"
                                    >
                                        {$_("contexts.noRules")}
                                    </div>
                                {/if}
                            </div>
                        </div>
                    {/each}

                    {#if contexts.length === 0}
                        <div
                            class="text-center py-12 text-muted-foreground/50 text-sm italic border-2 border-dashed border-border/50 rounded-xl bg-muted/5"
                        >
                            {$_("contexts.noContexts")}
                        </div>
                    {/if}
                </div>
            {/if}
        </div>
    </div>

    {#if contextToDelete !== null}
        <ConfirmationDialog
            open={true}
            title={$_("contexts.deleteConfirm")}
            description={$_("contexts.deleteConfirm")}
            confirmText={$_("common.delete")}
            variant="destructive"
            onConfirm={() => {
                if (contextToDelete !== null) removeContext(contextToDelete);
                contextToDelete = null;
            }}
            onCancel={() => (contextToDelete = null)}
        />
    {/if}

    {#if exportContext}
        <ExportDialog
            bind:open={exportDialogOpen}
            context={exportContext}
            stashes={getContextStashes(exportContext)}
            onClose={() => {
                exportDialogOpen = false;
                exportContext = null;
            }}
        />
    {/if}

    {#if importContext}
        <ImportDialog
            bind:open={importDialogOpen}
            context={importContext}
            existingStashes={getContextStashes(importContext)}
            onClose={() => {
                importDialogOpen = false;
                importContext = null;
            }}
            onImportComplete={() => {
                load();
            }}
        />
    {/if}
</div>
