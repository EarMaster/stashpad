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

use std::sync::{Arc, Mutex};
use crate::db::DbManager;
use crate::models::{AppContext, Context, Settings};

pub struct DbState {
    pub db: Arc<Mutex<DbManager>>,
}

pub struct WsState {
    /// Handle to the background task that manages the WebSocket connection
    pub task_handle: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

pub struct TrackerState {
    pub last_external_app: Option<AppContext>,
    pub current_context_id: Option<String>,
}

impl TrackerState {
    pub fn new() -> Self {
        Self {
            last_external_app: None,
            current_context_id: None,
        }
    }
}

pub struct SettingsState {
    pub settings: Mutex<Settings>,
}

pub struct ContextsState {
    pub contexts: Mutex<Vec<Context>>,
}
