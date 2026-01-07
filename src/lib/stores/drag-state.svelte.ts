// SPDX-License-Identifier: AGPL-3.0-only

// Copyright (C) 2025 Nico Wiedemann
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

/**
 * Find the stash ID from a position using elementFromPoint.
 * Returns null if not over a StashCard.
 */
export function findStashAtPosition(x: number, y: number): string | null {
    // Convert physical pixels to CSS pixels (Tauri provides physical coordinates)
    const dpr = window.devicePixelRatio || 1;
    const cssX = x / dpr;
    const cssY = y / dpr;

    const element = document.elementFromPoint(cssX, cssY);
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
