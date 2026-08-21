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

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
    installGlobalErrorReporter,
    reportError,
    setErrorForwarder,
} from '../error-reporting';

describe('error reporting', () => {
    let forwarded: string[];
    let consoleError: ReturnType<typeof vi.spyOn>;

    beforeEach(() => {
        forwarded = [];
        setErrorForwarder((message) => {
            forwarded.push(message);
        });
        consoleError = vi.spyOn(console, 'error').mockImplementation(() => {});
    });

    afterEach(() => {
        setErrorForwarder(null);
        consoleError.mockRestore();
    });

    it('forwards the message and the stack of an Error', () => {
        reportError('render', new ReferenceError('bg is not defined'));

        expect(forwarded).toHaveLength(1);
        expect(forwarded[0]).toContain('[render]');
        expect(forwarded[0]).toContain('bg is not defined');
    });

    it('handles a thrown non-Error without losing it', () => {
        reportError('uncaught', { code: 42 });

        expect(forwarded[0]).toContain('[uncaught]');
        expect(forwarded[0]).toContain('42');
    });

    it('survives a forwarder that throws', () => {
        // The reporter runs *from* error handlers. If it could throw, it would replace
        // the original error with a less useful one.
        setErrorForwarder(() => {
            throw new Error('backend unreachable');
        });

        expect(() => reportError('render', new Error('original'))).not.toThrow();
        expect(consoleError).toHaveBeenCalled();
    });

    it('survives a forwarder that rejects', () => {
        setErrorForwarder(() => Promise.reject(new Error('ipc down')));

        expect(() => reportError('render', new Error('original'))).not.toThrow();
    });

    it('reports uncaught window errors and unhandled rejections', () => {
        const teardown = installGlobalErrorReporter();

        window.dispatchEvent(
            new ErrorEvent('error', { error: new Error('from window'), message: 'from window' })
        );
        expect(forwarded.some((line) => line.includes('from window'))).toBe(true);

        // jsdom does not construct PromiseRejectionEvent, so build the event by hand.
        const rejection = new Event('unhandledrejection') as Event & { reason?: unknown };
        rejection.reason = new Error('from promise');
        window.dispatchEvent(rejection);
        expect(forwarded.some((line) => line.includes('from promise'))).toBe(true);

        teardown();

        // Checked via the rejection path: an 'error' event with nobody listening is
        // escalated by jsdom into a genuine uncaught exception and fails the run.
        const before = forwarded.length;
        const afterTeardown = new Event('unhandledrejection') as Event & { reason?: unknown };
        afterTeardown.reason = new Error('after teardown');
        window.dispatchEvent(afterTeardown);
        expect(forwarded).toHaveLength(before);
    });
});
