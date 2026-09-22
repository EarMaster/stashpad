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

use std::sync::{Mutex, OnceLock};

use crate::utils::get_app_dir;

/// Key of the long-retired XOR obfuscation. Reachable from exactly one place -
/// [`decrypt_legacy_secret`], used only by the one-time startup migration - so a secret
/// written by a very old build is still recoverable.
///
/// It is deliberately *not* reachable from [`decrypt_api_key`] any more. That path used to
/// fall through to XOR on **any** AEAD failure, which meant a corrupted or tampered value
/// was silently "decrypted" with a constant compiled into public source.
const LEGACY_OBFUSCATION_KEY: &[u8] = b"StashpadAIConfigKey2026";

/// Keychain identifiers - using explicit target for Windows compatibility
const KEYCHAIN_SERVICE: &str = "stashpad";
const KEYCHAIN_USER: &str = "ai_api_key";
const KEYCHAIN_TARGET: &str = "stashpad.ai_api_key";
/// Keychain identifiers for cloud access token
const KEYCHAIN_CLOUD_USER: &str = "cloud_access_token";
const KEYCHAIN_CLOUD_TARGET: &str = "stashpad.cloud_access_token";

const KEYCHAIN_LOCAL_KEY_USER: &str = "local_key";
const KEYCHAIN_LOCAL_KEY_TARGET: &str = "stashpad.local_key";

/// Create a keychain entry with consistent target across platforms
pub fn create_keychain_entry() -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new_with_target(KEYCHAIN_TARGET, KEYCHAIN_SERVICE, KEYCHAIN_USER)
}

/// Create a keychain entry for the cloud access token
pub fn create_cloud_keychain_entry() -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new_with_target(KEYCHAIN_CLOUD_TARGET, KEYCHAIN_SERVICE, KEYCHAIN_CLOUD_USER)
}

/// The entry holding this installation's local key.
///
/// Separate from the two secrets above because it protects them rather than being one: it is
/// what seals the device key file, and on a machine with a credential store it is the only
/// thing standing between that file and anyone who can read the folder.
pub fn create_local_key_entry() -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new_with_target(KEYCHAIN_LOCAL_KEY_TARGET, KEYCHAIN_SERVICE, KEYCHAIN_LOCAL_KEY_USER)
}

/// Whether this machine has a credential store that actually works.
///
/// Probed once at startup with a canary rather than discovered per write, so the rest of
/// the app can branch on a known fact. The distinction matters: an *absent* store is a
/// machine configuration to work around, while a *failure* on a machine that probed
/// `Working` is an error - and must never be answered by quietly writing the secret
/// somewhere weaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeychainStatus {
    /// A real credential store accepted a value and gave it back.
    Working,
    /// No usable credential store on this machine (headless Linux, no Secret Service).
    Unavailable,
}

/// Set once by [`probe_keychain`] at startup.
///
/// A module-level cell rather than Tauri managed state because the readers are free
/// functions in `settings.rs` that run outside any command context and have no `State`
/// to draw from.
static KEYCHAIN_STATUS: OnceLock<KeychainStatus> = OnceLock::new();

/// What the startup checks said, held until there is somewhere to say it.
///
/// `probe_keychain` and `localkey::initialize` run before `tauri::Builder`, and the log
/// plugin is registered inside it - so every line they emitted went to a logger that did not
/// exist yet and was dropped. That is why a user's log file has never contained a word about
/// the credential store, and why raising those lines from `debug` to `warn` changed nothing.
///
/// They are buffered here and replayed by [`flush_early_diagnostics`] from the setup hook.
static EARLY_DIAGNOSTICS: Mutex<Vec<(log::Level, String)>> = Mutex::new(Vec::new());

/// Record a line that happens before the logger exists.
pub(crate) fn early_log(level: log::Level, message: String) {
    if let Ok(mut buffered) = EARLY_DIAGNOSTICS.lock() {
        // Bounded: a pathological loop must not turn a diagnostic into a leak.
        if buffered.len() < 64 {
            buffered.push((level, message));
        }
    }
}

