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

use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::thread;

use tauri::menu::Menu;
// Only the macOS branch builds a custom menu; importing these unconditionally warns on
// every other platform.
#[cfg(target_os = "macos")]
use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::Manager;
use active_win_pos_rs::get_active_window;

mod models;
mod state;
mod utils;
mod keychain;
mod settings;
mod contexts;
mod sync;
mod stashes;
mod transfer;
pub mod db;

use models::{AppContext, Context};
use state::{DbState, TrackerState, WsState, SettingsState, lock_or_recover};
use utils::{
    get_app_dir, ensure_storage_ready, apply_window_effects_to_window, apply_window_background,
    get_system_prompt_path,
};
use settings::load_settings_from_disk;
use stashes::perform_startup_cleanup;
use db::DbManager;

/// How often the active window is sampled for auto context detection.
///
/// The frontend polls `get_previous_app_info` once a second, so sampling twice as
/// often only bought contention on the database mutex.
const WINDOW_POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Matches the active window against context rules, caching compiled regexes.
///
/// The regex for a rule used to be compiled on every tick - in fact twice, since the
/// first compile's result was discarded. Rules change rarely, so the compilation is
/// cached and keyed by the pattern actually used.
struct ContextMatcher {
    /// pattern (already case-folded when needed) -> compiled regex, or `None` when the
    /// pattern does not compile, so a bad rule is not retried twice a second.
    compiled: HashMap<String, Option<regex::Regex>>,
}

impl ContextMatcher {
    fn new() -> Self {
        Self {
            compiled: HashMap::new(),
        }
    }

    /// The id of the first context whose rules match, if any.
    fn match_context(
        &mut self,
        contexts: &[Context],
        app_name: &str,
        title: &str,
    ) -> Option<String> {
        for ctx in contexts {
            for rule in &ctx.rules {
                let target = if rule.rule_type == "process" {
                    app_name
                } else {
                    title
                };

                let matched = if rule.use_regex {
                    let pattern = if rule.match_case {
                        rule.value.clone()
                    } else {
                        format!("(?i){}", rule.value)
                    };
                    self.regex_for(pattern)
                        .map(|re| re.is_match(target))
                        .unwrap_or(false)
                } else if rule.match_case {
                    if rule.match_type == "exact" {
                        target == rule.value
                    } else {
                        target.contains(&rule.value)
                    }
                } else {
                    let target = target.to_lowercase();
                    let value = rule.value.to_lowercase();
                    if rule.match_type == "exact" {
                        target == value
                    } else {
                        target.contains(&value)
                    }
                };

                if matched {
                    return Some(ctx.id.clone());
                }
            }
        }
        None
    }

