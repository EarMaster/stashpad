<!--
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2025 Nico Wiedemann
-->

<script lang="ts">
    import type { Context } from "$lib/types";
    import { _, locale } from "$lib/i18n";
    import {
        Download,
        Upload,
        ExternalLink,
        Trash2,
        Plus,
        Pencil,
    } from "lucide-svelte";
    import { formatBytes, ATTACHMENT_SIZE_LIMITS } from "$lib/utils/format";
    import { getRelativeTime } from "$lib/utils/date";
    import ActionButton from "./ActionButton.svelte";
    import { tooltip } from "$lib/actions/tooltip";
    import marked from "$lib/utils/markdown";
    import { externalLinks } from "$lib/actions/externalLinks";
    import { tick } from "svelte";

    let {
        context = $bindable(),
        stats = { count: 0, size: 0 },
        isDefault = false,
        autoFocus = false,
        onSave,
        onDelete,
        onExport,
        onImport,
        onSelect,
    } = $props<{
        context: Context;
        stats: { count: number; size: number };
        isDefault?: boolean;
        autoFocus?: boolean;
        onSave?: (context: Context) => void;
        onDelete?: (shiftKey: boolean) => void;
        onExport: () => void;
        onImport: () => void;
        onSelect?: () => void;
    }>();

    let inputElement = $state<HTMLInputElement>();
    let descriptionTextarea = $state<HTMLTextAreaElement>();
    let isEditingDescription = $state(false);

    async function focusDescription() {
        isEditingDescription = true;
        await tick();
        descriptionTextarea?.focus();
    }

    $effect(() => {
        if (autoFocus && inputElement) {
            inputElement.focus();
            inputElement.select();
        }
    });

    function addRule() {
        if (isDefault) return;
        context.rules = [
            ...context.rules,
            {
                ruleType: "process",
                matchType: "contains",
                value: "",
            },
        ];
        onSave?.(context);
    }

    function removeRule(index: number) {
        if (isDefault) return;
        context.rules = context.rules.filter((_, i) => i !== index);
        onSave?.(context);
    }
</script>

