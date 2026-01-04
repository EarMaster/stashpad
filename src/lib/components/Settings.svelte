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
  import {
    _,
    setLocale,
    SUPPORTED_LOCALES,
    LOCALE_DISPLAY_NAMES,
    type SupportedLocale,
  } from "$lib/i18n";
  import ShortcutInput from "./ShortcutInput.svelte";

  let {
    settings = $bindable(),
    onBack,
    onOpenContexts,
  } = $props<{
    settings: Settings;
    onBack: () => void;
    onOpenContexts: () => void;
  }>();

  const adapter = new DesktopStorageAdapter();

  async function save() {
    try {
      await adapter.saveSettings(settings);
    } catch (e) {
      console.error("Failed to save settings", e);
    }
  }
  // Force rebuild
</script>

<div class="h-full flex flex-col bg-background">
  <div
    data-tauri-drag-region
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
    <div class="space-y-6 max-w-2xl mx-auto">
      <!-- Navigation to Contexts -->
      <section class="space-y-4">
        <h2
          class="text-sm font-semibold uppercase tracking-wider text-muted-foreground"
        >
          {$_("settings.contextManagement.title")}
        </h2>

        <!-- Auto Context Detection -->
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

        <button
          class="w-full flex items-center justify-between p-3 rounded-lg border border-border bg-card hover:bg-muted/50 transition-colors group"
          onclick={onOpenContexts}
        >
          <div class="flex flex-col items-start gap-1">
            <span class="text-sm font-medium"
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

        <!-- Language Selector -->
        <div
          class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
        >
          <div class="space-y-0.5">
            <div class="text-sm font-medium">
              {$_("settings.general.language.label")}
            </div>
            <div class="text-xs text-muted-foreground">
              {$_("settings.general.language.description")}
            </div>
          </div>
          <select
            class="bg-muted border border-border rounded-md px-3 py-1.5 text-sm font-medium cursor-pointer outline-none focus:border-primary focus:ring-1 focus:ring-primary transition-colors"
            value={settings.locale ?? "auto"}
            onchange={(e) => {
              const newLocale = e.currentTarget.value as
                | "auto"
                | SupportedLocale;
              settings.locale = newLocale;
              setLocale(newLocale);
              save();
            }}
          >
            <option value="auto">
              {$_("settings.general.language.automatic")}
            </option>
            {#each SUPPORTED_LOCALES as localeCode}
              <option value={localeCode}>
                {LOCALE_DISPLAY_NAMES[localeCode]}
              </option>
            {/each}
          </select>
        </div>

        <!-- New Stash Position Selector -->
        <div
          class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
        >
          <div class="space-y-0.5">
            <div class="text-sm font-medium">
              {$_("settings.general.newStashPosition.label")}
            </div>
            <div class="text-xs text-muted-foreground">
              {$_("settings.general.newStashPosition.description")}
            </div>
            <div class="text-[10px] text-muted-foreground/80 mt-1 italic">
              {$_("settings.general.newStashPosition.shiftModifier")}
            </div>
          </div>
          <div class="flex bg-muted p-1 rounded-lg border border-border">
            {#each ["top", "bottom"] as pos}
              <label
                class="flex items-center gap-2 px-4 py-1.5 rounded-md text-xs font-semibold cursor-pointer transition-all focus-within:ring-2 focus-within:ring-primary focus-within:ring-offset-2 focus-within:ring-offset-background {settings.newStashPosition ===
                pos
                  ? 'bg-primary text-primary-foreground shadow-sm ring-1 ring-primary/20'
                  : 'text-muted-foreground hover:text-foreground hover:bg-accent'}"
              >
                <input
                  type="radio"
                  name="newStashPosition"
                  value={pos}
                  class="sr-only"
                  checked={settings.newStashPosition === pos}
                  onchange={(e) => {
                    settings.newStashPosition = e.currentTarget.value as
                      | "top"
                      | "bottom";
                    save();
                  }}
                />
                {$_(`settings.general.newStashPosition.${pos}`)}
              </label>
            {/each}
          </div>
        </div>

        <!-- Strip Tags on Copy -->
        <div
          class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
        >
          <div class="space-y-0.5">
            <div class="text-sm font-medium">
              {$_("settings.general.stripTagsOnCopy.label")}
            </div>
            <div class="text-xs text-muted-foreground">
              {$_("settings.general.stripTagsOnCopy.description")}
            </div>
          </div>
          <label class="relative inline-flex items-center cursor-pointer">
            <input
              type="checkbox"
              class="sr-only peer"
              bind:checked={settings.stripTagsOnCopy}
              onchange={save}
            />
            <div
              class="w-11 h-6 bg-muted rounded-full peer peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-primary peer-focus-visible:ring-2 peer-focus-visible:ring-primary peer-focus-visible:ring-offset-2 peer-focus-visible:ring-offset-background"
            ></div>
          </label>
        </div>
      </section>

      <!-- Appearance Section -->
      <section class="space-y-4">
        <h2
          class="text-sm font-semibold uppercase tracking-wider text-muted-foreground"
        >
          {$_("settings.appearance.title")}
        </h2>

        <div
          class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
        >
          <div class="space-y-0.5">
            <div class="text-sm font-medium">
              {$_("settings.appearance.theme.label")}
            </div>
            <div class="text-xs text-muted-foreground">
              {$_("settings.appearance.theme.description")}
            </div>
          </div>
          <div class="flex bg-muted p-1 rounded-lg border border-border">
            {#each ["light", "dark", "system"] as theme}
              <label
                class="flex items-center gap-2 px-3 py-1.5 rounded-md text-xs font-semibold cursor-pointer transition-all focus-within:ring-2 focus-within:ring-primary focus-within:ring-offset-2 focus-within:ring-offset-background {(settings.theme ??
                  'system') === theme
                  ? 'bg-primary text-primary-foreground shadow-sm ring-1 ring-primary/20'
                  : 'text-muted-foreground hover:text-foreground hover:bg-accent'}"
              >
                <input
                  type="radio"
                  name="theme"
                  value={theme}
                  class="sr-only"
                  checked={(settings.theme ?? "system") === theme}
                  onchange={(e) => {
                    settings.theme = e.currentTarget.value as
                      | "light"
                      | "dark"
                      | "system";
                    save();
                  }}
                />
                {$_(`settings.appearance.theme.${theme}`)}
              </label>
            {/each}
          </div>
        </div>

        <div
          class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
        >
          <div class="space-y-0.5">
            <div class="text-sm font-medium">
              {$_("settings.appearance.uiScale.label")}
            </div>
            <div class="text-xs text-muted-foreground">
              {$_("settings.appearance.uiScale.description")}
            </div>
          </div>
          <div class="flex items-center gap-3">
            <input
              type="range"
              min="1"
              max="5"
              step="1"
              class="w-32 h-2 bg-muted rounded-lg appearance-none cursor-pointer accent-primary"
              value={settings.uiScale ?? 3}
              onchange={(e) => {
                const val = parseInt(e.currentTarget.value);
                settings.uiScale = val;
                save();
              }}
            />
          </div>
        </div>

        <div
          class="flex items-center justify-between p-3 rounded-lg border border-border bg-card"
        >
          <div class="space-y-0.5">
            <div class="text-sm font-medium">
              {$_("settings.appearance.visualEffects.label")}
            </div>
            <div class="text-xs text-muted-foreground">
              {$_("settings.appearance.visualEffects.description")}
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
  </div>
</div>
