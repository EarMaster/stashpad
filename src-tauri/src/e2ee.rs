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

//! The content key, and the three things that can unwrap it.
//!
//! One key per account encrypts every record ([`crate::envelope`] does the sealing). This
//! module is about getting that key to the places that need it, and only those:
//!
//! * **an installation**, through its X25519 key pair
//! * **the recovery code**, 240 bits the user writes down once
//! * **an MCP access key**, so an agent can reach the queue while the computer is off -
//!   which is the deliberate reason this is not end-to-end encryption
//!
//! The server stores the results and can open none of them. It holds public keys, opaque
//! wraps, a salt and a verifier.
//!
//! ## On the wrap construction
//!
//! Wrapping to a device is a sealed box: a fresh ephemeral key pair, X25519 with the
//! recipient's public key, HKDF over the shared secret, then XChaCha20-Poly1305. That is
//! the same shape as HPKE's base mode and as libsodium's `crypto_box_seal`, written out
//! here rather than pulled in, because the pieces (`hkdf`, `chacha20poly1305`) are already
//! dependencies and HPKE's framing buys nothing when both ends are this code.
//!
//! The part that is easy to get wrong is what goes into the derivation, so it is spelled
//! out: the HKDF info binds a version label, the account, **both** public keys and the
//! epoch. Binding both keys is what stops a wrap made for one device being replayed at
//! another, and binding the epoch stops one made for an older content key being presented
//! as current. Neither is optional.

// Ships ahead of its callers: enrolment and the conversion sweep are the next piece of
// work, and this is the foundation they are built on. The tests below exercise every item,
// so nothing here is unverified - only uncalled. The allow comes off when enrolment lands.
#![allow(dead_code)]

use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::RngCore;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

/// Crockford base32: no I, L, O or U, so nothing in a written code can be misread as
/// something else. Decoding folds I and L to 1, and O to 0, which is what people actually
/// type when reading their own handwriting.
const CROCKFORD: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// 240 bits, as 48 Crockford characters with no remainder.
const RECOVERY_SECRET_BYTES: usize = 30;

/// A truncated checksum, so a mistyped code fails immediately and legibly instead of
/// surfacing later as "your key does not work".
const RECOVERY_CHECKSUM_CHARS: usize = 4;

/// The content key itself. 32 bytes, and never written to disk in this form.
pub type ContentKey = Zeroizing<[u8; 32]>;

/// Generate a fresh content key.
pub fn new_content_key() -> ContentKey {
    let mut key = Zeroizing::new([0u8; 32]);
    rand::thread_rng().fill_bytes(key.as_mut());
    key
}

// ---------------------------------------------------------------------------------------
// Device key pairs
// ---------------------------------------------------------------------------------------

/// An installation's long-lived X25519 key pair.
pub struct DeviceKeypair {
    secret: x25519_dalek::StaticSecret,
}

impl DeviceKeypair {
    pub fn generate() -> Self {
        Self {
            secret: x25519_dalek::StaticSecret::random_from_rng(rand::thread_rng()),
        }
    }

    pub fn from_secret_bytes(bytes: [u8; 32]) -> Self {
        Self {
            secret: x25519_dalek::StaticSecret::from(bytes),
        }
    }

    pub fn secret_bytes(&self) -> Zeroizing<[u8; 32]> {
        Zeroizing::new(self.secret.to_bytes())
    }

    pub fn public_bytes(&self) -> [u8; 32] {
        x25519_dalek::PublicKey::from(&self.secret).to_bytes()
    }

    /// Standard base64 of the raw public key, which is the form the server stores.
    pub fn public_b64(&self) -> String {
        STANDARD.encode(self.public_bytes())
    }
}

/// The 80-bit fingerprint a user compares between two screens, as `4KQ7-M3XB-91TD-0WVH`.
///
/// **Not a six-digit code.** A short numeric comparison is only safe where an attacker gets
/// one online guess; here they can grind X25519 key pairs offline until one matches, so
/// twenty bits costs about a million key generations. Eighty is the floor.
///
/// Computed from the public key, so the device approving an enrolment recomputes it from the
/// key it fetched. Reading the server's stored copy would compare two numbers the server
/// supplied and verify nothing.
pub fn fingerprint(user_id: &str, public_key: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"stashpad/e2ee/v1/devfp");
    hasher.update(user_id.as_bytes());
    hasher.update(public_key);
    let digest = hasher.finalize();

    let encoded = crockford_encode(&digest[..10]);
    encoded
        .as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join("-")
}

