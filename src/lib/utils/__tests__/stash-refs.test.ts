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
import {
    findRefTrigger,
    formatCopyRefs,
    makeRefLink,
    parseRefIds,
    planCopyRefs,
    rankRefCandidates,
    refLabel,
    resolveRef,
    REF_COPY_LIMITS,
} from '../stash-refs';
import { safeParse } from '../markdown';

const A = 'aaaaaaaa-0000-4000-8000-000000000001';
const B = 'bbbbbbbb-0000-4000-8000-000000000002';
const C = 'cccccccc-0000-4000-8000-000000000003';

function stash(id: string, content: string, extra: Partial<StashItem> = {}): StashItem {
    return {
        id,
        content,
        attachments: [],
        createdAt: '2026-09-01T00:00:00Z',
        contextId: 'work',
        ...extra,
    };
}

describe('parseRefIds', () => {
    it('finds each referenced id once, in order', () => {
        expect(parseRefIds(`[x](stash:${B}) then [y](stash:${A}) and [x](stash:${B})`)).toEqual([B, A]);
    });

    it('ignores links inside code', () => {
        expect(parseRefIds('`[x](stash:' + B + ')`\n```\n[y](stash:' + A + ')\n```')).toEqual([]);
    });

    it('ignores other links and malformed ids', () => {
        expect(parseRefIds('[x](https://example.com) [y](stash:not-a-uuid)')).toEqual([]);
    });
});

describe('refLabel / makeRefLink', () => {
    it('takes the first meaningful line without markup or tags', () => {
        expect(refLabel('\n## Fix the #parser crash\nmore')).toBe('Fix the crash');
    });

    it('drops link syntax and brackets so the label cannot break the link', () => {
        expect(refLabel(`see [other](stash:${A}) [x]`)).toBe('see other x');
    });

    it('truncates long lines', () => {
        const label = refLabel('x'.repeat(200));
        expect(label.length).toBeLessThanOrEqual(60);
        expect(label.endsWith('…')).toBe(true);
    });

    it('writes a link the parser reads back', () => {
        const link = makeRefLink(stash(B, 'Target stash'));
        expect(link).toBe(`[Target stash](stash:${B})`);
        expect(parseRefIds(link)).toEqual([B]);
    });
});

describe('resolveRef', () => {
    const stashes = [
        stash(A, 'open one'),
        stash(B, 'done one', { completed: true }),
        stash(C, 'elsewhere', { contextId: 'home' }),
    ];

    it('tells every state apart', () => {
        expect(resolveRef(A, stashes, 'work').state).toBe('open');
        expect(resolveRef(B, stashes, 'work').state).toBe('completed');
        expect(resolveRef(C, stashes, 'work').state).toBe('other_context');
        expect(resolveRef('dddddddd-0000-4000-8000-000000000004', stashes, 'work').state).toBe('gone');
    });

    it('treats a deleted stash as gone', () => {
        expect(resolveRef(A, [stash(A, 'x', { deleted: true })], 'work')).toEqual({ id: A, state: 'gone' });
    });
});

describe('findRefTrigger', () => {
    it('opens on >> at the start of a word', () => {
        expect(findRefTrigger('see >>pars')).toEqual({ start: 4, query: 'pars' });
        expect(findRefTrigger('>>')).toEqual({ start: 0, query: '' });
    });

    it('stays shut for shell redirects, quotes and conflict markers', () => {
        expect(findRefTrigger('echo x >> log.txt')).toBeNull();
        expect(findRefTrigger('>> quoted')).toBeNull();
        expect(findRefTrigger('>>>>>>> main')).toBeNull();
        expect(findRefTrigger('>>>')).toBeNull();
        expect(findRefTrigger('a>>b')).toBeNull();
    });

    it('stays shut inside code', () => {
        expect(findRefTrigger('```\n>>x')).toBeNull();
        expect(findRefTrigger('run `>>x')).toBeNull();
        expect(findRefTrigger('```\ncode\n```\n>>x')).toEqual({ start: 13, query: 'x' });
    });
});

describe('rankRefCandidates', () => {
    const stashes = [
        stash(A, 'Parser crash', { completed: true }),
        stash(B, 'Update the parser'),
        stash(C, 'Unrelated'),
    ];

    it('puts open stashes first and filters by the query', () => {
        expect(rankRefCandidates('pars', stashes).map((s) => s.id)).toEqual([B, A]);
    });

    it('leaves out the stash being edited', () => {
        expect(rankRefCandidates('', stashes, B).map((s) => s.id)).toEqual([C, A]);
    });
});

