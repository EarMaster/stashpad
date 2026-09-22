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

//! The device passphrase, for machines with no OS credential store.
//!
//! On Windows, macOS and any desktop Linux running gnome-keyring or kwallet, none of this
//! runs: [`crate::keychain::probe_keychain`] reports `Working` and secrets go straight to
//! the platform store. This module exists for the machine that has none - a headless box,
//! a minimal window manager - where the only previous option was
//! [`crate::keychain::derive_machine_key`], whose inputs are the hostname, the app
//! directory and a constant in public source. Anyone who can read the file can rebuild
//! that key, so it protects nothing.
//!
//! What replaces it is a passphrase the user types. Argon2id stretches it into a 32-byte
//! device-protection key (the DKPK), and that key wraps the secrets in `settings.json` -
//! and, once there is one, the device key that end-to-end encryption introduces.
//!
//! The indirection through a DKPK rather than sealing with the passphrase directly is
//! what lets "remember on this machine" store something less dangerous than the passphrase
//! itself: see [`remember_key`].

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::state::lock_or_recover;
use crate::utils::get_app_dir;
use crate::uierror::UiError;

/// Proves a passphrase is the right one without storing anything that reveals it.
const VERIFIER_PLAINTEXT: &[u8] = b"stashpad-device-key-v1";

/// Where the passphrase is set up, and whether it has been entered this session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LocalKeyStatus {
    /// The OS credential store works here, so no passphrase is involved at all.
    NotNeeded,
    /// No credential store and no passphrase yet - the user has not chosen.
    Unset,
    /// A passphrase is configured but has not been entered since the app started.
    Locked,
    /// The key is in memory and secrets can be read and written.
    Unlocked,
    /// The user declined a passphrase. Secrets are not persisted on this machine.
    Declined,
}

/// Argon2id parameters, recorded per installation so they can be raised later without
/// stranding a key derived under the old ones.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct KeyFile {
    version: u32,
    kdf: String,
    /// Memory cost in KiB.
    m_cost: u32,
    /// Iterations.
    t_cost: u32,
    /// Parallelism.
    p_cost: u32,
    salt: String,
    /// Sealed [`VERIFIER_PLAINTEXT`], so a wrong passphrase is rejected immediately
    /// rather than surfacing later as an unreadable secret.
    verifier: String,
    /// Set when the user asked to be remembered, so the UI can say so without having to
    /// guess from the presence of a file.
    remembered: bool,
}

/// Defaults chosen to land near half a second on an unremarkable laptop. Benchmark before
/// raising them: the cost is paid at every cold start on the machines that use this path.
const DEFAULT_M_COST: u32 = 65_536; // 64 MiB
const DEFAULT_T_COST: u32 = 3;
const DEFAULT_P_COST: u32 = 1;

static LOCAL_KEY: Mutex<Option<Zeroizing<[u8; 32]>>> = Mutex::new(None);
static DECLINED: Mutex<bool> = Mutex::new(false);

fn key_file_path() -> PathBuf {
    get_app_dir().join("local_key.json")
}

/// The remembered DKPK, wrapped by the machine key.
///
/// This file is the whole of what "remember on this machine" costs, and it is why the
/// checkbox has to say what it says: the machine key is derivable by anything that can
/// read the folder, so whatever is in here is recoverable by anything that can read the
/// folder.
fn remembered_path() -> PathBuf {
    get_app_dir().join("local_key.remembered")
}

