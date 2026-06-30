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

use crate::utils::get_app_dir;

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
pub fn create_keychain_entry() -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new_with_target(KEYCHAIN_TARGET, KEYCHAIN_SERVICE, KEYCHAIN_USER)
}

/// Create a keychain entry for the cloud access token
pub fn create_cloud_keychain_entry() -> Result<keyring::Entry, keyring::Error> {
    keyring::Entry::new_with_target(KEYCHAIN_CLOUD_TARGET, KEYCHAIN_SERVICE, KEYCHAIN_CLOUD_USER)
}

/// Store a secret in the system keychain and verify it can be retrieved.
/// Generic helper used for both AI API key and cloud access token.
pub fn store_secret_in_keychain(
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

/// Encrypt a string using AES-256-GCM (fallback for when keychain unavailable)
pub fn encrypt_api_key(key: &str) -> String {
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
pub fn decrypt_api_key(encoded: &str) -> String {
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
pub fn obfuscate_simple(key: &str) -> String {
    let bytes: Vec<u8> = key
        .bytes()
        .enumerate()
        .map(|(i, b)| b ^ OBFUSCATION_KEY[i % OBFUSCATION_KEY.len()])
        .collect();
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    STANDARD.encode(&bytes)
}

/// Simple XOR deobfuscation (legacy fallback)
pub fn deobfuscate_simple(encoded: &str) -> String {
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
