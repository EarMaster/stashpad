<!--
// SPDX-License-Identifier: AGPL-3.0-only

// Copyright (C) 2026 Nico Wiedemann
//
// This file is part of Stashpad.
// Stashpad is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License, version 3,
// as published by the Free Software Foundation.
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Affero General Public License for more details.
-->

<script lang="ts">
    import { _ } from "$lib/i18n";
    import { DesktopStorageAdapter } from "$lib/services/desktop-adapter";
    import type { FilePreviewData } from "$lib/types";
    import { convertFileSrc } from "@tauri-apps/api/core";
    import {
        FileText,
        Image,
        Video,
        File,
        CloudDownload,
        CloudOff,
        Loader2,
    } from "lucide-svelte";
    import Tooltip from "./Tooltip.svelte";
    import { attachmentSync } from "$lib/stores/attachment-sync.svelte";
    import { formatBytes } from "$lib/utils/format";
    import { getAttachmentKind } from "$lib/utils/files";

    let {
        filePath,
        fileName,
        attachmentId = "",
        fileSize = 0,
        onclick,
    } = $props<{
        filePath: string;
        fileName: string;
        /** Attachment id, used to track cloud download state. */
        attachmentId?: string;
        /** Size in bytes, shown while the file is still downloading. */
        fileSize?: number;
        onclick?: (resolvedPath: string) => void;
    }>();

    const adapter = new DesktopStorageAdapter();

    /**
     * An attachment synced from another device exists as metadata before its bytes do.
     * `filePath` is empty until the download completes, so the file is real but not yet
     * openable - show it as pending rather than letting the preview fail.
     */
    const resolvedPath = $derived(
        attachmentId ? attachmentSync.pathFor(attachmentId, filePath) : filePath,
    );
    const isPending = $derived(resolvedPath.trim() === "");
    const syncStatus = $derived(
        attachmentId ? attachmentSync.status(attachmentId) : undefined,
    );
    const hasFailed = $derived(isPending && syncStatus === "error");

    /** True while the user is waiting on a download they explicitly asked for. */
    let isAwaitingOpen = $state(false);

    let isHovering = $state(false);
    let isLoading = $state(false);
    let previewData = $state<FilePreviewData | null>(null);
    let error = $state("");
    let tooltipX = $state(0);
    let tooltipY = $state(0);
    let xOffset = $state(0);
    let showBelow = $state(false);

    let hoverTimeout: ReturnType<typeof setTimeout> | null = null;

    /**
     * Open the attachment, fetching it first if this device does not have it yet.
     *
     * Clicking is treated as an explicit request, so the file jumps the download queue
     * ahead of any backlog the user has not asked for.
     */
    async function handleClick() {
        if (!isPending) {
            onclick?.(resolvedPath);
            return;
        }

        if (!attachmentId || isAwaitingOpen) return;

        isAwaitingOpen = true;
        try {
            const path = await attachmentSync.request(attachmentId, filePath);
            onclick?.(path);
        } catch (e) {
            console.error("Failed to download attachment:", e);
        } finally {
            isAwaitingOpen = false;
        }
    }

    async function handleMouseEnter(event: MouseEvent) {
        // No local file to preview yet.
        if (isPending) return;

        // Clear any existing timeout
        if (hoverTimeout) {
            clearTimeout(hoverTimeout);
        }

        // Store reference to the element before the timeout
        const targetElement = event.currentTarget as HTMLElement | null;
        if (!targetElement) return;

        // Wait 300ms before showing tooltip
        hoverTimeout = setTimeout(async () => {
            // Set loading state first, then show
            isLoading = true;
            isHovering = true;
            error = "";
            updateTooltipPosition(targetElement);

            // Load preview data
            try {
                previewData = await adapter.readFileForPreview(resolvedPath);
            } catch (e) {
                console.error("Failed to load preview:", e);
                error = $_(
                    e instanceof Error ? e.message : "common.unknownError",
                );
            } finally {
                isLoading = false;
            }
        }, 300);
    }

    function handleMouseLeave() {
        // Clear timeout if mouse leaves before delay
        if (hoverTimeout) {
            clearTimeout(hoverTimeout);
            hoverTimeout = null;
        }
        isHovering = false;
        previewData = null;
        error = "";
    }

    function updateTooltipPosition(target: HTMLElement) {
        const rect = target.getBoundingClientRect();

        const TOOLTIP_OFFSET = 8;
        const centerX = rect.left + rect.width / 2;
        const centerY = rect.top;

        // Calculate tooltip position
        tooltipX = centerX;

        // Determine if tooltip should show below or above
        if (centerY < window.innerHeight / 2) {
            // Show below if in top half of screen
            showBelow = true;
            tooltipY = rect.bottom + TOOLTIP_OFFSET;
        } else {
            // Show above if in bottom half of screen
            showBelow = false;
            tooltipY = rect.top - TOOLTIP_OFFSET;
        }

        // Calculate horizontal offset to keep tooltip centered on element
        const TOOLTIP_MAX_WIDTH = 280;
        const viewportPadding = 8;

        // Calculate if tooltip would overflow
        const tooltipLeft = centerX - TOOLTIP_MAX_WIDTH / 2;
        const tooltipRight = centerX + TOOLTIP_MAX_WIDTH / 2;

        if (tooltipLeft < viewportPadding) {
            // Would overflow left
            xOffset = tooltipLeft - viewportPadding;
        } else if (tooltipRight > window.innerWidth - viewportPadding) {
            // Would overflow right
            xOffset = tooltipRight - (window.innerWidth - viewportPadding);
        } else {
            xOffset = 0;
        }
    }

    function handleDragStart(event: DragEvent) {
        event.preventDefault();
        // Nothing on disk to hand to the OS yet.
        if (isPending) return;
        adapter.startDrag("", [resolvedPath]);
    }

    function getVideoSrc(content: string): string {
        // Convert file path to Tauri asset URL for local files
        return convertFileSrc(content);
    }

    // Derive from the name, not the path: a pending attachment has no path yet, but its
    // filename is always known.
    const fileType = $derived(getAttachmentKind(fileName || filePath));
