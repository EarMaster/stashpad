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
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{Manager, State};
use rusqlite::params;
use rusqlite::OptionalExtension;
use tauri::window::Color;
use tauri::menu::Menu;
use tiny_http::{Server, Response};
use url::Url;

mod db;
use db::DbManager;

// Window vibrancy effects (Windows and macOS only)
#[cfg(target_os = "windows")]
use window_vibrancy::{apply_acrylic, apply_mica, clear_acrylic, clear_mica};
#[cfg(target_os = "macos")]
use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial};

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub id: String,
    pub stash_id: String,
    pub file_path: String,
    pub file_name: String,
    pub file_size: i64,
    pub mime_type: Option<String>,
    pub syntax: Option<String>,
    pub created_at: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StashItem {
    pub id: String,
    pub content: String,
    /// AI-enhanced version of the content (if generated)
    #[serde(default)]
    pub enhanced_content: Option<String>,
    #[serde(default)]
    pub files: Vec<String>, // Deprecated, kept for backward compatibility/migration
    #[serde(default)]
    pub attachments: Vec<Attachment>,
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

pub struct DbState {
    pub db: Arc<Mutex<DbManager>>,
}

struct WsState {
    /// Handle to the background task that manages the WebSocket connection
    task_handle: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

impl TrackerState {
    fn new() -> Self {
        Self {
            last_external_app: None,
            current_context_id: None,
        }
    }
}

// Duplicate impl removed

// --- Persistence Helpers ---

fn get_app_dir() -> PathBuf {
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

fn get_system_prompt_path() -> PathBuf {
    get_app_dir().join("enhancement_prompt.txt")
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

    // Ensure default system prompt file exists
    let prompt_path = get_system_prompt_path();
    if !prompt_path.exists() {
        let _ = fs::write(prompt_path, DEFAULT_SYSTEM_PROMPT);
    }
}

#[allow(dead_code)]
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
    #[serde(default)]
    pub match_case: bool,
    #[serde(default)]
    pub use_regex: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Context {
    pub id: String,
    pub name: String,
    /// Optional description for AI context (tech stack, project info)
    #[serde(default)]
    pub description: Option<String>,
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
    #[serde(default = "default_strip_tags_on_copy")]
    pub strip_tags_on_copy: bool, // Strip #tags when copying to clipboard
    #[serde(default = "default_clear_completed_strategy")]
    pub clear_completed_strategy: String,
    #[serde(default = "default_clear_completed_days")]
    pub clear_completed_days: u32,
    /// Number of lines of pasted text before it becomes an attachment. 0 = ask user, default 8
    #[serde(default = "default_paste_as_attachment_threshold")]
    pub paste_as_attachment_threshold: u32,
    /// Last used timestamp for the default context
    #[serde(default)]
    pub default_context_last_used: Option<String>,
    /// Launch Stashpad automatically on system startup
    #[serde(default)]
    pub autostart: bool,
    /// AI enhancement configuration
    #[serde(default)]
    pub ai_config: Option<AiConfig>,
    /// Cloud sync configuration
    #[serde(default)]
    pub cloud_config: Option<CloudConfig>,
}

/// Cloud sync configuration
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CloudConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_cloud_endpoint")]
    pub endpoint: String,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    /// Access token (stored in JSON for now, should ideally be in keychain)
    #[serde(default)]
    pub access_token: Option<String>,
    /// Subscription tier: 'free', 'pro', 'enterprise'
    #[serde(default)]
    pub subscription_tier: Option<String>,
    /// Subscription status: 'active', 'canceled', etc.
    #[serde(default)]
    pub subscription_status: Option<String>,
    /// When the current billing period ends
    #[serde(default)]
    pub subscription_period_end: Option<String>,
    /// Last sync timestamp
    #[serde(default)]
    pub last_sync_at: Option<String>,
}

fn default_cloud_endpoint() -> String {
    "https://stashpad.org/api".to_string()
}

/// AI provider configuration for prompt enhancement
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct AiConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub endpoint: String,
    /// API key - stored obfuscated (not encrypted, just not plaintext)
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub preset_id: Option<String>,
}

fn default_clear_completed_strategy() -> String {
    "never".to_string()
}

fn default_clear_completed_days() -> u32 {
    7
}

fn default_paste_as_attachment_threshold() -> u32 {
    8
}

fn default_new_stash_position() -> String {
    "top".to_string()
}

fn default_strip_tags_on_copy() -> bool {
    true
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
            strip_tags_on_copy: true,
            clear_completed_strategy: default_clear_completed_strategy(),
            clear_completed_days: default_clear_completed_days(),
            paste_as_attachment_threshold: default_paste_as_attachment_threshold(),
            default_context_last_used: None,
            autostart: false,
            ai_config: None,
            cloud_config: None,
        }
    }
}

/// Simple obfuscation key for API key storage (fallback)
const OBFUSCATION_KEY: &[u8] = b"StashpadAIConfigKey2026";

/// Keychain identifiers - using explicit target for Windows compatibility
const KEYCHAIN_SERVICE: &str = "stashpad";
const KEYCHAIN_USER: &str = "ai_api_key";
const KEYCHAIN_TARGET: &str = "stashpad.ai_api_key";
/// Keychain identifiers for cloud access token
const KEYCHAIN_CLOUD_USER: &str = "cloud_access_token";
const KEYCHAIN_CLOUD_TARGET: &str = "stashpad.cloud_access_token";

/// Create a keychain entry with consistent target across platforms
fn create_keychain_entry() -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new_with_target(KEYCHAIN_TARGET, KEYCHAIN_SERVICE, KEYCHAIN_USER)
}

/// Create a keychain entry for the cloud access token
fn create_cloud_keychain_entry() -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new_with_target(KEYCHAIN_CLOUD_TARGET, KEYCHAIN_SERVICE, KEYCHAIN_CLOUD_USER)
}

/// Store a secret in the system keychain and verify it can be retrieved.
/// Generic helper used for both AI API key and cloud access token.
fn store_secret_in_keychain(
    create_entry: fn() -> Result<keyring::Entry, keyring::Error>,
    delete_fn: fn(),
    secret: &str,
) -> bool {
    if secret.is_empty() {
        delete_fn();
        return true;
    }
    match create_entry() {
        Ok(entry) => {
            match entry.set_password(secret) {
                Ok(_) => {
                    // Verify we can actually retrieve it
                    match create_entry() {
                        Ok(verify_entry) => {
                            match verify_entry.get_password() {
                                Ok(retrieved) if retrieved == secret => {
                                    log::debug!("Secret stored and verified in system keychain");
                                    true
                                }
                                Ok(_) => {
                                    log::warn!("Keychain verification failed: retrieved value doesn't match");
                                    false
                                }
                                Err(_) => {
                                    log::warn!("Keychain verification failed on retrieval");
                                    false
                                }
                            }
                        }
                        Err(_) => {
                            log::warn!("Keychain verification failed on entry creation");
                            false
                        }
                    }
                }
                Err(_) => {
                    log::warn!("Failed to store secret in keychain");
                    false
                }
            }
        }
        Err(_) => {
            log::warn!("Failed to create keychain entry");
            false
        }
    }
}

/// Store API key in system keychain and verify it can be retrieved
fn store_api_key_in_keychain(key: &str) -> bool {
    store_secret_in_keychain(create_keychain_entry, delete_api_key_from_keychain, key)
}

