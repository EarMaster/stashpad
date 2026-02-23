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
  import type { StashItem, FilePreviewData, Attachment } from "$lib/types";
  import { detectLanguage } from "$lib/utils/language-detection";
  import { _ } from "$lib/i18n";
  import {
    X,
    Paperclip,
    Image,
    Video,
    FileText,
    File as FileIcon,
    Bold,
    Italic,
    Link as LinkIcon,
    List,
    Code,
    Heading,
  } from "lucide-svelte";
  import { getCaretCoordinates } from "$lib/utils/caret";
  import FilePreviewModal from "./FilePreviewModal.svelte";
  import ConfirmationDialog from "./ConfirmationDialog.svelte";
  import Tooltip from "./Tooltip.svelte";
  import { fade } from "svelte/transition";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount, onDestroy, untrack } from "svelte";
  import { findStashAtPosition } from "$lib/stores/drag-state.svelte";
  import { stat } from "@tauri-apps/plugin-fs";
  import { open, message } from "@tauri-apps/plugin-dialog";
  import {
    checkAttachmentSizeLimits,
    calculateTotalAttachmentSize,
    formatBytes,
  } from "$lib/utils/format";
  import { locale } from "$lib/i18n";
  import { tooltip } from "$lib/actions/tooltip";
  import { resizeImage } from "$lib/utils/files";
  import { readFile } from "@tauri-apps/plugin-fs";

  let {
    onStash,
    currentContextId,
    existingStashId,
    content = $bindable(""),
    files = $bindable([]),
    initialFiles = [],
    onSave,
    onCancel,
    saveLabel,
    autoFocus = false,
    availableTags = [],
    pasteAsAttachmentThreshold = 8,
    resizeImages = true,
  } = $props<{
    onStash?: (stashId?: string) => void;
    currentContextId?: string;
    /** When editing an existing stash, pass its ID for proper file storage */
    existingStashId?: string;
    content?: string;
    files?: Attachment[];
    /** Original attachments when editing - used for tracking adds/removes */
    initialFiles?: Attachment[];
    onSave?: (content: string, files: Attachment[]) => Promise<void> | void;
    onCancel?: () => void;
    saveLabel?: string;
    autoFocus?: boolean;
    availableTags?: string[];
    /** Number of lines before pasted text becomes an attachment. 0 = ask user */
    pasteAsAttachmentThreshold?: number;
    resizeImages?: boolean;
  }>();

  // Generate or use existing stash ID for file storage organization
  // This ensures files are stored in the correct folder structure before the stash is saved
  // Using $state so we can regenerate the ID after saving a new stash
  let stashId = $state(untrack(() => existingStashId) ?? crypto.randomUUID());

  $effect(() => {
    if (existingStashId && existingStashId !== stashId) {
      stashId = existingStashId;
    }
  });

  let dragOver = $state(false);
  let dragCounter = 0;
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

  // Paste dialog state (when threshold is 0)
  let pasteChoiceDialogOpen = $state(false);
  let pendingPasteText = $state<string | null>(null);

  // Track files added/removed during edit session for deferred file operations
  // Added files should be deleted on cancel, removed files should be deleted on save
  let addedFilePaths = $state<string[]>([]);
  let removedFilePaths = $state<string[]>([]);

  // Clear confirmation dialog state
  let clearConfirmDialogOpen = $state(false);

  const adapter = new DesktopStorageAdapter();

  // Tauri native drag-drop event listeners
  let unlistenDrop: UnlistenFn | null = null;
  let unlistenEnter: UnlistenFn | null = null;
  let unlistenLeave: UnlistenFn | null = null;

  onMount(async () => {
    // Listen for Tauri's native drag-drop events
    unlistenEnter = await listen("tauri://drag-enter", () => {
      dragOver = true;
    });

    unlistenLeave = await listen("tauri://drag-leave", () => {
      dragOver = false;
    });

    unlistenDrop = await listen<{
      paths: string[];
      position: { x: number; y: number };
    }>("tauri://drag-drop", async (event) => {
      dragOver = false;

      // Check if drop is targeting a StashCard - if so, Queue handles it
      const targetStashId = findStashAtPosition(
        event.payload.position.x,
        event.payload.position.y,
      );
      if (targetStashId) {
        return; // Let Queue.svelte handle this drop
      }

      const paths = event.payload.paths;
      for (const path of paths) {
        try {
          // Check if it's an image and we need to resize
          const isImage = /\.(jpg|jpeg|png|webp|gif|bmp)$/i.test(path);
          if (resizeImages && isImage) {
            try {
              // Read the file manually to resize it
              const data = await readFile(path);
              const blob = new Blob([data], { type: "image/jpeg" }); // mime type guess, resizeImage handles check

              // resizeImage expects File or Blob.
              // We need to name it correct so it can detect type from name if blob type is generic?
              // Actually resizeImage checks file.type.
              // Let's try to get mime from extension or just pass a File object
              const fileName = path.split(/[\\/]/).pop() || "image.png";
              // Simple mime mapping for the file creation
              const ext = fileName.split(".").pop()?.toLowerCase();
              let mime = "application/octet-stream";
              if (ext === "png") mime = "image/png";
              else if (ext === "jpg" || ext === "jpeg") mime = "image/jpeg";
              else if (ext === "webp") mime = "image/webp";
              else if (ext === "gif") mime = "image/gif";

              const originalFile = new File([data], fileName, { type: mime });
              const resizedBlob = await resizeImage(originalFile);

              // If it was resized, it returns a Blob. If not, it returns original File.
              // We need to save it.
              // If it's the original file, we can use saveAssetFromPath (more efficient as it might copy/move?)
              // BUT saveAssetFromPath invokes rust which reads file again.
              // If we already read it, might as well use saveAsset.
              // Actually, if resizeImage returns the original object, we can strictly check reference.

              if (resizedBlob === originalFile) {
                // No resize needed, use efficient path method
                const attachment = await adapter.saveAssetFromPath(
                  path,
                  currentContextId,
                  stashId,
                );
                files = [...files, attachment];
                addedFilePaths = [...addedFilePaths, attachment.filePath];
              } else {
                // Resized, save the new blob
                // Need to convert Blob to File
                const newFile = new File([resizedBlob], fileName, {
                  type: resizedBlob.type,
                  lastModified: Date.now(),
                });

                const attachment = await adapter.saveAsset(
                  newFile,
                  currentContextId,
                  stashId,
                );
                files = [...files, attachment];
                addedFilePaths = [...addedFilePaths, attachment.filePath];
              }
            } catch (resizeErr) {
              console.warn(
                "Failed to resize image, falling back to original",
                resizeErr,
              );
              const attachment = await adapter.saveAssetFromPath(
                path,
                currentContextId,
                stashId,
              );
              files = [...files, attachment];
              addedFilePaths = [...addedFilePaths, attachment.filePath];
            }
          } else {
            const attachment = await adapter.saveAssetFromPath(
              path,
              currentContextId,
              stashId,
            );
            files = [...files, attachment];
            // Track added file for cleanup on cancel
            addedFilePaths = [...addedFilePaths, attachment.filePath];
          }
        } catch (err) {
          console.error("Failed to save dropped asset", err);
        }
      }
    });
  });

  onDestroy(() => {
    unlistenEnter?.();
    unlistenLeave?.();
    unlistenDrop?.();
  });

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

  /**
   * Handle drop events for file attachments.
   * NOTE: With dragDropEnabled=true, Tauri intercepts drops.
   * This handler is kept for fallback but primary handling is via Tauri events.
   */
  async function handleDrop(e: DragEvent) {
    e.preventDefault();
    e.stopPropagation();
    dragOver = false;
    // Tauri handles drops via tauri://drag-drop event
  }

  function handleDragEnter(e: DragEvent) {
    e.preventDefault();
    e.stopPropagation();
    // Tauri handles via tauri://drag-enter event
  }

  function handleDragOver(e: DragEvent) {
    e.preventDefault();
    e.stopPropagation();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
  }

  function handleDragLeave(e: DragEvent) {
    e.preventDefault();
    e.stopPropagation();
    // Tauri handles via tauri://drag-leave event
  }

  /**
   * Handle paste events to detect files or large text.
   * Files are added as attachments.
   * Large text (exceeding threshold) is converted to a text file attachment.
   * When threshold is 0, a dialog asks the user what to do.
   */
  async function handlePaste(e: ClipboardEvent) {
    const clipboardData = e.clipboardData;
    if (!clipboardData) return;

    // Check for files first
    if (clipboardData.files && clipboardData.files.length > 0) {
      e.preventDefault();
      for (let i = 0; i < clipboardData.files.length; i++) {
        const file = clipboardData.files[i];
        try {
          let fileToSave = file;

          if (resizeImages && file.type.startsWith("image/")) {
            try {
              const resizedBlob = await resizeImage(file);
              if (resizedBlob !== file) {
                fileToSave = new File([resizedBlob], file.name, {
                  type: resizedBlob.type,
                  lastModified: Date.now(),
                });
              }
            } catch (e) {
              console.warn("Failed to resize pasted image", e);
            }
          }

          const attachment = await adapter.saveAsset(
            fileToSave,
            currentContextId,
            stashId,
          );
          files = [...files, attachment];
          // Track added file for cleanup on cancel
          addedFilePaths = [...addedFilePaths, attachment.filePath];
        } catch (err) {
          console.error("Failed to save pasted file:", err);
        }
      }
      return;
    }

    // Check for text
    const pastedText = clipboardData.getData("text/plain");
    if (!pastedText) return;

    const lineCount = pastedText.split("\n").length;

    // If threshold is 0, ask user what to do
    if (pasteAsAttachmentThreshold === 0) {
      e.preventDefault();
      pendingPasteText = pastedText;
      pasteChoiceDialogOpen = true;
      return;
    }

    // If text exceeds threshold, convert to attachment
    if (lineCount > pasteAsAttachmentThreshold) {
      e.preventDefault();
      await saveTextAsAttachment(pastedText);
      return;
    }

    // Otherwise, let the default paste behavior happen (text is pasted normally)
  }

  /**
   * Save text as a text file attachment.
   * Detects the programming language from content and uses the appropriate file extension.
   */
  async function saveTextAsAttachment(text: string) {
    // Detect language from content to determine file extension
    const detection = await detectLanguage(text);
    const extension = detection.extension;

    // Generate a filename based on first line or timestamp
    const firstLine = text.split("\n")[0].slice(0, 30).trim();
    const safeName = firstLine.replace(/[^a-zA-Z0-9_-]/g, "_") || "pasted_code";
    const timestamp = Date.now();
    const filename = `${safeName}_${timestamp}.${extension}`;

    // Determine MIME type based on extension
    const mimeTypes: Record<string, string> = {
      js: "application/javascript",
      ts: "application/typescript",
      json: "application/json",
      html: "text/html",
      css: "text/css",
      xml: "application/xml",
      md: "text/markdown",
    };
    const mimeType = mimeTypes[extension] ?? "text/plain";

    // Create a File object from the text
    const blob = new Blob([text], { type: mimeType });
    const file = new File([blob], filename, { type: mimeType });

    try {
      const attachment = await adapter.saveAsset(
        file,
        currentContextId,
        stashId,
        detection.language ?? undefined,
      );
      files = [...files, attachment];
      // Track added file for cleanup on cancel
      addedFilePaths = [...addedFilePaths, attachment.filePath];
    } catch (err) {
      console.error("Failed to save text as attachment:", err);
    }
  }

  /**
   * Handle paste confirmation (save as attachment).
   */
  function handlePasteConfirm() {
    if (pendingPasteText) {
      saveTextAsAttachment(pendingPasteText);
    }
    pendingPasteText = null;
    pasteChoiceDialogOpen = false;
  }

  /**
   * Handle paste cancel (paste as inline text).
   */
  function handlePasteCancel() {
    if (pendingPasteText) {
      // Insert text at cursor position
      if (textareaRef) {
        const start = textareaRef.selectionStart;
        const end = textareaRef.selectionEnd;
        const before = content.slice(0, start);
        const after = content.slice(end);
        content = before + pendingPasteText + after;

        // Move cursor to end of inserted text
        const newPos = start + pendingPasteText.length;
        setTimeout(() => {
          textareaRef?.focus();
          textareaRef?.setSelectionRange(newPos, newPos);
        }, 0);
      } else {
        content += pendingPasteText;
      }
    }
    pendingPasteText = null;
    pasteChoiceDialogOpen = false;
  }

  async function save(e?: Event) {
    if (!content.trim() && files.length === 0) return;
    const invertPosition = (e as KeyboardEvent | MouseEvent)?.shiftKey ?? false;
    isSaving = true;
    try {
      // Delete files that were removed during the edit session
      for (const filePath of removedFilePaths) {
        try {
          await adapter.deleteAsset(filePath);
        } catch (err) {
          console.error("Failed to delete removed asset:", err);
        }
      }

      if (onSave) {
        await onSave(content, files);
      } else {
        if (!currentContextId) {
          console.error("Context ID required for new stash");
          return;
        }
        const stash: StashItem = {
          id: stashId, // Use pre-generated ID for consistency with file storage
          content,
          files: [], // Deprecated
          attachments: files.map((f) => ({
            ...f,
            id: f.id || crypto.randomUUID(),
            stashId: stashId,
          })),
          createdAt: new Date().toISOString(),
          contextId: currentContextId,
        };
        await adapter.saveStash(stash, { invertPosition });
        content = "";
        files = [];
        // Generate a new ID for the next stash to avoid overwriting the one we just saved
        stashId = crypto.randomUUID();
        onStash?.(stash.id);
      }

      // Clear tracking arrays after successful save
      addedFilePaths = [];
      removedFilePaths = [];
    } catch (e) {
      console.error(e);
    } finally {
      isSaving = false;
    }
  }

  /**
   * Handle cancel - clean up any files added during this edit session.
   */
  async function handleCancel() {
    // Delete files that were added during this edit session
    for (const filePath of addedFilePaths) {
      try {
        await adapter.deleteAsset(filePath);
      } catch (err) {
        console.error("Failed to delete added asset on cancel:", err);
      }
    }

    // Clear tracking arrays
    addedFilePaths = [];
    removedFilePaths = [];

    // Call the original onCancel callback
    onCancel?.();
  }

  /**
   * Handle clear button - shows confirmation unless shift is pressed.
   */
  function handleClear(e: MouseEvent) {
    const skipConfirm = e.shiftKey;

    // Only show clear if there's something to clear
    if (!content.trim() && files.length === 0) return;

    if (skipConfirm) {
      doClear();
    } else {
      clearConfirmDialogOpen = true;
    }
  }

  /**
   * Actually clear the editor content and attachments.
   */
  async function doClear() {
    // Delete all files that were added during this session
    for (const filePath of addedFilePaths) {
      try {
        await adapter.deleteAsset(filePath);
      } catch (err) {
        console.error("Failed to delete asset on clear:", err);
      }
    }

    // Clear the editor
    content = "";
    files = [];
    addedFilePaths = [];
    removedFilePaths = [];
    clearConfirmDialogOpen = false;
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

    // On macOS, use Cmd+Enter (metaKey); on Windows/Linux, use Ctrl+Enter (ctrlKey)
    const isMac = navigator.platform.includes("Mac");
    const isSaveShortcut = isMac ? e.metaKey : e.ctrlKey;
    if (e.key === "Enter" && isSaveShortcut) {
      e.preventDefault();
      save(e);
    }
    if (e.key === "Escape" && onCancel) {
      handleCancel();
    }
  }

  /**
   * Remove a file from the list.
   * If the file was originally part of the stash, track it for deletion on save.
   * If it was added during this session, remove it from the added list.
   */
  function removeFile(index: number) {
    const attachment = files[index];
    files = files.filter((_, i) => i !== index);

    // Check if this file was added during this edit session
    const addedIndex = addedFilePaths.indexOf(attachment.filePath);
    if (addedIndex !== -1) {
      // File was added during this session, just remove from tracking
      addedFilePaths = addedFilePaths.filter((_, i) => i !== addedIndex);
    } else {
      // File was part of the original stash, track for deletion on save
      removedFilePaths = [...removedFilePaths, attachment.filePath];
    }
  }

  /**
   * Determine file type icon based on extension.
   */
  function getFileIcon(fileName: string) {
    const ext = fileName.split(".").pop()?.toLowerCase() || "";
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

  let tooltipX = $state(0);
  let tooltipY = $state(0);
  let xOffset = $state(0);
  let showBelow = $state(false);

  function updateTooltipPosition(target: HTMLElement) {
    const rect = target.getBoundingClientRect();

    const TOOLTIP_OFFSET = 8;
    const centerX = rect.left + rect.width / 2;
    const centerY = rect.top;

    // Calculate tooltip position
    tooltipX = centerX;

    // Determine if tooltip should show below or above
    if (centerY < window.innerHeight / 2) {
      // Show below if in top half of screen
      showBelow = true;
      tooltipY = rect.bottom + TOOLTIP_OFFSET;
    } else {
      // Show above if in bottom half of screen
      showBelow = false;
      tooltipY = rect.top - TOOLTIP_OFFSET;
    }

    // Calculate horizontal offset to keep tooltip centered on element
    const TOOLTIP_MAX_WIDTH = 280;
    const viewportPadding = 8;

    // Calculate if tooltip would overflow
    const tooltipLeft = centerX - TOOLTIP_MAX_WIDTH / 2;
    const tooltipRight = centerX + TOOLTIP_MAX_WIDTH / 2;

    if (tooltipLeft < viewportPadding) {
      // Would overflow left
      xOffset = tooltipLeft - viewportPadding;
    } else if (tooltipRight > window.innerWidth - viewportPadding) {
      // Would overflow right
      xOffset = tooltipRight - (window.innerWidth - viewportPadding);
    } else {
      xOffset = 0;
    }
  }

  /**
   * Handle mouse enter on a file badge - start hover preview.
   */
  function handleFileMouseEnter(
    index: number,
    filePath: string,
    e: MouseEvent,
  ) {
    const target = e.currentTarget as HTMLElement;
    updateTooltipPosition(target);

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
            const attachment = await adapter.saveAssetFromPath(
              path,
              currentContextId,
              stashId,
            );
            files = [...files, attachment];
            // Track added file for cleanup on cancel
            addedFilePaths = [...addedFilePaths, attachment.filePath];
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
      use:tooltip
      tabindex="-1"
    >
      <Bold size={14} />
    </button>
    <button
      class="p-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-all"
      onclick={() => insertMarkdown("_", "_")}
      title={$_("editor.italic")}
      use:tooltip
      tabindex="-1"
    >
      <Italic size={14} />
    </button>
    <button
      class="p-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-all"
      onclick={() => insertMarkdown("### ")}
      title={$_("editor.heading")}
      use:tooltip
      tabindex="-1"
    >
      <Heading size={14} />
    </button>
    <div class="w-px h-4 bg-border/50 mx-1"></div>
    <button
      class="p-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-all"
      onclick={() => insertMarkdown("- ")}
      title={$_("editor.list")}
      use:tooltip
      tabindex="-1"
    >
      <List size={14} />
    </button>
    <button
      class="p-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-all"
      onclick={() => insertMarkdown("`", "`")}
      title={$_("editor.code")}
      use:tooltip
      tabindex="-1"
    >
      <Code size={14} />
    </button>
    <button
      class="p-1.5 rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-all"
      onclick={() => insertMarkdown("[", "](url)")}
      title={$_("editor.link")}
      use:tooltip
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
    onpaste={handlePaste}
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
    class="flex items-start justify-between p-2 border-t border-border bg-muted/30 rounded-b-xl"
  >
    <div class="flex gap-2 items-start flex-1 min-w-0">
      <!-- Add file button -->
      <button
        class="shrink-0 flex items-center gap-1 bg-background px-2 py-0.5 rounded text-[10px] border border-border text-muted-foreground hover:text-foreground hover:border-primary/50 transition-colors shadow-sm"
        onclick={handleAddFile}
        title={$_("editor.addFile")}
        use:tooltip
        type="button"
      >
        <Paperclip size={10} />
        <span>{$_("editor.addFile")}</span>
      </button>

      <div class="flex gap-2 flex-wrap items-center">
        {#each files as file, i}
          <div
            class="relative group/file bg-background px-2 py-0.5 rounded text-[10px] border border-border flex items-center gap-1 shadow-sm shrink-0 cursor-pointer hover:border-primary/50 transition-colors"
            title={$_("filePreview.clickToPreview")}
            onmouseenter={(e) => handleFileMouseEnter(i, file.filePath, e)}
            onmouseleave={handleFileMouseLeave}
            onclick={() => openFilePreview(file.filePath)}
            onkeydown={(e) =>
              e.key === "Enter" && openFilePreview(file.filePath)}
            role="button"
            tabindex="0"
          >
            <!-- File type icon -->
            {#if getFileIcon(file.fileName) === "image"}
              <Image size={10} class="shrink-0 text-muted-foreground" />
            {:else if getFileIcon(file.fileName) === "video"}
              <Video size={10} class="shrink-0 text-muted-foreground" />
            {:else if getFileIcon(file.fileName) === "text"}
              <FileText size={10} class="shrink-0 text-muted-foreground" />
            {:else}
              <FileIcon size={10} class="shrink-0 text-muted-foreground" />
            {/if}
            <span class="truncate max-w-[100px]">{file.fileName}</span>
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
            <Tooltip
              visible={hoveringFileIndex === i}
              x={tooltipX}
              y={tooltipY}
              position={showBelow ? "bottom" : "top"}
              {xOffset}
            >
              {#snippet children()}
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
                        <FileIcon size={20} class="text-muted-foreground" />
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
              {/snippet}
            </Tooltip>
          </div>
        {/each}
        {#if files.length === 0}
          <span class="text-[10px] text-muted-foreground/60 italic"
            >{$_("editor.dragFilesHere")}</span
          >
        {/if}
      </div>
    </div>

    <div class="flex gap-2">
      {#if onCancel}
        <button
          class="bg-muted text-muted-foreground hover:bg-muted/80 px-3 py-1.5 rounded-md font-medium text-xs transition-colors shadow-sm"
          onclick={handleCancel}
        >
          {$_("common.cancel")}
        </button>
      {:else}
        <button
          class="bg-muted text-muted-foreground hover:bg-muted/80 px-3 py-1.5 rounded-md font-medium text-xs transition-colors shadow-sm disabled:opacity-50"
          onclick={handleClear}
          disabled={!content.trim() && files.length === 0}
          title={$_("contexts.shiftClickToSkip")}
          use:tooltip
        >
          {$_("editor.clear")}
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

<!-- Paste Choice Dialog (when threshold is 0) -->
<ConfirmationDialog
  bind:open={pasteChoiceDialogOpen}
  title={$_("editor.pasteChoice.title")}
  description={$_("editor.pasteChoice.description")}
  confirmText={$_("editor.pasteChoice.attachment")}
  cancelText={$_("editor.pasteChoice.inline")}
  onConfirm={handlePasteConfirm}
  onCancel={handlePasteCancel}
/>

<!-- Clear Confirmation Dialog -->
<ConfirmationDialog
  bind:open={clearConfirmDialogOpen}
  title={$_("editor.clearConfirm.title")}
  description={$_("editor.clearConfirm.description")}
  confirmText={$_("common.clear")}
  onConfirm={doClear}
  onCancel={() => (clearConfirmDialogOpen = false)}
/>
