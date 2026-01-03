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
  import type { StashItem } from "$lib/types";
  import { DesktopStorageAdapter } from "$lib/services/desktop-adapter";
  import { _ } from "$lib/i18n";
  import { fade, fly } from "svelte/transition";
  import { dragHandle } from "svelte-dnd-action";
  import {
    CheckCircle2,
    Circle,
    Edit3,
    Trash2,
    ExternalLink,
    Check,
    X,
    Copy,
    GripVertical,
    RotateCcw,
    Move,
    MoveRightIcon,
    ArrowBigRightDash,
    Paperclip,
  } from "lucide-svelte";
  import Editor from "./Editor.svelte";
  import { open } from "@tauri-apps/plugin-dialog";

  let {
    item,
    mode,
    showReorderHandle = true,
    onMoveRequest,
    onToggleComplete,
    onDelete,
    onUpdateContent,
  } = $props<{
    item: StashItem;
    mode: "Drag" | "Copy";
    showReorderHandle?: boolean;
    onMoveRequest: () => void;
    onToggleComplete: () => void;
    onDelete: (skipConfirm?: boolean) => void;
    onUpdateContent: (content: string, files: string[]) => void;
  }>();

  const adapter = new DesktopStorageAdapter();
  let copied = $state(false);
  let isEditing = $state(false);
  let editContent = $state("");
  let editFiles = $state<string[]>([]);
  let dragOver = $state(false);

  $effect(() => {
    if (isEditing) {
      editContent = item.content;
      editFiles = [...item.files];
    }
  });

  async function handleCopy() {
    const text =
      item.files.length > 0
        ? `${item.content}\n\n---\n# SYSTEM CONTEXT - LOCAL FILES\n${item.files.join("\n")}`
        : item.content;
    await adapter.copyToClipboard(text);
    copied = true;
    setTimeout(() => (copied = false), 2000);
  }

  function handleDragStart(e: DragEvent) {
    if (mode === "Copy") return;
    if (isEditing) return;

    e.preventDefault();
    adapter.startDrag(item.content, item.files);
  }

  function saveEdit(content: string, files: string[]) {
    if (
      content.trim() !== item.content ||
      JSON.stringify(files) !== JSON.stringify(item.files)
    ) {
      onUpdateContent(content, files);
    }
    isEditing = false;
  }

  function cancelEdit() {
    isEditing = false;
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
        const paths = Array.isArray(selected) ? selected : [selected];
        const newFiles = [...item.files];
        for (const path of paths) {
          try {
            const savedPath = await adapter.saveAssetFromPath(path);
            newFiles.push(savedPath);
          } catch (err) {
            console.error("Failed to save asset from path", err);
          }
        }
        // Update the stash with the new files
        onUpdateContent(item.content, newFiles);
      }
    } catch (err) {
      console.error("Failed to open file picker", err);
    }
  }

  /**
   * Handles file drop onto the stash card.
   */
  async function handleFileDrop(e: DragEvent) {
    e.preventDefault();
    e.stopPropagation();
    dragOver = false;
    if (item.completed) return;
    if (e.dataTransfer?.files) {
      const newFiles = [...item.files];
      for (let i = 0; i < e.dataTransfer.files.length; i++) {
        const file = e.dataTransfer.files[i];
        try {
          const path = await adapter.saveAsset(file);
          newFiles.push(path);
        } catch (err) {
          console.error("Failed to save asset", err);
        }
      }
      if (newFiles.length > item.files.length) {
        onUpdateContent(item.content, newFiles);
      }
    }
  }

  function handleFileDragOver(e: DragEvent) {
    e.preventDefault();
    e.stopPropagation();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
    if (!item.completed) dragOver = true;
  }

  function handleFileDragEnter(e: DragEvent) {
    e.preventDefault();
    e.stopPropagation();
    if (!item.completed) dragOver = true;
  }

  function handleFileDragLeave(e: DragEvent) {
    const target = e.currentTarget as Node;
    const related = e.relatedTarget as Node;
    if (target.contains(related)) return;
    dragOver = false;
  }
</script>

<div
  class="group relative flex flex-col gap-2 rounded-lg border border-border bg-card p-3 shadow-sm hover:shadow-md transition-all hover:border-primary/50 cursor-pointer {item.completed
    ? 'opacity-60 grayscale-[0.3]'
    : ''} {dragOver ? 'border-primary border-2' : ''}"
  transition:fly={{ y: 20, duration: 300 }}
  onclick={handleCopy}
  onkeydown={(e) => e.key === "Enter" && handleCopy()}
  ondrop={handleFileDrop}
  ondragover={handleFileDragOver}
  ondragenter={handleFileDragEnter}
  ondragleave={handleFileDragLeave}
  role="button"
  tabindex="0"
  draggable="false"
