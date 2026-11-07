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
    import { convertFileSrc } from "@tauri-apps/api/core";
    import type { FilePreviewData } from "$lib/types";

    /**
     * The body of a file's hover card: an image, a video, the opening of a text file, or
     * just the name. Shared by attachment badges and `@path` file chips, so a file looks
     * the same wherever it is hovered. The caller owns the card and the loading.
     */
    let {
        isLoading,
        error,
        previewData,
        fileName,
    } = $props<{
        isLoading: boolean;
        error: string;
        previewData: FilePreviewData | null;
        /** Shown when there is no preview to show it with. */
        fileName: string;
    }>();

    function getVideoSrc(content: string): string {
        // Convert file path to Tauri asset URL for local files
        return convertFileSrc(content);
    }
</script>

{#if isLoading}
    <!-- Loading State -->
    <div class="flex items-center justify-center w-48 h-32 bg-muted/50 p-2">
        <div class="animate-pulse text-xs text-background/70">
            {$_("common.loading")}
        </div>
    </div>
{:else if error}
    <!-- Error State -->
    <div class="flex items-center justify-center w-48 h-32 bg-muted/50 p-2">
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
    {:else if previewData.fileType === "withheld"}
        <!-- Named like a file that holds secrets: its contents were never read -->
        <div class="p-2 pb-0 w-48 text-[11px] text-background/70 text-center">
            {$_("fileRef.withheld")}
        </div>
    {/if}
    <!-- File Name Footer -->
    <div class="p-2">
        <div class="text-[10px] text-background/70 truncate text-center">
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
