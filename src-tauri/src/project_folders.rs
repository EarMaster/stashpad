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

//! The project folder a context is linked to on this device, and the file list behind the
//! editor's `@path` picker.
//!
//! A folder path only means something on the machine it was picked on, so the link lives in
//! its own local table (see `context_folders` in db.rs) and never reaches sync.
//!
//! The webview never names a folder. It is picked in a dialog that runs here, the same rule
//! handover.rs applies to files, and what goes back to the page is the folder's path for
//! display and the *names* of the files inside it - never their contents.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::db::DbManager;
use crate::state::DbState;
use crate::uierror::UiError;

/// More files than this and the picker stops listing. A repository that large is mostly
/// generated or vendored files nobody `@`-mentions, and walking it on every refresh would
/// stall the editor.
const MAX_FILES: usize = 20_000;

/// A context's folder link, as the UI shows it.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextFolder {
    pub context_id: String,
    pub path: String,
    /// False when the folder is gone - an unmounted drive, a renamed parent. The link is
    /// kept so the user can relink it rather than silently losing it.
    pub exists: bool,
}

/// The files of a linked folder, relative to it, with `/` separators on every platform.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileIndex {
    pub files: Vec<String>,
    /// The folder held more than [`MAX_FILES`] files and the list stops there.
    pub truncated: bool,
}

fn folder(context_id: String, path: String) -> ContextFolder {
    let exists = Path::new(&path).is_dir();
    ContextFolder {
        context_id,
        path,
        exists,
    }
}

// ---------------------------------------------------------------------------
// The file index
// ---------------------------------------------------------------------------

/// Built indexes, by context, with the folder each was built from so a relink invalidates it.
type IndexCache = HashMap<String, (PathBuf, Arc<FileIndex>)>;

static INDEXES: OnceLock<Mutex<IndexCache>> = OnceLock::new();

fn indexes() -> &'static Mutex<IndexCache> {
    INDEXES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn forget_index(context_id: &str) {
    crate::state::lock_or_recover(indexes()).remove(context_id);
}

/// List the files under `root` the way the picker offers them.
///
/// Honours `.gitignore` and friends whether or not the folder is a git checkout, and keeps
/// dotfiles such as `.github/` - only `.git` itself is skipped. A path containing `"` is left
/// out: the editor quotes paths with spaces, and there is no clean way to quote a quote.
pub fn index_folder(root: &Path, max_files: usize) -> FileIndex {
    let walker = ignore::WalkBuilder::new(root)
        .hidden(false)
        .require_git(false)
        .follow_links(false)
        .filter_entry(|entry| entry.file_name() != ".git")
        .build();

    let mut files = Vec::new();
    let mut truncated = false;
    for entry in walker.flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let Some(relative) = relative_path(root, entry.path()) else {
            continue;
        };
        if files.len() == max_files {
            truncated = true;
            break;
        }
        files.push(relative);
    }
    files.sort();
    FileIndex { files, truncated }
}

