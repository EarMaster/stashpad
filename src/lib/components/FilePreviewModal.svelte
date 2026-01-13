<!--
// SPDX-License-Identifier: AGPL-3.0-only

// Copyright (C) 2026 Nico Wiedemann
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
    import { _ } from "$lib/i18n";
    import { fade, scale } from "svelte/transition";
    import {
        X,
        Download,
        ExternalLink,
        FileText,
        ChevronLeft,
        ChevronRight,
        ChevronDown,
        Code,
    } from "lucide-svelte";
    import type { FilePreviewData } from "$lib/types";
    import { convertFileSrc, invoke } from "@tauri-apps/api/core";
    import { DesktopStorageAdapter } from "$lib/services/desktop-adapter";
    import {
        highlightCode,
        getLanguageForExtension,
        SUPPORTED_LANGUAGES,
    } from "$lib/utils/language-detection";
    import markedInstance from "$lib/utils/markdown";
    import { formatBytes } from "$lib/utils/format";
    import { locale } from "$lib/i18n";
    import { tooltip } from "$lib/actions/tooltip";

    let {
        open = $bindable(false),
        files = [],
        filePath = $bindable(""),
        onClose,
    } = $props<{
        open?: boolean;
        files?: string[];
        filePath?: string;
        onClose: () => void;
    }>();

    let dialogRef = $state<HTMLDialogElement | null>(null);
    let contentRef = $state<HTMLDivElement | null>(null);
    let previewData = $state<FilePreviewData | null>(null);
    let loading = $state(false);

    /**
     * Convert file path to video source URL for Tauri.
     * Uses convertFileSrc to properly handle local file paths.
     */
    function getVideoSrc(path: string): string {
        return convertFileSrc(path);
    }

    const adapter = new DesktopStorageAdapter();
    let volume = $state(0.5);
    let muted = $state(true);
    let saveTimeout: ReturnType<typeof setTimeout>;

    async function loadPreview(path: string) {
        if (!path) return;
        loading = true;
        try {
            previewData = await adapter.readFileForPreview(path);
        } catch (err) {
            console.error("Failed to load file preview:", err);
            previewData = null;
        } finally {
            loading = false;
        }
    }

    let currentIndex = $derived(files.indexOf(filePath));
    let hasPrev = $derived(currentIndex > 0);
    let hasNext = $derived(
        currentIndex >= 0 && currentIndex < files.length - 1,
    );

    /**
     * User-selected language override. Null means auto-detect.
     */
    let selectedLanguage = $state<string | null>(null);

    /**
     * Check if the current file is a markdown file based on extension or selection.
     */
    let isMarkdown = $derived(() => {
        if (selectedLanguage === "markdown") return true;
        if (selectedLanguage && selectedLanguage !== "markdown") return false;
        if (!previewData) return false;
        const ext = previewData.fileName.split(".").pop()?.toLowerCase() ?? "";
        return ext === "md" || ext === "markdown";
    });

    /**
     * Get the current language (either user-selected or auto-detected).
     */
    let currentLanguage = $derived(() => {
        if (selectedLanguage) return selectedLanguage;
        if (!previewData || previewData.fileType !== "text") return null;
        const ext = previewData.fileName.split(".").pop()?.toLowerCase() ?? "";
        return getLanguageForExtension(ext);
    });

    /**
     * Syntax-highlighted HTML content (loaded asynchronously).
     */
    let highlightedContent = $state("");
    let highlightLoading = $state(false);

    /**
     * Load syntax-highlighted content when preview data or language changes.
     */
    $effect(() => {
        if (!previewData || previewData.fileType !== "text") {
            highlightedContent = "";
            return;
        }

        // For markdown files, render synchronously as HTML
        if (isMarkdown()) {
            highlightedContent = markedInstance.parse(
                previewData.content,
            ) as string;
            return;
        }

        // For code files, load syntax highlighting asynchronously
        highlightLoading = true;
        highlightCode(previewData.content, currentLanguage() ?? undefined)
            .then((result) => {
                highlightedContent = result.html;
                highlightLoading = false;
            })
            .catch(() => {
                // Fallback to plain text if highlighting fails
                highlightedContent = previewData?.content ?? "";
                highlightLoading = false;
            });
    });

    /**
     * Reset selected language when file changes.
     */
    $effect(() => {
        if (filePath) {
            selectedLanguage = null;
        }
    });

    function navigate(direction: number) {
        if (!files || files.length <= 1 || currentIndex === -1) return;

        let newIndex = currentIndex + direction;
        if (newIndex < 0 || newIndex >= files.length) return;

        filePath = files[newIndex];
    }

    async function loadVolume() {
        try {
            const s = await adapter.getSettings();
            if (s.videoVolume !== undefined) volume = s.videoVolume;
            if (s.videoMuted !== undefined) muted = s.videoMuted;
        } catch (e) {
            console.error("Failed to load volume setting", e);
        }
    }

    function handleVolumeChange() {
        clearTimeout(saveTimeout);
        saveTimeout = setTimeout(async () => {
            try {
                const s = await adapter.getSettings();
                s.videoVolume = volume;
                s.videoMuted = muted;
                await adapter.saveSettings(s);
            } catch (e) {
                console.error("Failed to save volume setting", e);
            }
        }, 1000);
    }

    $effect(() => {
        if (open) {
            loadVolume();
        }
    });

    $effect(() => {
        if (open && filePath) {
            loadPreview(filePath);
        }
    });

    /**
     * Handle click on the dialog area - close if clicked outside the content.
     */
    function handleDialogClick(e: MouseEvent) {
        // Check if the click was on the backdrop (outside the content container)
        if (contentRef && !contentRef.contains(e.target as Node)) {
            onClose();
        }
    }

    /**
     * Handle the native dialog close event (fires on Escape key press).
     * This ensures our state stays in sync when the dialog is closed natively.
     */
    function handleDialogClose() {
        // The native dialog close event fires AFTER the dialog is closed
        // We need to call onClose to sync the parent state
        onClose();
    }

    /**
     * Prevent the default Escape behavior and handle it ourselves.
     * This ensures consistent behavior across all close methods.
     */
    function handleKeydown(e: KeyboardEvent) {
        if (e.key === "Escape") {
            e.preventDefault();
            onClose();
        }
    }

    function handleWindowKeydown(e: KeyboardEvent) {
        if (!open) return;
        const target = e.target as HTMLElement;
        // Don't navigate if the user is interacting with an input or if the event was already handled
        if (e.defaultPrevented || target?.matches?.("input, textarea, video")) {
            return;
        }

        if (e.key === "ArrowLeft") {
            navigate(-1);
        } else if (e.key === "ArrowRight") {
            navigate(1);
        }
    }

    /**
     * Open the file location in the system file explorer.
     */
    async function openFileLocation() {
        try {
            await invoke("show_in_folder", { path: filePath });
        } catch (err) {
            console.error("Failed to open file location", err);
        }
    }

    let copied = $state(false);

    /**
     * Copy the file path to clipboard.
     */
    async function handleCopyPath() {
        try {
            await invoke("copy_to_clipboard", { text: filePath });
            copied = true;
            setTimeout(() => {
                copied = false;
            }, 2000);
        } catch (err) {
            console.error("Failed to copy path", err);
        }
    }

    // Sync dialog open state with prop
    $effect(() => {
        if (open && dialogRef) {
            dialogRef.showModal();
        } else if (!open && dialogRef && dialogRef.open) {
            dialogRef.close();
        }
    });
