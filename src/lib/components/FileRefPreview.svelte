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
    import Tooltip from "./Tooltip.svelte";
    import FilePreviewContent from "./FilePreviewContent.svelte";
    import type { FileRefResolver } from "$lib/utils/file-refs";
    import type { FilePreviewData } from "$lib/types";
    import { peekContextFile } from "$lib/stores/context-folders.svelte";
    import { errorText } from "$lib/errors";
    import { log } from "$lib/utils/log";

    /**
     * Hover card for an `@path` file chip. A file the folder holds is shown exactly as an
     * attachment is; one it does not hold says so, with a same-named file as the guess.
     * One instance serves every chip of a card; the card positions it.
     */
    let {
        contextId,
        path,
        resolve,
        visible,
        x,
        y,
        position = "top",
    } = $props<{
        contextId: string;
        path: string | null;
        resolve?: FileRefResolver;
        visible: boolean;
        x: number;
        y: number;
        position?: "top" | "bottom";
    }>();

    const display = $derived(path && resolve ? resolve(path) : undefined);
    const fileName = $derived(path ? path.slice(path.lastIndexOf("/") + 1) : "");

    let previewData = $state<FilePreviewData | null>(null);
    let isLoading = $state(false);
    let error = $state("");

    // Fetch when a resolved path is hovered. A late answer for a chip no longer hovered is
    // dropped, so moving across chips never shows the wrong file.
    $effect(() => {
        const wanted = path;
        previewData = null;
        error = "";
        isLoading = false;
        if (!wanted || display?.state !== "resolved") return;
        isLoading = true;
        peekContextFile(contextId, wanted).then(
            (result) => {
                if (path !== wanted) return;
                previewData = result;
                isLoading = false;
            },
            (e) => {
                if (path !== wanted) return;
                log.error("Failed to load file preview:", e);
                error = $_(errorText(e, "common.unknownError"));
                isLoading = false;
            },
        );
    });
</script>

<Tooltip visible={visible && !!path} {x} {y} {position}>
    {#snippet children()}
        {#if display?.state === "unresolved"}
            <div class="w-56 p-2.5 flex flex-col gap-1 text-[11px] leading-snug text-background">
                <span class="font-mono text-[10px] text-background/60 truncate">{path}</span>
                <p>{$_("fileRef.missing")}</p>
                {#if display.suggestion}
                    <p class="text-background/70">
                        {$_("fileRef.didYouMean", {
                            values: { path: display.suggestion },
                        })}
                    </p>
                {/if}
            </div>
        {:else}
            <FilePreviewContent {isLoading} {error} {previewData} {fileName} />
        {/if}
    {/snippet}
</Tooltip>
