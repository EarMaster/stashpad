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
        CaseSensitive,
        Braces,
        FolderOpen,
        Unlink,
        Sparkles,
    } from "lucide-svelte";
    import { formatBytes, ATTACHMENT_SIZE_LIMITS } from "$lib/utils/format";
    import { getRelativeTime } from "$lib/utils/date";
    import ActionButton from "./ActionButton.svelte";
    import { tooltip } from "$lib/actions/tooltip";
    import { safeParse } from "$lib/utils/markdown";
    import { externalLinks } from "$lib/actions/externalLinks";
    import { tick } from "svelte";
    import {
        getContextFolder,
        linkContextFolder,
        loadContextFolders,
        unlinkContextFolder,
    } from "$lib/stores/context-folders.svelte";
    import { log } from "$lib/utils/log";
    import { errorText } from "$lib/errors";
    import { FOLDER_VARIABLE, usesFolderVariable } from "$lib/utils/rules";
    import { DesktopStorageAdapter } from "$lib/services/desktop-adapter";
    import ConfirmationDialog from "./ConfirmationDialog.svelte";
    import type { DigestPreview } from "$lib/types";

    let {
        context = $bindable(),
        stats = { count: 0, total: 0, size: 0 },
        isDefault = false,
        autoFocus = false,
        aiReady = false,
        onSave,
        onDelete,
        onExport,
        onImport,
        onSelect,
    } = $props<{
        context: Context;
        /**
         * `count` is active stashes, shown in the badge. `total` includes completed ones
         * and is what export is gated on, since export writes those too.
         */
        stats: { count: number; total: number; size: number };
        isDefault?: boolean;
        autoFocus?: boolean;
        /** The AI provider is switched on and set up. */
        aiReady?: boolean;
        onSave?: (context: Context) => void;
        onDelete?: (shiftKey: boolean) => void;
        onExport: () => void;
        onImport: () => void;
        onSelect?: () => void;
    }>();

    let inputElement = $state<HTMLInputElement>();
    let descriptionTextarea = $state<HTMLTextAreaElement>();
    let isEditingDescription = $state(false);

    // The project folder lives on this device only, outside the synced context.
    let folder = $derived(getContextFolder(context.id));
    $effect(() => {
        loadContextFolders();
    });

    async function pickFolder() {
        try {
            await linkContextFolder(context.id, $_("contexts.folder.pickTitle"));
        } catch (e) {
            log.error("Failed to link a project folder:", e);
        }
    }

    const adapter = new DesktopStorageAdapter();

    // Drafting a description from the folder. Nothing is sent until the user has seen what
    // would be, and the result is only a proposal until they accept it.
    let digest = $state<DigestPreview | null>(null);
    let confirmSuggestOpen = $state(false);
    let isSuggesting = $state(false);
    let suggestion = $state<string | null>(null);
    let suggestError = $state("");

    async function startSuggestion() {
        suggestError = "";
        try {
            const preview = await adapter.previewContextDigest(context.id);
            if (preview.files.length === 0) {
                suggestError = $_("contexts.suggest.nothingToRead");
                return;
            }
            digest = preview;
            confirmSuggestOpen = true;
        } catch (e) {
            log.error("Failed to prepare the description suggestion:", e);
            suggestError = errorText(e, $_("contexts.suggest.failed"));
        }
    }

    async function draftDescription() {
        confirmSuggestOpen = false;
        isSuggesting = true;
        suggestError = "";
        try {
            suggestion = await adapter.suggestContextDescription(context.id);
        } catch (e) {
            log.error("Failed to draft a description:", e);
            suggestError = errorText(e, $_("contexts.suggest.failed"));
        } finally {
            isSuggesting = false;
        }
    }

    function acceptSuggestion() {
        if (suggestion === null) return;
        context.description = suggestion.trim();
        suggestion = null;
        onSave?.(context);
    }

    async function unlinkFolder() {
        try {
            await unlinkContextFolder(context.id);
        } catch (e) {
            log.error("Failed to unlink the project folder:", e);
        }
    }

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
                ruleType: "title",
                matchType: "contains",
                value: "",
                matchCase: false,
                useRegex: false,
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
                disabled={stats.total === 0}
                title={$_("contexts.exportContext")}
            >
                <Upload size={14} />
                <span class="hidden sm:inline">{$_("contexts.export")}</span>
            </ActionButton>

            <ActionButton
                variant="context"
                onclick={onImport}
                title={$_("contexts.importContext")}
            >
                <Download size={14} />
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
                        {@html safeParse(context.description)}
                    {:else}
                        <span class="text-muted-foreground italic"
                            >{$_("contexts.description.placeholder")}</span
                        >
                    {/if}
                </div>
            {/if}
        </div>
    {/if}

    <!-- Project folder on this device (only for non-default) -->
    {#if !isDefault}
        <div>
            <div class="flex justify-between items-center mb-1">
                <span
                    class="text-[10px] text-muted-foreground font-medium uppercase tracking-wider"
                >
                    {$_("contexts.folder.label")}
                </span>
                <div class="flex gap-1">
                    {#if folder?.exists && aiReady}
                        <ActionButton
                            variant="context"
                            title={$_("contexts.suggest.button")}
                            disabled={isSuggesting}
                            onclick={startSuggestion}
                        >
                            <Sparkles size={12} />
                            {isSuggesting
                                ? $_("contexts.suggest.working")
                                : $_("contexts.suggest.button")}
                        </ActionButton>
                    {/if}
                    <ActionButton
                        variant="context"
                        title={folder
                            ? $_("contexts.folder.change")
                            : $_("contexts.folder.link")}
                        onclick={pickFolder}
                    >
                        <FolderOpen size={12} />
                        {folder
                            ? $_("contexts.folder.change")
                            : $_("contexts.folder.link")}
                    </ActionButton>
                    {#if folder}
                        <ActionButton
                            variant="context"
                            title={$_("contexts.folder.unlink")}
                            onclick={unlinkFolder}
                        >
                            <Unlink size={12} />
                            <span class="hidden sm:inline"
                                >{$_("contexts.folder.unlink")}</span
                            >
                        </ActionButton>
                    {/if}
                </div>
            </div>
            {#if folder}
                <div class="px-2 text-xs font-mono break-all">
                    <span class={folder.exists ? "" : "text-muted-foreground line-through"}
                        >{folder.path}</span
                    >
                    {#if !folder.exists}
                        <span class="block font-sans text-amber-500"
                            >{$_("contexts.folder.missing")}</span
                        >
                    {/if}
                </div>
            {:else}
                <p class="px-2 text-xs text-muted-foreground italic">
                    {$_("contexts.folder.hint")}
                </p>
            {/if}
            {#if suggestError}
                <p class="px-2 pt-1 text-xs text-destructive" role="alert">
                    {suggestError}
                </p>
            {/if}
            {#if suggestion !== null}
                <div
                    class="mt-2 p-2 rounded-md border border-primary/40 bg-primary/5 space-y-2"
                >
                    <label
                        for="context-suggestion-{context.id}"
                        class="block text-[10px] text-muted-foreground font-medium uppercase tracking-wider"
                    >
                        {$_("contexts.suggest.proposalLabel")}
                    </label>
                    <textarea
                        id="context-suggestion-{context.id}"
                        class="block w-full bg-muted/50 rounded-md px-3 py-2 text-xs border border-transparent focus:border-primary/50 outline-none min-h-[120px]"
                        bind:value={suggestion}
                        rows="5"
                    ></textarea>
                    {#if context.description}
                        <p class="text-xs text-muted-foreground">
                            {$_("contexts.suggest.replaces")}
                        </p>
                    {/if}
                    <div class="flex gap-2 justify-end">
                        <ActionButton
                            variant="context"
                            onclick={() => (suggestion = null)}
                        >
                            {$_("contexts.suggest.discard")}
                        </ActionButton>
                        <ActionButton
                            variant="context"
                            onclick={acceptSuggestion}
                        >
                            {$_("contexts.suggest.use")}
                        </ActionButton>
                    </div>
                </div>
            {/if}
        </div>
    {/if}

    <ConfirmationDialog
        bind:open={confirmSuggestOpen}
        title={$_("contexts.suggest.title")}
        description={digest?.onDevice
            ? $_("contexts.suggest.onDevice", {
                  values: { count: digest.layoutEntries },
              })
            : $_("contexts.suggest.sends", {
                  values: {
                      count: digest?.layoutEntries ?? 0,
                      provider: digest?.provider ?? "",
                  },
              })}
        confirmText={digest?.onDevice
            ? $_("contexts.suggest.confirmOnDevice")
            : $_("contexts.suggest.confirm")}
        onConfirm={draftDescription}
    >
        <ul class="text-xs font-mono space-y-0.5 max-h-32 overflow-y-auto">
            {#each digest?.files ?? [] as name (name)}
                <li class="truncate">{name}</li>
            {/each}
        </ul>
    </ConfirmationDialog>

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

            <p class="text-[10px] text-muted-foreground normal-case tracking-normal">
                {$_("contexts.rules.folderHint", {
                    values: { token: FOLDER_VARIABLE },
                })}
            </p>

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
                    <div
                        class="flex items-center gap-1 border-r border-border/50 pr-2"
                    >
                        <button
                            class="p-1 rounded text-muted-foreground hover:bg-muted transition-colors {rule.matchCase
                                ? 'bg-primary/20 text-primary hover:bg-primary/30'
                                : ''}"
                            onclick={() => {
                                rule.matchCase = !rule.matchCase;
                                onSave?.(context);
                            }}
                            title={$_("contexts.rules.matchCase")}
                            use:tooltip
                        >
                            <CaseSensitive size={14} />
                        </button>
                        <button
                            class="p-1 rounded text-muted-foreground hover:bg-muted transition-colors {rule.useRegex
                                ? 'bg-primary/20 text-primary hover:bg-primary/30'
                                : ''}"
                            onclick={() => {
                                rule.useRegex = !rule.useRegex;
                                onSave?.(context);
                            }}
                            title={$_("contexts.rules.useRegex")}
                            use:tooltip
                        >
                            <Braces size={14} />
                        </button>
                    </div>
                    <button
                        class="text-muted-foreground hover:text-destructive opacity-0 group-hover:opacity-100 transition-opacity ml-auto sm:ml-0"
                        onclick={() => removeRule(j)}
                        title={$_("common.remove")}
                        use:tooltip
                    >
                        ×
                    </button>
                </div>
                {#if usesFolderVariable(rule) && !folder}
                    <p class="text-[10px] text-amber-500" role="status">
                        {$_("contexts.rules.folderMissing")}
                    </p>
                {/if}
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
