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
  import type { StashItem, FilePreviewData, Attachment } from "$lib/types";
  import { DesktopStorageAdapter } from "$lib/services/desktop-adapter";
  import { _, locale } from "$lib/i18n";
  import {
    formatBytes,
    calculateTotalAttachmentSize,
    checkAttachmentSizeLimits,
  } from "$lib/utils/format";
  import { fade, fly } from "svelte/transition";
  import { dragHandle } from "svelte-dnd-action";
  import {
    Circle,
    Edit3,
    Trash2,
    Check,
    Copy,
    GripVertical,
    RotateCcw,
    ArrowBigRightDash,
    ArrowUpToLine,
    ArrowDownToLine,
    Paperclip,
    FolderOutput,
  } from "lucide-svelte";
  import Editor from "./Editor.svelte";
  import FilePreviewTooltip from "./FilePreviewTooltip.svelte";
  import FilePreviewModal from "./FilePreviewModal.svelte";
  import { stat } from "@tauri-apps/plugin-fs";
  import { open, message } from "@tauri-apps/plugin-dialog";
  import { getRelativeTime } from "$lib/utils/date";
  import marked from "$lib/utils/markdown";
  import ActionButton from "./ActionButton.svelte";
  import { isStashHovered } from "$lib/stores/drag-state.svelte";

  let {
    item,
    mode,
    showReorderHandle = true,
    stripTagsOnCopy = true,
    isFirst = false,
    isLast = false,
    onMoveRequest,
    onMoveToTop,
    onMoveToBottom,
    onToggleComplete,
    onDelete,
    onUpdateContent,
    availableTags = [],
  } = $props<{
    item: StashItem;
    mode: "Drag" | "Copy";
    showReorderHandle?: boolean;
    stripTagsOnCopy?: boolean;
    isFirst?: boolean;
    isLast?: boolean;
    onMoveRequest: () => void;
    onMoveToTop: () => void;
    onMoveToBottom: () => void;
    onToggleComplete: () => void;
    onDelete: (skipConfirm?: boolean) => void;
    onUpdateContent: (content: string, attachments: Attachment[]) => void;
    availableTags?: string[];
  }>();

  const adapter = new DesktopStorageAdapter();
  let copied = $state(false);
  let isEditing = $state(false);
  let editContent = $state("");
  let editFiles = $state<Attachment[]>([]);
  let clickTimeout: ReturnType<typeof setTimeout> | undefined = undefined; // State for click debounce

  // File preview modal state
  let previewModalOpen = $state(false);
  let selectedPreviewFilePath = $state("");

  let isLoadingPreview = $state(false);

  // Drag-drop state: check if this stash is being hovered during a Tauri drag
  let dragOver = $derived(isStashHovered(item.id));
  let totalAttachmentSize = $derived(
    calculateTotalAttachmentSize(item.attachments),
  );

  $effect(() => {
    if (isEditing) {
      editContent = item.content;
      editContent = item.content;
      editFiles = [...item.attachments];
    }
  });

  async function handleCopy(invert = false) {
    let content = item.content;

    // Strip tags if setting is enabled (and not inverted) or disabled (and inverted)
    const shouldStrip = stripTagsOnCopy ? !invert : invert;

    if (shouldStrip) {
      content = content.replace(/#[\w-]+/g, "").trim();
    }

    const text =
      item.attachments.length > 0
        ? `${content}\n\n---\n# SYSTEM CONTEXT - LOCAL FILES\n${item.attachments.map((a) => a.filePath).join("\n")}`
        : content;
    await adapter.copyToClipboard(text);
    copied = true;
    setTimeout(() => (copied = false), 2000);
  }

  function saveEdit(content: string, attachments: Attachment[]) {
    if (
      content.trim() !== item.content ||
      JSON.stringify(attachments) !== JSON.stringify(item.attachments)
    ) {
      onUpdateContent(content, attachments);
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
        let newAttachments = [...item.attachments];

        const currentTotal = calculateTotalAttachmentSize(item.attachments);
        let plannedAddition = 0;

        for (const path of paths) {
          try {
            // Check size limits
            const metadata = await stat(path);
            const results = checkAttachmentSizeLimits(
              metadata.size,
              currentTotal + plannedAddition,
            );

            if (results.exceedsSingleLimit) {
              await message(
                $_("stashCard.fileTooLarge", {
                  values: {
                    limit: formatBytes(
                      results.singleLimitBytes,
                      $locale || "en",
                    ),
                  },
                }),
                { title: $_("stashCard.limitExceeded"), kind: "error" },
              );
              continue;
            }

            if (results.exceedsStashLimit) {
              await message(
                $_("stashCard.stashSizeExceeded", {
                  values: {
                    limit: formatBytes(
                      results.stashLimitBytes,
                      $locale || "en",
                    ),
                  },
                }),
                { title: $_("stashCard.limitExceeded"), kind: "error" },
              );
              continue;
            }

            // If check passes, track the size
            plannedAddition += metadata.size;

            const attachment = await adapter.saveAssetFromPath(
              path,
              item.contextId,
              item.id,
            );
            // Ensure IDs are consistent
            attachment.stashId = item.id;
            newAttachments.push(attachment);
          } catch (err) {
            console.error("Failed to save asset from path", err);
          }
        }
        // Update the stash with the new files
        onUpdateContent(item.content, newAttachments);
      }
    } catch (err) {
      console.error("Failed to open file picker", err);
    }
  }

  /**
   * Opens the file preview modal for a specific file.
   * @param filePath - The path to the file to preview
   */
  async function openFilePreview(filePath: string) {
    selectedPreviewFilePath = filePath;
    previewModalOpen = true;
  }

  /**
   * Closes the file preview modal.
   */
  function closeFilePreview() {
    previewModalOpen = false;
    selectedPreviewFilePath = "";
    selectedPreviewFilePath = "";
  }

  function handleCardClick(e: MouseEvent) {
    // If text is selected, don't trigger copy/edit behavior (standard UX)
    const selection = window.getSelection();
    if (selection && selection.toString().length > 0) return;

    const shiftKey = e.shiftKey;
    if (clickTimeout) clearTimeout(clickTimeout);
    clickTimeout = setTimeout(() => {
      handleCopy(shiftKey);
      clickTimeout = undefined;
    }, 250);
  }

  function handleDoubleClick(e: MouseEvent) {
    if (clickTimeout) {
      clearTimeout(clickTimeout);
      clickTimeout = undefined;
    }
    if (item.completed) return;
    e.stopPropagation();
    isEditing = true;
  }
