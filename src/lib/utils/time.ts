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
 * The app's only source of dates and times.
 *
 * Everything time-related goes through the Temporal API, and Temporal is taken from here
 * rather than from `globalThis`. The app runs in three webviews and only WebView2 ships
 * Temporal natively; importing the polyfill as a value means macOS and Linux run the same
 * implementation as Windows instead of whatever their engine happens to have. `Date` is
 * not used anywhere, and `npm run check:temporal` refuses it.
 *
 * "Now" is read only through {@link now}, never `Temporal.Now` directly, so the screenshot
 * demo can freeze the clock in one place with {@link setClock}.
 */
import { Temporal } from "temporal-polyfill";

export { Temporal };

const systemClock = (): Temporal.Instant => Temporal.Now.instant();
let clock = systemClock;

/**
 * Replace the clock, or restore the real one with `null`.
 *
 * For the screenshot demo, which renders "6m ago" labels and must produce the same image on
 * every run. Timers keep running; only the answer to "what time is it" is fixed.
 */
export function setClock(fixed: (() => Temporal.Instant) | null): void {
    clock = fixed ?? systemClock;
}

/** The current instant. */
export function now(): Temporal.Instant {
    return clock();
}

/** The current instant in milliseconds since the epoch, for timers, TTLs and backoff. */
export function nowMs(): number {
    return clock().epochMilliseconds;
}

/** The current instant in the canonical wire format. */
export function nowIso(): string {
    return toCanonical(clock());
}

/** The computer's own timezone, which every formatter is given explicitly. */
export function localTimeZone(): string {
    return Temporal.Now.timeZoneId();
}

/** Today on the computer's own calendar. */
export function today(): Temporal.PlainDate {
    return clock().toZonedDateTimeISO(localTimeZone()).toPlainDate();
}

/**
 * An instant in the format the app, the backend and the cloud all write: always nine
 * fractional digits, always `Z`.
 *
 * The width is fixed so that two of these compare as strings the same way they compare as
 * times - SQLite compares timestamp columns as text. Temporal's default drops trailing
 * zeros, and then `10:00:00Z` sorts after `10:00:00.5Z`.
 */
export function toCanonical(instant: Temporal.Instant): string {
    return instant.toString({ fractionalSecondDigits: 9 });
}

/** Milliseconds since the epoch, in the canonical wire format. */
export function fromEpochMs(ms: number): string {
    return toCanonical(Temporal.Instant.fromEpochMilliseconds(Math.trunc(ms)));
}

/** A date with no time of day and no zone written into it. */
const ZONELESS = /^\d{4}-\d{2}-\d{2}([ T]\d{2}:\d{2}(:\d{2}(\.\d+)?)?)?$/;

/**
 * Parse a stored or received timestamp, or `null` when it is missing or unreadable.
 *
 * Accepts RFC 3339 in any width, the space-separated form older backends wrote, and a
 * zoneless `YYYY-MM-DD HH:MM:SS` - which `Temporal.Instant.from` rejects outright, and
 * which every writer here meant as UTC. Numbers are Unix seconds, the unit the local
 * database keeps `updatedAt` in.
 */
export function parseInstant(
    value: string | number | Temporal.Instant | null | undefined,
): Temporal.Instant | null {
    if (value === null || value === undefined || value === "") return null;
    if (value instanceof Temporal.Instant) return value;
    if (typeof value === "number") {
        if (!Number.isFinite(value)) return null;
        return Temporal.Instant.fromEpochMilliseconds(Math.trunc(value * 1000));
    }

    let text = value.trim();
    if (ZONELESS.test(text)) {
        text = (text.length === 10 ? `${text}T00:00:00` : text.replace(" ", "T")) + "Z";
    }

    try {
        return Temporal.Instant.from(text);
    } catch {
        return null;
    }
}

/** Milliseconds since the epoch for a timestamp, or `fallback` when it does not parse. */
export function toEpochMs(
    value: string | number | null | undefined,
    fallback = 0,
): number {
    return parseInstant(value)?.epochMilliseconds ?? fallback;
}

/**
 * Oldest first, for `Array.prototype.sort`. A missing or unreadable timestamp sorts as
 * the epoch, the same place the old `new Date(x || 0)` comparisons put it.
 */
export function compareTimestamps(
    a: string | number | null | undefined,
    b: string | number | null | undefined,
): number {
    return toEpochMs(a) - toEpochMs(b);
}
