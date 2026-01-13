/*
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
*/

/**
 * Action to render an element in a different part of the DOM (Portal)
 * Defaults to appending to document.body
 */
export function portal(node: HTMLElement, target: HTMLElement | string = "body") {
    async function update(newTarget: HTMLElement | string) {
        let targetEl: HTMLElement | null = null;

        if (typeof newTarget === 'string') {
            targetEl = document.querySelector(newTarget);
            if (newTarget === 'body') targetEl = document.body;
        } else {
            targetEl = newTarget;
        }

        if (targetEl) {
            targetEl.appendChild(node);
            node.hidden = false;
        }
    }

    update(target);

    return {
        update,
        destroy() {
            if (node.parentNode) {
                node.parentNode.removeChild(node);
            }
        }
    };
}
