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
import { Marked } from "marked";
import { markedHighlight } from "marked-highlight";
import hljs from "highlight.js";
import DOMPurify from "dompurify";
import type { RefState } from "./stash-refs";

/**
 * How a `stash:` link should look, supplied by the component rendering it.
 *
 * `label` is the target's live first line; without it the label stored in the link
 * is shown. `note` is a short, already translated suffix such as "moved to Work".
 */
export interface RefDisplay {
    state: RefState | "unknown";
    label?: string;
    note?: string;
}

export type RefResolver = (id: string) => RefDisplay;

// Set only for the duration of one synchronous `safeParse` call. marked renders
// synchronously, so no other parse can observe it.
let activeRefResolver: RefResolver | null = null;

const STASH_HREF = /^stash:([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})$/;

function escapeHtml(text: string): string {
    return text
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;");
}

// Initialize marked with syntax highlighting
// Using a singleton ensures we don't accidentally stack extensions if we used marked.use() repeatedly in components
const markedInstance = new Marked(
    markedHighlight({
        langPrefix: 'hljs language-',
        highlight(code, lang) {
            const language = hljs.getLanguage(lang) ? lang : 'plaintext';
            return hljs.highlight(code, { language }).value;
        }
    }),
    {
        renderer: {
            // A `stash:` link becomes a reference chip. It is emitted as a span with the
            // id in `title`, because DOMPurify drops the scheme from `href` and every
            // data attribute; `injectRefs` turns it into the interactive chip after
            // sanitizing. Any other link renders as marked always did.
            link(token) {
                const match = STASH_HREF.exec(token.href ?? "");
                if (!match) return false;
                const id = match[1].toLowerCase();
                const display = activeRefResolver?.(id) ?? { state: "unknown" as const };
                const label = display.label || token.text || id.slice(0, 8);
                const note = display.note
                    ? `<span class="stash-ref-note">${escapeHtml(display.note)}</span>`
                    : "";
                return (
                    `<span class="stash-ref stash-ref-${display.state}" title="stash:${id}">` +
                    `<span class="stash-ref-label">${escapeHtml(label)}</span>${note}</span>`
                );
            },
        },
        extensions: [
            {
                name: 'mention',
                level: 'inline',
                start(src) {
                    const match = src.match(/(?:^|\s)(@\S+)/);
                    if (match && match.index !== undefined) {
                        return match.index + (match[0].match(/^\s/) ? 1 : 0);
                    }
                    return undefined;
                },
                tokenizer(src, tokens) {
                    const rule = /^@\S+/;
                    const match = rule.exec(src);
                    if (match) {
                        return {
                            type: 'mention',
                            raw: match[0],
                            text: match[0]
                        };
                    }
                },
                renderer(token) {
                    return `<span class="ai-mention">${token.text}</span>`;
                }
            },
            {
                name: 'command',
                level: 'inline',
                start(src) {
                    const match = src.match(/(?:^|\s)(\/\S+)/);
                    if (match && match.index !== undefined) {
                        // Adjust index if matched with leading whitespace
                        return match.index + (match[0].match(/^\s/) ? 1 : 0);
                    }
                    return undefined;
                },
                tokenizer(src, tokens) {
                    const rule = /^\/\S+/;
                    const match = rule.exec(src);
                    if (match) {
                        return {
                            type: 'command',
                            raw: match[0],
                            text: match[0]
                        };
                    }
                },
                renderer(token) {
                    return `<span class="ai-command">${token.text}</span>`;
                }
            },
            {
                name: 'color',
                level: 'inline',
                start(src) {
                    const match = src.match(/(?:^|[^a-zA-Z0-9_])#([0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})(?![a-zA-Z0-9_-])/);
                    if (match && match.index !== undefined) {
                        return match.index + (match[0].startsWith('#') ? 0 : 1);
                    }
                    return undefined;
                },
                tokenizer(src, tokens) {
                    const match = isHexColor(src);
                    if (match) {
                        return {
                            type: 'color',
                            raw: match,
                            text: match
                        };
                    }
                },
                renderer(token) {
                    // Similar to tags, we emit raw text and replace after DOMPurify
                    return token.raw;
                }
            },
            {
                name: 'tag',
                level: 'inline',
                start(src) {
                    const match = src.match(/(?:^|[^a-zA-Z0-9_])#[\w-]+/);
                    if (match && match.index !== undefined) {
                        return match.index + (match[0].startsWith('#') ? 0 : 1);
                    }
                    return undefined;
                },
                tokenizer(src, tokens) {
                    const rule = /^#[\w-]+/;
                    const match = rule.exec(src);
                    if (match) {
                        // Check if it's a hex color (Option 1: Strict exclusion)
                        if (isHexColor(match[0])) {
                            return; // Let the color extension handle it or just treat as plain text
                        }
                        return {
                            type: 'tag',
                            raw: match[0],
                            text: match[0]
                        };
                    }
                },
                renderer(token) {
                    return token.raw;
                }
            }
        ]
    }
);

/**
 * Lucide Hash icon at 10 px — identical to the icon used in TagBadge.svelte.
 */
const HASH_ICON_SVG =
    `<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" ` +
    `viewBox="0 0 24 24" fill="none" stroke="currentColor" ` +
    `stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">` +
    `<line x1="4" x2="20" y1="9" y2="9"/>` +
    `<line x1="4" x2="20" y1="15" y2="15"/>` +
    `<line x1="10" x2="8" y1="3" y2="21"/>` +
    `<line x1="16" x2="14" y1="3" y2="21"/>` +
    `</svg>`;

export function getTagHue(text: string): number {
    let hash = 0;
    for (let i = 0; i < text.length; i++) {
        hash = text.charCodeAt(i) + ((hash << 5) - hash);
    }
    return Math.abs(hash % 360);
}

/**
 * Check if a string is a hex color (#HHH, #HHHHHH, #HHHHHHHH).
 * Returns the matched hex string or null.
 */
export function isHexColor(text: string): string | null {
    const hexRule = /^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})(?![a-zA-Z0-9_-])/;
    const match = hexRule.exec(text);
    return match ? match[0] : null;
}