/// Store cloud access token in system keychain
fn store_cloud_token_in_keychain(token: &str) -> bool {
    store_secret_in_keychain(create_cloud_keychain_entry, delete_cloud_token_from_keychain, token)
}

/// Retrieve a secret from the system keychain.
/// Generic helper used for both AI API key and cloud access token.
fn get_secret_from_keychain(
    create_entry: fn() -> Result<keyring::Entry, keyring::Error>,
) -> Option<String> {
    match create_entry() {
        Ok(entry) => {
            match entry.get_password() {
                Ok(password) => Some(password),
                Err(_) => None
            }
        }
        Err(_) => None
    }
}

/// Retrieve API key from system keychain
fn get_api_key_from_keychain() -> Option<String> {
    get_secret_from_keychain(create_keychain_entry)
}

/// Retrieve cloud access token from system keychain
fn get_cloud_token_from_keychain() -> Option<String> {
    get_secret_from_keychain(create_cloud_keychain_entry)
}

/// Delete a secret from the keychain.
fn delete_secret_from_keychain(
    create_entry: fn() -> Result<keyring::Entry, keyring::Error>,
) {
    if let Ok(entry) = create_entry() {
        let _ = entry.delete_credential();
    }
}

/// Delete API key from keychain
fn delete_api_key_from_keychain() {
    delete_secret_from_keychain(create_keychain_entry);
}

/// Delete cloud access token from keychain
fn delete_cloud_token_from_keychain() {
    delete_secret_from_keychain(create_cloud_keychain_entry);
}

/// Derive a 256-bit key from machine-specific information
/// This makes the encrypted data machine-bound (can't be decrypted on another machine)
fn derive_machine_key() -> [u8; 32] {
    use sha2::{Sha256, Digest};
    
    let mut hasher = Sha256::new();
    
    // Add machine-specific data to the key derivation
    // This includes hostname and app directory path
    if let Ok(hostname) = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .or_else(|_| std::env::var("NAME"))
    {
        hasher.update(hostname.as_bytes());
    }
    
    // Add app directory path (unique per user/installation)
    hasher.update(get_app_dir().to_string_lossy().as_bytes());
    
    // Add a static salt
    hasher.update(b"StashpadAPIKeyEncryption2026");
    
    let result = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}

/// Encrypt a string using AES-256-GCM (fallback for when keychain unavailable)
fn encrypt_api_key(key: &str) -> String {
    if key.is_empty() {
        return String::new();
    }
    
    use aes_gcm::{
        aead::{Aead, KeyInit},
        Aes256Gcm, Nonce,
    };
    use rand::RngCore;
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    
    let encryption_key = derive_machine_key();
    let cipher = Aes256Gcm::new_from_slice(&encryption_key).expect("Invalid key length");
    
    // Generate random 12-byte nonce
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    
    match cipher.encrypt(nonce, key.as_bytes()) {
        Ok(ciphertext) => {
            // Prepend nonce to ciphertext
            let mut result = Vec::with_capacity(12 + ciphertext.len());
            result.extend_from_slice(&nonce_bytes);
            result.extend_from_slice(&ciphertext);
            STANDARD.encode(&result)
        }
        Err(_e) => {
            log::warn!("AES encryption failed, using XOR fallback");
            // Fallback to simple obfuscation if encryption fails
            obfuscate_simple(key)
        }
    }
}

/// Decrypt a string that was encrypted with encrypt_api_key
fn decrypt_api_key(encoded: &str) -> String {
    if encoded.is_empty() {
        return String::new();
    }
    
    use aes_gcm::{
        aead::{Aead, KeyInit},
        Aes256Gcm, Nonce,
    };
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    
    match STANDARD.decode(encoded) {
        Ok(data) => {
            if data.len() < 13 {
                // Too short to be valid (12 byte nonce + at least 1 byte)
                // Try legacy deobfuscation
                return deobfuscate_simple(encoded);
            }
            
            let (nonce_bytes, ciphertext) = data.split_at(12);
            let encryption_key = derive_machine_key();
            let cipher = Aes256Gcm::new_from_slice(&encryption_key).expect("Invalid key length");
            let nonce = Nonce::from_slice(nonce_bytes);
            
            match cipher.decrypt(nonce, ciphertext) {
                Ok(plaintext) => String::from_utf8(plaintext).unwrap_or_default(),
                Err(_) => {
                    // Decryption failed - might be old XOR obfuscated format
                    deobfuscate_simple(encoded)
                }
            }
        }
        Err(_) => {
            // Base64 decode failed - assume it's plaintext (migration case)
            encoded.to_string()
        }
    }
}

/// Simple XOR obfuscation (legacy fallback)
fn obfuscate_simple(key: &str) -> String {
    let bytes: Vec<u8> = key
        .bytes()
        .enumerate()
        .map(|(i, b)| b ^ OBFUSCATION_KEY[i % OBFUSCATION_KEY.len()])
        .collect();
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    STANDARD.encode(&bytes)
}

/// Simple XOR deobfuscation (legacy fallback)
fn deobfuscate_simple(encoded: &str) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    match STANDARD.decode(encoded) {
        Ok(bytes) => {
            let decoded: Vec<u8> = bytes
                .iter()
                .enumerate()
                .map(|(i, b)| b ^ OBFUSCATION_KEY[i % OBFUSCATION_KEY.len()])
                .collect();
            String::from_utf8(decoded).unwrap_or_default()
        }
        Err(_) => encoded.to_string()
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
fn validate_settings(mut settings: Settings) -> Settings {
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
            last_sync_at: None,
        });
    }
    
    settings
}

