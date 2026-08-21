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
use std::sync::OnceLock;
use std::sync::{Arc, Mutex};
use tauri::State;
use crate::models::AppContext;
use crate::state::{TrackerState, lock_or_recover};

/// Run blocking work on the blocking pool and flatten the join error away.
///
/// Tauri runs `async fn` commands on the shared async runtime, and Tokio does not move a
/// task that blocks its worker - so any command doing filesystem, registry,
/// credential-store or subprocess work has to hand it to the blocking pool, or it starves
/// the pool every other command needs and the whole app stops answering `invoke`.
async fn run_blocking<T, F>(work: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    match tauri::async_runtime::spawn_blocking(work).await {
        Ok(result) => result,
        Err(e) => Err(format!("Background task failed: {}", e)),
    }
}

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

/// The machine name, resolved once per process.
///
/// This used to shell out to `hostname` on every call, on the main thread, and without
/// `CREATE_NO_WINDOW` - so on Windows it also flashed a console window. Windows exposes
/// the name in the environment, so no process is needed there at all.
static DEVICE_NAME: OnceLock<String> = OnceLock::new();

fn resolve_device_name() -> String {
    #[cfg(target_os = "windows")]
    {
        if let Ok(name) = std::env::var("COMPUTERNAME") {
            if !name.trim().is_empty() {
                return name.trim().to_string();
            }
        }
    }

    // Unix: the variable is often not exported to children, so fall back to the tool.
    // No console window exists to flash here.
    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(name) = std::env::var("HOSTNAME") {
            if !name.trim().is_empty() {
                return name.trim().to_string();
            }
        }
        if let Ok(output) = std::process::Command::new("hostname").output() {
            let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !name.is_empty() {
                return name;
            }
        }
    }

    "Unknown Device".to_string()
}

#[tauri::command]
pub async fn get_device_name() -> String {
    DEVICE_NAME.get_or_init(resolve_device_name).clone()
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

/// Put text on the system clipboard.
///
/// `async` on purpose: a non-async command runs on the main thread, and opening the
/// clipboard can block when another process is holding it.
#[tauri::command]
pub async fn copy_to_clipboard(text: String) -> Result<(), String> {
    run_blocking(move || {
        let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        clipboard.set_text(text).map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
}

/// Reads text content from the system clipboard.
///
/// Used by the Shift+Paste override on macOS where
/// `navigator.clipboard.readText()` triggers a permission prompt.
#[tauri::command]
pub async fn read_clipboard_text() -> Result<String, String> {
    run_blocking(|| {
        let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        clipboard.get_text().map_err(|e| e.to_string())
    })
    .await
}

/// Begin an OS drag from the stash.
///
/// Deliberately **not** `async`: on Windows this ends in `DoDragDrop`, which must run on
/// the thread that owns the window and runs its own modal message loop until the drop
/// completes. Moving it to a worker would break dragging outright. The trade-off is that
/// a drag which never receives its mouse-up holds the UI thread, so the work done before
/// the drag call is kept minimal.
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
    let state = lock_or_recover(&state);
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

/// Reveal a file in the platform file manager.
///
/// `async` because `canonicalize` blocks on a disconnected network path or a removed
/// drive, and on the main thread that freezes the window itself.
#[tauri::command]
pub async fn show_in_folder(path: String) {
    let _ = tauri::async_runtime::spawn_blocking(move || show_in_folder_blocking(path)).await;
}

fn show_in_folder_blocking(path: String) {
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

/// Whether this is Windows 10 rather than 11, resolved once per process.
///
/// The answer cannot change while the app is running, but this ran a `cmd /c ver`
/// subprocess on the main thread on every call - and it is called on both the app mount
/// and the settings mount. Process creation under AV or EDR is not cheap.
static IS_WINDOWS_10: OnceLock<bool> = OnceLock::new();

#[tauri::command]
pub async fn is_windows_10() -> bool {
    if let Some(cached) = IS_WINDOWS_10.get() {
        return *cached;
    }
    let detected = tauri::async_runtime::spawn_blocking(detect_windows_10)
        .await
        .unwrap_or(false);
    *IS_WINDOWS_10.get_or_init(|| detected)
}

fn detect_windows_10() -> bool {
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

/// Enable or disable launch-at-login. Registry (or launch agent) I/O, so off-thread.
#[tauri::command]
pub async fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    run_blocking(move || {
        use tauri_plugin_autostart::ManagerExt;

        let autostart_manager = app.autolaunch();

        if enabled {
            autostart_manager
                .enable()
                .map_err(|e| format!("Failed to enable autostart: {}", e))?;
            log::info!("Autostart enabled");
        } else {
            autostart_manager
                .disable()
                .map_err(|e| format!("Failed to disable autostart: {}", e))?;
            log::info!("Autostart disabled");
        }

        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn get_autostart_enabled(app: tauri::AppHandle) -> Result<bool, String> {
    run_blocking(move || {
        use tauri_plugin_autostart::ManagerExt;

        let autostart_manager = app.autolaunch();
        autostart_manager
            .is_enabled()
            .map_err(|e| format!("Failed to check autostart status: {}", e))
    })
    .await
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
pub async fn apple_intelligence_enhance(content: String, system_prompt: String) -> Result<String, String> {
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

#[tauri::command]
pub fn get_previous_app_info(state: State<Arc<Mutex<TrackerState>>>) -> AppContext {
    let state = lock_or_recover(&state);
    if let Some(app) = &state.last_external_app {
        let mut app_ctx = app.clone();
        app_ctx.detected_context_id = state.current_context_id.clone();
        app_ctx
    } else {
        AppContext {
            window_title: "".into(),
            process_name: "".into(),
            detected_context_id: None,
        }
    }
}


/// Record an error the webview could not handle itself.
///
/// In a release build the webview console is discarded, so a render error that killed
/// the UI left no trace anywhere. Routing it through `log::error!` puts it in the same
/// app log as backend failures, which is the only place a user can be asked to look.
///
/// The message is truncated by character, not byte, so a multi-byte stack trace cannot
/// panic the command - the same mistake that used to kill the sync commands.
#[tauri::command]
pub fn log_frontend_error(message: String) {
    const MAX_CHARS: usize = 4000;
    let trimmed: String = message.chars().take(MAX_CHARS).collect();
    let elided = message.chars().nth(MAX_CHARS).is_some();
    log::error!("[frontend] {}{}", trimmed, if elided { " […]" } else { "" });
}
