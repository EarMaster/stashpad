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

import { describe, expect, it } from 'vitest';
import { isTraditionalChinese, matchLocale } from '../index';

describe('matchLocale', () => {
    it.each([
        ['en-US', 'en'],
        ['de-AT', 'de'],
        ['zh', 'zh'],
        ['zh-CN', 'zh'],
        ['zh-SG', 'zh'],
        ['zh-Hans-CN', 'zh'],
        ['zh_CN', 'zh'],
    ])('maps %s to %s', (tag, expected) => {
        expect(matchLocale(tag)).toBe(expected);
    });

    // Only Simplified Chinese ships. A Traditional reader gets English rather than the
    // other script.
    it.each([['zh-TW'], ['zh-HK'], ['zh-MO'], ['zh-Hant'], ['zh-Hant-TW']])(
        'leaves %s unmatched',
        (tag) => {
            expect(isTraditionalChinese(tag)).toBe(true);
            expect(matchLocale(tag)).toBeNull();
        },
    );

    it('leaves an unsupported language unmatched', () => {
        expect(matchLocale('fr-FR')).toBeNull();
    });
});

describe('locale files', () => {
    // svelte-i18n falls back to English for a missing key without a word, so a string
    // added to en.json alone ships as English in the German and Chinese UI.
    const files = import.meta.glob('../locales/*.json', { eager: true, import: 'default' }) as Record<
        string,
        Record<string, unknown>
    >;
    const leaves = (node: unknown, prefix = ''): Map<string, string> => {
        const out = new Map<string, string>();
        if (node && typeof node === 'object') {
            for (const [key, value] of Object.entries(node)) {
                if (key === '_meta') continue;
                const path = prefix ? `${prefix}.${key}` : key;
                if (typeof value === 'string') out.set(path, value);
                else for (const [p, v] of leaves(value, path)) out.set(p, v);
            }
        }
        return out;
    };
    // Names only: `{count}`, and `count` in `{count, plural, ...}` - not the branches of an
    // ICU plural, whose text is translated.
    const placeholders = (text: string) =>
        [...text.matchAll(/\{\s*([A-Za-z_][\w.]*)\s*[,}]/g)].map((m) => m[1]).sort();

    const en = leaves(files['../locales/en.json']);
    const others = Object.entries(files).filter(([path]) => !path.endsWith('/en.json'));

    it('include German and Chinese', () => {
        expect(others.map(([path]) => path).sort()).toEqual(['../locales/de.json', '../locales/zh.json']);
    });

    it.each(others)('%s has exactly the keys and placeholders of en.json', (_path, content) => {
        const theirs = leaves(content);
        expect([...theirs.keys()].sort()).toEqual([...en.keys()].sort());
        for (const [key, text] of en) {
            expect(placeholders(theirs.get(key) ?? ''), key).toEqual(placeholders(text));
        }
    });
});