fn read_key_file() -> Option<KeyFile> {
    let text = fs::read_to_string(key_file_path()).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_key_file(file: &KeyFile) -> Result<(), UiError> {
    let path = key_file_path();
    let temp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    fs::write(&temp, text).map_err(|e| e.to_string())?;
    fs::rename(&temp, &path).map_err(|e| e.to_string().into())
}

/// Whether a passphrase has been configured on this machine.
pub fn is_configured() -> bool {
    read_key_file().is_some()
}

/// Stretch a passphrase into the 32-byte device-protection key.
fn derive(passphrase: &str, salt: &[u8], m_cost: u32, t_cost: u32, p_cost: u32) -> Result<Zeroizing<[u8; 32]>, UiError> {
    use argon2::{Algorithm, Argon2, Params, Version};

    let params = Params::new(m_cost, t_cost, p_cost, Some(32))
        .map_err(|e| format!("Invalid key derivation parameters: {}", e))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut key = Zeroizing::new([0u8; 32]);
    argon
        .hash_password_into(passphrase.as_bytes(), salt, key.as_mut())
        .map_err(|e| format!("Could not derive the device key: {}", e))?;
    Ok(key)
}

/// Seal bytes under a 32-byte key: `nonce(12) || ciphertext||tag`, base64.
fn seal_with(key: &[u8; 32], plaintext: &[u8]) -> Result<String, UiError> {
    use aes_gcm::{
        aead::{Aead, KeyInit},
        Aes256Gcm, Nonce,
    };

    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plaintext)
        .map_err(|_| "Encryption failed".to_string())?;

    let mut out = Vec::with_capacity(12 + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(STANDARD.encode(&out))
}

/// Open something [`seal_with`] produced. `None` when the key is wrong or the value is not
/// intact - never a partial or guessed result.
fn open_with(key: &[u8; 32], encoded: &str) -> Option<Vec<u8>> {
    use aes_gcm::{
        aead::{Aead, KeyInit},
        Aes256Gcm, Nonce,
    };

    let data = STANDARD.decode(encoded).ok()?;
    if data.len() < 13 {
        return None;
    }
    let (nonce_bytes, ciphertext) = data.split_at(12);
    let cipher = Aes256Gcm::new_from_slice(key).ok()?;
    cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
        .ok()
}

/// Seal a secret for `settings.json` under the unlocked device key.
pub fn seal_secret(plaintext: &str) -> Option<String> {
    let guard = lock_or_recover(&LOCAL_KEY);
    let key = guard.as_ref()?;
    seal_with(key, plaintext.as_bytes()).ok()
}

/// Open a secret sealed by [`seal_secret`].
pub fn open_secret(encoded: &str) -> Option<String> {
    let guard = lock_or_recover(&LOCAL_KEY);
    let key = guard.as_ref()?;
    let bytes = open_with(key, encoded)?;
    String::from_utf8(bytes).ok()
}

/// Whether the device key is in memory right now.
pub fn is_unlocked() -> bool {
    lock_or_recover(&LOCAL_KEY).is_some()
}

/// Configure a passphrase for this installation.
///
/// `remember` writes the derived key to disk wrapped by the machine key, so later starts
/// do not prompt - at the cost the checkbox describes.
pub fn set_passphrase(passphrase: &str, remember: bool) -> Result<(), UiError> {
    if passphrase.trim().is_empty() {
        return Err(UiError::new(
            "localkey.passphrase_empty",
            "The passphrase cannot be empty",
        ));
    }

    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);

    let key = derive(passphrase, &salt, DEFAULT_M_COST, DEFAULT_T_COST, DEFAULT_P_COST)?;
    let verifier = seal_with(&key, VERIFIER_PLAINTEXT)?;

    write_key_file(&KeyFile {
        version: 1,
        kdf: "argon2id".to_string(),
        m_cost: DEFAULT_M_COST,
        t_cost: DEFAULT_T_COST,
        p_cost: DEFAULT_P_COST,
        salt: STANDARD.encode(salt),
        verifier,
        remembered: remember,
    })?;

    if remember {
        remember_key(&key)?;
    } else {
        forget_remembered();
    }

    *lock_or_recover(&LOCAL_KEY) = Some(key);
    *lock_or_recover(&DECLINED) = false;
    Ok(())
}

/// Enter an existing passphrase. `Ok(false)` means it was simply wrong.
pub fn unlock(passphrase: &str) -> Result<bool, UiError> {
    let Some(file) = read_key_file() else {
        return Err(UiError::new(
            "localkey.no_passphrase",
            "No passphrase is set on this machine",
        ));
    };
    let salt = STANDARD
        .decode(&file.salt)
        .map_err(|_| "The passphrase file is damaged".to_string())?;

    let key = derive(passphrase, &salt, file.m_cost, file.t_cost, file.p_cost)?;
    if open_with(&key, &file.verifier).as_deref() != Some(VERIFIER_PLAINTEXT) {
        return Ok(false);
    }

    *lock_or_recover(&LOCAL_KEY) = Some(key);
    Ok(true)
}

