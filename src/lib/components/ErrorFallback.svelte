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

<!--
  Shown by the `<svelte:boundary>` in App.svelte when a render or effect throws.

  Without a boundary, an error escaping Svelte's flush leaves the batch uncommitted:
  handlers still run and state still mutates, but the DOM stops updating. The app
  looks frozen and only a webview reload recovers it - which is exactly how the
  unquoted class ternary in the cloud-usage bar presented for two releases. This
  turns that failure mode into something the user can see and act on.
-->

<script lang="ts">
  import { AlertTriangle, RefreshCw, RotateCcw } from "lucide-svelte";
  import { _ } from "$lib/i18n";

  let { error, reset }: { error: unknown; reset: () => void } = $props();

  /** The message alone; the stack is for the log, not the panel. */
  let message = $derived(
    error instanceof Error ? error.message : String(error ?? "unknown"),
  );
</script>

<div
  class="flex flex-1 flex-col items-center justify-center gap-4 p-8 text-center select-text"
  role="alert"
>
  <AlertTriangle size={32} class="text-destructive" />

  <div class="space-y-1">
    <h2 class="text-sm font-medium">{$_("errorBoundary.title")}</h2>
    <p class="max-w-sm text-xs text-muted-foreground">
      {$_("errorBoundary.description")}
    </p>
  </div>

  <code
    class="max-w-sm overflow-x-auto rounded-md border border-border bg-muted/40 px-3 py-2 font-mono text-[10px] text-muted-foreground"
  >
    {message}
  </code>

  <div class="flex items-center gap-2">
    <button
      type="button"
      class="flex items-center gap-1.5 rounded-md border border-border px-3 py-1.5 text-xs transition-colors hover:bg-muted"
      onclick={reset}
    >
      <RotateCcw size={13} />
      {$_("errorBoundary.tryAgain")}
    </button>
    <button
      type="button"
      class="flex items-center gap-1.5 rounded-md bg-primary px-3 py-1.5 text-xs text-primary-foreground transition-colors hover:bg-primary/90"
      onclick={() => window.location.reload()}
    >
      <RefreshCw size={13} />
      {$_("errorBoundary.reload")}
    </button>
  </div>
</div>
