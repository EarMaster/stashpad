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
  What an available update looks like, wherever it is shown.

  Two places need it and they cannot share a container: the header popover (main view)
  and the Settings > Updates section, which is where the user ends up when the header is
  off screen. Keeping the body here means the release notes, the who-installs-it branch
  and the two sit-out actions have exactly one implementation.
-->
<script lang="ts">
  import { _ } from "$lib/i18n";
  import { Loader2 } from "lucide-svelte";
  import type { UpdateInfo } from "$lib/stores/updater.svelte";
  import { updateHintFor, storeNameFor } from "$lib/utils/installation";
  import { safeParse } from "$lib/utils/markdown";
  import { externalLinks } from "$lib/actions/externalLinks";

  let {
    update,
    busy = false,
    onInstall,
    onSkip,
    onRemindLater,
  } = $props<{
    update: UpdateInfo;
    busy?: boolean;
    onInstall: () => void;
    onSkip: () => void;
    onRemindLater: () => void;
  }>();

  let hint = $derived(updateHintFor(update.source));
  let storeName = $derived(storeNameFor(update.source));

  /**
   * Release notes are markdown and are rendered as such.
   *
   * Not truncated by character count: the notes are a whole changelog, and slicing them
   * lands mid-syntax and leaves stray `**` and `###` on screen. The container scrolls
   * instead, so the surrounding panel keeps its size however long the notes are.
   */
  let notesHtml = $derived(update.notes.trim() ? safeParse(update.notes) : "");
</script>

{#if notesHtml}
  <div class="px-3 pb-2">
    <div
      class="text-[10px] font-semibold uppercase tracking-wider text-muted-foreground mb-1"
    >
      {$_("updateNotice.notesHeading")}
    </div>
    <!-- `anywhere` rather than `break-words`: a bare changelog URL is one long word with
         no break opportunity, and it would otherwise run past the container edge. -->
    <div
      class="prose dark:prose-invert prose-xs max-w-none text-xs text-muted-foreground leading-relaxed font-sans [overflow-wrap:anywhere] max-h-44 overflow-y-auto pr-1"
      use:externalLinks
    >
      {@html notesHtml}
    </div>
  </div>
{/if}

<!-- Who installs it depends on who owns the bundle. Only a build we own gets a button
     that acts; the others get the instruction or the plain fact. -->
{#if update.kind === "self-update"}
  <div class="px-3 pb-3">
    <button
      class="inline-flex items-center justify-center gap-2 w-full px-3 py-1.5 rounded-md text-xs font-medium bg-primary text-primary-foreground hover:opacity-90 transition-opacity disabled:opacity-50 disabled:cursor-not-allowed"
      onclick={onInstall}
      disabled={busy}
    >
      {#if busy}
        <Loader2 size={12} class="animate-spin" />
      {/if}
      {$_("updateNotice.installAndRestart")}
    </button>
  </div>
{:else if update.kind === "app-store"}
  <div class="px-3 pb-3">
    <p class="text-xs text-muted-foreground">
      {$_("updateNotice.managedByStore", {
        values: { storeName: storeName ?? $_("settings.updates.genericStore") },
      })}
    </p>
  </div>
{:else}
  <div class="px-3 pb-3 space-y-1.5">
    <!-- Two wordings, because the version with a trailing colon reads as broken when
         there is no command to put after it. -->
    <p class="text-xs text-muted-foreground">
      {#if hint}
        {$_("updateNotice.howToUpdate", { values: { pmName: hint.pmName } })}
      {:else}
        {$_("updateNotice.updateExternally")}
      {/if}
    </p>
    {#if hint}
      <code
        class="block rounded bg-muted px-2 py-1 font-mono text-[11px] text-foreground select-all break-all"
        >{hint.cmd}</code
      >
    {/if}
  </div>
{/if}

<!-- Both labels name their own consequence. "Remind me later" and "Skip" alone read as
     disabled hints rather than choices, and gave no clue which one is permanent. -->
<div
  class="flex items-center justify-between gap-2 border-t border-border/50 bg-muted/30 px-3 py-2"
>
  <button
    class="rounded px-1.5 py-0.5 -mx-1.5 text-xs text-muted-foreground underline decoration-dotted decoration-muted-foreground/50 underline-offset-2 hover:text-foreground hover:bg-muted transition-colors"
    onclick={onRemindLater}
  >
    {$_("updateNotice.remindLater")}
  </button>
  <button
    class="rounded px-1.5 py-0.5 -mx-1.5 text-xs text-muted-foreground underline decoration-dotted decoration-muted-foreground/50 underline-offset-2 hover:text-foreground hover:bg-muted transition-colors"
    onclick={onSkip}
  >
    {$_("updateNotice.skipVersion", { values: { version: update.version } })}
  </button>
</div>
