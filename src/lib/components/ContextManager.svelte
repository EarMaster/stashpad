<!--
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2025 Nico Wiedemann
-->

<script lang="ts">
    import { DesktopStorageAdapter } from "$lib/services/desktop-adapter";
    import type { Settings } from "$lib/types";
    import { _ } from "$lib/i18n";

    let { onBack } = $props<{ onBack: () => void }>();

    let settings = $state<Settings>({
        autoContextDetection: true,
        contexts: [],
        activeContextId: undefined,
        shortcuts: {},
    });
    let isLoading = $state(true);

    const adapter = new DesktopStorageAdapter();

    async function load() {
        try {
            settings = await adapter.getSettings();
            if (!settings.contexts) settings.contexts = [];
        } catch (e) {
            console.error("Failed to load settings", e);
        } finally {
            isLoading = false;
        }
    }

    async function save() {
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
            },
        ];
        save();
    }

    function removeContext(index: number) {
        settings.contexts.splice(index, 1);
        save();
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
        save();
    }

    function removeRule(contextIndex: number, ruleIndex: number) {
        settings.contexts[contextIndex].rules.splice(ruleIndex, 1);
        save();
    }
</script>

<div class="h-full flex flex-col bg-background">
    <!-- Header -->
    <div
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
    <div class="flex-1 overflow-y-auto p-4 space-y-6">
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
                                onchange={save}
                                placeholder={$_(
                                    "contexts.contextNamePlaceholder",
                                )}
                            />
                            <button
                                class="text-muted-foreground hover:text-destructive text-xs px-2 py-1 rounded hover:bg-muted"
                                onclick={(e) => {
                                    if (
                                        e.shiftKey ||
                                        confirm($_("contexts.deleteConfirm"))
                                    ) {
                                        removeContext(i);
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
                                <span>{$_("contexts.autoSwitchRules")}</span>
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
                                        onchange={save}
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
                                        onchange={save}
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
                                        onchange={save}
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