/// Replay the startup diagnostics, once the log plugin is up.
pub fn flush_early_diagnostics() {
    let Ok(mut buffered) = EARLY_DIAGNOSTICS.lock() else {
        return;
    };
    for (level, message) in buffered.drain(..) {
        log::log!(level, "{}", message);
    }
}

/// Round-trip a canary through the credential store and remember the outcome.
///
/// Call once, early in startup, **before** anything reads a secret. The canary exists so
/// the probe never writes a real secret: a probe that stored the cloud token to find out
/// whether storing works is how you lose the cloud token.
///
/// Its findings go through [`early_log`], because this runs before the log plugin exists.
pub fn probe_keychain() -> KeychainStatus {
    let status = run_probe();
    match status {
        KeychainStatus::Working => {
            early_log(log::Level::Info, "Credential store is available and round-trips".to_string())
        }
        KeychainStatus::Unavailable => early_log(
            log::Level::Warn,
            "No usable credential store on this machine - secrets fall back to an encrypted file"
                .to_string(),
        ),
    }
    let _ = KEYCHAIN_STATUS.set(status);
    status
}

/// Try a real write and read-back, because "is there a credential store" cannot be answered
/// by asking the platform - only by using it.
///
/// Each failure path says which step failed, because the outcome alone ("no usable
/// credential store") is not something anyone can act on - a machine that wrongly falls
/// back to the passphrase prompt is diagnosable only if the reason is visible.
///
/// They go through [`early_log`] rather than `log::` directly. Raising them from `debug`
/// to `warn` was not enough on its own: this whole function runs before the log plugin is
/// registered, so every line went to a logger that did not exist and no level would have
/// saved it.
fn run_probe() -> KeychainStatus {
    // Unique per probe, because the probe deletes what it wrote.
    //
    // With a fixed name, two overlapping probes clobber each other: the updater restarts
    // the app while the previous process is still exiting, so B writes its canary, A's
    // delete removes it, and B's read-back finds nothing and concludes there is no
    // credential store. The app then asks for a device passphrase on a machine whose
    // keychain is fine - and the next ordinary start, with only one process, reports it as
    // working. That is the "it says my key is in the keychain but it asked for the
    // passphrase after an update" pair, and the two halves were from different runs.
    //
    // The cost is an orphaned credential if the process dies between the write and the
    // delete. That is a narrow window and a tiny value with a recognisable name, which is
    // a better trade than intermittently mistaking a working store for a missing one.
    let unique = format!(
        "{}-{:x}",
        std::process::id(),
        rand::random::<u64>()
    );
    let target = format!("stashpad.probe.{}", unique);
    let canary = format!("stashpad-keychain-probe-{}", unique);

    let entry = match keyring::Entry::new_with_target(&target, KEYCHAIN_SERVICE, "keychain_probe") {
        Ok(entry) => entry,
        Err(e) => {
            early_log(log::Level::Warn, format!("Credential store probe could not create an entry: {}", e));
            return KeychainStatus::Unavailable;
        }
    };

    if let Err(e) = entry.set_password(&canary) {
        early_log(log::Level::Warn, format!("Credential store probe could not write: {}", e));
        return KeychainStatus::Unavailable;
    }

    // A second `Entry` on purpose: the mock store keeps its value on the handle, so
    // reading back through the same one would pass against a store that persists nothing.
    // The value is unique per probe as well, so a stale entry cannot stand in for a live
    // write either.
    let readback = keyring::Entry::new_with_target(&target, KEYCHAIN_SERVICE, "keychain_probe")
        .and_then(|verify| verify.get_password());

    let _ = entry.delete_credential();

    match readback {
        Ok(value) if value == canary => KeychainStatus::Working,
        Ok(_) => {
            early_log(log::Level::Warn, "Credential store probe read back a different value".to_string());
            KeychainStatus::Unavailable
        }
        Err(e) => {
            early_log(log::Level::Warn, format!("Credential store probe could not read back: {}", e));
            KeychainStatus::Unavailable
        }
    }
}

