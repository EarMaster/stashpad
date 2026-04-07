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
    /**
     * Color badge component for hex colors.
     * Displays a small color swatch and the hex code.
     */
    let {
        color,
        size = "sm"
    } = $props<{
        /** The hex color string (e.g. #302919) */
        color: string;
        /** Size variant: 'xs' for compact, 'sm' for default */
        size?: "xs" | "sm";
    }>();

    let sizeClasses = $derived(
        size === "xs"
            ? "text-[9px] px-1 py-0.5 gap-1"
            : "text-[10px] px-1.5 py-0.5 gap-1.5",
    );
    let swatchSize = $derived(size === "xs" ? "w-2 h-2" : "w-2.5 h-2.5");
</script>

<span
    class="color-badge inline-flex items-center rounded-md border {sizeClasses} bg-muted/20"
>
    <span
        class="{swatchSize} rounded-[2px] border border-black/10 dark:border-white/20 shrink-0"
        style="background-color: {color};"
    ></span>
    <span class="font-mono lowercase opacity-90">{color}</span>
</span>

<style>
    .color-badge {
        border-color: var(--border);
        color: var(--muted-foreground);
    }
</style>
