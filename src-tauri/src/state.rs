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

use std::sync::{Arc, Mutex, MutexGuard};
use crate::db::DbManager;
use crate::models::{AppContext, Settings};

/// Take a lock, recovering the guard if a previous holder panicked.
///
/// `std::sync::Mutex` poisons itself when a thread panics while holding it, so the
/// usual `.lock().unwrap()` turns one panic into a permanent outage: every later
/// lock panics too, for the life of the process. Because Tauri does not
/// `catch_unwind` command bodies, a panicking command also drops its IPC responder
/// without answering, so the webview's `await invoke(...)` never settles - not
/// resolve, not reject. The two together are exactly the "app frozen, must be
/// force-closed" report: the window still paints while every command is dead.
///
/// The data behind these mutexes is a database handle, a settings struct and a
/// window tracker. A panic mid-update can leave them stale, never structurally
/// broken, so continuing with the value is strictly better than bricking the app.
/// The recovery is logged because it means a panic happened somewhere that must be
/// fixed at the source.
pub fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            log::error!(
                "Recovered a poisoned lock - a previous holder panicked. \
                 State may be stale; the panic itself needs fixing."
            );
            poisoned.into_inner()
        }
    }
}

pub struct DbState {
    pub db: Arc<Mutex<DbManager>>,
}

impl DbState {
    /// The database handle. Never poisons the app - see [`lock_or_recover`].
    pub fn lock_db(&self) -> MutexGuard<'_, DbManager> {
        lock_or_recover(&self.db)
    }
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

impl SettingsState {
    /// The live settings. Never poisons the app - see [`lock_or_recover`].
    pub fn lock_settings(&self) -> MutexGuard<'_, Settings> {
        lock_or_recover(&self.settings)
    }
}
