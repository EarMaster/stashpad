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
 * The webview's logger. Use this instead of `console.*`, which is the only other thing in
 * the app allowed to touch the console.
 *
 * In a release build the webview console goes nowhere, so a `console.error` was lost the
 * moment it was written - nothing a user could send, nothing to read after the fact. Each
 * call here still writes to the console, for the devtools in a dev build, and also hands
 * the line to the backend logger, which puts it in the same bounded log file as the Rust
 * side's records. The backend decides what to keep: debug lines only reach the file in a
 * dev build.
 *
 * Nothing in this module may throw: it runs from catch blocks and error handlers, and a
 * failure here would replace the error being logged with a less useful one.
 */

export type LogLevel = "error" | "warn" | "info" | "debug";

type Forwarder = (level: LogLevel, message: string) => void | Promise<void>;

let forward: Forwarder | null = null;

/**
 * Give the logger a way to reach the backend.
 *
 * Injected rather than imported so this module stays free of Tauri imports and remains
 * usable from a future web adapter.
 */
export function setLogForwarder(forwarder: Forwarder | null): void {
    forward = forwarder;
}

/** Flatten anything loggable into text, stack included when there is one. */
export function describe(value: unknown): string {
    if (value instanceof Error) {
        return value.stack
            ? `${value.name}: ${value.message}\n${value.stack}`
            : `${value.name}: ${value.message}`;
    }
    if (typeof value === "string") return value;
    try {
        return JSON.stringify(value) ?? String(value);
    } catch {
        return String(value);
    }
}

function emit(level: LogLevel, args: unknown[]): void {
    try {
        console[level](...args);
    } catch {
        // A console that throws is not worth recovering from.
    }
    try {
        const message = args.map(describe).join(" ");
        // Fire and forget: a failed forward must not mask what is being logged.
        void Promise.resolve(forward?.(level, message)).catch(() => {});
    } catch {
        // Same reasoning - swallow.
    }
}

/** Log a line to the console and to the app's log file. Never throws. */
export const log = {
    error: (...args: unknown[]) => emit("error", args),
    warn: (...args: unknown[]) => emit("warn", args),
    info: (...args: unknown[]) => emit("info", args),
    debug: (...args: unknown[]) => emit("debug", args),
};