/// Store the derived key, wrapped by the machine key, so startup does not prompt.
///
/// Deliberately the **derived key and not the passphrase**. People reuse passphrases, so a
/// leaked one can cost them something well outside Stashpad; a leaked device key costs
/// this installation and nothing else, and revoking the installation settles it.
fn remember_key(key: &[u8; 32]) -> Result<(), UiError> {
    let wrapped = seal_with(&crate::keychain::derive_machine_key(), key.as_slice())?;
    fs::write(remembered_path(), wrapped).map_err(|e| e.to_string().into())
}

/// Remove the remembered key. Called when the user unticks the box, on cloud logout, and
/// whenever the passphrase is replaced.
pub fn forget_remembered() {
    let _ = fs::remove_file(remembered_path());
    if let Some(mut file) = read_key_file() {
        if file.remembered {
            file.remembered = false;
            let _ = write_key_file(&file);
        }
    }
}

/// Put this installation's key in memory from the OS credential store.
///
/// The key is 32 random bytes generated once and kept in the store, base64 in the entry
/// because a credential entry holds a string. It is not derived from anything about the
/// machine: `derive_machine_key` exists for the one opt-in case where the user asked for a
/// recoverable copy, and using it here would mean the device key file could be opened by
/// anyone who could read the folder, which is the whole thing the credential store avoids.
///
/// Returns false if the store cannot be read or written, which the caller treats as an error
/// rather than a reason to store the key less safely.
fn load_or_create_machine_key() -> bool {
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    if let Some(existing) = crate::keychain::get_secret_from_keychain(
        crate::keychain::create_local_key_entry,
    ) {
        if let Ok(bytes) = STANDARD.decode(existing.trim()) {
            if bytes.len() == 32 {
                let mut key = Zeroizing::new([0u8; 32]);
                key.copy_from_slice(&bytes);
                *lock_or_recover(&LOCAL_KEY) = Some(key);
                return true;
            }
        }
        // A malformed entry is not something to silently replace: the device key file was
        // sealed with whatever the old value was, so overwriting it strands that file.
        crate::keychain::early_log(
            log::Level::Error,
            "The local key in the credential store is not 32 bytes; refusing to replace it".to_string(),
        );
        return false;
    }

    let mut key = Zeroizing::new([0u8; 32]);
    {
        use rand::RngCore;
        rand::thread_rng().fill_bytes(key.as_mut_slice());
    }

    if !crate::keychain::store_secret_in_keychain(
        crate::keychain::create_local_key_entry,
        || {},
        &STANDARD.encode(key.as_slice()),
    ) {
        return false;
    }

    *lock_or_recover(&LOCAL_KEY) = Some(key);
    crate::keychain::early_log(
        log::Level::Info,
        "Created this installation's local key in the credential store".to_string(),
    );
    true
}

/// Try to unlock from the remembered key, without prompting. Returns whether it worked.
fn try_remembered() -> bool {
    let Ok(wrapped) = fs::read_to_string(remembered_path()) else {
        return false;
    };
    let Some(bytes) = open_with(&crate::keychain::derive_machine_key(), wrapped.trim()) else {
        // Usually means the folder moved or the machine was renamed, which changes the
        // machine key. The passphrase still works, so ask for it rather than failing.
        crate::keychain::early_log(
            log::Level::Warn,
            "The remembered device key could not be opened; asking for the passphrase".to_string(),
        );
        return false;
    };
    if bytes.len() != 32 {
        return false;
    }
    let mut key = Zeroizing::new([0u8; 32]);
    key.copy_from_slice(&bytes);

    // A remembered key still has to match the verifier: an unopenable secret later is a
    // much worse way to discover the file is stale.
    match read_key_file() {
        Some(file) if open_with(&key, &file.verifier).as_deref() == Some(VERIFIER_PLAINTEXT) => {
            *lock_or_recover(&LOCAL_KEY) = Some(key);
            true
        }
        _ => false,
    }
}