</script>

<div
  class="group relative flex flex-col gap-2 rounded-lg border border-border bg-card p-3 shadow-sm hover:shadow-md transition-all hover:border-primary/50 cursor-pointer {item.completed
    ? 'opacity-60 grayscale-[0.3]'
    : ''} {dragOver ? 'border-primary border-2' : ''}"
  transition:fly={{ y: 20, duration: 300 }}
  onclick={handleCardClick}
  onkeydown={(e) => e.key === "Enter" && handleCopy(e.shiftKey)}
  ondblclick={handleDoubleClick}
  role="button"
  tabindex="0"
  draggable="false"
  data-stash-id={item.id}
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
      <ActionButton
        variant="complete"
        class={item.completed
          ? "text-green-500 bg-green-500/10 hover:bg-green-500/20 shadow-inner"
          : "text-muted-foreground bg-muted hover:bg-primary hover:text-primary-foreground"}
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
      </ActionButton>
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
            currentContextId={item.contextId}
            existingStashId={item.id}
            content={editContent}
            files={editFiles}
            onSave={saveEdit}
            onCancel={cancelEdit}
            saveLabel={$_("common.save")}
            autoFocus={true}
            {availableTags}
          />
        </div>
      {:else if item.content}
        <div
          class="prose dark:prose-invert prose-xs max-w-none text-sm text-foreground/90 leading-relaxed font-sans {item.completed
            ? 'line-through text-muted-foreground/70'
            : ''}"
        >
          {@html marked.parse(item.content)}
        </div>
      {:else}
        <div class="text-xs text-muted-foreground/50 italic text-center">
          {$_("stashCard.emptyStash")}
        </div>
      {/if}

      {#if !isEditing && item.attachments.length > 0}
        <div
          class="mt-2 flex gap-1.5 items-start leading-none {item.completed
            ? 'opacity-50'
            : ''}"
          onclick={(e) => e.stopPropagation()}
          onkeydown={(e) => e.stopPropagation()}
          role="presentation"
        >
          <!-- Drag All Attachments Button -->
          <div class="inline-block">
            <button
              class="inline-flex items-center gap-1 rounded-full border border-border bg-secondary/50 px-2 py-0.5 text-[10px] text-muted-foreground hover:bg-secondary hover:text-foreground hover:border-primary/50 transition-all cursor-grab active:cursor-grabbing"
              draggable="true"
              ondragstart={async (e) => {
                e.preventDefault();
                await adapter.startDrag(
                  "",
                  item.attachments.map((a) => a.filePath),
                );
              }}
              title={`${$_("stashCard.dragAllAttachments")} (${formatBytes(totalAttachmentSize, $locale || "en")})`}
            >
              <FolderOutput size={10} class="shrink-0" />
              <span class="truncate">{item.attachments.length}</span>
            </button>
          </div>
          <div class="inline-block flex flex-wrap items-center">
            {#each item.attachments as attachment}
              <FilePreviewTooltip
                filePath={attachment.filePath}
                fileName={attachment.fileName}
                onclick={() => openFilePreview(attachment.filePath)}
              />
            {/each}
          </div>
        </div>
      {/if}

      <div
        class="mt-2 text-[10px] text-muted-foreground/50 flex items-center justify-between"
      >
        <div class="flex items-center gap-0.5">
          <!-- Add File (always visible) -->
          {#if !item.completed}
            <ActionButton
              variant="instant"
              onclick={(e) => {
                e.stopPropagation();
                handleAddFile();
              }}
              title={$_("editor.addFile")}
            >
              <Paperclip size={12} />
            </ActionButton>
          {/if}
          <!-- Copy (Instant Action) -->
          <ActionButton
            variant="instant"
            onclick={(e) => {
              e.stopPropagation();
              handleCopy(e.shiftKey);
            }}
            title={$_("stashCard.copyToClipboard")}
          >
            <Copy size={12} />
          </ActionButton>
          <span>{getRelativeTime(item.createdAt, $_)}</span>
          {#if totalAttachmentSize > 0}
            <span>•</span>
            <span
              >{$_(
                item.attachments.length === 1
                  ? "stashCard.attachment"
                  : "stashCard.attachments",
                {
                  values: {
                    count: item.attachments.length,
                    size: formatBytes(totalAttachmentSize, $locale || "en"),
                  },
                },
              )}</span
            >
          {/if}
          {#if copied}
            <span>•</span>
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
            <!-- Move to Top -->
            {#if !item.completed && !isFirst}
              <ActionButton
                variant="additional"
                onclick={(e) => {
                  e.stopPropagation();
                  onMoveToTop();
                }}
                title={$_("stashCard.moveToTop")}
              >
                <ArrowUpToLine size={13} />
              </ActionButton>
            {/if}

            <!-- Move to Bottom -->
            {#if !item.completed && !isLast}
              <ActionButton
                variant="additional"
                onclick={(e) => {
                  e.stopPropagation();
                  onMoveToBottom();
                }}
                title={$_("stashCard.moveToBottom")}
              >
                <ArrowDownToLine size={13} />
              </ActionButton>
            {/if}

            <!-- Edit -->
            {#if !item.completed}
              <ActionButton
                variant="additional"
                onclick={(e) => {
                  e.stopPropagation();
                  isEditing = true;
                }}
                title={$_("stashCard.editContent")}
              >
                <Edit3 size={13} />
              </ActionButton>
            {/if}

            <!-- Move/Context -->
            {#if !item.completed}
              <ActionButton
                variant="additional"
                onclick={(e) => {
                  e.stopPropagation();
                  onMoveRequest();
                }}
                title={$_("stashCard.moveToContext")}
              >
                <ArrowBigRightDash size={13} />
              </ActionButton>
            {/if}

            <!-- Delete -->
            <ActionButton
              variant="additional"
              danger
              onclick={(e) => {
                e.stopPropagation();
                onDelete(e.shiftKey);
              }}
              title={$_("stashCard.shiftClickDelete")}
            >
              <Trash2 size={13} />
            </ActionButton>
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

<!-- File Preview Modal -->
<FilePreviewModal
  bind:open={previewModalOpen}
  files={item.attachments.map((a) => a.filePath)}
  bind:filePath={selectedPreviewFilePath}
  onClose={closeFilePreview}
/>
