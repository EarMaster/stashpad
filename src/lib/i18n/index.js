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
/**
 * i18n Configuration Module
 *
 * This module sets up internationalization for Stashpad using svelte-i18n.
 * It provides:
 * - Locale initialization with browser language detection
 * - Lazy loading of translation files
 * - Fallback locale support
 * - Re-exports of commonly used functions and stores
 */
import { register, init, getLocaleFromNavigator, locale } from "svelte-i18n";
// Define available locales
export const SUPPORTED_LOCALES = ["en", "de"];
// Default/fallback locale
export const DEFAULT_LOCALE = "en";
/**
 * Display names for each supported locale.
 * These are shown in their native language for easy identification.
 */
export const LOCALE_DISPLAY_NAMES = {
    en: "English",
    de: "Deutsch",
};
/**
 * Register all available locales with lazy loading.
 * Files are only loaded when the locale is actually used.
 */
function registerLocales() {
    register("en", () => import("./locales/en.json"));
    register("de", () => import("./locales/de.json"));
}
/**
 * Determines the best locale to use based on browser/OS settings.
 * Falls back to DEFAULT_LOCALE if the browser's locale is not supported.
 */
export function getAutoDetectedLocale() {
    const browserLocale = getLocaleFromNavigator();
    if (!browserLocale) {
        return DEFAULT_LOCALE;
    }
    // Extract the primary language code (e.g., "en-US" -> "en")
    const primaryLanguage = browserLocale.split("-")[0].toLowerCase();
    // Check if the primary language is supported
    if (SUPPORTED_LOCALES.includes(primaryLanguage)) {
        return primaryLanguage;
    }
    return DEFAULT_LOCALE;
}
/**
 * Initializes the i18n system.
 * This should be called once at application startup before rendering.
 *
 * @param preferredLocale - Optional preferred locale. If 'auto' or undefined, uses browser detection.
 */
export async function setupI18n(preferredLocale) {
    // Register all available locales
    registerLocales();
    // Determine which locale to use
    let initialLocale;
    if (!preferredLocale || preferredLocale === "auto") {
        initialLocale = getAutoDetectedLocale();
    }
    else if (SUPPORTED_LOCALES.includes(preferredLocale)) {
        initialLocale = preferredLocale;
    }
    else {
        initialLocale = getAutoDetectedLocale();
    }
    // Initialize with the detected or preferred locale
    await init({
        fallbackLocale: DEFAULT_LOCALE,
        initialLocale,
    });
}
/**
 * Changes the current locale.
 * The locale change is reactive and will update all translated strings.
 *
 * @param newLocale - The locale to switch to, or 'auto' for automatic detection
 */
export function setLocale(newLocale) {
    if (newLocale === "auto") {
        locale.set(getAutoDetectedLocale());
    }
    else if (SUPPORTED_LOCALES.includes(newLocale)) {
        locale.set(newLocale);
    }
    else {
        locale.set(getAutoDetectedLocale());
    }
}
/**
 * Gets the current locale value.
 *
 * @returns The current locale or null if not set
 */
export function getCurrentLocale() {
    let currentValue = null;
    locale.subscribe((value) => {
        currentValue = value;
    })();
    return currentValue;
}
/**
 * Checks if a locale string is a valid supported locale.
 *
 * @param locale - The locale string to check
 * @returns True if the locale is supported
 */
export function isSupportedLocale(localeStr) {
    return SUPPORTED_LOCALES.includes(localeStr);
}
// Re-export commonly used items from svelte-i18n
export { _, format, t, json, number, date, time, locale, locales, isLoading, } from "svelte-i18n";
