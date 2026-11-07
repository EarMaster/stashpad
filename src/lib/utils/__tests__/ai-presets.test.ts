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

import { describe, it, expect } from 'vitest';
import { isAIUsable } from '../ai-presets';

describe('isAIUsable', () => {
    it('is false with no configuration or with AI switched off', () => {
        expect(isAIUsable(undefined)).toBe(false);
        expect(isAIUsable({ enabled: false, endpoint: 'https://x/v1', model: 'm' })).toBe(false);
    });

    it('needs an endpoint and a model for an ordinary provider', () => {
        expect(isAIUsable({ enabled: true, endpoint: 'https://x/v1', model: 'm' })).toBe(true);
        expect(isAIUsable({ enabled: true, endpoint: '', model: 'm' })).toBe(false);
        expect(isAIUsable({ enabled: true, endpoint: 'https://x/v1', model: '  ' })).toBe(false);
    });

    it('needs nothing more for Apple Intelligence, which has no endpoint', () => {
        expect(isAIUsable({ enabled: true, presetId: 'apple-intelligence' })).toBe(true);
        expect(isAIUsable({ enabled: false, presetId: 'apple-intelligence' })).toBe(false);
    });
});
