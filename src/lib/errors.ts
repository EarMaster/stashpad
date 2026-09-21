// SPDX-License-Identifier: AGPL-3.0-only

// Copyright (C) 2026 Nico Wiedemann
//
// This file is part of Stashpad.
// Stashpad is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License, version 3,
// as published by the Free Software Foundation.
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Affero General Public License for more details.

/**
 * Turning whatever a failed command threw into something worth showing a person.
 *
 * Tauri rejects with the serialised `Err` of the command, so a backend failure arrives as
 * `{ code, message }` — see `src-tauri/src/uierror.rs`. `code` is an identifier such as
 * `e2ee.nowhere_safe` and is looked up as `errors.<code>`; `message` is English and is what
 * gets shown when there is no translation yet.
 *
 * That fallback is the point. A code can be added on the Rust side and translated weeks
 * later, and in the meantime the user sees the English sentence rather than a raw key or a
 * blank. It also means the 200-odd internal failures that will never be worth translating
 * keep working exactly as they did, with an empty code.
 */

import { unwrapFunctionStore, _ } from "svelte-i18n";

// `$_` is a store and this is not a component, so the formatter is unwrapped once here.
// Deliberately svelte-i18n's own accessor rather than `get(_)` from `svelte/store`: no
// other file in this codebase imports from `svelte/store`, and that import is the legacy
// API the project's conventions rule out.
const translate = unwrapFunctionStore(_);

/** The shape a Rust command's error arrives in. */
interface BackendError {
    code: string;
    message: string;
    /** Present only where a translation needs to interpolate something. */
    values?: Record<string, string>;
}

function isBackendError(value: unknown): value is BackendError {
    return (
        typeof value === "object" &&
        value !== null &&
        "message" in value &&
        typeof (value as { message: unknown }).message === "string"
    );
}

/**
 * The sentence to show for a caught value.
 *
 * `fallback` is used only when there is nothing usable at all — a thrown `undefined`, or an
 * object with no message. Pass a translation key's output, not a key.
 */
export function errorText(error: unknown, fallback = ""): string {
    if (isBackendError(error)) {
        const code = typeof error.code === "string" ? error.code : "";
        if (code) {
            const key = `errors.${code}`;
            // svelte-i18n hands back the key itself when it has no translation, which is
            // how we tell "not translated yet" from "translated to something".
            const translated = error.values
                ? translate(key, { values: error.values })
                : translate(key);
            if (translated && translated !== key) {
                return translated;
            }
        }
        return error.message;
    }

    // An Error thrown by our own frontend code, and the string errors that commands not yet
    // migrated still produce.
    if (error instanceof Error) return error.message;
    if (typeof error === "string") return error;

    return fallback || String(error ?? "");
}

/**
 * The identifier behind a failure, for code that wants to branch rather than print.
 *
 * Empty when the error carries none, which includes every error thrown by the frontend.
 */
export function errorCode(error: unknown): string {
    return isBackendError(error) && typeof error.code === "string" ? error.code : "";
}
