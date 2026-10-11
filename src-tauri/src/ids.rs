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

//! The shape a record id must have before it is trusted.
//!
//! Ids arrive from places this device does not control - the sync server, another
//! device, an imported file - and some of them end up as directory names under
//! `~/.stashpad/cache`. An id of `..\..\Startup` would put an attachment wherever it
//! pointed, so an id that is not a UUID is refused at the door rather than cleaned up
//! wherever it happens to be used.

/// A stash or attachment id: a hyphenated UUID, and nothing else.
///
/// `Uuid::try_parse` alone also accepts the braced and `urn:uuid:` forms; the length check
/// limits it to the 36-character hyphenated one every client generates, so no `:` or
/// brace ever reaches a path.
pub fn is_record_id(id: &str) -> bool {
    id.len() == 36 && uuid::Uuid::try_parse(id).is_ok()
}

/// A context id: a record id, or the literal `default` context every account has.
pub fn is_context_id(id: &str) -> bool {
    id == "default" || is_record_id(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_hyphenated_uuids_only() {
        assert!(is_record_id("0f8fad5b-d9cb-469f-a165-70867728950e"));
        assert!(is_record_id("0F8FAD5B-D9CB-469F-A165-70867728950E"));
        assert!(!is_record_id("0f8fad5bd9cb469fa16570867728950e"));
        assert!(!is_record_id("{0f8fad5b-d9cb-469f-a165-70867728950e}"));
        assert!(!is_record_id(
            "urn:uuid:0f8fad5b-d9cb-469f-a165-70867728950e"
        ));
        assert!(!is_record_id("default"));
        assert!(!is_record_id(""));
    }

    #[test]
    fn refuses_anything_that_could_walk_a_path() {
        for id in [
            "..",
            "../../../.config/autostart",
            "..\\..\\..\\AppData\\Roaming\\Microsoft\\Windows\\Start Menu\\Programs\\Startup",
            "/etc",
            "C:\\Windows",
            "C:version.dll",
        ] {
            assert!(!is_record_id(id), "{id}");
            assert!(!is_context_id(id), "{id}");
        }
    }

    #[test]
    fn context_ids_also_allow_default() {
        assert!(is_context_id("default"));
        assert!(is_context_id("0f8fad5b-d9cb-469f-a165-70867728950e"));
        assert!(!is_context_id("Default"));
    }
}
