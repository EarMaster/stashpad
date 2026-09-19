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

//! The on-the-wire form of an encrypted record field.
//!
//! ```text
//! SPE1.<epoch>.<base64url_nopad nonce>.<base64url_nopad ciphertext||tag>
//! ```
//!
//! It lives in the column the plaintext used to occupy - `stashes.content`,
//! `contexts.name` and the rest - rather than in a new one. Keeping the column and the wire
//! field means the sync request and response shapes, the last-write-wins comparison and the
//! delta read-back are all untouched, and a table holding both shapes at once during a
//! migration falls out for free.
//!
//! Why the parts are laid out like this:
//!
//! * `SPE1` is magic and version in one token. A later format is `SPE2`, and a client that
//!   predates it rejects it on the prefix instead of misreading it as this one.
//! * The epoch sits **outside** the ciphertext on purpose. A client holding keys for several
//!   epochs has to pick one *before* it can decrypt anything.
//! * Dotted ASCII rather than one opaque blob, so the epoch is readable in a `sqlite3` shell
//!   during an incident and `substr(content,1,4) = 'SPE1'` is a usable SQL predicate.
//! * UTF-8 by construction, so it survives a TEXT column, serde's `String` and the
//!   `json_object()` the account export is built from.
//!
//! **This release only opens envelopes; nothing here is wired into the sync path yet.** That
//! is deliberate: read support has to be in the field at least one release before anything
//! can produce an envelope, because a user can decline updates or leave a second machine off
//! for a month, and a client that meets a value it cannot parse must not write it back.

// Nothing calls this module yet, and that is the point: read support ships a release ahead
// of anything that can produce an envelope (see the note above). The allow comes off in the
// change that wires sealing into the sync path, and the tests below exercise every item
// meanwhile, so none of this is unverified.
#![allow(dead_code)]

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

/// Format marker and version. Bump only when the layout itself changes.
pub const MAGIC: &str = "SPE1";

/// Nonce length for XChaCha20-Poly1305.
const NONCE_LEN: usize = 24;

/// Poly1305 tag. An empty plaintext seals to exactly this, which is legitimate - a cleared
/// description or a tombstone's empty content - so the tag length is the floor, not a
/// value below the floor.
const TAG_LEN: usize = 16;

/// Which record a field belongs to. Part of the additional data, so a ciphertext cannot be
/// moved between record types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Stash,
    Context,
    Attachment,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Stash => "stash",
            Kind::Context => "context",
            Kind::Attachment => "attachment",
        }
    }
}

/// Which field of that record. Also part of the additional data: without it,
/// `enhanced_content` could be served in place of `content` - same row, same key, both
/// sealed strings - and a context's `name` in place of its `description`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Content,
    EnhancedContent,
    Name,
    Description,
    Rules,
    /// The attachment metadata blob: file name, MIME type, syntax, plaintext size.
    Meta,
}

impl Field {
    fn as_str(self) -> &'static str {
        match self {
            Field::Content => "content",
            Field::EnhancedContent => "enhanced_content",
            Field::Name => "name",
            Field::Description => "description",
            Field::Rules => "rules",
            Field::Meta => "meta",
        }
    }
}

/// Everything that binds a ciphertext to the one slot it belongs in.
#[derive(Debug, Clone)]
pub struct Binding<'a> {
    pub user_id: &'a str,
    pub kind: Kind,
    pub record_id: &'a str,
    pub field: Field,
    pub epoch: u32,
}

impl Binding<'_> {
    /// The additional authenticated data.
    ///
    /// **Invariant:** every component is a UUID or one of the fixed keywords above, none of
    /// which contain `|`, so the parts cannot be confused with one another and no length
    /// prefixing is needed. The day someone puts a user-supplied string in here that stops
    /// being true, and canonicalisation ambiguity turns into real forgery.
    fn aad(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}",
            MAGIC,
            self.user_id,
            self.kind.as_str(),
            self.record_id,
            self.field.as_str(),
            self.epoch
        )
    }
}

/// Why a value could not be opened. Every variant means the same thing to a caller - do not
/// use this value - but they are separated because the log line is the only thing anyone
/// will have when a report comes in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvelopeError {
    /// Not an envelope at all. Ordinary for a record written before encryption.
    NotAnEnvelope,
    /// A format this build does not know, e.g. `SPE2`.
    UnsupportedVersion(String),
    /// The right shape but damaged: wrong part count, bad base64, wrong nonce length.
    Malformed(&'static str),
    /// Sealed under an epoch whose key this client does not hold.
    UnknownEpoch(u32),
    /// The tag did not verify: wrong key, wrong binding, or tampering.
    NotAuthentic,
}