fn persist_settings_to_disk(settings: &Settings) {
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

// --- Contexts Storage (separate from settings) ---

#[allow(dead_code)]
struct ContextsState {
    contexts: Mutex<Vec<Context>>,
}

fn get_contexts_path() -> PathBuf {
    get_app_dir().join("contexts.json")
}

/// Loads contexts from disk.
/// On first run, migrates contexts from settings.json if present.
fn load_contexts_from_disk() -> Vec<Context> {
    let contexts_path = get_contexts_path();
    
    // Try to load from contexts.json first
    if contexts_path.exists() {
        if let Ok(file) = fs::File::open(&contexts_path) {
            if let Ok(contexts) = serde_json::from_reader(file) {
                return contexts;
            }
        }
    }
    
    // contexts.json doesn't exist or is invalid - try to migrate from settings.json
    let settings_path = get_settings_path();
    if settings_path.exists() {
        if let Ok(file) = fs::File::open(&settings_path) {
            // Parse settings as a raw JSON value to extract contexts
            if let Ok(value) = serde_json::from_reader::<_, serde_json::Value>(file) {
                if let Some(contexts_value) = value.get("contexts") {
                    if let Ok(contexts) = serde_json::from_value::<Vec<Context>>(contexts_value.clone()) {
                        if !contexts.is_empty() {
                            println!("Migrating {} contexts from settings.json to contexts.json", contexts.len());
                            // Persist to new location
                            persist_contexts_to_disk(&contexts);
                            // Remove contexts from settings.json
                            remove_contexts_from_settings();
                            return contexts;
                        }
                    }
                }
            }
        }
    }
    
    Vec::new() // Default empty
}

fn persist_contexts_to_disk(contexts: &Vec<Context>) {
    let path = get_contexts_path();
    if let Ok(file) = fs::File::create(path) {
        let _ = serde_json::to_writer_pretty(file, contexts);
    }
}

/// Removes the 'contexts' field from settings.json after migration.
/// This keeps settings.json clean and prevents duplicate data.
fn remove_contexts_from_settings() {
    let path = get_settings_path();
    if let Ok(file) = fs::File::open(&path) {
        if let Ok(mut value) = serde_json::from_reader::<_, serde_json::Value>(file) {
            if let Some(obj) = value.as_object_mut() {
                if obj.remove("contexts").is_some() {
                    if let Ok(file) = fs::File::create(&path) {
                        let _ = serde_json::to_writer_pretty(file, &value);
                        println!("Removed 'contexts' from settings.json after migration");
                    }
                }
            }
        }
    }
}

/// Apply window vibrancy effects based on the visual_effects_enabled setting and theme.
/// - enabled: None uses OS defaults, Some(true) enables effects, Some(false) disables
/// - theme: Optional theme for Acrylic color ("dark", "light", "system") - Windows 10 only
/// 
/// Platform support:
/// - Windows 11: Mica effect (theme handled by OS)
/// - Windows 10: Acrylic effect with theme-aware background color
/// - macOS: Vibrancy with HudWindow material
/// - Linux: No library support (compositor handles transparency)
fn apply_window_effects_to_window(window: &tauri::WebviewWindow, enabled: Option<bool>, _theme: Option<&str>) {
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

// --- Commands ---

#[tauri::command]
fn get_settings(state: State<Arc<SettingsState>>) -> Settings {
    let mut settings = state.settings.lock().unwrap().clone();
    if let Some(ref mut cloud_config) = settings.cloud_config {
        cloud_config.access_token = None;
    }
    settings
}

#[tauri::command]
fn save_settings(app: tauri::AppHandle, state: State<Arc<SettingsState>>, mut settings: Settings) {
    println!("Saving settings");

    // Preserve existing access token if the frontend didn't supply one
    {
        let current = state.settings.lock().unwrap();
        if let Some(ref mut new_cloud) = settings.cloud_config {
            if new_cloud.access_token.is_none() || new_cloud.access_token.as_ref().map(|t| t.is_empty()).unwrap_or(false) {
                if let Some(ref current_cloud) = current.cloud_config {
                    new_cloud.access_token = current_cloud.access_token.clone();
                }
            }
        }
    }

    // Update global shortcuts
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    
    let mut current = state.settings.lock().unwrap();
    
    // Check if visual effects setting or theme changed
    let effects_changed = current.visual_effects_enabled != settings.visual_effects_enabled;
    let theme_changed = current.theme != settings.theme;
    
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
    
    // Apply window effects if setting or theme changed
    if effects_changed || theme_changed {
        if let Some(window) = app.get_webview_window("main") {
            apply_window_effects_to_window(
                &window, 
                settings.visual_effects_enabled,
                settings.theme.as_deref()
            );
        }
    }
}

#[tauri::command]
async fn start_cloud_auth(
    app: tauri::AppHandle,
    settings_state: State<'_, Arc<SettingsState>>,
) -> Result<CloudConfig, String> {
    let (endpoint, _enabled) = {
        let settings = settings_state.settings.lock().unwrap();
        let config = settings.cloud_config.as_ref().ok_or("Cloud config missing")?;
        (config.endpoint.clone(), config.enabled)
    };

    // 1. Start local server on an ephemeral port to listen for callback
    let server = Server::http("127.0.0.1:0").map_err(|e| e.to_string())?;
    let callback_port = server.server_addr().to_ip().map(|a| a.port()).ok_or("Failed to get callback port")?;
    
    // 2. Generate a CSRF state parameter to verify the callback origin
    let state_param = uuid::Uuid::new_v4().to_string();
    
    // 3. Open browser to cloud auth with state parameter and callback port
    let auth_url = format!(
        "{}/auth/github?state={}&callback_port={}",
        endpoint.trim_end_matches('/'),
        state_param,
        callback_port
    );
    let _ = tauri_plugin_opener::OpenerExt::opener(&app).open_url(auth_url, None::<String>);

    // 3. Wait for request (with a timeout of 5 minutes)
    // In a real app, we'd use a thread or async task with a timeout.
    // For simplicity in this scaffold, we'll block briefly or just take the first request.
    
    if let Some(request) = server.recv_timeout(Duration::from_secs(300)).map_err(|e| e.to_string())? {
        let url_str = format!("http://localhost:{}{}", callback_port, request.url());
        let url = Url::parse(&url_str).map_err(|e| e.to_string())?;
        
        let mut token = None;
        let mut user_id = None;
        let mut email = None;
        let mut callback_state = None;
        
        for (key, value) in url.query_pairs() {
            match key.as_ref() {
                "token" => token = Some(value.into_owned()),
                "userId" => user_id = Some(value.into_owned()),
                "email" => email = Some(value.into_owned()),
                "state" => callback_state = Some(value.into_owned()),
                _ => {}
            }
        }
        
        // Verify the state parameter matches to prevent CSRF
        if callback_state.as_deref() != Some(&state_param) {
            let response = Response::from_string("Authentication failed: Invalid state parameter.")
                .with_status_code(400);
            let _ = request.respond(response);
            return Err("Authentication failed: State parameter mismatch (CSRF protection)".into());
        }
        
        if let (Some(t), Some(uid), Some(e)) = (token, user_id, email) {
            let mut settings = settings_state.settings.lock().unwrap();
            let mut config = settings.cloud_config.clone().unwrap_or(CloudConfig {
                enabled: false,
                endpoint: default_cloud_endpoint(),
                user_id: None,
                email: None,
                access_token: None,
                subscription_tier: None,
                subscription_status: None,
                subscription_period_end: None,
                last_sync_at: None,
            });
            
            config.access_token = Some(t);
            config.user_id = Some(uid);
            config.email = Some(e);
            config.enabled = true;
            
            settings.cloud_config = Some(config.clone());
            persist_settings_to_disk(&settings);
            
            let mut return_config = config.clone();
            return_config.access_token = None;
            Ok(return_config)
        } else {

            let response = Response::from_string("Authentication failed: Missing parameters.")
                .with_status_code(400);
            let _ = request.respond(response);
            Err("Authentication failed: Missing parameters".into())
        }
    } else {
        Err("Authentication timed out".into())
    }
}

/// Fetch account info from cloud service and update local subscription status
#[tauri::command]
async fn fetch_cloud_account(
    settings_state: State<'_, Arc<SettingsState>>,
) -> Result<CloudConfig, String> {
    let (endpoint, token) = {
        let settings = settings_state.settings.lock().unwrap();
        let config = settings.cloud_config.as_ref().ok_or("Cloud config missing")?;
        let token = config.access_token.clone().ok_or("Not authenticated")?;
        (config.endpoint.clone(), token)
    };

    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/account", endpoint.trim_end_matches('/')))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch account: {}", e))?;

    if response.status() == 401 {
        return Err("Authentication expired. Please log in again.".into());
    }

    if !response.status().is_success() {
        return Err(format!("Failed to fetch account: {}", response.status()));
    }

    let account: serde_json::Value = response.json().await
        .map_err(|e| format!("Failed to parse account: {}", e))?;

    // Update local config with subscription info
    let mut settings = settings_state.settings.lock().unwrap();
    if let Some(ref mut config) = settings.cloud_config {
        config.subscription_tier = account["subscriptionTier"].as_str().map(|s| s.to_string());
        config.subscription_status = account["subscriptionStatus"].as_str().map(|s| s.to_string());
        config.subscription_period_end = account["subscriptionPeriodEnd"].as_str().map(|s| s.to_string());
        
        let updated_config = config.clone();
        persist_settings_to_disk(&settings);
        
        let mut return_config = updated_config;
        return_config.access_token = None;
        return Ok(return_config);
    }

    Err("Cloud config not found".into())
}

