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
  What the header indicator opens.

  A popover rather than a modal, and never a self-dismissing toast: an available update
  is a persistent, actionable state the user is allowed to walk away from. The body is
  shared with the Settings > Updates section - see UpdateDetails.
-->
<script lang="ts">
  import { _ } from "$lib/i18n";
  import { fly } from "svelte/transition";
  import { X } from "lucide-svelte";
  import type { UpdateInfo } from "$lib/stores/updater.svelte";
  import UpdateDetails from "./UpdateDetails.svelte";

  let {
    update,
    busy = false,
    onInstall,
    onSkip,
    onRemindLater,
    onClose,
  } = $props<{
    update: UpdateInfo;
    busy?: boolean;
    onInstall: () => void;
    onSkip: () => void;
    onRemindLater: () => void;
    onClose: () => void;
  }>();
</script>

<div
  class="absolute right-4 top-12 z-50 w-80 rounded-lg border border-border bg-card shadow-lg pointer-events-auto overflow-hidden"
  transition:fly={{ y: -8, duration: 200 }}
  role="dialog"
  aria-label={$_("updateNotice.title")}
>
  <div class="flex items-start justify-between gap-2 p-3 pb-2">
    <div class="space-y-0.5">
      <div class="text-sm font-medium">{$_("updateNotice.title")}</div>
      <div class="text-xs text-muted-foreground">
        {$_("updateNotice.version", { values: { version: update.version } })}
      </div>
    </div>
    <button
      class="p-1 -m-1 rounded text-muted-foreground hover:text-foreground hover:bg-muted transition-colors"
      onclick={onClose}
      title={$_("common.close")}
      aria-label={$_("common.close")}
    >
      <X size={14} />
    </button>
  </div>

  <UpdateDetails {update} {busy} {onInstall} {onSkip} {onRemindLater} />
</div>
