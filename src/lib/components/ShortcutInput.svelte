<!--
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
-->

<script lang="ts">
    import { _ } from "$lib/i18n";
    import { tooltip } from "$lib/actions/tooltip";
    /**
     * ShortcutInput Component
     *
     * A keyboard shortcut recorder that captures key combinations when focused.
     * Displays shortcuts as styled key badges and outputs Tauri-compatible format.
     *
     * Usage:
     *   <ShortcutInput
     *     value="CommandOrControl+P"
     *     placeholder="Press shortcut…
     *     onchange={(shortcut) => console.log(shortcut)}
     *   />
     */

    interface Props {
        /** Current shortcut value in Tauri format (e.g., "CommandOrControl+Shift+K") */
        value: string;
        /** Placeholder text shown when no shortcut is set */
        placeholder?: string;
        /** Callback when shortcut changes */
        onchange?: (shortcut: string) => void;
    }

    let {
        value = "",
        placeholder = "Click to set…",
        onchange,
    }: Props = $props();

    // Component state
    let isRecording = $state(false);
    let inputElement: HTMLButtonElement | null = $state(null);

    /**
     * Maps browser key values to Tauri-compatible key names.
     * Tauri uses specific key names for global shortcuts.
     */
    const KEY_MAP: Record<string, string> = {
        // Function keys
        F1: "F1",
        F2: "F2",
        F3: "F3",
        F4: "F4",
        F5: "F5",
        F6: "F6",
        F7: "F7",
        F8: "F8",
        F9: "F9",
        F10: "F10",
        F11: "F11",
        F12: "F12",

        // Navigation keys
        ArrowUp: "Up",
        ArrowDown: "Down",
        ArrowLeft: "Left",
        ArrowRight: "Right",
        Home: "Home",
        End: "End",
        PageUp: "PageUp",
        PageDown: "PageDown",

        // Editing keys
        Backspace: "Backspace",
        Delete: "Delete",
        Insert: "Insert",
        Tab: "Tab",
        Enter: "Enter",
        Escape: "Escape",

        // Special keys
        " ": "Space",
        CapsLock: "CapsLock",
        NumLock: "NumLock",
        ScrollLock: "ScrollLock",
        PrintScreen: "PrintScreen",
        Pause: "Pause",
    };

    /**
     * Maps browser event.code values to Tauri-compatible key names.
     * Used for keys where event.key doesn't provide enough info (like numpad).
     */
    const CODE_MAP: Record<string, string> = {
        // Numpad keys - these need code detection since key value is ambiguous
        NumpadAdd: "NumpadAdd",
        NumpadSubtract: "NumpadSubtract",
        NumpadMultiply: "NumpadMultiply",
        NumpadDivide: "NumpadDivide",
        NumpadDecimal: "NumpadDecimal",
        NumpadEnter: "NumpadEnter",
        Numpad0: "Numpad0",
        Numpad1: "Numpad1",
        Numpad2: "Numpad2",
        Numpad3: "Numpad3",
        Numpad4: "Numpad4",
        Numpad5: "Numpad5",
        Numpad6: "Numpad6",
        Numpad7: "Numpad7",
        Numpad8: "Numpad8",
        Numpad9: "Numpad9",
        // Regular minus/plus for distinction
        Minus: "Minus",
        Equal: "Plus",
    };

    /**
     * Display names for keys (more user-friendly).
     * Used for rendering key badges.
     */
    const DISPLAY_MAP: Record<string, string> = {
        CommandOrControl: navigator.platform.includes("Mac") ? "⌘" : "Ctrl",
        Super: "Win",
        Alt: navigator.platform.includes("Mac") ? "⌥" : "Alt",
        Shift: "⇧",
        Space: "Space",
        Up: "↑",
        Down: "↓",
        Left: "←",
        Right: "→",
        Backspace: "⌫",
        Delete: "Del",
        Enter: "↵",
        Escape: "Esc",
        Tab: "⇥",
        NumpadSubtract: "Num-",
        NumpadAdd: "Num+",
        NumpadMultiply: "Num*",
        NumpadDivide: "Num/",
        NumpadDecimal: "Num.",
        NumpadEnter: "NumEnter",
        Numpad0: "Num0",
        Numpad1: "Num1",
        Numpad2: "Num2",
        Numpad3: "Num3",
        Numpad4: "Num4",
        Numpad5: "Num5",
        Numpad6: "Num6",
        Numpad7: "Num7",
        Numpad8: "Num8",
        Numpad9: "Num9",
        Minus: "-",
        Plus: "+",
    };

    /**
     * Converts a KeyboardEvent to a Tauri-compatible shortcut string.
     * Properly distinguishes between Ctrl and Win/Meta keys, and handles numpad.
     */
    function eventToShortcut(event: KeyboardEvent): string | null {
        const parts: string[] = [];
        const isMac = navigator.platform.includes("Mac");

        // Add modifiers in consistent order: Super > Ctrl > Alt > Shift
        // On Windows: metaKey = Win key, ctrlKey = Ctrl key
        // On Mac: metaKey = Cmd key (we use CommandOrControl for cross-platform)
        if (event.metaKey) {
            if (isMac) {
                // On Mac, Cmd should map to CommandOrControl
                parts.push("CommandOrControl");
            } else {
                // On Windows/Linux, Meta is the Win/Super key
                parts.push("Super");
            }
        }
        if (event.ctrlKey) {
            // Ctrl key - on Mac this is separate from Cmd
            // On Windows this is the main modifier
            if (isMac) {
                // On Mac, Ctrl is a separate modifier
                parts.push("Control");
            } else {
                // On Windows, use CommandOrControl for cross-platform compatibility
                parts.push("CommandOrControl");
            }
        }
        if (event.altKey) {
            parts.push("Alt");
        }
        if (event.shiftKey) {
            parts.push("Shift");
        }

        // Get the main key (skip if it's just a modifier)
        const key = event.key;
        const code = event.code;
        const isModifier = ["Control", "Meta", "Alt", "Shift"].includes(key);

        if (isModifier) {
            // Don't record shortcuts that are just modifier keys
            return null;
        }

        // First, check if this is a numpad key (use event.code)
        let mappedKey = CODE_MAP[code];

        // If not found in CODE_MAP, try KEY_MAP
        if (!mappedKey) {
            mappedKey = KEY_MAP[key];
        }

        // If still not found, handle as regular key
        if (!mappedKey) {
            if (key.length === 1) {
                // Single character - use uppercase for letters
                mappedKey = key.toUpperCase();
            } else if (key === "Dead") {
                // On macOS, dead keys (e.g., Alt+N for ˜) report "Dead"
                // Fall back to event.code to get the physical key
                const codeMatch = code.match(/^Key([A-Z])$/);
                const digitMatch = code.match(/^Digit([0-9])$/);
                if (codeMatch) {
                    mappedKey = codeMatch[1]; // e.g., "KeyN" → "N"
                } else if (digitMatch) {
                    mappedKey = digitMatch[1]; // e.g., "Digit1" → "1"
                } else {
                    mappedKey = code; // Use raw code as last resort
                }
            } else {
                // Unknown key, use as-is
                mappedKey = key;
            }
        }

        parts.push(mappedKey);

        return parts.join("+");
    }

    /**
     * Parses a Tauri shortcut string into an array of key parts.
     */
    function parseShortcut(shortcut: string): string[] {
        if (!shortcut) return [];
        return shortcut.split("+");
    }

    /**
     * Gets the display name for a key.
     */
    function getDisplayName(key: string): string {
        return DISPLAY_MAP[key] || key;
    }

    /**
     * Handles keydown events.
     * When not recording: Enter/Space activates recording mode.
     * When recording: captures the shortcut or Escape cancels.
     */
    function handleKeyDown(event: KeyboardEvent) {
        if (!isRecording) {
            // Not recording - check for activation keys
            if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                event.stopPropagation();
                isRecording = true;
            }
            // Allow Tab and other navigation keys to work normally
            return;
        }

        // Recording mode - capture the shortcut
        event.preventDefault();
        event.stopPropagation();

        // Escape cancels recording without changing the value
        if (event.key === "Escape") {
            isRecording = false;
            return;
        }

        // Try to build a shortcut from this event
        const shortcut = eventToShortcut(event);

        if (shortcut) {
            // Valid shortcut recorded
            isRecording = false;

            // Notify parent of change
            if (onchange) {
                onchange(shortcut);
            }
        }
    }

    /**
     * Handles click on the input (activates recording).
     */
    function handleClick() {
        isRecording = true;
    }

    /**
     * Handles blur on the input (stops recording).
     */
    function handleBlur() {
        isRecording = false;
    }

    /**
     * Clears the current shortcut.
     */
    function handleClear(event: MouseEvent) {
        event.stopPropagation();
        if (onchange) {
            onchange("");
        }
    }

    // Parsed shortcut parts for display
    let shortcutParts = $derived(parseShortcut(value));
