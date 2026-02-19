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
    // Dark mode uses higher lightness for better readability
    let computedStyle = $derived(
        selected
            ? `--tag-bg: hsla(${hue}, 100%, 75%, 0.25); --tag-bg-dark: hsla(${hue}, 70%, 30%, 0.35); --tag-border: hsla(${hue}, 80%, 40%, 0.4); --tag-border-dark: hsla(${hue}, 60%, 50%, 0.5); --tag-text: hsla(${hue}, 90%, 25%, 1); --tag-text-dark: hsla(${hue}, 85%, 75%, 1);`
            : isInteractive
              ? `--tag-bg: transparent; --tag-bg-dark: transparent; --tag-border: var(--border); --tag-border-dark: var(--border); --tag-text: var(--muted-foreground); --tag-text-dark: var(--muted-foreground);`
              : `--tag-bg: hsla(${hue}, 75%, 45%, 0.1); --tag-bg-dark: hsla(${hue}, 80%, 70%, 0.1); --tag-border: hsla(${hue}, 75%, 45%, 0.2); --tag-border-dark: hsla(${hue}, 80%, 70%, 0.2); --tag-text: hsl(${hue}, 75%, 45%); --tag-text-dark: hsl(${hue}, 80%, 70%);`,
    );
</script>

{#if isInteractive}
    <button
        class="tag-badge inline-flex items-center rounded-full border transition-all hover:opacity-80 cursor-pointer {sizeClasses}"
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
        class="tag-badge inline-flex items-center rounded-full border {sizeClasses}"
        style={computedStyle}
    >
        <Hash size={iconSize} />
        {label}
    </span>
{/if}

<style>
    /* Light mode (default) */
    .tag-badge {
        background-color: var(--tag-bg);
        border-color: var(--tag-border, transparent);
        color: var(--tag-text);
    }

    /* Dark mode */
    :global(.dark) .tag-badge {
        background-color: var(--tag-bg-dark, var(--tag-bg));
        border-color: var(--tag-border-dark, var(--tag-border, transparent));
        color: var(--tag-text-dark, var(--tag-text));
    }

    @media (prefers-color-scheme: dark) {
        :global(:not(.light)) .tag-badge {
            background-color: var(--tag-bg-dark, var(--tag-bg));
            border-color: var(
                --tag-border-dark,
                var(--tag-border, transparent)
            );
            color: var(--tag-text-dark, var(--tag-text));
        }
    }
</style>
