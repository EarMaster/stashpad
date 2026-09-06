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

/**
 * The dummy dataset the screenshot demo runs on.
 *
 * Everything here is invented. No real project, path, key or account may appear in a
 * fixture: these end up on the public website, and a screenshot is the easiest place
 * to leak a customer name or a home directory without noticing.
 *
 * Timestamps are relative to a fixed instant (`NOW`) rather than to `Date.now()`, so a
 * capture taken today and one taken next month produce the same images. The app renders
 * "2h ago" style labels from these, which would otherwise churn every release.
 */

import type { Attachment, Context, Settings, StashItem } from '../src/lib/types';

/** The instant the fixtures pretend it is. */
export const NOW = new Date('2026-01-15T14:30:00.000Z');

/** An ISO timestamp `minutes` before `NOW`. */
function ago(minutes: number): string {
    return new Date(NOW.getTime() - minutes * 60_000).toISOString();
}

/**
 * Where the demo's fake attachments pretend to live.
 *
 * `convertFileSrc` is replaced in `mock-backend.ts` so these paths resolve to files the
 * dev server can actually serve, which means the file name has to match something in
 * `screenshots/assets/`.
 */
export const CACHE_ROOT = '/home/dev/.stashpad/cache';

function attachment(
    id: string,
    stashId: string,
    contextId: string,
    fileName: string,
    fileSize: number,
    mimeType: string,
    extra: Partial<Attachment> = {},
): Attachment {
    return {
        id,
        stashId,
        filePath: `${CACHE_ROOT}/${contextId}/${stashId}/${fileName}`,
        fileName,
        fileSize,
        mimeType,
        createdAt: ago(90),
        ...extra,
    };
}

export const contexts: Context[] = [
    {
        id: 'orbit-api',
        name: 'Orbit API',
        description:
            'Rust and Axum service behind the mobile app. Postgres via sqlx, deployed on Fly.',
        rules: [
            { ruleType: 'process', value: 'Code.exe', matchType: 'contains' },
            { ruleType: 'title', value: 'orbit-api', matchType: 'contains' },
        ],
        lastUsed: ago(4),
    },
    {
        id: 'orbit-web',
        name: 'Orbit Web',
        description: 'SvelteKit front end. Tailwind, Vitest, Playwright.',
        rules: [{ ruleType: 'title', value: 'orbit-web', matchType: 'contains' }],
        lastUsed: ago(180),
    },
    {
        id: 'infra',
        name: 'Infrastructure',
        description: 'Terraform modules, CI pipelines, on-call runbooks.',
        rules: [{ ruleType: 'process', value: 'WindowsTerminal.exe', matchType: 'contains' }],
        lastUsed: ago(1440),
    },
];

export const stashes: StashItem[] = [
    {
        id: 'stash-1',
        contextId: 'orbit-api',
        createdAt: ago(6),
        updatedAt: ago(6),
        content:
            'The upload handler returns 500 whenever a file name repeats inside one request. #bug\n\n' +
            'The second file overwrites the first on disk and the row still points at the old ' +
            'size. Log is attached, line 44 is where it gives up.',
        attachments: [
            attachment('att-1', 'stash-1', 'orbit-api', 'upload-failure.log', 4_812, 'text/plain', {
                syntax: 'log',
            }),
        ],
    },
    {
        id: 'stash-2',
        contextId: 'orbit-api',
        createdAt: ago(48),
        updatedAt: ago(48),
        content:
            'The queue drops a row when two arrive in the same tick. #ui\n\n' +
            'Screenshot is from the staging build; the second item is the one that never renders.',
        attachments: [
            attachment('att-2', 'stash-2', 'orbit-api', 'queue-glitch.svg', 138_402, 'image/svg+xml'),
        ],
    },
    {
        id: 'stash-3',
        contextId: 'orbit-api',
        createdAt: ago(95),
        updatedAt: ago(95),
        content:
            'Give `resolve_tenant` the pool instead of letting it open its own connection. #refactor\n\n' +
            '```rust\npub async fn resolve_tenant(pool: &PgPool, host: &str) -> Result<Tenant> {\n' +
            '    sqlx::query_as!(Tenant, "select * from tenants where host = $1", host)\n' +
            '        .fetch_one(pool)\n' +
            '        .await\n' +
            '}\n```',
        attachments: [
            attachment('att-3', 'stash-3', 'orbit-api', 'tenant.rs', 2_104, 'text/x-rust', {
                syntax: 'rust',
            }),
        ],
    },
    {
        id: 'stash-4',
        contextId: 'orbit-api',
        createdAt: ago(300),
        updatedAt: ago(300),
        content:
            'Sync retries forever when the server rejects an attachment. Back off, and name the ' +
            'file instead of quoting the raw reply. #bug #sync',
        attachments: [],
    },
    {
        id: 'stash-5',
        contextId: 'orbit-api',
        createdAt: ago(600),
        updatedAt: ago(600),
        content: 'Ask design whether an empty queue should keep the filter bar. #question',
        attachments: [],
    },
    {
        id: 'stash-6',
        contextId: 'orbit-api',
        createdAt: ago(1_500),
        completed: true,
        completedAt: ago(1_400),
        updatedAt: ago(1_400),
        content: 'Pin the CI toolchain so the release build stops drifting. #infra',
        attachments: [],
    },
    {
        id: 'stash-7',
        contextId: 'orbit-web',
        createdAt: ago(200),
        updatedAt: ago(200),
        content: 'The dark theme leaves tag chips unreadable on the light panel. #ui',
        attachments: [],
    },
];