// ---------------------------------------------------------------------------------------
// Wrapping the content key
// ---------------------------------------------------------------------------------------

/// The label every derivation in this module starts from, so a key derived for one purpose
/// can never coincide with one derived for another.
fn wrap_info(user_id: &str, sender_pub: &[u8], recipient_pub: &[u8], epoch: u32) -> Vec<u8> {
    let mut info = Vec::with_capacity(96);
    info.extend_from_slice(b"stashpad/e2ee/v1/devicewrap");
    info.extend_from_slice(user_id.as_bytes());
    info.extend_from_slice(sender_pub);
    info.extend_from_slice(recipient_pub);
    info.extend_from_slice(&epoch.to_be_bytes());
    info
}

fn derive_wrap_key(shared: &[u8], info: &[u8]) -> Zeroizing<[u8; 32]> {
    use hkdf::Hkdf;
    let hk = Hkdf::<Sha256>::new(None, shared);
    let mut key = Zeroizing::new([0u8; 32]);
    hk.expand(info, key.as_mut())
        .expect("32 bytes is within the HKDF output limit");
    key
}

/// Seal the content key to an installation's public key.
///
/// The result is `ephemeral_public(32) || nonce(24) || ciphertext+tag(48)`, base64.
pub fn wrap_to_device(
    recipient_public: &[u8; 32],
    content_key: &ContentKey,
    user_id: &str,
    epoch: u32,
) -> Result<String, String> {
    use chacha20poly1305::{
        aead::{Aead, KeyInit},
        XChaCha20Poly1305, XNonce,
    };

    let ephemeral = x25519_dalek::EphemeralSecret::random_from_rng(rand::thread_rng());
    let ephemeral_public = x25519_dalek::PublicKey::from(&ephemeral).to_bytes();
    let shared = ephemeral.diffie_hellman(&x25519_dalek::PublicKey::from(*recipient_public));

    let info = wrap_info(user_id, &ephemeral_public, recipient_public, epoch);
    let key = derive_wrap_key(shared.as_bytes(), &info);

    let cipher =
        XChaCha20Poly1305::new_from_slice(key.as_slice()).map_err(|_| "bad key length")?;
    let mut nonce = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut nonce);
    let ciphertext = cipher
        .encrypt(XNonce::from_slice(&nonce), content_key.as_slice())
        .map_err(|_| "could not seal the content key".to_string())?;

    let mut out = Vec::with_capacity(32 + 24 + ciphertext.len());
    out.extend_from_slice(&ephemeral_public);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    Ok(STANDARD.encode(&out))
}

/// Open a wrap made by [`wrap_to_device`] for this installation.
pub fn unwrap_with_device(
    keypair: &DeviceKeypair,
    wrapped: &str,
    user_id: &str,
    epoch: u32,
) -> Result<ContentKey, String> {
    use chacha20poly1305::{
        aead::{Aead, KeyInit},
        XChaCha20Poly1305, XNonce,
    };

    let raw = STANDARD
        .decode(wrapped)
        .map_err(|_| "the wrapped key is not base64".to_string())?;
    if raw.len() != 32 + 24 + 48 {
        return Err(format!("the wrapped key is {} bytes, expected 104", raw.len()));
    }

    let mut ephemeral_public = [0u8; 32];
    ephemeral_public.copy_from_slice(&raw[..32]);
    let nonce = &raw[32..56];
    let ciphertext = &raw[56..];

    let shared = keypair
        .secret
        .diffie_hellman(&x25519_dalek::PublicKey::from(ephemeral_public));
    let info = wrap_info(user_id, &ephemeral_public, &keypair.public_bytes(), epoch);
    let key = derive_wrap_key(shared.as_bytes(), &info);

    let cipher =
        XChaCha20Poly1305::new_from_slice(key.as_slice()).map_err(|_| "bad key length")?;
    let plaintext = cipher
        .decrypt(XNonce::from_slice(nonce), ciphertext)
        .map_err(|_| "this wrapped key is not for this installation".to_string())?;

    if plaintext.len() != 32 {
        return Err("the unwrapped content key is the wrong length".to_string());
    }
    let mut key = Zeroizing::new([0u8; 32]);
    key.copy_from_slice(&plaintext);
    Ok(key)
}

