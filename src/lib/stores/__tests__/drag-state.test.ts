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

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { findCardDropTarget, findEditorAtPosition } from '../drag-state.svelte';

/**
 * A queue card with an editor open on it, beside a plain card - the layout where a drop
 * into the card's editor used to land in the main editor as well.
 */
function buildQueue() {
    document.body.innerHTML = `
        <div data-editor-root id="main-editor"><textarea id="main-text"></textarea></div>
        <div data-stash-id="editing">
            <div data-editor-root id="card-editor"><textarea id="card-text"></textarea></div>
        </div>
        <div data-stash-id="plain"><span id="plain-text">a stash</span></div>
        <div id="elsewhere"></div>
    `;
}

/** jsdom has no layout, so `elementFromPoint` answers with whatever id the test points at. */
let pointedAt: string | null = null;
let lastPoint: [number, number] | null = null;
const originalElementFromPoint = document.elementFromPoint;

beforeEach(() => {
    buildQueue();
    document.elementFromPoint = (x: number, y: number) => {
        lastPoint = [x, y];
        return pointedAt ? document.getElementById(pointedAt) : null;
    };
});

afterEach(() => {
    document.elementFromPoint = originalElementFromPoint;
    pointedAt = null;
    lastPoint = null;
    vi.unstubAllGlobals();
});

describe('drop targeting', () => {
    it('gives a drop inside a card editor to that editor, not to the card', () => {
        pointedAt = 'card-text';
        expect(findEditorAtPosition(0, 0)?.id).toBe('card-editor');
        expect(findCardDropTarget(0, 0)).toBeNull();
    });

    it('gives a drop on a card outside any editor to the card', () => {
        pointedAt = 'plain-text';
        expect(findEditorAtPosition(0, 0)).toBeNull();
        expect(findCardDropTarget(0, 0)).toBe('plain');
    });

    it('finds the main editor for a drop inside it', () => {
        pointedAt = 'main-text';
        expect(findEditorAtPosition(0, 0)?.id).toBe('main-editor');
        expect(findCardDropTarget(0, 0)).toBeNull();
    });

    it('finds neither for a drop outside every editor and card', () => {
        pointedAt = 'elsewhere';
        expect(findEditorAtPosition(0, 0)).toBeNull();
        expect(findCardDropTarget(0, 0)).toBeNull();
    });
});

describe('drop coordinates', () => {
    function onPlatform(platform: string) {
        vi.stubGlobal('navigator', { ...navigator, platform });
        vi.stubGlobal('devicePixelRatio', 2);
    }

    it('uses macOS positions as they come - they are already CSS pixels', () => {
        onPlatform('MacIntel');
        findEditorAtPosition(204, 417);
        expect(lastPoint).toEqual([204, 417]);
    });

    it('scales Windows positions, which are device pixels', () => {
        onPlatform('Win32');
        findEditorAtPosition(204, 418);
        expect(lastPoint).toEqual([102, 209]);
    });
});
