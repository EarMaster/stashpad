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
import type { StashItem } from "$lib/types";

/**
 * References between stashes.
 *
 * A reference is an ordinary Markdown link with a `stash:` scheme, stored in the
 * content itself: `[first line of the target](stash:<uuid>)`. Keeping it in the
 * text means it syncs, exports and encrypts with everything else, and anything
 * that does not understand the scheme still shows a readable label. The label is
 * a snapshot taken when the link was inserted; the app shows the target's live
 * first line instead, and falls back to the label once the target is gone.
 *
 * The cloud's MCP server parses the same syntax (`cloud/src/mcp/refs.rs`), and
 * the desktop auto-clear matches `stash:<id>` in SQL (`stashes.rs`), so the
 * format is a contract between three places. Change it in all of them or none.
 */

/** A `stash:` link. Group 1 is the stored label, group 2 the target id. */
export const STASH_REF_PATTERN =
    /\[([^\]\n]*)\]\(stash:([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})\)/g;

/** Longest label written into a new link. */
const LABEL_MAX = 60;

/**
 * Where a reference points, from the point of view of the stash holding it.
 *
 * `gone` covers both a deleted stash and one that never reached this device: the
 * app keeps no restore path for either, so there is nothing to tell apart.
 */
export type RefState = "open" | "completed" | "other_context" | "gone";

export interface ResolvedRef {
    id: string;
    state: RefState;
    /** The target, for every state but `gone`. */
    stash?: StashItem;
}

/** Blank out fenced and inline code, where a link is only text. */
function withoutCode(content: string): string {
    return content
        .replace(/```[\s\S]*?(?:```|$)/g, (m) => " ".repeat(m.length))
        .replace(/`[^`\n]*`/g, (m) => " ".repeat(m.length));
}

/** Every stash id referenced by the content, first occurrence first. */
export function parseRefIds(content: string): string[] {
    const ids: string[] = [];
    for (const match of withoutCode(content).matchAll(STASH_REF_PATTERN)) {
        const id = match[2].toLowerCase();
        if (!ids.includes(id)) ids.push(id);
    }
    return ids;
}

/**
 * The first meaningful line of a stash, short enough to sit inline.
 *
 * Tags are dropped on purpose: a `#tag` in the label would otherwise count as a
 * tag of the stash holding the link, and show up in its filter.
 */