/**
 * Extract all unique tags and hex colors from content, 
 * respecting word boundaries to avoid URL anchors.
 */
export function extractTagsAndColors(content: string): { tags: string[], colors: string[] } {
    const regex = /(?:^|[^a-zA-Z0-9_])(#[\w-]+)/g;
    const tags = new Set<string>();
    const colors = new Set<string>();

    const matches = content.matchAll(regex);
    for (const match of matches) {
        const fullTag = match[1]; // The first capture group is #[\w-]+
        if (isHexColor(fullTag)) {
            colors.add(fullTag);
        } else {
            tags.add(fullTag);
        }
    }

    return {
        tags: Array.from(tags).sort(),
        colors: Array.from(colors).sort()
    };
}

/**
 * Sanitize HTML output using DOMPurify to prevent XSS attacks.
 * Allows safe tags needed for markdown rendering, syntax highlighting,
 * and custom tag/mention/command extensions.
 */
export function sanitizeHtml(html: string): string {
    return DOMPurify.sanitize(html, {
        ALLOWED_TAGS: [
            // Markdown block elements
            'p', 'br', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6',
            'blockquote', 'pre', 'code', 'hr',
            // Lists
            'ul', 'ol', 'li',
            // Inline formatting
            'strong', 'em', 'del', 's', 'sub', 'sup', 'mark',
            // Links & images
            'a', 'img',
            // Tables
            'table', 'thead', 'tbody', 'tr', 'th', 'td',
            // Custom extension elements (tags, mentions, commands)
            'span',
            // Definition lists
            'dl', 'dt', 'dd',
            // Details/summary
            'details', 'summary',
        ],
        ALLOWED_ATTR: [
            // Link/image attributes
            'href', 'src', 'alt', 'title', 'target', 'rel',
            // Styling (for highlight.js classes, tag badge hue via --hue CSS var, etc.)
            'class', 'style',
            // Table attributes
            'align', 'valign', 'colspan', 'rowspan',
            // Misc
            'id', 'name', 'open',
        ],
        // Allow data: URIs for base64 images but not javascript:
        ALLOW_DATA_ATTR: false,
        ALLOWED_URI_REGEXP: /^(?:(?:https?|mailto|tel|asset):|data:image\/)/i,
    });
}

/**
 * Replace #tag and #hex occurrences in sanitized HTML with badges.
 * Ensures we don't match inside HTML tags (like href anchors).
 */
function injectBadges(html: string): string {
    // 1. Split by blocks that we MUST NOT touch (pre, code)
    const blocks = html.split(/(<pre[\s\S]*?<\/pre>|<code[\s\S]*?<\/code>)/gi);

    return blocks.map((block, i) => {
        if (i % 2 === 1) return block; // It's pre/code, return as is

        // 2. Split the remaining HTML by any HTML tag to avoid matching in attributes
        const segments = block.split(/(<[^>]*>)/g);
        return segments.map((segment, j) => {
            if (j % 2 === 1) return segment; // It's an HTML tag like <a href="...">

            // 3. Process text nodes
            return segment.replace(/(^|[^a-zA-Z0-9_])(#[\w-]+)/g, (match, prefix, fullTag) => {
                if (isHexColor(fullTag)) {
                    // Return ColorBadge HTML
                    return (
                        prefix +
                        `<span class="ai-color">` +
                        `<span class="ai-color-swatch" style="background-color: ${fullTag};"></span>` +
                        `<span class="font-mono lowercase opacity-90">${fullTag}</span>` +
                        `</span>`
                    );
                }

                const label = fullTag.slice(1);
                const h = getTagHue(fullTag);
                const style = [
                    `--tag-bg: hsla(${h}, 75%, 45%, 0.1)`,
                    `--tag-bg-dark: hsla(${h}, 80%, 70%, 0.1)`,
                    `--tag-border: hsla(${h}, 75%, 45%, 0.2)`,
                    `--tag-border-dark: hsla(${h}, 80%, 70%, 0.2)`,
                    `--tag-text: hsl(${h}, 75%, 45%)`,
                    `--tag-text-dark: hsl(${h}, 80%, 70%)`,
                ].join('; ');

                return (
                    prefix +
                    `<span class="ai-tag" style="${style}">` +
                    `${HASH_ICON_SVG}${label}` +
                    `</span>`
                );
            });
        }).join('');
    }).join('');
}

/**
 * Give the reference placeholders left by the link renderer their behaviour.
 *
 * Only the exact shape the renderer emits is matched, and the id must be a UUID,
 * so nothing a user typed can smuggle an attribute through. A hand-written span of
 * the same shape just becomes a chip, which is harmless.
 */
function injectRefs(html: string): string {
    return html.replace(
        /<span class="stash-ref stash-ref-(open|completed|other_context|gone|unknown)" title="stash:([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})">/g,
        (_, state: string, id: string) =>
            `<span class="stash-ref stash-ref-${state}" data-ref-id="${id}"` +
            (state === "gone" ? "" : ` role="link" tabindex="0"`) +
            `>`,
    );
}

/**
 * Parse markdown content and sanitize the output to prevent XSS.
 * Use this instead of raw `marked.parse()` + `{@html}`.
 *
 * Pipeline:
 *  1. marked  – tokenises markdown, emits placeholders
 *  2. DOMPurify – strips anything unsafe (XSS prevention)
 *  3. injectBadges – expands placeholders into fully-styled badge HTML
 *  4. injectRefs – makes `stash:` reference chips clickable
 *
 * `resolveRef` supplies the live label and state of each referenced stash.
 */
export function safeParse(content: string, resolveRef?: RefResolver): string {
    activeRefResolver = resolveRef ?? null;
    let raw: string;
    try {
        raw = markedInstance.parse(content) as string;
    } finally {
        activeRefResolver = null;
    }
    const sanitized = sanitizeHtml(raw);
    return injectRefs(injectBadges(sanitized));
}


export default markedInstance;
