// SPDX-License-Identifier: AGPL-3.0-only
import { Marked } from "marked";
import { markedHighlight } from "marked-highlight";
import hljs from "highlight.js";

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
                start(src) { return src.match(/@[\w-]+/)?.index; },
                tokenizer(src, tokens) {
                    const rule = /^@[\w-]+/;
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
                start(src) { return src.match(/\/[\w-]+/)?.index; },
                tokenizer(src, tokens) {
                    const rule = /^\/[\w-]+/;
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
                    const hue = getTagHue(token.text);
                    // content is text without the #
                    const label = token.text.substring(1);
                    const icon = `<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" class="flex-shrink-0 mr-1"><line x1="4" x2="20" y1="9" y2="9"/><line x1="4" x2="20" y1="15" y2="15"/><line x1="10" x2="8" y1="3" y2="21"/><line x1="16" x2="14" y1="3" y2="21"/></svg>`;
                    return `<span class="ai-tag" style="--tag-hue: ${hue}">${icon}${label}</span>`;
                }
            }
        ]
    }
);

export function getTagHue(text: string): number {
    let hash = 0;
    for (let i = 0; i < text.length; i++) {
        hash = text.charCodeAt(i) + ((hash << 5) - hash);
    }
    return Math.abs(hash % 360);
}

export default markedInstance;
