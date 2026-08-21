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

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub id: String,
    #[serde(default)]
    pub stash_id: String,
    #[serde(default)]
    pub file_path: String,
    pub file_name: String,
    pub file_size: i64,
    pub mime_type: Option<String>,
    pub syntax: Option<String>,
    pub created_at: String,
}

/// One stash's place in the order.
///
/// Travels separately from the record so a reorder never carries content with it.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StashPosition {
    pub id: String,
    pub position: f64,
    /// Client clock, in Unix seconds - the Last-Write-Wins discriminator for ordering.
    pub position_updated_at: u64,
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
    #[serde(default)]
    pub updated_at: Option<u64>,
    #[serde(default)]
    pub deleted: bool,
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
    pub window_title: String,
    pub process_name: String,
    pub detected_context_id: Option<String>,
}

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
    #[serde(default)]
    pub updated_at: Option<u64>,
    #[serde(default)]
    pub deleted: bool,
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
    /// Scale of the UI: 1-5, default 3
    #[serde(default)]
    pub ui_scale: Option<u32>,
    /// Playback volume for video attachments, 0.0-1.0
    #[serde(default)]
    pub video_volume: Option<f64>,
    #[serde(default)]
    pub video_muted: Option<bool>,
    /// Downscale large images when they are attached
    #[serde(default)]
    pub resize_images: Option<bool>,
    /// When the updater last completed a check, epoch milliseconds.
    ///
    /// Persisted so the 48h cadence survives a restart: without it every launch would
    /// hit the update endpoint again.
    #[serde(default)]
    pub last_update_check_at: Option<u64>,
    /// Newest version the updater has seen, so the header notice can be restored on
    /// launch without waiting for a network round-trip.
    #[serde(default)]
    pub latest_known_update_version: Option<String>,
    /// Version the user chose to skip. The notice stays hidden until something newer
    /// than this appears.
    #[serde(default)]
    pub dismissed_update_version: Option<String>,
    /// "Remind me later": epoch milliseconds before which the notice stays hidden even
    /// for a version the user has not skipped outright.
    #[serde(default)]
    pub update_remind_after: Option<u64>,
    /// Whether the periodic background update check runs at all.
    #[serde(default = "default_auto_update_checks")]
    pub auto_update_checks: bool,
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
    /// Enterprise owner ID if part of a team
    #[serde(default)]
    pub enterprise_owner_id: Option<String>,
    /// Last sync timestamp
    #[serde(default)]
    pub last_sync_at: Option<String>,
}

pub fn default_cloud_endpoint() -> String {
    "https://api.stashpad.org".to_string()
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

pub fn default_clear_completed_strategy() -> String {
    "never".to_string()
}

pub fn default_clear_completed_days() -> u32 {
    7
}

pub fn default_paste_as_attachment_threshold() -> u32 {
    8
}

pub fn default_new_stash_position() -> String {
    "top".to_string()
}

pub fn default_strip_tags_on_copy() -> bool {
    true
}

pub fn default_auto_update_checks() -> bool {
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
            ui_scale: None,
            video_volume: None,
            video_muted: None,
            resize_images: None,
            last_update_check_at: None,
            latest_known_update_version: None,
            dismissed_update_version: None,
            update_remind_after: None,
            auto_update_checks: default_auto_update_checks(),
        }
    }
}

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FilePreviewData {
    pub file_type: String,
    pub content: String,
    pub file_name: String,
    pub mime_type: String,
    pub file_size: u64,
}
