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
 * A stand-in for the Rust backend, good enough to photograph.
 *
 * The desktop app reaches the backend through exactly one door - `invoke()` in
 * `desktop-adapter.ts` - which is what makes this possible at all: replace the IPC
 * transport underneath and the entire UI runs in an ordinary browser, unmodified. That
 * is the adapter pattern paying for itself, so keep it intact.
 *
 * This is a demo prop, not a second implementation of the backend. It answers the
 * commands the UI calls while a screenshot is being taken, holds writes in memory so
 * the app feels alive, and answers everything else with a benign default rather than
 * throwing - an unhandled rejection during startup would leave the splash screen up.
 */

import type { InvokeArgs } from '@tauri-apps/api/core';
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import type { Attachment, Context, Settings, StashItem } from '../src/lib/types';
import * as fixtures from './fixtures';

/** Commands whose answer is "nothing happened, carry on". */
const NO_OP = new Set([
    'save_settings',
    'save_stash',
    'save_stashes',
    'save_context',
    'save_contexts',
    'delete_stash',
    'delete_context',
    'delete_asset',
    'delete_completed_stashes',
    'copy_to_clipboard',
    'start_drag',
    'show_in_folder',
    'set_autostart',
    'connect_websocket',
    'disconnect_websocket',
    'log_frontend_error',
    'mark_stashes_synced',
    'mark_contexts_synced',
    'mark_positions_synced',
    'import_stashes',
    'import_contexts',
    'cloud_logout',
    'create_system_prompt_file',
    'open_system_prompt_file',
    'open_macos_screen_recording_settings',
    'discard_import',
]);

/**
 * The demo's mutable state.
 *
 * Deep-copied from the fixtures on every boot so that a scene which types, completes or
 * deletes something cannot leak into the next scene through the module cache.
 */
interface DemoState {
    stashes: StashItem[];
    contexts: Context[];
    settings: Settings;
}

function freshState(): DemoState {
    return structuredClone({
        stashes: fixtures.stashes,
        contexts: fixtures.contexts,
        settings: fixtures.settings,
    });
}

/**
 * Serve a fixture attachment from the dev server instead of the `asset:` protocol.
 *
 * The real `convertFileSrc` hands the webview a custom-protocol URL that only Tauri can
 * answer. Fixture paths are rewritten to `/screenshots/assets/<file>`, which Vite serves
 * from this folder, so image previews and thumbnails actually render.
 */
function installConvertFileSrc(): void {
    const internals = (window as unknown as Record<string, Record<string, unknown>>)
        .__TAURI_INTERNALS__;
    internals.convertFileSrc = (filePath: string) => {
        const fileName = String(filePath).split(/[\\/]/).pop() ?? '';
        return `/screenshots/assets/${fileName}`;
    };
}

/**
 * Install the fake backend. Call before anything imports the app itself.
 *
 * @param overrides - Per-scene tweaks to the boot state, e.g. a signed-out cloud config.
 */