describe('planCopyRefs / formatCopyRefs', () => {
    const resolveIn = (stashes: StashItem[]) => (id: string) => resolveRef(id, stashes, 'work');

    it('numbers the links and inlines their targets one level deep', () => {
        const stashes = [
            stash(B, `Target text linking [c](stash:${C})`),
            stash(C, 'never inlined'),
        ];
        const { body, refs } = planCopyRefs(`Do it, see [label](stash:${B}).`, resolveIn(stashes));
        expect(body).toBe('Do it, see [label] (ref 1).');
        expect(refs).toHaveLength(1);
        expect(refs[0].inline).toBe('Target text linking [c]');

        const section = formatCopyRefs(refs, new Map());
        expect(section).toContain('# REFERENCED STASHES');
        expect(section).toContain('## Ref 1: Target text linking c');
        expect(section).not.toContain('never inlined');
    });

    it('does not follow a cycle', () => {
        const stashes = [stash(A, `a [b](stash:${B})`), stash(B, `b [a](stash:${A})`)];
        const { refs } = planCopyRefs(`[b](stash:${B})`, resolveIn(stashes));
        expect(refs.map((r) => r.id)).toEqual([B]);
    });

    it('notes gone and moved targets without their content', () => {
        const stashes = [stash(C, 'Title\nsecret elsewhere', { contextId: 'home' })];
        const { refs } = planCopyRefs(`[old](stash:${A}) [moved](stash:${C})`, resolveIn(stashes));
        const section = formatCopyRefs(refs, new Map());
        expect(section).toContain('## Ref 1: old\n(This stash was deleted');
        expect(section).toContain('moved to another context');
        expect(section).not.toContain('secret elsewhere');
    });

    it('truncates a long target and points at its file', () => {
        const long = 'y'.repeat(REF_COPY_LIMITS.each + 1);
        const { refs } = planCopyRefs(`[t](stash:${B})`, resolveIn([stash(B, long)]));
        expect(refs[0].inline).toHaveLength(REF_COPY_LIMITS.preview);
        expect(refs[0].full).toBe(long);
        const section = formatCopyRefs(refs, new Map([[B, '/cache/refs/b.md']]));
        expect(section).toContain('[Truncated. Full text: /cache/refs/b.md]');
    });

    it('caps the total inlined across all references', () => {
        const each = 'z'.repeat(REF_COPY_LIMITS.each);
        const ids = [A, B, C, 'dddddddd-0000-4000-8000-000000000004', 'eeeeeeee-0000-4000-8000-000000000005'];
        const stashes = ids.map((id) => stash(id, each));
        const content = ids.map((id) => `[x](stash:${id})`).join(' ');
        const { refs } = planCopyRefs(content, resolveIn(stashes));
        const inlined = refs.reduce((n, r) => n + (r.inline?.length ?? 0), 0);
        expect(refs.filter((r) => r.full)).toHaveLength(1);
        expect(inlined).toBeLessThanOrEqual(REF_COPY_LIMITS.total + REF_COPY_LIMITS.preview);
    });

    it('applies the tag stripping to referenced text too', () => {
        const { refs } = planCopyRefs(
            `[t](stash:${B})`,
            resolveIn([stash(B, 'fix it #urgent')]),
            (t) => t.replace(/#[\w-]+/g, '').trim(),
        );
        expect(refs[0].inline).toBe('fix it');
    });
});

describe('safeParse with references', () => {
    it('renders a stash link as a focusable chip with the live label', () => {
        const html = safeParse(`see [stored](stash:${B})`, () => ({ state: 'open', label: 'Live <label>' }));
        expect(html).toContain(`data-ref-id="${B}"`);
        expect(html).toContain('role="link" tabindex="0"');
        expect(html).toContain('Live &lt;label&gt;');
        expect(html).not.toContain('stored');
        expect(html).not.toContain('href="stash:');
    });

    it('falls back to the stored label and is not focusable when gone', () => {
        const html = safeParse(`[stored](stash:${B})`, () => ({ state: 'gone', note: 'deleted' }));
        expect(html).toContain('stash-ref-gone');
        expect(html).toContain('stored');
        expect(html).not.toContain('tabindex');
    });

    it('leaves ordinary links alone', () => {
        expect(safeParse('[x](https://example.com)')).toContain('<a href="https://example.com">x</a>');
    });

    it('does not turn a crafted span into anything but a chip', () => {
        const html = safeParse(`<span class="stash-ref stash-ref-open" title="stash:${B}" onclick="doHarm()">x</span>`);
        expect(html).not.toContain('onclick');
    });
});