    /// The compiled form of `pattern`, compiling it at most once.
    fn regex_for(&mut self, pattern: String) -> Option<&regex::Regex> {
        self.compiled
            .entry(pattern)
            .or_insert_with_key(|p| match regex::Regex::new(p) {
                Ok(re) => Some(re),
                Err(e) => {
                    log::warn!("Ignoring context rule with invalid regex {:?}: {}", p, e);
                    None
                }
            })
            .as_ref()
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 0. init devtools
    #[cfg(debug_assertions)] // only enable instrumentation in development builds
    let devtools = tauri_plugin_devtools::init();

    // 1. Initialize Storage
    ensure_storage_ready();

    // 2. Initialize DB
    let db_path = get_app_dir().join("stashpad.db");
    let db_manager = DbManager::new(&db_path).expect("Failed to init DB");

    let db_state = Arc::new(DbState {
        db: Arc::new(Mutex::new(db_manager)),
    });

    let tracker_state = Arc::new(Mutex::new(TrackerState::new()));
    let settings_state = Arc::new(SettingsState {
        settings: Mutex::new(load_settings_from_disk()),
    });

    let ws_state = Arc::new(WsState {
        task_handle: Mutex::new(None),
    });
    
    // Perform startup cleanup. Settings are copied out first so the two locks are never
    // held at once - the same ordering every command uses.
    {
        let settings_snapshot = settings_state.lock_settings().clone();
        let mut db_lock = db_state.lock_db();
        perform_startup_cleanup(&mut db_lock, &settings_snapshot);
    }
    
    let tracker_state_clone = tracker_state.clone();
    let settings_state_clone = settings_state.clone();
    // Clone db state for background thread
    let db_state_clone = db_state.clone();
    
    // Start background polling.
    //
    // This thread used to hold the global DB mutex across `get_contexts()` *and* the
    // whole nested rule loop, recompiling every regex rule twice per tick (the first
    // compile's result was thrown away outright) - twice a second, forever. Every
    // stash and sync command contends on that same mutex, so the poller was a
    // permanent tax on the rest of the app. Now it copies the rules out under the
    // lock, releases it, and matches against the copy, with compiled regexes cached
    // between ticks.
    thread::spawn(move || {
        let mut matcher = ContextMatcher::new();

        loop {
            // Check settings first
            let is_auto = {
                let settings = settings_state_clone.lock_settings();
                settings.auto_context_detection
            };

            if is_auto {
                if let Ok(window) = get_active_window() {
                    let app_name = window.app_name;
                    let title = window.title;

                    // Copy the rules out and drop the lock immediately. Matching is
                    // pure computation and has no business holding the database.
                    let contexts = {
                        let db = db_state_clone.lock_db();
                        db.get_contexts().unwrap_or_default()
                    };

                    let matched_context_id = matcher.match_context(&contexts, &app_name, &title);

                    let app_name_lower = app_name.to_lowercase();
                    if !app_name_lower.contains("stashpad")
                        && app_name_lower != "app"
                        && app_name_lower != "webview"
                    {
                        let mut state = lock_or_recover(&tracker_state_clone);
                        state.last_external_app = Some(AppContext {
                            window_title: title,
                            process_name: app_name,
                            detected_context_id: None, // Filled by getter
                        });
                        state.current_context_id = matched_context_id;
                    }
                }
            }
            thread::sleep(WINDOW_POLL_INTERVAL);
        }
    });

    // Clone for setup hook
    let settings_state_for_setup = settings_state.clone();

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            println!("Single instance triggered: argv={:?}, cwd={:?}", argv, cwd);
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .setup(move |app| {
            // Apply initial window effects based on saved settings
            let settings = settings_state_for_setup.lock_settings();
            let visual_effects_enabled = settings.visual_effects_enabled;
            let theme = settings.theme.clone();
            // The native menu is built here, before the webview reports a locale, so the
            // one label we own is picked from the saved setting instead. It only changes
            // on restart, which matches the rest of the menu.
            #[cfg(target_os = "macos")]
            let locale = settings.locale.clone();
            drop(settings); // Release lock

            if let Some(window) = app.get_webview_window("main") {
                // Clear only when translucency is actually on. A permanently transparent
                // window is what let the desktop show through wherever the webview had
                // not painted.
                apply_window_background(&window, visual_effects_enabled, theme.as_deref());

                // Build app menu with "Check for Updates…" item on macOS;
                // fall back to default menu on other platforms.
                #[cfg(target_os = "macos")]
                {
                    let _ = (|| -> Result<(), Box<dyn std::error::Error>> {
                        let check_updates_label = match locale.as_deref() {
                            Some("de") => "Nach Updates suchen…",
                            _ => "Check for Updates…",
                        };
                        let check_updates_item = MenuItemBuilder::new(check_updates_label)
                            .id("check_for_updates")
                            .build(app)?;
                        let app_submenu = SubmenuBuilder::new(app, "stashpad")
                            .about(None)
                            .item(&check_updates_item)
                            .separator()
                            .services()
                            .separator()
                            .hide()
                            .hide_others()
                            .show_all()
                            .separator()
                            .quit()
                            .build()?;
                        let edit_submenu = SubmenuBuilder::new(app, "Edit")
                            .undo()
                            .redo()
                            .separator()
                            .cut()
                            .copy()
                            .paste()
                            .select_all()
                            .build()?;
                        let window_submenu = SubmenuBuilder::new(app, "Window")
                            .minimize()
                            .separator()
                            .close_window()
                            .build()?;
                        let menu = MenuBuilder::new(app)
                            .item(&app_submenu)
                            .item(&edit_submenu)
                            .item(&window_submenu)
                            .build()?;
                        app.set_menu(menu)?;
                        Ok(())
                    })();
                }
                #[cfg(not(target_os = "macos"))]
                {
                    if let Ok(menu) = Menu::default(app.handle()) {
                        let _ = app.set_menu(menu);
                    }
                }

                // Emit Tauri event when "Check for Updates…" menu item is clicked
                app.on_menu_event(|app, event| {
                    if event.id().0 == "check_for_updates" {
                        use tauri::Emitter;
                        let _ = app.emit("menu:check-for-updates", ());
                    }
                });

                apply_window_effects_to_window(&window, visual_effects_enabled, theme.as_deref());
            }

            // Watch system prompt file
            let app_handle = app.handle().clone();
            thread::spawn(move || {
                let path = get_system_prompt_path();
                let mut last_mtime = fs::metadata(&path).and_then(|m| m.modified()).ok();
                
                loop {
                    thread::sleep(Duration::from_secs(2));
                    let current_mtime = fs::metadata(&path).and_then(|m| m.modified()).ok();
                    if current_mtime != last_mtime {
                        last_mtime = current_mtime;
                        // Emit event
                        use tauri::Emitter;
                        let _ = app_handle.emit("system-prompt-changed", ());
                    }
                }
            });

            Ok(())
        })
        .manage(tracker_state)
        .manage(db_state)
        .manage(settings_state)
        .manage(ws_state);


    #[cfg(debug_assertions)]
    {
        builder = builder.plugin(devtools);
    }

    #[cfg(not(debug_assertions))]
    {
        builder = builder.plugin(tauri_plugin_log::Builder::default().build());
    }

    builder
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec![])))
        .plugin(tauri_plugin_global_shortcut::Builder::new().with_handler(move |app, _shortcut, event| {
             // Handle global shortcut (toggle window)
             use tauri_plugin_global_shortcut::ShortcutState;
             use tauri::Manager; // For get_webview_window

             if event.state == ShortcutState::Pressed {
                 if let Some(window) = app.get_webview_window("main") {
                        let is_shown = window.is_visible().unwrap_or(false)
                            && window.is_focused().unwrap_or(false)
                            && !window.is_minimized().unwrap_or(false);
                        if is_shown {
                            // On macOS, minimize instead of hide to stay in Cmd+Tab and dock
                            #[cfg(target_os = "macos")]
                            {
                                let _ = window.minimize();
                            }
                            #[cfg(not(target_os = "macos"))]
                            {
                                let _ = window.hide();
                            }
                        } else {
                            // Restore: unminimize on macOS, show on all platforms
                            #[cfg(target_os = "macos")]
                            {
                                let _ = window.unminimize();
                            }
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                 }
             }
        }).build())
        .invoke_handler(tauri::generate_handler![
            utils::get_previous_app_info,
            utils::get_smart_transfer_target,
            stashes::save_stash,
            stashes::save_stashes,
            transfer::export_context_archive,
            transfer::read_import_archive,
            transfer::commit_import,
            transfer::discard_import,
            stashes::load_stashes,
            stashes::load_stashes_for_sync,
            stashes::get_contexts_for_sync,
            stashes::import_stashes,
            stashes::claim_pending_stashes,
            stashes::claim_pending_positions,
            stashes::mark_positions_synced,
            stashes::import_positions,
            stashes::mark_stashes_synced,
            stashes::delete_stash,
            stashes::delete_completed_stashes,
            stashes::trigger_auto_cleanup,
            stashes::save_asset,
            stashes::save_asset_from_path,
            stashes::delete_asset,
            stashes::read_file_for_preview,
            utils::show_in_folder,
            utils::copy_to_clipboard,
            utils::read_clipboard_text,
            utils::start_drag,
            utils::get_device_name,
            settings::get_settings,
            settings::save_settings,
            settings::cloud_logout,
            utils::is_windows_10,
            contexts::get_contexts,
            contexts::save_contexts,
            contexts::save_context,
            contexts::import_contexts,
            contexts::claim_pending_contexts,
            contexts::mark_contexts_synced,
            contexts::delete_context,
            utils::set_autostart,
            utils::get_autostart_enabled,
            sync::fetch_cloud_account,
            sync::fetch_cloud_usage,
            utils::check_screen_recording_permission,
            utils::open_macos_screen_recording_settings,
            utils::check_apple_intelligence_available,
            utils::apple_intelligence_enhance,
            utils::get_system_prompt,
            utils::get_system_prompt_path_str,
            utils::check_system_prompt_exists,
            utils::create_system_prompt_file,
            utils::open_system_prompt_file,
            sync::exchange_link_code_api,
            sync::sync_stashes_api,
            sync::sync_contexts_api,
            sync::upload_attachment_to_cloud,
            sync::download_attachment_from_cloud,
            sync::connect_websocket,
            sync::disconnect_websocket,
            utils::get_installation_source,
            utils::log_frontend_error
        ])
        .plugin(tauri_plugin_deep_link::init())
        .setup(|_app| {
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            match event {
                tauri::RunEvent::Exit => {
                    println!("App exiting, cleaning up...");
                    cleanup_websocket_state(app_handle);
                    cleanup_database_state(app_handle);
                }
                _ => {}
            }
        });
}

