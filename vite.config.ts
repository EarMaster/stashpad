// SPDX-License-Identifier: AGPL-3.0-only

// Copyright (C) 2025 Nico Wiedemann
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

import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import path from 'path';

export default defineConfig({
    plugins: [svelte()],
    resolve: {
        alias: {
            $lib: path.resolve('./src/lib'),
        },
    },
    build: {
        rollupOptions: {
            output: {
                manualChunks: {
                    // UI framework components
                    'vendor-ui': ['bits-ui', 'lucide-svelte', 'svelte-dnd-action', '@thisux/sveltednd'],
                    // Markdown rendering
                    'vendor-markdown': ['marked', 'marked-highlight'],
                    // Syntax highlighting (large — all language grammars)
                    'vendor-hljs': ['highlight.js'],
                    // ZIP archive support
                    'vendor-zip': ['jszip'],
                    // Tauri plugins
                    'vendor-tauri': [
                        '@tauri-apps/api',
                        '@tauri-apps/plugin-dialog',
                        '@tauri-apps/plugin-fs',
                        '@tauri-apps/plugin-autostart',
                    ],
                    // Internationalization
                    'vendor-i18n': ['svelte-i18n'],
                },
            },
        },
    },
    clearScreen: false,
    server: {
        port: 1420,
        strictPort: true,
        watch: {
            ignored: ['**/src-tauri/**'],
        },
    },
    envPrefix: ['VITE_', 'TAURI_'],
});
