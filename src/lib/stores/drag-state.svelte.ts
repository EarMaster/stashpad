// SPDX-License-Identifier: AGPL-3.0-only

// Copyright (C) 2025 Nico Wiedemann
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
 * Drag state store for coordinating Tauri drag-drop events with UI components.
 * Since Tauri's dragDropEnabled intercepts native drag events at window level,
 * we need a shared store to communicate drag state to individual components.
 */

// Current stash ID being hovered (or null if hovering Editor or nothing)
let hoveredStashId: string | null = $state(null);

// Whether a drag is currently in progress
let isDragging: boolean = $state(false);

/**
 * Set the currently hovered stash ID based on cursor position.
 * Call this from the global drag event listener with the drop target element.
 */
export function setHoveredStash(stashId: string | null) {
    hoveredStashId = stashId;
}

/**
 * Set whether a drag operation is in progress.
 */
export function setDragging(dragging: boolean) {
    isDragging = dragging;
    if (!dragging) {
        hoveredStashId = null;
    }
}

/**
 * Get the currently hovered stash ID.
 */
export function getHoveredStashId(): string | null {
    return hoveredStashId;
}

/**
 * Check if a specific stash is being hovered.
 */
export function isStashHovered(stashId: string): boolean {
    return isDragging && hoveredStashId === stashId;
}

/**
 * Check if drag is in progress.
 */
export function getIsDragging(): boolean {
    return isDragging;
}

/** Attribute marking an editor's root element, so a drop can tell which editor it hit. */
const EDITOR_ROOT_ATTR = "data-editor-root";

/**
 * The element under a drag position.
 *
 * Tauri types the position as physical pixels but passes on whatever the platform's webview
 * reported: device pixels on Windows (WebView2 goes through `ScreenToClient`), but points on
 * macOS and logical units on Linux - both already CSS pixels. Dividing by the scale factor
 * everywhere put every drop on a 2x Mac at half its real coordinates, up and to the left,
 * which is where the main editor sits.
 */
function elementAtPosition(x: number, y: number): Element | null {
    const scale = navigator.platform.startsWith("Win") ? window.devicePixelRatio || 1 : 1;
    return document.elementFromPoint(x / scale, y / scale);
}

/**
 * The editor root a drop landed in, or null when it landed outside every editor.
 *
 * Every editor hears every drop - the main one and any opened on a queue card - so this
 * is how they agree on exactly one of them taking it.
 */
export function findEditorAtPosition(x: number, y: number): Element | null {
    return elementAtPosition(x, y)?.closest(`[${EDITOR_ROOT_ATTR}]`) ?? null;
}

/**
 * Find the stash ID from a position using elementFromPoint.
 * Returns null if not over a StashCard.
 */
export function findStashAtPosition(x: number, y: number): string | null {
    const element = elementAtPosition(x, y);
    if (!element) return null;

    // Walk up the DOM tree to find element with data-stash-id
    let current: Element | null = element;
    while (current) {
        const stashId = current.getAttribute('data-stash-id');
        if (stashId) {
            return stashId;
        }
        current = current.parentElement;
    }
    return null;
}

/**
 * The stash card a drop adds straight to: a card under the position, unless the position
 * is inside an editor - an editor opened on a card takes its own drops.
 */
export function findCardDropTarget(x: number, y: number): string | null {
    return findEditorAtPosition(x, y) ? null : findStashAtPosition(x, y);
}
