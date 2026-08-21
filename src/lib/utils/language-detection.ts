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

// Lazy-load highlight.js to improve startup performance
// The library is only loaded when syntax highlighting is actually needed
let hljsInstance: typeof import("highlight.js").default | null = null;
let hljsLoadPromise: Promise<typeof import("highlight.js").default> | null = null;

/**
 * Lazily load and cache the highlight.js library.
 * This defers the ~4.8MB bundle until code highlighting is actually needed.
 */
async function getHljs(): Promise<typeof import("highlight.js").default> {
    if (hljsInstance) return hljsInstance;

    if (!hljsLoadPromise) {
        hljsLoadPromise = import("highlight.js").then(module => {
            hljsInstance = module.default;
            return hljsInstance;
        });
    }

    return hljsLoadPromise;
}

/**
 * Result of language detection analysis.
 */
export interface LanguageDetectionResult {
    /** The detected language identifier (e.g., "csharp", "javascript") or null if unknown */
    language: string | null;
    /** The file extension to use for this language (e.g., "cs", "js", "txt") */
    extension: string;
    /**
     * Highlighted HTML for the portion that was inspected.
     *
     * Detection samples the start of the input (see `DETECTION_SAMPLE_LIMIT`), so for a
     * large input this covers the sample, not the whole text. Use `highlightCode` when
     * you need HTML for the entire content.
     */
    highlightedHtml: string;
    /** Confidence score from highlight.js (higher = more confident) */
    relevance: number;
}

/**
 * Map of highlight.js language identifiers to file extensions.
 * This covers the most common programming languages and data formats.
 */
const LANGUAGE_TO_EXTENSION: Record<string, string> = {
    // C-family languages
    "c": "c",
    "cpp": "cpp",
    "csharp": "cs",
    "objectivec": "m",

    // Web technologies
    "javascript": "js",
    "typescript": "ts",
    "html": "html",
    "xml": "xml",
    "css": "css",
    "scss": "scss",
    "less": "less",
    "json": "json",

    // Frontend frameworks (detected as their base language)
    "jsx": "jsx",
    "tsx": "tsx",

    // Scripting languages
    "python": "py",
    "ruby": "rb",
    "php": "php",
    "perl": "pl",
    "lua": "lua",

    // JVM languages
    "java": "java",
    "kotlin": "kt",
    "scala": "scala",
    "groovy": "groovy",

    // Systems programming
    "rust": "rs",
    "go": "go",
    "swift": "swift",

    // Shell scripting
    "bash": "sh",
    "shell": "sh",
    "powershell": "ps1",
    "bat": "bat",

    // Data & configuration
    "yaml": "yaml",
    "toml": "toml",
    "ini": "ini",
    "properties": "properties",

    // Markup & documentation
    "markdown": "md",
    "latex": "tex",
    "asciidoc": "adoc",

    // Database
    "sql": "sql",
    "pgsql": "sql",
    "plsql": "sql",

    // Functional languages
    "haskell": "hs",
    "erlang": "erl",
    "elixir": "ex",
    "fsharp": "fs",
    "clojure": "clj",
    "lisp": "lisp",
    "scheme": "scm",

    // Other
    "r": "r",
    "matlab": "m",
    "dockerfile": "dockerfile",
    "makefile": "makefile",
    "cmake": "cmake",
    "diff": "diff",
    "plaintext": "txt",
};

/**
 * Minimum relevance score to trust the detection.
 * If the score is below this, we fall back to .txt
 */
const MIN_CONFIDENCE_THRESHOLD = 5;

/**
 * How much of the input `detectLanguage` inspects, in characters.
 *
 * Detection quality plateaus quickly - a language is recognisable from its first few
 * kilobytes - while `highlightAuto`'s cost grows with the whole input.
 */
const DETECTION_SAMPLE_LIMIT = 16_000;

/**
 * Above this many characters, `highlightCode` returns escaped plain text instead of
 * highlighting. Rendering runs on the UI thread, and a file this large is exactly the
 * case where the wait costs more than the colour is worth.
 */
const HIGHLIGHT_LIMIT = 400_000;

/**
 * List of supported languages for manual selection dropdown.
 * Sorted alphabetically for easy navigation.
 */
export const SUPPORTED_LANGUAGES: string[] = Object.keys(LANGUAGE_TO_EXTENSION).sort();

/**
 * Detect the programming language of a code snippet.
 * Uses highlight.js auto-detection with a curated subset of common languages.
 * 
 * @param code - The source code to analyze
 * @returns Detection result with language, extension, and highlighted HTML
 */
export async function detectLanguage(code: string): Promise<LanguageDetectionResult> {
    const hljs = await getHljs();

    // Detect from the start of the input rather than all of it. highlightAuto scores the
    // text against every candidate language and builds highlighted HTML for each, so its
    // cost is roughly the number of candidates times the length - uncapped, a large paste
    // blocked the window for seconds, and this runs on the UI thread. A few kilobytes are
    // more than enough to tell one language from another.
    const sample = code.length > DETECTION_SAMPLE_LIMIT
        ? code.slice(0, DETECTION_SAMPLE_LIMIT)
        : code;

    // Use a subset of common languages for faster and more accurate detection
    const result = hljs.highlightAuto(sample, [
        "javascript", "typescript", "python", "java", "csharp", "cpp", "c",
        "go", "rust", "ruby", "php", "swift", "kotlin", "scala",
        "html", "xml", "css", "scss", "json", "yaml", "toml",
        "sql", "bash", "powershell", "dockerfile",
        "markdown", "plaintext"
    ]);

    const language = result.language ?? null;
    const relevance = result.relevance ?? 0;

    // Determine extension based on detected language and confidence
    let extension = "txt";
    if (language && relevance >= MIN_CONFIDENCE_THRESHOLD) {
        extension = LANGUAGE_TO_EXTENSION[language] ?? "txt";
    }

    return {
        language,
        extension,
        highlightedHtml: result.value,
        relevance
    };
}

