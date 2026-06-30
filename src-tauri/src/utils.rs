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
use crate::models::AppContext;
use crate::state::TrackerState;

// Window vibrancy effects (Windows and macOS only)
#[cfg(target_os = "windows")]
use window_vibrancy::{apply_acrylic, apply_mica, clear_acrylic, clear_mica};
#[cfg(target_os = "macos")]
use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial};

pub fn get_app_dir() -> PathBuf {
    dirs::home_dir()
        .expect("Could not resolve home directory")
        .join(".stashpad")
}

const DEFAULT_SYSTEM_PROMPT: &str = r#"<instructions>
You are an expert prompt engineer. Transform raw notes into clear, structured AI agent prompts.
</instructions>

<output_format>
ACTION: <Short, imperative action line>

CONTEXT:
- <Essential context item 1> (Max 3, omit section if none)

CONSTRAINTS: 
- <Specific requirement 1> (Omit section if none)

TAGS: <Only hashtags present in original input, space-separated. Omit section entirely if none.>
</output_format>

<rules>
1. Be extremely concise - every word must add value.
2. Remove fluff, greetings, and unnecessary explanations.
3. Use imperative voice ("Implement X" not "Please implement X").
4. Preserve all technical terms, code, file paths, and specifics EXACTLY.
5. HASHTAGS (#): ONLY preserve hashtags that were in the ORIGINAL input. DO NOT suggest or add NEW hashtags.
6. If no hashtags were in the input, the output MUST NOT contain the word "TAGS" or any hashtags.
7. Structure for scannability - use Markdown bullets (-), not paragraphs.
8. Use valid Markdown formatting throughout the variable parts of the template.
9. Do not put the whole output in a Markdown block.
10. Make sure to preserve all aspects of the original input.
11. Follow the output format template exactly.
12. Return ONLY the enhanced prompt following the template. Do not include any meta-commentary or conversational filler.
</rules>"#;

pub fn get_system_prompt_path() -> PathBuf {
    get_app_dir().join("enhancement_prompt.txt")
}

pub fn ensure_storage_ready() {
    let app_dir = get_app_dir();
    let cache_dir = app_dir.join("cache");
    if !app_dir.exists() {
        fs::create_dir_all(&app_dir).expect("Failed to create app dir");
    }
    if !cache_dir.exists() {
        fs::create_dir_all(&cache_dir).expect("Failed to create cache dir");
    }

    // Ensure default system prompt file exists
    let prompt_path = get_system_prompt_path();
    if !prompt_path.exists() {
        let _ = fs::write(prompt_path, DEFAULT_SYSTEM_PROMPT);
    }
}

/// 
/// Platform support:
/// - Windows 11: Mica effect (theme handled by OS)
/// - Windows 10: Acrylic effect with theme-aware background color
/// - macOS: Vibrancy with HudWindow material
/// - Linux: No library support (compositor handles transparency)
pub fn apply_window_effects_to_window(window: &tauri::WebviewWindow, enabled: Option<bool>, _theme: Option<&str>) {
    #[cfg(target_os = "linux")]
    {
        let _ = window;
        let _ = _theme;
    }
    let should_enable = enabled.unwrap_or(true);
    
    if should_enable {
        // Apply OS-specific vibrancy effects
        #[cfg(target_os = "windows")]
        {
            // Determine if we should use dark or light colors based on theme
            // "dark" -> dark colors, "light" -> light colors, "system" or None -> dark (default)
            let is_dark = match _theme {
                Some("light") => false,
                _ => true, // dark, system, or unknown defaults to dark
            };
            
            // Choose Acrylic background color based on theme
            // Dark: zinc-900 (18, 18, 18), Light: zinc-50 (249, 250, 251)
            let acrylic_color = if is_dark {
                (18, 18, 18, 200)
            } else {
                (249, 250, 251, 200)
            };
            
            // Clear any existing effects first to ensure color change takes effect
            let _ = clear_mica(window);
            let _ = clear_acrylic(window);
            
            // Try Mica first (Windows 11) - Mica respects system theme automatically
            match apply_mica(window, Some(is_dark)) {
                Ok(_) => {
                    println!("Applied Mica effect (Windows 11, dark={})", is_dark);
                }
                Err(_) => {
                    // Mica not available (Windows 10 or earlier), try Acrylic
                    println!("Mica not available, trying Acrylic (Windows 10, dark={})…", is_dark);
                    match apply_acrylic(window, Some(acrylic_color)) {
                        Ok(_) => {
                            println!("Applied Acrylic effect (Windows 10, dark={})", is_dark);
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

#[tauri::command]
pub fn get_device_name() -> String {
    std::process::Command::new("hostname")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "Unknown Device".to_string())
}

#[tauri::command]
pub fn get_installation_source() -> String {
    if let Ok(exe_path) = std::env::current_exe() {
        let path_str = exe_path.to_string_lossy().to_lowercase();
        if path_str.contains("scoop/apps") || path_str.contains("scoop\\apps") {
            return "scoop".to_string();
        }
        if path_str.contains("homebrew/caskroom") || path_str.contains("homebrew\\caskroom") {
            return "homebrew".to_string();
        }
        if path_str.contains("windowsapps") {
            return "windowsapps".to_string();
        }
    }
    "standalone".to_string()
}

#[tauri::command]
pub fn copy_to_clipboard(text: String) -> Result<(), String> {
    println!("Copying to clipboard");
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text).map_err(|e| e.to_string())?;
    Ok(())
}

/// Reads text content from the system clipboard.
///
/// Used by the Shift+Paste override on macOS where
/// `navigator.clipboard.readText()` triggers a permission prompt.
#[tauri::command]
pub fn read_clipboard_text() -> Result<String, String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.get_text().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn start_drag(window: tauri::Window, text: String, files: Vec<String>) -> Result<(), String> {
    println!("Starting drag with {} files", files.len());

    let items = if !files.is_empty() {
        let paths: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
        drag::DragItem::Files(paths)
    } else {
        // Create temporary text file for text-only stashes
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

    #[cfg(target_os = "linux")]
    {
        let gtk_window = window.gtk_window().map_err(|e| e.to_string())?;
        drag::start_drag(&gtk_window, items, image, |_, _| {}, Default::default())
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "linux"))]
    {
        drag::start_drag(&window, items, image, |_, _| {}, Default::default())
            .map_err(|e| e.to_string())?;
    }

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
pub fn get_smart_transfer_target(state: State<Arc<Mutex<TrackerState>>>) -> String {
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
pub fn show_in_folder(path: String) {
    // Security: verify the path exists and canonicalize it before passing to OS commands
    let file_path = std::path::Path::new(&path);
    let canonical = match file_path.canonicalize() {
        Ok(p) => p,
        Err(_) => return, // Silently fail if path doesn't exist
    };
    let safe_path = canonical.to_string_lossy();
    
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer")
            .args(["/select,", &safe_path])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .args(["-R", &safe_path])
            .spawn();
    }
    #[cfg(target_os = "linux")]
    {
        if let Some(parent) = canonical.parent() {
            let _ = std::process::Command::new("xdg-open")
                .arg(parent)
                .spawn();
        }
    }
}

/// Checks if the app has macOS Screen Recording permission.
/// This is required for `active-win-pos-rs` to read window titles.
/// Returns `true` on non-macOS platforms (permission not needed).
#[tauri::command]
pub fn check_screen_recording_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        // Use FFI to call CGPreflightScreenCaptureAccess from CoreGraphics
        extern "C" {
            fn CGPreflightScreenCaptureAccess() -> bool;
        }
        // Safety: This is a well-known CoreGraphics API that returns a simple bool
        unsafe { CGPreflightScreenCaptureAccess() }
    }
    #[cfg(not(target_os = "macos"))]
    {
        true // Permission not required on other platforms
    }
}

/// Opens macOS System Settings to the Screen Recording permission pane.
/// No-op on non-macOS platforms.
#[tauri::command]
pub fn open_macos_screen_recording_settings() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
            .spawn();
    }
}

#[tauri::command]
pub fn is_windows_10() -> bool {
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

#[tauri::command]
pub fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    
    let autostart_manager = app.autolaunch();
    
    if enabled {
        autostart_manager.enable().map_err(|e| format!("Failed to enable autostart: {}", e))?;
        println!("Autostart enabled");
    } else {
        autostart_manager.disable().map_err(|e| format!("Failed to disable autostart: {}", e))?;
        println!("Autostart disabled");
    }
    
    Ok(())
}

#[tauri::command]
pub fn get_autostart_enabled(app: tauri::AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    
    let autostart_manager = app.autolaunch();
    autostart_manager.is_enabled().map_err(|e| format!("Failed to check autostart status: {}", e))
}

#[tauri::command]
pub fn check_apple_intelligence_available() -> Result<bool, String> {
    #[cfg(all(target_os = "macos", feature = "macos-apple-intelligence"))]
    {
        use fm_rs::SystemLanguageModel;
        let model = SystemLanguageModel::new().map_err(|e| e.to_string())?;
        match model.ensure_available() {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
    #[cfg(any(not(target_os = "macos"), not(feature = "macos-apple-intelligence")))]
    {
        Ok(false)
    }
}

#[tauri::command]
pub fn apple_intelligence_enhance(content: String, system_prompt: String) -> Result<String, String> {
    #[cfg(all(target_os = "macos", feature = "macos-apple-intelligence"))]
    {
        use fm_rs::{SystemLanguageModel, Session, GenerationOptions};
        let model = SystemLanguageModel::new().map_err(|e| e.to_string())?;
        let session = Session::with_instructions(&model, &system_prompt).map_err(|e| e.to_string())?;
        let response = session.respond(&content, &GenerationOptions::default()).map_err(|e| e.to_string())?;
        Ok(response.content().to_string())
    }
    #[cfg(any(not(target_os = "macos"), not(feature = "macos-apple-intelligence")))]
    {
        let _ = content;
        let _ = system_prompt;
        Err("Apple Intelligence is not available in this build".into())
    }
}

#[tauri::command]
pub fn get_system_prompt() -> String {
    let path = get_system_prompt_path();
    if path.exists() {
        fs::read_to_string(path).unwrap_or_else(|_| DEFAULT_SYSTEM_PROMPT.to_string())
    } else {
        DEFAULT_SYSTEM_PROMPT.to_string()
    }
}

#[tauri::command]
pub fn get_system_prompt_path_str() -> String {
    get_system_prompt_path().to_string_lossy().to_string()
}

#[tauri::command]
pub fn check_system_prompt_exists() -> bool {
    get_system_prompt_path().exists()
}

#[tauri::command]
pub fn create_system_prompt_file() -> Result<(), String> {
    let path = get_system_prompt_path();
    if !path.exists() {
        fs::write(path, DEFAULT_SYSTEM_PROMPT).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn open_system_prompt_file() {
    let path = get_system_prompt_path();
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer").arg(&path).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(&path).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(&path).spawn();
    }
}