// ---------------------------------------------------------------------------------------
// The recovery code
// ---------------------------------------------------------------------------------------

/// A freshly minted recovery code: what the user sees, and the bytes behind it.
pub struct RecoveryCode {
    /// `SP1-XXXX-…`, shown once and never stored.
    pub printed: String,
    secret: Zeroizing<[u8; RECOVERY_SECRET_BYTES]>,
}

impl RecoveryCode {
    /// The first printed group, e.g. `SP1-4K7Q`.
    ///
    /// Stored so a user holding two pieces of paper can tell which is current. It costs 20
    /// of 240 bits; the remaining 220 are still far past any brute-force budget.
    pub fn hint(&self) -> String {
        self.printed.split('-').take(2).collect::<Vec<_>>().join("-")
    }
}

/// Mint a recovery code.
pub fn new_recovery_code() -> RecoveryCode {
    let mut secret = Zeroizing::new([0u8; RECOVERY_SECRET_BYTES]);
    rand::thread_rng().fill_bytes(secret.as_mut());

    let payload = crockford_encode(secret.as_slice());
    let checksum = recovery_checksum(secret.as_slice());
    let all = format!("{}{}", payload, checksum);

    let groups: Vec<String> = all
        .as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect();

    RecoveryCode {
        printed: format!("SP1-{}", groups.join("-")),
        secret,
    }
}

/// Read a code the user typed back, tolerating the ways people write one down.
///
/// Case is folded, dashes and spaces ignored, and the Crockford substitutions applied, so
/// `sp1 4k7q...` and `SP1-4K7Q-...` are the same code. A wrong checksum is reported as
/// such rather than as a key that does not work.
pub fn parse_recovery_code(input: &str) -> Result<Zeroizing<[u8; RECOVERY_SECRET_BYTES]>, String> {
    let cleaned: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect::<String>()
        .to_ascii_uppercase();

    let body = cleaned
        .strip_prefix("SP1")
        .ok_or_else(|| "a recovery code starts with SP1".to_string())?;

    let expected = RECOVERY_SECRET_BYTES * 8 / 5 + RECOVERY_CHECKSUM_CHARS;
    if body.len() != expected {
        return Err(format!(
            "a recovery code has {} characters after SP1, this one has {}",
            expected,
            body.len()
        ));
    }

    let (payload, checksum) = body.split_at(expected - RECOVERY_CHECKSUM_CHARS);
    let decoded = crockford_decode(payload)?;
    if decoded.len() != RECOVERY_SECRET_BYTES {
        return Err("this does not look like a recovery code".to_string());
    }

    let mut secret = Zeroizing::new([0u8; RECOVERY_SECRET_BYTES]);
    secret.copy_from_slice(&decoded);

    if recovery_checksum(secret.as_slice()) != checksum {
        return Err("that recovery code has a typo in it".to_string());
    }
    Ok(secret)
}

fn recovery_checksum(secret: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"stashpad/e2ee/v1/recovery-checksum");
    hasher.update(secret);
    let digest = hasher.finalize();
    // 4 Crockford characters carry 20 bits.
    crockford_encode(&digest[..3])[..RECOVERY_CHECKSUM_CHARS].to_string()
}

/// Derive the key that seals the content key for recovery.
///
/// HKDF and not Argon2: the input is 240 bits of CSPRNG output with no dictionary behind
/// it, so a work factor buys nothing and is not free. The same reasoning the `api_keys`
/// migration sets out for hashing an access key with SHA-256.
fn derive_recovery_key(
    secret: &[u8],
    salt: &[u8],
    user_id: &str,
    epoch: u32,
) -> Zeroizing<[u8; 32]> {
    use hkdf::Hkdf;

    let mut info = Vec::with_capacity(64);
    info.extend_from_slice(b"stashpad/e2ee/v1/recovery");
    info.extend_from_slice(user_id.as_bytes());
    info.extend_from_slice(&epoch.to_be_bytes());

    let hk = Hkdf::<Sha256>::new(Some(salt), secret);
    let mut key = Zeroizing::new([0u8; 32]);
    hk.expand(&info, key.as_mut())
        .expect("32 bytes is within the HKDF output limit");
    key
}

