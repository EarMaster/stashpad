<!--
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2025 Nico Wiedemann
-->

<script lang="ts">
    import { DesktopStorageAdapter } from "$lib/services/desktop-adapter";
    import type { Settings, StashItem, Context } from "$lib/types";
    import { _ } from "$lib/i18n";
    import ConfirmationDialog from "./ConfirmationDialog.svelte";
    import { save } from "@tauri-apps/plugin-dialog";
    import { writeTextFile } from "@tauri-apps/plugin-fs";
    import { Download } from "lucide-svelte";

    let { onBack } = $props<{ onBack: () => void }>();

    let settings = $state<Settings>({
        autoContextDetection: true,
        contexts: [],
        activeContextId: undefined,
        shortcuts: {},
    });
    let stashCounts = $state<Record<string, number>>({});
    let allStashes = $state<StashItem[]>([]);
    let isLoading = $state(true);
    let contextToDelete = $state<number | null>(null);
    let isExporting = $state(false);

    const adapter = new DesktopStorageAdapter();

    async function load() {
        try {
            settings = await adapter.getSettings();
            if (!settings.contexts) settings.contexts = [];

            // Load stashes and count per context
            const stashes = await adapter.loadStashes();
            allStashes = stashes;
            const counts: Record<string, number> = { default: 0 };
            settings.contexts.forEach((ctx) => (counts[ctx.id] = 0));

            stashes.forEach((stash: StashItem) => {
                if (!stash.completed) {
                    const ctxId = stash.contextId || "default";
                    counts[ctxId] = (counts[ctxId] || 0) + 1;
                }
            });
            stashCounts = counts;
        } catch (e) {
            console.error("Failed to load settings", e);
        } finally {
            isLoading = false;
        }
    }

    async function saveSettings() {
        try {
            await adapter.saveSettings(settings);
        } catch (e) {
            console.error("Failed to save settings", e);
        }
    }

    $effect(() => {
        load();
    });

    function addContext() {
        settings.contexts = [
            ...settings.contexts,
            {
                id: crypto.randomUUID(),
                name: $_("contexts.newContext"),
                rules: [],
                lastUsed: new Date().toISOString(),
            },
        ];
        saveSettings();
    }

    function removeContext(index: number) {
        settings.contexts.splice(index, 1);
        saveSettings();
    }

    function addRule(contextIndex: number) {
        settings.contexts[contextIndex].rules = [
            ...settings.contexts[contextIndex].rules,
            {
                ruleType: "process",
                matchType: "contains",
                value: "",
            },
        ];
        saveSettings();
    }

    function removeRule(contextIndex: number, ruleIndex: number) {
        settings.contexts[contextIndex].rules.splice(ruleIndex, 1);
        saveSettings();
    }

    /**
     * Export all stashes from a context as a single markdown file.
     */
    async function exportContext(context: Context) {
        isExporting = true;
        try {
            // Filter stashes for this context
            const contextStashes = allStashes.filter(
                (s) => (s.contextId || "default") === context.id,
            );

            if (contextStashes.length === 0) {
                // Nothing to export - could show a message but for now just return
                return;
            }

            // Build markdown content
            const lines: string[] = [];
            lines.push(`# ${context.name}`);
            lines.push("");
            lines.push(
                `Exported from Stashpad on ${new Date().toLocaleDateString()}`,
            );
            lines.push("");
            lines.push(`Total stashes: ${contextStashes.length}`);
            lines.push("");
            lines.push("---");
            lines.push("");

            // Sort by created date (newest first)
            const sorted = [...contextStashes].sort(
                (a, b) =>
                    new Date(b.createdAt).getTime() -
                    new Date(a.createdAt).getTime(),
            );

            for (const stash of sorted) {
                // Add stash header with date and status
                const date = new Date(stash.createdAt).toLocaleString();
                const status = stash.completed ? "✓ Completed" : "Active";
                lines.push(`## [${status}] ${date}`);
                lines.push("");

                // Add content
                if (stash.content.trim()) {
                    lines.push(stash.content);
                    lines.push("");
                }

                // Add attachments
                if (stash.files && stash.files.length > 0) {
                    lines.push("**Attachments:**");
                    for (const file of stash.files) {
                        const fileName = file.split(/[\\/]/).pop() || file;
                        lines.push(`- ${fileName}`);
                    }
                    lines.push("");
                }

                lines.push("---");
                lines.push("");
            }

            const markdownContent = lines.join("\n");

            // Generate safe filename
            const safeName = context.name
                .replace(/[^a-zA-Z0-9_-]/g, "_")
                .toLowerCase();
            const timestamp = new Date().toISOString().slice(0, 10);
            const defaultFileName = `${safeName}_${timestamp}.md`;

            // Open save dialog
            const filePath = await save({
                title: $_("contexts.exportTitle"),
                defaultPath: defaultFileName,
                filters: [
                    {
                        name: "Markdown",
                        extensions: ["md"],
                    },
                ],
            });

            console.log("Export to:", filePath);

            if (filePath) {
                // Write the file
                await writeTextFile(filePath, markdownContent);
            }
        } catch (e) {
            console.error("Failed to export context", e);
        } finally {
            isExporting = false;
        }
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
                    {#each settings.contexts as context, i}
                        <div
                            class="rounded-lg border border-border bg-card p-4 space-y-3 shadow-sm"
                        >
                            <div class="flex items-center gap-2">
                                <input
                                    class="flex-1 bg-transparent font-medium focus:outline-none border-b border-transparent focus:border-primary/50 text-sm py-1"
                                    bind:value={context.name}
                                    onchange={saveSettings}
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
                                    onclick={() => exportContext(context)}
                                    disabled={isExporting ||
                                        (stashCounts[context.id] ?? 0) === 0}
                                    title={$_("contexts.exportContext")}
                                >
                                    <Download size={12} />
                                    {$_("contexts.export")}
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
                                            onchange={saveSettings}
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
                                            onchange={saveSettings}
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
                                            onchange={saveSettings}
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

                    {#if settings.contexts.length === 0}
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
</div>