/** The settings the demo boots with: signed in to cloud sync, AI configured. */
export const settings: Settings = {
    autoContextDetection: true,
    visualEffectsEnabled: false,
    activeContextId: 'orbit-api',
    shortcuts: {
        switch_context: 'CommandOrControl+P',
        global_stash: 'CommandOrControl+Shift+S',
    },
    locale: 'auto',
    newStashPosition: 'top',
    theme: 'dark',
    uiScale: 3,
    stripTagsOnCopy: true,
    clearCompletedStrategy: 'after-n-days',
    clearCompletedDays: 7,
    pasteAsAttachmentThreshold: 500,
    autostart: true,
    resizeImages: true,
    autoUpdateChecks: true,
    aiConfig: {
        enabled: true,
        endpoint: 'https://api.openai.com/v1',
        apiKey: 'sk-demo-0000000000000000',
        model: 'gpt-4o-mini',
        presetId: 'openai',
    },
    cloudConfig: {
        enabled: true,
        endpoint: 'https://api.stashpad.org',
        userId: 'usr_demo',
        email: 'dev@example.com',
        subscriptionTier: 'pro',
        subscriptionStatus: 'active',
        subscriptionPeriodEnd: new Date(NOW.getTime() + 21 * 86_400_000).toISOString(),
        lastSyncAt: ago(2),
    },
};

/** What the backend reports as the previously focused application. */
export const previousApp = {
    windowTitle: 'orbit-api - attachments.rs - Visual Studio Code',
    processName: 'Code.exe',
    detectedContextId: 'orbit-api',
};

/**
 * What the preview modal shows for each text attachment.
 *
 * Kept here rather than as files under `assets/` so a fixture cannot be mistaken for
 * project source - a `tenant.rs` sitting in the repo would be swept up by the AGPL
 * header check and by anything else that globs for Rust.
 */
export const previewText: Record<string, string> = {
    'upload-failure.log': [
        '[13:58:02 INFO  orbit_api::attachments] POST /v1/stashes/9f2a/attachments (2 files)',
        '[13:58:02 DEBUG orbit_api::storage] writing cache/orbit-api/9f2a/image.png (412 KB)',
        '[13:58:02 DEBUG orbit_api::storage] writing cache/orbit-api/9f2a/image.png (188 KB)',
        '[13:58:02 WARN  orbit_api::storage] destination exists, overwriting',
        '[13:58:03 ERROR orbit_api::attachments] size mismatch for att_7731: 412104 recorded, 192880 on disk',
        '[13:58:03 ERROR orbit_api::attachments] upload rejected: 500 Internal Server Error',
    ].join('\n'),
    'tenant.rs': [
        'pub async fn resolve_tenant(pool: &PgPool, host: &str) -> Result<Tenant> {',
        '    sqlx::query_as!(Tenant, "select * from tenants where host = $1", host)',
        '        .fetch_one(pool)',
        '        .await',
        '        .map_err(Error::from)',
        '}',
    ].join('\n'),
};

/** Cloud storage figures for the settings panel. */
export const cloudUsage = {
    usedBytes: 412_733_337,
    quotaBytes: 2_147_483_648,
    attachmentCount: 37,
};
