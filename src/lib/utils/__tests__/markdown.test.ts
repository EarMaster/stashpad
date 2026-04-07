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

import { describe, it, expect } from 'vitest';
import { getTagHue, safeParse } from '../markdown';

describe('markdown utilities', () => {
    describe('safeParse', () => {
        it('should render basic markdown', () => {
            const result = safeParse('**bold** text');
            expect(result).toContain('<strong>bold</strong>');
        });

        it('should highlight @ mentions', () => {
            const result = safeParse('@src/lib/i18n/locales');
            expect(result).toContain('class="ai-mention"');
            expect(result).toContain('@src/lib/i18n/locales');
        });

        it('should highlight @ mentions with special characters', () => {
            const result = safeParse('@path/to/file.ts');
            expect(result).toContain('class="ai-mention"');
            expect(result).toContain('@path/to/file.ts');
        });

        it('should highlight @ mentions after whitespace', () => {
            const result = safeParse('Check @src/utils for helpers');
            expect(result).toContain('class="ai-mention"');
            expect(result).toContain('@src/utils');
        });

        it('should highlight / commands when preceded by whitespace', () => {
            const result = safeParse(' /help');
            expect(result).toContain('class="ai-command"');
            expect(result).toContain('/help');
        });

        it('should highlight / commands at start of text', () => {
            const result = safeParse('/search query');
            expect(result).toContain('class="ai-command"');
            expect(result).toContain('/search');
        });

        it('should NOT highlight / in middle of paths (no preceding whitespace)', () => {
            const result = safeParse('src/lib/utils');
            expect(result).not.toContain('class="ai-command"');
        });

        it('should highlight #tags', () => {
            const result = safeParse('#bug-fix');
            expect(result).toContain('class="ai-tag"');
            expect(result).toContain('bug-fix');
        });

        it('should generate tag icons for hashtags', () => {
            const result = safeParse('#urgent');
            expect(result).toContain('<svg');
            expect(result).toContain('--tag-bg:');
        });

        it('should handle code blocks with syntax highlighting', () => {
            const code = '```javascript\nconst x = 42;\n```';
            const result = safeParse(code);
            expect(result).toContain('hljs');
            expect(result).toContain('language-javascript');
        });

        it('should handle multiple markdown features together', () => {
            const text = '**Check** @src/lib for /help and #urgent items';
            const result = safeParse(text);
            expect(result).toContain('<strong>Check</strong>');
            expect(result).toContain('class="ai-mention"');
            expect(result).toContain('class="ai-command"');
            expect(result).toContain('class="ai-tag"');
        });

        it('should recognize hex colors as color badges and NOT as tags', () => {
            const result = safeParse('Use color #302919 or #f00');
            expect(result).toContain('class="ai-color"');
            expect(result).toContain('background-color: #302919');
            expect(result).toContain('background-color: #f00');
            expect(result).not.toContain('class="ai-tag"');
        });

        it('should NOT recognize non-hex hashtags as colors', () => {
            const result = safeParse('#not-a-color and #bad');
            // #bad is a color if we follow Option 1 strictly!
            expect(result).toContain('class="ai-tag"');
            expect(result).toContain('not-a-color');
            expect(result).toContain('class="ai-color"'); // #bad is 3 hex chars
            expect(result).toContain('background-color: #bad');
        });

        it('should NOT recognize partial hex as colors if followed by more word characters', () => {
            const result = safeParse('#abc-tag and #abcdefg');
            expect(result).toContain('class="ai-tag"');
            expect(result).toContain('abc-tag');
            expect(result).toContain('abcdefg');
            expect(result).not.toContain('class="ai-color"');
        });
    });


    describe('getTagHue', () => {
        it('should return a number between 0 and 360', () => {
            const hue = getTagHue('#test');
            expect(hue).toBeGreaterThanOrEqual(0);
            expect(hue).toBeLessThan(360);
        });

        it('should return consistent hue for same tag', () => {
            const hue1 = getTagHue('#urgent');
            const hue2 = getTagHue('#urgent');
            expect(hue1).toBe(hue2);
        });

        it('should return different hues for different tags', () => {
            const hue1 = getTagHue('#bug');
            const hue2 = getTagHue('#feature');
            expect(hue1).not.toBe(hue2);
        });

        it('should NOT recognize anchors in URLs as tags', () => {
            const result = safeParse('Check https://example.com#section');
            expect(result).toContain('https://example.com#section');
            expect(result).not.toContain('class="ai-tag"');
        });

        it('should NOT recognize anchors in markdown links as tags', () => {
            const result = safeParse('[link](https://example.com#anchor)');
            expect(result).toContain('href="https://example.com#anchor"');
            expect(result).not.toContain('class="ai-tag"');
        });

        it('should NOT recognize hashes in middle of words as tags', () => {
            const result = safeParse('file#utils and foo#bar');
            expect(result).not.toContain('class="ai-tag"');
        });
    });
});
