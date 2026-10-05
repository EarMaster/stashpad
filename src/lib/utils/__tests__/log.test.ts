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

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { log, setLogForwarder, type LogLevel } from '../log';

describe('log', () => {
    let forwarded: [LogLevel, string][];

    beforeEach(() => {
        forwarded = [];
        setLogForwarder((level, message) => {
            forwarded.push([level, message]);
        });
        for (const level of ['error', 'warn', 'info', 'debug'] as const) {
            vi.spyOn(console, level).mockImplementation(() => {});
        }
    });

    afterEach(() => {
        setLogForwarder(null);
        vi.restoreAllMocks();
    });

    it('writes to the console and forwards every level with its own level', () => {
        log.error('e');
        log.warn('w');
        log.info('i');
        log.debug('d');

        expect(console.error).toHaveBeenCalledWith('e');
        expect(console.debug).toHaveBeenCalledWith('d');
        expect(forwarded).toEqual([
            ['error', 'e'],
            ['warn', 'w'],
            ['info', 'i'],
            ['debug', 'd'],
        ]);
    });

    it('joins its arguments into one line, keeping an error\'s stack and an object\'s fields', () => {
        log.error('Failed to save:', new Error('disk full'), { code: 'io' });

        const [, message] = forwarded[0];
        expect(message).toMatch(/^Failed to save: Error: disk full\n/);
        expect(message).toContain('{"code":"io"}');
    });

    it('never throws, whatever the forwarder does', () => {
        setLogForwarder(() => {
            throw new Error('backend unreachable');
        });
        expect(() => log.error('still fine')).not.toThrow();

        setLogForwarder(() => Promise.reject(new Error('ipc down')));
        expect(() => log.warn('still fine')).not.toThrow();
    });

    it('still writes to the console before a forwarder is set', () => {
        setLogForwarder(null);
        log.info('early');
        expect(console.info).toHaveBeenCalledWith('early');
    });
});
