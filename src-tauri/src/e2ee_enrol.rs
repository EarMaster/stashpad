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

//! Getting this installation the content key, and converting an account to use one.
//!
//! Three ways in, and no fourth:
//!
//! 1. **Bootstrap** - the first installation makes the key and the recovery code.
//! 2. **Approval** - an installation that already holds the key wraps it to a new one,
//!    after the user has compared a fingerprint on both screens.
//! 3. **Recovery** - the user types the code, which is the only path when no other
//!    installation is reachable.
//!
//! The third is not an emergency measure. It is the ordinary path whenever the other
//! machine is switched off, which is why the code has to be kept like a password rather
//! than filed away and forgotten.
//!
//! **Registering a key is not being trusted with one.** An installation publishes its
//! public key and waits. If publishing were enough, the server could enrol a device of its
//! own by minting a link code - the fingerprint comparison is what stands in the way, and
//! it only works because the approving side recomputes the fingerprint from the key it
//! fetched rather than reading the one the server offers.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::e2ee;
use crate::e2ee_session;
use crate::state::{DbState, SettingsState};

/// Where this account and this installation stand.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrolmentStatus {
    /// 0 when the account has never been encrypted.
    pub epoch: u32,
    /// `off`, `migrating` or `sealed`.
    pub state: String,
    /// Whether this installation holds the content key right now.
    pub unlocked: bool,
    /// Whether the server has a wrap for this installation.
    pub enrolled: bool,
    pub has_recovery: bool,
    pub recovery_acknowledged: bool,
    /// This installation's fingerprint, for the user to compare.
    pub fingerprint: String,
    pub devices: Vec<DeviceSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSummary {
    pub device_id: String,
    pub public_key: String,
    /// Recomputed locally, never the server's copy - see the module notes.
    pub fingerprint: String,
    pub status: String,
    pub enrolled_at: String,
}

#[derive(Deserialize)]
struct ServerState {
    epoch: u32,
    state: String,
    #[serde(default)]
    verifier: Option<String>,
    #[serde(default)]
    has_recovery: bool,
    #[serde(default)]
    recovery_acknowledged: bool,
    #[serde(default)]
    devices: Vec<ServerDevice>,
    #[serde(default)]
    wrapped_key_for_device: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServerDevice {
    device_id: String,
    public_key: String,
    status: String,
    enrolled_at: String,
}

/// Read the account's key state, and unlock this session if there is a wrap waiting.
async fn fetch_state(
    settings_state: &State<'_, Arc<SettingsState>>,
) -> Result<(ServerState, String, String), String> {
    let device_id = crate::utils::get_device_id(None).await?;
    let body = crate::sync::e2ee_get(settings_state, &format!("/e2ee/state?deviceId={}", device_id))
        .await?;
    let user_id = {
        let settings = settings_state.lock_settings();
        settings
            .cloud_config
            .as_ref()
            .and_then(|c| c.user_id.clone())
            .unwrap_or_default()
    };
    let state: ServerState = serde_json::from_value(body)
        .map_err(|e| format!("Could not read the account's encryption state: {}", e))?;
    Ok((state, user_id, device_id))
}

#[tauri::command]
pub async fn e2ee_status(
    settings_state: State<'_, Arc<SettingsState>>,
) -> Result<EnrolmentStatus, String> {
    let (server, user_id, device_id) = fetch_state(&settings_state).await?;
    let keypair = e2ee_session::load_or_create_device_keypair()?;

    // Unlock opportunistically: if the server is holding a wrap for this installation and
    // the session has not opened it yet, do it now rather than waiting for a ceremony.
    if !e2ee_session::is_unlocked() {
        if let Some(wrapped) = server.wrapped_key_for_device.as_deref() {
            if let Err(e) = e2ee_session::unlock_with_device(
                &keypair,
                wrapped,
                &user_id,
                server.epoch,
                server.verifier.as_deref().unwrap_or_default(),
            ) {
                log::warn!("Could not open this installation's copy of the key: {}", e);
            }
        }
    }

    let enrolled = server
        .devices
        .iter()
        .any(|d| d.device_id == device_id && d.status == "active");

    Ok(EnrolmentStatus {
        epoch: server.epoch,
        state: server.state,
        unlocked: e2ee_session::is_unlocked(),
        enrolled,
        has_recovery: server.has_recovery,
        recovery_acknowledged: server.recovery_acknowledged,
        fingerprint: e2ee::fingerprint(&user_id, &keypair.public_bytes()),
        devices: summarise(&server.devices, &user_id),
    })
}

/// Turn the server's device list into something the interface can show.
///
/// Every fingerprint is recomputed from the public key. Displaying the server's stored copy
/// would have the user compare two numbers that both came from the server, which verifies
/// nothing at all.
fn summarise(devices: &[ServerDevice], user_id: &str) -> Vec<DeviceSummary> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    devices
        .iter()
        .map(|d| {
            let fingerprint = STANDARD
                .decode(&d.public_key)
                .map(|key| e2ee::fingerprint(user_id, &key))
                .unwrap_or_else(|_| "unreadable key".to_string());
            DeviceSummary {
                device_id: d.device_id.clone(),
                public_key: d.public_key.clone(),
                fingerprint,
                status: d.status.clone(),
                enrolled_at: d.enrolled_at.clone(),
            }
        })
        .collect()
}