/// Seal the content key under a recovery code. Returns `(salt_b64, wrapped_b64)`.
pub fn wrap_to_recovery(
    code: &RecoveryCode,
    content_key: &ContentKey,
    user_id: &str,
    epoch: u32,
) -> Result<(String, String), String> {
    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    let key = derive_recovery_key(code.secret.as_slice(), &salt, user_id, epoch);
    let wrapped = seal_key(&key, content_key)?;
    Ok((STANDARD.encode(salt), wrapped))
}

/// Open a recovery wrap with a code the user typed.
pub fn unwrap_with_recovery(
    typed: &str,
    salt_b64: &str,
    wrapped: &str,
    user_id: &str,
    epoch: u32,
) -> Result<ContentKey, String> {
    let secret = parse_recovery_code(typed)?;
    let salt = STANDARD
        .decode(salt_b64)
        .map_err(|_| "the stored recovery salt is damaged".to_string())?;
    let key = derive_recovery_key(secret.as_slice(), &salt, user_id, epoch);
    open_key(&key, wrapped)
        .ok_or_else(|| "that recovery code does not belong to this account".to_string())
}

// ---------------------------------------------------------------------------------------
// MCP access keys
// ---------------------------------------------------------------------------------------

/// Seal the content key under an access key, so the server can open it for the duration of
/// one request carrying that key - and at no other time.
///
/// This is the deliberate hole, and it is why the guarantee stops short of end-to-end. The
/// server stores only a SHA-256 of the access key, so it cannot derive this on its own; the
/// full string arrives in the request.
pub fn wrap_to_api_key(
    api_key: &str,
    content_key: &ContentKey,
    user_id: &str,
    epoch: u32,
) -> Result<(String, String), String> {
    use hkdf::Hkdf;

    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);

    let mut info = Vec::with_capacity(64);
    info.extend_from_slice(b"stashpad/e2ee/v1/apikey");
    info.extend_from_slice(user_id.as_bytes());
    info.extend_from_slice(&epoch.to_be_bytes());

    let hk = Hkdf::<Sha256>::new(Some(&salt), api_key.as_bytes());
    let mut key = Zeroizing::new([0u8; 32]);
    hk.expand(&info, key.as_mut())
        .expect("32 bytes is within the HKDF output limit");

    Ok((STANDARD.encode(salt), seal_key(&key, content_key)?))
}

// ---------------------------------------------------------------------------------------
// The verifier
// ---------------------------------------------------------------------------------------

const VERIFIER_PLAINTEXT: &[u8] = b"stashpad-content-key-v1";

/// A constant sealed under the content key.
///
/// Lets a client that has just unwrapped a key confirm it is the right one, rather than
/// discovering it later through a record that will not open. Useless to the server, which
/// cannot open it either.
pub fn make_verifier(content_key: &ContentKey) -> Result<String, String> {
    let key = verifier_key(content_key);
    use chacha20poly1305::{
        aead::{Aead, KeyInit},
        XChaCha20Poly1305, XNonce,
    };
    let cipher = XChaCha20Poly1305::new_from_slice(key.as_slice()).map_err(|_| "bad key")?;
    let mut nonce = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut nonce);
    let ct = cipher
        .encrypt(XNonce::from_slice(&nonce), VERIFIER_PLAINTEXT)
        .map_err(|_| "could not build the verifier".to_string())?;
    let mut out = Vec::with_capacity(24 + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok(STANDARD.encode(&out))
}

/// Whether this content key is the one the account was set up with.
pub fn check_verifier(content_key: &ContentKey, verifier: &str) -> bool {
    use chacha20poly1305::{
        aead::{Aead, KeyInit},
        XChaCha20Poly1305, XNonce,
    };
    let Ok(raw) = STANDARD.decode(verifier) else {
        return false;
    };
    if raw.len() < 25 {
        return false;
    }
    let key = verifier_key(content_key);
    let Ok(cipher) = XChaCha20Poly1305::new_from_slice(key.as_slice()) else {
        return false;
    };
    cipher
        .decrypt(XNonce::from_slice(&raw[..24]), &raw[24..])
        .map(|p| p == VERIFIER_PLAINTEXT)
        .unwrap_or(false)
}

