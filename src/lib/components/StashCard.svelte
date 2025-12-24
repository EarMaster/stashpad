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
  } from "lucide-svelte";

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
    onDelete: () => void;
    onUpdateContent: (content: string) => void;
  }>();

  const adapter = new DesktopStorageAdapter();
  let copied = $state(false);
  let isEditing = $state(false);
  let editContent = $state("");

  $effect(() => {
    if (isEditing) editContent = item.content;
  });

  function focusOnMount(node: HTMLTextAreaElement) {
    node.focus();
  }

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

  function saveEdit() {
    if (editContent.trim() !== item.content) {
      onUpdateContent(editContent);
    }
    isEditing = false;
  }

  function cancelEdit() {
    editContent = item.content;
    isEditing = false;
  }
</script>

<div
  class="group relative flex flex-col gap-2 rounded-lg border border-border bg-card p-3 shadow-sm hover:shadow-md transition-all hover:border-primary/50 cursor-pointer {item.completed
    ? 'opacity-60 grayscale-[0.3]'
    : ''}"
  transition:fly={{ y: 20, duration: 300 }}
  onclick={handleCopy}
  onkeydown={(e) => e.key === "Enter" && handleCopy()}
  role="button"
  tabindex="0"
  draggable="false"
>
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
        title={item.completed ? "Restore to active queue" : "Mark as completed"}
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
        title="Drag to AI Context"
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
          <textarea
            bind:value={editContent}
            use:focusOnMount
            class="w-full bg-muted/50 border border-border rounded p-2 text-sm font-mono outline-none focus:border-primary min-h-[80px] resize-none"
            placeholder="Edit stash content..."
            onkeydown={(e) => {
              if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) saveEdit();
              if (e.key === "Escape") cancelEdit();
            }}
          ></textarea>
          <div class="flex justify-end gap-2">
            <button
              class="p-1 px-2 rounded text-[10px] bg-muted hover:bg-muted/80 transition-colors flex items-center gap-1"
              onclick={cancelEdit}
            >
              <X size={10} /> Cancel
            </button>
            <button
              class="p-1 px-2 rounded text-[10px] bg-primary text-primary-foreground hover:bg-primary/90 transition-colors flex items-center gap-1 font-bold"
              onclick={saveEdit}
            >
              <Check size={10} /> Save
            </button>
          </div>
        </div>
      {:else}
        <div
          class="prose prose-invert prose-xs max-w-none line-clamp-3 text-sm text-foreground/90 leading-relaxed font-sans {item.completed
            ? 'line-through text-muted-foreground/70'
            : ''}"
        >
          {item.content || "Empty stash"}
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
          <span>{new Date(item.createdAt).toLocaleTimeString()}</span>
          {#if copied}
            <span
              class="text-green-500 font-medium animate-pulse"
              transition:fade>Copied!</span
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
              title="Copy to clipboard"
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
              title="Edit content"
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
              title="Move to context"
            >
              <ExternalLink size={13} />
            </button>

            <!-- Delete -->
            <button
              class="p-1.5 rounded hover:bg-red-500/10 text-muted-foreground hover:text-red-500 transition-all"
              onclick={(e) => {
                e.stopPropagation();
                if (e.shiftKey || confirm("Delete this stash?")) {
                  onDelete();
                }
              }}
              title="Delete stash (Shift+Click to skip confirmation)"
            >
              <Trash2 size={13} />
            </button>
          </div>

          <!-- Reorder Handle -->
          {#if showReorderHandle}
            <div
              class="reorder-handle p-1 text-muted-foreground/30 group-hover:text-muted-foreground/60 cursor-grab active:cursor-grabbing transition-colors"
              title="Drag to reorder"
              use:dragHandle
              onclick={(e) => e.stopPropagation()}
              onkeydown={(e) => e.stopPropagation()}
              role="button"
              tabindex="0"
              aria-label="Drag to reorder"
            >
              <GripVertical size={16} />
            </div>
          {/if}
        </div>
      </div>
    </div>
  </div>
</div>
