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

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::State;
use std::time::{SystemTime, UNIX_EPOCH};
use rusqlite::params;
use crate::models::Context;
use crate::utils::get_app_dir;
use crate::state::DbState;
use crate::db::WriteOrigin;
use crate::settings::{get_settings_path, persist_settings_to_disk};

pub fn get_contexts_path() -> PathBuf {
    get_app_dir().join("contexts.json")
}

/// Loads contexts from disk.
/// On first run, migrates contexts from settings.json if present.
pub fn load_contexts_from_disk() -> Vec<Context> {
    let contexts_path = get_contexts_path();
    
    // Try to load from contexts.json first
    if contexts_path.exists() {
        if let Ok(file) = fs::File::open(&contexts_path) {
            if let Ok(contexts) = serde_json::from_reader(file) {
                return contexts;
            }
        }
    }
    
    // contexts.json doesn't exist or is invalid - try to migrate from settings.json
    let settings_path = get_settings_path();
    if settings_path.exists() {
        if let Ok(file) = fs::File::open(&settings_path) {
            // Parse settings as a raw JSON value to extract contexts
            if let Ok(value) = serde_json::from_reader::<_, serde_json::Value>(file) {
                if let Some(contexts_value) = value.get("contexts") {
                    if let Ok(contexts) = serde_json::from_value::<Vec<Context>>(contexts_value.clone()) {
                        if !contexts.is_empty() {
                            println!("Migrating {} contexts from settings.json to contexts.json", contexts.len());
                            // Persist to new location
                            persist_contexts_to_disk(&contexts);
                            // Remove contexts from settings.json
                            remove_contexts_from_settings();
                            return contexts;
                        }
                    }
                }
            }
        }
    }
    
    Vec::new() // Default empty
}

pub fn persist_contexts_to_disk(contexts: &Vec<Context>) {
    let path = get_contexts_path();
    if let Ok(file) = fs::File::create(path) {
        let _ = serde_json::to_writer_pretty(file, contexts);
    }
}

/// Removes the 'contexts' field from settings.json after migration.
/// This keeps settings.json clean and prevents duplicate data.
pub fn remove_contexts_from_settings() {
    let path = get_settings_path();
    if let Ok(file) = fs::File::open(&path) {
        if let Ok(mut value) = serde_json::from_reader::<_, serde_json::Value>(file) {
            if let Some(obj) = value.as_object_mut() {
                if obj.remove("contexts").is_some() {
                    if let Ok(file) = fs::File::create(&path) {
                        let _ = serde_json::to_writer_pretty(file, &value);
                        println!("Removed 'contexts' from settings.json after migration");
                    }
                }
            }
        }
    }
}

#[tauri::command]
pub fn get_contexts(state: State<Arc<DbState>>) -> Vec<Context> {
    match state.db.lock().unwrap().get_contexts() {
        Ok(contexts) => contexts,
        Err(e) => {
            println!("Failed to get contexts: {}", e);
            vec![]
        }
    }
}

#[tauri::command]
pub fn save_contexts(state: State<Arc<DbState>>, contexts: Vec<Context>) {
    println!("Saving {} contexts", contexts.len());
    let mut db = state.db.lock().unwrap();
    let tx_result = db.conn.transaction().and_then(|tx| {
        for ctx in &contexts {
            let rules_json = serde_json::to_string(&ctx.rules).unwrap_or_default();
            tx.execute(
                "INSERT OR REPLACE INTO contexts (id, name, rules, last_used, updated_at, deleted, description) VALUES (?1, ?2, ?3, ?4, ?5, ?7, ?6)",
                params![
                    ctx.id,
                    ctx.name,
                    rules_json,
                    ctx.last_used,
                    // Local edit: always stamp now, never echo the value the UI just
                    // read back, or the server's last-write-wins check will reject it.
                    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
                    ctx.description,
                    ctx.deleted as i32
                ],
            )?;
        }
        tx.commit()
    });

    if let Err(e) = tx_result {
        println!("Failed to save contexts: {}", e);
    }
}

#[tauri::command]
pub fn save_context(state: State<Arc<DbState>>, context: Context) {
    println!("Saving context: {} ({})", context.name, context.id);
    if let Err(e) = state
        .db
        .lock()
        .unwrap()
        .save_context(&context, WriteOrigin::LocalEdit)
    {
        println!("Failed to save context: {}", e);
    }
}

/// Apply contexts pulled from the cloud.
///
/// Separate from `save_context` because the server's `updated_at` must survive intact:
/// stamping it with the local clock here would make every pulled record look locally
/// edited and push it straight back on the next sync.
#[tauri::command]
pub fn import_contexts(state: State<Arc<DbState>>, contexts: Vec<Context>) -> Result<(), String> {
    state
        .db
        .lock()
        .unwrap()
        .import_contexts(&contexts)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_context(state: State<Arc<DbState>>, id: String) {
    println!("Deleting context: {}", id);
    if let Err(e) = state.db.lock().unwrap().delete_context(&id) {
        println!("Failed to delete context: {}", e);
    }
}
