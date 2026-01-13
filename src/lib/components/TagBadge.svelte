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
    import { Hash } from "lucide-svelte";
    import { getTagHue } from "$lib/utils/markdown";

    /**
     * Reusable tag badge component.
     * Displays a hashtag with consistent styling and color based on tag name.
     * Supports optional click handler, selected state, and count for filter functionality.
     */
    let {
        tag,
        size = "sm",
        selected = false,
        count,
        onclick,
    } = $props<{
        /** The tag string (with or without the # prefix) */
        tag: string;
        /** Size variant: 'xs' for compact, 'sm' for default */
        size?: "xs" | "sm";
        /** Whether the tag is selected (for filter buttons) */
        selected?: boolean;
        /** Optional count to display (e.g., number of items with this tag) */
        count?: number;
        /** Optional click handler (makes the tag interactive) */
        onclick?: (e: MouseEvent) => void;
    }>();

    // Normalize tag (ensure it has # prefix for hue calculation)
    let normalizedTag = $derived(tag.startsWith("#") ? tag : `#${tag}`);
    let hue = $derived(getTagHue(normalizedTag));
    let label = $derived(normalizedTag.substring(1));

    // Size variants
    let sizeClasses = $derived(
        size === "xs"
            ? "text-[9px] px-1 py-0.5 gap-0.5"
            : "text-[10px] px-1.5 py-0.5 gap-0.5",
    );
    let iconSize = $derived(size === "xs" ? 8 : 10);

    // Interactive styling
    let isInteractive = $derived(!!onclick);

    // Compute styles based on selected state
    let computedStyle = $derived(
        selected
            ? `background-color: hsla(${hue}, 100%, 75%, 0.25); border-color: hsla(${hue}, 80%, 40%, 0.4); color: hsla(${hue}, 90%, 25%, 1);`
            : isInteractive
              ? `background-color: transparent; border-color: var(--border); color: var(--muted-foreground);`
              : `background-color: hsla(${hue}, 100%, 75%, 0.15); color: hsla(${hue}, 80%, 35%, 1);`,
    );
</script>

{#if isInteractive}
    <button
        class="inline-flex items-center rounded-full border transition-all hover:opacity-80 cursor-pointer {sizeClasses}"
        style={computedStyle}
        {onclick}
        type="button"
    >
        <Hash size={iconSize} />
        <span class="font-medium">{label}</span>
        {#if count !== undefined && count > 0}
            <span class="text-[9px] opacity-60">{count}</span>
        {/if}
    </button>
{:else}
    <span
        class="inline-flex items-center rounded-full {sizeClasses}"
        style={computedStyle}
    >
        <Hash size={iconSize} />
        {label}
    </span>
{/if}
