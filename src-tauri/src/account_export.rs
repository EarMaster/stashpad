// SPDX-License-Identifier: AGPL-3.0-only

// Copyright (C) 2026 Nico Wiedemann

// This file is part of Stashpad.
// Stashpad is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License, version 3,
// as published by the Free Software Foundation.
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Affero General Public License for more details.

//! Reading a server export back, and writing a whole account out.
//!
//! The account page hands out a complete copy of what the server holds. For an encrypted
//! account that copy is sealed, and the browser can open the records but not the files -
//! decrypting a two-gigabyte account in a tab is not something to attempt. This is the
//! route that can do all of it, because this installation already holds the key.
//!
//! It also closes a gap that predates encryption: there has never been a way to put an
//! export back. `commit_import` exists and reads Markdown, so what was missing is the step
//! from `export.json` to the records it already knows how to commit.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::e2ee_session;
use crate::models::{Context, StashItem};
use crate::state::DbState;

/// What an import would do, so the user can see it before it happens.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPlan {
    pub contexts: usize,
    pub stashes: usize,
    /// Records the file holds that this installation cannot read.
    ///
    /// Non-zero means the export belongs to a different account, or was taken under a key
    /// this installation does not have. Surfaced rather than skipped quietly, because
    /// importing half an archive and saying nothing is worse than refusing.
    pub unreadable: usize,
    /// Deleted records, which an export includes and which carry no text once an account
    /// is encrypted. Counted separately so an empty stash is not mistaken for a failure.
    pub deleted: usize,
}

#[derive(Debug, Deserialize)]
struct ServerExport {
    #[serde(default)]
    profile: ExportProfile,
    #[serde(default)]
    contexts: Vec<serde_json::Value>,
    #[serde(default)]
    stashes: Vec<serde_json::Value>,
}

#[derive(Debug, Default, Deserialize)]
struct ExportProfile {
    #[serde(default)]
    id: String,
}

/// Read an `export.json`, decrypting anything sealed, and report what it holds.
///
/// Separate from committing it so the user sees the shape of the thing before it lands in
/// their queue. An import that silently merged a stranger's archive would be very hard to
/// undo by hand.
#[tauri::command]
pub async fn read_account_export(path: String) -> Result<ImportPlan, String> {
    let (contexts, stashes, unreadable, deleted) = parse_export(&path).await?;
    Ok(ImportPlan {
        contexts: contexts.len(),
        stashes: stashes.len(),
        unreadable,
        deleted,
    })
}

/// Commit a previously read export into the local database.
///
/// Records land as local edits, so they are pushed back to the server on the next sync -
/// which is what makes this a restore rather than a read-only view.
#[tauri::command]
pub async fn import_account_export(
    state: State<'_, Arc<DbState>>,
    path: String,
) -> Result<ImportPlan, String> {
    let (contexts, stashes, unreadable, deleted) = parse_export(&path).await?;

    let plan = ImportPlan {
        contexts: contexts.len(),
        stashes: stashes.len(),
        unreadable,
        deleted,
    };

    {
        let mut db = state.lock_db();
        for context in &contexts {
            // A local edit, not a sync import: these are records this installation is
            // putting back, and they have to reach the server again.
            db.save_context(context, crate::db::WriteOrigin::LocalEdit)
                .map_err(|e| format!("Could not save the contexts: {}", e))?;
        }
        if !stashes.is_empty() {
            db.insert_local_stashes(&stashes)
                .map_err(|e| format!("Could not save the stashes: {}", e))?;
        }
    }

    Ok(plan)
}

/// Parse and decrypt, without touching the database.
async fn parse_export(
    path: &str,
) -> Result<(Vec<Context>, Vec<StashItem>, usize, usize), String> {
    let text = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| format!("Could not read {}: {}", path, e))?;

    let export: ServerExport =
        serde_json::from_str(&text).map_err(|e| format!("This is not a Stashpad export: {}", e))?;

    let user_id = export.profile.id;
    let mut unreadable = 0usize;
    let mut deleted = 0usize;

    let mut contexts = Vec::new();
    for raw in &export.contexts {
        if raw["deleted_at"].as_str().is_some() {
            deleted += 1;
            continue;
        }
        let id = raw["id"].as_str().unwrap_or_default().to_string();
        let name = match open(raw["name"].as_str(), &user_id, "context", &id, "name") {
            Ok(value) => value,
            Err(_) => {
                unreadable += 1;
                continue;
            }
        };
        let description = open(raw["description"].as_str(), &user_id, "context", &id, "description")
            .ok()
            .filter(|d| !d.is_empty());

        contexts.push(Context {
            id,
            name,
            description,
            rules: Vec::new(),
            last_used: None,
            updated_at: None,
            deleted: false,
        });
    }

    let mut stashes = Vec::new();
    for raw in &export.stashes {
        if raw["deleted_at"].as_str().is_some() {
            deleted += 1;
            continue;
        }
        let id = raw["id"].as_str().unwrap_or_default().to_string();
        let content = match open(raw["content"].as_str(), &user_id, "stash", &id, "content") {
            Ok(value) => value,
            Err(_) => {
                unreadable += 1;
                continue;
            }
        };

        let enhanced_content = open(
            raw["enhanced_content"].as_str(),
            &user_id,
            "stash",
            &id,
            "enhanced_content",
        )
        .ok()
        .filter(|c| !c.is_empty());

        stashes.push(StashItem {
            id,
            context_id: raw["context_id"].as_str().map(str::to_string),
            content,
            files: Vec::new(),
            created_at: raw["created_at"].as_str().unwrap_or_default().to_string(),
            completed: raw["completed"].as_i64().unwrap_or(0) != 0,
            completed_at: raw["completed_at"].as_str().map(str::to_string),
            // Position is deliberately not carried over. It is the user's manual order on
            // the devices they still have, and an import is a restore into whatever order
            // those already hold - not an instruction to rearrange them.
            updated_at: None,
            enhanced_content,
            deleted: false,
            attachments: Vec::new(),
        });
    }

    Ok((contexts, stashes, unreadable, deleted))
}

