<!--
// SPDX-License-Identifier: AGPL-3.0-only

// Copyright (C) 2025 Nico Wiedemann
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
  import { DesktopStorageAdapter } from "$lib/services/desktop-adapter";
  import type { StashItem } from "$lib/types";
  import { _ } from "$lib/i18n";

  let { onStash, currentContextId } = $props<{
    onStash: () => void;
    currentContextId: string;
  }>();

  let content = $state("");
  let files = $state<string[]>([]);
  let dragOver = $state(false);
  let isSaving = $state(false);

  const adapter = new DesktopStorageAdapter();

  async function handleDrop(e: DragEvent) {
    e.preventDefault();
    dragOver = false;
    if (e.dataTransfer?.files) {
      // In a real scenario, this iterates over FileList
      for (let i = 0; i < e.dataTransfer.files.length; i++) {
        const file = e.dataTransfer.files[i];
        try {
          const path = await adapter.saveAsset(file);
          files = [...files, path];
        } catch (err) {
          console.error("Failed to save asset", err);
        }
      }
    }
  }

  function handleDragOver(e: DragEvent) {
    e.preventDefault();
    dragOver = true;
  }

  function handleDragLeave() {
    dragOver = false;
  }

  async function save() {
    if (!content.trim() && files.length === 0) return;
    isSaving = true;
    try {
      const stash: StashItem = {
        id: crypto.randomUUID(),
        content,
        files: [...files], // copy
        createdAt: new Date().toISOString(),
        contextId: currentContextId,
      };
      await adapter.saveStash(stash);
      content = "";
      files = [];
      onStash();
    } catch (e) {
      console.error(e);
    } finally {
      isSaving = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      save();
    }
  }
</script>

<div
  class="relative flex flex-col rounded-xl border border-border bg-card text-card-foreground shadow-sm h-[200px] transition-colors"
  ondrop={handleDrop}
  ondragover={handleDragOver}
  ondragleave={handleDragLeave}
  role="region"
  aria-label="Stash Editor"
>
  <!-- Overlay for drag -->
  {#if dragOver}
    <div
      class="absolute inset-0 bg-primary/5 rounded-xl border-2 border-primary border-dashed flex items-center justify-center z-10 pointer-events-none backdrop-blur-[1px]"
    >
      <span
        class="text-primary font-bold bg-background/80 px-4 py-2 rounded-full shadow-sm"
        >{$_("editor.dropFiles")}</span
      >
    </div>
  {/if}

  <textarea
    class="flex-1 bg-transparent resize-none outline-none text-sm p-4 placeholder:text-muted-foreground/50 font-mono"
    placeholder={$_("editor.placeholder")}
    bind:value={content}
    onkeydown={handleKeydown}
  ></textarea>

  <div
    class="flex items-center justify-between p-2 border-t border-border bg-muted/30 rounded-b-xl"
  >
    <div class="flex gap-2 overflow-x-auto max-w-[200px] no-scrollbar">
      {#each files as file}
        <div
          class="bg-background px-2 py-0.5 rounded text-[10px] border border-border flex items-center gap-1 shadow-sm"
          title={file}
        >
          <span class="truncate max-w-[80px]">{file.split(/[\\/]/).pop()}</span>
        </div>
      {/each}
      {#if files.length === 0}
        <span class="text-[10px] text-muted-foreground/60 italic pl-2"
          >{$_("editor.dragFilesHere")}</span
        >
      {/if}
    </div>

    <button
      class="bg-primary text-primary-foreground hover:bg-primary/90 px-3 py-1.5 rounded-md font-medium text-xs transition-colors shadow-sm disabled:opacity-50"
      onclick={save}
      disabled={isSaving || (!content && files.length === 0)}
    >
      {isSaving ? $_("editor.saving") : $_("editor.addStash")}
    </button>
  </div>
</div>
