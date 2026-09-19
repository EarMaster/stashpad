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

//! This installation's key material, and sealing at the sync boundary.
//!
//! Two jobs. It keeps the device key pair and, once unwrapped, the content key. And it
//! seals and opens records as they cross [`crate::sync`] - which is the seam the whole
//! design hangs on.
//!
//! **Why here and not in the webview.** Sealing on the way out and opening on the way in
//! means the TypeScript above the adapter keeps handling plaintext: the merge logic, the
//! last-write-wins comparison, the pending flags and the ordering channel are all unchanged
//! and unaware. There is no crypto in Svelte, no key crossing the IPC boundary, and one
//! place to audit.
//!
//! The safety rule the whole client obeys, stated once here because breaking it destroys
//! data rather than merely failing:
//!
//! > **A value this installation cannot open must never be written into a local row, and
//! > must never enter the push queue.**
//!
//! `db.rs` has two write origins. `SyncImport` stores without flagging; `LocalEdit` stamps
//! the row pending so it is pushed back up. A value that cannot be opened, stored as a
//! placeholder and then edited, is pushed as if it were the record - which is how a stash
//! is lost for good rather than temporarily. So an unopenable record is dropped from the
//! merge and reported, never substituted.

// Not yet called from the sync path or the interface: wiring this into `sync.rs` and the
// enrolment ceremony is the rest of this piece of work. The tests below drive every
// function, so it is uncalled rather than unverified, and the allow comes off with the
// wiring.
#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use zeroize::Zeroizing;

use crate::e2ee::{self, ContentKey, DeviceKeypair};
use crate::envelope::{self, Binding, Field, Kind};
use crate::state::lock_or_recover;
use crate::utils::get_app_dir;

/// The content key for this account, once something has unwrapped it.
static CONTENT_KEY: Mutex<Option<ContentKey>> = Mutex::new(None);

/// Which generation of content key [`CONTENT_KEY`] is.
static EPOCH: Mutex<u32> = Mutex::new(0);

/// Where the device key pair lives, sealed by the same protection as every other local
/// secret: the OS credential store, or the device passphrase where there is none.
fn device_key_path() -> PathBuf {
    get_app_dir().join("device_key.enc")
}

/// Load this installation's key pair, creating one the first time.
///
/// The secret is sealed before it touches the disk. On a machine with a working credential
/// store that is a key from the store; on one without, it is the device passphrase - and if
/// the user declined a passphrase there is nowhere safe to keep it, so enrolment is refused
/// rather than the key being written somewhere anyone could read.
pub fn load_or_create_device_keypair() -> Result<DeviceKeypair, String> {
    let path = device_key_path();

    if let Ok(sealed) = fs::read_to_string(&path) {
        let opened = open_local(sealed.trim())
            .ok_or("This installation's key could not be opened on this machine")?;
        if opened.len() != 32 {
            return Err("This installation's key is damaged".to_string());
        }
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&opened);
        return Ok(DeviceKeypair::from_secret_bytes(bytes));
    }

    let keypair = DeviceKeypair::generate();
    let sealed = seal_local(keypair.secret_bytes().as_slice())
        .ok_or("There is nowhere safe on this machine to keep an encryption key")?;
    fs::write(&path, sealed).map_err(|e| format!("Could not save the key: {}", e))?;
    Ok(keypair)
}

/// Seal bytes with whatever protects local secrets on this machine.
fn seal_local(bytes: &[u8]) -> Option<String> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    // The device passphrase path takes precedence when one is set; otherwise the OS
    // credential store's key. `localkey` already encodes which applies here.
    crate::localkey::seal_secret(&STANDARD.encode(bytes))
}

fn open_local(sealed: &str) -> Option<Vec<u8>> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    let inner = crate::localkey::open_secret(sealed)?;
    STANDARD.decode(inner).ok()
}

/// Hold the content key for this session.
pub fn set_content_key(key: ContentKey, epoch: u32) {
    *lock_or_recover(&CONTENT_KEY) = Some(key);
    *lock_or_recover(&EPOCH) = epoch;
}

/// Forget it - on logout, or when the account is no longer encrypted.
pub fn clear_content_key() {
    *lock_or_recover(&CONTENT_KEY) = None;
    *lock_or_recover(&EPOCH) = 0;
}

pub fn is_unlocked() -> bool {
    lock_or_recover(&CONTENT_KEY).is_some()
}