/// Publish this installation's public key. It lands pending.
#[tauri::command]
pub async fn e2ee_register_device(
    settings_state: State<'_, Arc<SettingsState>>,
) -> Result<String, String> {
    let keypair = e2ee_session::load_or_create_device_keypair()?;
    let device_id = crate::utils::get_device_id(None).await?;
    let user_id = {
        let settings = settings_state.lock_settings();
        settings
            .cloud_config
            .as_ref()
            .and_then(|c| c.user_id.clone())
            .unwrap_or_default()
    };
    let fingerprint = e2ee::fingerprint(&user_id, &keypair.public_bytes());

    crate::sync::e2ee_post(
        &settings_state,
        "/e2ee/devices",
        serde_json::json!({
            "deviceId": device_id,
            "publicKey": keypair.public_b64(),
            "fingerprint": fingerprint,
        }),
    )
    .await?;

    Ok(fingerprint)
}

/// What the user must write down, returned exactly once.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnableResult {
    pub recovery_code: String,
    pub fingerprint: String,
}

/// Bootstrap: make the content key and the recovery code, and turn encryption on.
///
/// The key is wrapped to this installation and to the recovery code in the same request, so
/// it is never reachable by only one thing - a machine lost before the code was written
/// down would otherwise take the account with it.
#[tauri::command]
pub async fn e2ee_enable(
    settings_state: State<'_, Arc<SettingsState>>,
) -> Result<EnableResult, String> {
    let keypair = e2ee_session::load_or_create_device_keypair()?;
    let device_id = crate::utils::get_device_id(None).await?;
    let user_id = {
        let settings = settings_state.lock_settings();
        settings
            .cloud_config
            .as_ref()
            .and_then(|c| c.user_id.clone())
            .ok_or("Sign in to Stashpad Cloud first")?
    };

    // Publish the key first; the server refuses to enable for an installation it has never
    // seen, which is what stops a key being wrapped to a public key nobody published.
    e2ee_register_device(settings_state.clone()).await?;

    let content_key = e2ee::new_content_key();
    let verifier = e2ee::make_verifier(&content_key)?;
    let wrapped = e2ee::wrap_to_device(&keypair.public_bytes(), &content_key, &user_id, 1)?;

    let recovery = e2ee::new_recovery_code();
    let (salt, recovery_wrap) = e2ee::wrap_to_recovery(&recovery, &content_key, &user_id, 1)?;

    crate::sync::e2ee_post(
        &settings_state,
        "/e2ee/enable",
        serde_json::json!({
            "epoch": 1,
            "verifier": verifier,
            "deviceId": device_id,
            "wrappedKeys": [{
                "id": uuid::Uuid::new_v4().to_string(),
                "kind": "device",
                "deviceId": device_id,
                "wrapAlg": "hpke-x25519-hkdfsha256-chachapoly",
                "wrappedKey": wrapped,
            }],
            "recovery": {
                "id": uuid::Uuid::new_v4().to_string(),
                "kdfSalt": salt,
                "wrapAlg": "xchacha20poly1305",
                "wrappedKey": recovery_wrap,
                "codeHint": recovery.hint(),
            },
        }),
    )
    .await?;

    let fingerprint = e2ee::fingerprint(&user_id, &keypair.public_bytes());
    e2ee_session::set_content_key(content_key, 1);

    Ok(EnableResult {
        recovery_code: recovery.printed,
        fingerprint,
    })
}

