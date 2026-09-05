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
 * Entry point for the screenshot demo. Never shipped: `vite build` only takes
 * `index.html` as input, so this page exists on the dev server and nowhere else.
 *
 * The fake backend has to be installed before the app module is evaluated, because
 * `main.ts` starts calling `invoke()` at import time. Hence the dynamic import at the
 * bottom rather than a static one at the top.
 *
 * Query parameters, all optional:
 *   ?lang=en|de     locale to boot in (default: en)
 *   ?theme=dark|light
 *   ?signedOut=1    cloud sync signed out, for the "connect" state
 */

import { installMockBackend } from './mock-backend';
import { settings as fixtureSettings } from './fixtures';
import type { Settings } from '../src/lib/types';

const params = new URLSearchParams(window.location.search);

const settings: Settings = structuredClone(fixtureSettings);
settings.locale = params.get('lang') ?? 'en';
settings.theme = (params.get('theme') as Settings['theme']) ?? 'dark';

if (params.get('signedOut') === '1') {
    settings.cloudConfig = { enabled: false, endpoint: 'https://api.stashpad.org' };
}

installMockBackend({ settings });

/**
 * Freeze the clock at the fixtures' instant.
 *
 * Relative timestamps ("6m ago") are computed against `Date.now()`, so without this the
 * same scene captured twice produces two different images and every release ships a
 * screenshot diff that means nothing. Only the reading of "now" is frozen - timers still
 * run, or the app would never finish starting.
 */
async function freezeClock(): Promise<void> {
    const { NOW } = await import('./fixtures');
    const RealDate = Date;
    const fixed = NOW.getTime();
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const Frozen: any = function (this: unknown, ...args: unknown[]) {
        return args.length === 0
            ? new RealDate(fixed)
            : new (RealDate as unknown as new (...a: unknown[]) => Date)(...args);
    };
    Frozen.prototype = RealDate.prototype;
    Frozen.now = () => fixed;
    Frozen.parse = RealDate.parse;
    Frozen.UTC = RealDate.UTC;
    globalThis.Date = Frozen;
}

await freezeClock();
await import('../src/main');

/**
 * Mark the page ready once the app has actually painted.
 *
 * `main.ts` kicks off an async boot it does not export, so importing it resolves long
 * before there is anything to photograph. The capture script waits on this attribute;
 * without the poll it would race the first render and catch an empty window.
 */
const mounted = setInterval(() => {
    if (document.querySelector('#app main')) {
        clearInterval(mounted);
        document.documentElement.dataset.demo = 'ready';
    }
}, 50);
