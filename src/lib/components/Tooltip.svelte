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
    import { fade } from "svelte/transition";
    import { portal } from "$lib/actions/portal";
    import type { Snippet } from "svelte";

    /**
     * Reusable Tooltip component
     * Renders to document.body to avoid overflow clipping
     * Handles positioning, arrow, and styling
     * Content is provided via slot
     *
     * `x` is where the arrow points. The card is centred on it, and shifted sideways by
     * however far it would otherwise leave the window.
     */

    let {
        visible = false,
        x = 0,
        y = 0,
        position = "top",
        children,
    } = $props<{
        visible: boolean;
        x: number;
        y: number;
        position?: "top" | "bottom";
        children: Snippet;
    }>();

    const showBelow = $derived(position === "bottom");

    /** Gap kept between the card and either edge of the window. */
    const VIEWPORT_PADDING = 8;

    let innerWidth = $state(0);
    let cardWidth = $state(0);

    // Measured, not assumed. Each caller used to guess the card was 280px wide and work
    // out the shift itself - with the sign reversed, so a card near an edge was pushed
    // further out of the window instead of back in. An image preview is wider than 280px
    // anyway, so even the right sign left it hanging over the edge.
    const xOffset = $derived.by(() => {
        if (!cardWidth || !innerWidth) return 0;
        const left = x - cardWidth / 2;
        const right = x + cardWidth / 2;
        if (left < VIEWPORT_PADDING) return VIEWPORT_PADDING - left;
        if (right > innerWidth - VIEWPORT_PADDING) {
            return innerWidth - VIEWPORT_PADDING - right;
        }
        return 0;
    });
</script>

<svelte:window bind:innerWidth />

{#if visible}
    <div
        use:portal={"body"}
        class="fixed pointer-events-none"
        style="
            z-index: 999999 !important;
            left: {x}px;
            top: {y}px;
            transform: translate(calc(-50% + {xOffset}px), {showBelow
            ? '0'
            : '-100%'});
        "
        transition:fade={{ duration: 100 }}
    >
        <div
            class="relative bg-foreground border border-border rounded-lg shadow-xl"
            bind:offsetWidth={cardWidth}
        >
            <!-- Custom content via slot -->
            {@render children()}

            <!-- Arrow Pointer -->
            <div
                class="absolute left-1/2 -translate-x-1/2 w-2 h-2 rotate-45 bg-foreground border-border {showBelow
                    ? 'top-0 -translate-y-1/2 border-l border-t'
                    : 'bottom-0 translate-y-1/2 border-r border-b'}"
                style="margin-left: {-xOffset}px;"
            ></div>
        </div>
    </div>
{/if}
