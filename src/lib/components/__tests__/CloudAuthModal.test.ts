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

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import { readable } from 'svelte/store';

const { openUrl } = vi.hoisted(() => ({
    openUrl: vi.fn(() => Promise.resolve()),
}));

vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl }));
vi.mock('@tauri-apps/plugin-deep-link', () => ({
    onOpenUrl: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock('$lib/i18n', () => ({
    _: readable((key: string) => key),
}));
vi.mock('$lib/services/desktop-adapter', () => ({
    DesktopStorageAdapter: vi.fn(),
}));
vi.mock('$lib/services/cloud-sync', () => ({
    getOrCreateDeviceId: vi.fn(),
}));

import CloudAuthModal from '../CloudAuthModal.svelte';

function settingsWithEndpoint() {
    return { cloudConfig: { endpoint: 'https://api.stashpad.org' } } as never;
}

describe('CloudAuthModal', () => {
    beforeEach(() => {
        openUrl.mockClear();
    });

    // Opening the dialog wrote a fresh nonce and read it back in the same effect, which
    // reran the effect until Svelte gave up: hundreds of browser tabs and a frozen dialog.
    it('opens the browser exactly once when the dialog opens', () => {
        const component = mount(CloudAuthModal, {
            target: document.body,
            props: {
                open: true,
                settings: settingsWithEndpoint(),
                onSuccess: () => {},
                onCancel: () => {},
            },
        });
        flushSync();

        expect(openUrl).toHaveBeenCalledTimes(1);
        expect(openUrl.mock.calls[0][0]).toMatch(/action=link-desktop&nonce=[0-9a-f-]{36}$/);

        unmount(component);
    });

    it('does not open the browser while the dialog is closed', () => {
        const component = mount(CloudAuthModal, {
            target: document.body,
            props: {
                open: false,
                settings: settingsWithEndpoint(),
                onSuccess: () => {},
                onCancel: () => {},
            },
        });
        flushSync();

        expect(openUrl).not.toHaveBeenCalled();

        unmount(component);
    });
});
