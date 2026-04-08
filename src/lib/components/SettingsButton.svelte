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
  import type { Snippet } from "svelte";
  import type { HTMLButtonAttributes } from "svelte/elements";

  let {
    variant = "outside",
    icon,
    iconOnly = false,
    children,
    class: className = "",
    ...restProps
  }: {
    variant?: "inside" | "outside" | "destructive" | "ghost" | "outline";
    icon?: Snippet;
    iconOnly?: boolean;
    children?: Snippet;
  } & HTMLButtonAttributes = $props();

  let sizeClasses = $derived(
    iconOnly
      ? "p-2 text-sm"
      : variant === "inside"
        ? "px-3 py-1.5 text-sm"
        : "px-4 py-2 text-sm"
  );

  let colorClasses = $derived.by(() => {
    switch (variant) {
      case "inside":
        return "border border-border bg-muted hover:bg-accent hover:text-accent-foreground";
      case "outside":
        return "bg-primary text-primary-foreground hover:bg-primary/90";
      case "destructive":
        return "border border-red-500/20 text-red-500 hover:bg-red-500/10";
      case "ghost":
        return "border border-transparent hover:bg-muted text-muted-foreground hover:text-foreground";
      case "outline":
        return "border border-border hover:bg-muted text-foreground";
      default:
        return "bg-primary text-primary-foreground hover:bg-primary/90";
    }
  });

  let baseClasses =
    "inline-flex items-center justify-center gap-2 rounded-md font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed";
</script>

<button
  class="{baseClasses} {sizeClasses} {colorClasses} {className}"
  {...restProps}
>
  {#if icon}
    {@render icon()}
  {/if}
  {#if children && !iconOnly}
    {@render children()}
  {/if}
</button>
