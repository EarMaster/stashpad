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
use tauri::{Manager, State};
use tauri::window::Color;

// Window vibrancy effects (Windows and macOS only)
#[cfg(target_os = "windows")]
use window_vibrancy::{apply_acrylic, apply_mica, clear_acrylic, clear_mica};
#[cfg(target_os = "macos")]
use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial};

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
    #[serde(default)]
    pub completed_at: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveOptions {
    pub stash: StashItem,
    #[serde(default)]
    pub invert_position: bool,
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
    pub visual_effects_enabled: Option<bool>,
    #[serde(default)]
    pub contexts: Vec<Context>,
    #[serde(default)]
    pub active_context_id: Option<String>,
    #[serde(default)]
    pub shortcuts: std::collections::HashMap<String, String>,
    /// Locale preference: 'auto' for automatic detection or a specific locale code
    #[serde(default)]
    pub locale: Option<String>,
    #[serde(default = "default_new_stash_position")]
    pub new_stash_position: String, // "top" or "bottom"
    #[serde(default)]
    pub theme: Option<String>, // "light", "dark", "system"
    #[serde(default)]
    pub strip_tags_on_copy: bool, // Strip #tags when copying to clipboard
    #[serde(default = "default_clear_completed_strategy")]
    pub clear_completed_strategy: String,
    #[serde(default = "default_clear_completed_days")]
    pub clear_completed_days: u32,
}

fn default_clear_completed_strategy() -> String {
    "never".to_string()
}

fn default_clear_completed_days() -> u32 {
    7
}