/// Open a field if it is sealed, pass it through if it is not.
fn open(
    value: Option<&str>,
    user_id: &str,
    kind: &str,
    record_id: &str,
    field: &str,
) -> Result<String, String> {
    let Some(value) = value else {
        return Ok(String::new());
    };
    if !crate::envelope::is_envelope(value) {
        return Ok(value.to_string());
    }

    let binding = match (kind, field) {
        ("stash", "content") => (crate::envelope::Kind::Stash, crate::envelope::Field::Content),
        ("stash", "enhanced_content") => (
            crate::envelope::Kind::Stash,
            crate::envelope::Field::EnhancedContent,
        ),
        ("context", "name") => (crate::envelope::Kind::Context, crate::envelope::Field::Name),
        ("context", "description") => (
            crate::envelope::Kind::Context,
            crate::envelope::Field::Description,
        ),
        _ => return Err(format!("unknown field {}/{}", kind, field)),
    };

    let key = e2ee_session::content_key_bytes()
        .ok_or("This installation does not hold the key for this export")?;

    crate::envelope::open(
        &key,
        e2ee_session::epoch(),
        value,
        &crate::envelope::Binding {
            user_id,
            kind: binding.0,
            record_id,
            field: binding.1,
            epoch: e2ee_session::epoch(),
        },
    )
    .map_err(|e| format!("{:?}", e))
}

/// Reduce a context name to something that can be a file name.
///
/// A context name is user-authored and becomes a path here, so anything that could climb
/// out of the destination directory has to go. Falls back to the id rather than producing
/// an empty name, which would collide for every context that sanitises to nothing.
fn safe_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').to_string();
    if trimmed.is_empty() {
        "context".to_string()
    } else {
        trimmed.chars().take(80).collect()
    }
}

/// Counts for a whole-account Markdown export, so the caller can report what it wrote.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountExportSummary {
    pub contexts: usize,
    pub stashes: usize,
}

/// Write every context out as Markdown, one file per context, into a directory.
///
/// Reuses the per-context writer rather than inventing a second format, so what comes out
/// is the same thing the existing exporter produces and the existing importer reads.
#[tauri::command]
pub async fn export_whole_account(
    state: State<'_, Arc<DbState>>,
    dest_dir: String,
) -> Result<AccountExportSummary, String> {
    let (contexts, by_context) = {
        let db = state.lock_db();
        let contexts = db.get_contexts().map_err(|e| e.to_string())?;
        let stashes = db.get_stashes().map_err(|e| e.to_string())?;

        let mut by_context: HashMap<String, Vec<String>> = HashMap::new();
        for stash in stashes {
            let owner = stash
                .context_id
                .clone()
                .unwrap_or_else(|| "default".to_string());
            by_context.entry(owner).or_default().push(stash.id);
        }
        (contexts, by_context)
    };

    tokio::fs::create_dir_all(&dest_dir)
        .await
        .map_err(|e| format!("Could not create {}: {}", dest_dir, e))?;

    let mut written = 0usize;
    let mut total = 0usize;

    for context in &contexts {
        let ids = by_context.get(&context.id).cloned().unwrap_or_default();
        if ids.is_empty() {
            continue;
        }
        total += ids.len();

        // One archive per context, named after it. `safe_name` because a context name is
        // user-authored and this becomes a path.
        let file = std::path::Path::new(&dest_dir)
            .join(format!("{}.md", safe_file_name(&context.name)));

        crate::transfer::export_context_archive(
            state.clone(),
            context.id.clone(),
            ids,
            false,
            file.to_string_lossy().to_string(),
        )
        .await?;
        written += 1;
    }

    Ok(AccountExportSummary {
        contexts: written,
        stashes: total,
    })
}
