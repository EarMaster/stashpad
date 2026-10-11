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

//! Files the user handed over: dropped on the window, or picked in a file dialog.
//!
//! The commands that read or write an outside path used to take whatever path the webview
//! sent, and the webview could also read Downloads, Desktop and Documents itself through the
//! fs plugin. A script that got into the page - through a rendered stash, say - could
//! therefore copy any file the user can read into an attachment, and from there into sync.
//!
//! Now only the backend learns these paths: from the window's drop event, and from the file
//! dialogs, which run here rather than in the webview. Every command that touches an outside
//! path asks [`check`] first, so a path the user never handed over is refused however it
//! reaches the backend.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::Deserialize;
use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, FilePath};

use crate::uierror::UiError;

/// More than any session hands over; past it the oldest choices simply stop counting.
const MAX_REMEMBERED: usize = 4096;

static HANDED_OVER: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();

fn handed_over() -> &'static Mutex<HashSet<PathBuf>> {
    HANDED_OVER.get_or_init(|| Mutex::new(HashSet::new()))
}

/// The one spelling of a path that is compared: canonical where the file exists, and for a
/// save target that does not exist yet, its canonical folder plus the file name.
fn normalize(path: &Path) -> Option<PathBuf> {
    if let Ok(canonical) = path.canonicalize() {
        return Some(canonical);
    }
    let name = path.file_name()?;
    let parent = path.parent()?.canonicalize().ok()?;
    Some(parent.join(name))
}

/// Remember paths the user handed over.
pub fn record<I, P>(paths: I)
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    let mut set = crate::state::lock_or_recover(handed_over());
    if set.len() > MAX_REMEMBERED {
        set.clear();
    }
    for path in paths {
        if let Some(normalized) = normalize(path.as_ref()) {
            set.insert(normalized);
        }
    }
}

/// Whether the user handed this path over.
pub fn is_handed_over(path: &Path) -> bool {
    match normalize(path) {
        Some(normalized) => crate::state::lock_or_recover(handed_over()).contains(&normalized),
        None => false,
    }
}

/// The path, if the user handed it over; otherwise the refusal every caller returns.
pub fn check(path: &str) -> Result<PathBuf, UiError> {
    let path = PathBuf::from(path);
    if is_handed_over(&path) {
        Ok(path)
    } else {
        log::warn!(
            "Refused a path that was not dropped or picked: {}",
            path.display()
        );
        Err("Only files you dropped or picked can be used here".into())
    }
}

/// A file type a dialog offers, as the frontend describes it.
#[derive(Deserialize)]
pub struct DialogFilter {
    pub name: String,
    pub extensions: Vec<String>,
}

fn into_paths(picked: Vec<FilePath>) -> Vec<PathBuf> {
    picked
        .into_iter()
        .filter_map(|p| p.into_path().ok())
        .collect()
}

fn to_strings(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

/// Ask the user for files to open. What they pick may then be read; nothing else may.
#[tauri::command]
pub async fn pick_files(
    app: AppHandle,
    title: String,
    filters: Option<Vec<DialogFilter>>,
    multiple: bool,
) -> Result<Vec<String>, UiError> {
    let mut dialog = app.dialog().file().set_title(title);
    for filter in filters.unwrap_or_default() {
        let extensions: Vec<&str> = filter.extensions.iter().map(String::as_str).collect();
        dialog = dialog.add_filter(filter.name, &extensions);
    }
    // The dialog blocks until the user answers, so it waits on the blocking pool.
    let picked = tauri::async_runtime::spawn_blocking(move || {
        if multiple {
            dialog.blocking_pick_files().unwrap_or_default()
        } else {
            dialog.blocking_pick_file().into_iter().collect()
        }
    })
    .await
    .map_err(|e| format!("File dialog failed: {}", e))?;

    let paths = into_paths(picked);
    record(&paths);
    Ok(to_strings(&paths))
}

/// Ask the user where to save. Only the place they choose may then be written.
#[tauri::command]
pub async fn pick_save_path(
    app: AppHandle,
    title: String,
    default_name: String,
    filters: Option<Vec<DialogFilter>>,
) -> Result<Option<String>, UiError> {
    let mut dialog = app
        .dialog()
        .file()
        .set_title(title)
        .set_file_name(default_name);
    for filter in filters.unwrap_or_default() {
        let extensions: Vec<&str> = filter.extensions.iter().map(String::as_str).collect();
        dialog = dialog.add_filter(filter.name, &extensions);
    }
    let picked = tauri::async_runtime::spawn_blocking(move || dialog.blocking_save_file())
        .await
        .map_err(|e| format!("File dialog failed: {}", e))?;

    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    record([&path]);
    Ok(Some(path.to_string_lossy().into_owned()))
}

/// The bytes of a file the user handed over - for resizing a dropped image before it is
/// saved.
#[tauri::command]
pub async fn read_handed_over_file(path: String) -> Result<tauri::ipc::Response, UiError> {
    let path = check(&path)?;
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// The size of a file the user handed over or one in the attachment cache; `None` for any
/// other path, or one that is gone.
#[tauri::command]
pub async fn file_size(path: String) -> Option<u64> {
    let path = PathBuf::from(path);
    if !is_handed_over(&path) && !crate::utils::is_inside_cache(&path) {
        return None;
    }
    tokio::fs::metadata(&path).await.ok().map(|m| m.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_recorded_paths_pass() {
        let dir = tempfile::tempdir().unwrap();
        let picked = dir.path().join("picked.txt");
        let other = dir.path().join("other.txt");
        std::fs::write(&picked, b"x").unwrap();
        std::fs::write(&other, b"x").unwrap();

        record([&picked]);
        assert!(check(&picked.to_string_lossy()).is_ok());
        assert!(check(&other.to_string_lossy()).is_err());
    }

    #[test]
    fn another_spelling_of_a_recorded_path_still_matches() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        let file = dir.path().join("dropped.png");
        std::fs::write(&file, b"x").unwrap();

        record([&file]);
        let detour = dir.path().join("sub").join("..").join("dropped.png");
        assert!(is_handed_over(&detour));
    }

    #[test]
    fn a_save_target_counts_before_it_exists() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("export.zip");

        record([&target]);
        assert!(is_handed_over(&target));
        assert!(!is_handed_over(&dir.path().join("elsewhere.zip")));
    }
}
