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
    import { fade } from "svelte/transition";
    import { DesktopStorageAdapter } from "$lib/services/desktop-adapter";
    import type { FilePreviewData } from "$lib/types";
    import { convertFileSrc } from "@tauri-apps/api/core";
    import { FileText, Image, Video, File } from "lucide-svelte";

    let { filePath, fileName, onclick } = $props<{
        filePath: string;
        fileName: string;
        onclick: () => void;
    }>();

    const adapter = new DesktopStorageAdapter();

    let isHovering = $state(false);
    let previewData = $state<FilePreviewData | null>(null);
    let isLoading = $state(false);
    let error = $state<string | null>(null);
    let hoverTimeout = $state<ReturnType<typeof setTimeout> | null>(null);

    /**
     * Determine the file type icon based on extension.
     */
    function getFileIcon(path: string) {
        const ext = path.split(".").pop()?.toLowerCase() || "";
        const imageExts = [
            "png",
            "jpg",
            "jpeg",
            "gif",
            "webp",
            "svg",
            "bmp",
            "ico",
        ];
        const videoExts = ["mp4", "webm", "ogg", "ogv", "mov", "avi", "mkv"];
        const textExts = [
            "txt",
            "md",
            "markdown",
            "json",
            "xml",
            "html",
            "htm",
            "css",
            "js",
            "mjs",
            "ts",
            "tsx",
            "jsx",
            "py",
            "rs",
            "go",
            "java",
            "c",
            "h",
            "cpp",
            "hpp",
            "cc",
            "cs",
            "rb",
            "php",
            "sh",
            "bash",
            "zsh",
            "ps1",
            "yaml",
            "yml",
            "toml",
            "ini",
            "cfg",
            "conf",
            "log",
            "sql",
            "svelte",
            "vue",
        ];

        if (imageExts.includes(ext)) return "image";
        if (videoExts.includes(ext)) return "video";
        if (textExts.includes(ext)) return "text";
        return "file";
    }

    /**
     * Load preview data when hovering starts.
     */
    async function loadPreview() {
        if (previewData || isLoading) return;

        isLoading = true;
        error = null;

        try {
            previewData = await adapter.readFileForPreview(filePath);
        } catch (err) {
            error =
                err instanceof Error ? err.message : "Failed to load preview";
            console.error("Failed to load file preview:", err);
        } finally {
            isLoading = false;
        }
    }

    /**
     * Handle mouse enter - start delayed preview load.
     */
    let showBelow = $state(false);
    let tooltipX = $state(0);
    let tooltipY = $state(0);
    let xOffset = $state(0);

    /**
     * Handle mouse enter - start delayed preview load.
     */
    function handleMouseEnter(e: MouseEvent) {
        // Check distance to top of viewport to decide position
        const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
        showBelow = rect.top < 250;

        // Calculate X center
        tooltipX = rect.left + rect.width / 2;

        // Calculate Y reference
        if (showBelow) {
            tooltipY = rect.bottom + 8;
        } else {
            tooltipY = rect.top - 8;
        }

        // Edge detection assuming max width ~320px
        const estimatedWidth = 320;
        const windowWidth = window.innerWidth;

        const idealLeft = tooltipX - estimatedWidth / 2;
        const diffL = 10 - idealLeft;

        const idealRight = tooltipX + estimatedWidth / 2;
        const diffR = idealRight - (windowWidth - 10);

        xOffset = Math.max(0, diffL) - Math.max(0, diffR);

        hoverTimeout = setTimeout(() => {
            isHovering = true;
            loadPreview();
        }, 300);
    }

    /**
     * Handle mouse leave - cancel pending preview and hide.
     */
    function handleMouseLeave() {
        if (hoverTimeout) {
            clearTimeout(hoverTimeout);
            hoverTimeout = null;
        }
        isHovering = false;
    }

    /**
     * Get video source URL for preview thumbnail.
     */
    function getVideoSrc(path: string): string {
        return convertFileSrc(path);
    }

    const fileType = $derived(getFileIcon(filePath));
