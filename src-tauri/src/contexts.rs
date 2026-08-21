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

use std::sync::Arc;
use tauri::State;
use std::time::{SystemTime, UNIX_EPOCH};
use rusqlite::params;
use crate::models::Context;
use crate::state::DbState;
use crate::db::WriteOrigin;

#[tauri::command]
pub async fn get_contexts(state: State<'_, Arc<DbState>>) -> Result<Vec<Context>, String> {
    // A read failure still resolves to an empty list rather than rejecting: the callers
    // treat "no contexts" as a valid state, and surfacing an error here would break the
    // startup path. The Result is required because async commands that borrow State
    // must return one.
    Ok(match state.lock_db().get_contexts() {
        Ok(contexts) => contexts,
        Err(e) => {
            println!("Failed to get contexts: {}", e);
            vec![]
        }
    })
}

#[tauri::command]
pub async fn save_contexts(state: State<'_, Arc<DbState>>, contexts: Vec<Context>) -> Result<(), String> {
    println!("Saving {} contexts", contexts.len());
    let mut db = state.lock_db();
    let tx_result = db.conn.transaction().and_then(|tx| {
        for ctx in &contexts {
            let rules_json = serde_json::to_string(&ctx.rules).unwrap_or_default();
            tx.execute(
                // pending_sync = 1: a local edit still has to reach the server.
                "INSERT OR REPLACE INTO contexts (id, name, rules, last_used, updated_at, deleted, description, pending_sync) VALUES (?1, ?2, ?3, ?4, ?5, ?7, ?6, 1)",
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
    Ok(())
}

#[tauri::command]
pub async fn save_context(state: State<'_, Arc<DbState>>, context: Context) -> Result<(), String> {
    println!("Saving context: {} ({})", context.name, context.id);
    if let Err(e) = state
        .lock_db()
        .save_context(&context, WriteOrigin::LocalEdit)
    {
        println!("Failed to save context: {}", e);
    }
    Ok(())
}

/// Apply contexts pulled from the cloud.
///
/// Separate from `save_context` because the server's `updated_at` must survive intact:
/// stamping it with the local clock here would make every pulled record look locally
/// edited and push it straight back on the next sync.
#[tauri::command]
pub async fn import_contexts(state: State<'_, Arc<DbState>>, contexts: Vec<Context>) -> Result<(), String> {
    state
        .lock_db()
        .import_contexts(&contexts)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_context(state: State<'_, Arc<DbState>>, id: String) -> Result<(), String> {
    println!("Deleting context: {}", id);
    if let Err(e) = state.lock_db().delete_context(&id) {
        println!("Failed to delete context: {}", e);
    }
    Ok(())
}

/// Contexts with local changes the server has not acknowledged yet.
#[tauri::command]
pub async fn claim_pending_contexts(state: State<'_, Arc<DbState>>) -> Result<Vec<Context>, String> {
    Ok(state.lock_db().claim_pending_contexts().unwrap_or_default())
}

/// Clear the pending flag for contexts the server accepted.
#[tauri::command]
pub async fn mark_contexts_synced(
    state: State<'_, Arc<DbState>>,
    ids: Vec<String>,
) -> Result<(), String> {
    state
        .lock_db()
        .mark_synced("contexts", &ids)
        .map_err(|e| e.to_string())
}
