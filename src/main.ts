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

import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";
import { setupI18n, type SupportedLocale } from "$lib/i18n";
import { DesktopStorageAdapter } from "$lib/services/desktop-adapter";

// Extend window interface for initial data passing
declare global {
  interface Window {
    __app__?: unknown;
    __initialSettings__?: Settings;
  }
}

import type { Settings } from "$lib/types";

/**
 * Loads all initial data needed for app startup in parallel.
 * Returns settings and locale preference together.
 */
async function loadInitialData(): Promise<{ settings: Settings | null; locale: "auto" | SupportedLocale }> {
  try {
    const adapter = new DesktopStorageAdapter();
    const settings = await adapter.getSettings();
    return {
      settings,
      locale: (settings.locale ?? "auto") as "auto" | SupportedLocale
    };
  } catch (error) {
    // If settings can't be loaded (e.g., first run), use automatic detection
    console.warn("Could not load initial settings, using defaults:", error);
    return { settings: null, locale: "auto" };
  }
}

/**
 * Hides the splash screen with a smooth fade-out animation.
 * Called after the app is fully mounted and ready.
 */
function hideSplash(): void {
  const splash = document.getElementById("splash");
  const appContainer = document.getElementById("app");

  if (splash) {
    splash.classList.add("fade-out");
    // Remove from DOM after animation completes
    setTimeout(() => splash.remove(), 400);
  }

  if (appContainer) {
    appContainer.classList.add("ready");
  }
}

/**
 * Initialize the application.
 * Loads settings and i18n in parallel, then mounts the Svelte app.
 */
async function initApp(): Promise<void> {
  // Load initial data (settings includes locale preference)
  const { settings, locale } = await loadInitialData();

  // Store settings on window to avoid duplicate load in App.svelte
  if (settings) {
    window.__initialSettings__ = settings;
  }

  // Initialize i18n with the saved preference (or auto-detect if not set)
  await setupI18n(locale);

  // Mount the Svelte app
  const app = mount(App, {
    target: document.getElementById("app")!,
  });

  // Export for potential HMR usage
  window.__app__ = app;

  // Hide splash screen after a brief moment to ensure smooth transition
  requestAnimationFrame(() => {
    hideSplash();
  });
}

// Start the application
initApp();