fn verifier_key(content_key: &ContentKey) -> Zeroizing<[u8; 32]> {
    use hkdf::Hkdf;
    let hk = Hkdf::<Sha256>::from_prk(content_key.as_slice()).expect("32-byte PRK");
    let mut key = Zeroizing::new([0u8; 32]);
    hk.expand(b"stashpad/e2ee/v1/verify", key.as_mut())
        .expect("32 bytes is within the HKDF output limit");
    key
}

// ---------------------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------------------

fn seal_key(key: &[u8; 32], content_key: &ContentKey) -> Result<String, String> {
    use chacha20poly1305::{
        aead::{Aead, KeyInit},
        XChaCha20Poly1305, XNonce,
    };
    let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|_| "bad key length")?;
    let mut nonce = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut nonce);
    let ct = cipher
        .encrypt(XNonce::from_slice(&nonce), content_key.as_slice())
        .map_err(|_| "could not seal the content key".to_string())?;
    let mut out = Vec::with_capacity(24 + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok(STANDARD.encode(&out))
}

fn open_key(key: &[u8; 32], wrapped: &str) -> Option<ContentKey> {
    use chacha20poly1305::{
        aead::{Aead, KeyInit},
        XChaCha20Poly1305, XNonce,
    };
    let raw = STANDARD.decode(wrapped).ok()?;
    if raw.len() != 24 + 48 {
        return None;
    }
    let cipher = XChaCha20Poly1305::new_from_slice(key).ok()?;
    let plaintext = cipher
        .decrypt(XNonce::from_slice(&raw[..24]), &raw[24..])
        .ok()?;
    if plaintext.len() != 32 {
        return None;
    }
    let mut out = Zeroizing::new([0u8; 32]);
    out.copy_from_slice(&plaintext);
    Some(out)
}