/// `path` relative to `root` with `/` separators, or `None` for a path the picker skips.
fn relative_path(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let mut parts = Vec::new();
    for component in relative.components() {
        parts.push(component.as_os_str().to_str()?);
    }
    let joined = parts.join("/");
    if joined.is_empty() || joined.contains('"') {
        return None;
    }
    Some(joined)
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

/// Folders read from import archives, by import token, until the import is committed or
/// discarded. Kept here rather than handed to the webview and back, so the page cannot
/// substitute a folder of its own choosing on the way.
static PENDING_IMPORTS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

fn pending_imports() -> &'static Mutex<HashMap<String, String>> {
    PENDING_IMPORTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Note the folder an archive names, if it exists on this device. A folder that does not
/// exist is treated as no folder at all. Returns what the import will link.
pub fn remember_import_folder(token: &str, folder: Option<String>) -> Option<String> {
    let folder = folder.filter(|path| Path::new(path).is_dir())?;
    crate::state::lock_or_recover(pending_imports()).insert(token.to_string(), folder.clone());
    Some(folder)
}

pub fn take_import_folder(token: &str) -> Option<String> {
    crate::state::lock_or_recover(pending_imports()).remove(token)
}

/// Link an imported context to the folder its archive named.
///
/// A link the user already made on this device wins over the archive's: it was chosen here,
/// for this machine, and an archive may have been written somewhere else entirely.
pub fn apply_imported_folder(
    db: &DbManager,
    context_id: &str,
    folder: &str,
) -> rusqlite::Result<bool> {
    if !Path::new(folder).is_dir() || db.get_context_folder(context_id)?.is_some() {
        return Ok(false);
    }
    db.set_context_folder(context_id, folder)?;
    forget_index(context_id);
    Ok(true)
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_context_folders(
    state: State<'_, Arc<DbState>>,
) -> Result<Vec<ContextFolder>, UiError> {
    let links = state
        .lock_db()
        .get_context_folders()
        .map_err(|e| e.to_string())?;
    Ok(links
        .into_iter()
        .map(|(context_id, path)| folder(context_id, path))
        .collect())
}

/// Ask the user for the context's project folder and link it. `None` when they cancel.
#[tauri::command]
pub async fn pick_context_folder(
    app: AppHandle,
    state: State<'_, Arc<DbState>>,
    context_id: String,
    title: String,
) -> Result<Option<ContextFolder>, UiError> {
    let dialog = app.dialog().file().set_title(title);
    // The dialog blocks until the user answers, so it waits on the blocking pool.
    let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_folder())
        .await
        .map_err(|e| format!("Folder dialog failed: {}", e))?;

    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    let path = path.to_string_lossy().into_owned();
    state
        .lock_db()
        .set_context_folder(&context_id, &path)
        .map_err(|e| e.to_string())?;
    forget_index(&context_id);
    log::info!("Linked a project folder to context {}", context_id);
    Ok(Some(folder(context_id, path)))
}

#[tauri::command]
pub async fn unlink_context_folder(
    state: State<'_, Arc<DbState>>,
    context_id: String,
) -> Result<(), UiError> {
    state
        .lock_db()
        .remove_context_folder(&context_id)
        .map_err(|e| e.to_string())?;
    forget_index(&context_id);
    log::info!("Unlinked the project folder of context {}", context_id);
    Ok(())
}

/// Drop every folder link on this device - the sign-out choice not to keep them.
#[tauri::command]
pub async fn clear_context_folders(state: State<'_, Arc<DbState>>) -> Result<(), UiError> {
    state
        .lock_db()
        .clear_context_folders()
        .map_err(|e| e.to_string())?;
    crate::state::lock_or_recover(indexes()).clear();
    log::info!("Cleared every project folder link");
    Ok(())
}

/// The files in the context's linked folder. Empty when no folder is linked or it is gone.
///
/// Built once and then served from memory; `refresh` walks the folder again, which the
/// editor asks for when it opens, so files added since show up.
#[tauri::command]
pub async fn list_context_files(
    state: State<'_, Arc<DbState>>,
    context_id: String,
    refresh: bool,
) -> Result<FileIndex, UiError> {
    let Some(root) = state
        .lock_db()
        .get_context_folder(&context_id)
        .map_err(|e| e.to_string())?
        .map(PathBuf::from)
    else {
        return Ok(FileIndex::default());
    };

    if !refresh {
        if let Some((built_from, index)) = crate::state::lock_or_recover(indexes()).get(&context_id)
        {
            if *built_from == root {
                return Ok(index.as_ref().clone());
            }
        }
    }

    if !root.is_dir() {
        forget_index(&context_id);
        return Ok(FileIndex::default());
    }

    let walk_root = root.clone();
    let index = tauri::async_runtime::spawn_blocking(move || index_folder(&walk_root, MAX_FILES))
        .await
        .map_err(|e| format!("Indexing the project folder failed: {}", e))?;
    if index.truncated {
        log::warn!(
            "Project folder of context {} holds more than {} files; the picker lists the first {}",
            context_id,
            MAX_FILES,
            MAX_FILES
        );
    }

    let index = Arc::new(index);
    crate::state::lock_or_recover(indexes()).insert(context_id, (root, index.clone()));
    Ok(index.as_ref().clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(root: &Path, relative: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"x").unwrap();
    }

    #[test]
    fn the_index_lists_relative_paths_with_forward_slashes() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "src/lib/types.ts");
        write(dir.path(), "README.md");

        let index = index_folder(dir.path(), MAX_FILES);
        assert_eq!(index.files, vec!["README.md", "src/lib/types.ts"]);
        assert!(!index.truncated);
    }

    #[test]
    fn the_index_honours_gitignore_without_a_git_checkout() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".gitignore"), "node_modules/\n*.log\n").unwrap();
        write(dir.path(), "node_modules/pkg/index.js");
        write(dir.path(), "debug.log");
        write(dir.path(), "src/app.ts");

        let index = index_folder(dir.path(), MAX_FILES);
        assert_eq!(index.files, vec![".gitignore", "src/app.ts"]);
    }

    #[test]
    fn dotfiles_are_listed_but_the_git_directory_is_not() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), ".github/workflows/test.yml");
        write(dir.path(), ".git/config");

        let index = index_folder(dir.path(), MAX_FILES);
        assert_eq!(index.files, vec![".github/workflows/test.yml"]);
    }

    #[test]
    fn a_path_with_spaces_is_kept_and_one_with_a_quote_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "docs/release notes.md");
        // Windows does not allow `"` in a file name, so this half only runs elsewhere.
        if cfg!(not(windows)) {
            write(dir.path(), "docs/say \"hi\".md");
        }

        let index = index_folder(dir.path(), MAX_FILES);
        assert_eq!(index.files, vec!["docs/release notes.md"]);
    }

    #[test]
    fn the_index_stops_at_the_cap_and_says_so() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..5 {
            write(dir.path(), &format!("f{}.txt", i));
        }

        let index = index_folder(dir.path(), 3);
        assert_eq!(index.files.len(), 3);
        assert!(index.truncated);
    }

    fn test_db() -> DbManager {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let db = DbManager { conn };
        db.init_tables().unwrap();
        db
    }

    #[test]
    fn an_imported_folder_that_exists_is_linked() {
        let db = test_db();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_string_lossy().into_owned();

        assert!(apply_imported_folder(&db, "ctx", &path).unwrap());
        assert_eq!(db.get_context_folder("ctx").unwrap(), Some(path));
    }

    #[test]
    fn an_imported_folder_that_is_missing_is_treated_as_none() {
        let db = test_db();
        let dir = tempfile::tempdir().unwrap();
        let gone = dir.path().join("gone").to_string_lossy().into_owned();

        assert!(!apply_imported_folder(&db, "ctx", &gone).unwrap());
        assert_eq!(db.get_context_folder("ctx").unwrap(), None);
        assert_eq!(remember_import_folder("token-missing", Some(gone)), None);
        assert_eq!(take_import_folder("token-missing"), None);
    }

    #[test]
    fn an_existing_link_wins_over_the_imported_one() {
        let db = test_db();
        let mine = tempfile::tempdir().unwrap();
        let theirs = tempfile::tempdir().unwrap();
        let mine = mine.path().to_string_lossy().into_owned();
        db.set_context_folder("ctx", &mine).unwrap();

        let theirs = theirs.path().to_string_lossy().into_owned();
        assert!(!apply_imported_folder(&db, "ctx", &theirs).unwrap());
        assert_eq!(db.get_context_folder("ctx").unwrap(), Some(mine));
    }

    #[test]
    fn a_remembered_import_folder_is_handed_out_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_string_lossy().into_owned();

        assert_eq!(
            remember_import_folder("token-once", Some(path.clone())),
            Some(path.clone())
        );
        assert_eq!(take_import_folder("token-once"), Some(path));
        assert_eq!(take_import_folder("token-once"), None);
    }
}