<div class="rounded-lg border border-border bg-card p-4 space-y-3 shadow-sm">
    <!-- Title Bar -->
    <div class="flex items-center gap-2">
        {#if isDefault}
            <span class="flex-1 font-medium text-sm text-muted-foreground">
                {$_("contexts.defaultContext")}
            </span>
        {:else}
            <input
                bind:this={inputElement}
                class="flex-1 bg-transparent font-medium focus:outline-none border-b border-transparent focus:border-primary/50 text-sm py-1"
                bind:value={context.name}
                onblur={() => onSave?.(context)}
                placeholder={$_("contexts.contextNamePlaceholder")}
            />
        {/if}

        <!-- Actions -->
        <div class="flex items-center gap-1">
            <ActionButton
                variant="context"
                onclick={onExport}
                disabled={stats.count === 0}
                title={$_("contexts.exportContext")}
            >
                <Download size={14} />
                <span class="hidden sm:inline">{$_("contexts.export")}</span>
            </ActionButton>

            <ActionButton
                variant="context"
                onclick={onImport}
                title={$_("contexts.importContext")}
            >
                <Upload size={14} />
                <span class="hidden sm:inline">{$_("contexts.import")}</span>
            </ActionButton>

            {#if onSelect}
                <ActionButton
                    variant="context"
                    onclick={onSelect}
                    title={$_("contextSwitcher.selectContext")}
                >
                    <ExternalLink size={14} />
                    <span class="hidden sm:inline"
                        >{$_("contextSwitcher.selectContext")}</span
                    >
                </ActionButton>
            {/if}

            {#if !isDefault}
                <ActionButton
                    variant="context"
                    danger={true}
                    onclick={(e) => {
                        onDelete?.(e.shiftKey);
                    }}
                    title={$_("contexts.shiftClickToSkip")}
                >
                    <Trash2 size={14} />
                    <span class="hidden sm:inline"
                        >{$_("contexts.removeContext")}</span
                    >
                </ActionButton>
            {/if}
        </div>
    </div>

    <!-- Description (only for non-default) -->
    {#if !isDefault}
        <div>
            <div class="flex justify-between items-center mb-1">
                <label
                    for="context-desc-{context.id}"
                    class="text-[10px] text-muted-foreground font-medium uppercase tracking-wider"
                >
                    {$_("contexts.description.label")}
                </label>
                {#if !isEditingDescription && context.description}
                    <ActionButton
                        variant="context"
                        title={$_("common.edit")}
                        onclick={focusDescription}
                    >
                        <Pencil size={12} />
                        {$_("common.edit")}
                    </ActionButton>
                {/if}
            </div>

            {#if isEditingDescription}
                <textarea
                    id="context-desc-{context.id}"
                    bind:this={descriptionTextarea}
                    class="block w-full bg-muted/50 rounded-md px-3 py-2 text-xs border border-transparent focus:border-primary/50 outline-none min-h-[120px]"
                    bind:value={context.description}
                    onblur={() => {
                        onSave?.(context);
                        isEditingDescription = false;
                    }}
                    placeholder={$_("contexts.description.placeholder")}
                    rows="2"
                ></textarea>
            {:else}
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div
                    class="prose dark:prose-invert prose-xs max-w-none text-xs min-h-[40px] p-2 rounded-md hover:bg-muted/30 cursor-text border border-transparent hover:border-border/50 transition-colors"
                    use:externalLinks
                    onclick={focusDescription}
                >
                    {#if context.description}
                        {@html marked.parse(context.description)}
                    {:else}
                        <span class="text-muted-foreground italic"
                            >{$_("contexts.description.placeholder")}</span
                        >
                    {/if}
                </div>
            {/if}
        </div>
    {/if}

    <!-- Rules (only for non-default) -->
    {#if !isDefault}
        <div class="pl-2 border-l-2 border-muted">
            <div
                class="text-[10px] text-muted-foreground font-medium flex justify-between items-center uppercase tracking-wider"
            >
                <span>{$_("contexts.autoSwitchRules")}</span>
                <ActionButton
                    variant="context"
                    onclick={addRule}
                    title={$_("contexts.addRule")}
                >
                    <Plus size={14} />
                    <span class="hidden sm:inline"
                        >{$_("contexts.addRule")}</span
                    >
                </ActionButton>
            </div>

            {#each context.rules as rule, j}
                <div class="flex flex-wrap items-center gap-2 text-xs group">
                    <select
                        class="bg-muted/50 rounded px-2 py-1 border border-transparent focus:border-primary/50 outline-none"
                        bind:value={rule.ruleType}
                        onblur={() => onSave?.(context)}
                    >
                        <option value="process"
                            >{$_("contexts.rules.process")}</option
                        >
                        <option value="title"
                            >{$_("contexts.rules.title")}</option
                        >
                    </select>
                    <select
                        class="bg-muted/50 rounded px-2 py-1 border border-transparent focus:border-primary/50 outline-none"
                        bind:value={rule.matchType}
                        onblur={() => onSave?.(context)}
                    >
                        <option value="contains"
                            >{$_("contexts.rules.contains")}</option
                        >
                        <option value="exact"
                            >{$_("contexts.rules.exact")}</option
                        >
                    </select>
                    <input
                        class="flex-1 min-w-0 sm:min-w-[120px] bg-muted/50 rounded px-2 py-1 border border-transparent focus:border-primary/50 outline-none"
                        bind:value={rule.value}
                        onblur={() => onSave?.(context)}
                        placeholder={$_("contexts.rules.valuePlaceholder")}
                    />
                    <button
                        class="text-muted-foreground hover:text-destructive opacity-0 group-hover:opacity-100 transition-opacity ml-auto sm:ml-0"
                        onclick={() => removeRule(j)}
                        title={$_("common.remove")}
                        use:tooltip
                    >
                        ×
                    </button>
                </div>
            {/each}
            {#if context.rules.length === 0}
                <div class="text-[10px] text-muted-foreground italic">
                    {$_("contexts.noRules")}
                </div>
            {/if}
        </div>
    {/if}

    <!-- Statistics Section -->
    <div class="flex items-center gap-4 text-xs text-muted-foreground px-1">
        <div class="flex items-center gap-1.5">
            <span class="font-medium">{stats.count}</span>
            <span class="opacity-70">
                {stats.count === 1
                    ? $_("contexts.stats.stash")
                    : $_("contexts.stats.stashes")}
            </span>
        </div>
        <div class="flex items-center gap-1.5">
            <span class="font-medium">
                {formatBytes(stats.size, $locale || "en")} / {formatBytes(
                    ATTACHMENT_SIZE_LIMITS.MAX_CONTEXT_TOTAL,
                    $locale || "en",
                )}
            </span>
            <span class="opacity-70">{$_("contexts.stats.used")}</span>
        </div>
        {#if context.lastUsed}
            <div class="flex items-center gap-1.5">
                <span class="opacity-70">{$_("contexts.stats.lastUsed")}</span>
                <span class="font-medium"
                    >{getRelativeTime(context.lastUsed, $_)}</span
                >
            </div>
        {/if}
    </div>
</div>