fn cleanup_websocket_state<R: tauri::Runtime>(app_handle: &tauri::AppHandle<R>) {
    use tauri::Manager;
    if let Some(ws_arc_state) = app_handle.try_state::<Arc<WsState>>() {
        let ws_arc: &Arc<WsState> = ws_arc_state.inner();
        if let Some(handle) = lock_or_recover(&ws_arc.task_handle).take() {
            handle.abort();
        }
    }
}

/// Checkpoint the WAL on the way out, but never at the cost of hanging the exit.
///
/// This runs on the main thread during `RunEvent::Exit`. A plain `.lock()` here blocks
/// until whatever worker holds the database is done - a large archive import or a
/// cleanup pass over thousands of stashes - so the window disappeared while the process
/// stayed alive, which is the "app won't close, needs killing" report.
///
/// Skipping the checkpoint is safe: the WAL is not lost, SQLite simply replays it the
/// next time the database is opened. A missed truncation is a slightly larger file, not
/// data loss, so a bounded wait is strictly better than an unbounded one.
fn cleanup_database_state<R: tauri::Runtime>(app_handle: &tauri::AppHandle<R>) {
    use std::sync::Arc;
    use tauri::Manager;

    /// Total time we are willing to spend waiting for the database on shutdown.
    const SHUTDOWN_LOCK_BUDGET: Duration = Duration::from_secs(2);
    const RETRY_DELAY: Duration = Duration::from_millis(25);

    let Some(db_state) = app_handle.try_state::<Arc<DbState>>() else {
        return;
    };
    let db_arc = &db_state.db;
    let deadline = std::time::Instant::now() + SHUTDOWN_LOCK_BUDGET;

    loop {
        match db_arc.try_lock() {
            Ok(db) => {
                let _: rusqlite::Result<()> = db.prepare_shutdown();
                println!("DB shutdown successful (WAL checkpointed).");
                return;
            }
            Err(std::sync::TryLockError::Poisoned(poisoned)) => {
                // A panic already happened; still worth checkpointing.
                let _: rusqlite::Result<()> = poisoned.into_inner().prepare_shutdown();
                return;
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                if std::time::Instant::now() >= deadline {
                    log::warn!(
                        "Database still busy at exit; skipping WAL checkpoint so the \
                         process can terminate. The WAL replays on next launch."
                    );
                    return;
                }
                thread::sleep(RETRY_DELAY);
            }
        }
    }
}
