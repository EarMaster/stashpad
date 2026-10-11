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
import type { StashItem } from '$lib/types';
import { buildSearchIndex, searchStashes, segments, snippets, highlight } from '../stash-search';

function stash(id: string, content: string, extra: Partial<StashItem> = {}): StashItem {
    return {
        id,
        content,
        contextId: 'default',
        completed: false,
        createdAt: '2026-10-11T00:00:00Z',
        attachments: [],
        ...extra,
    } as StashItem;
}

const stashes = [
    stash('a', 'Fix the parser crash on empty input'),
    stash('b', 'Deploy the website', { completed: true }),
    stash('c', 'Notes', { enhancedContent: 'Investigate the memory leak in sync' }),
    stash('d', 'Screenshot', {
        attachments: [{ id: 'x', fileName: 'invoice-october.pdf' } as StashItem['attachments'][number]],
    }),
    stash('e', 'Gone parser', { deleted: true }),
    stash('f', 'See [the parser one](stash:00000000-0000-4000-8000-000000000001)'),
];

const ids = (query: string) => [...searchStashes(buildSearchIndex(stashes), query).keys()];

describe('searchStashes', () => {
    it('finds words anywhere, completed stashes included, deleted ones not', () => {
        expect(ids('website')).toEqual(['b']);
        expect(ids('parser').sort()).toEqual(['a', 'f']);
    });

    it('forgives a typo in a longer word, and finds a word while it is typed', () => {
        expect(ids('parsor')).toContain('a');
        expect(ids('depolyment')).toEqual([]);
        expect(ids('wbesite')).toEqual([]);
        expect(ids('octobr')).toEqual(['d']);
        expect(ids('webs')).toEqual(['b']);
    });

    it('requires every word', () => {
        expect(ids('parser crash')).toEqual(['a']);
        expect(ids('parser website')).toEqual([]);
    });

    it('looks in the enhanced text and in attachment names', () => {
        expect(ids('leak')).toEqual(['c']);
        expect(ids('invoice')).toEqual(['d']);
    });

    it('leaves out a sealed attachment name that is still waiting for its file', () => {
        const sealed = stash('s', 'Report', {
            attachments: [{ id: 'y', fileName: 'SPE1.abc.secretname' } as StashItem['attachments'][number]],
        });
        expect([...searchStashes(buildSearchIndex([sealed]), 'secretname').keys()]).toEqual([]);
        expect([...searchStashes(buildSearchIndex([sealed]), 'report').keys()]).toEqual(['s']);
    });

    it('does not match the id behind a stash link', () => {
        expect(ids('000000000001')).toEqual([]);
    });

    it('reports the words that matched, for highlighting', () => {
        const hits = searchStashes(buildSearchIndex(stashes), 'pars');
        expect(hits.get('a')).toEqual(['parser']);
    });

    it('finds nothing for an empty query', () => {
        expect(ids('   ')).toEqual([]);
    });
});

describe('segments and snippet', () => {
    it('splits out whole matched words, ignoring case', () => {
        expect(segments('Parser, parsers and PARSER', ['parser'])).toEqual([
            { text: 'Parser', hit: true },
            { text: ', parsers and ', hit: false },
            { text: 'PARSER', hit: true },
        ]);
    });

    it('gives each match one line, sharing a line when matches are close', () => {
        const text = 'lorem '.repeat(60) + 'needle needle ' + 'ipsum '.repeat(60) + 'needle';
        const lines = snippets(text, ['needle'], 60);
        expect(lines).toHaveLength(2);
        const hitCount = lines.flat().filter((p) => p.hit).length;
        expect(hitCount).toBe(3);
        expect(lines[0].map((p) => p.text).join('').startsWith('…')).toBe(true);
    });

    it('falls back to the opening when the text itself has no match', () => {
        const lines = snippets('Screenshot of the bug', ['invoice']);
        expect(lines).toEqual([[{ text: 'Screenshot of the bug', hit: false }]]);
    });
});

describe('highlight', () => {
    it('marks matched words in text only, and undoes an earlier search', () => {
        const root = document.createElement('div');
        root.innerHTML = '<p>Fix the <code>parser</code> crash</p>';
        highlight(root, ['parser']);
        expect(root.innerHTML).toBe('<p>Fix the <code><mark class="search-hit">parser</mark></code> crash</p>');

        highlight(root, ['crash']);
        expect(root.innerHTML).toBe('<p>Fix the <code>parser</code> <mark class="search-hit">crash</mark></p>');

        highlight(root, []);
        expect(root.innerHTML).toBe('<p>Fix the <code>parser</code> crash</p>');
    });

    it('never turns text into markup', () => {
        const root = document.createElement('div');
        root.textContent = '<img src=x onerror=boom> parser';
        highlight(root, ['parser']);
        expect(root.querySelector('img')).toBeNull();
        expect(root.querySelector('mark')?.textContent).toBe('parser');
    });
});
