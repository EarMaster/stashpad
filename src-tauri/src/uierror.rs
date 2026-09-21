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

//! The error a Tauri command hands to the interface.
//!
//! Errors used to cross as bare English strings and were printed exactly as they arrived, so
//! a German user met "There is nowhere safe on this machine to keep an encryption key" in the
//! middle of an otherwise translated panel. A string cannot be translated after the fact
//! without matching on its prose, which breaks the moment anyone edits the wording.
//!
//! So an error carries a `code` as well as its `message`. The interface looks up
//! `errors.<code>` and falls back to `message` when there is no translation, which means a
//! code can be added here and translated later without anything breaking in between, and an
//! error nobody expected still says something useful rather than nothing.
//!
//! # Why the `From` impls matter
//!
//! There are around 250 places that produce an error, and the overwhelming majority are
//! `map_err` wrappings of an IO or serde failure - things that should not happen and that no
//! translation improves. Those keep an empty code and are shown as they always were.
//!
//! `From<String>` and `From<&str>` exist so that `?` converts them silently. A function whose
//! signature changes to `Result<T, UiError>` keeps compiling with every existing
//! `.map_err(|e| e.to_string())?` and `.ok_or("...")?` untouched, and only the errors worth
//! naming get a code. Without that this change would have to rewrite all 250 sites at once.

use std::collections::BTreeMap;

use serde::Serialize;

/// An error on its way to the interface.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiError {
    /// Identifier the interface translates, e.g. `e2ee.nowhere_safe`. Empty when this error
    /// was never given one, which is the normal case for an unexpected failure.
    pub code: String,
    /// English, and always present. The fallback when a translation is missing, and what
    /// ends up in the log either way.
    pub message: String,
    /// Values a translation interpolates, e.g. the fingerprint in a mismatch.
    ///
    /// Without these a translated string would have to drop the detail the English one
    /// carries - "the codes do not match" instead of naming the code that was shown - which
    /// is a worse message than leaving it in English.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub values: Option<BTreeMap<String, String>>,
}

impl UiError {
    /// An error the interface can translate.
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            values: None,
        }
    }

    /// The same, with values a translation can interpolate.
    ///
    /// The English `message` should already read correctly on its own, because it is what
    /// gets shown until someone writes the translation.
    pub fn with_values<K, V>(
        code: &str,
        message: impl Into<String>,
        values: impl IntoIterator<Item = (K, V)>,
    ) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        Self {
            code: code.to_string(),
            message: message.into(),
            values: Some(
                values
                    .into_iter()
                    .map(|(k, v)| (k.into(), v.into()))
                    .collect(),
            ),
        }
    }

    /// An error that carries no code: unexpected, and shown as-is.
    pub fn plain(message: impl Into<String>) -> Self {
        Self {
            code: String::new(),
            message: message.into(),
            values: None,
        }
    }
}

impl std::fmt::Display for UiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The message alone: the code is for the interface, and a log line reading
        // "e2ee.nowhere_safe: There is nowhere safe..." helps nobody who is reading logs.
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for UiError {}

impl From<String> for UiError {
    fn from(message: String) -> Self {
        Self::plain(message)
    }
}

impl From<&str> for UiError {
    fn from(message: &str) -> Self {
        Self::plain(message)
    }
}

/// So a `UiError` can still be handed to something that wants a string, which several
/// internal helpers do.
impl From<UiError> for String {
    fn from(err: UiError) -> String {
        err.message
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_error_carries_no_code() {
        // The interface uses an empty code as "there is nothing to translate, print the
        // message", so this has to stay empty rather than become something like "unknown".
        let err: UiError = "disk on fire".to_string().into();
        assert_eq!(err.code, "");
        assert_eq!(err.message, "disk on fire");
    }

    #[test]
    fn the_question_mark_operator_converts_a_string_error() {
        fn inner() -> Result<(), String> {
            Err("no".to_string())
        }
        fn outer() -> Result<(), UiError> {
            inner()?;
            Ok(())
        }
        // This is the property the migration rests on: every existing `?` on a
        // Result<_, String> keeps compiling once a signature becomes Result<_, UiError>.
        let err = outer().unwrap_err();
        assert_eq!(err.code, "");
        assert_eq!(err.message, "no");
    }

    #[test]
    fn a_coded_error_keeps_both_halves() {
        let err = UiError::new("e2ee.nowhere_safe", "There is nowhere safe");
        assert_eq!(err.code, "e2ee.nowhere_safe");
        assert_eq!(err.message, "There is nowhere safe");
        // Display is the message only, because that is what reaches a log file.
        assert_eq!(err.to_string(), "There is nowhere safe");
    }

    #[test]
    fn it_serialises_as_the_interface_expects() {
        let json = serde_json::to_string(&UiError::new("a.b", "c")).unwrap();
        assert_eq!(json, r#"{"code":"a.b","message":"c"}"#);
    }
}