export function refLabel(content: string): string {
    const line =
        content
            .split("\n")
            .map((l) =>
                l
                    .replace(/^\s*(?:#{1,6}\s+|>\s*|[-*+]\s+|\d+\.\s+)/, "")
                    .replace(/(^|\s)#[\w-]+/g, "$1")
                    .replace(STASH_REF_PATTERN, "$1")
                    .replace(/[[\]]/g, "")
                    .replace(/\s+/g, " ")
                    .trim(),
            )
            .find((l) => l.length > 0 && !l.startsWith("```")) ?? "";
    return line.length > LABEL_MAX
        ? line.slice(0, LABEL_MAX - 1).trimEnd() + "…"
        : line;
}

/** The Markdown written into a stash to reference `target`. */
export function makeRefLink(target: Pick<StashItem, "id" | "content">): string {
    return `[${refLabel(target.content) || target.id.slice(0, 8)}](stash:${target.id})`;
}

/** Resolve one reference against the stashes this device knows about. */
export function resolveRef(
    id: string,
    stashes: StashItem[],
    contextId: string,
): ResolvedRef {
    const stash = stashes.find((s) => s.id.toLowerCase() === id.toLowerCase());
    if (!stash || stash.deleted) return { id, state: "gone" };
    if ((stash.contextId || "default") !== (contextId || "default")) {
        return { id, state: "other_context", stash };
    }
    return { id, state: stash.completed ? "completed" : "open", stash };
}

/**
 * The `>>` trigger for the reference picker, if the caret sits right after one.
 *
 * `>>` is common in text that is not a reference, so the pattern is narrow: it
 * must start a word, must not be part of a longer run of `>` (git conflict
 * markers), and a space ends it - which is what separates `>>parser` from
 * `echo x >> log.txt` and from a nested quote. Code is left alone entirely.
 */
export function findRefTrigger(
    textBeforeCursor: string,
): { start: number; query: string } | null {
    const match = textBeforeCursor.match(/(?:^|\s)>>([^\s>]*)$/);
    if (!match || match.index === undefined) return null;

    const start = textBeforeCursor.length - match[0].length + (/^\s/.test(match[0]) ? 1 : 0);
    const before = textBeforeCursor.slice(0, start);
    const fences = before.match(/^\s*```/gm)?.length ?? 0;
    if (fences % 2 === 1) return null;
    const line = before.slice(before.lastIndexOf("\n") + 1);
    if ((line.match(/`/g)?.length ?? 0) % 2 === 1) return null;

    return { start, query: match[1] };
}

/**
 * Stashes offered by the picker: the context's live stashes, open ones first,
 * best match first within each group, never the stash being edited.
 */
export function rankRefCandidates(
    query: string,
    candidates: StashItem[],
    excludeId?: string,
    limit = 8,
): StashItem[] {
    const q = query.toLowerCase();
    const score = (s: StashItem) => {
        const label = refLabel(s.content).toLowerCase();
        if (!q) return 0;
        if (label.startsWith(q)) return 0;
        if (label.includes(q)) return 1;
        if (s.content.toLowerCase().includes(q)) return 2;
        return -1;
    };
    return candidates
        .filter((s) => s.id !== excludeId && !s.deleted && s.content.trim())
        .map((s) => ({ s, rank: score(s) }))
        .filter(({ rank }) => rank >= 0)
        .sort(
            (a, b) =>
                Number(!!a.s.completed) - Number(!!b.s.completed) ||
                a.rank - b.rank,
        )
        .slice(0, limit)
        .map(({ s }) => s);
}

/** Per-reference and overall limits on what copy-to-AI inlines. */
export const REF_COPY_LIMITS = {
    /** A referenced stash up to this length is inlined whole. */
    each: 2000,
    /** Inlined reference text across one copy. */
    total: 8000,
    /** What is inlined of a stash that did not fit; the rest goes to a file. */
    preview: 500,
} as const;

export interface CopyRef {
    /** 1-based, in order of first appearance. */
    n: number;
    id: string;
    label: string;
    state: RefState;
    /** What goes inline: the whole stash, or its opening when truncated. */
    inline?: string;
    /** The whole stash, set only when `inline` is not all of it. */
    full?: string;
}

/**
 * Prepare a stash's references for copying to an AI tool.
 *
 * Returns the body with each link turned into `[label] (ref N)`, and one entry
 * per referenced stash. References go one level deep: links inside a referenced
 * stash are reduced to their label and not followed, so a cycle cannot recurse.
 */
export function planCopyRefs(
    content: string,
    resolve: (id: string) => ResolvedRef,
    transform: (text: string) => string = (t) => t,
): { body: string; refs: CopyRef[] } {
    const refs: CopyRef[] = [];
    const numberOf = new Map<string, number>();
    let used = 0;

    for (const id of parseRefIds(content)) {
        const resolved = resolve(id);
        const n = refs.length + 1;
        numberOf.set(id, n);

        const ref: CopyRef = {
            n,
            id,
            state: resolved.state,
            label: resolved.stash ? refLabel(resolved.stash.content) : "",
        };

        if (resolved.stash && resolved.state !== "other_context") {
            const text = transform(
                resolved.stash.content.replace(STASH_REF_PATTERN, "[$1]"),
            ).trim();
            if (
                text.length <= REF_COPY_LIMITS.each &&
                used + text.length <= REF_COPY_LIMITS.total
            ) {
                ref.inline = text;
            } else {
                ref.inline = text.slice(0, REF_COPY_LIMITS.preview).trimEnd();
                ref.full = text;
            }
            used += ref.inline.length;
        }
        refs.push(ref);
    }

    const body = content.replace(STASH_REF_PATTERN, (whole, label: string, id: string) => {
        const n = numberOf.get(id.toLowerCase());
        return n ? `[${label}] (ref ${n})` : whole;
    });

    // Labels were read from live targets where possible; fill the rest from the
    // text of the link itself, which is all that is left of a deleted stash.
    for (const match of content.matchAll(STASH_REF_PATTERN)) {
        const ref = refs.find((r) => r.id === match[2].toLowerCase());
        if (ref && !ref.label) ref.label = match[1];
    }

    return { body, refs };
}

/**
 * The `# REFERENCED STASHES` section appended to copied text.
 *
 * `files` maps the id of each truncated reference to the file holding its full
 * text. The section is written in English on purpose, like the attachment
 * section beside it: it is read by the AI tool, not by the user.
 */
export function formatCopyRefs(refs: CopyRef[], files: Map<string, string>): string {
    if (refs.length === 0) return "";
    const parts = refs.map((ref) => {
        const head = `## Ref ${ref.n}: ${ref.label || ref.id}`;
        switch (ref.state) {
            case "gone":
                return `${head}\n(This stash was deleted or is no longer available.)`;
            case "other_context":
                return `${head}\n(This stash was moved to another context and is not included.)`;
            default: {
                const status = ref.state === "completed" ? "(completed)\n\n" : "";
                const file = files.get(ref.id);
                const tail = ref.full
                    ? file
                        ? `\n\n[Truncated. Full text: ${file}]`
                        : "\n\n[Truncated.]"
                    : "";
                return `${head}\n${status}${ref.inline ?? ""}${tail}`;
            }
        }
    });
    return `# REFERENCED STASHES\n\n${parts.join("\n\n")}`;
}
