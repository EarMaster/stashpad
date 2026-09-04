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

import { describe, it, expect } from 'vitest';
import { applyWrap, applyCode, applyLinePrefix } from '../markdown-format';

/**
 * Marks the selection with `|` so a case reads as the document the user is looking at.
 * `run('a |bc| d', applyCode)` selects `bc`.
 */
function at(marked: string) {
    const start = marked.indexOf('|');
    const end = marked.indexOf('|', start + 1) - 1;
    return { content: marked.replace(/\|/g, ''), start, end };
}

/** The result rendered back with `|` around the resulting selection. */
function marked(result: { content: string; selectionStart: number; selectionEnd: number }) {
    return (
        result.content.slice(0, result.selectionStart) +
        '|' +
        result.content.slice(result.selectionStart, result.selectionEnd) +
        '|' +
        result.content.slice(result.selectionEnd)
    );
}

describe('applyCode', () => {
    it('fences a selection that spans lines instead of using inline backticks', () => {
        // The reported bug: single backticks around several lines are one inline span
        // running across the breaks, not a code block.
        const { content, start, end } = at('|const a = 1;\nconst b = 2;|');
        const result = applyCode(content, start, end);

        expect(result.content).toBe('```\nconst a = 1;\nconst b = 2;\n```');
        expect(marked(result)).toBe('```\n|const a = 1;\nconst b = 2;|\n```');
    });

    it('still uses inline backticks within one line', () => {
        const { content, start, end } = at('call |process()| here');
        const result = applyCode(content, start, end);

        expect(result.content).toBe('call `process()` here');
        expect(marked(result)).toBe('call `|process()|` here');
    });

    it('gives the fence its own line when there is text around the selection', () => {
        const { content, start, end } = at('before |one\ntwo| after');
        const result = applyCode(content, start, end);

        expect(result.content).toBe('before \n```\none\ntwo\n```\n after');
    });

    it('does not add a blank line where the selection already starts one', () => {
        const { content, start, end } = at('intro\n|one\ntwo|\noutro');
        const result = applyCode(content, start, end);

        expect(result.content).toBe('intro\n```\none\ntwo\n```\noutro');
    });

    it('lengthens the fence when the code contains one of its own', () => {
        // A ``` inside the body would otherwise close the block halfway through.
        const { content, start, end } = at('|a\n```\nb|');
        const result = applyCode(content, start, end);

        expect(result.content).toBe('````\na\n```\nb\n````');
    });

    it('leaves the trailing newline of a selected line outside the markers', () => {
        // Selecting a line takes its newline; ``line\n`` renders the backticks literally.
        const { content, start, end } = at('|value|\nnext');
        const result = applyCode(content, start, end);

        expect(result.content).toBe('`value`\nnext');
    });

    it('drops an empty pair in for an empty selection and puts the caret between', () => {
        const result = applyCode('ab', 1, 1);

        expect(result.content).toBe('a``b');
        expect(result.selectionStart).toBe(2);
        expect(result.selectionEnd).toBe(2);
    });
});

describe('applyWrap', () => {
    it('keeps a trailing newline outside the markers', () => {
        // `**line\n**` puts the closing marker on the next line, where it renders as
        // literal asterisks rather than bold.
        const { content, start, end } = at('|line|\nnext');
        const result = applyWrap(content, start, end, '**', '**');

        expect(result.content).toBe('**line**\nnext');
    });

    it('wraps each paragraph separately across a blank line', () => {
        // Emphasis does not survive a paragraph break: `**a\n\nb**` is two paragraphs
        // with stray asterisks in them.
        const { content, start, end } = at('|first para\n\nsecond para|');
        const result = applyWrap(content, start, end, '**', '**');

        expect(result.content).toBe('**first para**\n\n**second para**');
    });

    it('wraps consecutive lines as one run, since a soft break keeps emphasis', () => {
        const { content, start, end } = at('|one\ntwo|');
        const result = applyWrap(content, start, end, '_', '_');

        expect(result.content).toBe('_one\ntwo_');
    });

    it('selects the placeholder in the suffix so a link url can be typed over', () => {
        const { content, start, end } = at('see |the docs| now');
        const result = applyWrap(content, start, end, '[', '](url)', 'url');

        expect(result.content).toBe('see [the docs](url) now');
        expect(marked(result)).toBe('see [the docs](|url|) now');
    });

    it('puts the caret between the markers for an empty selection', () => {
        const result = applyWrap('ab', 1, 1, '**', '**');

        expect(result.content).toBe('a****b');
        expect(result.selectionStart).toBe(3);
        expect(result.selectionEnd).toBe(3);
    });

    it('does not swallow a whitespace-only selection', () => {
        const result = applyWrap('a  b', 1, 3, '**', '**');

        expect(result.content).toBe('a****  b');
    });
});

describe('applyLinePrefix', () => {
    it('prefixes every line the selection touches', () => {
        const { content, start, end } = at('|one\ntwo\nthree|');
        const result = applyLinePrefix(content, start, end, '- ');

        expect(result.content).toBe('- one\n- two\n- three');
    });

    it('removes the prefix when every line already has it', () => {
        const { content, start, end } = at('|- one\n- two|');
        const result = applyLinePrefix(content, start, end, '- ');

        expect(result.content).toBe('one\ntwo');
    });

    it('numbers an ordered list in sequence rather than repeating 1.', () => {
        const { content, start, end } = at('|one\ntwo\nthree|');
        const result = applyLinePrefix(content, start, end, '1. ');

        expect(result.content).toBe('1. one\n2. two\n3. three');
    });

    it('recognises an already numbered list when toggling it off', () => {
        // Matching only the literal `1. ` left 2. and 3. unrecognised, so the button
        // could never turn a list it had just made back off.
        const { content, start, end } = at('|1. one\n2. two\n3. three|');
        const result = applyLinePrefix(content, start, end, '1. ');

        expect(result.content).toBe('one\ntwo\nthree');
    });

    it('leaves blank lines alone rather than making empty items', () => {
        const { content, start, end } = at('|one\n\ntwo|');
        const result = applyLinePrefix(content, start, end, '- ');

        expect(result.content).toBe('- one\n\n- two');
    });

    it('toggles off even when the selection contains a blank line', () => {
        // Counting the blank line as "not prefixed" meant a selection like this never
        // read as fully prefixed, so it kept adding instead of removing.
        const { content, start, end } = at('|- one\n\n- two|');
        const result = applyLinePrefix(content, start, end, '- ');

        expect(result.content).toBe('one\n\ntwo');
    });

    it('replaces a different marker rather than stacking on it', () => {
        const { content, start, end } = at('|- one\n- two|');
        const result = applyLinePrefix(content, start, end, '### ');

        expect(result.content).toBe('### one\n### two');
    });

    it('covers the whole line from a caret sitting inside it', () => {
        const content = 'hello';
        const result = applyLinePrefix(content, 2, 2, '### ');

        expect(result.content).toBe('### hello');
        expect(result.selectionStart).toBe(6);
    });

    it('does not pull in the next line when the selection ends on the break', () => {
        const { content, start, end } = at('|one\n|two');
        const result = applyLinePrefix(content, start, end, '- ');

        expect(result.content).toBe('- one\ntwo');
    });
});
