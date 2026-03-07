// SPDX-License-Identifier: AGPL-3.0-only
import { Marked } from "marked";
import { markedHighlight } from "marked-highlight";
import hljs from "highlight.js";
import DOMPurify from "dompurify";

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
                name: 'tag',
                level: 'inline',
                start(src) { return src.match(/#[\w-]+/)?.index; },
                tokenizer(src, tokens) {
                    const rule = /^#[\w-]+/;
                    const match = rule.exec(src);
                    if (match) {
                        return {
                            type: 'tag',
                            raw: match[0],
                            text: match[0]
                        };
                    }
                },
                renderer(token) {
                    // Emit the raw tag text (e.g. "#cloud") as plain text.
                    // The tokenizer still correctly distinguishes tags from
                    // markdown headings (which require a space after #).
                    // injectTagBadges() will replace these after DOMPurify.
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
 * Replace #tag occurrences in sanitized HTML with fully-styled badge HTML.
 *
 * Runs AFTER DOMPurify so we can inject SVG and computed styles freely.
 * Skips content inside <pre> and <code> blocks so that code examples are
 * never converted to badges.
 *
 * The CSS variables written here mirror the non-selected, non-interactive
 * branch of TagBadge.svelte so that inline badges and card-header badges
 * look identical.
 */
function injectTagBadges(html: string): string {
    // Split on pre/code blocks; odd-indexed parts are those blocks.
    const segments = html.split(/(<pre[\s\S]*?<\/pre>|<code[\s\S]*?<\/code>)/gi);

    return segments.map((segment, index) => {
        // Leave code / pre blocks untouched.
        if (index % 2 === 1) return segment;

        return segment.replace(/#([\w-]+)/g, (_, label) => {
            const h = getTagHue(`#${label}`);
            // Mirror the non-selected, non-interactive branch of TagBadge.svelte:
            const style = [
                `--tag-bg: hsla(${h}, 75%, 45%, 0.1)`,
                `--tag-bg-dark: hsla(${h}, 80%, 70%, 0.1)`,
                `--tag-border: hsla(${h}, 75%, 45%, 0.2)`,
                `--tag-border-dark: hsla(${h}, 80%, 70%, 0.2)`,
                `--tag-text: hsl(${h}, 75%, 45%)`,
                `--tag-text-dark: hsl(${h}, 80%, 70%)`,
            ].join('; ');
            return (
                `<span class="ai-tag" style="${style}">` +
                `${HASH_ICON_SVG}${label}` +
                `</span>`
            );
        });
    }).join('');
}

/**
 * Parse markdown content and sanitize the output to prevent XSS.
 * Use this instead of raw `marked.parse()` + `{@html}`.
 *
 * Pipeline:
 *  1. marked  – tokenises markdown, emits placeholder spans for #tags
 *  2. DOMPurify – strips anything unsafe (XSS prevention)
 *  3. injectTagBadges – expands placeholders into fully-styled badge HTML
 */
export function safeParse(content: string): string {
    const raw = markedInstance.parse(content) as string;
    const sanitized = sanitizeHtml(raw);
    return injectTagBadges(sanitized);
}

export default markedInstance;
