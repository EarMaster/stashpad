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

import { mount } from 'svelte';
import './app.css';
import App from './App.svelte';
import { setupI18n } from '$lib/i18n';

// Initialize i18n before mounting the app
setupI18n().then(() => {
  const app = mount(App, {
    target: document.getElementById('app')!,
  });

  // Export for potential HMR usage
  (window as any).__app__ = app;
});
