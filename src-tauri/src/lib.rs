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
#[serde(rename_all = "camelCase")]
pub struct StashItem {
    pub id: String,
    pub content: String,
    pub files: Vec<String>,
    pub created_at: String,
    #[serde(default)]
    pub context_id: Option<String>,
    #[serde(default)]
    pub completed: bool,
}

#[derive(serde::Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppContext {
    window_title: String,
    process_name: String,
    detected_context_id: Option<String>,
}

struct TrackerState {
    last_external_app: Option<AppContext>,
    current_context_id: Option<String>,
}

struct StashState {
    stashes: Mutex<Vec<StashItem>>,
}

impl TrackerState {
    fn new() -> Self {
        Self {
            last_external_app: None,
            current_context_id: None,
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

// --- Settings ---

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ContextRule {
    pub rule_type: String, // "process" or "title"
    pub value: String,
    pub match_type: String, // "contains", "exact"
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Context {
    pub id: String,
    pub name: String,
    pub rules: Vec<ContextRule>,
    #[serde(default)]
    pub last_used: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub auto_context_detection: bool,
    #[serde(default)]
    pub contexts: Vec<Context>,
    #[serde(default)]
    pub active_context_id: Option<String>,
    #[serde(default)]
    pub shortcuts: std::collections::HashMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_context_detection: true,
            contexts: vec![],
            active_context_id: None,
            shortcuts: std::collections::HashMap::new(),
        }
    }
}

struct SettingsState {
    settings: Mutex<Settings>,
}

fn get_settings_path() -> PathBuf {
    get_app_dir().join("settings.json")
}

fn load_settings_from_disk() -> Settings {
    let path = get_settings_path();
    if path.exists() {
        if let Ok(file) = fs::File::open(path) {
            if let Ok(settings) = serde_json::from_reader(file) {
                return settings;
            }
        }
    }
    Settings::default()
}

fn persist_settings_to_disk(settings: &Settings) {
    let path = get_settings_path();
    if let Ok(file) = fs::File::create(path) {
        let _ = serde_json::to_writer_pretty(file, settings);
    }
}

// --- Commands ---

#[tauri::command]
fn get_settings(state: State<Arc<SettingsState>>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]

fn save_settings(app: tauri::AppHandle, state: State<Arc<SettingsState>>, settings: Settings) {
    println!("Saving settings: {:?}", settings);
    
    // Update global shortcuts
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    
    let mut current = state.settings.lock().unwrap();
    
    // Simple logic: Unregister known old/register new
    if let Some(old_shortcut) = current.shortcuts.get("global_toggle") {
         if !old_shortcut.is_empty() {
             let _ = app.global_shortcut().unregister(old_shortcut.as_str());
         }
    }

    if let Some(new_shortcut) = settings.shortcuts.get("global_toggle") {
        if !new_shortcut.is_empty() {
            // Note: register expects a str
            if let Err(e) = app.global_shortcut().register(new_shortcut.as_str()) {
                println!("Failed to register shortcut '{}': {}", new_shortcut, e);
            }
        }
    }

    *current = settings.clone();
    persist_settings_to_disk(&settings);
}

#[tauri::command]
fn get_previous_app_info(state: State<Arc<Mutex<TrackerState>>>) -> AppContext {
    let state = state.lock().unwrap();
    if let Some(app) = &state.last_external_app {
        let mut app_ctx = app.clone();
        app_ctx.detected_context_id = state.current_context_id.clone();
        app_ctx
    } else {
        AppContext {
            window_title: "Unknown".into(),
            process_name: "Unknown".into(),
            detected_context_id: None,
        }
    }
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
fn delete_stash(state: State<Arc<StashState>>, id: String) {
    let mut stashes = state.stashes.lock().unwrap();
    stashes.retain(|s| s.id != id);
    persist_stashes_to_disk(&stashes);
}

#[tauri::command]
fn delete_completed_stashes(state: State<Arc<StashState>>) {
    let mut stashes = state.stashes.lock().unwrap();
    stashes.retain(|s| !s.completed);
    persist_stashes_to_disk(&stashes);
}

#[tauri::command]
fn save_stashes(state: State<Arc<StashState>>, stashes_list: Vec<StashItem>) {
    let mut stashes = state.stashes.lock().unwrap();
    *stashes = stashes_list;
    persist_stashes_to_disk(&stashes);
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
        // Create temporary text file for text-only stashes
        use std::fs;
        let cache_dir = get_app_dir().join("cache").join("drags");
        let _ = fs::create_dir_all(&cache_dir);
        
        // Use a hash or sanitized content for filename
        let safe_name = text.chars().take(20).filter(|c| c.is_alphanumeric()).collect::<String>();
        let filename = if safe_name.is_empty() { "stash.txt".to_string() } else { format!("{}.txt", safe_name) };
        let temp_path = cache_dir.join(filename);
        
        if let Err(e) = fs::write(&temp_path, &text) {
             println!("Failed to write temp drag file: {}", e);
             return Err("Failed to create drag data".into());
        }
        
        drag::DragItem::Files(vec![temp_path])
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
    // 0. init devtools
    #[cfg(debug_assertions)] // only enable instrumentation in development builds
    let devtools = tauri_plugin_devtools::init();

    // 1. Initialize Storage
    ensure_storage_ready();

    // 2. Load Stashes
    let initial_stashes = load_stashes_from_disk();

    let tracker_state = Arc::new(Mutex::new(TrackerState::new()));
    let stash_state = Arc::new(StashState {
        stashes: Mutex::new(initial_stashes),
    });
    let settings_state = Arc::new(SettingsState {
        settings: Mutex::new(load_settings_from_disk()),
    });
    
    let tracker_state_clone = tracker_state.clone();
    let settings_state_clone = settings_state.clone();

    // Start background polling
    thread::spawn(move || {
        loop {
            // Check settings first
            let is_auto = {
                let settings = settings_state_clone.settings.lock().unwrap();
                settings.auto_context_detection
            };

            if is_auto {
                if let Ok(window) = get_active_window() {
                    let app_name = window.app_name;
                    let title = window.title;

                    // Match context
                    let mut matched_context_id = None;
                    {
                        let settings = settings_state_clone.settings.lock().unwrap();
                        'ctx_loop: for ctx in &settings.contexts {
                            for rule in &ctx.rules {
                                let target = if rule.rule_type == "process" {
                                    &app_name
                                } else {
                                    &title
                                };
                                
                                let matched = if rule.match_type == "exact" {
                                    target == &rule.value
                                } else {
                                    target.contains(&rule.value)
                                };

                                if matched {
                                    matched_context_id = Some(ctx.id.clone());
                                    break 'ctx_loop;
                                }
                            }
                        }
                    }

                    // Adjust these names based on actual process names
                    // Ignore stashpad itself
                    let app_name_lower = app_name.to_lowercase();
                    if !app_name_lower.contains("stashpad")
                        && app_name_lower != "app"
                        && app_name_lower != "webview"
                    {
                        let mut state = tracker_state_clone.lock().unwrap();
                        state.last_external_app = Some(AppContext {
                            window_title: title,
                            process_name: app_name,
                            detected_context_id: None, // Filled by getter
                        });
                        state.current_context_id = matched_context_id;
                    }
                }
            }
            thread::sleep(Duration::from_millis(500));
        }
    });

    let mut builder = tauri::Builder::default()
        .manage(tracker_state)
        .manage(stash_state)
        .manage(settings_state);

    #[cfg(debug_assertions)]
    {
        builder = builder.plugin(devtools);
    }

    #[cfg(not(debug_assertions))]
    {
        builder = builder.plugin(tauri_plugin_log::Builder::default().build());
    }

    builder
        .plugin(tauri_plugin_global_shortcut::Builder::new().with_handler(move |app, _shortcut, event| {
             // Handle global shortcut (toggle window)
             use tauri_plugin_global_shortcut::ShortcutState;
             use tauri::Manager; // For get_webview_window

             if event.state == ShortcutState::Pressed {
                 if let Some(window) = app.get_webview_window("main") {
                        if window.is_visible().unwrap_or(false) && window.is_focused().unwrap_or(false) {
                            let _ = window.hide();
                        } else {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                 }
             }
        }).build())
        .invoke_handler(tauri::generate_handler![
            get_previous_app_info,
            get_smart_transfer_target,
            save_stash,
            save_stashes,
            load_stashes,
            delete_stash,
            delete_completed_stashes,
            save_asset,
            save_asset_from_path,
            copy_to_clipboard,
            start_drag,
            get_settings,
            save_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
