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
  import { _ } from "$lib/i18n";
  import ShortcutInput from "./ShortcutInput.svelte";

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
      title={$_("settings.backToStash")}
    >
      ←
    </button>
    <h1 class="text-xl font-bold tracking-tight">{$_("settings.title")}</h1>
  </div>

  <div class="flex-1 overflow-y-auto p-4 scrollbar-hide">
    {#if isLoading}
      <div class="text-sm text-muted-foreground animate-pulse">
        {$_("settings.loadingSettings")}
      </div>
    {:else}
      <div class="space-y-6 max-w-2xl mx-auto">
        <!-- Navigation to Contexts -->
        <section class="space-y-4">
          <h2
            class="text-sm font-semibold uppercase tracking-wider text-muted-foreground"
          >
            {$_("settings.contextManagement.title")}
          </h2>
          <button
            class="w-full flex items-center justify-between p-4 rounded-lg border border-border bg-card hover:bg-muted/50 transition-colors group"
            onclick={onOpenContexts}
          >
            <div class="flex flex-col items-start gap-1">
              <span class="font-medium"
                >{$_("settings.contextManagement.manageContexts")}</span
              >
              <span class="text-xs text-muted-foreground"
                >{$_("settings.contextManagement.description")}</span
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
            {$_("settings.general.title")}
          </h2>

          <div
            class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
          >
            <div class="space-y-0.5">
              <div class="text-sm font-medium">
                {$_("settings.general.autoContextDetection.label")}
              </div>
              <div class="text-xs text-muted-foreground">
                {$_("settings.general.autoContextDetection.description")}
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
                class="w-11 h-6 bg-muted rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-primary peer-focus-visible:ring-2 peer-focus-visible:ring-primary peer-focus-visible:ring-offset-2 peer-focus-visible:ring-offset-background"
              ></div>
            </label>
          </div>

          <div
            class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
          >
            <div class="space-y-0.5">
              <div class="text-sm font-medium">
                {$_("settings.general.visualEffects.label")}
              </div>
              <div class="text-xs text-muted-foreground">
                {$_("settings.general.visualEffects.description")}
              </div>
            </div>
            <label class="relative inline-flex items-center cursor-pointer">
              <input
                type="checkbox"
                class="sr-only peer"
                checked={settings.visualEffectsEnabled ??
                  !window.matchMedia("(prefers-reduced-transparency: reduce)")
                    .matches}
                onchange={(e) => {
                  settings.visualEffectsEnabled = e.currentTarget.checked;
                  save();
                }}
              />
              <div
                class="w-11 h-6 bg-muted rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-primary peer-focus-visible:ring-2 peer-focus-visible:ring-primary peer-focus-visible:ring-offset-2 peer-focus-visible:ring-offset-background"
              ></div>
            </label>
          </div>
        </section>

        <!-- Shortcuts Section -->
        <section class="space-y-4">
          <h2
            class="text-sm font-semibold uppercase tracking-wider text-muted-foreground"
          >
            {$_("settings.shortcuts.title")}
          </h2>

          <div class="space-y-3">
            <!-- Local Switching -->
            <div
              class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
            >
              <div class="space-y-0.5">
                <div class="text-sm font-medium">
                  {$_("settings.shortcuts.switchContext.label")}
                </div>
                <div class="text-xs text-muted-foreground">
                  {$_("settings.shortcuts.switchContext.description")}
                </div>
              </div>
              <ShortcutInput
                value={settings.shortcuts?.["switch_context"] ||
                  "CommandOrControl+P"}
                placeholder={$_("settings.shortcuts.clickToSet")}
                onchange={(shortcut) => {
                  if (!settings.shortcuts) settings.shortcuts = {};
                  settings.shortcuts["switch_context"] = shortcut;
                  save();
                }}
              />
            </div>

            <!-- Global Toggle -->
            <div
              class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
            >
              <div class="space-y-0.5">
                <div class="text-sm font-medium">
                  {$_("settings.shortcuts.toggleStashpad.label")}
                </div>
                <div class="text-xs text-muted-foreground">
                  {$_("settings.shortcuts.toggleStashpad.description")}
                </div>
              </div>
              <ShortcutInput
                value={settings.shortcuts?.["global_toggle"] || ""}
                placeholder={$_("settings.shortcuts.clickToSet")}
                onchange={(shortcut) => {
                  if (!settings.shortcuts) settings.shortcuts = {};
                  settings.shortcuts["global_toggle"] = shortcut;
                  save();
                }}
              />
            </div>
          </div>
        </section>

        <!-- About / Footer -->
        <div class="pt-8 pb-4 text-center">
          <div class="text-xs text-muted-foreground space-y-2">
            <p class="font-medium text-foreground/80">
              {$_("app.name")}
              {$_("app.version")}
            </p>
            <p>{$_("app.copyright")}</p>
            <p>{$_("app.license")}</p>
            <div class="pt-2 opacity-50 text-[10px]">
              {$_("app.madeWith")}
            </div>
          </div>
        </div>
      </div>
    {/if}
  </div>
</div>