/// Approve another installation, after the user has compared fingerprints.
///
/// `expected_fingerprint` is what the user read off the other screen. It is checked against
/// one recomputed here from the fetched key, so a server that substituted a key of its own
/// is caught - that comparison is the entire defence, and skipping it makes enrolment
/// meaningless.
#[tauri::command]
pub async fn e2ee_approve_device(
    settings_state: State<'_, Arc<SettingsState>>,
    device_id: String,
    expected_fingerprint: String,
) -> Result<(), String> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    let content_key =
        e2ee_session::content_key_bytes().ok_or("Unlock this installation first")?;
    let (server, user_id, this_device) = fetch_state(&settings_state).await?;

    let target = server
        .devices
        .iter()
        .find(|d| d.device_id == device_id)
        .ok_or("That installation is not waiting to be approved")?;

    let public_key = STANDARD
        .decode(&target.public_key)
        .map_err(|_| "That installation published an unreadable key".to_string())?;
    if public_key.len() != 32 {
        return Err("That installation published a key of the wrong size".to_string());
    }

    let actual = e2ee::fingerprint(&user_id, &public_key);
    if actual != expected_fingerprint.trim().to_ascii_uppercase() {
        return Err(format!(
            "The codes do not match. This installation shows {}, so do not approve it.",
            actual
        ));
    }

    let mut key = [0u8; 32];
    key.copy_from_slice(&public_key);
    let wrapped = e2ee::wrap_to_device(&key, &content_key, &user_id, server.epoch)?;

    crate::sync::e2ee_post(
        &settings_state,
        "/e2ee/keys",
        serde_json::json!({
            "byDeviceId": this_device,
            "epoch": server.epoch,
            "id": uuid::Uuid::new_v4().to_string(),
            "kind": "device",
            "deviceId": device_id,
            "wrapAlg": "hpke-x25519-hkdfsha256-chachapoly",
            "wrappedKey": wrapped,
        }),
    )
    .await?;

    Ok(())
}

/// Let this installation in with the recovery code, when no other is reachable.
#[tauri::command]
pub async fn e2ee_recover(
    settings_state: State<'_, Arc<SettingsState>>,
    code: String,
) -> Result<(), String> {
    let keypair = e2ee_session::load_or_create_device_keypair()?;
    let (server, user_id, device_id) = fetch_state(&settings_state).await?;

    if server.epoch == 0 {
        return Err("This account is not encrypted".to_string());
    }

    let recovery = crate::sync::e2ee_get(&settings_state, "/e2ee/recovery").await?;
    let salt = recovery["kdfSalt"]
        .as_str()
        .ok_or("This account has no recovery code on file")?;
    let wrapped = recovery["wrappedKey"]
        .as_str()
        .ok_or("This account has no recovery code on file")?;

    e2ee_session::unlock_with_recovery(
        &code,
        salt,
        wrapped,
        &user_id,
        server.epoch,
        server.verifier.as_deref().unwrap_or_default(),
    )?;

    // Now this installation holds the key, wrap it to its own public key so later starts do
    // not need the code again.
    let content_key = e2ee_session::content_key_bytes().ok_or("Unlocking did not take")?;
    let wrap = e2ee::wrap_to_device(&keypair.public_bytes(), &content_key, &user_id, server.epoch)?;

    crate::sync::e2ee_post(
        &settings_state,
        "/e2ee/keys",
        serde_json::json!({
            "byDeviceId": device_id,
            "epoch": server.epoch,
            "id": uuid::Uuid::new_v4().to_string(),
            "kind": "device",
            "deviceId": device_id,
            "wrapAlg": "hpke-x25519-hkdfsha256-chachapoly",
            "wrappedKey": wrap,
        }),
    )
    .await?;

    Ok(())
}

/// Record that the user has written the recovery code down.
///
/// The server refuses to convert anything until this lands, which is the point: a corpus
/// encrypted under a key nobody wrote down is one lost machine from being unreadable.
#[tauri::command]
pub async fn e2ee_acknowledge_recovery(
    settings_state: State<'_, Arc<SettingsState>>,
) -> Result<(), String> {
    let recovery = crate::sync::e2ee_get(&settings_state, "/e2ee/recovery").await?;
    let id = recovery["id"]
        .as_str()
        .ok_or("This account has no recovery code on file")?;

    crate::sync::e2ee_post(
        &settings_state,
        "/e2ee/recovery/ack",
        serde_json::json!({ "id": id }),
    )
    .await?;
    Ok(())
}

/// Convert everything this installation holds.
///
/// Not a special code path: it marks every local record as needing a push and lets the
/// ordinary sync machinery carry them, which already handles partial failure, backoff and
/// edits arriving mid-flight. Resumable for the same reason - a record that made it keeps
/// its marker and is not sent again.
#[tauri::command]
pub async fn e2ee_start_conversion(db_state: State<'_, Arc<DbState>>) -> Result<usize, String> {
    let db = db_state.lock_db();
    let marked = db
        .mark_everything_pending()
        .map_err(|e| format!("Could not queue the conversion: {}", e))?;
    log::info!("Queued {} records for encryption", marked);
    Ok(marked)
}

/// Tell the server the conversion is finished. It verifies before believing it.
#[tauri::command]
pub async fn e2ee_seal(settings_state: State<'_, Arc<SettingsState>>) -> Result<(), String> {
    crate::sync::e2ee_post(&settings_state, "/e2ee/seal", serde_json::json!({})).await?;
    Ok(())
}