/// A parsed but still sealed envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    pub epoch: u32,
    nonce: [u8; NONCE_LEN],
    ciphertext: Vec<u8>,
}

/// Cheap prefix test, for deciding whether a column holds ciphertext.
///
/// A plaintext stash could in principle begin with `SPE1.`, and that is fine: it fails to
/// parse or fails to authenticate, and the caller's rule is to leave a value it cannot open
/// alone rather than to guess.
pub fn is_envelope(value: &str) -> bool {
    value.starts_with(concat!("SPE1", "."))
}

/// Split an envelope into its parts without needing a key.
pub fn parse(value: &str) -> Result<Envelope, EnvelopeError> {
    let mut parts = value.split('.');
    let magic = parts.next().ok_or(EnvelopeError::NotAnEnvelope)?;

    if magic != MAGIC {
        // Tell a future format apart from something that was never an envelope, so the log
        // says "update Stashpad" rather than "this record is broken".
        if magic.len() == 4 && magic.starts_with("SPE") {
            return Err(EnvelopeError::UnsupportedVersion(magic.to_string()));
        }
        return Err(EnvelopeError::NotAnEnvelope);
    }

    let epoch = parts
        .next()
        .ok_or(EnvelopeError::Malformed("missing epoch"))?
        .parse::<u32>()
        .map_err(|_| EnvelopeError::Malformed("epoch is not a number"))?;

    let nonce_b64 = parts.next().ok_or(EnvelopeError::Malformed("missing nonce"))?;
    let ct_b64 = parts
        .next()
        .ok_or(EnvelopeError::Malformed("missing ciphertext"))?;

    if parts.next().is_some() {
        return Err(EnvelopeError::Malformed("too many parts"));
    }

    let nonce_bytes = URL_SAFE_NO_PAD
        .decode(nonce_b64)
        .map_err(|_| EnvelopeError::Malformed("nonce is not base64url"))?;
    if nonce_bytes.len() != NONCE_LEN {
        return Err(EnvelopeError::Malformed("nonce is the wrong length"));
    }
    let mut nonce = [0u8; NONCE_LEN];
    nonce.copy_from_slice(&nonce_bytes);

    let ciphertext = URL_SAFE_NO_PAD
        .decode(ct_b64)
        .map_err(|_| EnvelopeError::Malformed("ciphertext is not base64url"))?;
    if ciphertext.len() < TAG_LEN {
        return Err(EnvelopeError::Malformed("ciphertext is too short to carry a tag"));
    }

    Ok(Envelope {
        epoch,
        nonce,
        ciphertext,
    })
}

/// Derive the key for one family of fields from the account content key.
///
/// One HKDF per family, not per record. Per-record keys were considered and rejected: nonce
/// hygiene is already solved by 192 random bits, there is no sharing feature for selective
/// disclosure to serve, and a per-record key derived from the same content key limits no
/// blast radius. The domain separation is kept because it is nearly free and leaves room to
/// hand out one family's key later without minting a new epoch.
pub fn field_key(content_key: &[u8; 32], kind: Kind, field: Field) -> [u8; 32] {
    use hkdf::Hkdf;
    use sha2::Sha256;

    let info = format!("stashpad/v1/{}/{}", kind.as_str(), field.as_str());
    let hk = Hkdf::<Sha256>::from_prk(content_key).expect("a 32-byte key is a valid PRK");
    let mut key = [0u8; 32];
    hk.expand(info.as_bytes(), &mut key)
        .expect("32 bytes is within the HKDF output limit");
    key
}

