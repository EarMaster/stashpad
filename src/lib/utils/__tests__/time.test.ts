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

import { afterEach, describe, expect, it } from 'vitest';
import {
    compareTimestamps,
    fromEpochMs,
    nowIso,
    nowMs,
    parseInstant,
    setClock,
    Temporal,
    toCanonical,
    toEpochMs,
} from '../time';

const AT = Temporal.Instant.from('2026-08-22T10:00:00Z');

describe('time', () => {
    afterEach(() => setClock(null));

    describe('toCanonical', () => {
        it('always writes nine fractional digits and a Z', () => {
            expect(toCanonical(AT)).toBe('2026-08-22T10:00:00.000000000Z');
            expect(fromEpochMs(AT.epochMilliseconds + 500)).toBe('2026-08-22T10:00:00.500000000Z');
        });

        it('sorts as text in the same order as in time', () => {
            // The reason for the fixed width: with Temporal's default, ".5Z" would sort
            // before the whole second it comes after.
            const whole = toCanonical(AT);
            const half = fromEpochMs(AT.epochMilliseconds + 500);
            expect(whole < half).toBe(true);
        });
    });

    describe('parseInstant', () => {
        it.each([
            ['RFC 3339 with Z', '2026-08-22T10:00:00Z'],
            ['RFC 3339 with an offset', '2026-08-22T12:00:00+02:00'],
            ['nine fractional digits', '2026-08-22T10:00:00.000000000Z'],
            ['the space-separated form older backends wrote', '2026-08-22 10:00:00+00:00'],
            ['a zoneless SQLite CURRENT_TIMESTAMP, read as UTC', '2026-08-22 10:00:00'],
            ['a zoneless ISO string, read as UTC', '2026-08-22T10:00:00'],
            ['Unix seconds', AT.epochMilliseconds / 1000],
        ])('reads %s', (_label, value) => {
            expect(parseInstant(value)?.equals(AT)).toBe(true);
        });

        it('reads a bare date as midnight UTC', () => {
            expect(toCanonical(parseInstant('2026-08-22')!)).toBe('2026-08-22T00:00:00.000000000Z');
        });

        it.each([[''], [null], [undefined], ['yesterday'], [Number.NaN]])(
            'returns null for %s',
            (value) => {
                expect(parseInstant(value as string)).toBeNull();
            },
        );
    });

    it('toEpochMs falls back when the value does not parse', () => {
        expect(toEpochMs('2026-08-22T10:00:00Z')).toBe(AT.epochMilliseconds);
        expect(toEpochMs('garbage', 7)).toBe(7);
        expect(toEpochMs(undefined)).toBe(0);
    });

    it('compareTimestamps sorts oldest first and puts missing values at the epoch', () => {
        // null, not undefined: sort() moves undefined to the end without asking the comparator.
        const sorted = ['2026-08-22T10:00:01Z', null, '2026-08-22T10:00:00Z'].sort(
            compareTimestamps,
        );
        expect(sorted).toEqual([null, '2026-08-22T10:00:00Z', '2026-08-22T10:00:01Z']);
    });

    it('setClock fixes every reading of now, and null restores the real one', () => {
        setClock(() => AT);
        expect(nowMs()).toBe(AT.epochMilliseconds);
        expect(nowIso()).toBe('2026-08-22T10:00:00.000000000Z');

        setClock(null);
        expect(nowMs()).toBeGreaterThan(AT.epochMilliseconds);
    });
});
