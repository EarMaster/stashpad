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
 * `@path` references to files in a context's linked project folder.
 *
 * The stash holds nothing but the path, relative to the project root, in the form Claude
 * Code itself writes: `@src/app.ts`, quoted only when the path needs it -
 * `@"docs/release notes.md"`. An agent working in the repository resolves it directly, on
 * any machine, so no metadata is stored beside it.
 */

/** Punctuation that ends a sentence rather than a path: "see @src/app.ts." */
const TRAILING_PUNCTUATION = /[.,;:!?)\]]+$/;

/** The text a stash holds for `path`. Quotes only when whitespace would end it early. */
export function formatFileRef(path: string): string {
    return /\s/.test(path) ? `@"${path}"` : `@${path}`;
}

/**
 * Read a file reference at the very start of `src`: `@"quoted path"` or a bare `@path`.
 *
 * `raw` is what the reference occupies, so trailing punctuation after a bare path stays
 * outside it. `null` when `src` does not start with one.
 */
export function readFileRef(src: string): { raw: string; path: string } | null {
    const quoted = /^@"([^"\n]+)"/.exec(src);
    if (quoted) return { raw: quoted[0], path: quoted[1] };

    const bare = /^@[^\s"]+/.exec(src);
    if (!bare) return null;
    const raw = bare[0].replace(TRAILING_PUNCTUATION, "");
    if (raw.length < 2) return null;
    return { raw, path: raw.slice(1) };
}

/**
 * The `@` query being typed just before the cursor, or null.
 *
 * Only after whitespace or at the start of a line, so an email address never opens the
 * picker, and never inside a code fence or inline code span.
 */
export function findFileTrigger(
    textBeforeCursor: string,
): { start: number; query: string } | null {
    const match = textBeforeCursor.match(/(?:^|\s)@([^\s@"]*)$/);
    if (!match || match.index === undefined) return null;

    const start = textBeforeCursor.length - match[1].length - 1;
    const before = textBeforeCursor.slice(0, start);
    const fences = before.match(/^\s*```/gm)?.length ?? 0;
    if (fences % 2 === 1) return null;
    const line = before.slice(before.lastIndexOf("\n") + 1);
    if ((line.match(/`/g)?.length ?? 0) % 2 === 1) return null;

    return { start, query: match[1] };
}

function baseName(path: string): string {
    return path.slice(path.lastIndexOf("/") + 1);
}

/** Whether every character of `query` appears in `text`, in order. */
function isSubsequence(query: string, text: string): boolean {
    let i = 0;
    for (const ch of text) {
        if (ch === query[i]) i++;
        if (i === query.length) return true;
    }
    return query.length === 0;
}

/**
 * Files offered by the picker, best match first.
 *
 * A match on the file's own name beats one somewhere in its folders, and a short path
 * beats a long one, so `@app` finds `src/app.ts` before `node_modules/x/lib/app/index.js`.
 */
export function rankFileCandidates(query: string, files: string[], limit = 8): string[] {
    const q = query.toLowerCase();
    const score = (path: string): number => {
        if (!q) return 3;
        const lower = path.toLowerCase();
        const name = baseName(lower);
        if (name.startsWith(q)) return 0;
        if (name.includes(q)) return 1;
        if (lower.startsWith(q) || lower.includes(q)) return 2;
        if (isSubsequence(q, lower)) return 4;
        return -1;
    };
    return files
        .map((path) => ({ path, rank: score(path) }))
        .filter(({ rank }) => rank >= 0)
        .sort(
            (a, b) =>
                a.rank - b.rank ||
                a.path.length - b.path.length ||
                a.path.localeCompare(b.path),
        )
        .slice(0, limit)
        .map(({ path }) => path);
}

export type FileRefState = "resolved" | "unresolved";

export interface FileRefDisplay {
    state: FileRefState;
    /** For an unresolved path: a file of the same name elsewhere in the project. */
    suggestion?: string;
}

export type FileRefResolver = (path: string) => FileRefDisplay;

/**
 * Check `@path`s against a linked folder's files.
 *
 * Nothing about a reference is stored beyond its path, so a file that was moved is found
 * again by its name: the suggestion is the closest file of that name, preferring one that
 * kept more of the original folders.
 */
export function makeFileRefResolver(files: string[]): FileRefResolver {
    const known = new Set(files);
    const byName = new Map<string, string[]>();
    for (const file of files) {
        const name = baseName(file).toLowerCase();
        const list = byName.get(name);
        if (list) list.push(file);
        else byName.set(name, [file]);
    }

    return (path) => {
        if (known.has(path)) return { state: "resolved" };
        const candidates = byName.get(baseName(path).toLowerCase());
        if (!candidates) return { state: "unresolved" };
        const folders = path.split("/").slice(0, -1);
        const shared = (file: string) =>
            file.split("/").filter((part) => folders.includes(part)).length;
        const suggestion = [...candidates].sort(
            (a, b) => shared(b) - shared(a) || a.length - b.length,
        )[0];
        return { state: "unresolved", suggestion };
    };
}