/// Open an envelope, given the content key for its epoch.
///
/// `available_epoch` is the epoch this client holds a key for; anything else is reported as
/// [`EnvelopeError::UnknownEpoch`] rather than attempted, because trying would only produce
/// an authentication failure and lose the reason.
pub fn open(
    content_key: &[u8; 32],
    available_epoch: u32,
    value: &str,
    binding: &Binding<'_>,
) -> Result<String, EnvelopeError> {
    use chacha20poly1305::{
        aead::{Aead, KeyInit, Payload},
        XChaCha20Poly1305, XNonce,
    };

    let envelope = parse(value)?;
    if envelope.epoch != available_epoch {
        return Err(EnvelopeError::UnknownEpoch(envelope.epoch));
    }

    // The binding's epoch has to be the one actually on the value, or the additional data
    // would describe a different slot than the one being opened.
    let binding = Binding {
        epoch: envelope.epoch,
        ..binding.clone()
    };

    let key = field_key(content_key, binding.kind, binding.field);
    let cipher = XChaCha20Poly1305::new_from_slice(&key)
        .map_err(|_| EnvelopeError::Malformed("bad key length"))?;

    let aad = binding.aad();
    let plaintext = cipher
        .decrypt(
            XNonce::from_slice(&envelope.nonce),
            Payload {
                msg: &envelope.ciphertext,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| EnvelopeError::NotAuthentic)?;

    String::from_utf8(plaintext).map_err(|_| EnvelopeError::Malformed("plaintext is not UTF-8"))
}

/// Seal a value into an envelope.
///
/// Present so the format has a reference implementation and the tests below can exercise it
/// from both ends. **Nothing in the sync path calls this yet** - see the module note on why
/// reading has to ship first.
pub fn seal(
    content_key: &[u8; 32],
    plaintext: &str,
    binding: &Binding<'_>,
) -> Result<String, EnvelopeError> {
    use chacha20poly1305::{
        aead::{Aead, KeyInit, Payload},
        XChaCha20Poly1305, XNonce,
    };
    use rand::RngCore;

    let key = field_key(content_key, binding.kind, binding.field);
    let cipher = XChaCha20Poly1305::new_from_slice(&key)
        .map_err(|_| EnvelopeError::Malformed("bad key length"))?;

    let mut nonce = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce);

    let aad = binding.aad();
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plaintext.as_bytes(),
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| EnvelopeError::Malformed("encryption failed"))?;

    Ok(format!(
        "{}.{}.{}.{}",
        MAGIC,
        binding.epoch,
        URL_SAFE_NO_PAD.encode(nonce),
        URL_SAFE_NO_PAD.encode(&ciphertext)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 32] = [42u8; 32];

    fn binding<'a>() -> Binding<'a> {
        Binding {
            user_id: "11111111-1111-4111-8111-111111111111",
            kind: Kind::Stash,
            record_id: "22222222-2222-4222-8222-222222222222",
            field: Field::Content,
            epoch: 1,
        }
    }

    #[test]
    fn a_value_round_trips() {
        let sealed = seal(&KEY, "a stash with ünïcode and\nnewlines", &binding()).expect("seal");
        assert!(is_envelope(&sealed));
        assert_eq!(
            open(&KEY, 1, &sealed, &binding()).expect("open"),
            "a stash with ünïcode and\nnewlines"
        );
    }

    #[test]
    fn an_empty_value_round_trips() {
        let sealed = seal(&KEY, "", &binding()).expect("seal");
        assert_eq!(open(&KEY, 1, &sealed, &binding()).expect("open"), "");
    }

    #[test]
    fn the_epoch_is_readable_without_a_key() {
        let sealed = seal(&KEY, "x", &binding()).expect("seal");
        assert_eq!(parse(&sealed).expect("parse").epoch, 1);
    }

    #[test]
    fn a_different_key_does_not_open_it() {
        let sealed = seal(&KEY, "secret", &binding()).expect("seal");
        assert_eq!(
            open(&[7u8; 32], 1, &sealed, &binding()),
            Err(EnvelopeError::NotAuthentic)
        );
    }

    /// Ids are client-generated and the server's key is `(id, user_id)`, so without the user
    /// in the additional data a blob moved from one account to another would open cleanly.
    #[test]
    fn a_value_moved_to_another_account_does_not_open() {
        let sealed = seal(&KEY, "secret", &binding()).expect("seal");
        let other = Binding {
            user_id: "99999999-9999-4999-8999-999999999999",
            ..binding()
        };
        assert_eq!(open(&KEY, 1, &sealed, &other), Err(EnvelopeError::NotAuthentic));
    }

    #[test]
    fn a_value_moved_to_another_record_does_not_open() {
        let sealed = seal(&KEY, "secret", &binding()).expect("seal");
        let other = Binding {
            record_id: "33333333-3333-4333-8333-333333333333",
            ..binding()
        };
        assert_eq!(open(&KEY, 1, &sealed, &other), Err(EnvelopeError::NotAuthentic));
    }

    /// Both are sealed strings on the same row under the same content key, so only the
    /// additional data stops one being served as the other.
    #[test]
    fn enhanced_content_cannot_be_served_as_content() {
        let enhanced = Binding {
            field: Field::EnhancedContent,
            ..binding()
        };
        let sealed = seal(&KEY, "the rewritten copy", &enhanced).expect("seal");
        assert_eq!(open(&KEY, 1, &sealed, &binding()), Err(EnvelopeError::NotAuthentic));
    }

    #[test]
    fn a_context_name_cannot_be_served_as_its_description() {
        let name = Binding {
            kind: Kind::Context,
            field: Field::Name,
            ..binding()
        };
        let description = Binding {
            field: Field::Description,
            ..name.clone()
        };
        let sealed = seal(&KEY, "Client project", &name).expect("seal");
        assert_eq!(
            open(&KEY, 1, &sealed, &description),
            Err(EnvelopeError::NotAuthentic)
        );
    }

    /// The epoch is authenticated, so rewriting the header to point at an older, compromised
    /// epoch is a tag failure rather than a successful downgrade.
    #[test]
    fn the_epoch_cannot_be_rewritten() {
        let sealed = seal(&KEY, "secret", &binding()).expect("seal");
        let downgraded = sealed.replacen("SPE1.1.", "SPE1.0.", 1);
        assert_eq!(
            open(&KEY, 0, &downgraded, &binding()),
            Err(EnvelopeError::NotAuthentic)
        );
    }

    #[test]
    fn an_epoch_this_client_has_no_key_for_is_reported_as_such() {
        let future = Binding { epoch: 9, ..binding() };
        let sealed = seal(&KEY, "secret", &future).expect("seal");
        assert_eq!(
            open(&KEY, 1, &sealed, &binding()),
            Err(EnvelopeError::UnknownEpoch(9))
        );
    }

    #[test]
    fn a_tampered_ciphertext_does_not_open() {
        let sealed = seal(&KEY, "secret", &binding()).expect("seal");
        let (head, tail) = sealed.rsplit_once('.').expect("four parts");
        let mut raw = URL_SAFE_NO_PAD.decode(tail).expect("base64url");
        let last = raw.len() - 1;
        raw[last] ^= 0xff;
        let tampered = format!("{}.{}", head, URL_SAFE_NO_PAD.encode(&raw));
        assert_eq!(open(&KEY, 1, &tampered, &binding()), Err(EnvelopeError::NotAuthentic));
    }

    #[test]
    fn a_future_format_is_refused_by_prefix() {
        let sealed = seal(&KEY, "secret", &binding()).expect("seal");
        let future = sealed.replacen("SPE1", "SPE2", 1);
        assert_eq!(
            parse(&future),
            Err(EnvelopeError::UnsupportedVersion("SPE2".to_string()))
        );
    }

    #[test]
    fn plaintext_is_not_mistaken_for_an_envelope() {
        assert!(!is_envelope("just a normal stash"));
        assert_eq!(parse("just a normal stash"), Err(EnvelopeError::NotAnEnvelope));
    }

    /// A stash whose text genuinely starts with the marker looks like an envelope to the
    /// cheap test and must still fail closed rather than opening as something else.
    #[test]
    fn a_plaintext_stash_that_starts_with_the_marker_fails_closed() {
        let value = "SPE1. was a format I read about today";
        assert!(is_envelope(value));
        assert!(matches!(parse(value), Err(EnvelopeError::Malformed(_))));
        assert!(open(&KEY, 1, value, &binding()).is_err());
    }

    #[test]
    fn malformed_shapes_are_refused() {
        assert!(matches!(parse("SPE1.1.onlythree"), Err(EnvelopeError::Malformed(_))));
        assert!(matches!(
            parse("SPE1.notanumber.AAAA.AAAA"),
            Err(EnvelopeError::Malformed(_))
        ));
        assert!(matches!(
            parse("SPE1.1.tooshort.AAAAAAAAAAAAAAAAAAAAAAAA"),
            Err(EnvelopeError::Malformed(_))
        ));
        let nonce = URL_SAFE_NO_PAD.encode([0u8; NONCE_LEN]);
        assert!(matches!(
            parse(&format!("SPE1.1.{}.AAAA", nonce)),
            Err(EnvelopeError::Malformed(_))
        ));
        assert!(matches!(
            parse(&format!("SPE1.1.{}.AAAAAAAAAAAAAAAAAAAAAAAA.extra", nonce)),
            Err(EnvelopeError::Malformed(_))
        ));
    }

    /// Two seals of the same text differ, because the nonce is random. This is what makes a
    /// server unable to tell "nothing changed" by comparing bytes - worth pinning so nobody
    /// later builds a dedupe on the assumption that it can.
    #[test]
    fn sealing_the_same_text_twice_gives_different_bytes() {
        let a = seal(&KEY, "same", &binding()).expect("seal");
        let b = seal(&KEY, "same", &binding()).expect("seal");
        assert_ne!(a, b);
        assert_eq!(open(&KEY, 1, &a, &binding()).unwrap(), open(&KEY, 1, &b, &binding()).unwrap());
    }

    #[test]
    fn field_keys_are_separated_by_family() {
        let content = field_key(&KEY, Kind::Stash, Field::Content);
        let enhanced = field_key(&KEY, Kind::Stash, Field::EnhancedContent);
        let ctx_name = field_key(&KEY, Kind::Context, Field::Name);
        assert_ne!(content, enhanced);
        assert_ne!(content, ctx_name);
        assert_eq!(content, field_key(&KEY, Kind::Stash, Field::Content));
    }
}
