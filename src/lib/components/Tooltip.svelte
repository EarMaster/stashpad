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
    import { fade } from "svelte/transition";
    import { portal } from "$lib/actions/portal";
    import type { Snippet } from "svelte";

    /**
     * Reusable Tooltip component
     * Renders to document.body to avoid overflow clipping
     * Handles positioning, arrow, and styling
     * Content is provided via slot
     */

    let {
        visible = false,
        x = 0,
        y = 0,
        position = "top",
        xOffset = 0,
        children,
    } = $props<{
        visible: boolean;
        x: number;
        y: number;
        position?: "top" | "bottom";
        xOffset?: number;
        children: Snippet;
    }>();

    const showBelow = $derived(position === "bottom");
</script>

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
