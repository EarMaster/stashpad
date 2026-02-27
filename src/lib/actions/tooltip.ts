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

/**
 * Custom tooltip action with auto-positioning and styling.
 * Uses the title attribute for content and prevents native browser tooltips.
 */

export interface TooltipOptions {
    /** Delay in ms before showing tooltip (default: 200) */
    delay?: number;
    /** Position preference: 'top' | 'bottom' | 'left' | 'right' (default: 'top') */
    position?: "top" | "bottom" | "left" | "right";
}

/**
 * Svelte action for tooltips.
 * Gets content from the element's title attribute.
 *
 * Usage: <button use:tooltip title="Hello">Click</button>
 */
export function tooltip(
    element: HTMLElement,
    options: TooltipOptions = {},
): { destroy: () => void } {
    const delay = options.delay ?? 200;
    let preferredPosition = options.position ?? "top";

    let tooltipEl: HTMLDivElement | null = null;
    let arrowEl: HTMLDivElement | null = null;
    let showTimeout: ReturnType<typeof setTimeout> | null = null;
    let title = "";
    let observer: MutationObserver | null = null;

    // Store and clear the title to prevent native tooltip
    function updateTitle() {
        const titleAttr = element.getAttribute("title");
        if (titleAttr && titleAttr !== "") {
            title = titleAttr;
            element.removeAttribute("title");
            // Also set to empty to ensure native tooltip doesn't show
            element.setAttribute("title", "");
            // Then remove it completely
            setTimeout(() => {
                if (element.getAttribute("title") === "") {
                    element.removeAttribute("title");
                }
            }, 0);
        } else if (titleAttr === "") {
            // Already handled or empty
        } else {
            // Title removed completely
            title = "";
        }
    }

    // Initialize observer to watch for title changes
    function initObserver() {
        observer = new MutationObserver((mutations) => {
            for (const mutation of mutations) {
                if (mutation.attributeName === "title") {
                    const newTitle = element.getAttribute("title");
                    // Only update if it's not the empty/hidden title we just set
                    if (newTitle !== null && newTitle !== "") {
                        updateTitle();
                        // If tooltip is already showing, update its content
                        if (tooltipEl) {
                            tooltipEl.textContent = title;
                            // Re-append arrow
                            if (arrowEl) tooltipEl.appendChild(arrowEl);
                            positionTooltip();
                        }
                    }
                }
            }
        });

        observer.observe(element, { attributes: true, attributeFilter: ["title"] });
    }

    // Create tooltip element
    function createTooltip() {
        // Clean up any orphaned tooltips from previous instances
        const existingTooltips = document.querySelectorAll(".tooltip");
        existingTooltips.forEach((el) => el.remove());

        tooltipEl = document.createElement("div");
        tooltipEl.className = "tooltip";
        tooltipEl.textContent = title;
        tooltipEl.setAttribute("role", "tooltip");

        arrowEl = document.createElement("div");
        arrowEl.className = "tooltip-arrow";
        tooltipEl.appendChild(arrowEl);

        // Check if the element is inside a dialog (top-layer)
        // If so, append to the dialog to participate in its stacking context
        const dialog = element.closest("dialog");
        if (dialog) {
            dialog.appendChild(tooltipEl);
        } else {
            document.body.appendChild(tooltipEl);
        }
    }

    // Position tooltip with auto-repositioning if needed
    function positionTooltip() {
        if (!tooltipEl || !arrowEl) return;

        const rect = element.getBoundingClientRect();
        const tooltipRect = tooltipEl.getBoundingClientRect();
        const padding = 8; // Gap between element and tooltip
        const viewportPadding = 8; // Min distance from viewport edge

        let top = 0;
        let left = 0;
        let position = preferredPosition;

        // Calculate position
        switch (position) {
            case "top":
                top = rect.top - tooltipRect.height - padding;
                left = rect.left + rect.width / 2 - tooltipRect.width / 2;
                // If would overflow top, flip to bottom
                if (top < viewportPadding) {
                    position = "bottom";
                    top = rect.bottom + padding;
                }
                break;
            case "bottom":
                top = rect.bottom + padding;
                left = rect.left + rect.width / 2 - tooltipRect.width / 2;
                // If would overflow bottom, flip to top
                if (top + tooltipRect.height > window.innerHeight - viewportPadding) {
                    position = "top";
                    top = rect.top - tooltipRect.height - padding;
                }
                break;
            case "left":
                top = rect.top + rect.height / 2 - tooltipRect.height / 2;
                left = rect.left - tooltipRect.width - padding;
                // If would overflow left, flip to right
                if (left < viewportPadding) {
                    position = "right";
                    left = rect.right + padding;
                }
                break;
            case "right":
                top = rect.top + rect.height / 2 - tooltipRect.height / 2;
                left = rect.right + padding;
                // If would overflow right, flip to left
                if (left + tooltipRect.width > window.innerWidth - viewportPadding) {
                    position = "left";
                    left = rect.left - tooltipRect.width - padding;
                }
                break;
        }

        // Store the ideal left position (before clamping) to calculate arrow offset
        const idealLeft = left;

        // Clamp to viewport horizontally
        if (left < viewportPadding) {
            left = viewportPadding;
        } else if (left + tooltipRect.width > window.innerWidth - viewportPadding) {
            left = window.innerWidth - tooltipRect.width - viewportPadding;
        }

        // Calculate arrow offset for top/bottom positions
        // The arrow should point to the center of the trigger element
        let arrowOffset = 0;
        if (position === "top" || position === "bottom") {
            // Calculate how much we shifted the tooltip
            const shift = left - idealLeft;
            // The arrow needs to shift in the opposite direction to point at the element
            arrowOffset = -shift;
        }

        // Add scroll offset
        top += window.scrollY;
        left += window.scrollX;

        tooltipEl.style.top = `${top}px`;
        tooltipEl.style.left = `${left}px`;

        // Update arrow position class and offset
        arrowEl.className = `tooltip-arrow arrow-${position}`;
        if (position === "top" || position === "bottom") {
            arrowEl.style.left = arrowOffset !== 0
                ? `calc(50% + ${arrowOffset}px)`
                : "50%";
        } else {
            arrowEl.style.left = "";
        }
    }

    function show() {
        if (!title) return;

        showTimeout = setTimeout(() => {
            createTooltip();
            // Need to wait for the next frame to get accurate dimensions
            requestAnimationFrame(() => {
                positionTooltip();
                if (tooltipEl) {
                    tooltipEl.classList.add("visible");
                }
            });
        }, delay);
    }

    function hide() {
        if (showTimeout) {
            clearTimeout(showTimeout);
            showTimeout = null;
        }
        if (tooltipEl) {
            tooltipEl.remove();
            tooltipEl = null;
            arrowEl = null;
        }
    }

    // Initialize
    updateTitle();
    initObserver();

    // Event listeners
    element.addEventListener("mouseenter", show);
    element.addEventListener("mouseleave", hide);
    element.addEventListener("focus", show);
    element.addEventListener("blur", hide);

    return {
        destroy() {
            hide();
            if (observer) observer.disconnect();
            element.removeEventListener("mouseenter", show);
            element.removeEventListener("mouseleave", hide);
            element.removeEventListener("focus", show);
            element.removeEventListener("blur", hide);

            // Restore the title attribute
            if (title) {
                element.setAttribute("title", title);
            }
        },
    };
}
