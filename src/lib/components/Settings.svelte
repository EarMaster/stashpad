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
  import type { Settings } from "$lib/types";

  let { onBack, onOpenContexts } = $props<{
    onBack: () => void;
    onOpenContexts: () => void;
  }>();

  let settings = $state<Settings>({
    autoContextDetection: true,
    contexts: [],
    activeContextId: undefined,
    shortcuts: {},
  });
  let isLoading = $state(true);

  const adapter = new DesktopStorageAdapter();

  async function load() {
    try {
      settings = await adapter.getSettings();
    } catch (e) {
      console.error("Failed to load settings", e);
    } finally {
      isLoading = false;
    }
  }

  async function save() {
    try {
      await adapter.saveSettings(settings);
    } catch (e) {
      console.error("Failed to save settings", e);
    }
  }

  $effect(() => {
    load();
  });
</script>

<div class="h-full flex flex-col bg-background">
  <div
    class="flex items-center gap-3 p-4 border-b border-border bg-muted/20 shrink-0"
  >
    <button
      class="p-2 hover:bg-muted rounded-md text-muted-foreground hover:text-foreground transition-colors"
      onclick={onBack}
      title="Back to Stash"
    >
      ←
    </button>
    <h1 class="text-xl font-bold tracking-tight">Settings</h1>
  </div>

  <div class="flex-1 overflow-y-auto p-4 scrollbar-hide">
    {#if isLoading}
      <div class="text-sm text-muted-foreground animate-pulse">
        Loading settings...
      </div>
    {:else}
      <div class="space-y-6 max-w-2xl mx-auto">
        <!-- Navigation to Contexts -->
        <section class="space-y-4">
          <h2
            class="text-sm font-semibold uppercase tracking-wider text-muted-foreground"
          >
            Context Management
          </h2>
          <button
            class="w-full flex items-center justify-between p-4 rounded-lg border border-border bg-card hover:bg-muted/50 transition-colors group"
            onclick={onOpenContexts}
          >
            <div class="flex flex-col items-start gap-1">
              <span class="font-medium">Manage Contexts</span>
              <span class="text-xs text-muted-foreground"
                >Configure custom contexts and auto-switching rules (Process
                Name, Window Title)</span
              >
            </div>
            <span class="text-muted-foreground group-hover:text-foreground"
              >→</span
            >
          </button>
        </section>

        <!-- General Section -->
        <section class="space-y-4">
          <h2
            class="text-sm font-semibold uppercase tracking-wider text-muted-foreground"
          >
            General
          </h2>

          <div
            class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
          >
            <div class="space-y-0.5">
              <div class="text-sm font-medium">Auto Context Detection</div>
              <div class="text-xs text-muted-foreground">
                Automatically switch stash context based on active window
              </div>
            </div>
            <label class="relative inline-flex items-center cursor-pointer">
              <input
                type="checkbox"
                class="sr-only peer"
                bind:checked={settings.autoContextDetection}
                onchange={save}
              />
              <div
                class="w-11 h-6 bg-muted peer-focus:outline-none rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-primary"
              ></div>
            </label>
          </div>
        </section>

        <!-- Shortcuts Section -->
        <section class="space-y-4">
          <h2
            class="text-sm font-semibold uppercase tracking-wider text-muted-foreground"
          >
            Shortcuts
          </h2>

          <div class="space-y-3">
            <!-- Local Switching -->
            <div
              class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
            >
              <div class="space-y-0.5">
                <div class="text-sm font-medium">Switch Context</div>
                <div class="text-xs text-muted-foreground">
                  Short press to switch, hold to cycle
                </div>
              </div>
              <input
                class="bg-muted px-2 py-1 rounded text-sm text-center font-mono w-40"
                value={settings.shortcuts?.["switch_context"] ||
                  "CommandOrControl+P"}
                onchange={(e) => {
                  if (!settings.shortcuts) settings.shortcuts = {};
                  settings.shortcuts["switch_context"] = (
                    e.target as HTMLInputElement
                  ).value;
                  save();
                }}
                placeholder="CommandOrControl+P"
              />
            </div>

            <!-- Global Toggle -->
            <div
              class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
            >
              <div class="space-y-0.5">
                <div class="text-sm font-medium">Toggle Stashpad</div>
                <div class="text-xs text-muted-foreground">
                  Global shortcut to show/hide window
                </div>
              </div>
              <input
                class="bg-muted px-2 py-1 rounded text-sm text-center font-mono w-40"
                value={settings.shortcuts?.["global_toggle"] || ""}
                onchange={(e) => {
                  if (!settings.shortcuts) settings.shortcuts = {};
                  settings.shortcuts["global_toggle"] = (
                    e.target as HTMLInputElement
                  ).value;
                  save();
                }}
                placeholder="e.g. Alt+Space"
              />
            </div>
          </div>
        </section>

        <!-- About / Footer -->
        <div class="pt-8 pb-4 text-center">
          <div class="text-xs text-muted-foreground space-y-2">
            <p class="font-medium text-foreground/80">Stashpad v0.1.0</p>
            <p>© 2025 Nico Wiedemann</p>
            <p>Licensed under AGPL-3.0-only</p>
            <div class="pt-2 opacity-50 text-[10px]">
              Made with Tauri, Svelte 5, and Rust
            </div>
          </div>
        </div>
      </div>
    {/if}
  </div>
</div>
