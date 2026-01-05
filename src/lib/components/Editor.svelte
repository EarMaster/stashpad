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
  import type { StashItem, FilePreviewData } from "$lib/types";
  import { _ } from "$lib/i18n";
  import {
    X,
    Paperclip,
    Image,
    Video,
    FileText,
    File,
    Bold,
    Italic,
    Link as LinkIcon,
    List,
    Code,
    Heading,
  } from "lucide-svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { getCaretCoordinates } from "$lib/utils/caret";
  import FilePreviewModal from "./FilePreviewModal.svelte";
  import { fade } from "svelte/transition";
  import { convertFileSrc } from "@tauri-apps/api/core";

  let {
    onStash,
    currentContextId,
    content = $bindable(""),
    files = $bindable([]),
    onSave,
    onCancel,
    saveLabel,
    autoFocus = false,
    availableTags = [],
  } = $props<{
    onStash?: (stashId?: string) => void;
    currentContextId?: string;
    content?: string;
    files?: string[];
    onSave?: (content: string, files: string[]) => Promise<void> | void;
    onCancel?: () => void;
    saveLabel?: string;
    autoFocus?: boolean;
    availableTags?: string[];
  }>();

  let dragOver = $state(false);
  let isSaving = $state(false);

  // Tag Autocomplete
  let showSuggestions = $state(false);
  let filteredTags = $state<string[]>([]);
  let focusedTagIndex = $state(0);
  let suggestionPosition = $state<{ top: number; left: number }>({
    top: 0,
    left: 0,
  });
  let triggerStart = $state<number | null>(null);
  let hoveringSuggestions = $state(false);

  // File preview state
  let previewModalOpen = $state(false);
  let selectedPreviewFilePath = $state("");

  let hoveringFileIndex = $state<number | null>(null);
  let hoverPreviewData = $state<FilePreviewData | null>(null);
  let isLoadingHoverPreview = $state(false);
  let hoverTimeout = $state<ReturnType<typeof setTimeout> | null>(null);

  const adapter = new DesktopStorageAdapter();

  function focusOnMount(node: HTMLTextAreaElement) {
    if (autoFocus) node.focus();
  }

  function handleInput() {
    checkForTagTrigger();
  }

  function checkForTagTrigger() {
    if (!textareaRef) return;

    const cursorParams = getCursorParams();
    const textBeforeCursor = content.slice(0, cursorParams.end);
    const wordMatch = textBeforeCursor.match(/#[\w-]*$/); // Match #word at end

    if (wordMatch) {
      const query = wordMatch[0].slice(1).toLowerCase(); // remove #
      const matchIndex = wordMatch.index!;

      filteredTags = availableTags.filter((t) =>
        t.toLowerCase().startsWith("#" + query),
      );

      if (filteredTags.length > 0) {
        showSuggestions = true;
        focusedTagIndex = 0;
        triggerStart = matchIndex;

        updateSuggestionPosition(cursorParams.end);
      } else {
        showSuggestions = false;
      }
    } else {
      showSuggestions = false;
    }
  }

  function insertTag(tag: string) {
    if (!textareaRef || triggerStart === null) return;

    const before = content.slice(0, triggerStart);
    const after = content.slice(textareaRef.selectionEnd);

    content = before + tag + " " + after;

    const newCursorPos = before.length + tag.length + 1; // +1 for space (tag has #)

    showSuggestions = false;

    // Restore focus and cursor
    setTimeout(() => {
      textareaRef?.focus();
      textareaRef?.setSelectionRange(newCursorPos, newCursorPos);
    }, 0);
  }

  // Helper for text area caret position
  function updateSuggestionPosition(cursorIndex: number) {
    if (!textareaRef) return;

    try {
      const coords = getCaretCoordinates(textareaRef, cursorIndex, {
        appendTo: textareaRef.parentElement || undefined,
      });

      suggestionPosition = {
        top:
          textareaRef.offsetTop +
          coords.top -
          textareaRef.scrollTop +
          (coords.lineHeight + coords.height) / 2,
        left: textareaRef.offsetLeft + coords.left - textareaRef.scrollLeft,
      };
    } catch (e) {
      console.error("Failed to calculate caret coordinates", e);
    }
  }

  function getCursorParams() {
    return {
      start: textareaRef?.selectionStart || 0,
      end: textareaRef?.selectionEnd || 0,
    };
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

  async function save(e?: Event) {
    if (!content.trim() && files.length === 0) return;
    const invertPosition = (e as KeyboardEvent | MouseEvent)?.shiftKey ?? false;
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
        await adapter.saveStash(stash, { invertPosition });
        content = "";
        files = [];
        onStash?.(stash.id);
      }
    } catch (e) {
      console.error(e);
    } finally {
      isSaving = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (showSuggestions && filteredTags.length > 0) {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        focusedTagIndex = (focusedTagIndex + 1) % filteredTags.length;
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        focusedTagIndex =
          (focusedTagIndex - 1 + filteredTags.length) % filteredTags.length;
        return;
      }
      if (e.key === "Enter" || e.key === "Tab") {
        e.preventDefault();
        if (filteredTags[focusedTagIndex]) {
          insertTag(filteredTags[focusedTagIndex]);
        }
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        showSuggestions = false;
        return;
      }
    }

    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      save(e);
    }
    if (e.key === "Escape" && onCancel) {
      onCancel();
    }
  }

  function removeFile(index: number) {
    files = files.filter((_, i) => i !== index);
  }

  /**
   * Determine file type icon based on extension.
   */
  function getFileIcon(path: string) {
    const ext = path.split(".").pop()?.toLowerCase() || "";
    const imageExts = [
      "png",
      "jpg",
      "jpeg",
      "gif",
      "webp",
      "svg",
      "bmp",
      "ico",
    ];
    const videoExts = ["mp4", "webm", "ogg", "ogv", "mov", "avi", "mkv"];
    const textExts = [
      "txt",
      "md",
      "markdown",
      "json",
      "xml",
      "html",
      "htm",
      "css",
      "js",
      "mjs",
      "ts",
      "tsx",
      "jsx",
      "py",
      "rs",
      "go",
      "java",
      "c",
      "h",
      "cpp",
      "hpp",
      "cc",
      "cs",
      "rb",
      "php",
      "sh",
      "bash",
      "zsh",
      "ps1",
      "yaml",
      "yml",
      "toml",
      "ini",
      "cfg",
      "conf",
      "log",
      "sql",
      "svelte",
      "vue",
    ];
    if (imageExts.includes(ext)) return "image";
    if (videoExts.includes(ext)) return "video";
    if (textExts.includes(ext)) return "text";
    return "file";
  }

  /**
   * Opens the file preview modal for a specific file.
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

  let hoverTooltipPosition = $state<"top" | "bottom">("bottom");

  /**
   * Handle mouse enter on a file badge - start hover preview.
   */
  function handleFileMouseEnter(
    index: number,
    filePath: string,
    e: MouseEvent,
  ) {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    hoverTooltipPosition = rect.top < 200 ? "top" : "bottom";

    hoverTimeout = setTimeout(async () => {
      hoveringFileIndex = index;
      isLoadingHoverPreview = true;
      try {
        hoverPreviewData = await adapter.readFileForPreview(filePath);
      } catch (err) {
        console.error("Failed to load hover preview:", err);
        hoverPreviewData = null;
      } finally {
        isLoadingHoverPreview = false;
      }
    }, 300);
  }

  /**
   * Handle mouse leave - cancel hover preview.
   */
  function handleFileMouseLeave() {
    if (hoverTimeout) {
      clearTimeout(hoverTimeout);
      hoverTimeout = null;
    }
    hoveringFileIndex = null;
    hoverPreviewData = null;
  }

  /**
   * Get video source URL for preview.
   */
  function getVideoSrc(path: string): string {
    return convertFileSrc(path);
  }

  /**
   * Opens a file picker dialog and adds selected files to the stash.
   */
  let textareaRef = $state<HTMLTextAreaElement | null>(null);

  function insertMarkdown(prefix: string, suffix: string = "") {
    if (!textareaRef) return;
    const start = textareaRef.selectionStart;
    const end = textareaRef.selectionEnd;
    const text = content;
    const selection = text.substring(start, end);

    const replacement = prefix + selection + suffix;
    content = text.substring(0, start) + replacement + text.substring(end);

    setTimeout(() => {
      if (!textareaRef) return;
      textareaRef.focus();
      if (start === end) {
        const newPos = start + prefix.length;
        textareaRef.setSelectionRange(newPos, newPos);
      } else {
        textareaRef.setSelectionRange(
          start + prefix.length,
          start + prefix.length + selection.length,
        );
      }
    }, 0);
  }

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

  <div class="flex items-center gap-0.5 p-2 pb-0 border-b border-border/50">
    <button
      class="p-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-all"
      onclick={() => insertMarkdown("**", "**")}
      title={$_("editor.bold")}
      tabindex="-1"
    >
      <Bold size={14} />
    </button>
    <button
      class="p-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-all"
      onclick={() => insertMarkdown("_", "_")}
      title={$_("editor.italic")}
      tabindex="-1"
    >
      <Italic size={14} />
    </button>
    <button
      class="p-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-all"
      onclick={() => insertMarkdown("### ")}
      title={$_("editor.heading")}
      tabindex="-1"
    >
      <Heading size={14} />
    </button>
    <div class="w-px h-4 bg-border/50 mx-1"></div>
    <button
      class="p-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-all"
      onclick={() => insertMarkdown("- ")}
      title={$_("editor.list")}
      tabindex="-1"
    >
      <List size={14} />
    </button>
    <button
      class="p-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-all"
      onclick={() => insertMarkdown("`", "`")}
      title={$_("editor.code")}
      tabindex="-1"
    >
      <Code size={14} />
    </button>
    <button
      class="p-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-all"
      onclick={() => insertMarkdown("[", "](url)")}
      title={$_("editor.link")}
      tabindex="-1"
    >
      <LinkIcon size={14} />
    </button>
  </div>

  <textarea
    class="flex-1 bg-transparent resize-none outline-none text-sm p-4 placeholder:text-muted-foreground/50 font-mono"
    placeholder={$_("editor.placeholder")}
    bind:value={content}
    bind:this={textareaRef}
    onkeydown={handleKeydown}
    oninput={handleInput}
    onblur={() =>
      setTimeout(() => {
        if (!hoveringSuggestions) showSuggestions = false;
      }, 200)}
    use:focusOnMount
  ></textarea>

  {#if showSuggestions}
    <div
      class="absolute z-50 bg-popover border border-border rounded-md shadow-lg flex flex-col min-w-[120px] max-h-[200px] overflow-y-auto"
      style="top: {suggestionPosition.top}px; left: {suggestionPosition.left}px;"
      onmouseenter={() => (hoveringSuggestions = true)}
      onmouseleave={() => (hoveringSuggestions = false)}
      role="presentation"
    >
      {#each filteredTags as tag, i}
        <button
          class="text-xs px-2 py-1.5 text-left hover:bg-muted transition-colors {i ===
          focusedTagIndex
            ? 'bg-primary/10 text-primary'
            : ''}"
          onclick={() => {
            insertTag(tag);
            hoveringSuggestions = false;
          }}
        >
          {tag}
        </button>
      {/each}
    </div>
  {/if}

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
          class="relative group/file bg-background px-2 py-0.5 rounded text-[10px] border border-border flex items-center gap-1 shadow-sm shrink-0 cursor-pointer hover:border-primary/50 transition-colors"
          title={$_("filePreview.clickToPreview")}
          onmouseenter={(e) => handleFileMouseEnter(i, file, e)}
          onmouseleave={handleFileMouseLeave}
          onclick={() => openFilePreview(file)}
          onkeydown={(e) => e.key === "Enter" && openFilePreview(file)}
          role="button"
          tabindex="0"
        >
          <!-- File type icon -->
          {#if getFileIcon(file) === "image"}
            <Image size={10} class="shrink-0 text-muted-foreground" />
          {:else if getFileIcon(file) === "video"}
            <Video size={10} class="shrink-0 text-muted-foreground" />
          {:else if getFileIcon(file) === "text"}
            <FileText size={10} class="shrink-0 text-muted-foreground" />
          {:else}
            <File size={10} class="shrink-0 text-muted-foreground" />
          {/if}
          <span class="truncate max-w-[100px]">{file.split(/[\\/]/).pop()}</span
          >
          <button
            class="text-muted-foreground hover:text-destructive transition-colors"
            onclick={(e) => {
              e.stopPropagation();
              removeFile(i);
            }}
            aria-label="Remove file"
          >
            <X size={10} />
          </button>

          <!-- Hover Preview Tooltip -->
          {#if hoveringFileIndex === i}
            <div
              class="absolute z-50 left-1/2 -translate-x-1/2 pointer-events-none {hoverTooltipPosition ===
              'top'
                ? 'top-full mt-2'
                : 'bottom-full mb-2'}"
              transition:fade={{ duration: 100 }}
            >
              <div
                class="relative bg-popover border border-border rounded-lg shadow-xl"
              >
                {#if isLoadingHoverPreview}
                  <div
                    class="flex items-center justify-center w-32 h-24 bg-muted/50 p-2"
                  >
                    <div class="animate-pulse text-xs text-muted-foreground">
                      {$_("common.loading")}
                    </div>
                  </div>
                {:else if hoverPreviewData}
                  <div class="p-2">
                    {#if hoverPreviewData.fileType === "image"}
                      <img
                        src={hoverPreviewData.content}
                        alt={hoverPreviewData.fileName}
                        class="block max-w-[180px] max-h-[150px] object-contain rounded"
                      />
                    {:else if hoverPreviewData.fileType === "video"}
                      <!-- svelte-ignore a11y_media_has_caption -->
                      <video
                        src={getVideoSrc(hoverPreviewData.content)}
                        class="block max-w-[180px] max-h-[150px] object-contain rounded"
                        muted
                        autoplay
                        loop
                        playsinline
                      ></video>
                    {:else if hoverPreviewData.fileType === "text"}
                      <pre
                        class="w-28 h-20 overflow-hidden bg-muted/50 p-2 text-[8px] font-mono text-foreground whitespace-pre-wrap break-words rounded">{hoverPreviewData.content.slice(
                          0,
                          300,
                        )}</pre>
                    {:else}
                      <div
                        class="flex items-center justify-center w-28 h-20 bg-muted/50 rounded"
                      >
                        <File size={20} class="text-muted-foreground" />
                      </div>
                    {/if}
                  </div>

                  <!-- File Name Footer -->
                  <div class="px-2 pb-2 pt-0">
                    <div
                      class="text-[9px] text-muted-foreground truncate text-center"
                      title={hoverPreviewData.fileName}
                    >
                      {hoverPreviewData.fileName}
                    </div>
                  </div>
                {/if}
                <!-- Arrow Pointer -->
                <div
                  class="absolute left-1/2 -translate-x-1/2 w-2 h-2 rotate-45 bg-popover border-border {hoverTooltipPosition ===
                  'top'
                    ? 'top-0 -translate-y-1/2 border-l border-t'
                    : 'bottom-0 translate-y-1/2 border-r border-b'}"
                ></div>
              </div>
            </div>
          {/if}
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

<!-- File Preview Modal -->
<FilePreviewModal
  bind:open={previewModalOpen}
  {files}
  bind:filePath={selectedPreviewFilePath}
  onClose={closeFilePreview}
/>
