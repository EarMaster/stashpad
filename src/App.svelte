<!--
// SPDX-License-Identifier: AGPL-3.0-only

// Copyright (C) 2025-2026 Nico Wiedemann
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
   import { CloudSyncService, type SyncStatus } from "$lib/services/cloud-sync";
   import { _ } from "$lib/i18n";
   import type { Settings, StashItem, Context, Attachment } from "$lib/types";
   import Header from "$lib/components/Header.svelte";
   import Editor from "$lib/components/Editor.svelte";
   import Queue from "$lib/components/Queue.svelte";
   import SettingsView from "$lib/components/Settings.svelte";
   import ContextManager from "$lib/components/ContextManager.svelte";
   import ContextSwitcher from "$lib/components/ContextSwitcher.svelte";
   import ConfirmationDialog from "$lib/components/ConfirmationDialog.svelte";
   import { onMount } from "svelte";
   import { fly } from "svelte/transition";
   import { Sparkles } from "lucide-svelte";
   import { getCurrentWindow } from "@tauri-apps/api/window";

   let transferMode = $state("Drag");
   let refreshTrigger = $state(0);
   let view = $state<"Main" | "Settings" | "Contexts">("Main");
   let navigationSource = $state<"Settings" | "Switcher">("Settings");
   let currentContextId = $state<string>("default");
   let movingStash = $state<StashItem | null>(null);
   let newlyAddedStashId = $state<string | null>(null);
   let allTags = $state<string[]>([]);
   let autoDetectedWindowTitle = $state<string | undefined>(undefined);

   // Draft state persistence
   let editorDraft = $state("");
   let editorFiles = $state<Attachment[]>([]);

   let showExitConfirmation = $state(false);
   let isWin10 = $state(false);

   // Cloud sync status
   let syncStatus = $state<SyncStatus>("idle");
   let showPromptReloadedToast = $state(false);

   const appWindow = getCurrentWindow();
   const adapter = new DesktopStorageAdapter();
   const cloudSync = new CloudSyncService(adapter);

   onMount(() => {
      adapter.isWindows10().then((v) => (isWin10 = v));
      const unlisten = appWindow.onCloseRequested(async (event) => {
         if (editorDraft.trim() || editorFiles.length > 0) {
            event.preventDefault();
            showExitConfirmation = true;
         }
      });

      // Periodic cleanup for "after-n-days" strategy (every 5 minutes)
      const cleanupInterval = setInterval(
         () => {
            if (settings.clearCompletedStrategy === "after-n-days") {
               adapter.triggerAutoCleanup().then(() => {
                  // Refresh the queue to reflect any deleted stashes
                  refreshTrigger++;
               });
            }
         },
         5 * 60 * 1000,
      ); // 5 minutes

      // Listen to sync status changes
      const unsubscribeSync = cloudSync.addListener((status) => {
         syncStatus = status;
         if (status === "success") {
            // Refresh queue after successful sync to show server changes
            refreshTrigger++;
         }
      });

      // Listen for prompt reload event
      const handlePromptReloaded = () => {
         showPromptReloadedToast = true;
         setTimeout(() => (showPromptReloadedToast = false), 3000);
      };
      window.addEventListener("stashpad:prompt-reloaded", handlePromptReloaded);

      return () => {
         unlisten.then((f) => f());
         clearInterval(cleanupInterval);
         unsubscribeSync();
         cloudSync.dispose();
         window.removeEventListener(
            "stashpad:prompt-reloaded",
            handlePromptReloaded,
         );
      };
   });

   // Centralized settings state
   let settings = $state<Settings>({
      autoContextDetection: true,
      visualEffectsEnabled: undefined,
      activeContextId: "default",
      shortcuts: {
         switch_context: "CommandOrControl+P",
      },
   });

   // Centralized contexts state
   let contexts = $state<Context[]>([]);

   let isGlass = $derived(
      settings.visualEffectsEnabled ??
         (isWin10
            ? false
            : !window.matchMedia("(prefers-reduced-transparency: reduce)")
                 .matches),
   );

   // Context Switching Logic
   let contextSelectorOpen = $state(false);
   let isCycling = $state(false); // Track if user is cycling through contexts
   let lastUsedContexts = $state<string[]>([]); // Future use
   let stashCounts = $state<Record<string, number>>({});

   // Load stash counts when context switcher opens
   $effect(() => {
      if (contextSelectorOpen) {
         adapter.loadStashes().then((stashes: StashItem[]) => {
            const counts: Record<string, number> = { default: 0 };
            contexts.forEach((ctx) => (counts[ctx.id] = 0));
            stashes.forEach((stash) => {
               if (!stash.completed) {
                  const ctxId = stash.contextId || "default";
                  counts[ctxId] = (counts[ctxId] || 0) + 1;
               }
            });
            stashCounts = counts;
         });
      }
   });

   function handleKeydown(e: KeyboardEvent) {
      // Check for switch_context shortcut
      // Parsing "CommandOrControl+P" is tricky natively in JS without a library,
      // but for this specific request we can hardcode the check or do basic parsing.
      // Ideally we'd use a robust hotkey library, but for now:
      const shortcut =
         settings.shortcuts["switch_context"] || "CommandOrControl+P";
      const keys = shortcut.toLowerCase().split("+");
      const isCtrl =
         keys.includes("control") ||
         keys.includes("commandorcontrol") ||
         keys.includes("ctrl");
      const isMeta =
         keys.includes("command") ||
         keys.includes("commandorcontrol") ||
         keys.includes("meta");
      const key = keys.find((k) => k.length === 1); // finding the char

      const pressedCtrl = e.ctrlKey || (isMeta && e.metaKey);

      if (pressedCtrl && e.key.toLowerCase() === key) {
         e.preventDefault();
         if (!contextSelectorOpen) {
            contextSelectorOpen = true;
            isCycling = false; // Reset cycling state on initial open
         } else {
            // Cycle selection
            selectNextContext();
            isCycling = true; // We are now cycling
         }
         return;
      }

      // Handle navigation keys when context switcher is open
      if (contextSelectorOpen) {
         if (e.key === "ArrowDown") {
            e.preventDefault();
            contextSwitcher?.next();
         } else if (e.key === "ArrowUp") {
            e.preventDefault();
            contextSwitcher?.prev();
         } else if (e.key === "Enter") {
            e.preventDefault();
            contextSwitcher?.confirm();
         } else if (e.key === "Escape") {
            e.preventDefault();
            contextSelectorOpen = false;
            isCycling = false;
         }
      }
   }

   function handleKeyup(e: KeyboardEvent) {
      if (!contextSelectorOpen) return;

      const shortcut =
         settings.shortcuts["switch_context"] || "CommandOrControl+P";
      const keys = shortcut.toLowerCase().split("+");
      const requiresCtrl =
         keys.includes("control") ||
         keys.includes("commandorcontrol") ||
         keys.includes("ctrl");
      const requiresMeta =
         keys.includes("command") ||
         keys.includes("commandorcontrol") ||
         keys.includes("meta");
      const requiresAlt = keys.includes("alt");

      // Check if the released key is the significant modifier
      if (
         (requiresCtrl && e.key === "Control") ||
         (requiresMeta && e.key === "Meta") ||
         (requiresAlt && e.key === "Alt")
      ) {
         // If we were cycling (i.e. pressed shortcut > 1 time), confirm on release
         if (isCycling) {
            confirmContextSelection();
            isCycling = false;
         }
      }
   }

   // Component instance binding - don't use $state for bind:this
   let contextSwitcher = $state<{
      next: () => void;
      prev: () => void;
      confirm: () => void;
   } | null>(null);

   function selectNextContext() {
      contextSwitcher?.next();
   }

   function confirmContextSelection() {
      contextSwitcher?.confirm();
   }

   function selectContext(ctxId: string, shiftKey = false) {
      if (movingStash) {
         handleMoveStash(movingStash, ctxId, shiftKey);
         movingStash = null;
      } else {
         settings.autoContextDetection = false;
         settings.activeContextId = ctxId;
         currentContextId = ctxId;

         // Update lastUsed timestamp for selected context
         const ctx = contexts.find((c) => c.id === ctxId);
         if (ctx) {
            ctx.lastUsed = new Date().toISOString();
            // Save context update
            adapter.saveContext(ctx);
         }

         adapter.saveSettings(settings);
      }
      contextSelectorOpen = false;
   }

   async function handleMoveStash(
      item: StashItem,
      targetId: string,
      keepCopy: boolean,
   ) {
      try {
         if (keepCopy) {
            const copy: StashItem = {
               ...item,
               id: crypto.randomUUID(),
               contextId: targetId,
               createdAt: new Date().toISOString(),
            };
            await adapter.saveStash(copy);
         } else {
            const updated: StashItem = {
               ...item,
               contextId: targetId,
               createdAt: item.createdAt, // Ensure created_at is preserved? Or update? Types say it is string.
               content: item.content,
               files: item.files || [],
               completed: item.completed,
               // ...item might override something if I am not careful.
            };
            // Ideally just ...item and overwrite contextId
            // But let's stick to safe spread
            const safeUpdated = { ...item, contextId: targetId };
            await adapter.saveStash(safeUpdated);
         }
         refreshTrigger++;
      } catch (e) {
         console.error("Failed to move/copy stash", e);
      }
   }

   function getAvailableContexts() {
      return [{ id: "default", name: "Default" }, ...contexts];
   }

   /**
    * Apply loaded settings to state after validation.
    * Extracted to reuse between initial load and async load.
    */
   function applySettings(loaded: Settings) {
      // Ensure defaults if missing
      if (!loaded.shortcuts) {
         loaded.shortcuts = {};
      }
      settings = loaded;

      const loadedId = loaded.activeContextId;
      // Validate context exists (or is default)
      const exists =
         loadedId === "default" || contexts.some((c) => c.id === loadedId);

      if (loadedId && exists) {
         currentContextId = loadedId;
      } else {
         // Fallback to default if active context is invalid/deleted
         currentContextId = "default";
         settings.activeContextId = "default";
         adapter.saveSettings(settings);
      }

      // Initialize cloud sync with current settings
      cloudSync.initialize(settings);
   }

   async function loadContexts() {
      try {
         contexts = await adapter.getContexts();
      } catch (e) {
         console.error("Failed to load contexts", e);
      }
   }

   async function loadSettings() {
      try {
         // Load contexts first so we can validate activeContextId
         await loadContexts();
         const loaded = await adapter.getSettings();
         applySettings(loaded);
      } catch (e) {
         console.error(e);
      }
   }

   // Use pre-loaded settings from main.ts if available (avoids duplicate Tauri call)
   let settingsInitialized = false;
   $effect(() => {
      if (settingsInitialized) return;
      settingsInitialized = true;

      const initial = (window as Window & { __initialSettings__?: Settings })
         .__initialSettings__;

      loadContexts().then(() => {
         if (initial) {
            // Use pre-loaded settings immediately
            applySettings(initial);
            // Clear from window to free memory
            delete (window as Window & { __initialSettings__?: Settings })
               .__initialSettings__;
         } else {
            // Fallback: load settings from backend if not pre-loaded
            loadSettings();
         }
      });
   });

   // Update cloud sync when settings change
   $effect(() => {
      cloudSync.updateSettings(settings);
   });

   function handleStash(id?: string) {
      if (id) newlyAddedStashId = id;
      refreshTrigger++;
      // Trigger cloud sync after local save (debounced)
      cloudSync.triggerSync();
   }

   $effect(() => {
      const theme = settings.theme || "system";
      const root = document.documentElement;

      const applyTheme = (isDark: boolean) => {
         if (isDark) {
            root.classList.add("dark");
         } else {
            root.classList.remove("dark");
         }
      };

      if (theme === "system") {
         const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
         applyTheme(mediaQuery.matches);

         const handler = (e: MediaQueryListEvent) => {
            if (settings.theme === "system" || !settings.theme) {
               applyTheme(e.matches);
            }
         };

         mediaQuery.addEventListener("change", handler);
         return () => mediaQuery.removeEventListener("change", handler);
      } else {
         applyTheme(theme === "dark");
      }
   });

   $effect(() => {
      const scale = settings.uiScale ?? 3;
      // 1->14px, 2->15px, 3->16px, 4->17px, 5->18px
      const fontSize = 16 + (scale - 3);
      document.documentElement.style.fontSize = `${fontSize}px`;
   });
