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
   import { _ } from "$lib/i18n";
   import type { Settings, StashItem } from "$lib/types";
   import Header from "$lib/components/Header.svelte";
   import Editor from "$lib/components/Editor.svelte";
   import Queue from "$lib/components/Queue.svelte";
   import SettingsView from "$lib/components/Settings.svelte";
   import ContextManager from "$lib/components/ContextManager.svelte";
   import ContextSwitcher from "$lib/components/ContextSwitcher.svelte";
   import ConfirmationDialog from "$lib/components/ConfirmationDialog.svelte";
   import { onMount } from "svelte";
   import { getCurrentWindow } from "@tauri-apps/api/window";

   let transferMode = $state("Drag");
   let refreshTrigger = $state(0);
   let view = $state<"Main" | "Settings" | "Contexts">("Main");
   let navigationSource = $state<"Settings" | "Switcher">("Settings");
   let currentContextId = $state<string>("default");
   let movingStash = $state<StashItem | null>(null);
   let newlyAddedStashId = $state<string | null>(null);
   let allTags = $state<string[]>([]);

   // Draft state persistence
   let editorDraft = $state("");
   let editorFiles = $state<string[]>([]);
   let showExitConfirmation = $state(false);

   const appWindow = getCurrentWindow();

   onMount(() => {
      const unlisten = appWindow.onCloseRequested(async (event) => {
         if (editorDraft.trim() || editorFiles.length > 0) {
            event.preventDefault();
            showExitConfirmation = true;
         }
      });
      return () => {
         unlisten.then((f) => f());
      };
   });

   // Centralized settings state
   let settings = $state<Settings>({
      autoContextDetection: true,
      visualEffectsEnabled: undefined,
      contexts: [],
      activeContextId: "default",
      shortcuts: {
         switch_context: "CommandOrControl+P",
      },
   });

   const adapter = new DesktopStorageAdapter();

   let isGlass = $derived(
      settings.visualEffectsEnabled ??
         !window.matchMedia("(prefers-reduced-transparency: reduce)").matches,
   );

   // Context Switching Logic
   let contextSelectorOpen = $state(false);
   let isCycling = $state(false); // Track if user is cycling through contexts
   let lastUsedContexts = $state<string[]>([]); // Future use

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

         // Update lastUsed timestamp
         const ctx = settings.contexts.find((c) => c.id === ctxId);
         if (ctx) {
            ctx.lastUsed = new Date().toISOString();
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
      return [{ id: "default", name: "Default" }, ...settings.contexts];
   }

   async function loadSettings() {
      try {
         settings = await adapter.getSettings();
         // Ensure defaults if missing
         if (!settings.shortcuts) {
            settings.shortcuts = {};
         }

         const loadedId = settings.activeContextId;
         // Validate context exists (or is default)
         const exists =
            loadedId === "default" ||
            settings.contexts.some((c) => c.id === loadedId);

         if (loadedId && exists) {
            currentContextId = loadedId;
         }
      } catch (e) {
         console.error(e);
      }
   }

   $effect(() => {
      loadSettings();
   });

   function handleStash(id?: string) {
      if (id) newlyAddedStashId = id;
      refreshTrigger++;
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
         contexts={settings.contexts}
         {currentContextId}
         autoContextDetection={settings.autoContextDetection}
         mode={movingStash ? "move" : "switch"}
         title={movingStash ? "Move Stash to…" : "Switch Context"}
         onSelect={(ctx, shift) => selectContext(ctx.id, shift)}
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
         onOpenSettings={() => (view = "Settings")}
         {settings}
         bind:currentContextId
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
            />
         </div>

         <Queue
            {transferMode}
            {refreshTrigger}
            {currentContextId}
            contexts={settings.contexts}
            newStashId={newlyAddedStashId}
            onStashHandled={() => (newlyAddedStashId = null)}
            onMoveRequest={(stash) => {
               movingStash = stash;
               contextSelectorOpen = true;
            }}
            bind:allTags
            stripTagsOnCopy={settings.stripTagsOnCopy ?? false}
         />
      </div>
   {:else if view === "Settings"}
      <SettingsView
         bind:settings
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
</main>
