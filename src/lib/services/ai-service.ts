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

OUTPUT FORMAT:
- Start with a single ACTION line (imperative verb + clear objective)
- Add CONTEXT bullet points only if essential (max 3)
- End with CONSTRAINTS if there are specific requirements

RULES:
1. Be extremely concise - every word must add value
2. Remove fluff, greetings, and unnecessary explanations
3. Use imperative voice ("Implement X" not "Please implement X")
4. Preserve all technical terms, code, file paths, and specifics
5. Structure for scannability - use Markdown bullets (- or *), not paragraphs or Unicode bullets (•)
6. Use valid Markdown formatting throughout

Return ONLY the enhanced prompt. No meta-commentary.`;

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
        // API key is optional for local LLMs like Ollama, LM Studio
        if (!config.enabled || !config.endpoint || !config.model) {
            throw new Error('AI configuration is incomplete');
        }

        // Build the API URL
        const url = `${config.endpoint.replace(/\/$/, '')}/chat/completions`;

        // Build system prompt with optional project context
        const systemPrompt = buildSystemPrompt(context);

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
        const enhancedContent = data.choices?.[0]?.message?.content;
        if (!enhancedContent) {
            throw new Error('No content in API response');
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
