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
    import { Paperclip } from "lucide-svelte";
    import Tooltip from "./Tooltip.svelte";
    import { safeParse } from "$lib/utils/markdown";
    import { STASH_REF_PATTERN, type ResolvedRef } from "$lib/utils/stash-refs";
    import { getRelativeTime } from "$lib/utils/date";

    /**
     * Hover card for a reference to another stash, modelled on the attachment
     * preview. One instance serves every chip of a card; the card positions it.
     */
    let {
        ref,
        visible,
        x,
        y,
        position = "top",
        contextName,
    } = $props<{
        ref: ResolvedRef | null;
        visible: boolean;
        x: number;
        y: number;
        position?: "top" | "bottom";
        contextName?: (id: string) => string;
    }>();

    const PREVIEW_CHARS = 300;

    // Links inside the preview are reduced to their label, so hovering a preview
    // never opens another one.
    const previewHtml = $derived.by(() => {
        const content = ref?.stash?.content;
        if (!content) return "";
        const plain = content.replace(STASH_REF_PATTERN, "$1");
        return safeParse(
            plain.length > PREVIEW_CHARS
                ? plain.slice(0, PREVIEW_CHARS).trimEnd() + "…"
                : plain,
        );
    });

    const status = $derived.by(() => {
        if (!ref) return "";
        switch (ref.state) {
            case "open":
                return $_("stashRef.stateOpen");
            case "completed":
                return ref.stash?.completedAt
                    ? $_("stashRef.stateCompletedAt", {
                          values: {
                              time: getRelativeTime(ref.stash.completedAt, $_),
                          },
                      })
                    : $_("stashRef.stateCompleted");
            case "other_context":
                return $_("stashRef.inContext", {
                    values: {
                        name:
                            contextName?.(ref.stash?.contextId ?? "") ??
                            ref.stash?.contextId,
                    },
                });
            default:
                return $_("stashRef.gone");
        }
    });

    const attachmentCount = $derived(ref?.stash?.attachments.length ?? 0);
</script>

<Tooltip visible={visible && !!ref} {x} {y} {position}>
    {#snippet children()}
        <div class="w-64 p-2.5 flex flex-col gap-1.5 text-background">
            <div
                class="flex items-center gap-1.5 text-[10px] font-semibold uppercase tracking-wide text-background/60"
            >
                <span>{status}</span>
                {#if attachmentCount > 0}
                    <span class="flex-1"></span>
                    <span class="inline-flex items-center gap-0.5 normal-case">
                        <Paperclip size={10} />
                        {attachmentCount}
                    </span>
                {/if}
            </div>
            {#if previewHtml && ref?.state !== "gone"}
                <div
                    class="stash-ref-preview text-[11px] leading-snug break-words max-h-40 overflow-hidden {ref?.state ===
                    'completed'
                        ? 'line-through opacity-80'
                        : ''}"
                >
                    {@html previewHtml}
                </div>
            {/if}
        </div>
    {/snippet}
</Tooltip>

<style>
    .stash-ref-preview :global(p),
    .stash-ref-preview :global(ul),
    .stash-ref-preview :global(ol),
    .stash-ref-preview :global(pre) {
        margin: 0 0 0.25rem;
    }
    .stash-ref-preview :global(h1),
    .stash-ref-preview :global(h2),
    .stash-ref-preview :global(h3) {
        font-weight: 600;
        margin: 0 0 0.25rem;
    }
    .stash-ref-preview :global(pre) {
        white-space: pre-wrap;
        font-size: 10px;
    }
</style>