</script>

<!-- File Badge with Hover Trigger (Draggable) -->
<button
    class="group/file inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[10px] transition-all max-w-[180px] {isPending
        ? 'border-dashed border-border/70 bg-secondary/20 text-muted-foreground/70 cursor-progress'
        : 'border-border bg-secondary/50 text-muted-foreground hover:bg-secondary hover:text-foreground hover:border-primary/50 cursor-pointer'}"
    onmouseenter={handleMouseEnter}
    onmouseleave={handleMouseLeave}
    onclick={handleClick}
    draggable={!isPending}
    ondragstart={handleDragStart}
    title={isPending
        ? hasFailed
            ? $_("attachments.downloadFailed", { values: { name: fileName } })
            : $_("attachments.syncing", { values: { name: fileName } })
        : fileName}
>
    <!-- File Type Icon, or sync state when the bytes are not here yet -->
    {#if isPending}
        {#if hasFailed}
            <CloudOff size={10} class="shrink-0 text-destructive/70" />
        {:else if syncStatus === "downloading" || isAwaitingOpen}
            <Loader2 size={10} class="shrink-0 animate-spin" />
        {:else}
            <CloudDownload size={10} class="shrink-0" />
        {/if}
    {:else if fileType === "image"}
        <Image size={10} class="shrink-0" />
    {:else if fileType === "video"}
        <Video size={10} class="shrink-0" />
    {:else if fileType === "text"}
        <FileText size={10} class="shrink-0" />
    {:else}
        <File size={10} class="shrink-0" />
    {/if}

    <span class="truncate">{fileName}</span>

    <!-- Size is the only metadata we have before the file arrives, so surface it -->
    {#if isPending && fileSize > 0}
        <span class="shrink-0 tabular-nums opacity-60">{formatBytes(fileSize)}</span>
    {/if}
</button>

<!-- Use reusable Tooltip component -->
<Tooltip
    visible={isHovering}
    x={tooltipX}
    y={tooltipY}
    position={showBelow ? "bottom" : "top"}
    {xOffset}
>
    {#snippet children()}
        {#if isLoading}
            <!-- Loading State -->
            <div
                class="flex items-center justify-center w-48 h-32 bg-muted/50 p-2"
            >
                <div class="animate-pulse text-xs text-background/70">
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
                            class="w-44 h-28 overflow-hidden bg-background/20 p-2 text-[9px] font-mono text-background whitespace-pre-wrap break-words rounded">{previewData.content.slice(
                                0,
                                500,
                            )}</pre>
                    {/if}
                </div>
            {/if}
            <!-- File Name Footer -->
            <div class="p-2">
                <div
                    class="text-[10px] text-background/70 truncate text-center"
                >
                    {previewData.fileName}
                </div>
            </div>
        {:else}
            <!-- Fallback: Show filename if no preview data  -->
            <div class="flex items-center justify-center w-48 h-16 p-2">
                <div class="text-xs text-background/70 text-center">
                    {fileName}
                </div>
            </div>
        {/if}
    {/snippet}
</Tooltip>
