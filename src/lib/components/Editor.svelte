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
  import { X, Paperclip } from "lucide-svelte";
  import { open } from "@tauri-apps/plugin-dialog";

  let {
    onStash,
    currentContextId,
    content = $bindable(""),
    files = $bindable([]),
    onSave,
    onCancel,
    saveLabel,
    autoFocus = false,
  } = $props<{
    onStash?: () => void;
    currentContextId?: string;
    content?: string;
    files?: string[];
    onSave?: (content: string, files: string[]) => Promise<void> | void;
    onCancel?: () => void;
    saveLabel?: string;
    autoFocus?: boolean;
  }>();

  let dragOver = $state(false);
  let isSaving = $state(false);

  const adapter = new DesktopStorageAdapter();

  function focusOnMount(node: HTMLTextAreaElement) {
    if (autoFocus) node.focus();
  }

  async function handleDrop(e: DragEvent) {
    e.preventDefault();
    e.stopPropagation();
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

  function handleDragEnter(e: DragEvent) {
    e.preventDefault();
    e.stopPropagation();
    dragOver = true;
  }

  function handleDragOver(e: DragEvent) {
    e.preventDefault();
    e.stopPropagation();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
    dragOver = true;
  }

  function handleDragLeave(e: DragEvent) {
    const target = e.currentTarget as Node;
    const related = e.relatedTarget as Node;
    if (target.contains(related)) return;
    dragOver = false;
  }

  async function save() {
    if (!content.trim() && files.length === 0) return;
    isSaving = true;
    try {
      if (onSave) {
        await onSave(content, files);
      } else {
        if (!currentContextId) {
          console.error("Context ID required for new stash");
          return;
        }
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
        onStash?.();
      }
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
    if (e.key === "Escape" && onCancel) {
      onCancel();
    }
  }

  function removeFile(index: number) {
    files = files.filter((_, i) => i !== index);
  }

  /**
   * Opens a file picker dialog and adds selected files to the stash.
   */
  async function handleAddFile() {
    try {
      const selected = await open({
        multiple: true,
        title: "Select files to attach",
      });
      if (selected) {
        // selected can be a string or string[] depending on multiple option
        const paths = Array.isArray(selected) ? selected : [selected];
        for (const path of paths) {
          try {
            const savedPath = await adapter.saveAssetFromPath(path);
            files = [...files, savedPath];
          } catch (err) {
            console.error("Failed to save asset from path", err);
          }
        }
      }
    } catch (err) {
      console.error("Failed to open file picker", err);
    }
  }
</script>

<div
  class="relative flex flex-col rounded-xl border border-border bg-[var(--muted-editor)] text-card-foreground shadow-sm h-[200px] transition-colors"
  ondrop={handleDrop}
  ondragover={handleDragOver}
  ondragenter={handleDragEnter}
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
    use:focusOnMount
  ></textarea>

  <div
    class="flex items-center justify-between p-2 border-t border-border bg-muted/30 rounded-b-xl"
  >
    <div
      class="flex gap-2 overflow-x-auto flex-1 min-w-0 no-scrollbar items-center"
    >
      <!-- Add file button -->
      <button
        class="shrink-0 flex items-center gap-1 bg-background px-2 py-0.5 rounded text-[10px] border border-border text-muted-foreground hover:text-foreground hover:border-primary/50 transition-colors shadow-sm"
        onclick={handleAddFile}
        title={$_("editor.addFile")}
        type="button"
      >
        <Paperclip size={10} />
        <span>{$_("editor.addFile")}</span>
      </button>
      {#each files as file, i}
        <div
          class="group/file bg-background px-2 py-0.5 rounded text-[10px] border border-border flex items-center gap-1 shadow-sm cursor-default shrink-0"
          title={file}
        >
          <span class="truncate max-w-[100px]">{file.split(/[\\/]/).pop()}</span
          >
          <button
            class="text-muted-foreground hover:text-destructive transition-colors"
            onclick={() => removeFile(i)}
            aria-label="Remove file"
          >
            <X size={10} />
          </button>
        </div>
      {/each}
      {#if files.length === 0}
        <span class="text-[10px] text-muted-foreground/60 italic"
          >{$_("editor.dragFilesHere")}</span
        >
      {/if}
    </div>

    <div class="flex gap-2">
      {#if onCancel}
        <button
          class="bg-muted text-muted-foreground hover:bg-muted/80 px-3 py-1.5 rounded-md font-medium text-xs transition-colors shadow-sm"
          onclick={onCancel}
        >
          {$_("common.cancel")}
        </button>
      {/if}
      <button
        class="bg-primary text-primary-foreground hover:bg-primary/90 px-3 py-1.5 rounded-md font-medium text-xs transition-colors shadow-sm disabled:opacity-50"
        onclick={save}
        disabled={isSaving || (!content && files.length === 0)}
      >
        {isSaving ? $_("editor.saving") : saveLabel || $_("editor.addStash")}
      </button>
    </div>
  </div>
</div>