fn default_new_stash_position() -> String {
    "top".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_context_detection: true,
            visual_effects_enabled: None, // None implies "follow OS/default"
            contexts: vec![],
            active_context_id: None,
            shortcuts: std::collections::HashMap::new(),
            locale: None,
            new_stash_position: "top".into(),
            theme: None,
            strip_tags_on_copy: false,
            clear_completed_strategy: default_clear_completed_strategy(),
            clear_completed_days: default_clear_completed_days(),
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

/// Apply window vibrancy effects based on the visual_effects_enabled setting.
/// - None: Use OS defaults (effects enabled)
/// - Some(true): Enable effects
/// - Some(false): Disable effects (opaque background)
/// 
/// Platform support:
/// - Windows 11: Mica effect
/// - Windows 10: Acrylic effect (fallback)
/// - macOS: Vibrancy with HudWindow material
/// - Linux: No library support (compositor handles transparency)
fn apply_window_effects_to_window(window: &tauri::WebviewWindow, enabled: Option<bool>) {
    let should_enable = enabled.unwrap_or(true);
    
    if should_enable {
        // Apply OS-specific vibrancy effects
        #[cfg(target_os = "windows")]
        {
            // Try Mica first (Windows 11)
            match apply_mica(window, Some(true)) {
                Ok(_) => {
                    println!("Applied Mica effect (Windows 11)");
                }
                Err(_) => {
                    // Mica not available (Windows 10 or earlier), try Acrylic
                    println!("Mica not available, trying Acrylic (Windows 10)…");
                    match apply_acrylic(window, Some((18, 18, 18, 200))) {
                        Ok(_) => {
                            println!("Applied Acrylic effect (Windows 10)");
                        }
                        Err(e) => {
                            println!("Failed to apply Acrylic effect: {:?}", e);
                            // Fall back to transparent window without effects
                        }
                    }
                }
            }
        }
        
        #[cfg(target_os = "macos")]
        {
            // Apply vibrancy with a dark appearance
            if let Err(e) = apply_vibrancy(
                window,
                NSVisualEffectMaterial::HudWindow,
                None,
                None,
            ) {
                println!("Failed to apply vibrancy effect: {:?}", e);
            } else {
                println!("Applied vibrancy effect (macOS)");
            }
        }
        
        // Linux: No window-vibrancy support, transparency handled by compositor
        #[cfg(target_os = "linux")]
        {
            println!("Linux: Window transparency is handled by the compositor");
        }
    } else {
        // Clear effects for opaque background
        #[cfg(target_os = "windows")]
        {
            // Try to clear both effects (one will succeed based on what was applied)
            let _ = clear_mica(window);
            let _ = clear_acrylic(window);
            println!("Cleared window effects (Windows)");
        }
        
        #[cfg(target_os = "macos")]
        {
            // On macOS, vibrancy can't be easily cleared programmatically,
            // but the CSS will show an opaque background when effects are disabled
            println!("macOS: Visual effects disabled (CSS will handle opaque background)");
        }
        
        #[cfg(target_os = "linux")]
        {
            println!("Linux: Visual effects disabled");
        }
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
    
    // Check if visual effects setting changed
    let effects_changed = current.visual_effects_enabled != settings.visual_effects_enabled;
    
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
    drop(current); // Release lock before applying effects
    
    // Apply window effects if setting changed
    if effects_changed {
        if let Some(window) = app.get_webview_window("main") {
            apply_window_effects_to_window(&window, settings.visual_effects_enabled);
        }
    }
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
fn save_stash(
    state: State<Arc<StashState>>, 
    settings_state: State<Arc<SettingsState>>, 
    options: SaveOptions
) {
    let stash = options.stash;
    let invert = options.invert_position;
    
    let mut stashes = state.stashes.lock().unwrap();
    let settings = settings_state.settings.lock().unwrap();
    let position = settings.new_stash_position.clone();

    let effective_position = if invert {
        if position == "bottom" { "top" } else { "bottom" }
    } else {
        position.as_str()
    };


    // Upsert logic: if id exists, replace; else push
    if let Some(index) = stashes.iter().position(|s| s.id == stash.id) {
        let old_item = &stashes[index];
        let status_changed = old_item.completed != stash.completed;
        
        let mut new_stash = stash.clone();
        if status_changed {
             if new_stash.completed {
                 new_stash.completed_at = Some(chrono::Utc::now().to_rfc3339());
             } else {
                 new_stash.completed_at = None;
             }
        } else if new_stash.completed && new_stash.completed_at.is_none() {
             // Enforce completed_at if missing
             if let Some(old_completed_at) = &old_item.completed_at {
                 new_stash.completed_at = Some(old_completed_at.clone());
             }
        }

        if status_changed {
            // Remove and re-insert at top/bottom according to setting
            stashes.remove(index);
            if effective_position == "bottom" {
                stashes.push(new_stash);
            } else {
                stashes.insert(0, new_stash);
            }
        } else {
            // Update in place to preserve manual order
            stashes[index] = new_stash;
        }
    } else {
        // New item
        let mut new_stash = stash.clone();
        if new_stash.completed && new_stash.completed_at.is_none() {
            new_stash.completed_at = Some(chrono::Utc::now().to_rfc3339());
        }

        if effective_position == "bottom" {
            stashes.push(new_stash);
        } else {
            stashes.insert(0, new_stash);
        }
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
fn delete_completed_stashes(state: State<Arc<StashState>>, context_id: Option<String>) {
    let mut stashes = state.stashes.lock().unwrap();
    // Only delete stashes that match the context_id (or all if context_id is None - though manual action is usually context-aware)
    // Actually, manual "Clear Completed" is per-context.
    // If context_id is None, it implies clearing ALL (internal usage maybe). 
    stashes.retain(|s| !(s.completed && (context_id.is_none() || s.context_id == context_id)));
    persist_stashes_to_disk(&stashes);
}

fn perform_startup_cleanup(stashes: &mut Vec<StashItem>, settings: &Settings) {
    if settings.clear_completed_strategy == "on-close" {
         println!("Startup Cleanup: Clearing all completed stashes (on-close strategy)");
         stashes.retain(|s| !s.completed);
    } else if settings.clear_completed_strategy == "after-n-days" {
         let days = settings.clear_completed_days as i64;
         let now = chrono::Utc::now();
         println!("Startup Cleanup: Clearing completed stashes older than {} days", days);
         stashes.retain(|s| {
             if !s.completed { return true; }
             // Fallback to created_at if completed_at is missing (legacy data)
             let time_str = s.completed_at.as_ref().or(Some(&s.created_at));
             
             if let Some(ts_str) = time_str {
                 if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(ts_str) {
                      let age = now.signed_duration_since(ts);
                      if age.num_days() >= days {
                          return false; // delete
                      }
                 }
             }
             true // keep
         });
    }
}

#[tauri::command]
fn save_stashes(state: State<Arc<StashState>>, stashes_list: Vec<StashItem>) {
    let mut stashes = state.stashes.lock().unwrap();
    *stashes = stashes_list;
    persist_stashes_to_disk(&stashes);
}
 
#[tauri::command]
fn trigger_auto_cleanup(state: State<Arc<StashState>>, settings_state: State<Arc<SettingsState>>) {
    let mut stashes = state.stashes.lock().unwrap();
    let settings = settings_state.settings.lock().unwrap();
    
    // Reuse the logic
    perform_startup_cleanup(&mut stashes, &settings);
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

/// Response structure for file preview data
#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FilePreviewData {
    /// Type of the file: "image", "video", "text", or "unsupported"
    pub file_type: String,
    /// For images: base64 encoded data with data URI prefix
    /// For videos: file path (to be converted to asset URL)
    /// For text: the file content (limited to first 10KB)
    pub content: String,
    /// Original file name
    pub file_name: String,
    /// MIME type if detected
    pub mime_type: String,
}

/// Reads a file and returns preview data based on its type.
/// - Images: Returns base64 encoded data
/// - Videos: Returns the file path (frontend converts to asset URL)
/// - Text files: Returns first 10KB of content
/// - Other: Returns unsupported type indicator
#[tauri::command]
fn read_file_for_preview(path: String) -> Result<FilePreviewData, String> {
    let file_path = std::path::Path::new(&path);
    
    if !file_path.exists() {
        return Err("File does not exist".into());
    }

    let file_name = file_path
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("unknown"))
        .to_string_lossy()
        .into_owned();

    let extension = file_path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    // Determine file type based on extension
    let (file_type, mime_type) = match extension.as_str() {
        // Image types
        "png" => ("image", "image/png"),
        "jpg" | "jpeg" => ("image", "image/jpeg"),
        "gif" => ("image", "image/gif"),
        "webp" => ("image", "image/webp"),
        "svg" => ("image", "image/svg+xml"),
        "bmp" => ("image", "image/bmp"),
        "ico" => ("image", "image/x-icon"),
        
        // Video types
        "mp4" => ("video", "video/mp4"),
        "webm" => ("video", "video/webm"),
        "ogg" | "ogv" => ("video", "video/ogg"),
        "mov" => ("video", "video/quicktime"),
        "avi" => ("video", "video/x-msvideo"),
        "mkv" => ("video", "video/x-matroska"),
        
        // Text and code types
        "txt" | "md" | "markdown" => ("text", "text/plain"),
        "json" => ("text", "application/json"),
        "xml" => ("text", "application/xml"),
        "html" | "htm" => ("text", "text/html"),
        "css" => ("text", "text/css"),
        "js" | "mjs" => ("text", "application/javascript"),
        "ts" | "tsx" => ("text", "text/typescript"),
        "jsx" => ("text", "text/jsx"),
        "py" => ("text", "text/x-python"),
        "rs" => ("text", "text/x-rust"),
        "go" => ("text", "text/x-go"),
        "java" => ("text", "text/x-java"),
        "c" | "h" => ("text", "text/x-c"),
        "cpp" | "hpp" | "cc" => ("text", "text/x-c++"),
        "cs" => ("text", "text/x-csharp"),
        "rb" => ("text", "text/x-ruby"),
        "php" => ("text", "text/x-php"),
        "sh" | "bash" | "zsh" => ("text", "text/x-shellscript"),
        "ps1" => ("text", "text/x-powershell"),
        "yaml" | "yml" => ("text", "text/yaml"),
        "toml" => ("text", "text/toml"),
        "ini" | "cfg" | "conf" => ("text", "text/plain"),
        "log" => ("text", "text/plain"),
        "sql" => ("text", "text/x-sql"),
        "svelte" => ("text", "text/x-svelte"),
        "vue" => ("text", "text/x-vue"),
        
        _ => ("unsupported", "application/octet-stream"),
    };

    let content = match file_type {
        "image" => {
            // Read image and convert to base64
            match fs::read(file_path) {
                Ok(data) => {
                    use base64::{Engine as _, engine::general_purpose};
                    let b64 = general_purpose::STANDARD.encode(&data);
                    format!("data:{};base64,{}", mime_type, b64)
                }
                Err(e) => return Err(format!("Failed to read image: {}", e)),
            }
        }
        "video" => {
            // For videos, return the file path - frontend will convert to asset URL
            path.clone()
        }
        "text" => {
            // Read text file content (limit to 10KB for preview)
            match fs::read(file_path) {
                Ok(data) => {
                    let max_size = 10 * 1024; // 10KB
                    let truncated = if data.len() > max_size {
                        &data[..max_size]
                    } else {
                        &data
                    };
                    String::from_utf8_lossy(truncated).into_owned()
                }
                Err(e) => return Err(format!("Failed to read file: {}", e)),
            }
        }
        _ => String::new(),
    };

    Ok(FilePreviewData {
        file_type: file_type.into(),
        content,
        file_name,
        mime_type: mime_type.into(),
    })
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

#[tauri::command]
fn show_in_folder(path: String) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer")
            .args(["/select,", &path])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .args(["-R", &path])
            .spawn();
    }
    #[cfg(target_os = "linux")]
    {
        if let Some(parent) = std::path::Path::new(&path).parent() {
            let _ = std::process::Command::new("xdg-open")
                .arg(parent)
                .spawn();
        }
    }
}

#[tauri::command]
fn is_windows_10() -> bool {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        use std::os::windows::process::CommandExt;
        
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let output = Command::new("cmd")
            .args(["/c", "ver"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();

        if let Ok(o) = output {
            let s = String::from_utf8_lossy(&o.stdout);
            // Format: Microsoft Windows [Version 10.0.xxxxx.xxx]
            // We look for "Version 10.0." and then the build number
            if let Some(ver_idx) = s.find("Version 10.0.") {
                let start = ver_idx + "Version 10.0.".len();
                let rest = &s[start..];
                // rest starts with build number, e.g. "19045.3693]"
                // find the next dot or closing bracket
                let end = rest.find('.').or_else(|| rest.find(']')).unwrap_or(rest.len());
                if let Ok(build) = rest[..end].parse::<u32>() {
                     // Windows 11 starts at build 22000
                     return build < 22000;
                }
            }
        }
        false
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
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
    
    // Perform startup cleanup
    {
        let mut stashes_lock = stash_state.stashes.lock().unwrap();
        let settings_lock = settings_state.settings.lock().unwrap();
        perform_startup_cleanup(&mut stashes_lock, &settings_lock);
        // Persist the cleaned up stashes
        persist_stashes_to_disk(&stashes_lock);
    }
    
    let tracker_state_clone = tracker_state.clone();
    let settings_state_clone = settings_state.clone();
    
    // Apply initial effects
    // Need AppHandle. But we are in run() building the app.
    // We can use setup hook.

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
                    // Additional check for "electron" or "tauri" generic wrappers if necessary
                    // But importantly, we MUST ignore our own window
                    if !app_name_lower.contains("stashpad")
                        && app_name_lower != "app" 
                        && app_name_lower != "webview"
                    {
                        // Check if we matched a context
                        // If NO context was matched, we should NOT overwrite the current context if it was
                        // previously set by a valid external app. 
                        // However, the rule is: "If is_auto is enabled, the active window determines the context".
                        // If the active window is NOT Stashpad, we update.
                        
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

    // Clone for setup hook
    let settings_state_for_setup = settings_state.clone();

    let mut builder = tauri::Builder::default()
        .setup(move |app| {
            // Apply initial window effects based on saved settings
            let settings = settings_state_for_setup.settings.lock().unwrap();
            let visual_effects_enabled = settings.visual_effects_enabled;
            drop(settings); // Release lock

            if let Some(window) = app.get_webview_window("main") {
                // Platform-specific background handling to ensure correct visual/vibrancy behavior
                // while keeping 'transparent: false' in config (which fixes Windows border artifacts).
                
                // Windows & macOS: Manually clear background to allow Vibrancy/Mica to show through
                #[cfg(any(target_os = "windows", target_os = "macos"))]
                {
                    let _ = window.set_background_color(Some(Color(0, 0, 0, 0)));
                }

                // Linux: Vibrancy is not supported, so we enforce a consistent dark background
                // to match the app theme (Zinc-950 #18181b), preventing a potential white flash/background.
                #[cfg(target_os = "linux")]
                {
                    let _ = window.set_background_color(Some(Color(24, 24, 27, 255)));
                }

                apply_window_effects_to_window(&window, visual_effects_enabled);
            }

            Ok(())
        })
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
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
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
            trigger_auto_cleanup,
            save_asset,
            save_asset_from_path,
            read_file_for_preview,
            show_in_folder,
            copy_to_clipboard,
            start_drag,
            get_settings,
            save_settings,
            is_windows_10
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
