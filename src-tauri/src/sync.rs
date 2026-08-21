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

use std::sync::Arc;
use std::time::Duration;
use tauri::State;
use rusqlite::params;
use rusqlite::OptionalExtension;
use std::fs;
use crate::models::{CloudConfig, Attachment, default_cloud_endpoint};
use crate::state::{SettingsState, DbState, WsState, lock_or_recover};
use crate::settings::persist_settings_off_thread;
use crate::utils::get_app_dir;

/// How long a JSON API call may take before it is abandoned.
///
/// Every HTTP client here used to be a bare `reqwest::Client::new()`, which has no
/// timeout at all. Combined with the `isSyncing` guard in cloud-sync.ts, a single
/// stalled socket wedged *all* syncing - stashes and contexts included - until the OS
/// gave up on the connection, which reads to the user as the app hanging.
const API_TIMEOUT: Duration = Duration::from_secs(30);

/// Attachment transfers get a longer ceiling: they move whole files, not small JSON
/// bodies, and the presigned URL they use is valid for an hour.
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(300);

/// Connect phase is bounded separately - an unreachable host should fail fast even when
/// the overall budget is generous.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// How much of a failed response body to quote back in the error message.
const ERROR_SNIPPET_CHARS: usize = 100;

/// Quote the start of an error body, counting characters rather than bytes.
///
/// This used to be `&body[..100]`, which panics with "byte index 100 is not a char
/// boundary" whenever byte 100 lands inside a multi-byte character - an umlaut in a
/// German proxy page, an emoji echoed back from stash content, a typographic
/// apostrophe in an HTML 500. Tauri does not `catch_unwind` command bodies, so that
/// panic dropped the IPC responder without answering and the webview's `await
/// invoke(...)` never settled. `cloud-sync.ts` therefore never ran its
/// `finally { isSyncing = false }`, and its own guard refused every later sync for
/// the rest of the session: sync died silently and stayed dead until a restart.
fn error_snippet(body: &str) -> String {
    let snippet: String = body.chars().take(ERROR_SNIPPET_CHARS).collect();
    if body.chars().nth(ERROR_SNIPPET_CHARS).is_some() {
        format!("{}…", snippet)
    } else {
        snippet
    }
}

/// Client for JSON API calls.
fn api_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(API_TIMEOUT)
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))
}

/// Client for uploading and downloading attachment bytes.
fn transfer_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(TRANSFER_TIMEOUT)
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))
}

/// Fetch account info from cloud service and update local subscription status
#[tauri::command]
pub async fn fetch_cloud_account(
    settings_state: State<'_, Arc<SettingsState>>,
) -> Result<CloudConfig, String> {
    let (endpoint, token) = {
        let settings = settings_state.lock_settings();
        let config = settings.cloud_config.as_ref().ok_or("Cloud config missing")?;
        let token = config.access_token.clone().ok_or("Not authenticated")?;
        (config.endpoint.clone(), token)
    };

    let client = api_client()?;
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

    // Update local config with subscription info. The lock is dropped before the write:
    // holding it across the credential store and the settings file blocked every other
    // command that reads settings, each one parking its own Tokio worker.
    let (updated_config, snapshot) = {
        let mut settings = settings_state.lock_settings();
        let Some(ref mut config) = settings.cloud_config else {
            return Err("Cloud config not found".into());
        };
        config.subscription_tier = account["subscriptionTier"].as_str().map(|s| s.to_string());
        config.subscription_status = account["subscriptionStatus"].as_str().map(|s| s.to_string());
        config.subscription_period_end = account["subscriptionPeriodEnd"].as_str().map(|s| s.to_string());
        config.enterprise_owner_id = account["enterpriseOwnerId"].as_str().map(|s| s.to_string());

        (config.clone(), settings.clone())
    };

    persist_settings_off_thread(snapshot).await;

    let mut return_config = updated_config;
    return_config.access_token = None;
    Ok(return_config)
}

