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
use std::sync::Arc;
use tauri::{State, Manager};
use crate::models::{Settings, CloudConfig, default_cloud_endpoint};
use crate::utils::get_app_dir;
use crate::keychain::{
    get_api_key_from_keychain, get_cloud_token_from_keychain, 
    store_api_key_in_keychain, store_cloud_token_in_keychain, 
    encrypt_api_key, decrypt_api_key
};
use crate::state::SettingsState;

pub fn get_settings_path() -> PathBuf {
    get_app_dir().join("settings.json")
}

pub fn load_settings_from_disk() -> Settings {
    let path = get_settings_path();
    if path.exists() {
        if let Ok(file) = fs::File::open(path) {
            if let Ok(mut settings) = serde_json::from_reader::<_, Settings>(file) {
                // Try to get API key - keychain first, then JSON fallback
                if let Some(ref mut ai_config) = settings.ai_config {
                    if let Some(keychain_key) = get_api_key_from_keychain() {
                        ai_config.api_key = keychain_key;
                    } else if !ai_config.api_key.is_empty() {
                        // Fallback: decrypt from JSON
                        ai_config.api_key = decrypt_api_key(&ai_config.api_key);
                    }
                }
                // Try to get cloud access token - keychain first, then JSON fallback
                if let Some(ref mut cloud_config) = settings.cloud_config {
                    if let Some(keychain_token) = get_cloud_token_from_keychain() {
                        cloud_config.access_token = Some(keychain_token);
                    } else if let Some(ref token) = cloud_config.access_token {
                        if !token.is_empty() {
                            cloud_config.access_token = Some(decrypt_api_key(token));
                        }
                    }
                }
                // Validate and sanitize settings before returning
                return validate_settings(settings);
            }
        }
    }
    Settings::default()
}

/// Validates settings and falls back to defaults for any invalid values.
/// This ensures robustness against manual edits or corruption of settings.json.
pub fn validate_settings(mut settings: Settings) -> Settings {
    let defaults = Settings::default();
    
    // Validate new_stash_position: must be "top" or "bottom"
    if settings.new_stash_position != "top" && settings.new_stash_position != "bottom" {
        println!(
            "Warning: Invalid new_stash_position '{}', defaulting to '{}'",
            settings.new_stash_position, defaults.new_stash_position
        );
        settings.new_stash_position = defaults.new_stash_position.clone();
    }
    
    // Validate clear_completed_strategy: must be "never", "on-close", or "after-n-days"
    let valid_strategies = ["never", "on-close", "after-n-days"];
    if !valid_strategies.contains(&settings.clear_completed_strategy.as_str()) {
        println!(
            "Warning: Invalid clear_completed_strategy '{}', defaulting to '{}'",
            settings.clear_completed_strategy, defaults.clear_completed_strategy
        );
        settings.clear_completed_strategy = defaults.clear_completed_strategy.clone();
    }
    
    // Validate theme: must be "light", "dark", "system", or None
    if let Some(ref theme) = settings.theme {
        if !["light", "dark", "system"].contains(&theme.as_str()) {
            println!(
                "Warning: Invalid theme '{}', defaulting to None (system)",
                theme
            );
            settings.theme = None;
        }
    }
    
    // Validate clear_completed_days: must be at least 1 if strategy is after-n-days
    if settings.clear_completed_strategy == "after-n-days" && settings.clear_completed_days == 0 {
        println!(
            "Warning: clear_completed_days is 0 with after-n-days strategy, defaulting to {}",
            defaults.clear_completed_days
        );
        settings.clear_completed_days = defaults.clear_completed_days;
    }
    
    // Validate paste_as_attachment_threshold: 0 is valid (ask user), but cap at reasonable max
    if settings.paste_as_attachment_threshold > 1000 {
        println!(
            "Warning: paste_as_attachment_threshold {} is too high, defaulting to {}",
            settings.paste_as_attachment_threshold, defaults.paste_as_attachment_threshold
        );
        settings.paste_as_attachment_threshold = defaults.paste_as_attachment_threshold;
    }
    
    // Ensure cloud_config exists with default endpoint if not present
    if settings.cloud_config.is_none() {
        settings.cloud_config = Some(CloudConfig {
            enabled: false,
            endpoint: default_cloud_endpoint(),
            user_id: None,
            email: None,
            access_token: None,
            subscription_tier: None,
            subscription_status: None,
            subscription_period_end: None,
            enterprise_owner_id: None,
            last_sync_at: None,
        });
    }
    
    settings
}