>
  <!-- Drop overlay -->
  {#if dragOver}
    <div
      class="absolute inset-0 bg-primary/10 rounded-lg flex items-center justify-center z-10 pointer-events-none"
    >
      <span
        class="text-primary font-bold bg-background/90 px-3 py-1.5 rounded-full shadow-sm text-xs"
        >{$_("editor.dropFiles")}</span
      >
    </div>
  {/if}
  <div class="flex items-start gap-3">
    <!-- Action Sidebar -->
    <div
      class="flex flex-col gap-1.5 shrink-0 pt-0.5"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
      role="presentation"
    >
      <!-- Complete Toggle -->
      <button
        class="h-7 w-7 flex items-center justify-center rounded-md transition-all {item.completed
          ? 'text-green-500 bg-green-500/10 hover:bg-green-500/20 shadow-inner'
          : 'text-muted-foreground bg-muted hover:bg-primary hover:text-primary-foreground'}"
        onclick={(e) => {
          e.stopPropagation();
          onToggleComplete();
        }}
        title={item.completed
          ? $_("stashCard.restoreToActive")
          : $_("stashCard.markAsCompleted")}
      >
        {#if item.completed}
          <RotateCcw
            size={16}
            class="transition-transform group-hover:rotate-[-45deg]"
          />
        {:else}
          <Circle size={16} />
        {/if}
      </button>

      <!-- Drag Handle (Internal - for AI Context) -->
      <div
        class="h-7 w-7 flex shrink-0 cursor-grab items-center justify-center rounded-md bg-muted text-muted-foreground transition-colors hover:bg-primary hover:text-primary-foreground active:cursor-grabbing"
        role="button"
        tabindex="0"
        draggable={mode === "Drag" && !isEditing}
        ondragstart={(e) => {
          handleDragStart(e);
        }}
        onclick={(e) => e.stopPropagation()}
        onkeydown={(e) => e.stopPropagation()}
        title={$_("stashCard.dragToAIContext")}
      >
        {#if copied}
          <Check size={14} />
        {:else}
          <span class="text-sm">✋</span>
        {/if}
      </div>
    </div>

    <div class="flex-1 min-w-0">
      {#if isEditing}
        <div
          class="flex flex-col gap-2"
          onclick={(e) => e.stopPropagation()}
          onkeydown={(e) => e.stopPropagation()}
          role="presentation"
        >
          <Editor
            content={editContent}
            files={editFiles}
            onSave={saveEdit}
            onCancel={cancelEdit}
            saveLabel={$_("common.save")}
            autoFocus={true}
          />
        </div>
      {:else if item.content}
        <div
          class="prose dark:prose-invert prose-xs max-w-none line-clamp-3 text-sm text-foreground/90 leading-relaxed font-sans {item.completed
            ? 'line-through text-muted-foreground/70'
            : ''}"
        >
          {item.content}
        </div>
      {:else}
        <div class="text-sm text-muted-foreground/50 italic text-center">
          {$_("stashCard.emptyStash")}
        </div>
      {/if}

      {#if !isEditing && item.files.length > 0}
        <div
          class="mt-2 flex flex-wrap gap-1.5 {item.completed
            ? 'opacity-50'
            : ''}"
        >
          {#each item.files as file}
            <span
              class="inline-flex items-center rounded-full border border-border bg-secondary/50 px-2 py-0.5 text-[10px] text-muted-foreground truncate max-w-[150px]"
            >
              {file.split(/[\\/]/).pop()}
            </span>
          {/each}
        </div>
      {/if}

      <div
        class="mt-2 text-[10px] text-muted-foreground/50 flex items-center justify-between"
      >
        <div class="flex items-center gap-2">
          <!-- Add File (always visible) -->
          {#if !item.completed}
            <button
              class="p-1 rounded hover:bg-muted text-muted-foreground/50 hover:text-foreground transition-all"
              onclick={(e) => {
                e.stopPropagation();
                handleAddFile();
              }}
              title={$_("editor.addFile")}
            >
              <Paperclip size={12} />
            </button>
          {/if}
          <span>{new Date(item.createdAt).toLocaleTimeString()}</span>
          {#if copied}
            <span
              class="text-green-500 font-medium animate-pulse"
              transition:fade>{$_("stashCard.copied")}</span
            >
          {/if}
        </div>

        <div class="flex items-center gap-1">
          <div
            class="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity"
            onclick={(e) => e.stopPropagation()}
            onkeydown={(e) => e.stopPropagation()}
            role="presentation"
          >
            <!-- Copy -->
            <button
              class="p-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-all"
              onclick={(e) => {
                e.stopPropagation();
                handleCopy();
              }}
              title={$_("stashCard.copyToClipboard")}
            >
              <Copy size={13} />
            </button>

            <!-- Edit -->
            <button
              class="p-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-all"
              onclick={(e) => {
                e.stopPropagation();
                isEditing = true;
              }}
              title={$_("stashCard.editContent")}
              disabled={item.completed}
            >
              <Edit3 size={13} />
            </button>

            <!-- Move/Context -->
            <button
              class="p-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-all flex items-center gap-1"
              onclick={(e) => {
                e.stopPropagation();
                onMoveRequest();
              }}
              title={$_("stashCard.moveToContext")}
            >
              <ArrowBigRightDash size={13} />
            </button>

            <!-- Delete -->
            <button
              class="p-1.5 rounded hover:bg-red-500/10 text-muted-foreground hover:text-red-500 transition-all"
              onclick={(e) => {
                e.stopPropagation();
                onDelete(e.shiftKey);
              }}
              title={$_("stashCard.shiftClickDelete")}
            >
              <Trash2 size={13} />
            </button>
          </div>

          <!-- Reorder Handle -->
          {#if showReorderHandle}
            <div
              class="reorder-handle p-1 text-muted-foreground/30 group-hover:text-muted-foreground/60 cursor-grab active:cursor-grabbing transition-colors"
              title={$_("stashCard.dragToReorder")}
              use:dragHandle
              onclick={(e) => e.stopPropagation()}
              onkeydown={(e) => e.stopPropagation()}
              role="button"
              tabindex="0"
              aria-label={$_("stashCard.dragToReorder")}
            >
              <GripVertical size={16} />
            </div>
          {/if}
        </div>
      </div>
    </div>
  </div>
</div>
