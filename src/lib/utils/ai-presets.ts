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

import type { AIProviderPreset } from '$lib/types';

/**
 * List of OpenAI-compatible API provider presets.
 * These presets provide quick configuration for common AI providers.
 */
export const AI_PROVIDER_PRESETS: AIProviderPreset[] = [
    {
        id: 'apple-intelligence',
        name: 'Apple Intelligence',
        endpoint: '',
        defaultModel: '',
    },
    {
        id: 'openai',
        name: 'OpenAI',
        endpoint: 'https://api.openai.com/v1',
        defaultModel: 'gpt-4o-mini',
    },
    {
        id: 'azure',
        name: 'Azure OpenAI',
        endpoint: 'https://{resource}.openai.azure.com',
        defaultModel: 'gpt-4o-mini',
    },
    {
        id: 'groq',
        name: 'Groq',
        endpoint: 'https://api.groq.com/openai/v1',
        defaultModel: 'llama-3.3-70b-versatile',
    },
    {
        id: 'openrouter',
        name: 'OpenRouter',
        endpoint: 'https://openrouter.ai/api/v1',
        defaultModel: 'anthropic/claude-3.5-haiku',
    },
    {
        id: 'mistral',
        name: 'Mistral AI',
        endpoint: 'https://api.mistral.ai/v1',
        defaultModel: 'mistral-small-latest',
    },
    {
        id: 'deepseek',
        name: 'Deepseek',
        endpoint: 'https://api.deepseek.com/v1',
        defaultModel: 'deepseek-chat',
    },
    {
        id: 'ollama',
        name: 'Ollama (Local)',
        endpoint: 'http://localhost:11434/v1',
        defaultModel: 'llama3.3',
    },
    {
        id: 'lm-studio',
        name: 'LM Studio (Local)',
        endpoint: 'http://localhost:1234/v1',
        defaultModel: 'model-identifier',
    },
    {
        id: 'custom',
        name: 'Custom',
        endpoint: '',
        defaultModel: '',
    },
];

/**
 * Check if a preset ID refers to the Apple Intelligence preset
 * @param id - The preset ID to check
 * @returns True if the ID is apple-intelligence
 */
export function isAppleIntelligencePreset(id?: string): boolean {
    return id === 'apple-intelligence';
}

/**
 * Get a provider preset by its ID.
 * @param id - The preset ID to find
 * @returns The preset or undefined if not found
 */
export function getPresetById(id: string): AIProviderPreset | undefined {
    return AI_PROVIDER_PRESETS.find(preset => preset.id === id);
}

/**
 * Check if the AI configuration refers to a local provider.
 * This is used to provide context-specific error messages (e.g., CORS issues).
 * @param config - The AI configuration to check
 * @returns True if it's a known local provider or uses a local endpoint
 */
export function isLocalAIProvider(config?: { presetId?: string; endpoint?: string }): boolean {
    if (!config) return false;

    // Explicit local presets
    if (config.presetId === 'ollama' || config.presetId === 'lm-studio') return true;

    // Check for local endpoints (localhost, 127.0.0.1, [::1])
    if (config.endpoint) {
        const url = config.endpoint.toLowerCase();
        return url.includes('localhost') || url.includes('127.0.0.1') || url.includes('[::1]');
    }

    return false;
}

/**
 * Get the default AI configuration.
 * @returns A default AIConfig with all fields empty/disabled
 */
export function getDefaultAIConfig() {
    return {
        enabled: false,
        endpoint: '',
        apiKey: '',
        model: '',
        presetId: undefined,
    };
}