pub fn persist_settings_to_disk(settings: &Settings) {
    let path = get_settings_path();
    let mut settings_to_save = settings.clone();
    
    // Handle API key storage - try keychain first, fallback to encryption
    if let Some(ref mut ai_config) = settings_to_save.ai_config {
        let api_key = ai_config.api_key.clone();
        
        if !api_key.is_empty() {
            if store_api_key_in_keychain(&api_key) {
                // Keychain success - store empty in JSON
                ai_config.api_key = String::new();
            } else {
                // Keychain failed - use encrypted JSON storage
                log::info!("Keychain unavailable for API key, using encrypted JSON");
                ai_config.api_key = encrypt_api_key(&api_key);
            }
        }
    }

    // Handle cloud access token storage - try keychain first, fallback to encryption
    if let Some(ref mut cloud_config) = settings_to_save.cloud_config {
        if let Some(ref token) = cloud_config.access_token {
            if !token.is_empty() {
                let token_clone = token.clone();
                if store_cloud_token_in_keychain(&token_clone) {
                    // Keychain success - store empty in JSON
                    cloud_config.access_token = Some(String::new());
                } else {
                    // Keychain failed - use encrypted JSON storage
                    log::info!("Keychain unavailable for cloud token, using encrypted JSON");
                    cloud_config.access_token = Some(encrypt_api_key(&token_clone));
                }
            }
        }
    }
    
    if let Ok(file) = fs::File::create(path) {
        let _ = serde_json::to_writer_pretty(file, &settings_to_save);
    }
}

#[tauri::command]
pub async fn get_settings(state: State<'_, Arc<SettingsState>>) -> Result<Settings, String> {
    let mut settings = state.settings.lock().unwrap().clone();
    if let Some(ref mut cloud_config) = settings.cloud_config {
        cloud_config.access_token = None;
    }
    Ok(settings)
}

#[tauri::command]
pub async fn save_settings(app: tauri::AppHandle, state: State<'_, Arc<SettingsState>>, mut settings: Settings) -> Result<(), String> {
    let old_theme = {
        let current = state.settings.lock().unwrap();
        current.theme.clone()
    };
    
    let old_autostart = {
        let current = state.settings.lock().unwrap();
        current.autostart
    };
    
    // Don't save empty api keys if we already have one
    if let Some(ref mut new_ai_config) = settings.ai_config {
        let current = state.settings.lock().unwrap();
        if let Some(ref current_ai_config) = current.ai_config {
            if new_ai_config.api_key.is_empty() && !current_ai_config.api_key.is_empty() {
                new_ai_config.api_key = current_ai_config.api_key.clone();
            }
        }
    }
    
    // Verify changes to cloud access token
    if let Some(ref mut new_cloud_config) = settings.cloud_config {
        let current = state.settings.lock().unwrap();
        if let Some(ref current_cloud_config) = current.cloud_config {
            if new_cloud_config.access_token.is_none() && current_cloud_config.access_token.is_some() {
                new_cloud_config.access_token = current_cloud_config.access_token.clone();
            }
        }
    }

    persist_settings_to_disk(&settings);
    let mut state_settings = state.settings.lock().unwrap();
    *state_settings = settings.clone();

    // Reapply theme and window effects if changed
    if settings.theme != old_theme {
        if let Some(window) = app.get_webview_window("main") {
            // Re-apply effects which depend on theme (e.g. Acrylic color)
            crate::utils::apply_window_effects_to_window(&window, settings.visual_effects_enabled, settings.theme.as_deref());
        }
    }

    // Toggle autostart
    if settings.autostart != old_autostart {
        use tauri_plugin_autostart::ManagerExt;
        let autostart_manager = app.autolaunch();
        if settings.autostart {
            let _ = autostart_manager.enable();
        } else {
            let _ = autostart_manager.disable();
        }
    }
    Ok(())
}

/// Sign out of the cloud and destroy the stored credential.
///
/// `save_settings` deliberately restores a missing `access_token` so a frontend save
/// cannot wipe it (the token is never exposed to the webview). That protection also
/// meant clearing the fields in the UI left the JWT alive in the keychain forever, so
/// logout needs its own command that erases it explicitly.
#[tauri::command]
pub async fn cloud_logout(state: State<'_, Arc<SettingsState>>) -> Result<(), String> {
    let mut settings = state.settings.lock().unwrap();

    if let Some(ref mut config) = settings.cloud_config {
        config.access_token = None;
        config.user_id = None;
        config.email = None;
        config.subscription_tier = None;
        config.subscription_status = None;
        config.subscription_period_end = None;
        config.enterprise_owner_id = None;
        config.last_sync_at = None;
        config.enabled = false;
    }

    crate::keychain::delete_cloud_token_from_keychain();
    persist_settings_to_disk(&settings);
    Ok(())
}
