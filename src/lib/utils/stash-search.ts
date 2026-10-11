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

import MiniSearch from "minisearch";
import type { StashItem } from "$lib/types";
import { STASH_REF_PATTERN } from "./stash-refs";
import { liveAttachments } from "./stash-filters";

/**
 * Search over stashes.
 *
 * The webview's find-in-page could not do this: it only sees what is rendered, and a
 * queue hides most of its text - completed stashes are collapsed away and long cards are
 * clamped. This searches the stashes themselves: their text, the AI-enhanced version and
 * the names of their attachments, with typos and unfinished words forgiven.
 */

interface SearchDoc {
    id: string;
    content: string;
    enhanced: string;
    files: string;
}

/** Matches, best first: each stash id with the words in it that matched. */
export type SearchHits = Map<string, string[]>;

/**
 * An attachment synced to an encrypted account keeps its sealed details in the name
 * until its file is downloaded, which is when the real name is written back. That
 * envelope is not a name anyone searches for. See `is_envelope` in envelope.rs.
 */
const SEALED_PREFIX = "SPE1.";

/** How words are split, for the index and for highlighting alike. */
const WORD = /[\p{L}\p{N}_]+/gu;

function tokenize(text: string): string[] {
    return text.match(WORD) ?? [];
}

/** A link reads as its label: the id behind it is not something anyone searches for. */
function readable(text: string | undefined): string {
    return (text ?? "").replace(STASH_REF_PATTERN, "$1");
}

export function buildSearchIndex(stashes: StashItem[]): MiniSearch<SearchDoc> {
    const index = new MiniSearch<SearchDoc>({
        fields: ["content", "enhanced", "files"],
        tokenize,
        searchOptions: {
            tokenize,
            // Every word has to be found, so each one narrows the result.
            combineWith: "AND",
            // "pars" finds "parser" while it is being typed.
            prefix: true,
            // A typo in a word of five letters, two from eight - a swapped pair of
            // letters counts as two. Shorter words have to be exact, or they match
            // everything.
            fuzzy: (term) => (term.length >= 8 ? 2 : term.length >= 5 ? 1 : false),
            boost: { content: 2 },
        },
    });
    index.addAll(
        stashes
            .filter((s) => !s.deleted && !s.isDndShadowItem)
            .map((s) => ({
                id: s.id,
                content: readable(s.content),
                enhanced: readable(s.enhancedContent),
                files: liveAttachments(s)
                    .map((a) => a.fileName)
                    .filter((name) => !name.startsWith(SEALED_PREFIX))
                    .join(" "),
            })),
    );
    return index;
}

export function searchStashes(index: MiniSearch<SearchDoc>, query: string): SearchHits {
    const hits: SearchHits = new Map();
    if (!query.trim()) return hits;
    for (const result of index.search(query)) {
        hits.set(String(result.id), result.terms);
    }
    return hits;
}

/** A piece of text, and whether it is one of the words that matched. */
export interface Segment {
    text: string;
    hit: boolean;
}

/** Split text into matched words and the rest between them. */
export function segments(text: string, terms: string[]): Segment[] {
    const wanted = new Set(terms.map((t) => t.toLowerCase()));
    const out: Segment[] = [];
    let last = 0;
    for (const match of text.matchAll(WORD)) {
        if (!wanted.has(match[0].toLowerCase())) continue;
        const at = match.index ?? 0;
        if (at > last) out.push({ text: text.slice(last, at), hit: false });
        out.push({ text: match[0], hit: true });
        last = at + match[0].length;
    }
    if (last < text.length) out.push({ text: text.slice(last), hit: false });
    return out;
}

/**
 * Lines of context around every matched word, for a result list: one line per stretch of
 * text, so several matches close together share a line. Each match shows up exactly once,
 * which keeps the count of a result the same as in its card. When nothing in the text
 * matched - the match was in an attachment name, say - the opening of the text instead.
 */
export function snippets(text: string, terms: string[], length = 120): Segment[][] {
    const flat = readable(text).replace(/\s+/g, " ").trim();
    const wanted = new Set(terms.map((t) => t.toLowerCase()));
    const hits = [...flat.matchAll(WORD)]
        .filter((m) => wanted.has(m[0].toLowerCase()))
        .map((m) => ({ at: m.index ?? 0, end: (m.index ?? 0) + m[0].length }));

    const cut = (start: number, end: number) =>
        segments(
            (start > 0 ? "…" : "") + flat.slice(start, end) + (end < flat.length ? "…" : ""),
            terms,
        );
    if (hits.length === 0) return [cut(0, Math.min(flat.length, length))];

    const lines: Segment[][] = [];
    let i = 0;
    while (i < hits.length) {
        let start = Math.max(0, hits[i].at - Math.floor(length / 3));
        if (start > 0) {
            // Start on a word, not in the middle of one.
            const space = flat.indexOf(" ", start);
            start = space >= 0 && space < hits[i].at ? space + 1 : start;
        }
        let end = Math.min(flat.length, start + length);
        // Every match that fits wholly on this line belongs to it; the line is stretched
        // to the end of the last one rather than cutting a word in half.
        let j = i;
        while (j + 1 < hits.length && hits[j + 1].end <= end) j++;
        end = Math.max(end, hits[j].end);
        lines.push(cut(start, end));
        i = j + 1;
    }
    return lines;
}

const MARK_CLASS = "search-hit";

/**
 * Mark the matched words in rendered content, after taking out the marks a previous
 * search left. Text only - code, links and badges keep their structure, and nothing is
 * parsed, so content cannot be turned into markup this way.
 */
export function highlight(root: HTMLElement, terms: string[]): void {
    for (const mark of Array.from(root.querySelectorAll(`mark.${MARK_CLASS}`))) {
        mark.replaceWith(document.createTextNode(mark.textContent ?? ""));
    }
    root.normalize();
    if (terms.length === 0) return;

    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    const nodes: Text[] = [];
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
        nodes.push(node as Text);
    }
    for (const node of nodes) {
        const parts = segments(node.data, terms);
        if (!parts.some((p) => p.hit)) continue;
        const fragment = document.createDocumentFragment();
        for (const part of parts) {
            if (part.hit) {
                const mark = document.createElement("mark");
                mark.className = MARK_CLASS;
                mark.textContent = part.text;
                fragment.append(mark);
            } else {
                fragment.append(part.text);
            }
        }
        node.replaceWith(fragment);
    }
}