</script>

<div class="relative inline-block">
    <!-- File Badge with Hover Trigger -->
    <button
        class="group/file inline-flex items-center gap-1 rounded-full border border-border bg-secondary/50 px-2 py-0.5 text-[10px] text-muted-foreground hover:bg-secondary hover:text-foreground hover:border-primary/50 transition-all cursor-pointer max-w-[150px]"
        onmouseenter={handleMouseEnter}
        onmouseleave={handleMouseLeave}
        {onclick}
        title={$_("filePreview.clickToPreview")}
    >
        <!-- File Type Icon -->
        {#if fileType === "image"}
            <Image size={10} class="shrink-0" />
        {:else if fileType === "video"}
            <Video size={10} class="shrink-0" />
        {:else if fileType === "text"}
            <FileText size={10} class="shrink-0" />
        {:else}
            <File size={10} class="shrink-0" />
        {/if}
        <span class="truncate">{fileName}</span>
    </button>

    <!-- Hover Preview Tooltip -->
    {#if isHovering}
        <div
            class="fixed z-[9999] pointer-events-none left-0 top-0"
            style="
                left: {tooltipX}px;
                top: {tooltipY}px;
                transform: translate(calc(-50% + {xOffset}px), {showBelow
                ? '0'
                : '-100%'});
            "
            transition:fade={{ duration: 100 }}
        >
            <div
                class="relative bg-popover border border-border rounded-lg shadow-xl"
            >
                {#if isLoading}
                    <!-- Loading State -->
                    <div
                        class="flex items-center justify-center w-48 h-32 bg-muted/50 p-2"
                    >
                        <div
                            class="animate-pulse text-xs text-muted-foreground"
                        >
                            {$_("common.loading")}
                        </div>
                    </div>
                {:else if error}
                    <!-- Error State -->
                    <div
                        class="flex items-center justify-center w-48 h-32 bg-muted/50 p-2"
                    >
                        <div class="text-xs text-destructive p-2 text-center">
                            {error}
                        </div>
                    </div>
                {:else if previewData}
                    <!-- Preview Content -->
                    {#if previewData.fileType === "image" || previewData.fileType === "video" || previewData.fileType === "text"}
                        <div class="p-2 pb-0 flex justify-center">
                            {#if previewData.fileType === "image"}
                                <img
                                    src={previewData.content}
                                    alt={previewData.fileName}
                                    class="block max-w-[280px] max-h-[200px] object-contain rounded"
                                />
                            {:else if previewData.fileType === "video"}
                                <!-- svelte-ignore a11y_media_has_caption -->
                                <video
                                    src={getVideoSrc(previewData.content)}
                                    class="block max-w-[280px] max-h-[200px] object-contain rounded"
                                    muted
                                    autoplay
                                    loop
                                    playsinline
                                >
                                </video>
                            {:else if previewData.fileType === "text"}
                                <pre
                                    class="w-44 h-28 overflow-hidden bg-muted/50 p-2 text-[9px] font-mono text-foreground whitespace-pre-wrap break-words rounded">{previewData.content.slice(
                                        0,
                                        500,
                                    )}</pre>
                            {/if}
                        </div>
                    {/if}
                    <!-- File Name Footer -->
                    <div class="p-2">
                        <div
                            class="text-[10px] text-muted-foreground truncate text-center"
                            title={previewData.fileName}
                        >
                            {previewData.fileName}
                        </div>
                    </div>
                {/if}

                <!-- Arrow Pointer -->
                <div
                    class="absolute left-1/2 -translate-x-1/2 w-2 h-2 rotate-45 bg-popover border-border {showBelow
                        ? 'top-0 -translate-y-1/2 border-l border-t'
                        : 'bottom-0 translate-y-1/2 border-r border-b'}"
                    style="margin-left: {-xOffset}px;"
                ></div>
            </div>
        </div>
    {/if}
</div>