/// What the startup probe found. `Unavailable` until [`probe_keychain`] has run.
pub fn keychain_status() -> KeychainStatus {
    *KEYCHAIN_STATUS.get().unwrap_or(&KeychainStatus::Unavailable)
}

/// Store a secret in the system keychain.
///
/// Returns `false` when the secret could not be stored, so the caller can fall back
/// to encrypted JSON. The first successful write of a process is read back to confirm
/// the store actually works; later writes trust it.
pub fn store_secret_in_keychain(
    create_entry: fn() -> Result<keyring::Entry, keyring::Error>,
    delete_fn: fn(),
    secret: &str,
) -> bool {
    if secret.is_empty() {
        delete_fn();
        return true;
    }
    let entry = match create_entry() {
        Ok(entry) => entry,
        Err(_) => {
            log::warn!("Failed to create keychain entry");
            return false;
        }
    };
    if let Err(e) = entry.set_password(secret) {
        log::warn!("Failed to store secret in keychain: {}", e);
        return false;
    }

    // No read-back here any more. The startup probe already proved the store round-trips,
    // with a canary rather than a real secret, so verifying on every first write only
    // repeated that at the cost of a credential-store read on the hot save path.
    true
}

/// Store API key in system keychain and verify it can be retrieved
pub fn store_api_key_in_keychain(key: &str) -> bool {
    store_secret_in_keychain(create_keychain_entry, delete_api_key_from_keychain, key)
}

/// Store cloud access token in system keychain
pub fn store_cloud_token_in_keychain(token: &str) -> bool {
    store_secret_in_keychain(create_cloud_keychain_entry, delete_cloud_token_from_keychain, token)
}