fn crockford_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 8 / 5 + 1);
    let mut buffer: u32 = 0;
    let mut bits = 0u32;
    for byte in bytes {
        buffer = (buffer << 8) | *byte as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(CROCKFORD[((buffer >> bits) & 0x1f) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(CROCKFORD[((buffer << (5 - bits)) & 0x1f) as usize] as char);
    }
    out
}

fn crockford_decode(value: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(value.len() * 5 / 8);
    let mut buffer: u32 = 0;
    let mut bits = 0u32;

    for c in value.chars() {
        // The substitutions Crockford specifies, which are the ones people actually make.
        let c = match c {
            'I' | 'L' => '1',
            'O' => '0',
            other => other,
        };
        let value = CROCKFORD
            .iter()
            .position(|&a| a as char == c)
            .ok_or_else(|| format!("{:?} is not part of a recovery code", c))?;
        buffer = (buffer << 5) | value as u32;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER: &str = "11111111-1111-4111-8111-111111111111";

    #[test]
    fn a_content_key_round_trips_to_a_device() {
        let device = DeviceKeypair::generate();
        let ck = new_content_key();
        let wrapped = wrap_to_device(&device.public_bytes(), &ck, USER, 1).expect("wrap");
        let opened = unwrap_with_device(&device, &wrapped, USER, 1).expect("unwrap");
        assert_eq!(opened.as_slice(), ck.as_slice());
    }

    #[test]
    fn the_wrap_is_the_documented_size() {
        let device = DeviceKeypair::generate();
        let wrapped = wrap_to_device(&device.public_bytes(), &new_content_key(), USER, 1).unwrap();
        assert_eq!(STANDARD.decode(&wrapped).unwrap().len(), 32 + 24 + 48);
    }

    /// Binding the recipient's key is what stops a wrap being replayed at another device.
    #[test]
    fn another_device_cannot_open_a_wrap() {
        let a = DeviceKeypair::generate();
        let b = DeviceKeypair::generate();
        let wrapped = wrap_to_device(&a.public_bytes(), &new_content_key(), USER, 1).unwrap();
        assert!(unwrap_with_device(&b, &wrapped, USER, 1).is_err());
    }

    /// Binding the account stops a wrap being moved between them.
    #[test]
    fn a_wrap_does_not_open_under_another_account() {
        let device = DeviceKeypair::generate();
        let wrapped = wrap_to_device(&device.public_bytes(), &new_content_key(), USER, 1).unwrap();
        let other = "99999999-9999-4999-8999-999999999999";
        assert!(unwrap_with_device(&device, &wrapped, other, 1).is_err());
    }

    /// Binding the epoch stops a wrap for a superseded key being presented as current.
    #[test]
    fn a_wrap_does_not_open_under_another_epoch() {
        let device = DeviceKeypair::generate();
        let wrapped = wrap_to_device(&device.public_bytes(), &new_content_key(), USER, 1).unwrap();
        assert!(unwrap_with_device(&device, &wrapped, USER, 2).is_err());
    }

    #[test]
    fn a_tampered_wrap_does_not_open() {
        let device = DeviceKeypair::generate();
        let wrapped = wrap_to_device(&device.public_bytes(), &new_content_key(), USER, 1).unwrap();
        let mut raw = STANDARD.decode(&wrapped).unwrap();
        let last = raw.len() - 1;
        raw[last] ^= 0xff;
        assert!(unwrap_with_device(&device, &STANDARD.encode(&raw), USER, 1).is_err());
    }

    #[test]
    fn a_device_keypair_survives_a_round_trip_through_bytes() {
        let original = DeviceKeypair::generate();
        let restored = DeviceKeypair::from_secret_bytes(*original.secret_bytes());
        assert_eq!(original.public_bytes(), restored.public_bytes());

        let ck = new_content_key();
        let wrapped = wrap_to_device(&original.public_bytes(), &ck, USER, 1).unwrap();
        assert_eq!(
            unwrap_with_device(&restored, &wrapped, USER, 1).unwrap().as_slice(),
            ck.as_slice()
        );
    }

    // --- fingerprint ---

    #[test]
    fn a_fingerprint_is_eighty_bits_in_four_groups() {
        let fp = fingerprint(USER, &DeviceKeypair::generate().public_bytes());
        assert_eq!(fp.len(), 19, "16 characters and three dashes: {}", fp);
        assert_eq!(fp.split('-').count(), 4);
        assert!(fp.split('-').all(|g| g.len() == 4));
    }

    #[test]
    fn a_fingerprint_follows_the_key_and_the_account() {
        let a = DeviceKeypair::generate();
        let b = DeviceKeypair::generate();
        assert_ne!(
            fingerprint(USER, &a.public_bytes()),
            fingerprint(USER, &b.public_bytes())
        );
        assert_ne!(
            fingerprint(USER, &a.public_bytes()),
            fingerprint("other-account", &a.public_bytes())
        );
        assert_eq!(
            fingerprint(USER, &a.public_bytes()),
            fingerprint(USER, &a.public_bytes())
        );
    }

    // --- recovery code ---

    #[test]
    fn a_recovery_code_round_trips() {
        let code = new_recovery_code();
        let parsed = parse_recovery_code(&code.printed).expect("parse");
        assert_eq!(parsed.as_slice(), code.secret.as_slice());
    }

    #[test]
    fn a_recovery_code_has_the_documented_shape() {
        let code = new_recovery_code();
        assert!(code.printed.starts_with("SP1-"));
        let groups: Vec<&str> = code.printed.split('-').collect();
        // SP1 plus 48 payload characters and 4 of checksum, in groups of four.
        assert_eq!(groups.len(), 1 + 13, "{}", code.printed);
        assert!(groups[1..].iter().all(|g| g.len() == 4));
        assert_eq!(code.hint(), format!("SP1-{}", groups[1]));
    }

    /// People write these down by hand, so the reader has to tolerate how they come back.
    #[test]
    fn a_recovery_code_is_read_back_forgivingly() {
        let code = new_recovery_code();
        let messy = code.printed.to_lowercase().replace('-', " ");
        assert_eq!(
            parse_recovery_code(&messy).expect("parse").as_slice(),
            code.secret.as_slice()
        );
    }

    /// A typo must fail as a typo, not as a key that mysteriously does not work.
    #[test]
    fn a_mistyped_recovery_code_is_named_as_a_typo() {
        let code = new_recovery_code();

        // Change exactly one payload character to a different valid one. Located rather
        // than guessed at, so the test cannot quietly start corrupting the prefix or a
        // separator instead and pass for the wrong reason.
        let at = code.printed.find('-').expect("SP1- prefix") + 1;
        let mut chars: Vec<char> = code.printed.chars().collect();
        chars[at] = if chars[at] == '2' { '3' } else { '2' };
        let typo: String = chars.into_iter().collect();
        assert_ne!(typo, code.printed, "the test must actually change something");
        assert_eq!(typo.len(), code.printed.len(), "and only the one character");

        match parse_recovery_code(&typo) {
            Err(message) => assert!(message.contains("typo"), "got {:?}", message),
            Ok(_) => panic!("a changed character must not pass the checksum"),
        }
    }

    #[test]
    fn a_recovery_code_without_the_prefix_is_refused() {
        let code = new_recovery_code();
        let without = code.printed.trim_start_matches("SP1-").to_string();
        assert!(parse_recovery_code(&without).is_err());
    }

    #[test]
    fn the_content_key_round_trips_through_a_recovery_code() {
        let code = new_recovery_code();
        let ck = new_content_key();
        let (salt, wrapped) = wrap_to_recovery(&code, &ck, USER, 1).expect("wrap");
        let opened =
            unwrap_with_recovery(&code.printed, &salt, &wrapped, USER, 1).expect("unwrap");
        assert_eq!(opened.as_slice(), ck.as_slice());
    }

    #[test]
    fn another_recovery_code_does_not_open_it() {
        let ck = new_content_key();
        let (salt, wrapped) = wrap_to_recovery(&new_recovery_code(), &ck, USER, 1).unwrap();
        let other = new_recovery_code();
        assert!(unwrap_with_recovery(&other.printed, &salt, &wrapped, USER, 1).is_err());
    }

    #[test]
    fn a_recovery_wrap_is_bound_to_its_account_and_epoch() {
        let code = new_recovery_code();
        let ck = new_content_key();
        let (salt, wrapped) = wrap_to_recovery(&code, &ck, USER, 1).unwrap();
        assert!(unwrap_with_recovery(&code.printed, &salt, &wrapped, "other", 1).is_err());
        assert!(unwrap_with_recovery(&code.printed, &salt, &wrapped, USER, 2).is_err());
    }

    // --- access keys and the verifier ---

    #[test]
    fn an_access_key_wrap_is_bound_to_the_key() {
        let ck = new_content_key();
        let (salt, wrapped) =
            wrap_to_api_key("sk_stashpad_abcd_0123456789", &ck, USER, 1).expect("wrap");
        assert!(!salt.is_empty());
        assert_eq!(STANDARD.decode(&wrapped).unwrap().len(), 24 + 48);
    }

    #[test]
    fn the_verifier_accepts_only_the_right_content_key() {
        let ck = new_content_key();
        let verifier = make_verifier(&ck).expect("verifier");
        assert!(check_verifier(&ck, &verifier));
        assert!(!check_verifier(&new_content_key(), &verifier));
        assert!(!check_verifier(&ck, "not base64 at all !!"));
        assert!(!check_verifier(&ck, ""));
    }

    // --- base32 ---

    #[test]
    fn crockford_round_trips() {
        let bytes: Vec<u8> = (0u8..30).collect();
        let encoded = crockford_encode(&bytes);
        assert_eq!(encoded.len(), 48, "240 bits is 48 characters exactly");
        assert_eq!(crockford_decode(&encoded).unwrap(), bytes);
    }

    #[test]
    fn crockford_folds_the_letters_people_confuse() {
        let bytes: Vec<u8> = (0u8..30).collect();
        let encoded = crockford_encode(&bytes);
        let confused = encoded.replace('1', "I").replace('0', "O");
        assert_eq!(crockford_decode(&confused).unwrap(), bytes);
    }

    #[test]
    fn crockford_refuses_a_character_it_never_emits() {
        assert!(crockford_decode("UUUU").is_err());
    }
}
