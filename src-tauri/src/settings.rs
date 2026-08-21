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
use tauri::{State, Manager};
use crate::models::{Settings, CloudConfig, default_cloud_endpoint};
use crate::utils::get_app_dir;
use crate::keychain::{
    get_api_key_from_keychain, get_cloud_token_from_keychain, 
    store_api_key_in_keychain, store_cloud_token_in_keychain, 
    encrypt_api_key, decrypt_api_key
};
use crate::state::{SettingsState, lock_or_recover};

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
    
    // Discard update timestamps that sit implausibly far in the future. A clock that
    // jumped forward once would otherwise suppress every later update check - the 48h
    // deadline and the "remind me later" deadline would both never be reached again.
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let horizon = now_ms.saturating_add(24 * 60 * 60 * 1000);
    if settings.last_update_check_at.is_some_and(|t| t > horizon) {
        println!("Warning: last_update_check_at is in the future, resetting");
        settings.last_update_check_at = None;
    }
    if settings.update_remind_after.is_some_and(|t| t > horizon + 7 * 24 * 60 * 60 * 1000) {
        println!("Warning: update_remind_after is implausibly far ahead, resetting");
        settings.update_remind_after = None;
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

/// How a secret was last persisted, so an unchanged one is never rewritten.
#[derive(Clone, PartialEq)]
struct StoredSecret {
    value: String,
    /// `true` when the keychain took it, `false` when we fell back to encrypted JSON.
    in_keychain: bool,
}

/// What the last successful `persist_settings_to_disk` wrote, per slot.
///
/// Writing the settings file is cheap; getting a secret into the OS credential store
/// is not. Every successful sync stamps `lastSyncAt` and saves settings, and sync runs
/// a couple of seconds after every local edit - so without this cache, simply typing a
/// stash drove a credential-store write for the API key *and* the cloud token, on a
/// Tokio worker, indefinitely. That is why "the app freezes" looked sync-related: the
/// credential store, not the network, was the bottleneck.
static LAST_API_KEY: Mutex<Option<StoredSecret>> = Mutex::new(None);
static LAST_CLOUD_TOKEN: Mutex<Option<StoredSecret>> = Mutex::new(None);

/// Persist one secret, skipping the credential store when the value has not changed.
///
/// Returns what belongs in `settings.json` for this slot: an empty string when the
/// keychain holds the secret, or the encrypted value when it does not.
fn persist_secret(
    cache: &Mutex<Option<StoredSecret>>,
    store: fn(&str) -> bool,
    secret: &str,
    label: &str,
) -> String {
    {
        let cached = lock_or_recover(cache);
        if let Some(previous) = cached.as_ref() {
            if previous.value == secret {
                // Already where it needs to be - no credential-store round trip.
                return if previous.in_keychain {
                    String::new()
                } else {
                    encrypt_api_key(secret)
                };
            }
        }
    }

    let in_keychain = store(secret);
    if !in_keychain {
        log::info!("Keychain unavailable for {}, using encrypted JSON", label);
    }
    *lock_or_recover(cache) = Some(StoredSecret {
        value: secret.to_string(),
        in_keychain,
    });

    if in_keychain {
        String::new()
    } else {
        encrypt_api_key(secret)
    }
}

/// Forget the cached secrets, so the next save writes them again.
///
/// Needed after an explicit logout: the credential is deleted behind the cache's back,
/// and a later save of the same token must not be skipped as "unchanged".
pub fn invalidate_secret_cache() {
    *lock_or_recover(&LAST_API_KEY) = None;
    *lock_or_recover(&LAST_CLOUD_TOKEN) = None;
}

/// Write `settings.json`, moving any secrets into the keychain first.
///
/// Blocking: touches the OS credential store and the filesystem. Callers running on
/// the async runtime must go through [`persist_settings_off_thread`] instead, or they
/// park a Tokio worker.
pub fn persist_settings_to_disk(settings: &Settings) {
    let path = get_settings_path();
    let mut settings_to_save = settings.clone();

    // Handle API key storage - try keychain first, fallback to encryption
    if let Some(ref mut ai_config) = settings_to_save.ai_config {
        let api_key = ai_config.api_key.clone();

        if !api_key.is_empty() {
            ai_config.api_key =
                persist_secret(&LAST_API_KEY, store_api_key_in_keychain, &api_key, "API key");
        }
    }

    // Handle cloud access token storage - try keychain first, fallback to encryption
    if let Some(ref mut cloud_config) = settings_to_save.cloud_config {
        if let Some(ref token) = cloud_config.access_token {
            if !token.is_empty() {
                cloud_config.access_token = Some(persist_secret(
                    &LAST_CLOUD_TOKEN,
                    store_cloud_token_in_keychain,
                    token,
                    "cloud token",
                ));
            }
        }
    }

    write_settings_file(&path, &settings_to_save);
}

/// Serialize the settings to `path` via a temporary file and a rename.
///
/// The previous `File::create` truncated the real file before writing a byte, so an
/// interrupted write left a half-written or empty `settings.json`. Losing it costs the
/// cloud config and `lastSyncAt`, and a missing `lastSyncAt` makes the next sync push
/// the entire dataset. A rename is atomic, so a reader sees either the old file or the
/// new one and never a partial one.
fn write_settings_file(path: &PathBuf, settings: &Settings) {
    let temp = path.with_extension("json.tmp");

    match fs::File::create(&temp) {
        Ok(file) => {
            if let Err(e) = serde_json::to_writer_pretty(file, settings) {
                log::error!("Failed to write settings: {}", e);
                let _ = fs::remove_file(&temp);
                return;
            }
        }
        Err(e) => {
            log::error!("Failed to create temporary settings file: {}", e);
            return;
        }
    }

    if let Err(e) = fs::rename(&temp, path) {
        log::error!("Failed to replace settings file: {}", e);
        let _ = fs::remove_file(&temp);
    }
}

/// Persist settings without blocking the async runtime.
///
/// The credential store can take a noticeable moment, and Tokio does not move a task
/// that blocks its worker - so doing this inline on an async command starved the pool
/// and left every `invoke` in the app unanswered while the window kept painting.
pub async fn persist_settings_off_thread(settings: Settings) {
    if let Err(e) =
        tauri::async_runtime::spawn_blocking(move || persist_settings_to_disk(&settings)).await
    {
        log::error!("Settings persistence task failed: {}", e);
    }
}

#[tauri::command]
pub async fn get_settings(state: State<'_, Arc<SettingsState>>) -> Result<Settings, String> {
    let mut settings = state.lock_settings().clone();
    if let Some(ref mut cloud_config) = settings.cloud_config {
        cloud_config.access_token = None;
    }
    Ok(settings)
}

#[tauri::command]
pub async fn save_settings(app: tauri::AppHandle, state: State<'_, Arc<SettingsState>>, mut settings: Settings) -> Result<(), String> {
    // One critical section, not five. Each `lock_settings()` is a blocking acquire on
    // an async worker, and this command runs on every keystroke in the settings panel;
    // taking the lock five times per call multiplied that contention for no reason.
    let (old_theme, old_autostart) = {
        let current = state.lock_settings();

        // Don't save empty api keys if we already have one
        if let Some(ref mut new_ai_config) = settings.ai_config {
            if let Some(ref current_ai_config) = current.ai_config {
                if new_ai_config.api_key.is_empty() && !current_ai_config.api_key.is_empty() {
                    new_ai_config.api_key = current_ai_config.api_key.clone();
                }
            }
        }

        // Verify changes to cloud access token
        if let Some(ref mut new_cloud_config) = settings.cloud_config {
            if let Some(ref current_cloud_config) = current.cloud_config {
                if new_cloud_config.access_token.is_none()
                    && current_cloud_config.access_token.is_some()
                {
                    new_cloud_config.access_token = current_cloud_config.access_token.clone();
                }
            }
        }

        (current.theme.clone(), current.autostart)
    };

    // Publish to the in-memory state first, then write to disk off-thread. Callers only
    // need the new values to be live; waiting on the credential store and the
    // filesystem before returning is what used to stall the whole runtime.
    {
        let mut state_settings = state.lock_settings();
        *state_settings = settings.clone();
    }
    persist_settings_off_thread(settings.clone()).await;

    // Reapply theme and window effects if changed
    if settings.theme != old_theme {
        if let Some(window) = app.get_webview_window("main") {
            // Re-apply effects which depend on theme (e.g. Acrylic color)
            crate::utils::apply_window_effects_to_window(&window, settings.visual_effects_enabled, settings.theme.as_deref());
        }
    }

    // Toggle autostart. Registry work, so it goes off-thread too.
    if settings.autostart != old_autostart {
        let enable = settings.autostart;
        let app_handle = app.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || {
            use tauri_plugin_autostart::ManagerExt;
            let autostart_manager = app_handle.autolaunch();
            let result = if enable {
                autostart_manager.enable()
            } else {
                autostart_manager.disable()
            };
            if let Err(e) = result {
                log::warn!("Failed to change autostart: {}", e);
            }
        })
        .await;
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
    // Clear under the lock, then release it before touching the credential store. The
    // lock used to be held across a blocking delete and a full settings write, so every
    // other command that reads settings blocked its own worker waiting on it.
    let cleared = {
        let mut settings = state.lock_settings();

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

        settings.clone()
    };

    let _ = tauri::async_runtime::spawn_blocking(|| {
        crate::keychain::delete_cloud_token_from_keychain();
        // The credential is gone behind the cache's back, so drop what it remembers or
        // a later save of the same token would be skipped as unchanged.
        invalidate_secret_cache();
    })
    .await;

    persist_settings_off_thread(cleared).await;
    Ok(())
}