#[tauri::command]
pub async fn exchange_link_code_api(
    settings_state: State<'_, Arc<SettingsState>>,
    token: String,
    device_id: Option<String>,
) -> Result<CloudConfig, String> {
    let endpoint = {
        let settings = settings_state.lock_settings();
        let config = settings.cloud_config.as_ref().ok_or("Cloud config missing")?;
        config.endpoint.clone()
    };

    let client = api_client()?;
    let response = client
        .post(format!("{}/auth/exchange-token", endpoint.trim_end_matches('/')))
        .header("Content-Type", "application/json")
        // The device id ties the issued token to this installation, so the account page
        // can revoke this instance on its own instead of every session at once.
        .json(&serde_json::json!({ "token": token, "device_id": device_id }))
        .send()
        .await
        .map_err(|e| format!("Failed to reach server: {}", e))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();

        let message = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
            json["error"]
                .as_str()
                .or_else(|| json["message"].as_str())
                .unwrap_or("Unknown server error")
                .to_string()
        } else if body.trim_start().starts_with('<') || body.is_empty() {
            format!("Server returned an error (HTTP {})", status)
        } else {
            body
        };

        return Err(message);
    }

    let data: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let access_token_val = data["token"]
        .as_str()
        .ok_or("Missing token in response")?
        .to_string();

    let user_id_val = data["user_id"]
        .as_str()
        .map(|s| s.to_string());

    // Same as above: mutate under the lock, then release it before the blocking write.
    let (config, snapshot) = {
        let mut settings = settings_state.lock_settings();
        let mut config = settings.cloud_config.clone().unwrap_or(CloudConfig {
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

        config.access_token = Some(access_token_val);
        config.user_id = user_id_val;
        config.enabled = true;

        settings.cloud_config = Some(config.clone());
        (config, settings.clone())
    };

    persist_settings_off_thread(snapshot).await;

    let mut return_config = config;
    return_config.access_token = None;
    Ok(return_config)
}


#[tauri::command]
pub async fn sync_stashes_api(
    settings_state: State<'_, Arc<SettingsState>>,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let (endpoint, token) = {
        let settings = settings_state.lock_settings();
        let config = settings.cloud_config.as_ref().ok_or("Cloud config missing")?;
        let token = config.access_token.clone().ok_or("Not authenticated")?;
        (config.endpoint.clone(), token)
    };

    let client = api_client()?;
    let response = client
        .post(format!("{}/sync/stashes", endpoint.trim_end_matches('/')))
        .header("Authorization", format!("Bearer {}", token))
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("Failed to sync stashes: {}", e))?;

    // Match the wording sync_contexts_api uses so the frontend can detect an expired
    // session from either call and surface the "log in again" path.
    if response.status() == 401 {
        return Err("Authentication expired. Please log in again.".into());
    }

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let snippet = error_snippet(&body);
        return Err(format!("Stash sync failed ({}): {}", status, snippet));
    }

    response.json().await.map_err(|e| format!("Failed to parse sync response: {}", e))
}

