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
   import type { Settings, StashItem } from "$lib/types";
   import Header from "$lib/components/Header.svelte";
   import Editor from "$lib/components/Editor.svelte";
   import Queue from "$lib/components/Queue.svelte";
   import SettingsView from "$lib/components/Settings.svelte";
   import ContextManager from "$lib/components/ContextManager.svelte";
   import ContextSwitcher from "$lib/components/ContextSwitcher.svelte";

   let transferMode = $state("Drag");
   let refreshTrigger = $state(0);
   let view = $state<"Main" | "Settings" | "Contexts">("Main");
   let navigationSource = $state<"Settings" | "Switcher">("Settings");
   let currentContextId = $state<string>("default");
   let movingStash = $state<StashItem | null>(null);

   // Centralized settings state
   let settings = $state<Settings>({
      autoContextDetection: true,
      contexts: [],
      activeContextId: "default",
      shortcuts: {
         switch_context: "CommandOrControl+P",
      },
   });

   const adapter = new DesktopStorageAdapter();

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
      }
      // Arrow keys, Escape, Enter are now handled by ContextSwitcher when open!
      // But wait: if we rely on ContextSwitcher for those, we must ensure it is mounted.
      // And we handle cycling logic in App (keyup).
      // The prop "selectedIndex" is bindable, so selectNextContext updates it in App, reflected in Switcher.
      // The Switcher's own keyDown handler will handle arrows.
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

   let contextSwitcher = $state<any>(null);

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
            };
            await adapter.saveStash(updated);
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
      } catch (e) {
         console.error(e);
      }
   }

   $effect(() => {
      loadSettings();
   });

   function handleStash() {
      refreshTrigger++;
   }
</script>

<svelte:window onkeydown={handleKeydown} onkeyup={handleKeyup} />

<main
   class="h-screen w-screen flex flex-col bg-background text-foreground overflow-hidden font-sans select-none relative"
>
   {#if contextSelectorOpen}
      <ContextSwitcher
         bind:this={contextSwitcher}
         contexts={settings.contexts}
         {currentContextId}
         autoContextDetection={settings.autoContextDetection}
         mode={movingStash ? "move" : "switch"}
         title={movingStash ? "Move Stash to..." : "Switch Context"}
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
         <div class="p-4 pb-0 shrink-0">
            <Editor onStash={handleStash} {currentContextId} />
         </div>

         <div class="h-px bg-border/50 my-2 mx-4 shrink-0"></div>

         <Queue
            {transferMode}
            {refreshTrigger}
            {currentContextId}
            contexts={settings.contexts}
            onMoveRequest={(stash) => {
               movingStash = stash;
               contextSelectorOpen = true;
            }}
         />
      </div>
   {:else if view === "Settings"}
      <SettingsView
         onBack={() => {
            view = "Main";
            loadSettings(); // Reload on return in case of changes
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
</main>
