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

//! The global shortcut that shows and hides the window.
//!
//! The plugin's handler (in `lib.rs`) does the toggling; this is what puts a key
//! combination behind it. The combination is `settings.shortcuts["global_toggle"]`, a string
//! the shortcut picker (`ShortcutInput.svelte`) writes in the plugin's own format, for
//! example `CommandOrControl+Shift+K`.
//!
//! It is registered at startup and again whenever the setting changes. Both used to be
//! missing: the registration lived in `save_settings` and went when that file was split up,
//! and before that it only ever ran on a save, so it did not survive a restart either. Nothing
//! failed - the plugin simply had a handler and no shortcut to call it for.

use std::str::FromStr;
use std::sync::Mutex;

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::uierror::UiError;

/// The key in `settings.shortcuts` that holds the show/hide shortcut.
pub const GLOBAL_TOGGLE: &str = "global_toggle";

/// Why the current show/hide shortcut is not registered, for Settings to show beside it.
///
/// Kept rather than returned because registration also happens at startup, long before
/// anyone opens Settings, and because a failed shortcut must not fail the settings save.
static LAST_ERROR: Mutex<Option<UiError>> = Mutex::new(None);

/// Make `new` the show/hide shortcut and drop `old`.
///
/// Both are the raw setting, where an empty string means "none". A failure is logged and
/// remembered for [`global_shortcut_error`], never returned: it must not stop the rest of
/// the settings from saving.
pub fn apply_global_toggle(app: &AppHandle, old: Option<&str>, new: Option<&str>) {
    let shortcuts = app.global_shortcut();

    if let Some(old) = old.filter(|s| !s.is_empty()) {
        if let Err(e) = shortcuts.unregister(old) {
            // Not fatal: it may never have registered in the first place.
            log::debug!("Could not unregister shortcut '{}': {}", old, e);
        }
    }

    let outcome = match new.filter(|s| !s.is_empty()) {
        None => None,
        Some(new) => register(app, new).err(),
    };
    *crate::state::lock_or_recover(&LAST_ERROR) = outcome;
}

fn register(app: &AppHandle, shortcut: &str) -> Result<(), UiError> {
    let parsed = Shortcut::from_str(shortcut).map_err(|e| {
        log::warn!("Global shortcut '{}' cannot be parsed: {}", shortcut, e);
        UiError::new(
            "shortcut.invalid",
            "This shortcut can't be used. Choose a different one.",
        )
    })?;
    match app.global_shortcut().register(parsed) {
        Ok(()) => {
            log::info!("Registered global shortcut '{}'", shortcut);
            Ok(())
        }
        Err(e) => {
            log::warn!("Could not register global shortcut '{}': {}", shortcut, e);
            Err(classify(parsed, &e.to_string()))
        }
    }
}

/// Tell "another app holds it" apart from every other failure.
///
/// The plugin hands errors on as text, so the library's own message for this shortcut is
/// rebuilt and compared, rather than its wording being copied here. The OS only reports a
/// clash on Windows and X11; macOS accepts a combination another app already holds, so
/// there it can never be detected and nothing is shown.
fn classify(shortcut: Shortcut, message: &str) -> UiError {
    if message == global_hotkey::Error::AlreadyRegistered(shortcut).to_string() {
        UiError::new(
            "shortcut.taken",
            "Another app already uses this shortcut. Choose a different one.",
        )
    } else {
        UiError::with_values(
            "shortcut.failed",
            format!("This shortcut couldn't be set up: {}", message),
            [("reason", message)],
        )
    }
}

/// Why the show/hide shortcut is not active, or nothing when it is (or none is set).
#[tauri::command]
pub fn global_shortcut_error() -> Option<UiError> {
    crate::state::lock_or_recover(&LAST_ERROR).clone()
}

/// The parsed shortcut, or why it was refused. For the tests.
#[cfg(test)]
fn parses(shortcut: &str) -> Result<Shortcut, String> {
    Shortcut::from_str(shortcut).map_err(|e| format!("{}: {}", shortcut, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every key name `ShortcutInput.svelte` can write, taken from its `KEY_MAP` and
    /// `CODE_MAP`. A name the plugin rejects does not error anywhere: the picker shows the
    /// shortcut, saves it, and it simply never fires.
    const KEYS: &[&str] = &[
        "A",
        "K",
        "Z",
        "0",
        "5",
        "9",
        "F1",
        "F5",
        "F12",
        "Up",
        "Down",
        "Left",
        "Right",
        "Home",
        "End",
        "PageUp",
        "PageDown",
        "Backspace",
        "Delete",
        "Insert",
        "Tab",
        "Enter",
        "Escape",
        "Space",
        "CapsLock",
        "NumLock",
        "ScrollLock",
        "PrintScreen",
        "Pause",
        "NumpadAdd",
        "NumpadSubtract",
        "NumpadMultiply",
        "NumpadDivide",
        "NumpadDecimal",
        "NumpadEnter",
        "Numpad0",
        "Numpad9",
        "Minus",
        "Equal",
    ];

    #[test]
    fn every_key_the_picker_writes_is_understood() {
        let rejected: Vec<String> = KEYS
            .iter()
            .filter_map(|key| parses(&format!("CommandOrControl+{}", key)).err())
            .collect();
        assert!(
            rejected.is_empty(),
            "the plugin rejects keys the picker offers: {:#?}",
            rejected
        );
    }

    #[test]
    fn every_modifier_the_picker_writes_is_understood() {
        for modifiers in [
            "CommandOrControl",
            "Control",
            "Super",
            "Alt",
            "Shift",
            "CommandOrControl+Shift",
            "Control+Alt+Shift",
            "Super+Alt",
        ] {
            parses(&format!("{}+K", modifiers)).expect(modifiers);
        }
    }

    #[test]
    fn a_shortcut_held_by_another_app_is_named_as_such() {
        let shortcut = parses("CommandOrControl+Shift+K").unwrap();
        let held = global_hotkey::Error::AlreadyRegistered(shortcut).to_string();
        assert_eq!(classify(shortcut, &held).code, "shortcut.taken");
    }

    #[test]
    fn any_other_failure_carries_its_reason() {
        let shortcut = parses("CommandOrControl+Shift+K").unwrap();
        let error = classify(shortcut, "RegisterEventHotKey failed for KeyK");
        assert_eq!(error.code, "shortcut.failed");
        assert_eq!(
            error.values.unwrap().get("reason").map(String::as_str),
            Some("RegisterEventHotKey failed for KeyK")
        );
    }

    #[test]
    fn a_nonsense_shortcut_is_refused_rather_than_accepted() {
        assert!(parses("CommandOrControl+NotAKey").is_err());
        assert!(parses("").is_err());
    }
}
