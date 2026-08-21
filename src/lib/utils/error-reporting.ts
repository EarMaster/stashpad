// SPDX-License-Identifier: AGPL-3.0-only

// Copyright (C) 2026 Nico Wiedemann
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
 * Frontend error reporting.
 *
 * In a release build the webview console goes nowhere, so an uncaught render error
 * left no trace at all - which is why an unquoted class ternary in the cloud-usage
 * bar froze the settings page for two releases without a single report pointing at
 * it. Everything here also forwards to the Rust logger so it lands in the app log.
 *
 * Nothing in this module may throw: it runs *from* error handlers, and a failure
 * here would replace the original error with a less useful one.
 */

/** Where the error came from, so the log line is diagnosable on its own. */
export type ErrorSource =
    | 'render'
    | 'uncaught'
    | 'unhandled-rejection';

type Reporter = (message: string) => void | Promise<void>;

let forward: Reporter | null = null;

/**
 * Give the reporter a way to reach the backend logger.
 *
 * Injected rather than imported so this module stays free of Tauri imports and
 * remains usable from a future web adapter.
 */
export function setErrorForwarder(reporter: Reporter | null): void {
    forward = reporter;
}

/** Flatten anything throwable into one log line, stack included when there is one. */
function describe(error: unknown): string {
    if (error instanceof Error) {
        return error.stack ? `${error.name}: ${error.message}\n${error.stack}` : `${error.name}: ${error.message}`;
    }
    if (typeof error === 'string') return error;
    try {
        return JSON.stringify(error);
    } catch {
        return String(error);
    }
}

/** Log an error locally and forward it to the backend. Never throws. */
export function reportError(source: ErrorSource, error: unknown): void {
    const line = `[${source}] ${describe(error)}`;
    try {
        console.error(line);
    } catch {
        // A console that throws is not worth recovering from.
    }
    try {
        // Fire and forget: a failed report must not mask the error being reported.
        void Promise.resolve(forward?.(line)).catch(() => {});
    } catch {
        // Same reasoning - swallow.
    }
}

/**
 * Install window-level handlers for errors that escape every component boundary.
 *
 * Returns a teardown function, mainly so tests can install and remove cleanly.
 */
export function installGlobalErrorReporter(): () => void {
    const onError = (event: ErrorEvent) => {
        reportError('uncaught', event.error ?? event.message);
    };
    const onRejection = (event: PromiseRejectionEvent) => {
        reportError('unhandled-rejection', event.reason);
    };

    window.addEventListener('error', onError);
    window.addEventListener('unhandledrejection', onRejection);

    return () => {
        window.removeEventListener('error', onError);
        window.removeEventListener('unhandledrejection', onRejection);
    };
}