export function installMockBackend(overrides: Partial<DemoState> = {}): void {
    const state: DemoState = { ...freshState(), ...structuredClone(overrides) };

    mockWindows('main');
    mockIPC(handle, { shouldMockEvents: true });
    installConvertFileSrc();

    function handle(cmd: string, args?: InvokeArgs): unknown {
        // Every command this app sends carries a named-argument object, never the raw
        // byte array the type also allows.
        const payload = args as Record<string, unknown> | undefined;
        // Window, event, dialog and updater plugins all arrive here namespaced. None of
        // them has anything to do on a web page; answering `null` keeps their promises
        // resolved, which is all the UI waits on.
        if (cmd.startsWith('plugin:')) return pluginDefault(cmd);

        switch (cmd) {
            case 'get_settings':
                return state.settings;
            case 'get_contexts':
            case 'get_contexts_for_sync':
                return state.contexts;
            case 'load_stashes':
            case 'load_stashes_for_sync':
                return state.stashes;
            case 'get_previous_app_info':
                return fixtures.previousApp;
            case 'get_smart_transfer_target':
                return 'GUI';
            case 'get_device_name':
                return 'workstation';
            case 'get_device_id':
                return 'device-demo';
            case 'get_installation_source':
                return 'github';
            case 'is_windows_10':
                return false;
            case 'get_autostart_enabled':
                return state.settings.autostart ?? false;
            case 'trigger_auto_cleanup':
                return 0;
            case 'fetch_cloud_usage':
                return {
                    stashes: state.stashes.length,
                    contexts: state.contexts.length,
                    attachments: fixtures.cloudUsage.attachmentCount,
                    attachmentBytes: fixtures.cloudUsage.usedBytes,
                    quotaBytes: fixtures.cloudUsage.quotaBytes,
                    overQuota: false,
                };
            case 'fetch_cloud_account':
                return state.settings.cloudConfig;
            case 'read_clipboard_text':
                return '';
            case 'read_file_for_preview':
                return previewFor(String(payload?.path ?? ''));
            case 'save_asset':
            case 'save_asset_from_path':
                return newAttachment(payload);
            case 'claim_pending_stashes':
            case 'claim_pending_positions':
            case 'claim_pending_contexts':
                return [];
            case 'import_positions':
                return 0;
            // A sync that succeeds and brings nothing back. The shape matters: the
            // header shows a red cloud for a failed sync, so a sloppy answer here is
            // visible in every screenshot.
            case 'sync_stashes_api':
                return {
                    synced: [],
                    serverTime: fixtures.NOW.toISOString(),
                    rejected: [],
                    partial: true,
                    positions: [],
                };
            case 'sync_contexts_api':
                return {
                    synced: [],
                    serverTime: fixtures.NOW.toISOString(),
                    rejected: [],
                };
            case 'upload_attachment_to_cloud':
                return false;
            case 'check_screen_recording_permission':
                return true;
            case 'check_apple_intelligence_available':
                return false;
            case 'check_system_prompt_exists':
                return true;
            case 'get_system_prompt':
                return 'You are a precise engineering assistant. Rewrite the note as a task.';
            case 'get_system_prompt_path_str':
                return '/home/dev/.stashpad/system-prompt.md';
            default:
                if (NO_OP.has(cmd)) return null;
                console.warn(`[demo] unhandled command: ${cmd}`);
                return null;
        }
    }

    /**
     * Plugin commands the UI actually reads an answer from.
     *
     * The updater is the one that matters: `check()` parses this reply, and anything
     * other than a well-formed "no update" makes the header indicator light up in every
     * screenshot.
     */
    function pluginDefault(cmd: string): unknown {
        if (cmd.startsWith('plugin:updater|')) return null;
        if (cmd === 'plugin:window|is_maximized') return false;
        if (cmd === 'plugin:window|is_fullscreen') return false;
        if (cmd === 'plugin:autostart|is_enabled') return state.settings.autostart ?? false;
        if (cmd === 'plugin:os|platform') return 'linux';
        return null;
    }

    /**
     * Answer the preview modal without touching a disk.
     *
     * Text previews carry their content inline, which is why only images need a real
     * file in `screenshots/assets/` - an image is fetched by URL, everything else is
     * handed straight back from here.
     */
    function previewFor(path: string) {
        const fileName = path.split(/[\\/]/).pop() ?? '';
        const text = fixtures.previewText[fileName];
        if (text !== undefined) {
            return {
                fileType: 'text' as const,
                content: text,
                fileName,
                mimeType: 'text/plain',
                fileSize: text.length,
            };
        }
        return {
            fileType: 'image' as const,
            content: `/screenshots/assets/${fileName}`,
            fileName,
            mimeType: 'image/svg+xml',
            fileSize: 138_402,
        };
    }

    function newAttachment(payload?: Record<string, unknown>): Attachment {
        const fileName = String(payload?.fileName ?? payload?.path ?? 'pasted.txt')
            .split(/[\\/]/)
            .pop() as string;
        const contextId = String(payload?.contextId ?? 'orbit-api');
        const stashId = String(payload?.stashId ?? 'stash-new');
        return {
            id: `att-${Math.random().toString(36).slice(2, 8)}`,
            stashId,
            filePath: `${fixtures.CACHE_ROOT}/${contextId}/${stashId}/${fileName}`,
            fileName,
            fileSize: 2_048,
            mimeType: 'text/plain',
            createdAt: fixtures.NOW.toISOString(),
        };
    }
}
