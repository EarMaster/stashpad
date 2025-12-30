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
import { setupI18n } from "$lib/i18n";
import { DesktopStorageAdapter } from "$lib/services/desktop-adapter";
/**
 * Loads the saved locale preference from settings.
 * Returns 'auto' if no preference is saved or if loading fails.
 */
async function getSavedLocalePreference() {
    try {
        const adapter = new DesktopStorageAdapter();
        const settings = await adapter.getSettings();
        return (settings.locale ?? "auto");
    }
    catch (error) {
        // If settings can't be loaded (e.g., first run), use automatic detection
        console.warn("Could not load locale preference, using automatic:", error);
        return "auto";
    }
}
/**
 * Initialize the application.
 * Loads locale preference, sets up i18n, then mounts the Svelte app.
 */
async function initApp() {
    // Load saved locale preference
    const savedLocale = await getSavedLocalePreference();
    // Initialize i18n with the saved preference (or auto-detect if not set)
    await setupI18n(savedLocale);
    // Mount the Svelte app
    const app = mount(App, {
        target: document.getElementById("app"),
    });
    // Export for potential HMR usage
    window.__app__ = app;
}
// Start the application
initApp();