</script>

<svelte:window onkeydown={handleWindowKeydown} />

{#if open}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <dialog
        bind:this={dialogRef}
        class="fixed inset-0 z-50 m-0 h-full w-full max-h-full max-w-full bg-transparent backdrop:bg-black/80"
        onclick={handleDialogClick}
        onclose={handleDialogClose}
        onkeydown={handleKeydown}
        aria-labelledby="preview-title"
        aria-modal="true"
    >
        <div
            class="relative flex h-full w-full items-center justify-center p-4 md:p-8"
            transition:fade={{ duration: 150 }}
        >
            <!-- Navigation Buttons -->
            {#if files.length > 1}
                {#if hasPrev}
                    <button
                        class="absolute left-2 top-1/2 -translate-y-1/2 p-2 text-white/50 hover:text-white transition-colors z-50 rounded-full hover:bg-white/10"
                        onclick={(e) => {
                            e.stopPropagation();
                            navigate(-1);
                        }}
                        aria-label={$_("common.previous")}
                    >
                        <ChevronLeft size={32} />
                    </button>
                {/if}
                {#if hasNext}
                    <button
                        class="absolute right-2 top-1/2 -translate-y-1/2 p-2 text-white/50 hover:text-white transition-colors z-50 rounded-full hover:bg-white/10"
                        onclick={(e) => {
                            e.stopPropagation();
                            navigate(1);
                        }}
                        aria-label={$_("common.next")}
                    >
                        <ChevronRight size={32} />
                    </button>
                {/if}
            {/if}

            <!-- Modal Content Container -->
            <div
                bind:this={contentRef}
                class="relative flex max-h-[90vh] max-w-[90vw] flex-col rounded-xl border border-border bg-card shadow-2xl overflow-hidden min-h-[300px] min-w-[300px]"
                transition:scale={{ duration: 150, start: 0.95 }}
                role="document"
            >
                {#if previewData}
                    <!-- Header -->
                    <header
                        class="flex items-center justify-between border-b border-border bg-muted/50 px-4 py-3"
                    >
                        <div class="flex items-center gap-3 min-w-0 flex-1">
                            {#if previewData.fileType === "text"}
                                {#if isMarkdown()}
                                    <FileText
                                        size={18}
                                        class="shrink-0 text-muted-foreground"
                                    />
                                {:else}
                                    <Code
                                        size={18}
                                        class="shrink-0 text-muted-foreground"
                                    />
                                {/if}
                            {/if}
                            <h2
                                id="preview-title"
                                class="text-sm font-medium text-foreground truncate"
                                title={previewData.fileName}
                                use:tooltip
                            >
                                {previewData.fileName}
                            </h2>
                            {#if previewData.fileType === "text" && !isMarkdown()}
                                <!-- Language Selector Dropdown -->
                                <div class="relative shrink-0">
                                    <select
                                        class="appearance-none bg-muted/50 border border-border rounded px-2 py-0.5 pr-6 text-xs text-muted-foreground hover:text-foreground hover:bg-muted cursor-pointer transition-colors focus:outline-none focus:ring-1 focus:ring-primary"
                                        bind:value={selectedLanguage}
                                        title={$_("filePreview.selectLanguage")}
                                    >
                                        <option value={null}>
                                            {currentLanguage() ?? "auto"}
                                        </option>
                                        {#each SUPPORTED_LANGUAGES as lang}
                                            <option value={lang}>{lang}</option>
                                        {/each}
                                    </select>
                                    <ChevronDown
                                        size={12}
                                        class="absolute right-1.5 top-1/2 -translate-y-1/2 pointer-events-none text-muted-foreground"
                                    />
                                </div>
                            {:else}
                                <span
                                    class="text-xs text-muted-foreground shrink-0"
                                >
                                    {previewData.mimeType} • {formatBytes(
                                        previewData.fileSize || 0,
                                        $locale || "en",
                                    )}
                                </span>
                            {/if}
                        </div>

                        <div class="flex items-center gap-2">
                            <button
                                class="p-2 rounded-md hover:bg-muted text-muted-foreground hover:text-foreground transition-colors"
                                onclick={onClose}
                                title={$_("common.close")}
                                use:tooltip
                            >
                                <X size={18} />
                            </button>
                        </div>
                    </header>

                    <!-- Preview Content -->
                    <div
                        class="flex-1 overflow-hidden p-4 flex items-center justify-center min-h-[200px] bg-background/50 relative"
                    >
                        {#if loading}
                            <div
                                class="absolute inset-0 flex items-center justify-center bg-background/50 z-10 backdrop-blur-[1px]"
                            >
                                <span
                                    class="animate-pulse text-muted-foreground"
                                    >{$_("common.loading")}</span
                                >
                            </div>
                        {/if}

                        {#if previewData.fileType === "image"}
                            <!-- Image Preview -->
                            <img
                                src={previewData.content}
                                alt={previewData.fileName}
                                class="max-w-full max-h-[calc(90vh-12rem)] object-contain rounded-lg shadow-lg"
                            />
                        {:else if previewData.fileType === "video"}
                            <!-- Video Preview -->
                            <!-- svelte-ignore a11y_media_has_caption -->
                            <video
                                src={getVideoSrc(previewData.content)}
                                controls
                                class="max-w-full max-h-[calc(90vh-12rem)] rounded-lg shadow-lg"
                                autoplay
                                bind:volume
                                bind:muted
                                onvolumechange={handleVolumeChange}
                            >
                                {$_("filePreview.videoNotSupported")}
                            </video>
                        {:else if previewData.fileType === "text"}
                            <!-- Text/Code Preview with Syntax Highlighting -->
                            {#if isMarkdown()}
                                <!-- Rendered Markdown -->
                                <div
                                    class="w-full h-full max-h-[calc(90vh-12rem)] overflow-auto bg-muted/50 rounded-lg p-6 prose prose-sm dark:prose-invert max-w-none border border-border"
                                >
                                    {@html highlightedContent}
                                </div>
                            {:else}
                                <!-- Syntax Highlighted Code -->
                                <div
                                    class="w-full h-full max-h-[calc(90vh-12rem)] overflow-auto bg-muted/50 rounded-lg border border-border relative"
                                >
                                    {#if highlightLoading}
                                        <div
                                            class="absolute inset-0 flex items-center justify-center bg-muted/30"
                                        >
                                            <span
                                                class="animate-pulse text-xs text-muted-foreground"
                                                >{$_("common.loading")}</span
                                            >
                                        </div>
                                    {/if}
                                    <pre
                                        class="p-4 text-xs font-mono text-foreground whitespace-pre min-w-max"><code
                                            class="hljs block"
                                            >{@html highlightedContent}</code
                                        ></pre>
                                </div>
                            {/if}
                        {:else}
                            <!-- Unsupported File Type -->
                            <div
                                class="flex flex-col items-center justify-center gap-4 text-muted-foreground"
                            >
                                <FileText size={48} />
                                <p class="text-sm">
                                    {$_("filePreview.unsupportedType")}
                                </p>
                                <p class="text-xs">{previewData.mimeType}</p>
                            </div>
                        {/if}
                    </div>

                    <!-- Footer -->
                    <footer
                        class="flex items-center justify-between border-t border-border bg-muted/30 px-4 py-2"
                    >
                        <button
                            class="text-xs text-muted-foreground truncate max-w-[60%] hover:text-foreground cursor-pointer text-left focus:outline-none transition-colors"
                            title={copied ? $_("common.copied") : filePath}
                            use:tooltip
                            onclick={handleCopyPath}
                        >
                            {copied ? $_("common.copied") : filePath}
                        </button>
                        <div class="flex items-center gap-2">
                            <button
                                class="flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs bg-muted hover:bg-muted/80 text-muted-foreground hover:text-foreground transition-colors"
                                onclick={openFileLocation}
                                title={$_("filePreview.openLocation")}
                                use:tooltip
                            >
                                <ExternalLink size={12} />
                                {$_("filePreview.openLocation")}
                            </button>
                        </div>
                    </footer>
                {:else}
                    <!-- Loading State (Initial) -->
                    <div class="flex-1 flex items-center justify-center p-8">
                        <span class="animate-pulse text-muted-foreground"
                            >{$_("common.loading")}</span
                        >
                    </div>
                {/if}
            </div>
        </div>
    </dialog>
{/if}

<style>
    dialog::backdrop {
        background: rgba(0, 0, 0, 0.8);
        backdrop-filter: blur(4px);
    }

    /* Language selector dropdown - ensure proper dark mode styling */
    select {
        color-scheme: dark;
    }

    select option {
        background-color: #27272a;
        color: #d8d8d9;
        padding: 0.5rem;
    }

    select option:hover,
    select option:focus,
    select option:checked {
        background-color: #3f3f46;
    }
</style>