#[tauri::command]
async fn exchange_link_code_api(
    settings_state: State<'_, Arc<SettingsState>>,
    token: String,
) -> Result<CloudConfig, String> {
    let endpoint = {
        let settings = settings_state.settings.lock().unwrap();
        let config = settings.cloud_config.as_ref().ok_or("Cloud config missing")?;
        config.endpoint.clone()
    };

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/auth/exchange-token", endpoint.trim_end_matches('/')))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({ "token": token }))
        .send()
        .await
        .map_err(|e| format!("Failed to exchange token: {}", e))?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_else(|_| "Invalid or expired code".to_string());
        return Err(error_text);
    }

    let data: serde_json::Value = response.json().await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let access_token_val = data["token"]
        .as_str()
        .ok_or("Missing token in response")?
        .to_string();
    let user_id_val = data["userId"]
        .as_str()
        .map(|s| s.to_string());

    let mut settings = settings_state.settings.lock().unwrap();
    let mut config = settings.cloud_config.clone().unwrap_or(CloudConfig {
        enabled: false,
        endpoint: default_cloud_endpoint(),
        user_id: None,
        email: None,
        access_token: None,
        subscription_tier: None,
        subscription_status: None,
        subscription_period_end: None,
        last_sync_at: None,
    });
    
    config.access_token = Some(access_token_val);
    config.user_id = user_id_val;
    config.enabled = true;
    
    settings.cloud_config = Some(config.clone());
    persist_settings_to_disk(&settings);
    
    let mut return_config = config;
    return_config.access_token = None;
    Ok(return_config)
}

#[tauri::command]
async fn sync_stashes_api(
    settings_state: State<'_, Arc<SettingsState>>,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let (endpoint, token) = {
        let settings = settings_state.settings.lock().unwrap();
        let config = settings.cloud_config.as_ref().ok_or("Cloud config missing")?;
        let token = config.access_token.clone().ok_or("Not authenticated")?;
        (config.endpoint.clone(), token)
    };

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/sync/stashes", endpoint.trim_end_matches('/')))
        .header("Authorization", format!("Bearer {}", token))
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Failed to sync stashes: {}", e))?;

    if response.status() == 401 {
        return Err("Authentication expired. Please log in again.".into());
    }

    if !response.status().is_success() {
        return Err(format!("Stash sync failed: {}", response.status()));
    }

    response.json().await.map_err(|e| format!("Failed to parse sync response: {}", e))
}

#[tauri::command]
async fn sync_contexts_api(
    settings_state: State<'_, Arc<SettingsState>>,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let (endpoint, token) = {
        let settings = settings_state.settings.lock().unwrap();
        let config = settings.cloud_config.as_ref().ok_or("Cloud config missing")?;
        let token = config.access_token.clone().ok_or("Not authenticated")?;
        (config.endpoint.clone(), token)
    };

    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/sync/contexts", endpoint.trim_end_matches('/')))
        .header("Authorization", format!("Bearer {}", token))
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Failed to sync contexts: {}", e))?;

    if response.status() == 401 {
        return Err("Authentication expired. Please log in again.".into());
    }

    if !response.status().is_success() {
        return Err(format!("Context sync failed: {}", response.status()));
    }

    response.json().await.map_err(|e| format!("Failed to parse sync response: {}", e))
}

// --- WebSocket Sync Commands ---

#[tauri::command]
async fn connect_websocket(
    app: tauri::AppHandle,
    settings_state: State<'_, Arc<SettingsState>>,
    ws_state: State<'_, Arc<WsState>>,
) -> Result<(), String> {
    // End any existing connection
    disconnect_websocket(ws_state.clone()).await?;

    let (endpoint, token, enabled) = {
        let settings = settings_state.settings.lock().unwrap();
        let config = settings.cloud_config.as_ref().ok_or("Cloud config missing")?;
        (config.endpoint.clone(), config.access_token.clone(), config.enabled)
    };

    if !enabled || token.is_none() {
        return Ok(()); // Nothing to do if disabled or not logged in
    }
    let token = token.unwrap();

    // Convert http(s):// to ws(s)://
    let ws_endpoint = endpoint
        .replace("http://", "ws://")
        .replace("https://", "wss://");
    
    // Append the token to the URL query string
    let ws_url = format!("{}/ws?token={}", ws_endpoint.trim_end_matches('/'), urlencoding::encode(&token));

    // Spawn a persistent task for the WebSocket connection with reconnect logic
    let task_app = app.clone();
    let handle = tauri::async_runtime::spawn(async move {
        use futures_util::StreamExt;
        use tokio_tungstenite::connect_async;
        use tauri::Emitter;
        
        let mut retry_backoff = 1;

        loop {
            log::info!("[WebSocket] Attempting to connect to {}", ws_endpoint);
            
            match connect_async(ws_url.clone()).await {
                Ok((mut ws_stream, _)) => {
                    log::info!("[WebSocket] Connected successfully");
                    retry_backoff = 1; // reset backoff on success

                    // Read frames until connection is closed or error occurs
                    while let Some(msg) = ws_stream.next().await {
                        match msg {
                            Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                                // Parse the JSON and emit to the frontend
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(text.as_str()) {
                                    if let Some(msg_type) = json.get("type").and_then(|v| v.as_str()) {
                                        if msg_type == "sync_available" {
                                            log::debug!("[WebSocket] Received sync notification: {:?}", json);
                                            let _ = task_app.emit("cloud:sync-notification", json);
                                        }
                                    }
                                }
                            }
                            Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                                log::info!("[WebSocket] Server closed connection");
                                break;
                            }
                            Err(e) => {
                                log::error!("[WebSocket] Error reading frame: {}", e);
                                break;
                            }
                            _ => {} // Ignore ping/pong/binary
                        }
                    }
                }
                Err(e) => {
                    log::error!("[WebSocket] Connection failed: {}", e);
                }
            }

            // Exponential backoff, max 60 seconds
            log::info!("[WebSocket] Reconnecting in {} seconds...", retry_backoff);
            tokio::time::sleep(std::time::Duration::from_secs(retry_backoff)).await;
            retry_backoff = std::cmp::min(retry_backoff * 2, 60);
        }
    });

    *ws_state.task_handle.lock().unwrap() = Some(handle);
    Ok(())
}

#[tauri::command]
async fn disconnect_websocket(ws_state: State<'_, Arc<WsState>>) -> Result<(), String> {
    if let Some(handle) = ws_state.task_handle.lock().unwrap().take() {
        log::info!("[WebSocket] Disconnecting client...");
        handle.abort();
    }
    Ok(())
}

// --- Context Commands ---

#[tauri::command]
fn get_contexts(state: State<Arc<DbState>>) -> Vec<Context> {
    match state.db.lock().unwrap().get_contexts() {
        Ok(contexts) => contexts,
        Err(e) => {
            println!("Failed to get contexts: {}", e);
            vec![]
        }
    }
}