pub fn epoch() -> u32 {
    *lock_or_recover(&EPOCH)
}

/// What the interface needs to know about this installation's key state.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatus {
    pub unlocked: bool,
    pub epoch: u32,
}

pub fn status() -> SessionStatus {
    SessionStatus {
        unlocked: is_unlocked(),
        epoch: epoch(),
    }
}

// ---------------------------------------------------------------------------------------
// Sealing at the sync boundary
// ---------------------------------------------------------------------------------------

/// A record that could not be opened, so the merge can drop it rather than store it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnreadableRecord {
    pub id: String,
    pub reason: String,
}

/// Seal a stash-sync payload on its way out, and stamp the format version.
///
/// A no-op when this installation holds no content key, which is every account that has not
/// turned encryption on.
pub fn seal_stash_payload(
    payload: &mut serde_json::Value,
    user_id: &str,
) -> Result<(), String> {
    let guard = lock_or_recover(&CONTENT_KEY);
    let Some(key) = guard.as_ref() else {
        return Ok(());
    };
    let epoch = *lock_or_recover(&EPOCH);

    // The server reads this to decide whether this build may be served at all. Setting it
    // is the client's claim that it understands the format.
    payload["cryptoVersion"] = serde_json::json!(1);

    let Some(stashes) = payload["stashes"].as_array_mut() else {
        return Ok(());
    };

    for stash in stashes.iter_mut() {
        let id = stash["id"].as_str().unwrap_or_default().to_string();

        // A tombstone carries no text, and the server stores none for one either. Sealing
        // an empty string would only add a blob nobody reads.
        if stash["deleted"].as_bool().unwrap_or(false) {
            continue;
        }

        seal_field(stash, "content", key, user_id, &id, Kind::Stash, Field::Content, epoch)?;
        seal_field(
            stash,
            "enhancedContent",
            key,
            user_id,
            &id,
            Kind::Stash,
            Field::EnhancedContent,
            epoch,
        )?;
    }
    Ok(())
}

/// Open a stash-sync response on its way in.
///
/// Records that cannot be opened are removed from `synced` and returned, so the caller can
/// report them. They are never written locally - see the rule in the module notes.
pub fn open_stash_response(
    body: &mut serde_json::Value,
    user_id: &str,
) -> Vec<UnreadableRecord> {
    let mut unreadable = Vec::new();

    let guard = lock_or_recover(&CONTENT_KEY);
    let Some(key) = guard.as_ref() else {
        return unreadable;
    };
    let epoch = *lock_or_recover(&EPOCH);

    let Some(stashes) = body["synced"].as_array_mut() else {
        return unreadable;
    };

    stashes.retain_mut(|stash| {
        let id = stash["id"].as_str().unwrap_or_default().to_string();

        for (field, which) in [
            ("content", Field::Content),
            ("enhancedContent", Field::EnhancedContent),
        ] {
            if let Err(reason) =
                open_field(stash, field, key, user_id, &id, Kind::Stash, which, epoch)
            {
                log::warn!("Dropping stash {} from this sync: {}", id, reason);
                unreadable.push(UnreadableRecord { id: id.clone(), reason });
                return false;
            }
        }
        true
    });

    unreadable
}

/// Seal a context-sync payload. Name, description and rules travel together.
pub fn seal_context_payload(
    payload: &mut serde_json::Value,
    user_id: &str,
) -> Result<(), String> {
    let guard = lock_or_recover(&CONTENT_KEY);
    let Some(key) = guard.as_ref() else {
        return Ok(());
    };
    let epoch = *lock_or_recover(&EPOCH);

    payload["cryptoVersion"] = serde_json::json!(1);

    let Some(contexts) = payload["contexts"].as_array_mut() else {
        return Ok(());
    };

    for ctx in contexts.iter_mut() {
        let id = ctx["id"].as_str().unwrap_or_default().to_string();
        if ctx["deleted"].as_bool().unwrap_or(false) {
            continue;
        }

        seal_field(ctx, "name", key, user_id, &id, Kind::Context, Field::Name, epoch)?;
        seal_field(
            ctx,
            "description",
            key,
            user_id,
            &id,
            Kind::Context,
            Field::Description,
            epoch,
        )?;

        // `rules` is an array on the wire, and an envelope is a scalar, so a sealed list
        // travels as a single string element. The server's untagged reader accepts either.
        if let Some(rules) = ctx["rules"].as_array() {
            if !rules.is_empty() {
                let json = serde_json::to_string(rules).unwrap_or_else(|_| "[]".into());
                let sealed = envelope::seal(
                    key,
                    &json,
                    &Binding {
                        user_id,
                        kind: Kind::Context,
                        record_id: &id,
                        field: Field::Rules,
                        epoch,
                    },
                )
                .map_err(|e| format!("{:?}", e))?;
                ctx["rules"] = serde_json::json!([sealed]);
            }
        }
    }
    Ok(())
}