</script>

<svelte:window onkeydown={handleKeydown} onkeyup={handleKeyup} />

<main
   class="absolute inset-0 flex flex-col overflow-hidden font-sans select-none {isGlass
      ? 'bg-background/60'
      : 'bg-background text-foreground'}"
>
   {#if contextSelectorOpen}
      <ContextSwitcher
         bind:this={contextSwitcher}
         {contexts}
         {currentContextId}
         {stashCounts}
         autoContextDetection={settings.autoContextDetection}
         mode={movingStash ? "move" : "switch"}
         title={movingStash ? "Move Stash to…" : "Switch Context"}
         onSelect={(ctx, shift) => selectContext(ctx.id, shift)}
         onCreate={async (name) => {
            const newContext = {
               id: crypto.randomUUID(),
               name,
               rules: [],
               lastUsed: new Date().toISOString(),
            };
            await adapter.saveContext(newContext);
            contexts = [...contexts, newContext];
            selectContext(newContext.id);
         }}
         onAutoContextToggle={(enabled) => {
            settings.autoContextDetection = enabled;
            adapter.saveSettings(settings); // Immediate save
         }}
         onManageContexts={() => {
            navigationSource = "Switcher";
            view = "Contexts";
         }}
         onClose={() => {
            contextSelectorOpen = false;
            isCycling = false;
            movingStash = null;
         }}
      />
   {/if}

   {#if view === "Main"}
      <Header
         bind:transferMode
         onOpenSettings={() => {
            contextSelectorOpen = false;
            view = "Settings";
         }}
         {settings}
         {contexts}
         bind:currentContextId
         bind:autoDetectedWindowTitle
         onOpenContextSwitcher={() => {
            contextSelectorOpen = true;
            isCycling = false;
         }}
      />

      <div class="flex-1 flex flex-col min-h-0">
         <div class="p-4 shrink-0">
            <Editor
               onStash={handleStash}
               {currentContextId}
               bind:content={editorDraft}
               bind:files={editorFiles}
               availableTags={allTags}
               pasteAsAttachmentThreshold={settings.pasteAsAttachmentThreshold ??
                  8}
               resizeImages={settings.resizeImages ?? true}
            />
         </div>

         <Queue
            {transferMode}
            {refreshTrigger}
            {currentContextId}
            {contexts}
            newStashId={newlyAddedStashId}
            onStashHandled={() => (newlyAddedStashId = null)}
            onMoveRequest={(stash) => {
               movingStash = stash;
               contextSelectorOpen = true;
            }}
            bind:allTags
            stripTagsOnCopy={settings.stripTagsOnCopy ?? true}
            aiConfig={settings.aiConfig}
            {autoDetectedWindowTitle}
         />
      </div>
   {:else if view === "Settings"}
      <SettingsView
         bind:settings
         {syncStatus}
         onBack={() => {
            view = "Main";
            // No need to reload, we are bound. But safe to keep or remove.
            // loadSettings();
         }}
         onOpenContexts={() => {
            navigationSource = "Settings";
            view = "Contexts";
         }}
      />
   {:else if view === "Contexts"}
      <ContextManager
         onBack={() => {
            if (navigationSource === "Switcher") {
               view = "Main";
               contextSelectorOpen = true; // Re-open switcher
            } else {
               view = "Settings";
            }
            loadSettings();
         }}
         onSelect={async (id) => {
            await loadContexts();
            selectContext(id);
            view = "Main";
            contextSelectorOpen = false;
         }}
      />
   {/if}

   <ConfirmationDialog
      bind:open={showExitConfirmation}
      title={$_("exitDialog.title")}
      description={$_("exitDialog.description")}
      confirmText={$_("exitDialog.discardAndExit")}
      variant="destructive"
      onConfirm={() => {
         editorDraft = "";
         editorFiles = [];
         appWindow.close();
      }}
   />

   <!-- Prompt Reloaded Notification -->
   {#if showPromptReloadedToast}
      <div
         class="fixed bottom-6 left-1/2 -translate-x-1/2 z-[100] px-4 py-2.5 rounded-full bg-primary text-primary-foreground shadow-lg border border-primary/20 flex items-center gap-2 text-sm font-medium"
         transition:fly={{ y: 20, duration: 250 }}
      >
         <Sparkles size={16} />
         {$_("settings.aiEnhancement.systemPrompt.reloaded")}
      </div>
   {/if}
</main>
