// SPDX-License-Identifier: AGPL-3.0-only

// Copyright (C) 2025-2026 Nico Wiedemann
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

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { DesktopStorageAdapter } from '../desktop-adapter';
import type { StashItem, Context, Settings, Attachment } from '$lib/types';

// Mock the Tauri API
vi.mock('@tauri-apps/api/core', () => ({
    invoke: vi.fn(),
}));

import { invoke } from '@tauri-apps/api/core';

describe('DesktopStorageAdapter', () => {
    let adapter: DesktopStorageAdapter;
    let mockInvoke: ReturnType<typeof vi.fn>;

    beforeEach(() => {
        adapter = new DesktopStorageAdapter();
        mockInvoke = invoke as ReturnType<typeof vi.fn>;
        mockInvoke.mockClear();
    });

    describe('saveStash', () => {
        it('should call save_stash command with correct parameters', async () => {
            const stash: StashItem = {
                id: 'test-id',
                content: 'test content',
                attachments: [],
                createdAt: '2026-01-08T00:00:00Z',
            };

            mockInvoke.mockResolvedValue(undefined);
            await adapter.saveStash(stash);

            expect(mockInvoke).toHaveBeenCalledWith('save_stash', {
                options: { stash, invertPosition: false },
            });
        });

        it('should pass invertPosition option when provided', async () => {
            const stash: StashItem = {
                id: 'test-id',
                content: 'test content',
                attachments: [],
                createdAt: '2026-01-08T00:00:00Z',
            };

            mockInvoke.mockResolvedValue(undefined);
            await adapter.saveStash(stash, { invertPosition: true });

            expect(mockInvoke).toHaveBeenCalledWith('save_stash', {
                options: { stash, invertPosition: true },
            });
        });
    });

    describe('loadStashes', () => {
        it('should call load_stashes command and return stashes', async () => {
            const mockStashes: StashItem[] = [
                {
                    id: '1',
                    content: 'stash 1',
                    attachments: [],
                    createdAt: '2026-01-08T00:00:00Z',
                },
                {
                    id: '2',
                    content: 'stash 2',
                    attachments: [],
                    createdAt: '2026-01-08T01:00:00Z',
                },
            ];

            mockInvoke.mockResolvedValue(mockStashes);
            const result = await adapter.loadStashes();

            expect(mockInvoke).toHaveBeenCalledWith('load_stashes');
            expect(result).toEqual(mockStashes);
        });
    });

    describe('saveAsset', () => {
        it('should convert File to byte array and call save_asset', async () => {
            const fileContent = 'test content';
            const mockFile = new File([fileContent], 'test.txt', { type: 'text/plain' });

            // Mock arrayBuffer since jsdom's File doesn't support it
            mockFile.arrayBuffer = vi.fn().mockResolvedValue(
                new TextEncoder().encode(fileContent).buffer
            );

            const mockAttachment: Attachment = {
                id: 'attachment-id',
                stashId: 'stash-id',
                filePath: '/path/to/test.txt',
                fileName: 'test.txt',
                fileSize: 12,
                mimeType: 'text/plain',
                createdAt: '2026-01-08T00:00:00Z',
            };

            mockInvoke.mockResolvedValue(mockAttachment);

            const result = await adapter.saveAsset(mockFile, 'context-id', 'stash-id', 'plaintext');

            expect(mockInvoke).toHaveBeenCalledWith('save_asset', {
                name: 'test.txt',
                data: expect.any(Array),
                context_id: 'context-id',
                contextId: 'context-id',
                stash_id: 'stash-id',
                stashId: 'stash-id',
                syntax: 'plaintext',
            });
            expect(result).toEqual(mockAttachment);
        });

        it('should pass null for optional parameters when not provided', async () => {
            const mockFile = new File(['test'], 'test.txt');

            // Mock arrayBuffer
            mockFile.arrayBuffer = vi.fn().mockResolvedValue(
                new TextEncoder().encode('test').buffer
            );

            mockInvoke.mockResolvedValue({} as Attachment);

            await adapter.saveAsset(mockFile);

            expect(mockInvoke).toHaveBeenCalledWith('save_asset', {
                name: 'test.txt',
                data: expect.any(Array),
                context_id: null,
                contextId: null,
                stash_id: null,
                stashId: null,
                syntax: null,
            });
        });
    });

    describe('getPreviousAppInfo', () => {
        it('should call get_previous_app_info command', async () => {
            const mockAppInfo = {
                windowTitle: 'Test Project',
                processName: 'code',
            };

            mockInvoke.mockResolvedValue(mockAppInfo);
            const result = await adapter.getPreviousAppInfo();

            expect(mockInvoke).toHaveBeenCalledWith('get_previous_app_info');
            expect(result).toEqual(mockAppInfo);
        });
    });

    describe('copyToClipboard', () => {
        it('should call copy_to_clipboard with text', async () => {
            mockInvoke.mockResolvedValue(undefined);
            await adapter.copyToClipboard('test text');

            expect(mockInvoke).toHaveBeenCalledWith('copy_to_clipboard', { text: 'test text' });
        });
    });

    describe('startDrag', () => {
        it('should call start_drag with text and files', async () => {
            const files = ['/path/to/file1.png', '/path/to/file2.txt'];
            mockInvoke.mockResolvedValue(undefined);

            await adapter.startDrag('drag content', files);

            expect(mockInvoke).toHaveBeenCalledWith('start_drag', {
                text: 'drag content',
                files,
            });
        });
    });

    describe('getSettings', () => {
        it('should call get_settings and return settings', async () => {
            const mockSettings: Settings = {
                autoContextDetection: true,
                visualEffectsEnabled: true,
                shortcuts: {},
                theme: 'dark',
            };

            mockInvoke.mockResolvedValue(mockSettings);
            const result = await adapter.getSettings();

            expect(mockInvoke).toHaveBeenCalledWith('get_settings');
            expect(result).toEqual(mockSettings);
        });
    });

    describe('saveSettings', () => {
        it('should call save_settings with settings object', async () => {
            const settings: Settings = {
                autoContextDetection: false,
                shortcuts: { toggle: 'Ctrl+Shift+S' },
            };

            mockInvoke.mockResolvedValue(undefined);
            await adapter.saveSettings(settings);

            expect(mockInvoke).toHaveBeenCalledWith('save_settings', { settings });
        });
    });

    describe('deleteStash', () => {
        it('should call delete_stash with stash id', async () => {
            mockInvoke.mockResolvedValue(undefined);
            await adapter.deleteStash('stash-123');

            expect(mockInvoke).toHaveBeenCalledWith('delete_stash', { id: 'stash-123' });
        });
    });

    describe('getContexts', () => {
        it('should call get_contexts and return contexts', async () => {
            const mockContexts: Context[] = [
                {
                    id: 'default',
                    name: 'Default',
                    rules: [],
                },
                {
                    id: 'project-1',
                    name: 'Project 1',
                    rules: [{ ruleType: 'process', value: 'code', matchType: 'exact' }],
                },
            ];

            mockInvoke.mockResolvedValue(mockContexts);
            const result = await adapter.getContexts();

            expect(mockInvoke).toHaveBeenCalledWith('get_contexts');
            expect(result).toEqual(mockContexts);
        });
    });

    describe('saveContext', () => {
        it('should call save_context with context object', async () => {
            const context: Context = {
                id: 'new-context',
                name: 'New Context',
                rules: [{ ruleType: 'title', value: 'test', matchType: 'contains' }],
            };

            mockInvoke.mockResolvedValue(undefined);
            await adapter.saveContext(context);

            expect(mockInvoke).toHaveBeenCalledWith('save_context', { context });
        });
    });

    describe('deleteContext', () => {
        it('should call delete_context with context id', async () => {
            mockInvoke.mockResolvedValue(undefined);
            await adapter.deleteContext('context-123');

            expect(mockInvoke).toHaveBeenCalledWith('delete_context', { id: 'context-123' });
        });
    });

    describe('setAutostart', () => {
        it('should call set_autostart with enabled flag', async () => {
            mockInvoke.mockResolvedValue(undefined);
            await adapter.setAutostart(true);

            expect(mockInvoke).toHaveBeenCalledWith('set_autostart', { enabled: true });
        });
    });

    describe('isWindows10', () => {
        it('should call is_windows_10 and return boolean', async () => {
            mockInvoke.mockResolvedValue(true);
            const result = await adapter.isWindows10();

            expect(mockInvoke).toHaveBeenCalledWith('is_windows_10');
            expect(result).toBe(true);
        });
    });

    describe('error handling', () => {
        it('should propagate errors from invoke', async () => {
            mockInvoke.mockRejectedValue(new Error('Backend error'));

            await expect(adapter.loadStashes()).rejects.toThrow('Backend error');
        });
    });
});