#[tauri::command]
/// Push an attachment's bytes to the cloud.
///
/// Returns `true` only when bytes were actually uploaded, so the caller can schedule a
/// follow-up sync: confirming an upload publishes the file server-side but sends no
/// WebSocket notification of its own, and other devices would otherwise not hear about
/// it until the next fallback poll.
pub async fn upload_attachment_to_cloud(
    state: State<'_, Arc<DbState>>,
    settings_state: State<'_, Arc<SettingsState>>,
    attachment_id: String,
) -> Result<bool, String> {
    let (endpoint, token) = {
        let settings = settings_state.lock_settings();
        let config = settings.cloud_config.as_ref().ok_or("Cloud config missing")?;
        let token = config.access_token.clone().ok_or("Not authenticated")?;
        (config.endpoint.clone(), token)
    };

    let (attachment, already_uploaded) = {
        let db = state.lock_db();
        let mut stmt = db.conn.prepare("SELECT id, stash_id, file_path, file_name, file_size, mime_type, syntax, created_at, uploaded_at FROM attachments WHERE id = ?")
            .map_err(|e| e.to_string())?;
        stmt.query_row(params![attachment_id], |row| {
            let uploaded_at: Option<u64> = row.get(8)?;
            Ok((
                Attachment {
                    id: row.get(0)?,
                    stash_id: row.get(1)?,
                    file_path: row.get(2)?,
                    file_name: row.get(3)?,
                    file_size: row.get(4)?,
                    mime_type: row.get(5)?,
                    syntax: row.get(6)?,
                    created_at: row.get(7)?,
                },
                uploaded_at.is_some(),
            ))
        }).optional().map_err(|e| e.to_string())?
            .ok_or_else(|| "Attachment not found".to_string())?
    };

    // Idempotency: without this every sync re-PUT the full body of every attachment
    // that had ever been created.
    if already_uploaded {
        return Ok(false);
    }

    // Attachments pulled from another device arrive as metadata only until their bytes
    // are downloaded, so there is nothing local to push yet.
    if attachment.file_path.trim().is_empty() {
        return Ok(false);
    }

    // The file is recorded but gone from disk - deleting a stash removes its cache
    // folder while leaving the attachment rows behind. Clear the stale path instead of
    // failing: it stops this row claiming to hold bytes it does not have, and stops the
    // upload being retried on every single sync forever.
    if !std::path::Path::new(&attachment.file_path).exists() {
        log::warn!(
            "[Attachment] {} references a file that no longer exists: {}",
            attachment.id,
            attachment.file_path
        );
        let db = state.lock_db();
        db.conn
            .execute(
                "UPDATE attachments SET file_path = '' WHERE id = ?1",
                params![attachment.id],
            )
            .map_err(|e| e.to_string())?;
        return Ok(false);
    }

    let client = transfer_client()?;
    
    // 1. Get presigned upload URL from cloud
    let upload_req = serde_json::json!({
        "id": attachment.id,
        "stashId": attachment.stash_id,
        "fileName": attachment.file_name,
        "fileSize": attachment.file_size,
        "mimeType": attachment.mime_type,
        "syntax": attachment.syntax,
    });

    let upload_url_resp = client
        .post(format!("{}/attachments/upload", endpoint.trim_end_matches('/')))
        .header("Authorization", format!("Bearer {}", token))
        .json(&upload_req)
        .send()
        .await
        .map_err(|e| format!("Failed to get upload URL: {}", e))?;

    let status = upload_url_resp.status();
    let resp_text = upload_url_resp.text().await
        .map_err(|e| format!("Failed to read upload URL response: {}", e))?;

    if !status.is_success() {
        let msg = format!("Cloud rejected upload request for {}: {} - {}", attachment.id, status, resp_text);
        log::error!("[Attachment] {}", msg);
        return Err(msg);
    }

    let upload_data: serde_json::Value = serde_json::from_str(&resp_text)
        .map_err(|e| format!("Failed to parse upload URL response: {}", e))?;
    
    let upload_url = upload_data["uploadUrl"].as_str()
        .ok_or_else(|| "No upload URL in response".to_string())?;

    // 2. Read file content, on the blocking pool. Attachments are whole files -
    // screenshots and logs - so reading one inline parks an async worker for the
    // duration of the disk read.
    let file_to_read = attachment.file_path.clone();
    let file_content = tauri::async_runtime::spawn_blocking(move || fs::read(&file_to_read))
        .await
        .map_err(|e| format!("Attachment read task failed: {}", e))?
        .map_err(|e| {
            let msg = format!("Failed to read attachment file {}: {}", attachment.file_path, e);
            log::error!("[Attachment] {}", msg);
            msg
        })?;

    // 3. PUT file to R2
    let put_resp = client
        .put(upload_url)
        .header("Content-Type", attachment.mime_type.unwrap_or_else(|| "application/octet-stream".to_string()))
        .body(file_content)
        .send()
        .await
        .map_err(|e| format!("Failed to upload file to storage: {}", e))?;

    if !put_resp.status().is_success() {
        let msg = format!("Storage rejected the file for {}: {}", attachment.id, put_resp.status());
        log::error!("[Attachment] {}", msg);
        return Err(msg);
    }

    // 4. Confirm the upload. Until this lands the server keeps the row invisible to
    // sync, so no other device is shown a file whose bytes are not in storage yet.
    let confirm_resp = client
        .post(format!(
            "{}/attachments/{}/confirm",
            endpoint.trim_end_matches('/'),
            attachment.id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Failed to confirm upload: {}", e))?;

    if !confirm_resp.status().is_success() {
        let status = confirm_resp.status();
        let body = confirm_resp.text().await.unwrap_or_default();
        let msg = format!(
            "Cloud rejected upload confirmation for {}: {} - {}",
            attachment.id, status, body
        );
        log::error!("[Attachment] {}", msg);
        return Err(msg);
    }

    log::info!("[Attachment] Uploaded {} ({})", attachment.id, attachment.file_name);

    // 5. Record locally so we never re-upload these bytes.
    {
        let db = state.lock_db();
        db.conn
            .execute(
                "UPDATE attachments SET uploaded_at = ?2 WHERE id = ?1",
                params![attachment.id, crate::db::now_ts()],
            )
            .map_err(|e| e.to_string())?;
    }

    Ok(true)
}

/// Download an attachment's bytes from the cloud into the local cache.
///
/// Sync previously only ever uploaded. A device receiving a stash got attachment
/// metadata with an empty `file_path`, so the UI showed a file that could never be
/// opened - and then tried to re-upload it, failing on `fs::read("")` every cycle.
#[tauri::command]
pub async fn download_attachment_from_cloud(
    state: State<'_, Arc<DbState>>,
    settings_state: State<'_, Arc<SettingsState>>,
    attachment_id: String,
) -> Result<String, String> {
    let (endpoint, token) = {
        let settings = settings_state.lock_settings();
        let config = settings.cloud_config.as_ref().ok_or("Cloud config missing")?;
        let token = config.access_token.clone().ok_or("Not authenticated")?;
        (config.endpoint.clone(), token)
    };

    // Resolve where this file belongs: cache/<context_id>/<stash_id>/<file_name>
    let (file_name, stash_id, context_id, existing_path) = {
        let db = state.lock_db();
        let mut stmt = db
            .conn
            .prepare(
                "SELECT a.file_name, a.stash_id, s.context_id, a.file_path \
                 FROM attachments a LEFT JOIN stashes s ON s.id = a.stash_id \
                 WHERE a.id = ?",
            )
            .map_err(|e| e.to_string())?;
        stmt.query_row(params![attachment_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Attachment not found".to_string())?
    };

    // Already present locally - nothing to fetch.
    if !existing_path.trim().is_empty() && std::path::Path::new(&existing_path).exists() {
        return Ok(existing_path);
    }

    let client = transfer_client()?;

    let meta_resp = client
        .get(format!(
            "{}/attachments/{}",
            endpoint.trim_end_matches('/'),
            attachment_id
        ))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Failed to request download URL: {}", e))?;

    let status = meta_resp.status();
    let body = meta_resp
        .text()
        .await
        .map_err(|e| format!("Failed to read download URL response: {}", e))?;

    if !status.is_success() {
        return Err(format!("Cloud rejected download request: {} - {}", status, body));
    }

    let data: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Failed to parse download response: {}", e))?;
    let download_url = data["downloadUrl"]
        .as_str()
        .ok_or_else(|| "No download URL in response".to_string())?;

    let file_resp = client
        .get(download_url)
        .send()
        .await
        .map_err(|e| format!("Failed to download attachment: {}", e))?;

    if !file_resp.status().is_success() {
        return Err(format!("Attachment download failed: {}", file_resp.status()));
    }

    let bytes = file_resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to read attachment body: {}", e))?;

    // Mirror the layout save_asset uses so cleanup on delete keeps working.
    let mut dir = get_app_dir().join("cache");
    if let Some(cid) = context_id.as_deref() {
        dir = dir.join(cid);
    }
    dir = dir.join(&stash_id);
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create cache dir: {}", e))?;

    let target = dir.join(&file_name);

    // Write to a temporary file and rename into place. A direct write that is
    // interrupted - crash, power loss, full disk - leaves a truncated file at the real
    // path, and every later check only tests whether the path exists, so the corruption
    // would look like a complete download forever. Rename within a directory is atomic.
    // Off the async worker: this writes the whole file to disk.
    let temp = dir.join(format!(".{}.part", attachment_id));
    let write_temp = temp.clone();
    let write_target = target.clone();
    tauri::async_runtime::spawn_blocking(move || {
        fs::write(&write_temp, &bytes)
            .map_err(|e| format!("Failed to write attachment: {}", e))?;
        if let Err(e) = fs::rename(&write_temp, &write_target) {
            let _ = fs::remove_file(&write_temp);
            return Err(format!("Failed to finalise attachment: {}", e));
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("Attachment write task failed: {}", e))??;

    let path_str = target.to_string_lossy().to_string();

    {
        let db = state.lock_db();
        // uploaded_at is set too: the bytes demonstrably already exist in the cloud, so
        // this device must not push them back up.
        db.conn
            .execute(
                "UPDATE attachments SET file_path = ?2, uploaded_at = ?3 WHERE id = ?1",
                params![attachment_id, path_str, crate::db::now_ts()],
            )
            .map_err(|e| e.to_string())?;
    }

    Ok(path_str)
}

#[tauri::command]
pub async fn sync_contexts_api(
    settings_state: State<'_, Arc<SettingsState>>,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let (endpoint, token) = {
        let settings = settings_state.lock_settings();
        let config = settings.cloud_config.as_ref().ok_or("Cloud config missing")?;
        let token = config.access_token.clone().ok_or("Not authenticated")?;
        (config.endpoint.clone(), token)
    };

    let client = api_client()?;
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
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let snippet = error_snippet(&body);
        return Err(format!("Context sync failed ({}): {}", status, snippet));
    }

    response.json().await.map_err(|e| format!("Failed to parse sync response: {}", e))
}

// --- WebSocket Sync Commands ---

#[tauri::command]
pub async fn connect_websocket(
    app: tauri::AppHandle,
    settings_state: State<'_, Arc<SettingsState>>,
    ws_state: State<'_, Arc<WsState>>,
) -> Result<(), String> {
    // End any existing connection
    disconnect_websocket(ws_state.clone()).await?;

    let (endpoint, token, enabled) = {
        let settings = settings_state.lock_settings();
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
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::connect_async;
        use tauri::Emitter;

        // How often to ping the server. Idle WebSockets through a NAT or proxy are
        // dropped silently; without traffic the client believes it is still connected
        // and simply stops receiving sync notifications.
        const PING_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);
        // Ceiling on the handshake itself, so an unreachable or silently dropping host
        // fails fast into the backoff instead of parking the task forever.
        const WS_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
        // A connection that survived this long counts as healthy, so the next drop
        // starts backing off from scratch rather than from wherever it left off.
        const HEALTHY_CONNECTION: std::time::Duration = std::time::Duration::from_secs(60);

        let mut retry_backoff = 1;

        loop {
            log::info!("[WebSocket] Attempting to connect to {}", ws_endpoint);

            // Bounded, unlike the bare `connect_async` this replaces. A black-holed
            // TCP or TLS handshake never returns on its own, so the reconnect loop
            // stopped looping: the app kept believing a connection was pending and
            // fell back to the 15-minute poll, which reads as sync being broken.
            let attempt = tokio::time::timeout(WS_CONNECT_TIMEOUT, connect_async(ws_url.clone()));
            match attempt.await.unwrap_or_else(|_| {
                Err(tokio_tungstenite::tungstenite::Error::Io(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "WebSocket handshake timed out",
                )))
            }) {
                Ok((ws_stream, _)) => {
                    log::info!("[WebSocket] Connected successfully");
                    let connected_at = std::time::Instant::now();

                    // Split so the ping timer can write while the reader is parked on
                    // the next frame - a select! over the unsplit stream would need two
                    // simultaneous mutable borrows.
                    let (mut ws_write, mut ws_read) = ws_stream.split();
                    let mut ping_timer = tokio::time::interval(PING_INTERVAL);
                    ping_timer.tick().await; // the first tick completes immediately

                    loop {
                        tokio::select! {
                            frame = ws_read.next() => {
                                let Some(msg) = frame else {
                                    log::info!("[WebSocket] Stream ended");
                                    break;
                                };
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
                                    _ => {} // Ignore pong/binary/ping
                                }
                            }
                            _ = ping_timer.tick() => {
                                if let Err(e) = ws_write
                                    .send(tokio_tungstenite::tungstenite::Message::Ping(Default::default()))
                                    .await
                                {
                                    log::warn!("[WebSocket] Keepalive ping failed: {}", e);
                                    break;
                                }
                            }
                        }
                    }

                    if connected_at.elapsed() >= HEALTHY_CONNECTION {
                        retry_backoff = 1;
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

    *lock_or_recover(&ws_state.task_handle) = Some(handle);
    Ok(())
}

#[tauri::command]
pub async fn disconnect_websocket(ws_state: State<'_, Arc<WsState>>) -> Result<(), String> {
    if let Some(handle) = lock_or_recover(&ws_state.task_handle).take() {
        log::info!("[WebSocket] Disconnecting client...");
        handle.abort();
    }
    Ok(())
}

/// What this account is storing in the cloud.
///
/// Mirrors the server's `AccountUsage`. Kept as a plain struct rather than reusing the
/// settings types because none of it is persisted: it is fetched on demand when the user
/// opens Settings, and is stale the moment anything syncs.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CloudUsage {
    pub stashes: i64,
    pub contexts: i64,
    pub attachments: i64,
    pub attachment_bytes: i64,
    pub quota_bytes: i64,
    pub over_quota: bool,
}

#[tauri::command]
pub async fn fetch_cloud_usage(
    settings_state: State<'_, Arc<SettingsState>>,
) -> Result<CloudUsage, String> {
    let (endpoint, token) = {
        let settings = settings_state.lock_settings();
        let config = settings.cloud_config.as_ref().ok_or("Cloud config missing")?;
        let token = config.access_token.clone().ok_or("Not authenticated")?;
        (config.endpoint.clone(), token)
    };

    let client = api_client()?;
    let response = client
        .get(format!("{}/account/usage", endpoint.trim_end_matches('/')))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch usage: {}", e))?;

    if response.status() == 401 {
        return Err("Authentication expired. Please log in again.".into());
    }

    if !response.status().is_success() {
        return Err(format!("Failed to fetch usage: {}", response.status()));
    }

    response
        .json()
        .await
        .map_err(|e| format!("Failed to parse usage: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression test for the panic that used to kill sync outright.
    ///
    /// `&body[..100]` panics when byte 100 lands inside a multi-byte character. Because
    /// Tauri does not catch panics in commands, the IPC responder was dropped without
    /// answering, so cloud-sync.ts never cleared `isSyncing` and refused every later
    /// sync for the rest of the session.
    #[test]
    fn an_error_body_is_truncated_on_a_character_boundary() {
        // One ASCII byte followed by two-byte characters puts byte 100 *inside* a
        // character, which is exactly what `&body[..100]` used to panic on. Assert that
        // precondition too, so this test still proves something if the body changes.
        let body = format!("a{}", "ä".repeat(200));
        assert!(
            !body.is_char_boundary(ERROR_SNIPPET_CHARS),
            "test body no longer reproduces the hazard"
        );

        let snippet = error_snippet(&body);

        assert_eq!(snippet.chars().count(), ERROR_SNIPPET_CHARS + 1); // + the ellipsis
        assert!(snippet.ends_with('…'));
        assert!(snippet.starts_with("aä"));
    }

    /// Three-byte characters put most offsets off a boundary, so cover them too.
    #[test]
    fn a_multibyte_error_body_is_truncated_by_characters_not_bytes() {
        let body = "€".repeat(200);
        assert!(!body.is_char_boundary(ERROR_SNIPPET_CHARS));

        let snippet = error_snippet(&body);
        assert_eq!(snippet.chars().filter(|c| *c == '€').count(), ERROR_SNIPPET_CHARS);
    }

    #[test]
    fn a_short_error_body_is_quoted_whole_and_unmarked() {
        let snippet = error_snippet("Bad Gateway");
        assert_eq!(snippet, "Bad Gateway");
    }

    #[test]
    fn an_error_body_exactly_at_the_limit_is_not_marked_as_cut() {
        let body = "a".repeat(ERROR_SNIPPET_CHARS);
        let snippet = error_snippet(&body);
        assert_eq!(snippet, body);
        assert!(!snippet.ends_with('…'));
    }
}