/// Retrieve a secret from the system keychain.
/// Generic helper used for both AI API key and cloud access token.
pub fn get_secret_from_keychain(
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
pub fn get_api_key_from_keychain() -> Option<String> {
    get_secret_from_keychain(create_keychain_entry)
}

/// Retrieve cloud access token from system keychain
pub fn get_cloud_token_from_keychain() -> Option<String> {
    get_secret_from_keychain(create_cloud_keychain_entry)
}

/// Delete a secret from the keychain.
pub fn delete_secret_from_keychain(
    create_entry: fn() -> Result<keyring::Entry, keyring::Error>,
) {
    if let Ok(entry) = create_entry() {
        let _ = entry.delete_credential();
    }
}

/// Delete API key from keychain
pub fn delete_api_key_from_keychain() {
    delete_secret_from_keychain(create_keychain_entry);
}

/// Delete cloud access token from keychain
pub fn delete_cloud_token_from_keychain() {
    delete_secret_from_keychain(create_cloud_keychain_entry);
}

/// Derive a 256-bit key from machine-specific information
/// This makes the encrypted data machine-bound (can't be decrypted on another machine)
pub fn derive_machine_key() -> [u8; 32] {
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

/// Decrypt a secret that an older build sealed under the machine key.
///
/// This is the **steady-state** reader, and it fails closed: an empty string when the value
/// cannot be opened. It deliberately does not fall back to the retired XOR format, because
/// that fallback fired on any AEAD failure and returned whatever XOR made of the bytes -
/// under a key that is a literal in public source - as though it were the secret.
///
/// XOR lives in [`decrypt_legacy_secret`], which the one-time startup migration calls and
/// nothing else does.
pub fn decrypt_api_key(encoded: &str) -> String {
    decrypt_with_aes(encoded).unwrap_or_else(|| {
        log::warn!("Stored secret could not be decrypted on this machine");
        String::new()
    })
}

/// One-time reader for secrets written by older builds.
///
/// Tries AES-256-GCM first, then the retired XOR obfuscation. Called **only** from the
/// startup migration in `settings.rs`, so the live read path keeps failing closed.
pub fn decrypt_legacy_secret(encoded: &str) -> String {
    if let Some(plaintext) = decrypt_with_aes(encoded) {
        return plaintext;
    }
    let xor = legacy_deobfuscate(encoded);
    if !xor.is_empty() {
        log::info!("Recovered a secret written in the retired obfuscation format");
    }
    xor
}

/// `Some(plaintext)` when the value opens under the machine key, `None` when it does not.
///
/// A value that is not base64 at all is treated as plaintext from a build that predates
/// any encryption here - an actual migration case, unlike the AEAD failures above.
fn decrypt_with_aes(encoded: &str) -> Option<String> {
    if encoded.is_empty() {
        return Some(String::new());
    }

    use aes_gcm::{
        aead::{Aead, KeyInit},
        Aes256Gcm, Nonce,
    };
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    let data = match STANDARD.decode(encoded) {
        Ok(data) => data,
        Err(_) => return Some(encoded.to_string()),
    };

    // 12-byte nonce plus at least one byte of ciphertext.
    if data.len() < 13 {
        return None;
    }

    let (nonce_bytes, ciphertext) = data.split_at(12);
    let encryption_key = derive_machine_key();
    let cipher = Aes256Gcm::new_from_slice(&encryption_key).expect("Invalid key length");
    let nonce = Nonce::from_slice(nonce_bytes);

    match cipher.decrypt(nonce, ciphertext) {
        Ok(plaintext) => String::from_utf8(plaintext).ok(),
        Err(_) => None,
    }
}

/// Undo the retired XOR obfuscation. Migration only - see [`LEGACY_OBFUSCATION_KEY`].
fn legacy_deobfuscate(encoded: &str) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    match STANDARD.decode(encoded) {
        Ok(bytes) => {
            let decoded: Vec<u8> = bytes
                .iter()
                .enumerate()
                .map(|(i, b)| b ^ LEGACY_OBFUSCATION_KEY[i % LEGACY_OBFUSCATION_KEY.len()])
                .collect();
            String::from_utf8(decoded).unwrap_or_default()
        }
        Err(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn early_diagnostics_are_held_and_then_handed_over_once() {
        // The whole point of the buffer is that these lines survive being emitted before
        // the log plugin exists. If draining stopped working they would go quiet again,
        // which is exactly the failure that made the credential store undiagnosable.
        EARLY_DIAGNOSTICS.lock().expect("lock").clear();

        early_log(log::Level::Warn, "first".to_string());
        early_log(log::Level::Info, "second".to_string());
        assert_eq!(EARLY_DIAGNOSTICS.lock().expect("lock").len(), 2);

        flush_early_diagnostics();
        assert!(
            EARLY_DIAGNOSTICS.lock().expect("lock").is_empty(),
            "a flush must hand the lines over, not copy them"
        );

        // A second flush is a no-op rather than a repeat, since setup can run again.
        flush_early_diagnostics();
        assert!(EARLY_DIAGNOSTICS.lock().expect("lock").is_empty());
    }

    #[test]
    fn the_early_buffer_does_not_grow_without_bound() {
        EARLY_DIAGNOSTICS.lock().expect("lock").clear();
        for i in 0..200 {
            early_log(log::Level::Warn, format!("line {}", i));
        }
        assert_eq!(
            EARLY_DIAGNOSTICS.lock().expect("lock").len(),
            64,
            "startup diagnostics are bounded; a loop must not turn them into a leak"
        );
        EARLY_DIAGNOSTICS.lock().expect("lock").clear();
    }

    /// Builds a value in the format older builds wrote. Only the tests need this now -
    /// nothing in the app seals under the machine key any more.
    fn seal_under_machine_key(secret: &str) -> String {
        use aes_gcm::{
            aead::{Aead, KeyInit},
            Aes256Gcm, Nonce,
        };
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        use rand::RngCore;

        let cipher = Aes256Gcm::new_from_slice(&derive_machine_key()).expect("32-byte key");
        let mut nonce = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce);
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), secret.as_bytes())
            .expect("encrypt");
        let mut out = Vec::with_capacity(12 + ciphertext.len());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ciphertext);
        STANDARD.encode(&out)
    }

    #[test]
    fn a_secret_round_trips_through_the_machine_key() {
        let secret = "sk_stashpad_0123456789abcdef";
        let sealed = seal_under_machine_key(secret);
        assert_ne!(sealed, secret, "the stored form must not be the plaintext");
        assert_eq!(decrypt_api_key(&sealed), secret);
    }

    #[test]
    fn an_empty_secret_stays_empty() {
        assert_eq!(decrypt_api_key(""), "");
    }

    /// The point of the change: a value that does not open must come back empty rather
    /// than being run through XOR and returned as if it were the secret.
    #[test]
    fn a_tampered_value_never_falls_back_to_the_retired_format() {
        use base64::{engine::general_purpose::STANDARD, Engine as _};

        let sealed = seal_under_machine_key("sk_stashpad_0123456789abcdef");
        let mut raw = STANDARD.decode(&sealed).expect("encrypt emits base64");
        let last = raw.len() - 1;
        raw[last] ^= 0xff;
        let tampered = STANDARD.encode(&raw);

        // The old path returned whatever XOR made of these bytes and presented it as the
        // secret. That the retired format is genuinely unreachable is pinned by
        // `the_migration_reader_still_opens_the_retired_format` below, which feeds a value
        // XOR *can* decode and checks the live path still refuses it.
        assert_eq!(decrypt_api_key(&tampered), "");
    }

    /// Too short to carry a nonce. Used to reach the XOR path; must now fail closed.
    #[test]
    fn a_truncated_value_fails_closed() {
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        assert_eq!(decrypt_api_key(&STANDARD.encode(b"short")), "");
    }

    /// Old builds stored the key with no encoding at all.
    #[test]
    fn a_pre_encryption_plaintext_value_still_reads() {
        assert_eq!(decrypt_api_key("not base64 !!"), "not base64 !!");
    }

    /// The retired format is unreachable from the live path but still migratable.
    #[test]
    fn the_migration_reader_still_opens_the_retired_format() {
        use base64::{engine::general_purpose::STANDARD, Engine as _};
        let secret = "sk-legacy-value";
        let obfuscated: Vec<u8> = secret
            .bytes()
            .enumerate()
            .map(|(i, b)| b ^ LEGACY_OBFUSCATION_KEY[i % LEGACY_OBFUSCATION_KEY.len()])
            .collect();
        let encoded = STANDARD.encode(&obfuscated);

        assert_eq!(decrypt_legacy_secret(&encoded), secret);
        assert_eq!(decrypt_api_key(&encoded), "", "the live path must not open it");
    }

    /// Proves the thing the unit tests above cannot: that a *real* credential store is
    /// compiled in and round-trips. Ignored by default because a CI container has no
    /// Secret Service and would fail it for the wrong reason.
    ///
    /// Run it by hand on each platform when touching the keyring wiring:
    ///   cargo test -- --ignored the_real_credential_store_round_trips
    #[test]
    #[ignore]
    fn the_real_credential_store_round_trips() {
        assert_eq!(
            probe_keychain(),
            KeychainStatus::Working,
            "no platform credential store is compiled in - check the per-target keyring              features in Cargo.toml"
        );

        let secret = "sk_stashpad_round_trip_check";
        assert!(store_api_key_in_keychain(secret), "the store refused a write");
        assert_eq!(get_api_key_from_keychain().as_deref(), Some(secret));
        delete_api_key_from_keychain();
        assert_eq!(get_api_key_from_keychain(), None, "delete must actually remove it");
    }

    #[test]
    fn the_migration_reader_prefers_aes() {
        let secret = "sk_stashpad_aes_wins";
        let sealed = seal_under_machine_key(secret);
        assert_eq!(decrypt_legacy_secret(&sealed), secret);
    }
}