/// Open a context-sync response.
pub fn open_context_response(
    body: &mut serde_json::Value,
    user_id: &str,
) -> Vec<UnreadableRecord> {
    let mut unreadable = Vec::new();

    let guard = lock_or_recover(&CONTENT_KEY);
    let Some(key) = guard.as_ref() else {
        return unreadable;
    };
    let epoch = *lock_or_recover(&EPOCH);

    let Some(contexts) = body["synced"].as_array_mut() else {
        return unreadable;
    };

    contexts.retain_mut(|ctx| {
        let id = ctx["id"].as_str().unwrap_or_default().to_string();

        for (field, which) in [("name", Field::Name), ("description", Field::Description)] {
            if let Err(reason) =
                open_field(ctx, field, key, user_id, &id, Kind::Context, which, epoch)
            {
                log::warn!("Dropping context {} from this sync: {}", id, reason);
                unreadable.push(UnreadableRecord { id: id.clone(), reason });
                return false;
            }
        }

        // The sealed-rules case: a one-element array holding an envelope.
        let sealed_rules = ctx["rules"]
            .as_array()
            .and_then(|r| match r.as_slice() {
                [serde_json::Value::String(s)] if envelope::is_envelope(s) => Some(s.clone()),
                _ => None,
            });

        if let Some(sealed) = sealed_rules {
            match envelope::open(
                key,
                epoch,
                &sealed,
                &Binding {
                    user_id,
                    kind: Kind::Context,
                    record_id: &id,
                    field: Field::Rules,
                    epoch,
                },
            ) {
                Ok(json) => {
                    ctx["rules"] = serde_json::from_str(&json).unwrap_or_else(|_| serde_json::json!([]));
                }
                Err(e) => {
                    let reason = format!("{:?}", e);
                    log::warn!("Dropping context {} from this sync: {}", id, reason);
                    unreadable.push(UnreadableRecord { id: id.clone(), reason });
                    return false;
                }
            }
        }
        true
    });

    unreadable
}