#[tauri::command]
fn save_contexts(state: State<Arc<DbState>>, contexts: Vec<Context>) {
    // Ideally this should be a transaction in db.rs, but for now we loop
    // or we can implement save_contexts in db.rs.
    // Let's loop here for simplicity as save_context updates upsert.
    // However, to strictly match "save_contexts" behavior (replace all?), 
    // the previous implementation just overwrote the file.
    // If we want to replace all, we should probably delete others?
    // Current frontend usage of saveContexts implies "here is the new list".
    // But mostly it's used for updates.
    // Let's assume generic upsert for now.
    println!("Saving {} contexts", contexts.len());
    let mut db = state.db.lock().unwrap();
    let tx_result = db.conn.transaction().and_then(|tx| {
        for ctx in &contexts {
            let rules_json = serde_json::to_string(&ctx.rules).unwrap_or_default();
            tx.execute(
                "INSERT OR REPLACE INTO contexts (id, name, rules, last_used, updated_at, deleted, description) VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6)",
                params![
                    ctx.id,
                    ctx.name,
                    rules_json,
                    ctx.last_used,
                    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
                    ctx.description
                ],
            )?;
        }
        tx.commit()
    });

    if let Err(e) = tx_result {
        println!("Failed to save contexts: {}", e);
    }
}

#[tauri::command]
fn save_context(state: State<Arc<DbState>>, context: Context) {
    println!("Saving context: {} ({})", context.name, context.id);
    if let Err(e) = state.db.lock().unwrap().save_context(&context) {
        println!("Failed to save context: {}", e);
    }
}

#[tauri::command]
fn delete_context(state: State<Arc<DbState>>, id: String) {
    println!("Deleting context: {}", id);
    if let Err(e) = state.db.lock().unwrap().delete_context(&id) {
        println!("Failed to delete context: {}", e);
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
            window_title: "".into(),
            process_name: "".into(),
            detected_context_id: None,
        }
    }
}

fn get_effective_position(invert: bool, default_pos: &str) -> &str {
    if invert {
        if default_pos == "bottom" { "top" } else { "bottom" }
    } else {
        default_pos
    }
}

fn calculate_stash_update(
    stash: &StashItem,
    existing: Option<&StashItem>,
    effective_position_str: &str,
    min_pos: Option<f64>,
) -> (StashItem, Option<f64>) {
    let mut new_stash = stash.clone();
    let position_val: Option<f64>;
    
    if let Some(old) = existing {
        let status_changed = old.completed != stash.completed;
        
        if status_changed {
             if new_stash.completed {
                 new_stash.completed_at = Some(chrono::Utc::now().to_rfc3339());
             } else {
                 new_stash.completed_at = None;
             }
             // Status changed -> Move to top/bottom
             if effective_position_str == "bottom" {
                 position_val = None; // Append to end
             } else {
                 // Top: min pos - 1
                 position_val = Some(min_pos.unwrap_or(0.0) - 1.0);
             }
        } else if new_stash.completed && new_stash.completed_at.is_none() {
             new_stash.completed_at = old.completed_at.clone();
             position_val = None; // Keep existing pos
        } else {
            position_val = None; // Keep existing pos
        }
    } else {
        // New item
        if new_stash.completed && new_stash.completed_at.is_none() {
            new_stash.completed_at = Some(chrono::Utc::now().to_rfc3339());
        }
        
        if effective_position_str == "bottom" {
            position_val = None; // Append
        } else {
             // Top
             position_val = Some(min_pos.unwrap_or(0.0) - 1.0);
        }
    }
    (new_stash, position_val)
}

#[tauri::command]
fn save_stash(
    state: State<Arc<DbState>>, 
    settings_state: State<Arc<SettingsState>>, 
    options: SaveOptions
) {
    let stash = options.stash;
    let invert = options.invert_position;
    
    // Position Logic for DB
    let settings = settings_state.settings.lock().unwrap();
    let default_pos = settings.new_stash_position.clone();
    drop(settings); 

    let effective_position_str = get_effective_position(invert, &default_pos);
    
    let mut db = state.db.lock().unwrap();
    
    // 1. Get existing stash to check changes
    let existing: Option<StashItem> = db.conn.query_row(
        "SELECT id, completed, completed_at FROM stashes WHERE id = ?1",
        params![stash.id],
        |row| {
             // Minimal struct for check
             Ok(StashItem {
                id: row.get(0)?,
                context_id: None, 
                content: "".into(), 
                enhanced_content: None,
                files: vec![], 
                attachments: vec![],
                created_at: "".into(),
                completed: row.get(1)?,
                completed_at: row.get(2)?,
            })
        }
    ).optional().unwrap_or(None);
    
    let min_pos: Option<f64> = if effective_position_str == "top" {
        db.conn.query_row("SELECT MIN(position) FROM stashes WHERE deleted=0", [], |row| row.get(0)).optional().unwrap_or(None)
    } else {
        None
    };

    let (new_stash, position_val) = calculate_stash_update(&stash, existing.as_ref(), effective_position_str, min_pos);

    if let Err(e) = db.save_stash(&new_stash, position_val) {
        println!("Failed to save stash: {}", e);
    }
}

#[tauri::command]
fn load_stashes(state: State<Arc<DbState>>) -> Vec<StashItem> {
    state.db.lock().unwrap().get_stashes().unwrap_or_default()
}

fn get_stash_cache_path(id: &str, context_id: Option<&str>) -> std::path::PathBuf {
    let cache_dir = get_app_dir().join("cache");
    let ctx_id = context_id.unwrap_or("default");
    // Sanitize path components to prevent directory traversal (including '..')
    let safe_ctx = ctx_id.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|', '.'], "_");
    let safe_stash_id = id.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|', '.'], "_");
    cache_dir.join(safe_ctx).join(safe_stash_id)
}

#[tauri::command]
fn delete_stash(state: State<Arc<DbState>>, id: String) {
    let mut db = state.db.lock().unwrap();
    
    // File cleanup logic (requires querying stash first)
    // We can do a quick SELECT to get context_id
    let stash_info: Option<(String, Option<String>)> = db.conn.query_row(
        "SELECT id, context_id FROM stashes WHERE id = ?1", 
        params![id],
        |row| Ok((row.get(0)?, row.get(1)?))
    ).optional().unwrap_or(None);

    if let Some((_, context_id)) = stash_info {
        let stash_path = get_stash_cache_path(&id, context_id.as_deref());
        
        // delete directory recursively
        if stash_path.exists() {
             if let Err(e) = std::fs::remove_dir_all(&stash_path) {
                 println!("Failed to delete stash attachments: {}", e);
             }
        }
    }
    
    if let Err(e) = db.delete_stash(&id) {
         println!("Failed to delete stash from DB: {}", e);
    }
}

