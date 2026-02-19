<!--
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Nico Wiedemann
-->

<script lang="ts">
    import { _ } from "$lib/i18n";
    import { Dialog } from "bits-ui";
    import { open as openFile } from "@tauri-apps/plugin-dialog";
    import { readFile, readTextFile } from "@tauri-apps/plugin-fs";
    import { getCurrentWebview } from "@tauri-apps/api/webview";
    import { onDestroy } from "svelte";
    import JSZip from "jszip";
    import type {
        Context,
        StashItem,
        Attachment,
        ContextRule,
    } from "$lib/types";
    import { DesktopStorageAdapter } from "$lib/services/desktop-adapter";
    import { getRelativeTime } from "$lib/utils/date";
    import {
        Upload,
        FileText,
        FileArchive,
        Square,
        CheckSquare,
        AlertTriangle,
        FolderOpen,
    } from "lucide-svelte";
    import { tooltip } from "$lib/actions/tooltip";
    import { load } from "js-yaml";

    let {
        open = $bindable(false),
        context,
        existingStashes,
        onClose,
        onImportComplete,
    } = $props<{
        open: boolean;
        context: Context;
        existingStashes: StashItem[];
        onClose: () => void;
        onImportComplete: () => void;
    }>();

    interface Metadata {
        name?: string;
        description?: string;
        rules?: ContextRule[];
    }

    // State for import workflow
    let step = $state<"select" | "preview">("select");
    let parsedStashes = $state<StashItem[]>([]);
    let selectedIds = $state<Set<string>>(new Set());
    let duplicateIds = $state<Set<string>>(new Set());
    let attachmentFiles = $state<Map<string, Uint8Array>>(new Map());
    let isImporting = $state(false);
    let isParsing = $state(false);
    let importedFileName = $state("");
    let isDragging = $state(false);

    // Metadata conflict state
    let importedMetadata = $state<Metadata | null>(null);
    let metadataConflict = $state<{
        name: boolean;
        description: boolean;
        rules: boolean;
    }>({ name: false, description: false, rules: false });
    let conflictDialogOpen = $state(false);
    let conflictChoices = $state<Record<string, "current" | "imported">>({
        name: "current",
        description: "current",
        rules: "current",
    });

    const adapter = new DesktopStorageAdapter();
    let unlistenDrop: (() => void) | null = null;

    // Reset state when dialog opens
    $effect(() => {
        if (open) {
            step = "select";
            parsedStashes = [];
            selectedIds = new Set();
            duplicateIds = new Set();
            attachmentFiles = new Map();
            isImporting = false;
            isParsing = false;
            importedFileName = "";
            isDragging = false;
            importedMetadata = null;
            conflictDialogOpen = false;
        }
    });

    // Setup drag-drop listener when in select step
    $effect(() => {
        // Only setup listener when dialog is open and in select step
        if (!open || step !== "select") {
            if (unlistenDrop) {
                unlistenDrop();
                unlistenDrop = null;
            }
            return;
        }

        // Setup Tauri file drop listener
        getCurrentWebview()
            .onDragDropEvent(async (event) => {
                if (event.payload.type === "drop") {
                    const paths = event.payload.paths;
                    if (paths && paths.length > 0) {
                        const filePath = paths[0];
                        const ext = filePath.toLowerCase().split(".").pop();
                        if (ext === "md" || ext === "zip") {
                            await loadFromPath(filePath);
                        }
                    }
                    isDragging = false;
                } else if (
                    event.payload.type === "enter" ||
                    event.payload.type === "over"
                ) {
                    isDragging = true;
                } else if (event.payload.type === "leave") {
                    isDragging = false;
                }
            })
            .then((unlisten) => {
                unlistenDrop = unlisten;
            });

        // Return cleanup function
        return () => {
            if (unlistenDrop) {
                unlistenDrop();
                unlistenDrop = null;
            }
        };
    });

    // Cleanup on destroy
    onDestroy(() => {
        if (unlistenDrop) {
            unlistenDrop();
        }
    });

    // Derived stats
    let activeStashes = $derived(
        parsedStashes
            .filter((s) => !s.completed)
            .sort(
                (a, b) =>
                    new Date(b.createdAt).getTime() -
                    new Date(a.createdAt).getTime(),
            ),
    );
    let completedStashes = $derived(
        parsedStashes
            .filter((s) => s.completed)
            .sort(
                (a, b) =>
                    new Date(b.createdAt).getTime() -
                    new Date(a.createdAt).getTime(),
            ),
    );
    let selectedStashes = $derived(
        parsedStashes.filter((s) => selectedIds.has(s.id)),
    );
    let duplicateCount = $derived(
        [...selectedIds].filter((id) => duplicateIds.has(id)).length,
    );

    /**
     * Open file picker and load file
     */
    async function selectFile() {
        const filePath = await openFile({
            title: $_("contexts.importDialog.selectFile"),
            filters: [
                {
                    name: "Stashpad Export",
                    extensions: ["md", "zip"],
                },
            ],
        });

        if (!filePath) return;

        await loadFromPath(filePath as string);
    }

    /**
     * Load and parse a file from a path
     */
    async function loadFromPath(filePath: string) {
        isParsing = true;
        try {
            const fileName = filePath.split(/[\\/]/).pop() || "";
            importedFileName = fileName;

            let metadata: Metadata | undefined;

            if (fileName.endsWith(".zip")) {
                metadata = await parseZipFile(filePath);
            } else {
                metadata = await parseMarkdownFile(filePath);
            }

            // Check for metadata conflicts
            if (metadata) {
                importedMetadata = metadata;
                const hasConflicts = detectMetadataConflicts(metadata);

                if (hasConflicts) {
                    conflictDialogOpen = true;
                    // We pause here. The dialog will handle the rest.
                    // But we also need to detect duplicates for the preview background
                    detectDuplicates();
                    // Select non-completed, non-duplicate stashes by default for preview
                    // (This will be visible behind the conflict dialog or after it closes)
                    selectedIds = new Set(
                        parsedStashes
                            .filter(
                                (s) => !s.completed && !duplicateIds.has(s.id),
                            )
                            .map((s) => s.id),
                    );
                    return; // Stop here, wait for user resolution
                }
            }

            // Detect duplicates (if no conflicts or conflicts auto-resolved?)
            detectDuplicates();

            // Select non-completed, non-duplicate stashes by default
            selectedIds = new Set(
                parsedStashes
                    .filter((s) => !s.completed && !duplicateIds.has(s.id))
                    .map((s) => s.id),
            );

            step = "preview";
        } catch (e) {
            console.error("Failed to parse file:", e);
        } finally {
            isParsing = false;
        }
    }

    /**
     * Detect conflicts between imported metadata and current context
     */
    function detectMetadataConflicts(metadata: Metadata): boolean {
        let hasConflict = false;
        const conflicts = { name: false, description: false, rules: false };

        // Check name
        if (metadata.name && metadata.name !== context.name) {
            conflicts.name = true;
            hasConflict = true;
        }

        // Check description (treat falsy as empty string for comparison)
        const currentDesc = context.description || "";
        const importedDesc = metadata.description || "";
        if (importedDesc !== currentDesc) {
            conflicts.description = true;
            hasConflict = true;
        }

        // Check rules
        // For now, simple JSON stringify comparison.
        // Ideally we should sort rules but order might matter.
        // Let's assume order matters.
        const currentRules = JSON.stringify(context.rules || []);
        const importedRules = JSON.stringify(metadata.rules || []);
        if (importedRules !== currentRules) {
            conflicts.rules = true;
            hasConflict = true;
        }

        metadataConflict = conflicts;
        return hasConflict;
    }
    /**
     * Resolve conflicts and proceed to preview
     */
    function resolveConflictAndImport() {
        if (!importedMetadata) return;

        // Apply choices to a temporary resolved state or directly modify context if we want to preview it?
        // We will store the pending context updates and apply them during the final import save.
        // For now, let's just update the context object in memory so the UI reflects it (e.g. title)
        // and we will save it to persistence in handleImport.

        if (conflictChoices.name === "imported" && importedMetadata.name) {
            context.name = importedMetadata.name;
        }
        if (
            conflictChoices.description === "imported" &&
            importedMetadata.description
        ) {
            context.description = importedMetadata.description;
        }
        if (conflictChoices.rules === "imported" && importedMetadata.rules) {
            context.rules = importedMetadata.rules;
        }

        conflictDialogOpen = false;
        step = "preview";
    }

    /**
     * Parse a markdown file exported from Stashpad
     */
    async function parseMarkdownFile(filePath: string) {
        const content = await readTextFile(filePath);
        const result = parseMarkdownContent(content);
        parsedStashes = result.stashes;
        return result.metadata;
    }

    /**
     * Parse a ZIP file exported from Stashpad
     */
    async function parseZipFile(filePath: string) {
        const zipData = await readFile(filePath);
        const zip = await JSZip.loadAsync(zipData);

        // Find and parse export.md
        const mdFile = zip.file("export.md");
        if (mdFile) {
            const content = await mdFile.async("text");
            const result = parseMarkdownContent(content);
            parsedStashes = result.stashes;

            // Load attachments
            const attachmentsFolder = zip.folder("attachments");
            if (attachmentsFolder) {
                const files = attachmentsFolder.file(/.*/);
                for (const file of files) {
                    if (!file.dir) {
                        const data = await file.async("uint8array");
                        let name = file.name.split("/").pop() || file.name;

                        // Strip stash ID prefix if present (format: 12345678_filename.ext)
                        const prefixMatch = name.match(/^[a-f0-9]{8}_(.+)$/);
                        if (prefixMatch) {
                            name = prefixMatch[1]; // Use the original filename without prefix
                        }

                        attachmentFiles.set(name, data);
                    }
                }
                attachmentFiles = new Map(attachmentFiles); // Trigger reactivity
            }
            return result.metadata;
        }
        return undefined;
    }

    /**
     * Parse markdown content into stash items and metadata
     */
    function parseMarkdownContent(content: string): {
        stashes: StashItem[];
        metadata?: Metadata;
    } {
        const stashes: StashItem[] = [];
        let metadata: Metadata | undefined;
        let contentToParse = content;

        // Parse YAML frontmatter
        if (content.startsWith("---")) {
            const endFrontmatter = content.indexOf("\n---", 3);
            if (endFrontmatter !== -1) {
                const frontmatter = content.slice(4, endFrontmatter);
                try {
                    const parsed = load(frontmatter) as any;
                    if (parsed && typeof parsed === "object") {
                        metadata = {
                            name: parsed.name,
                            description: parsed.description,
                            rules: parsed.rules,
                        };
                    }
                } catch (e) {
                    console.error("Failed to parse frontmatter:", e);
                }
                // content starts after the closing --- \n
                contentToParse = content.slice(endFrontmatter + 5);
            }
        }

        const lines = contentToParse.split("\n");

        let currentStash: Partial<StashItem> | null = null;
        let currentContent: string[] = [];
        let currentFiles: string[] = [];
        let inAttachments = false;
        let currentSectionCompleted = false; // Track whether we're in completed section

        for (let i = 0; i < lines.length; i++) {
            const line = lines[i];

            // Detect new format section headers: ## Active Stashes (N) or ## Completed Stashes (N)
            const sectionMatch = line.match(
                /^## (Active|Completed) Stashes \(\d+\)$/,
            );
            if (sectionMatch) {
                // Save previous stash before changing sections
                if (currentStash) {
                    stashes.push(
                        finalizeStash(
                            currentStash,
                            currentContent,
                            currentFiles,
                        ),
                    );
                    currentStash = null;
                    currentContent = [];
                    currentFiles = [];
                }
                currentSectionCompleted = sectionMatch[1] === "Completed";
                inAttachments = false;
                continue;
            }

            // Detect new format stash header: ### Date
            const newHeaderMatch = line.match(/^### (.+)$/);
            if (newHeaderMatch) {
                // Save previous stash
                if (currentStash) {
                    stashes.push(
                        finalizeStash(
                            currentStash,
                            currentContent,
                            currentFiles,
                        ),
                    );
                }

                // Start new stash
                const dateStr = newHeaderMatch[1];
                currentStash = {
                    id: crypto.randomUUID(),
                    completed: currentSectionCompleted,
                    createdAt: parseDate(dateStr),
                    contextId: context.id,
                };
                currentContent = [];
                currentFiles = [];
                inAttachments = false;
                continue;
            }

            // Skip if no current stash
            if (!currentStash) continue;

            // Detect attachments section
            if (line.startsWith("**Attachments:**")) {
                inAttachments = true;
                continue;
            }

            // Detect separator
            if (line === "---") {
                inAttachments = false;
                continue;
            }

            // Parse attachment lines
            if (inAttachments && line.startsWith("- ")) {
                const attachMatch = line.match(
                    /^- \[(.+)\]\(attachments\/(.+)\)$/,
                );
                if (attachMatch) {
                    let fileName = attachMatch[2];

                    // Strip stash ID prefix if present (format: 12345678_filename.ext)
                    const prefixMatch = fileName.match(/^[a-f0-9]{8}_(.+)$/);
                    if (prefixMatch) {
                        fileName = prefixMatch[1]; // Use the original filename without prefix
                    }

                    currentFiles.push(fileName);
                } else {
                    // Plain attachment reference: - filename.ext
                    const plainMatch = line.match(/^- (.+)$/);
                    if (plainMatch) {
                        let fileName = plainMatch[1];

                        // Strip stash ID prefix if present (format: 12345678_filename.ext)
                        const prefixMatch =
                            fileName.match(/^[a-f0-9]{8}_(.+)$/);
                        if (prefixMatch) {
                            fileName = prefixMatch[1]; // Use the original filename without prefix
                        }

                        currentFiles.push(fileName);
                    }
                }
                continue;
            }

            // Regular content line
            if (!inAttachments && line.trim() !== "") {
                currentContent.push(line);
            }
        }

        // Don't forget the last stash
        if (currentStash) {
            stashes.push(
                finalizeStash(currentStash, currentContent, currentFiles),
            );
        }

        return { stashes, metadata };
    }

    /**
     * Finalize a stash object from parsed data
     */
    function finalizeStash(
        partial: Partial<StashItem>,
        contentLines: string[],
        files: string[],
    ): StashItem {
        return {
            id: partial.id || crypto.randomUUID(),
            content: contentLines.join("\n").trim(),
            files: files,
            attachments: [], // Will be populated during import
            createdAt: partial.createdAt || new Date().toISOString(),
            contextId: partial.contextId || context.id,
            completed: partial.completed || false,
            completedAt: partial.completed
                ? new Date().toISOString()
                : undefined,
        };
    }

    /**
     * Parse date string from export format
     */
    function parseDate(dateStr: string): string {
        try {
            const date = new Date(dateStr);
            if (!isNaN(date.getTime())) {
                return date.toISOString();
            }
        } catch {
            // Fall through to default
        }
        return new Date().toISOString();
    }

    /**
     * Detect duplicate stashes by comparing content
     */
    function detectDuplicates() {
        const dupes = new Set<string>();

        for (const parsed of parsedStashes) {
            const normalizedParsed = normalizeContent(parsed.content);
            if (!normalizedParsed) continue;

            for (const existing of existingStashes) {
                const normalizedExisting = normalizeContent(existing.content);
                if (!normalizedExisting) continue;

                // Check for exact match or high similarity
                if (
                    normalizedParsed === normalizedExisting ||
                    calculateSimilarity(normalizedParsed, normalizedExisting) >
                        0.8
                ) {
                    dupes.add(parsed.id);
                    break;
                }
            }
        }

        duplicateIds = dupes;
    }

    /**
     * Normalize content for comparison
     */
    function normalizeContent(content: string): string {
        return content.trim().toLowerCase().replace(/\s+/g, " ");
    }

    /**
     * Simple similarity calculation (Jaccard index on words)
     */
    function calculateSimilarity(a: string, b: string): number {
        const wordsA = new Set(a.split(/\s+/));
        const wordsB = new Set(b.split(/\s+/));

        const intersection = [...wordsA].filter((w) => wordsB.has(w)).length;
        const union = new Set([...wordsA, ...wordsB]).size;

        return union > 0 ? intersection / union : 0;
    }

    /**
     * Toggle selection for a single stash
     */
    function toggleStash(id: string) {
        if (selectedIds.has(id)) {
            selectedIds.delete(id);
        } else {
            selectedIds.add(id);
        }
        selectedIds = new Set(selectedIds);
    }

    /**
     * Select all stashes
     */
    function selectAll() {
        selectedIds = new Set(parsedStashes.map((s) => s.id));
    }

    /**
     * Deselect all stashes
     */
    function deselectAll() {
        selectedIds = new Set();
    }

    /**
     * Import selected stashes
     */
    async function handleImport() {
        if (selectedIds.size === 0) return;

        isImporting = true;
        try {
            // Save context metadata if it was updated during conflict resolution
            if (importedMetadata) {
                // Check if context was modified in memory (by resolveConflictAndImport)
                // We can just save the current context state, as resolveConflictAndImport
                // already updated the in-memory context object.
                await adapter.saveContext(context);
            }

            for (const stash of selectedStashes) {
                // First, save the stash to the database (without attachments)
                const stashToSave: StashItem = {
                    ...stash,
                    files: [], // Clear legacy files array
                    attachments: [], // Will be populated after files are saved
                    contextId: context.id,
                };

                await adapter.saveStash(stashToSave);

                // Now that the stash exists in the DB, save attachments
                const attachments: Attachment[] = [];
                for (const fileName of stash.files || []) {
                    const fileData = attachmentFiles.get(fileName);
                    if (fileData) {
                        try {
                            // Create a File object from Uint8Array and save via adapter
                            const blob = new Blob([
                                fileData.buffer.slice(
                                    fileData.byteOffset,
                                    fileData.byteOffset + fileData.byteLength,
                                ) as ArrayBuffer,
                            ]);
                            const file = new File([blob], fileName);
                            const savedAttachment = await adapter.saveAsset(
                                file,
                                context.id,
                                stash.id,
                            );
                            // saveAsset returns an Attachment object
                            attachments.push(savedAttachment);
                        } catch (err) {
                            console.error(
                                `Failed to save attachment ${fileName}:`,
                                err,
                            );
                        }
                    }
                }

                // Update the stash with the attachments
                if (attachments.length > 0) {
                    const updatedStash: StashItem = {
                        ...stashToSave,
                        attachments,
                    };
                    await adapter.saveStash(updatedStash);
                }
            }

            onImportComplete();
            handleClose();
        } catch (e) {
            console.error("Import failed:", e);
        } finally {
            isImporting = false;
        }
    }

    /**
     * Handle dialog close
     */
    function handleClose() {
        open = false;
        onClose();
    }

    /**
     * Get preview text for a stash (truncated)
     */
    function getPreviewText(stash: StashItem): string {
        const text = stash.content.trim();
        if (!text) return $_("stashCard.emptyStash");
        if (text.length > 60) return text.slice(0, 60) + "…";
        return text;
    }
</script>

<Dialog.Root bind:open onOpenChange={(v) => (open = v)}>
    <Dialog.Portal>
        <Dialog.Overlay
            class="fixed inset-0 z-[100] bg-black/50 backdrop-blur-sm animate-in fade-in-0"
        />
        <Dialog.Content
            class="fixed left-[50%] top-[50%] z-[100] w-full max-w-2xl translate-x-[-50%] translate-y-[-50%] outline-none max-h-[85vh] flex flex-col animate-in zoom-in-95 fade-in-0 duration-200"
        >
            <div
                class="bg-popover text-popover-foreground border-border border shadow-lg rounded-lg flex flex-col overflow-hidden"
            >
                <!-- Header -->
                <div class="px-4 py-3 border-b border-border shrink-0">
                    <Dialog.Title
                        class="text-base font-semibold block tracking-tight"
                    >
                        {$_("contexts.importDialog.title")}: {context.name}
                    </Dialog.Title>
                    <Dialog.Description
                        class="text-xs text-muted-foreground mt-0.5"
                    >
                        {step === "select"
                            ? $_("contexts.importDialog.selectFileDesc")
                            : $_("contexts.importDialog.selectStashes")}
                    </Dialog.Description>
                </div>

                {#if step === "select"}
                    <!-- File Selection Step -->
                    <div
                        class="p-8 flex flex-col items-center gap-4 border-2 border-dashed rounded-lg m-4 transition-colors
                            {isDragging
                            ? 'border-primary bg-primary/5'
                            : 'border-muted'}"
                        role="region"
                        aria-label="Drop zone"
                    >
                        <div
                            class="w-16 h-16 rounded-full bg-muted flex items-center justify-center"
                        >
                            <FolderOpen
                                size={32}
                                class="text-muted-foreground"
                            />
                        </div>
                        <p class="text-sm text-muted-foreground text-center">
                            {$_("contexts.importDialog.supportedFormats")}
                        </p>
                        <button
                            type="button"
                            class="bg-primary text-primary-foreground hover:bg-primary/90 px-4 py-2 text-sm font-medium rounded-md transition-colors flex items-center gap-2"
                            onclick={selectFile}
                            disabled={isParsing}
                        >
                            <Upload size={16} />
                            {isParsing
                                ? $_("contexts.importDialog.parsing")
                                : $_("contexts.importDialog.chooseFile")}
                        </button>
                        <p class="text-xs text-muted-foreground">
                            {$_("contexts.importDialog.dropToImport")}
                        </p>
                    </div>
                {:else}
                    <!-- Preview Step -->
                    <div class="flex-1 overflow-y-auto px-4 py-2 max-h-[50vh]">
                        {#if duplicateCount > 0}
                            <div
                                class="flex items-center gap-2 p-2 rounded-lg bg-amber-500/10 border border-amber-500/30 text-amber-600 dark:text-amber-400 text-xs mb-3"
                            >
                                <AlertTriangle size={14} />
                                <span>
                                    {$_(
                                        "contexts.importDialog.duplicatesFound",
                                        {
                                            values: { count: duplicateCount },
                                        },
                                    )}
                                </span>
                            </div>
                        {/if}

                        <!-- Active Stashes -->
                        {#if activeStashes.length > 0}
                            <div class="mb-3">
                                <div
                                    class="text-[10px] uppercase tracking-wider text-muted-foreground font-medium mb-1.5 px-1"
                                >
                                    {$_("queue.active")} ({activeStashes.length})
                                </div>
                                <div class="space-y-0.5">
                                    {#each activeStashes as stash (stash.id)}
                                        <button
                                            type="button"
                                            class="w-full flex items-center gap-2 px-2 py-1.5 rounded transition-colors text-left
                                                {selectedIds.has(stash.id)
                                                ? 'bg-primary/10'
                                                : 'hover:bg-muted/50'}"
                                            onclick={() =>
                                                toggleStash(stash.id)}
                                        >
                                            <div class="shrink-0">
                                                {#if selectedIds.has(stash.id)}
                                                    <CheckSquare
                                                        size={14}
                                                        class="text-primary"
                                                    />
                                                {:else}
                                                    <Square
                                                        size={14}
                                                        class="text-muted-foreground"
                                                    />
                                                {/if}
                                            </div>
                                            <span
                                                class="flex-1 text-sm truncate"
                                                >{getPreviewText(stash)}</span
                                            >
                                            <span
                                                class="text-[10px] text-muted-foreground shrink-0"
                                                >{getRelativeTime(
                                                    stash.createdAt,
                                                    $_,
                                                )}</span
                                            >
                                            {#if duplicateIds.has(stash.id)}
                                                <span
                                                    class="text-[10px] text-amber-500 shrink-0 cursor-help"
                                                    title={$_(
                                                        "contexts.importDialog.duplicateTooltip",
                                                    )}
                                                    use:tooltip>⚠️</span
                                                >
                                            {/if}
                                            {#if stash.files && stash.files.length > 0}
                                                <span
                                                    class="text-[10px] text-muted-foreground shrink-0"
                                                    >📎{stash.files
                                                        .length}</span
                                                >
                                            {/if}
                                        </button>
                                    {/each}
                                </div>
                            </div>
                        {/if}

                        <!-- Completed Stashes -->
                        {#if completedStashes.length > 0}
                            <div>
                                <div
                                    class="text-[10px] uppercase tracking-wider text-muted-foreground font-medium mb-1.5 px-1"
                                >
                                    {$_("queue.completed")} ({completedStashes.length})
                                </div>
                                <div class="space-y-0.5">
                                    {#each completedStashes as stash (stash.id)}
                                        <button
                                            type="button"
                                            class="w-full flex items-center gap-2 px-2 py-1.5 rounded transition-colors text-left opacity-60
                                                {selectedIds.has(stash.id)
                                                ? 'bg-primary/10'
                                                : 'hover:bg-muted/50'}"
                                            onclick={() =>
                                                toggleStash(stash.id)}
                                        >
                                            <div class="shrink-0">
                                                {#if selectedIds.has(stash.id)}
                                                    <CheckSquare
                                                        size={14}
                                                        class="text-primary"
                                                    />
                                                {:else}
                                                    <Square
                                                        size={14}
                                                        class="text-muted-foreground"
                                                    />
                                                {/if}
                                            </div>
                                            <span
                                                class="flex-1 text-sm truncate line-through"
                                                >{getPreviewText(stash)}</span
                                            >
                                            <span
                                                class="text-[10px] text-muted-foreground shrink-0"
                                                >{getRelativeTime(
                                                    stash.createdAt,
                                                    $_,
                                                )}</span
                                            >
                                            {#if duplicateIds.has(stash.id)}
                                                <span
                                                    class="text-[10px] text-amber-500 shrink-0 cursor-help"
                                                    title={$_(
                                                        "contexts.importDialog.duplicateTooltip",
                                                    )}
                                                    use:tooltip>⚠️</span
                                                >
                                            {/if}
                                            {#if stash.files && stash.files.length > 0}
                                                <span
                                                    class="text-[10px] text-muted-foreground shrink-0"
                                                    >📎{stash.files
                                                        .length}</span
                                                >
                                            {/if}
                                        </button>
                                    {/each}
                                </div>
                            </div>
                        {/if}

                        {#if parsedStashes.length === 0}
                            <div
                                class="text-center py-8 text-muted-foreground text-sm"
                            >
                                {$_("contexts.importDialog.noStashesFound")}
                            </div>
                        {/if}
                    </div>

                    <!-- Footer -->
                    <div class="p-4 border-t border-border space-y-4 shrink-0">
                        <!-- Selection controls -->
                        <div class="flex items-center justify-between text-xs">
                            <div class="flex gap-2">
                                <button
                                    type="button"
                                    class="text-primary hover:underline"
                                    onclick={selectAll}
                                >
                                    {$_("contexts.exportDialog.selectAll")}
                                </button>
                                <span class="text-muted-foreground">|</span>
                                <button
                                    type="button"
                                    class="text-primary hover:underline"
                                    onclick={deselectAll}
                                >
                                    {$_("contexts.exportDialog.deselectAll")}
                                </button>
                            </div>
                            <div class="text-muted-foreground">
                                {selectedIds.size} / {parsedStashes.length}
                            </div>
                        </div>

                        <!-- File info -->
                        <div
                            class="flex items-center gap-2 text-xs text-muted-foreground"
                        >
                            {#if importedFileName.endsWith(".zip")}
                                <FileArchive size={14} />
                            {:else}
                                <FileText size={14} />
                            {/if}
                            <span class="truncate">{importedFileName}</span>
                            {#if attachmentFiles.size > 0}
                                <span class="text-muted-foreground">
                                    ({attachmentFiles.size} attachments)
                                </span>
                            {/if}
                        </div>

                        <!-- Action buttons -->
                        <div class="flex justify-end gap-2">
                            <button
                                type="button"
                                class="px-3 py-2 text-sm font-medium hover:bg-muted rounded-md transition-colors"
                                onclick={handleClose}
                            >
                                {$_("common.cancel")}
                            </button>
                            <button
                                type="button"
                                class="bg-primary text-primary-foreground hover:bg-primary/90 px-4 py-2 text-sm font-medium rounded-md transition-colors flex items-center gap-2 disabled:opacity-50 disabled:cursor-not-allowed"
                                onclick={handleImport}
                                disabled={selectedIds.size === 0 || isImporting}
                            >
                                <Upload size={16} />
                                {isImporting
                                    ? $_("common.loading")
                                    : $_("contexts.importDialog.import")}
                            </button>
                        </div>
                    </div>
                {/if}
            </div>
        </Dialog.Content>
    </Dialog.Portal>
</Dialog.Root>

{#if conflictDialogOpen && importedMetadata}
    <Dialog.Root open={true} onOpenChange={() => (conflictDialogOpen = false)}>
        <Dialog.Portal>
            <Dialog.Overlay
                class="fixed inset-0 z-[200] bg-black/50 backdrop-blur-sm animate-in fade-in-0"
            />
            <Dialog.Content
                class="fixed left-[50%] top-[50%] z-[200] w-full max-w-2xl translate-x-[-50%] translate-y-[-50%] outline-none max-h-[85vh] flex flex-col animate-in zoom-in-95 fade-in-0 duration-200"
            >
                <div
                    class="bg-background text-foreground border-border border shadow-lg rounded-lg flex flex-col overflow-hidden"
                >
                    <div class="px-6 py-4 border-b border-border">
                        <Dialog.Title class="text-lg font-semibold"
                            >{$_(
                                "contexts.importDialog.conflictTitle",
                            )}</Dialog.Title
                        >
                        <Dialog.Description
                            class="text-sm text-muted-foreground"
                        >
                            {$_("contexts.importDialog.conflictDescription")}
                        </Dialog.Description>
                    </div>

                    <div class="p-6 space-y-6 overflow-y-auto">
                        <!-- Metadata Fields -->
                        {#each ["name", "description", "rules"] as field}
                            {#if metadataConflict[field as keyof Metadata]}
                                <div
                                    class="grid grid-cols-2 gap-4 items-stretch"
                                >
                                    <!-- Current Value -->
                                    <button
                                        class="p-4 rounded-lg border-2 text-left transition-all relative h-full flex flex-col {conflictChoices[
                                            field
                                        ] === 'current'
                                            ? 'border-primary bg-primary/5 ring-1 ring-primary/20'
                                            : 'border-border bg-muted/30 opacity-60 hover:opacity-80 hover:border-primary/50'}"
                                        onclick={() =>
                                            (conflictChoices[field] =
                                                "current")}
                                    >
                                        <div
                                            class="text-xs font-semibold text-muted-foreground mb-2 uppercase tracking-wider flex items-center justify-between"
                                        >
                                            {$_("common.current")}
                                            {#if conflictChoices[field] === "current"}
                                                <div
                                                    class="w-2 h-2 rounded-full bg-primary shadow-[0_0_8px_rgba(var(--primary),0.5)]"
                                                ></div>
                                            {/if}
                                        </div>
                                        <div
                                            class="font-medium text-sm break-words whitespace-pre-wrap"
                                        >
                                            {field === "rules"
                                                ? JSON.stringify(
                                                      context.rules,
                                                      null,
                                                      2,
                                                  )
                                                : context[
                                                      field as keyof Context
                                                  ] || "—"}
                                        </div>
                                    </button>

                                    <!-- Imported Value -->
                                    <button
                                        class="p-4 rounded-lg border-2 text-left transition-all relative h-full flex flex-col {conflictChoices[
                                            field
                                        ] === 'imported'
                                            ? 'border-primary bg-primary/5 ring-1 ring-primary/20'
                                            : 'border-border bg-muted/30 opacity-60 hover:opacity-80 hover:border-primary/50'}"
                                        onclick={() =>
                                            (conflictChoices[field] =
                                                "imported")}
                                    >
                                        <div
                                            class="text-xs font-semibold text-muted-foreground mb-2 uppercase tracking-wider flex items-center justify-between"
                                        >
                                            {$_("common.imported")}
                                            {#if conflictChoices[field] === "imported"}
                                                <div
                                                    class="w-2 h-2 rounded-full bg-primary shadow-[0_0_8px_rgba(var(--primary),0.5)]"
                                                ></div>
                                            {/if}
                                        </div>
                                        <div
                                            class="font-medium text-sm break-words whitespace-pre-wrap"
                                        >
                                            {field === "rules"
                                                ? JSON.stringify(
                                                      importedMetadata.rules,
                                                      null,
                                                      2,
                                                  )
                                                : importedMetadata[
                                                      field as keyof Metadata
                                                  ] || "—"}
                                        </div>
                                    </button>
                                </div>
                            {/if}
                        {/each}
                    </div>

                    <div
                        class="px-6 py-4 border-t border-border bg-muted/20 flex justify-end gap-2"
                    >
                        <button
                            class="px-4 py-2 text-sm font-medium hover:bg-muted rounded-md transition-colors"
                            onclick={() => (conflictDialogOpen = false)}
                        >
                            {$_("common.cancel")}
                        </button>
                        <button
                            class="bg-primary text-primary-foreground hover:bg-primary/90 px-4 py-2 text-sm font-medium rounded-md transition-colors"
                            onclick={resolveConflictAndImport}
                        >
                            {$_("contexts.importDialog.confirmImport")}
                        </button>
                    </div>
                </div>
            </Dialog.Content>
        </Dialog.Portal>
    </Dialog.Root>
{/if}
