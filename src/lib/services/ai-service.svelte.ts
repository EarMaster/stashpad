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
import { listen } from '@tauri-apps/api/event';
import { DEFAULT_SYSTEM_PROMPT } from '$lib/constants/ai-prompts';

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
 * Service for AI-powered prompt enhancement.
 * Uses OpenAI-compatible APIs to improve stash content.
 */
export class AIService {
    private _systemPrompt = $state(DEFAULT_SYSTEM_PROMPT);
    private _systemPromptPath = $state('');
    private _promptFileExists = $state(false);

    constructor() {
        // Load initial prompt and path
        this.refreshPrompt();

        // Listen for changes from the backend
        listen('system-prompt-changed', () => {
            this.refreshPrompt(true);
        });
    }

    /**
     * Get the current system prompt content
     */
    get systemPrompt() {
        return this._systemPrompt;
    }

    /**
     * Get the absolute path to the system prompt file
     */
    get systemPromptPath() {
        return this._systemPromptPath;
    }

    /**
     * Check if the system prompt file exists on disk
     */
    get promptFileExists() {
        return this._promptFileExists;
    }

    /**
     * Refresh the prompt from disk
     * @param notify - Whether to trigger a notification (handled externally via reactivity)
     */
    private async refreshPrompt(notify = false) {
        try {
            const [content, path, exists] = await Promise.all([
                adapter.getSystemPrompt(),
                adapter.getSystemPromptPath(),
                adapter.checkSystemPromptExists()
            ]);
            this._systemPrompt = content;
            this._systemPromptPath = path;
            this._promptFileExists = exists;

            if (notify) {
                // We'll use a custom event for the UI to show a notification
                window.dispatchEvent(new CustomEvent('stashpad:prompt-reloaded'));
            }
        } catch (e) {
            console.error('Failed to refresh system prompt:', e);
        }
    }

    /**
     * Create the prompt file on disk manually using the fallback
     */
    async createPromptFile() {
        try {
            await adapter.createPromptFile();
            await this.refreshPrompt(true);
            await adapter.openSystemPromptFile();
        } catch (e) {
            console.error('Failed to create prompt file:', e);
        }
    }

    /**
     * Build the system prompt, optionally injecting project context.
     * @param context - Optional project context information
     * @returns The complete system prompt
     */
    buildSystemPrompt(context?: AIEnhanceContext): string {
        const basePrompt = this._systemPrompt;

        // If no context info is available, return the base prompt
        if (!context?.contextName && !context?.contextDescription && !context?.windowTitle) {
            return basePrompt;
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
            return basePrompt;
        }

        const projectSection = `
<project_context>
${contextLines.join('\n')}

Use this context to make the prompt more specific and relevant to the project.
</project_context>`;

        return basePrompt + projectSection;
    }

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

        const systemPrompt = this.buildSystemPrompt(context);

        // Handle Apple Intelligence separately
        if (isAppleIntelligencePreset(config.presetId)) {
            return await adapter.appleIntelligenceEnhance(content, systemPrompt);
        }

        // API key is optional
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
                { role: 'user', content: `<user_notes>\n${content}\n</user_notes>` },
            ],
            temperature: 0.3, // Lower temperature for more consistent, focused output
            max_tokens: 1024, // Reduced since we want concise output
            stream: false,
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

        // API key is optional
        if (!config.endpoint || !config.model) {
            throw new Error('Configuration is incomplete');
        }

        const url = `${config.endpoint.replace(/\/$/, '')}/chat/completions`;

        const body = {
            model: config.model,
            messages: [
                { role: 'system', content: 'You are a connection tester.' },
                { role: 'user', content: 'Say "OK" to confirm the connection works.' },
            ],
            max_tokens: 10,
            stream: false,
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
