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

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{Manager, State}; // AppHandle, Emitter removed if not used
use active_win_pos_rs::get_active_window;

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
    stashes.push(stash);
}

#[tauri::command]
fn load_stashes(state: State<Arc<StashState>>) -> Vec<StashItem> {
    state.stashes.lock().unwrap().clone()
}

#[tauri::command]
fn save_asset(name: String, data: Vec<u8>) -> String {
    println!("Saving asset: {} ({} bytes)", name, data.len());
    // In real app, save to ~/.stashpad/cache/<id>/
    // Return absolute path
    format!("C:\\Users\\nicow\\.stashpad\\cache\\{}", name)
}

#[tauri::command]
fn copy_to_clipboard(text: String) {
    println!("Copying to clipboard: {}", text);
    // TODO: Use clipboard crate
}

#[tauri::command]
fn start_drag(text: String, files: Vec<String>) {
    println!("Starting drag: {} with files {:?}", text, files);
    // TODO: Trigger OS drag
}

// Basic list of terminal/CLI applications
const CLI_APPS: &[&str] = &[
    "alacritty", "iterm2", "terminal", "powershell", "cmd", "wsl", "bash", "zsh", "fish", "windowsterminal", "conhost"
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
    let tracker_state = Arc::new(Mutex::new(TrackerState::new()));
    let stash_state = Arc::new(StashState { stashes: Mutex::new(Vec::new()) });
    let tracker_state_clone = tracker_state.clone();

    // Start background polling
    thread::spawn(move || {
        loop {
            if let Ok(window) = get_active_window() {
                let app_name = window.app_name; 
                let title = window.title;

                // Adjust these names based on actual process names
                if app_name != "stashpad" && app_name != "Stashpad" && app_name != "app" {
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
            copy_to_clipboard,
            start_drag
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
