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
 * The markdown the editor toolbar and its keyboard shortcuts insert.
 *
 * Pure functions over (text, selection): they take the document and where the selection
 * is, and return the new document and where the selection should end up. Nothing here
 * touches the DOM, which is the point - this logic lived inside `Editor.svelte` with no
 * test covering it, and the bugs it grew were all about what a *multi-line* selection
 * should turn into.
 */

/** A document and where the selection sits in it, after a formatting action. */
export interface FormatResult {
    content: string;
    selectionStart: number;
    selectionEnd: number;
}

/**
 * A blank line, captured so `split` keeps the separators and a rewrite can put them back
 * exactly as they were. Even indices of the result are blocks, odd ones separators.
 */
const BLANK_LINE = /(\n(?:[ \t]*\n)+)/;

/** Leading list bullet, ordered marker or ATX heading on a line. */
const LINE_MARKER = /^(?:[-*+]\s|\d+\.\s|#{1,6}\s)/;

/** An ordered-list prefix such as `1. `, which is renumbered rather than repeated. */
const ORDERED_PREFIX = /^\d+\.\s$/;

/** An ordered-list marker already on a line. */
const ORDERED_MARKER = /^\d+\.\s/;

/** Split off the whitespace at each end, so markers never wrap it. */
function splitEdges(selection: string): { lead: string; core: string; trail: string } {
    const lead = selection.slice(0, selection.length - selection.trimStart().length);
    const trail = selection.slice(selection.trimEnd().length);
    const core = selection.slice(lead.length, selection.length - trail.length);
    return { lead, core, trail };
}

/**
 * Wrap a selection in `prefix`/`suffix` - bold, italic, a link, inline code.
 *
 * Two things a naive `prefix + selection + suffix` gets wrong, both of which happen the
 * moment a selection covers more than a word:
 *
 * - Selecting a line usually takes its trailing newline with it, and `**line\n**` puts
 *   the closing marker on the next line, where it renders as literal asterisks. The
 *   whitespace at either end is left outside the markers.
 * - Emphasis does not survive a blank line: `**a\n\nb**` is two paragraphs with stray
 *   asterisks, not bold text. Each paragraph in the selection is wrapped separately.
 *
 * `placeholder` names a run inside `suffix` to leave selected afterwards, so the link
 * button can drop the caret on `url`.
 */
export function applyWrap(
    content: string,
    start: number,
    end: number,
    prefix: string,
    suffix: string,
    placeholder?: string
): FormatResult {
    const selection = content.slice(start, end);
    const { lead, core, trail } = splitEdges(selection);

    // Nothing but whitespace to mark up: drop an empty pair in and put the caret between
    // the markers, ready to type. The whitespace itself is preserved, not swallowed.
    if (!core) {
        const caret = start + prefix.length;
        return {
            content: content.slice(0, start) + prefix + suffix + selection + content.slice(end),
            selectionStart: caret,
            selectionEnd: caret,
        };
    }

    const parts = core.split(BLANK_LINE);
    const wrapped = parts
        .map((part, i) => {
            // Odd indices are the blank-line separators; they stay as they were.
            if (i % 2 === 1 || !part.trim()) return part;
            const edges = splitEdges(part);
            return edges.lead + prefix + edges.core + suffix + edges.trail;
        })
        .join('');

    const contentStart = start + lead.length;
    const result =
        content.slice(0, contentStart) + wrapped + content.slice(end - trail.length);

    // A single block keeps the old behaviour: the text stays selected inside its new
    // markers. Across paragraphs there is no single "inside", so the whole run is
    // selected instead.
    if (parts.length === 1) {
        const innerStart = contentStart + prefix.length;
        if (placeholder && suffix.includes(placeholder)) {
            const at = innerStart + core.length + suffix.indexOf(placeholder);
            return { content: result, selectionStart: at, selectionEnd: at + placeholder.length };
        }
        return {
            content: result,
            selectionStart: innerStart,
            selectionEnd: innerStart + core.length,
        };
    }

    return {
        content: result,
        selectionStart: contentStart,
        selectionEnd: contentStart + wrapped.length,
    };
}

/**
 * The longest fence that cannot be closed early by the code itself.
 *
 * A body containing its own ``` line would otherwise end the block halfway through, so
 * the fence grows to one backtick longer than the longest run the body opens a line with.
 */
function fenceFor(code: string): string {
    let longest = 0;
    for (const line of code.split('\n')) {
        const run = /^\s*(`+)/.exec(line);
        if (run && run[1].length > longest) longest = run[1].length;
    }
    return '`'.repeat(Math.max(3, longest + 1));
}

/**
 * Code formatting, which is inline or fenced depending on what is selected.
 *
 * Wrapping several lines in single backticks does not produce a code block - it produces
 * one inline span running across the line breaks, and any blank line inside it ends the
 * span entirely and leaves the backticks visible. A selection spanning lines gets a
 * fenced block, which is what the button is asking for.
 */
export function applyCode(content: string, start: number, end: number): FormatResult {
    const selection = content.slice(start, end);
    const { lead, core, trail } = splitEdges(selection);

    if (!core.includes('\n')) {
        return applyWrap(content, start, end, '`', '`');
    }

    const fence = fenceFor(core);

    // The fence has to own its line at both ends, so a newline is added only where there
    // is something else on the line already.
    const before = content.slice(0, start) + lead;
    const after = trail + content.slice(end);
    const openNewline = before === '' || before.endsWith('\n') ? '' : '\n';
    const closeNewline = after === '' || after.startsWith('\n') ? '' : '\n';

    const block = `${openNewline}${fence}\n${core}\n${fence}${closeNewline}`;
    const innerStart = before.length + openNewline.length + fence.length + 1;

    return {
        content: before + block + after,
        selectionStart: innerStart,
        selectionEnd: innerStart + core.length,
    };
}

/**
 * Add or remove a line prefix across every line the selection touches - heading, bullet
 * list, ordered list. Pressing it again on lines that all carry the prefix removes it.
 */
export function applyLinePrefix(
    content: string,
    start: number,
    end: number,
    prefix: string
): FormatResult {
    const lineStart = content.lastIndexOf('\n', start - 1) + 1;
    let lineEnd = content.indexOf('\n', end);
    if (lineEnd === -1) lineEnd = content.length;

    // A selection dragged to the start of the next line should not pull that line in.
    if (end > start && content[end - 1] === '\n') {
        lineEnd = end - 1;
    }

    const selected = content.slice(lineStart, lineEnd);
    const lines = selected.split('\n');
    const ordered = ORDERED_PREFIX.test(prefix);

    const has = (line: string) =>
        ordered ? ORDERED_MARKER.test(line) : line.startsWith(prefix);

    // Blank lines are ignored throughout: prefixing one produces an empty list item or a
    // bare `###`, and counting it would stop a mixed selection ever reading as "all
    // prefixed" and so make the button impossible to toggle off.
    const filled = lines.filter((line) => line.trim());
    const allPrefixed = filled.length > 0 && filled.every(has);

    let counter = 0;
    const newLines = lines.map((line) => {
        if (!line.trim()) return line;
        if (allPrefixed) {
            return line.replace(ordered ? ORDERED_MARKER : new RegExp(`^${escapeRe(prefix)}`), '');
        }
        const bare = line.replace(LINE_MARKER, '');
        // Numbered in sequence rather than a row of `1.`. Both render as 1, 2, 3, but the
        // stash text itself is what gets handed to an AI tool, and it should read right.
        return ordered ? `${++counter}. ${bare}` : prefix + bare;
    });

    const replacement = newLines.join('\n');
    const result = content.slice(0, lineStart) + replacement + content.slice(lineEnd);

    // A bare caret keeps its place in the line rather than jumping; a real selection grows
    // or shrinks to cover exactly the lines that were rewritten.
    if (start === end) {
        const moved = Math.max(0, start + (replacement.length - selected.length));
        return { content: result, selectionStart: moved, selectionEnd: moved };
    }

    return {
        content: result,
        selectionStart: lineStart,
        selectionEnd: lineStart + replacement.length,
    };
}

/** Escape a literal for use inside a RegExp. */
function escapeRe(value: string): string {
    return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}