#[allow(clippy::too_many_arguments)]
fn seal_field(
    record: &mut serde_json::Value,
    field: &str,
    key: &ContentKey,
    user_id: &str,
    record_id: &str,
    kind: Kind,
    which: Field,
    epoch: u32,
) -> Result<(), String> {
    let Some(plaintext) = record[field].as_str() else {
        // Absent or null. A null description means "there is none", which is not the same
        // as an empty one and must not become a blob.
        return Ok(());
    };
    if envelope::is_envelope(plaintext) {
        // Already sealed - a record that came from another device and is being pushed back
        // unchanged. Sealing twice would nest one envelope inside another.
        return Ok(());
    }

    let sealed = envelope::seal(
        key,
        plaintext,
        &Binding {
            user_id,
            kind,
            record_id,
            field: which,
            epoch,
        },
    )
    .map_err(|e| format!("could not encrypt {}: {:?}", field, e))?;

    record[field] = serde_json::Value::String(sealed);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn open_field(
    record: &mut serde_json::Value,
    field: &str,
    key: &ContentKey,
    user_id: &str,
    record_id: &str,
    kind: Kind,
    which: Field,
    epoch: u32,
) -> Result<(), String> {
    let Some(value) = record[field].as_str() else {
        return Ok(());
    };
    if !envelope::is_envelope(value) {
        // Plaintext, from an account part-way through converting. Leave it alone.
        return Ok(());
    }

    let plaintext = envelope::open(
        key,
        epoch,
        value,
        &Binding {
            user_id,
            kind,
            record_id,
            field: which,
            epoch,
        },
    )
    .map_err(|e| format!("{:?}", e))?;

    record[field] = serde_json::Value::String(plaintext);
    Ok(())
}

/// Unwrap the content key with this installation's key pair.
pub fn unlock_with_device(
    keypair: &DeviceKeypair,
    wrapped: &str,
    user_id: &str,
    epoch: u32,
    verifier: &str,
) -> Result<(), String> {
    let key = e2ee::unwrap_with_device(keypair, wrapped, user_id, epoch)?;
    confirm_and_hold(key, epoch, verifier)
}

/// Unwrap it with a recovery code the user typed.
pub fn unlock_with_recovery(
    typed: &str,
    salt: &str,
    wrapped: &str,
    user_id: &str,
    epoch: u32,
    verifier: &str,
) -> Result<(), String> {
    let key = e2ee::unwrap_with_recovery(typed, salt, wrapped, user_id, epoch)?;
    confirm_and_hold(key, epoch, verifier)
}

/// Check a freshly unwrapped key against the verifier before trusting it.
///
/// Without this, the wrong key surfaces later as a record that will not open, which reads
/// as data loss rather than as "that was the wrong code".
fn confirm_and_hold(key: ContentKey, epoch: u32, verifier: &str) -> Result<(), String> {
    if !verifier.is_empty() && !e2ee::check_verifier(&key, verifier) {
        return Err("That key does not belong to this account".to_string());
    }
    set_content_key(key, epoch);
    Ok(())
}

/// Keep a copy of the raw key bytes for a caller that has to wrap it onward.
pub fn content_key_bytes() -> Option<Zeroizing<[u8; 32]>> {
    lock_or_recover(&CONTENT_KEY).as_ref().map(|k| Zeroizing::new(**k))
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER: &str = "11111111-1111-4111-8111-111111111111";

    /// The content key is process-wide state, and cargo runs tests in parallel, so every
    /// test that touches it takes this first. Without it they clear each other's key
    /// mid-assertion and fail in a way that looks like a crypto bug and is not.
    ///
    /// `lock_or_recover` rather than `.lock().unwrap()`: one failing test would otherwise
    /// poison the mutex and every later test would panic on the lock instead of running.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn with_key<T>(body: impl FnOnce() -> T) -> T {
        let _guard = lock_or_recover(&TEST_LOCK);
        set_content_key(e2ee::new_content_key(), 1);
        let out = body();
        clear_content_key();
        out
    }

    fn stash_payload() -> serde_json::Value {
        serde_json::json!({
            "deviceId": "device-a",
            "stashes": [{
                "id": "22222222-2222-4222-8222-222222222222",
                "content": "the original text",
                "enhancedContent": "the rewritten text",
                "deleted": false
            }]
        })
    }

    #[test]
    fn a_payload_round_trips_through_the_boundary() {
        with_key(|| {
            let mut payload = stash_payload();
            seal_stash_payload(&mut payload, USER).expect("seal");

            let sealed = payload["stashes"][0]["content"].as_str().unwrap();
            assert!(envelope::is_envelope(sealed), "content must be sealed");
            assert!(envelope::is_envelope(
                payload["stashes"][0]["enhancedContent"].as_str().unwrap()
            ));
            assert_eq!(payload["cryptoVersion"], 1);

            // The response shape the server returns.
            let mut body = serde_json::json!({ "synced": payload["stashes"].clone() });
            let unreadable = open_stash_response(&mut body, USER);
            assert!(unreadable.is_empty());
            assert_eq!(body["synced"][0]["content"], "the original text");
            assert_eq!(body["synced"][0]["enhancedContent"], "the rewritten text");
        });
    }

    #[test]
    fn without_a_key_nothing_is_touched() {
        let _guard = lock_or_recover(&TEST_LOCK);
        clear_content_key();
        let mut payload = stash_payload();
        seal_stash_payload(&mut payload, USER).expect("seal");
        assert_eq!(payload["stashes"][0]["content"], "the original text");
        assert!(payload["cryptoVersion"].is_null());
    }

    /// A tombstone carries no text and the server stores none for one either.
    #[test]
    fn a_delete_is_not_sealed() {
        with_key(|| {
            let mut payload = stash_payload();
            payload["stashes"][0]["deleted"] = serde_json::json!(true);
            payload["stashes"][0]["content"] = serde_json::json!("");
            seal_stash_payload(&mut payload, USER).expect("seal");
            assert_eq!(payload["stashes"][0]["content"], "");
        });
    }

    /// A record pushed back unchanged is already sealed; sealing again would nest one
    /// envelope inside another and the outer one would open to ciphertext.
    #[test]
    fn an_already_sealed_value_is_not_sealed_twice() {
        with_key(|| {
            let mut payload = stash_payload();
            seal_stash_payload(&mut payload, USER).expect("first");
            let once = payload["stashes"][0]["content"].as_str().unwrap().to_string();

            seal_stash_payload(&mut payload, USER).expect("second");
            assert_eq!(payload["stashes"][0]["content"], once);
        });
    }

    /// Plaintext mixed in mid-conversion must survive the trip untouched.
    #[test]
    fn plaintext_in_a_response_is_left_alone() {
        with_key(|| {
            let mut body = serde_json::json!({
                "synced": [{ "id": "33333333-3333-4333-8333-333333333333",
                             "content": "not converted yet" }]
            });
            let unreadable = open_stash_response(&mut body, USER);
            assert!(unreadable.is_empty());
            assert_eq!(body["synced"][0]["content"], "not converted yet");
        });
    }

    /// The rule that matters most: a record this installation cannot open is dropped from
    /// the merge, never written locally with a placeholder.
    #[test]
    fn an_unopenable_record_is_dropped_rather_than_stored() {
        with_key(|| {
            let mut payload = stash_payload();
            seal_stash_payload(&mut payload, USER).expect("seal");

            // Same ciphertext, different record id: the additional data no longer matches.
            let mut moved = payload["stashes"][0].clone();
            moved["id"] = serde_json::json!("99999999-9999-4999-8999-999999999999");
            let mut body = serde_json::json!({ "synced": [moved] });

            let unreadable = open_stash_response(&mut body, USER);
            assert_eq!(unreadable.len(), 1);
            assert_eq!(
                body["synced"].as_array().unwrap().len(),
                0,
                "it must not reach the merge at all"
            );
        });
    }

    #[test]
    fn a_context_round_trips_including_its_rules() {
        with_key(|| {
            let mut payload = serde_json::json!({
                "deviceId": "device-a",
                "contexts": [{
                    "id": "44444444-4444-4444-8444-444444444444",
                    "name": "Client project",
                    "description": "the tech stack",
                    "rules": [{"ruleType": "title", "value": "acme", "matchType": "contains"}],
                    "deleted": false
                }]
            });

            seal_context_payload(&mut payload, USER).expect("seal");
            assert!(envelope::is_envelope(
                payload["contexts"][0]["name"].as_str().unwrap()
            ));
            let rules = payload["contexts"][0]["rules"].as_array().unwrap();
            assert_eq!(rules.len(), 1, "sealed rules travel as one element");
            assert!(envelope::is_envelope(rules[0].as_str().unwrap()));

            let mut body = serde_json::json!({ "synced": payload["contexts"].clone() });
            let unreadable = open_context_response(&mut body, USER);
            assert!(unreadable.is_empty());
            assert_eq!(body["synced"][0]["name"], "Client project");
            assert_eq!(body["synced"][0]["description"], "the tech stack");
            assert_eq!(body["synced"][0]["rules"][0]["value"], "acme");
        });
    }

    /// A null description means "there is none", which is not an empty one.
    #[test]
    fn a_null_description_stays_null() {
        with_key(|| {
            let mut payload = serde_json::json!({
                "contexts": [{
                    "id": "44444444-4444-4444-8444-444444444444",
                    "name": "Client project",
                    "description": null,
                    "rules": [],
                    "deleted": false
                }]
            });
            seal_context_payload(&mut payload, USER).expect("seal");
            assert!(payload["contexts"][0]["description"].is_null());
            assert_eq!(payload["contexts"][0]["rules"].as_array().unwrap().len(), 0);
        });
    }

    #[test]
    fn the_wrong_key_is_refused_against_the_verifier() {
        let _guard = lock_or_recover(&TEST_LOCK);
        let key = e2ee::new_content_key();
        let verifier = e2ee::make_verifier(&key).expect("verifier");

        assert!(confirm_and_hold(e2ee::new_content_key(), 1, &verifier).is_err());
        assert!(!is_unlocked(), "a refused key must not be held");

        assert!(confirm_and_hold(key, 1, &verifier).is_ok());
        assert!(is_unlocked());
        clear_content_key();
    }
}
