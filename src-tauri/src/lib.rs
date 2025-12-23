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

use active_win_pos_rs::get_active_window;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::State;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct StashItem {
    id: String,
    content: String,
    files: Vec<String>,
    #[serde(rename = "createdAt")]
    created_at: String,
}

#[derive(serde::Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppContext {
    window_title: String,
    process_name: String,
}

struct TrackerState {
    last_external_app: Option<AppContext>,
}

struct StashState {
    stashes: Mutex<Vec<StashItem>>,
}

impl TrackerState {
    fn new() -> Self {
        Self {
            last_external_app: None,
        }
    }
}

// --- Persistence Helpers ---

fn get_app_dir() -> PathBuf {
    dirs::home_dir()
        .expect("Could not resolve home directory")
        .join(".stashpad")
}

fn ensure_storage_ready() {
    let app_dir = get_app_dir();
    let cache_dir = app_dir.join("cache");
    if !app_dir.exists() {
        fs::create_dir_all(&app_dir).expect("Failed to create app dir");
    }
    if !cache_dir.exists() {
        fs::create_dir_all(&cache_dir).expect("Failed to create cache dir");
    }
}

fn persist_stashes_to_disk(stashes: &Vec<StashItem>) {
    let db_path = get_app_dir().join("db.json");
    if let Ok(file) = fs::File::create(db_path) {
        let _ = serde_json::to_writer_pretty(file, stashes);
    }
}

fn load_stashes_from_disk() -> Vec<StashItem> {
    let db_path = get_app_dir().join("db.json");
    if db_path.exists() {
        if let Ok(file) = fs::File::open(db_path) {
            if let Ok(stashes) = serde_json::from_reader(file) {
                return stashes;
            }
        }
    }
    Vec::new() // Default empty
}

// --- Commands ---

#[tauri::command]
fn get_previous_app_info(state: State<Arc<Mutex<TrackerState>>>) -> AppContext {
    let state = state.lock().unwrap();
    state.last_external_app.clone().unwrap_or(AppContext {
        window_title: "Unknown".into(),
        process_name: "Unknown".into(),
    })
}

#[tauri::command]
fn save_stash(state: State<Arc<StashState>>, stash: StashItem) {
    println!("Saving stash: {:?}", stash);
    let mut stashes = state.stashes.lock().unwrap();

    // Upsert logic: if id exists, replace; else push
    if let Some(index) = stashes.iter().position(|s| s.id == stash.id) {
        stashes[index] = stash;
    } else {
        stashes.push(stash);
    }

    persist_stashes_to_disk(&stashes);
}

#[tauri::command]
fn load_stashes(state: State<Arc<StashState>>) -> Vec<StashItem> {
    state.stashes.lock().unwrap().clone()
}

#[tauri::command]
fn save_asset(name: String, data: Vec<u8>) -> Result<String, String> {
    println!("Saving asset: {} ({} bytes)", name, data.len());

    let cache_dir = get_app_dir().join("cache");
    // Basic sanitization
    let safe_name = std::path::Path::new(&name)
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("unknown_file"))
        .to_string_lossy();

    let file_path = cache_dir.join(safe_name.as_ref());

    match fs::write(&file_path, data) {
        Ok(_) => Ok(file_path.to_string_lossy().into_owned()),
        Err(e) => Err(format!("Failed to write file: {}", e)),
    }
}

#[tauri::command]
fn save_asset_from_path(path: String) -> Result<String, String> {
    println!("Importing asset from path: {}", path);
    let path = std::path::Path::new(&path);
    if !path.exists() {
        return Err("File does not exist".into());
    }

    let cache_dir = get_app_dir().join("cache");
    let file_name = path
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("unknown_file"))
        .to_string_lossy();
    
    // Create unique name if collision
    let dest_path = cache_dir.join(file_name.as_ref());

    match fs::copy(path, &dest_path) {
        Ok(_) => Ok(dest_path.to_string_lossy().into_owned()),
        Err(e) => Err(format!("Failed to copy file: {}", e)),
    }
}

#[tauri::command]
fn copy_to_clipboard(text: String) -> Result<(), String> {
    println!("Copying to clipboard: {}", text);
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn start_drag(window: tauri::Window, text: String, files: Vec<String>) -> Result<(), String> {
    println!("Starting drag: {} with files {:?}", text, files);

    let items = if !files.is_empty() {
        use std::path::PathBuf;
        let paths: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
        drag::DragItem::Files(paths)
    } else {
        return Err("No files to drag".into());
    };

    let image = drag::Image::Raw(vec![]);

    drag::start_drag(&window, items, image, |_, _| {}, Default::default())
        .map_err(|e| e.to_string())?;

    Ok(())
}

// Basic list of terminal/CLI applications
const CLI_APPS: &[&str] = &[
    "alacritty",
    "iterm2",
    "terminal",
    "powershell",
    "cmd",
    "wsl",
    "bash",
    "zsh",
    "fish",
    "windowsterminal",
    "conhost",
    "warp",
    "hyper",
];

#[tauri::command]
fn get_smart_transfer_target(state: State<Arc<Mutex<TrackerState>>>) -> String {
    let state = state.lock().unwrap();
    if let Some(app) = &state.last_external_app {
        let lower = app.process_name.to_lowercase();
        // aggressive matching
        for cli in CLI_APPS {
            if lower.contains(cli) {
                return "CLI".into();
            }
        }
    }
    "GUI".into()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 1. Initialize Storage
    ensure_storage_ready();

    // 2. Load Stashes
    let initial_stashes = load_stashes_from_disk();

    let tracker_state = Arc::new(Mutex::new(TrackerState::new()));
    let stash_state = Arc::new(StashState {
        stashes: Mutex::new(initial_stashes),
    });
    let tracker_state_clone = tracker_state.clone();

    // Start background polling
    thread::spawn(move || {
        loop {
            if let Ok(window) = get_active_window() {
                let app_name = window.app_name;
                let title = window.title;

                // Adjust these names based on actual process names
                if app_name != "stashpad"
                    && app_name != "Stashpad"
                    && app_name != "app"
                    && app_name != "electron.exe"
                {
                    let mut state = tracker_state_clone.lock().unwrap();
                    state.last_external_app = Some(AppContext {
                        window_title: title,
                        process_name: app_name,
                    });
                }
            }
            thread::sleep(Duration::from_millis(500));
        }
    });

    tauri::Builder::default()
        .manage(tracker_state)
        .manage(stash_state)
        .plugin(tauri_plugin_log::Builder::default().build())
        .invoke_handler(tauri::generate_handler![
            get_previous_app_info,
            get_smart_transfer_target,
            save_stash,
            load_stashes,
            save_asset,
            save_asset_from_path,
            copy_to_clipboard,
            start_drag
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
