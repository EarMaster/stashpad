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

import { describe, it, expect } from "vitest";
import {
    findFileTrigger,
    formatFileRef,
    makeFileRefResolver,
    rankFileCandidates,
    readFileRef,
} from "../file-refs";

describe("formatFileRef", () => {
    it("writes a plain path bare", () => {
        expect(formatFileRef("src/lib/types.ts")).toBe("@src/lib/types.ts");
    });

    it("quotes only a path with whitespace, as Claude Code does", () => {
        expect(formatFileRef("docs/release notes.md")).toBe('@"docs/release notes.md"');
    });
});

describe("readFileRef", () => {
    it("reads a bare path", () => {
        expect(readFileRef("@src/app.ts and more")).toEqual({
            raw: "@src/app.ts",
            path: "src/app.ts",
        });
    });

    it("reads a quoted path with spaces", () => {
        expect(readFileRef('@"docs/release notes.md" now')).toEqual({
            raw: '@"docs/release notes.md"',
            path: "docs/release notes.md",
        });
    });

    it("leaves sentence punctuation outside a bare path", () => {
        expect(readFileRef("@src/app.ts.")?.path).toBe("src/app.ts");
        expect(readFileRef("@src/app.ts),")?.path).toBe("src/app.ts");
        expect(readFileRef("@src/app.ts?")?.raw).toBe("@src/app.ts");
    });

    it("keeps dots that belong to the path", () => {
        expect(readFileRef("@.github/workflows/test.yml")?.path).toBe(
            ".github/workflows/test.yml",
        );
    });

    it("reads nothing from a lone @ or an unterminated quote", () => {
        expect(readFileRef("@ ")).toBeNull();
        expect(readFileRef("@.")).toBeNull();
        expect(readFileRef('@"open quote')).toBeNull();
    });

    it("round-trips everything formatFileRef writes", () => {
        for (const path of ["src/app.ts", "docs/release notes.md", "a b/c d.txt"]) {
            expect(readFileRef(formatFileRef(path))?.path).toBe(path);
        }
    });
});

describe("findFileTrigger", () => {
    it("opens at the start of the text and after whitespace", () => {
        expect(findFileTrigger("@")).toEqual({ start: 0, query: "" });
        expect(findFileTrigger("see @src/ap")).toEqual({ start: 4, query: "src/ap" });
        expect(findFileTrigger("line\n@typ")).toEqual({ start: 5, query: "typ" });
    });

    it("does not open inside an email address", () => {
        expect(findFileTrigger("mail nico@example")).toBeNull();
    });

    it("closes once the query is followed by a space", () => {
        expect(findFileTrigger("@src/app.ts ")).toBeNull();
    });

    it("does not open inside code", () => {
        expect(findFileTrigger("```\n@src")).toBeNull();
        expect(findFileTrigger("run `npm @src")).toBeNull();
        expect(findFileTrigger("```\ncode\n```\n@src")).toEqual({ start: 13, query: "src" });
    });
});

describe("rankFileCandidates", () => {
    const files = [
        "node_modules/pkg/lib/app/index.js",
        "src/app.ts",
        "src/lib/apply.ts",
        "src/lib/types.ts",
        "README.md",
    ];

    it("prefers a match on the file name, then the shorter path", () => {
        expect(rankFileCandidates("app", files)).toEqual([
            "src/app.ts",
            "src/lib/apply.ts",
            "node_modules/pkg/lib/app/index.js",
        ]);
    });

    it("matches case-insensitively and by subsequence", () => {
        expect(rankFileCandidates("readme", files)).toEqual(["README.md"]);
        expect(rankFileCandidates("sltyp", files)).toEqual(["src/lib/types.ts"]);
    });

    it("offers files with an empty query, shortest first, up to the limit", () => {
        expect(rankFileCandidates("", files, 2)).toEqual(["README.md", "src/app.ts"]);
    });
});

describe("makeFileRefResolver", () => {
    const resolve = makeFileRefResolver([
        "src/lib/types.ts",
        "app/src/lib/stores/types.ts",
        "src/components/Editor.svelte",
    ]);

    it("resolves a path the folder holds", () => {
        expect(resolve("src/lib/types.ts")).toEqual({ state: "resolved" });
    });

    it("suggests a moved file by its name, keeping the most of its folders", () => {
        expect(resolve("src/components/old/Editor.svelte")).toEqual({
            state: "unresolved",
            suggestion: "src/components/Editor.svelte",
        });
        expect(resolve("lib/stores/types.ts")).toEqual({
            state: "unresolved",
            suggestion: "app/src/lib/stores/types.ts",
        });
    });

    it("suggests nothing when no file of that name exists", () => {
        expect(resolve("src/gone.ts")).toEqual({ state: "unresolved" });
    });
});