/// Record that the user does not want a passphrase on this machine.
///
/// Secrets are then not written to disk at all: the app works offline, which it does by
/// design, and the provider key is asked for again next launch. Storing it under a key
/// anyone can rebuild is not protection, so this says no rather than pretending.
pub fn decline() {
    *lock_or_recover(&DECLINED) = true;
    *lock_or_recover(&LOCAL_KEY) = None;
    let _ = fs::remove_file(key_file_path());
    forget_remembered();
}

/// Called once at startup, after the credential-store probe.
pub fn initialize() -> LocalKeyStatus {
    if crate::keychain::keychain_status() == crate::keychain::KeychainStatus::Working {
        // Load this installation's key from the credential store, creating it on first run.
        //
        // Without this the store being healthy meant LOCAL_KEY was simply never populated,
        // because the only other writers are the three passphrase paths. `seal_secret` then
        // returned None and enrolment failed with "there is nowhere safe on this machine to
        // keep an encryption key" - on precisely the machines that have somewhere safe.
        // Encryption worked only where a passphrase had been set, the exact inverse of what
        // was intended.
        if load_or_create_machine_key() {
            return LocalKeyStatus::NotNeeded;
        }
        // The probe said the store works, so a failure here is an error rather than a cue to
        // write the key somewhere weaker. Fall through and let the passphrase path offer
        // itself, which at least tells the user something is wrong.
        crate::keychain::early_log(
            log::Level::Error,
            "The credential store passed its probe but would not hold the local key".to_string(),
        );
    }
    if !is_configured() {
        return LocalKeyStatus::Unset;
    }
    if try_remembered() {
        crate::keychain::early_log(
            log::Level::Info,
            "Device key restored from the remembered copy on this machine".to_string(),
        );
        return LocalKeyStatus::Unlocked;
    }
    LocalKeyStatus::Locked
}

/// The current state, for the UI.
pub fn status() -> LocalKeyStatus {
    // `NotNeeded` means "the credential store is holding the key", so it has to test that
    // the key is actually in hand and not just that the probe passed. `initialize` can
    // find a healthy store that then refuses to hold the key; reporting `NotNeeded` there
    // told the interface everything was fine while every attempt to seal a secret failed,
    // and offered no passphrase as a way out.
    if crate::keychain::keychain_status() == crate::keychain::KeychainStatus::Working
        && is_unlocked()
    {
        return LocalKeyStatus::NotNeeded;
    }
    if is_unlocked() {
        return LocalKeyStatus::Unlocked;
    }
    if *lock_or_recover(&DECLINED) {
        return LocalKeyStatus::Declined;
    }
    if is_configured() {
        LocalKeyStatus::Locked
    } else {
        LocalKeyStatus::Unset
    }
}

/// Whether the passphrase is remembered on this machine, for the settings line.
pub fn is_remembered() -> bool {
    read_key_file().is_some_and(|f| f.remembered)
}

// ---------------------------------------------------------------------------------------
// Tauri commands
//
// Every one that derives a key is `async` and goes through `spawn_blocking`: Argon2id is
// deliberately expensive, and running it inline on a command would park a Tokio worker for
// half a second - the same failure the credential store used to cause on every keystroke.
// ---------------------------------------------------------------------------------------

#[tauri::command]
pub fn local_key_status() -> LocalKeyStatus {
    status()
}

#[tauri::command]
pub fn local_key_is_remembered() -> bool {
    is_remembered()
}

#[tauri::command]
pub async fn unlock_local_key(passphrase: String) -> Result<bool, UiError> {
    tauri::async_runtime::spawn_blocking(move || unlock(&passphrase))
        .await
        .map_err(|e| format!("Could not run the key derivation: {}", e))?
}

/// Set or replace the passphrase, then re-save settings so the secrets already held in
/// memory are written back sealed under the new key.
#[tauri::command]
pub async fn set_local_passphrase(
    passphrase: String,
    remember: bool,
    state: tauri::State<'_, std::sync::Arc<crate::state::SettingsState>>,
) -> Result<(), UiError> {
    tauri::async_runtime::spawn_blocking(move || set_passphrase(&passphrase, remember))
        .await
        .map_err(|e| format!("Could not run the key derivation: {}", e))??;

    let settings = state.lock_settings().clone();
    crate::settings::persist_settings_off_thread(settings).await;
    Ok(())
}

