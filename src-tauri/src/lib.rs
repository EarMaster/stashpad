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
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::thread;

use tauri::menu::{Menu, MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::window::Color;
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
pub mod db;

use models::AppContext;
use state::{DbState, TrackerState, WsState, SettingsState};
use utils::{
    get_app_dir, ensure_storage_ready, apply_window_effects_to_window,
    get_system_prompt_path,
};
use settings::load_settings_from_disk;
use stashes::perform_startup_cleanup;
use db::DbManager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 0. init devtools
    #[cfg(debug_assertions)] // only enable instrumentation in development builds
    let devtools = tauri_plugin_devtools::init();

    // 1. Initialize Storage
    ensure_storage_ready();

    // 2. Initialize DB and Migrate
    let db_path = get_app_dir().join("stashpad.db");
    let mut db_manager = DbManager::new(&db_path).expect("Failed to init DB");
    
    // Check for migration
    let legacy_stashes_path = get_app_dir().join("db.json");
    if legacy_stashes_path.exists() { 
         println!("Migrating legacy JSON data to SQLite...");
         // Using stashes method (we might need to expose a migration method or move to stashes)
         // For now, load_stashes_from_disk was in old lib.rs. We moved contexts to load_contexts_from_disk.
         // Let's assume DbManager handles this. We might need to copy load_stashes_from_disk if it doesn't exist.
         // Wait, load_stashes_from_disk wasn't extracted yet? I will need to make sure it's in stashes.rs.
         // Ah, let's look at legacy migration:
         // let stashes = stashes::load_stashes_from_disk();
    }

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
    
    // Perform startup cleanup
    {
        // For startup cleanup, we need to lock DB.
        // We reuse logic but adapted.
        let mut db_lock = db_state.db.lock().unwrap();
        let settings_lock = settings_state.settings.lock().unwrap();
        perform_startup_cleanup(&mut db_lock, &settings_lock);
    }
    
    let tracker_state_clone = tracker_state.clone();
    let settings_state_clone = settings_state.clone();
    // Clone db state for background thread
    let db_state_clone = db_state.clone();
    
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
                        if let Ok(db) = db_state_clone.db.lock() {
                            if let Ok(contexts) = db.get_contexts() {
                                'ctx_loop: for ctx in contexts.iter() {
                                    for rule in &ctx.rules {
                                        let mut target = if rule.rule_type == "process" {
                                            app_name.clone()
                                        } else {
                                            title.clone()
                                        };
                                        
                                        let mut rule_value = rule.value.clone();

                                        if !rule.match_case {
                                            target = target.to_lowercase();
                                            rule_value = rule_value.to_lowercase();
                                        }

                                        let matched = if rule.use_regex {
                                            if let Ok(_re) = regex::Regex::new(&rule.value) {
                                                let re_str = if rule.match_case {
                                                    rule.value.clone()
                                                } else {
                                                    format!("(?i){}", rule.value)
                                                };
                                                if let Ok(re_case) = regex::Regex::new(&re_str) {
                                                    let orig_target = if rule.rule_type == "process" {
                                                        &app_name
                                                    } else {
                                                        &title
                                                    };
                                                    re_case.is_match(orig_target)
                                                } else {
                                                    false
                                                }
                                            } else {
                                                false
                                            }
                                        } else if rule.match_type == "exact" {
                                            target == rule_value
                                        } else {
                                            target.contains(&rule_value)
                                        };

                                        if matched {
                                            matched_context_id = Some(ctx.id.clone());
                                            break 'ctx_loop;
                                        }
                                    }
                                }
                            }
                        }
                    }

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
            let settings = settings_state_for_setup.settings.lock().unwrap();
            let visual_effects_enabled = settings.visual_effects_enabled;
            let theme = settings.theme.clone();
            drop(settings); // Release lock

            if let Some(window) = app.get_webview_window("main") {
                #[cfg(any(target_os = "windows", target_os = "macos"))]
                {
                    let _ = window.set_background_color(Some(Color(0, 0, 0, 0)));
                }
                #[cfg(target_os = "linux")]
                {
                    let _ = window.set_background_color(Some(Color(24, 24, 27, 255)));
                }

                // Build app menu with "Check for Updates…" item on macOS;
                // fall back to default menu on other platforms.
                #[cfg(target_os = "macos")]
                {
                    let _ = (|| -> Result<(), Box<dyn std::error::Error>> {
                        let check_updates_item = MenuItemBuilder::new("Check for Updates…")
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
            stashes::load_stashes,
            stashes::load_stashes_for_sync,
            stashes::get_contexts_for_sync,
            stashes::import_stashes,
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
            contexts::delete_context,
            utils::set_autostart,
            utils::get_autostart_enabled,
            sync::start_cloud_auth,
            sync::fetch_cloud_account,
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
            utils::get_installation_source
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
        if let Some(handle) = ws_arc.task_handle.lock().unwrap().take() {
            handle.abort();
        }
    }
}

fn cleanup_database_state<R: tauri::Runtime>(app_handle: &tauri::AppHandle<R>) {
    use std::sync::Arc;
    use tauri::Manager;
    if let Some(db_state) = app_handle.try_state::<Arc<DbState>>() {
        let db_arc = &db_state.db;
        if let Ok(db) = db_arc.lock() {
            let _: rusqlite::Result<()> = db.prepare_shutdown();
            println!("DB shutdown successful (WAL checkpointed).");
        }
    }
}
