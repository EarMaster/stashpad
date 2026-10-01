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

//! The backend's only source of dates and times, on the Temporal API (`temporal_rs`).
//!
//! Every timestamp the app stores as text is written by [`to_canonical`]: RFC 3339 in UTC,
//! always nine fractional digits, always `Z` - the same format the frontend and the cloud
//! write. The width is fixed because SQLite compares these columns as text, and the
//! `completed_at < cutoff` purge and the newest-first export ordering both rely on string
//! order matching time order. Before this, Rust wrote `+00:00` with up to nine digits and
//! the frontend wrote `Z` with three, in the same columns.
//!
//! Nothing else in the crate reads the clock: no `chrono`, no `SystemTime`.
//! `npm run check:temporal` refuses both.

use std::str::FromStr;

use temporal_rs::options::{Disambiguation, ToStringRoundingOptions};
use temporal_rs::parsers::Precision;
use temporal_rs::{Instant, PlainDateTime, Temporal, TimeZone};

/// What [`to_canonical`] writes for an instant it somehow cannot format. Never expected:
/// formatting a UTC instant has no failure path in practice, but the API is fallible.
const EPOCH: &str = "1970-01-01T00:00:00.000000000Z";

/// The current instant.
pub fn now() -> Instant {
    Temporal::utc_now()
        .instant()
        .or_else(|_| Instant::try_new(0))
        .expect("the epoch is a valid instant")
}

/// The current time in whole Unix seconds - the unit `updated_at` is stored in.
pub fn now_ts() -> u64 {
    u64::try_from(now().epoch_milliseconds() / 1000).unwrap_or(0)
}

/// The current time in Unix milliseconds - the unit the update-check settings use.
pub fn now_ms() -> u64 {
    u64::try_from(now().epoch_milliseconds()).unwrap_or(0)
}

/// The current instant in the canonical text format.
pub fn now_iso() -> String {
    to_canonical(&now())
}

/// `days` before now, in the canonical text format, for `< cutoff` comparisons.
pub fn days_ago_iso(days: i64) -> String {
    // Temporal refuses to add calendar days to an instant - a day is not a fixed length
    // once a timezone is involved - so the cutoff is counted in exact 24-hour days, which is
    // what chrono's `Duration::days` meant too.
    let nanos = i128::from(days) * 86_400 * 1_000_000_000;
    Instant::try_new(now().as_i128() - nanos)
        .map(|i| to_canonical(&i))
        .unwrap_or_else(|_| EPOCH.to_string())
}

/// An instant as RFC 3339 in UTC with exactly nine fractional digits and a `Z`.
pub fn to_canonical(instant: &Instant) -> String {
    let options = ToStringRoundingOptions {
        precision: Precision::Digit(9),
        ..Default::default()
    };
    instant
        .to_ixdtf_string(None, options)
        .unwrap_or_else(|_| EPOCH.to_string())
}

/// Parse a stored or received timestamp.
///
/// Accepts RFC 3339 at any width and with any offset, the space-separated form, and a
/// zoneless date or date-time, which is read as UTC - the only zone anything here ever
/// wrote. Returns `None` rather than "now" for anything else, so the caller decides.
pub fn parse(raw: &str) -> Option<Instant> {
    let text = raw.trim();
    if text.is_empty() {
        return None;
    }
    if let Ok(instant) = Instant::from_str(text) {
        return Some(instant);
    }
    // Zoneless. `Instant` refuses these by design; a plain date-time accepts them.
    let plain = PlainDateTime::from_str(text).ok()?;
    plain
        .to_zoned_date_time(TimeZone::utc(), Disambiguation::Compatible)
        .ok()
        .map(|zoned| zoned.to_instant())
}

/// A timestamp rewritten in the canonical format, or returned unchanged when it does not
/// parse - a writer must not invent a time, and an unreadable value is still better kept
/// than dropped.
pub fn canonical(raw: &str) -> String {
    parse(raw).map_or_else(|| raw.to_string(), |instant| to_canonical(&instant))
}

/// [`canonical`] for an optional column.
pub fn canonical_opt(raw: Option<&str>) -> Option<String> {
    raw.map(canonical)
}

/// An instant's wall-clock fields in UTC: `(year, month, day, hour, minute, second)`.
pub fn utc_fields(instant: &Instant) -> (i32, u8, u8, u8, u8, u8) {
    match instant.to_zoned_date_time_iso(TimeZone::utc()) {
        Ok(z) => (
            z.year(),
            z.month(),
            z.day(),
            z.hour(),
            z.minute(),
            z.second(),
        ),
        Err(_) => (1970, 1, 1, 0, 0, 0),
    }
}

/// A UTC wall-clock time as an instant. `None` for a date or time that does not exist.
pub fn from_utc_fields(
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
) -> Option<Instant> {
    PlainDateTime::try_new_iso(year, month, day, hour, minute, second, 0, 0, 0)
        .ok()?
        .to_zoned_date_time(TimeZone::utc(), Disambiguation::Compatible)
        .ok()
        .map(|zoned| zoned.to_instant())
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: &str = "2026-08-22T10:00:00.000000000Z";

    #[test]
    fn canonical_has_a_fixed_width() {
        assert_eq!(canonical("2026-08-22T10:00:00Z"), AT);
        assert_eq!(
            canonical("2026-08-22T10:00:00.5Z"),
            "2026-08-22T10:00:00.500000000Z"
        );
        // The reason for the fixed width: string order is time order.
        assert!(canonical("2026-08-22T10:00:00Z") < canonical("2026-08-22T10:00:00.5Z"));
    }

    #[test]
    fn parse_accepts_every_form_the_app_ever_wrote() {
        for raw in [
            "2026-08-22T10:00:00Z",
            "2026-08-22T10:00:00.000Z",
            "2026-08-22T10:00:00+00:00",
            "2026-08-22T12:00:00+02:00",
            "2026-08-22T10:00:00.000000000+00:00",
            "2026-08-22 10:00:00+00:00",
            "2026-08-22 10:00:00",
            "2026-08-22T10:00:00",
        ] {
            assert_eq!(canonical(raw), AT, "{raw}");
        }
        assert_eq!(canonical("2026-08-22"), "2026-08-22T00:00:00.000000000Z");
    }

    #[test]
    fn unreadable_values_are_kept_not_replaced() {
        assert_eq!(parse("yesterday"), None);
        assert_eq!(parse("  "), None);
        assert_eq!(canonical("yesterday"), "yesterday");
        assert_eq!(canonical_opt(None), None);
    }

    #[test]
    fn days_ago_is_exact_days_before_now() {
        let cutoff = parse(&days_ago_iso(2)).unwrap();
        let elapsed_ms = now().epoch_milliseconds() - cutoff.epoch_milliseconds();
        assert!((2 * 86_400_000..2 * 86_400_000 + 5_000).contains(&elapsed_ms));
    }

    #[test]
    fn utc_fields_round_trip() {
        let instant = parse(AT).unwrap();
        assert_eq!(utc_fields(&instant), (2026, 8, 22, 10, 0, 0));
        assert_eq!(from_utc_fields(2026, 8, 22, 10, 0, 0), Some(instant));
        assert_eq!(from_utc_fields(2026, 2, 30, 0, 0, 0), None);
    }

    #[test]
    fn the_clock_units_agree() {
        let ts = now_ts();
        let ms = now_ms();
        assert!(ms / 1000 >= ts && ms / 1000 - ts <= 1);
    }
}