/// Turn "remember on this machine" on or off for an already-configured passphrase.
///
/// Turning it on needs the key, which means the session must already be unlocked - there
/// is nothing to remember otherwise.
#[tauri::command]
pub fn set_local_key_remembered(remember: bool) -> Result<(), UiError> {
    if !remember {
        forget_remembered();
        return Ok(());
    }

    let guard = lock_or_recover(&LOCAL_KEY);
    let Some(key) = guard.as_ref() else {
        return Err(UiError::new(
            "localkey.locked",
            "Enter your passphrase first",
        ));
    };
    remember_key(key)?;
    drop(guard);

    if let Some(mut file) = read_key_file() {
        file.remembered = true;
        write_key_file(&file)?;
    }
    Ok(())
}

/// The user does not want a passphrase on this machine.
#[tauri::command]
pub async fn decline_local_key(
    state: tauri::State<'_, std::sync::Arc<crate::state::SettingsState>>,
) -> Result<(), UiError> {
    decline();
    // Rewrite settings so anything previously held under the machine key stops being
    // written back. The secrets stay usable in memory for this session.
    let settings = state.lock_settings().clone();
    crate::settings::persist_settings_off_thread(settings).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    #[test]
    fn a_secret_round_trips_under_a_key() {
        let k = key(7);
        let sealed = seal_with(&k, b"sk_stashpad_value").expect("seal");
        assert_eq!(open_with(&k, &sealed).as_deref(), Some(&b"sk_stashpad_value"[..]));
    }

    #[test]
    fn the_wrong_key_opens_nothing() {
        let sealed = seal_with(&key(7), b"sk_stashpad_value").expect("seal");
        assert_eq!(open_with(&key(8), &sealed), None);
    }

    #[test]
    fn a_tampered_value_opens_nothing() {
        let sealed = seal_with(&key(7), b"sk_stashpad_value").expect("seal");
        let mut raw = STANDARD.decode(&sealed).expect("base64");
        let last = raw.len() - 1;
        raw[last] ^= 0xff;
        assert_eq!(open_with(&key(7), &STANDARD.encode(&raw)), None);
    }

    #[test]
    fn a_truncated_value_opens_nothing() {
        assert_eq!(open_with(&key(7), &STANDARD.encode(b"short")), None);
    }

    /// The same passphrase and salt must always give the same key, or a remembered key and
    /// a typed one would disagree.
    #[test]
    fn derivation_is_deterministic() {
        let salt = [3u8; 16];
        let a = derive("correct horse battery staple", &salt, 8, 1, 1).expect("derive");
        let b = derive("correct horse battery staple", &salt, 8, 1, 1).expect("derive");
        assert_eq!(a.as_slice(), b.as_slice());
    }

    #[test]
    fn a_different_salt_gives_a_different_key() {
        let a = derive("correct horse battery staple", &[3u8; 16], 8, 1, 1).expect("derive");
        let b = derive("correct horse battery staple", &[4u8; 16], 8, 1, 1).expect("derive");
        assert_ne!(a.as_slice(), b.as_slice());
    }

    #[test]
    fn a_different_passphrase_gives_a_different_key() {
        let salt = [3u8; 16];
        let a = derive("correct horse battery staple", &salt, 8, 1, 1).expect("derive");
        let b = derive("correct horse battery stapler", &salt, 8, 1, 1).expect("derive");
        assert_ne!(a.as_slice(), b.as_slice());
    }

    /// The verifier is what turns a wrong passphrase into an immediate "that is not it"
    /// rather than an unreadable secret discovered much later.
    #[test]
    fn the_verifier_accepts_only_the_right_key() {
        let right = derive("right", &[1u8; 16], 8, 1, 1).expect("derive");
        let wrong = derive("wrong", &[1u8; 16], 8, 1, 1).expect("derive");
        let verifier = seal_with(&right, VERIFIER_PLAINTEXT).expect("seal");

        assert_eq!(
            open_with(&right, &verifier).as_deref(),
            Some(VERIFIER_PLAINTEXT)
        );
        assert_eq!(open_with(&wrong, &verifier), None);
    }
}