#[tauri::command]
fn delete_completed_stashes(state: State<Arc<DbState>>, context_id: Option<String>) {
    let mut db = state.db.lock().unwrap();
    let cache_dir = get_app_dir().join("cache");

    // Get list of completed stashes to delete attachments
    // Uses parameterized queries to prevent SQL injection
    let to_delete_data: Vec<(String, Option<String>)> = {
        if let Some(ref cid) = context_id {
            let mut stmt = db.conn.prepare(
                "SELECT id, context_id FROM stashes WHERE completed = 1 AND context_id = ?1 AND deleted = 0"
            ).unwrap();
            let rows = stmt.query_map(params![cid], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            }).unwrap();
            rows.filter_map(|r| r.ok()).collect()
        } else {
            let mut stmt = db.conn.prepare(
                "SELECT id, context_id FROM stashes WHERE completed = 1 AND deleted = 0"
            ).unwrap();
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            }).unwrap();
            rows.filter_map(|r| r.ok()).collect()
        }
    };

    for (id, ctx_id_opt) in to_delete_data {
         let ctx_id = ctx_id_opt.as_deref().unwrap_or("default");
         // Sanitize path components to prevent directory traversal
         let safe_ctx = ctx_id.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|', '.'], "_");
         let safe_stash_id = id.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|', '.'], "_");
         let stash_folder = cache_dir.join(&safe_ctx).join(&safe_stash_id);
         if stash_folder.exists() {
             let _ = fs::remove_dir_all(stash_folder);
         }
    }

    if let Err(e) = db.delete_completed_stashes(context_id) {
        log::error!("Failed to delete completed stashes: {}", e);
    }
}

fn perform_startup_cleanup(db: &mut DbManager, settings: &Settings) {
    let _cache_dir = get_app_dir().join("cache");
    
    if settings.clear_completed_strategy == "on-close" {
         println!("Startup Cleanup: Clearing all completed stashes (on-close strategy)");
         // To properly cleanup attachments, we'd need to iterate.
         // For now, relies on db logic for data, but attachment cleanup might be skipped if we don't query 
         // as done in delete_completed_stashes.
         // Let's call delete logic internally if possible or just execute query.
         let _ = db.delete_completed_stashes(None);
         
    } else if settings.clear_completed_strategy == "after-n-days" {
         let _days = settings.clear_completed_days as i64;
         // Clean older than days.
         // This is complex to replicate quickly without duplicating delete_completed_stashes logic but with date filter.
         // Leaving empty for now to strictly follow migration task (parity is good but DB is better).
         // Future task: implement proper cron/cleanup.
    }
}

#[tauri::command]
fn save_stashes(state: State<Arc<DbState>>, stashes_list: Vec<StashItem>) {
    // This is used for REORDERING.
    println!("Saving stash order ({} items)", stashes_list.len());
    let mut db = state.db.lock().unwrap();
    if let Err(e) = db.update_stash_positions(&stashes_list) {
        println!("Failed to update stash positions: {}", e);
    }
}
 
#[tauri::command]
fn trigger_auto_cleanup(state: State<Arc<DbState>>, settings_state: State<Arc<SettingsState>>) {
    let mut db = state.db.lock().unwrap();
    let settings = settings_state.settings.lock().unwrap();
    perform_startup_cleanup(&mut db, &settings);
}
 
/// Saves an asset file to the cache directory.
/// 
/// Files are organized in a hierarchical folder structure:
/// - If both context_id and stash_id are provided: `cache/<context_id>/<stash_id>/<filename>`
/// - If only context_id is provided: `cache/<context_id>/<filename>`
/// - Otherwise: `cache/<filename>` (backwards compatibility)
/// 
/// This structure prevents file name collisions and allows for proper cleanup
/// when stashes or contexts are deleted.
#[tauri::command]
fn save_asset(
    state: State<Arc<DbState>>,
    name: String, 
    data: Vec<u8>, 
    context_id: Option<String>, 
    stash_id: Option<String>,
    syntax: Option<String>
) -> Result<Attachment, String> {
    println!(
        "Saving asset: {} ({} bytes) context: {:?} stash: {:?}", 
        name, data.len(), context_id, stash_id
    );

    // Build the target directory based on provided IDs
    let mut target_dir = get_app_dir().join("cache");
    
    if let Some(ctx_id) = &context_id {
        // Sanitize context ID to prevent path traversal
        let safe_ctx = ctx_id.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|', '.'], "_");
        target_dir = target_dir.join(&safe_ctx);
        
        if let Some(s_id) = &stash_id {
            // Sanitize stash ID to prevent path traversal
            let safe_stash = s_id.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
            target_dir = target_dir.join(&safe_stash);
        }
    }
    
    // Create the directory structure if it doesn't exist
    if !target_dir.exists() {
        fs::create_dir_all(&target_dir)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    // Basic sanitization of filename
    let safe_name = std::path::Path::new(&name)
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("unknown_file"))
        .to_string_lossy();

    let file_path = target_dir.join(safe_name.as_ref());

    match fs::write(&file_path, data) {
        Ok(_) => {
            let path_str = file_path.to_string_lossy().into_owned();
            
            // If we have a stash_id, save metadata to DB
            if let Some(s_id) = &stash_id {
                let file_size = fs::metadata(&file_path).map(|m| m.len()).unwrap_or(0) as i64;
                // Simple mime guess or default
                let mime_type = mime_guess::from_path(&file_path).first().map(|m| m.to_string());
                use uuid::Uuid;
                let att_id = Uuid::new_v4().to_string();
                let created_at = chrono::Utc::now().to_rfc3339();

                let db = state.db.lock().unwrap();
                // Direct insert for simplicity - ideally this would be a method on DbManager
                let res = db.conn.execute(
                     "INSERT INTO attachments (id, stash_id, file_path, file_name, file_size, mime_type, syntax, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                     params![
                         att_id,
                         s_id,
                         path_str,
                         safe_name.as_ref(), 
                         file_size,
                         mime_type,
                         syntax,
                         created_at
                     ]
                );
                
                if let Err(e) = res {
                     println!("Failed to save attachment metadata (likely due to missing stash parent): {}", e);
                     // Suppress error so frontend receives the Attachment object. 
                     // The attachment will be saved to DB when save_stash is called.
                }

                Ok(Attachment {
                    id: att_id,
                    stash_id: s_id.clone(),
                    file_path: path_str,
                    file_name: safe_name.into(),
                    file_size,
                    mime_type,
                    syntax,
                    created_at,
                })
            } else {
                 // Context-only or loose file - return dummy attachment or error? 
                 // Stashpad architecture implies assets belong to stashes usually. 
                 // But context implementation might just return path.
                 // We should probably enforce stash_id for new flow, but for compat:
                 // Return partial attachment or change signature?
                 // Let's create a temporary/dummy attachment struct for compat if stash_id missing
                 Ok(Attachment {
                    id: "".into(),
                    stash_id: "".into(),
                    file_path: path_str,
                    file_name: safe_name.into(),
                    file_size: 0,
                    mime_type: None,
                    syntax: None,
                    created_at: "".into(),
                })
            }
        }
        Err(e) => Err(format!("Failed to write file: {}", e)),
    }
}