</script>

<div class="shortcut-input-container">
    <button
        bind:this={inputElement}
        type="button"
        class="shortcut-input"
        class:recording={isRecording}
        class:empty={!value}
        onclick={handleClick}
        onblur={handleBlur}
        onkeydown={handleKeyDown}
        aria-label={$_("shortcutInput.ariaLabel")}
    >
        {#if isRecording}
            <span class="recording-text"
                >{$_("settings.shortcuts.pressShortcut")}</span
            >
        {:else if shortcutParts.length > 0}
            <span class="key-badges">
                {#each shortcutParts as part, index}
                    <kbd class="key-badge">{getDisplayName(part)}</kbd>
                    {#if index < shortcutParts.length - 1}
                        <span class="key-separator">+</span>
                    {/if}
                {/each}
            </span>
        {:else}
            <span class="placeholder-text">{placeholder}</span>
        {/if}
    </button>

    {#if value && !isRecording}
        <button
            type="button"
            class="clear-button"
            onclick={handleClear}
            title={$_("settings.shortcuts.clearShortcut")}
            use:tooltip
            aria-label={$_("settings.shortcuts.clearShortcut")}
        >
            ×
        </button>
    {/if}
</div>

<style>
    .shortcut-input-container {
        display: flex;
        align-items: center;
        gap: 0.5rem;
    }

    .shortcut-input {
        display: flex;
        align-items: center;
        justify-content: center;
        min-width: 10rem;
        height: 2.25rem; /* Fixed height for consistency */
        padding: 0.375rem 0.75rem;
        border-radius: 0.375rem;
        font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas,
            monospace;
        font-size: 0.875rem;
        cursor: pointer;
        transition: all 0.15s ease;

        /* Default state - visible border for accessibility */
        background-color: var(--background);
        border: 1px solid var(--border);
        color: var(--foreground);
    }

    .shortcut-input:hover {
        background-color: var(--accent);
        border-color: var(--input);
    }

    .shortcut-input:focus {
        outline: none;
        border-color: var(--ring);
        box-shadow: 0 0 0 2px color-mix(in srgb, var(--ring), transparent 70%);
    }

    .shortcut-input:focus-visible {
        outline: 2px solid var(--ring);
        outline-offset: 2px;
    }

    .shortcut-input.recording {
        background-color: color-mix(in srgb, var(--primary), transparent 85%);
        border-color: var(--primary);
        animation: pulse 1.5s infinite;
    }

    .shortcut-input.empty {
        color: var(--muted-foreground);
    }

    .shortcut-input.recording,
    .shortcut-input.empty {
        margin-right: 2rem;
    }

    @keyframes pulse {
        0%,
        100% {
            box-shadow: 0 0 0 2px
                color-mix(in srgb, var(--primary), transparent 70%);
        }
        50% {
            box-shadow: 0 0 0 4px
                color-mix(in srgb, var(--primary), transparent 85%);
        }
    }

    .key-badges {
        display: flex;
        align-items: center;
        gap: 0.25rem;
    }

    .key-badge {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        min-width: 1.5rem;
        height: 1.5rem;
        padding: 0 0.375rem;
        border-radius: 0.25rem;
        font-size: 0.75rem;
        font-weight: 500;
        font-family: inherit;

        /* Key badge styling */
        background-color: var(--muted);
        border: 1px solid var(--border);
        box-shadow:
            0 1px 2px rgba(0, 0, 0, 0.1),
            inset 0 1px 0 rgba(255, 255, 255, 0.1);
        color: var(--foreground);
    }

    .key-separator {
        color: var(--muted-foreground);
        font-size: 0.75rem;
        margin: 0 0.125rem;
    }

    .recording-text {
        color: var(--primary);
        font-weight: 500;
        font-style: italic;
    }

    .placeholder-text {
        color: var(--muted-foreground);
        font-style: italic;
    }

    .clear-button {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 1.5rem;
        height: 1.5rem;
        padding: 0;
        border: none;
        border-radius: 0.25rem;
        background-color: transparent;
        color: var(--muted-foreground);
        font-size: 1.125rem;
        cursor: pointer;
        transition: all 0.15s ease;
    }

    .clear-button:hover {
        background-color: color-mix(
            in srgb,
            var(--destructive),
            transparent 90%
        );
        color: var(--destructive);
    }

    .clear-button:focus {
        outline: none;
        background-color: color-mix(
            in srgb,
            var(--destructive),
            transparent 90%
        );
        color: var(--destructive);
    }

    .clear-button:focus-visible {
        outline: 2px solid var(--destructive);
        outline-offset: 2px;
    }
</style>
