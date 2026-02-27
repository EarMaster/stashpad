// SPDX-License-Identifier: AGPL-3.0-only

// Copyright (C) 2026 Nico Wiedemann
//
// This file is part of Stashpad.
// Stashpad is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Affero General Public License for more details.

import type { AIConfig } from '$lib/types';
import { DesktopStorageAdapter } from '$lib/services/desktop-adapter';
import { isAppleIntelligencePreset } from '$lib/utils/ai-presets';

const adapter = new DesktopStorageAdapter();

/**
 * Context information to inject into AI enhancement prompts.
 * Provides project awareness for more relevant prompt generation.
 */
export interface AIEnhanceContext {
    /** Name of the current context/project */
    contextName?: string;
    /** User-provided description of the project (tech stack, conventions) */
    contextDescription?: string;
    /** Active window title - only provided when auto-detection matched */
    windowTitle?: string;
}

/**
 * Base system prompt for AI enhancement.
 * Uses prompt engineering best practices to create structured, effective AI agent prompts.
 */
const BASE_SYSTEM_PROMPT = `You are an expert prompt engineer. Transform raw notes into clear, structured AI agent prompts.

OUTPUT FORMAT TEMPLATE:
ACTION: <Short, imperative action line>
CONTEXT: 
- <Essential context item 1> (Max 3, omit section if none)
CONSTRAINTS: 
- <Specific requirement 1> (Omit section if none)
TAGS: <Only hashtags present in original input, space-separated. Omit section entirely if none.>

RULES:
1. Be extremely concise - every word must add value.
2. Remove fluff, greetings, and unnecessary explanations.
3. Use imperative voice ("Implement X" not "Please implement X").
4. Preserve all technical terms, code, file paths, and specifics EXACTLY.
5. HASHTAGS (#): ONLY preserve hashtags that were in the ORIGINAL input. DO NOT suggest or add NEW hashtags.
6. If no hashtags were in the input, the output MUST NOT contain the word "TAGS" or any hashtags.
7. Structure for scannability - use Markdown bullets (-), not paragraphs.
8. Use valid Markdown formatting throughout.
9. Make sure to preserve all aspects of the original input.
10. Follow the output format template exactly.

Return ONLY the enhanced prompt following the template above. No meta-commentary.`;

/**
 * Build the system prompt, optionally injecting project context.
 * @param context - Optional project context information
 * @returns The complete system prompt
 */
function buildSystemPrompt(context?: AIEnhanceContext): string {
    // If no context info is available, return the base prompt
    if (!context?.contextName && !context?.contextDescription && !context?.windowTitle) {
        return BASE_SYSTEM_PROMPT;
    }

    // Build project context section
    const contextLines: string[] = [];
    if (context.contextName && context.contextName !== 'Default') {
        contextLines.push(`- Project: ${context.contextName}`);
    }
    if (context.contextDescription) {
        contextLines.push(`- Details: ${context.contextDescription}`);
    }
    if (context.windowTitle) {
        contextLines.push(`- Active file/window: ${context.windowTitle}`);
    }

    if (contextLines.length === 0) {
        return BASE_SYSTEM_PROMPT;
    }

    const projectSection = `
PROJECT CONTEXT:
${contextLines.join('\n')}

Use this context to make the prompt more specific and relevant to the project.`;

    return BASE_SYSTEM_PROMPT + projectSection;
}

/**
 * Service for AI-powered prompt enhancement.
 * Uses OpenAI-compatible APIs to improve stash content.
 */
export class AIService {
    /**
     * Enhance a stash prompt using the configured AI provider.
     * @param content - The original stash content
     * @param config - The AI configuration
     * @param context - Optional project context for more relevant enhancement
     * @returns Enhanced prompt text
     * @throws Error if the API call fails
     */
    async enhancePrompt(
        content: string,
        config: AIConfig,
        context?: AIEnhanceContext
    ): Promise<string> {
        if (!config.enabled) {
            throw new Error('AI configuration is disabled');
        }

        const systemPrompt = buildSystemPrompt(context);

        // Handle Apple Intelligence separately
        if (isAppleIntelligencePreset(config.presetId)) {
            return await adapter.appleIntelligenceEnhance(content, systemPrompt);
        }

        // API key is optional for local LLMs like Ollama, LM Studio
        if (!config.endpoint || !config.model) {
            throw new Error('AI configuration is incomplete');
        }

        // Build the API URL
        const url = `${config.endpoint.replace(/\/$/, '')}/chat/completions`;

        // Prepare the request body
        const body = {
            model: config.model,
            messages: [
                { role: 'system', content: systemPrompt },
                { role: 'user', content: content },
            ],
            temperature: 0.3, // Lower temperature for more consistent, focused output
            max_tokens: 1024, // Reduced since we want concise output
        };

        // Build headers - only add Authorization if API key is provided
        const headers: Record<string, string> = {
            'Content-Type': 'application/json',
        };
        if (config.apiKey) {
            headers['Authorization'] = `Bearer ${config.apiKey}`;
        }

        // Make the API request
        const response = await fetch(url, {
            method: 'POST',
            headers,
            body: JSON.stringify(body),
        });

        if (!response.ok) {
            const errorText = await response.text();
            throw new Error(`API error: ${response.status} - ${errorText}`);
        }

        const data = await response.json();

        // Extract the enhanced content from the response
        let enhancedContent = data.choices?.[0]?.message?.content;
        if (!enhancedContent) {
            throw new Error('No content in API response');
        }

        // Post-process to ensure no hallucinated tag headers if original had none
        const originalTags = content.match(/#[\w-]+/g);
        if (!originalTags) {
            // Remove common variations of the tag header if it appears at the end
            enhancedContent = enhancedContent.replace(/\n*^(TAGS:|#Tags:|Tags:).*$/im, "");
        }

        return enhancedContent.trim();
    }

    /**
     * Test the AI connection with a simple request.
     * @param config - The AI configuration to test
     * @returns true if the connection is successful
     * @throws Error if the connection fails
     */
    async testConnection(config: AIConfig): Promise<boolean> {
        // Handle Apple Intelligence separately
        if (isAppleIntelligencePreset(config.presetId)) {
            const available = await adapter.checkAppleIntelligenceAvailable();
            if (!available) {
                throw new Error('Apple Intelligence is not available on this device');
            }
            return true;
        }

        // API key is optional for local LLMs
        if (!config.endpoint || !config.model) {
            throw new Error('Configuration is incomplete');
        }

        const url = `${config.endpoint.replace(/\/$/, '')}/chat/completions`;

        const body = {
            model: config.model,
            messages: [
                { role: 'user', content: 'Say "OK" to confirm the connection works.' },
            ],
            max_tokens: 10,
        };

        // Build headers - only add Authorization if API key is provided
        const headers: Record<string, string> = {
            'Content-Type': 'application/json',
        };
        if (config.apiKey) {
            headers['Authorization'] = `Bearer ${config.apiKey}`;
        }

        const response = await fetch(url, {
            method: 'POST',
            headers,
            body: JSON.stringify(body),
        });

        if (!response.ok) {
            const errorText = await response.text();
            throw new Error(`${response.status}: ${errorText}`);
        }

        return true;
    }
}

// Singleton instance
export const aiService = new AIService();