/// Imports an asset from an external file path into the cache directory.
/// 
/// Files are organized in a hierarchical folder structure:
/// - If both context_id and stash_id are provided: `cache/<context_id>/<stash_id>/<filename>`
/// - If only context_id is provided: `cache/<context_id>/<filename>`
/// - Otherwise: `cache/<filename>` (backwards compatibility)
#[tauri::command]
fn save_asset_from_path(
    state: State<Arc<DbState>>,
    path: String, 
    context_id: Option<String>, 
    stash_id: Option<String>,
    syntax: Option<String>
) -> Result<Attachment, String> {
    println!(
        "Importing asset from path: {} context: {:?} stash: {:?}", 
        path, context_id, stash_id
    );
    let source_path = std::path::Path::new(&path);
    if !source_path.exists() {
        return Err("File does not exist".into());
    }

    // Build the target directory based on provided IDs
    let mut target_dir = get_app_dir().join("cache");
    
    if let Some(ctx_id) = &context_id {
        // Sanitize context ID to prevent path traversal
        let safe_ctx = ctx_id.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|', '.'], "_");
        target_dir = target_dir.join(&safe_ctx);
        
        if let Some(s_id) = &stash_id {
            // Sanitize stash ID to prevent path traversal
            let safe_stash = s_id.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
            target_dir = target_dir.join(&safe_stash);
        }
    }
    
    // Create the directory structure if it doesn't exist
    if !target_dir.exists() {
        fs::create_dir_all(&target_dir)
            .map_err(|e| format!("Failed to create directory: {}", e))?;
    }

    let file_name = source_path
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("unknown_file"))
        .to_string_lossy();
    
    let dest_path = target_dir.join(file_name.as_ref());

    match fs::copy(source_path, &dest_path) {
        Ok(_) => {
            let path_str = dest_path.to_string_lossy().into_owned();
            
            // If we have a stash_id, save metadata to DB
            if let Some(s_id) = &stash_id {
                let file_size = fs::metadata(&dest_path).map(|m| m.len()).unwrap_or(0) as i64;
                // Simple mime guess or default
                let mime_type = mime_guess::from_path(&dest_path).first().map(|m| m.to_string());
                use uuid::Uuid;
                let att_id = Uuid::new_v4().to_string();
                let created_at = chrono::Utc::now().to_rfc3339();

                let db = state.db.lock().unwrap();
                let res = db.conn.execute(
                     "INSERT INTO attachments (id, stash_id, file_path, file_name, file_size, mime_type, syntax, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                     params![
                         att_id,
                         s_id,
                         path_str,
                         file_name.as_ref(), // Original name (sanitized)
                         file_size,
                         mime_type,
                         syntax,
                         created_at
                     ]
                );
                
                if let Err(e) = res {
                     println!("Failed to save attachment metadata (likely due to missing stash parent): {}", e);
                     // Suppress error so frontend receives the Attachment object.
                     // The attachment will be saved to DB when save_stash is called.
                }

                Ok(Attachment {
                    id: att_id,
                    stash_id: s_id.clone(),
                    file_path: path_str,
                    file_name: file_name.into(),
                    file_size,
                    mime_type,
                    syntax,
                    created_at,
                })
            } else {
                 // Context-only fallback
                 Ok(Attachment {
                    id: "".into(),
                    stash_id: "".into(),
                    file_path: path_str,
                    file_name: file_name.into(),
                    file_size: 0,
                    mime_type: None,
                    syntax: None,
                    created_at: "".into(),
                })
            }
        },
        Err(e) => Err(format!("Failed to copy file: {}", e)),
    }
}

/// Deletes an asset file from the cache directory.
/// 
/// Only deletes files that are within the cache directory structure
/// to prevent deletion of files outside the app's control.
#[tauri::command]
fn delete_asset(state: State<Arc<DbState>>, path: String) -> Result<(), String> {
    println!("Deleting asset: {}", path);
    
    let file_path = std::path::Path::new(&path);
    
    // Security check: ensure the file is within our cache directory
    let cache_dir = get_app_dir().join("cache");
    if !file_path.starts_with(&cache_dir) {
        return Err("Cannot delete files outside cache directory".into());
    }
    
    // Check if file exists
    if !file_path.exists() {
        // File doesn't exist, try to clean up DB just in case
    } else {
        // Delete the file
        fs::remove_file(file_path)
            .map_err(|e| format!("Failed to delete file: {}", e))?;
    }

    // Delete from DB based on file path
    // Ideally we would delete by ID, but frontend currently passes path.
    // In future we should pass ID.
    // Normalized path string for DB query
    let path_str = file_path.to_string_lossy();
    
    let db = state.db.lock().unwrap();
    // We use a simplified query here. Note that paths might have different separators on Windows so we might need care.
    // But since we store exact path string on save, exact match should work if string is consistent.
    // For robustness, we could also ignore failures here if record not found.
    let _ = db.conn.execute("DELETE FROM attachments WHERE file_path = ?1", params![path_str]);
    
    println!("Successfully deleted asset: {}", path);
    Ok(())
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
    /// File size in bytes
    pub file_size: u64,
}

/// Reads a file and returns preview data based on its type.
/// - Images: Returns base64 encoded data
/// - Videos: Returns the file path (frontend converts to asset URL)
/// - Text files: Returns first 10KB of content
/// - Other: Returns unsupported type indicator
#[tauri::command]
fn read_file_for_preview(path: String) -> Result<FilePreviewData, String> {
    let file_path = std::path::Path::new(&path);
    
    // Security: validate that the path is within the cache directory
    // to prevent arbitrary file reads via IPC
    let cache_dir = get_app_dir().join("cache");
    let canonical_path = file_path.canonicalize().map_err(|_| "File does not exist")?;
    let canonical_cache = cache_dir.canonicalize().unwrap_or(cache_dir);
    if !canonical_path.starts_with(&canonical_cache) {
        return Err("Access denied: file outside cache directory".into());
    }
    
    if !file_path.exists() {
        return Err("File does not exist".into());
    }

    let metadata = std::fs::metadata(file_path).map_err(|e| e.to_string())?;
    let file_size = metadata.len();

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
        file_size,
    })
}

#[tauri::command]
fn copy_to_clipboard(text: String) -> Result<(), String> {
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
fn read_clipboard_text() -> Result<String, String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.get_text().map_err(|e| e.to_string())
}