/**
 * Get the file extension for a known language identifier.
 * 
 * @param language - The highlight.js language identifier
 * @returns The file extension (without dot) or "txt" if unknown
 */
export function getExtensionForLanguage(language: string): string {
    return LANGUAGE_TO_EXTENSION[language] ?? "txt";
}

/**
 * Get syntax-highlighted HTML for code content.
 * If the language is known (from file extension), use it directly.
 * Otherwise, auto-detect the language.
 * 
 * @param code - The source code to highlight
 * @param language - Optional language hint (e.g., from file extension)
 * @returns Object with highlighted HTML and detected/used language
 */
export async function highlightCode(
    code: string,
    language?: string
): Promise<{ html: string; language: string | null }> {
    const hljs = await getHljs();

    // Past a point, highlighting costs more than it is worth: this runs on the UI thread
    // and a large file would block the window while it renders. Show it as plain text.
    if (code.length > HIGHLIGHT_LIMIT) {
        return { html: escapeHtml(code), language: language ?? null };
    }

    if (language && hljs.getLanguage(language)) {
        // Use the specified language
        const result = hljs.highlight(code, { language });
        return { html: result.value, language };
    }

    // Auto-detect from a sample, then highlight the whole text with the single language
    // that came back. detectLanguage only inspects the first few kilobytes, so its own
    // HTML covers the sample rather than the file - and one pass with a known language
    // beats highlightAuto scoring the full text against every candidate anyway.
    const detection = await detectLanguage(code);
    if (detection.language && hljs.getLanguage(detection.language)) {
        const result = hljs.highlight(code, { language: detection.language });
        return { html: result.value, language: detection.language };
    }

    return { html: escapeHtml(code), language: detection.language };
}

/** Render text safely when it is not being highlighted. */
function escapeHtml(text: string): string {
    return text
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;');
}

/**
 * Map file extension to highlight.js language identifier.
 * This is the reverse of LANGUAGE_TO_EXTENSION.
 */
const EXTENSION_TO_LANGUAGE: Record<string, string> = {
    // Build reverse map plus add common aliases
    "c": "c",
    "h": "c",
    "cpp": "cpp",
    "hpp": "cpp",
    "cc": "cpp",
    "cxx": "cpp",
    "cs": "csharp",
    "m": "objectivec",
    "js": "javascript",
    "mjs": "javascript",
    "cjs": "javascript",
    "ts": "typescript",
    "mts": "typescript",
    "cts": "typescript",
    "jsx": "javascript",  // highlight.js uses javascript for jsx
    "tsx": "typescript",  // highlight.js uses typescript for tsx
    "html": "html",
    "htm": "html",
    "xml": "xml",
    "svg": "xml",
    "css": "css",
    "scss": "scss",
    "sass": "scss",
    "less": "less",
    "json": "json",
    "py": "python",
    "pyw": "python",
    "rb": "ruby",
    "php": "php",
    "pl": "perl",
    "pm": "perl",
    "lua": "lua",
    "java": "java",
    "kt": "kotlin",
    "kts": "kotlin",
    "scala": "scala",
    "groovy": "groovy",
    "rs": "rust",
    "go": "go",
    "swift": "swift",
    "sh": "bash",
    "bash": "bash",
    "zsh": "bash",
    "fish": "bash",
    "ps1": "powershell",
    "psm1": "powershell",
    "bat": "bat",
    "cmd": "bat",
    "yaml": "yaml",
    "yml": "yaml",
    "toml": "toml",
    "ini": "ini",
    "cfg": "ini",
    "conf": "ini",
    "md": "markdown",
    "markdown": "markdown",
    "tex": "latex",
    "sql": "sql",
    "hs": "haskell",
    "erl": "erlang",
    "ex": "elixir",
    "exs": "elixir",
    "fs": "fsharp",
    "fsx": "fsharp",
    "clj": "clojure",
    "cljs": "clojure",
    "lisp": "lisp",
    "scm": "scheme",
    "r": "r",
    "dockerfile": "dockerfile",
    "makefile": "makefile",
    "cmake": "cmake",
    "diff": "diff",
    "patch": "diff",
    "txt": "plaintext",
    "log": "plaintext",
    "svelte": "html",  // Svelte uses HTML-like syntax
    "vue": "html",     // Vue SFC uses HTML-like syntax
};

/**
 * Get the highlight.js language identifier for a file extension.
 * 
 * @param extension - The file extension (without dot)
 * @returns The highlight.js language identifier or null if unknown
 */
export function getLanguageForExtension(extension: string): string | null {
    return EXTENSION_TO_LANGUAGE[extension.toLowerCase()] ?? null;
}
