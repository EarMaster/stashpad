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
 * Frontend error reporting.
 *
 * In a release build the webview console goes nowhere, so an uncaught render error
 * left no trace at all - which is why an unquoted class ternary in the cloud-usage
 * bar froze the settings page for two releases without a single report pointing at
 * it. Everything here goes through `log.ts`, so it lands in the app log.
 *
 * Nothing in this module may throw: it runs *from* error handlers, and a failure
 * here would replace the original error with a less useful one.
 */

import { describe, log } from './log';

/** Where the error came from, so the log line is diagnosable on its own. */
export type ErrorSource =
    | 'render'
    | 'uncaught'
    | 'unhandled-rejection'
    | 'attachment';

/**
 * Log an error through the app logger, tagged with where it came from. Never throws.
 *
 * `context` says what was being attempted, for errors that are caught and handled -
 * a rejected `invoke` carries the backend's message but not which action sent it.
 */
export function reportError(source: ErrorSource, error: unknown, context?: string): void {
    log.error(context ? `[${source}] ${context}: ${describe(error)}` : `[${source}] ${describe(error)}`);
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