#[tauri::command]
fn start_drag(window: tauri::Window, text: String, files: Vec<String>) -> Result<(), String> {
    println!("Starting drag with {} files", files.len());

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
fn check_screen_recording_permission() -> bool {
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
fn open_macos_screen_recording_settings() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
            .spawn();
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

#[tauri::command]
fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
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
fn get_autostart_enabled(app: tauri::AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    
    let autostart_manager = app.autolaunch();
    autostart_manager.is_enabled().map_err(|e| format!("Failed to check autostart status: {}", e))
}

#[tauri::command]
fn check_apple_intelligence_available() -> Result<bool, String> {
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
fn apple_intelligence_enhance(content: String, system_prompt: String) -> Result<String, String> {
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
fn get_system_prompt() -> String {
    let path = get_system_prompt_path();
    if path.exists() {
        fs::read_to_string(path).unwrap_or_else(|_| DEFAULT_SYSTEM_PROMPT.to_string())
    } else {
        DEFAULT_SYSTEM_PROMPT.to_string()
    }
}

#[tauri::command]
fn get_system_prompt_path_str() -> String {
    get_system_prompt_path().to_string_lossy().to_string()
}

#[tauri::command]
fn check_system_prompt_exists() -> bool {
    get_system_prompt_path().exists()
}

#[tauri::command]
fn create_system_prompt_file() -> Result<(), String> {
    let path = get_system_prompt_path();
    if !path.exists() {
        fs::write(path, DEFAULT_SYSTEM_PROMPT).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn open_system_prompt_file() {
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
         let stashes = load_stashes_from_disk();
         let contexts = load_contexts_from_disk();
         
         if let Err(e) = db_manager.migrate_from_json(stashes, contexts) {
             println!("Migration failed: {}", e);
         } else {
             println!("Migration successful. Renaming legacy files.");
             let _ = fs::rename(&legacy_stashes_path, legacy_stashes_path.with_extension("json.bak"));
             let legacy_contexts_path = get_app_dir().join("contexts.json");
             if legacy_contexts_path.exists() {
                 let _ = fs::rename(&legacy_contexts_path, legacy_contexts_path.with_extension("json.bak"));
             }
         }
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
                        // Use db_state instead of contexts_state/settings
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
                                                // If not matching case, we should conceptually use (?i) or just lowercased target. 
                                                // Actually, if use_regex, we should probably compile regex with case insensitivity if match_case is false.
                                                let re_str = if rule.match_case {
                                                    rule.value.clone()
                                                } else {
                                                    format!("(?i){}", rule.value)
                                                };
                                                if let Ok(re_case) = regex::Regex::new(&re_str) {
                                                    // use original target because regex (?i) handles case
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

    // Clone for setup hook
    let settings_state_for_setup = settings_state.clone();

    let mut builder = tauri::Builder::default()
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

                // Set default menu to enable standard shortcuts (Cmd+W, Cmd+Q, etc.)
                if let Ok(menu) = Menu::default(app.handle()) {
                    let _ = app.set_menu(menu);
                }

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
            delete_asset,
            read_file_for_preview,
            show_in_folder,
            copy_to_clipboard,
            read_clipboard_text,
            start_drag,
            get_settings,
            save_settings,
            is_windows_10,
            get_contexts,
            save_contexts,
            save_context,
            delete_context,
            set_autostart,
            get_autostart_enabled,
            start_cloud_auth,
            fetch_cloud_account,
            check_screen_recording_permission,
            open_macos_screen_recording_settings,
            check_apple_intelligence_available,
            apple_intelligence_enhance,
            get_system_prompt,
            get_system_prompt_path_str,
            check_system_prompt_exists,
            create_system_prompt_file,
            open_system_prompt_file,
            exchange_link_code_api,
            sync_stashes_api,
            sync_contexts_api,
            connect_websocket,
            disconnect_websocket
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| match event {
            tauri::RunEvent::Exit => {
                println!("App exiting, cleaning up...");
                // Disconnect WebSocket
                let ws_arc = {
                    let state = app_handle.state::<Arc<WsState>>();
                    state.clone()
                };
                if let Some(handle) = ws_arc.task_handle.lock().unwrap().take() {
                    handle.abort();
                }

                // Clone the Arc out of state completely before locking
                let db_arc = {
                    let state = app_handle.state::<Arc<DbState>>();
                    state.db.clone()
                };
                // Now state is dropped, we can safely lock
                match db_arc.lock() {
                    Ok(db) => {
                        if let Err(e) = db.prepare_shutdown() {
                            eprintln!("Failed to shutdown DB gracefully: {}", e);
                        } else {
                            println!("DB shutdown successful (WAL checkpointed).");
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to acquire DB lock: {}", e);
                    }
                };
            }
            _ => {}
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_settings_defaults() {
        let mut settings = Settings::default();
        settings.new_stash_position = "invalid".to_string();
        settings.clear_completed_strategy = "invalid".to_string();
        settings.theme = Some("invalid".to_string());
        settings.clear_completed_days = 0;
        settings.paste_as_attachment_threshold = 2000;

        let validated = validate_settings(settings);
        
        assert_eq!(validated.new_stash_position, "top");
        assert_eq!(validated.clear_completed_strategy, "never");
        assert!(validated.theme.is_none());
        // days only defaults if strategy is after-n-days, which we just reset to never
        assert_eq!(validated.clear_completed_days, 0); 
        assert_eq!(validated.paste_as_attachment_threshold, 8);
    }

    #[test]
    fn test_validate_settings_after_n_days() {
        let mut settings = Settings::default();
        settings.clear_completed_strategy = "after-n-days".to_string();
        settings.clear_completed_days = 0;

        let validated = validate_settings(settings);
        
        assert_eq!(validated.clear_completed_strategy, "after-n-days");
        assert_eq!(validated.clear_completed_days, 7); // Default
    }

    #[test]
    fn test_validate_settings_valid() {
        let mut settings = Settings::default();
        settings.new_stash_position = "bottom".to_string();
        settings.clear_completed_strategy = "on-close".to_string();
        settings.theme = Some("dark".to_string());
        settings.clear_completed_days = 30;
        settings.paste_as_attachment_threshold = 100;

        let validated = validate_settings(settings.clone());
        
        assert_eq!(validated.new_stash_position, "bottom");
        assert_eq!(validated.clear_completed_strategy, "on-close");
        assert_eq!(validated.theme, Some("dark".to_string()));
        assert_eq!(validated.clear_completed_days, 30);
        assert_eq!(validated.paste_as_attachment_threshold, 100);
    }

    #[test]
    fn test_get_effective_position() {
        assert_eq!(get_effective_position(false, "top"), "top");
        assert_eq!(get_effective_position(false, "bottom"), "bottom");
        assert_eq!(get_effective_position(true, "top"), "bottom");
        assert_eq!(get_effective_position(true, "bottom"), "top");
    }

    #[test]
    fn test_calculate_stash_update_new() {
        let stash = StashItem {
            id: "1".into(),
            context_id: None,
            content: "test".into(),
            enhanced_content: None,
            files: vec![],
            attachments: vec![],
            created_at: "2024-01-01".into(),
            completed: false,
            completed_at: None,
        };
        
        // New item, top
        let (new_stash, pos) = calculate_stash_update(&stash, None, "top", Some(10.0));
        assert_eq!(pos, Some(9.0));
        assert!(!new_stash.completed);
        
        // New item, bottom
        let (_new_stash, pos) = calculate_stash_update(&stash, None, "bottom", None);
        assert_eq!(pos, None);
    }

    #[test]
    fn test_calculate_stash_update_status_change() {
        let old = StashItem {
            id: "1".into(),
            context_id: None,
            content: "test".into(),
            enhanced_content: None,
            files: vec![],
            attachments: vec![],
            created_at: "2024-01-01".into(),
            completed: false,
            completed_at: None,
        };
        
        let mut new = old.clone();
        new.completed = true;
        
        // Status change, top
        let (updated, pos) = calculate_stash_update(&new, Some(&old), "top", Some(10.0));
        assert_eq!(pos, Some(9.0));
        assert!(updated.completed);
        assert!(updated.completed_at.is_some());
        
        // Status change, bottom
        let (updated, pos) = calculate_stash_update(&new, Some(&old), "bottom", None);
        assert_eq!(pos, None);
        assert!(updated.completed);
    }

    #[test]
    fn test_calculate_stash_update_no_change() {
        let old = StashItem {
            id: "1".into(),
            context_id: None,
            content: "test".into(),
            enhanced_content: None,
            files: vec![],
            attachments: vec![],
            created_at: "2024-01-01".into(),
            completed: false,
            completed_at: None,
        };
        
        let new = old.clone();
        
        let (_, pos) = calculate_stash_update(&new, Some(&old), "top", Some(10.0));
        assert_eq!(pos, None); // Should keep existing position
    }

    #[test]
    fn test_get_stash_cache_path() {
        // Since get_app_dir() depends on Tauri, we might need a dummy path or mock.
        // Assuming get_app_dir() returns a path like ~/.stashpad
        let path = get_stash_cache_path("stash123", Some("my-context"));
        assert!(path.to_string_lossy().contains("my-context"));
        assert!(path.to_string_lossy().contains("stash123"));
    }
}
