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

//! The one logger for the whole process, backend and webview alike.
//!
//! A bundled app has no terminal, so `println!` and the webview console go nowhere in a
//! release build; the log file is the only record there is. Everything therefore goes
//! through `log::`, and the webview reaches it through [`log_frontend`].
//!
//! The log plugin needs an `AppHandle` to find its folder, so it only exists once the app
//! is set up - but settings validation, the database migration and the credential store
//! probe all run before that. A logger installed then would be dropped, and the `log`
//! crate allows exactly one. So [`install`] puts a deferring logger in place as the first
//! thing `run()` does: it holds early records until [`attach`] hands it the plugin's
//! logger, replays them into it, and forwards everything after.
//!
//! The file is bounded: [`MAX_FILE_BYTES`] per file and [`KEEP_ARCHIVED`] older files
//! beside the active one, so about 1.5 MB at most however long the app runs.

use std::sync::{Mutex, OnceLock};

use log::{Level, LevelFilter, Log, Metadata, Record};

/// Size at which the active log file is archived and a new one started.
pub const MAX_FILE_BYTES: u128 = 512_000;

/// Archived files kept beside the active one.
pub const KEEP_ARCHIVED: usize = 2;

/// This crate's records: everything in a dev build, info and up in a release build.
pub const APP_LEVEL: LevelFilter = if cfg!(debug_assertions) {
    LevelFilter::Debug
} else {
    LevelFilter::Info
};

/// Everyone else's - Tauri and its plugins log every IPC call at trace, which once made
/// the file wipe itself every few seconds.
pub const OTHER_LEVEL: LevelFilter = LevelFilter::Warn;

/// Early records held at most. A loop at startup must not turn the buffer into a leak.
const EARLY_LIMIT: usize = 500;

/// Whether a record passes the same filter the file applies.
fn wanted(metadata: &Metadata) -> bool {
    let limit = if metadata.target().starts_with("app_lib") {
        APP_LEVEL
    } else {
        OTHER_LEVEL
    };
    metadata.level() <= limit
}

struct EarlyRecord {
    level: Level,
    target: String,
    message: String,
}

/// Holds records until the real logger exists, then forwards to it.
struct Deferred {
    inner: OnceLock<Box<dyn Log>>,
    early: Mutex<Vec<EarlyRecord>>,
}

impl Log for Deferred {
    fn enabled(&self, metadata: &Metadata) -> bool {
        match self.inner.get() {
            Some(inner) => inner.enabled(metadata),
            None => wanted(metadata),
        }
    }

    fn log(&self, record: &Record) {
        // The lock orders this against `attach`: a record either lands in the buffer
        // before the replay, or sees the inner logger after it - never between the two.
        let Ok(mut early) = self.early.lock() else {
            return;
        };
        if let Some(inner) = self.inner.get() {
            drop(early);
            inner.log(record);
        } else if wanted(record.metadata()) && early.len() < EARLY_LIMIT {
            early.push(EarlyRecord {
                level: record.level(),
                target: record.target().to_string(),
                message: record.args().to_string(),
            });
        }
    }

    fn flush(&self) {
        if let Some(inner) = self.inner.get() {
            inner.flush();
        }
    }
}

impl Deferred {
    const fn new() -> Self {
        Self {
            inner: OnceLock::new(),
            early: Mutex::new(Vec::new()),
        }
    }

    /// Take the real logger and replay the held records into it. False if it already had one.
    fn hand_over(&self, inner: Box<dyn Log>) -> bool {
        let Ok(mut early) = self.early.lock() else {
            return false;
        };
        if self.inner.set(inner).is_err() {
            return false;
        }
        let inner = self.inner.get().expect("just set");
        for record in early.drain(..) {
            inner.log(
                &Record::builder()
                    .level(record.level)
                    .target(&record.target)
                    .args(format_args!("{}", record.message))
                    .build(),
            );
        }
        true
    }
}

static LOGGER: Deferred = Deferred::new();

/// Install the deferring logger. Call first thing in `run()`, before anything can log.
pub fn install() {
    if log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(APP_LEVEL.max(OTHER_LEVEL));
    }
}

/// Hand over the real logger and replay what was held for it.
///
/// Early records are written with the time of the replay rather than of the event; they
/// all come from the first moments of startup, so the order is what matters, and that is
/// kept.
pub fn attach(inner: Box<dyn Log>, max_level: LevelFilter) {
    if LOGGER.hand_over(inner) {
        log::set_max_level(max_level);
    }
}

/// A line from the webview's logger (`$lib/utils/log.ts`).
///
/// Capped in length, because the webview can hand over anything, including a stack that
/// runs to megabytes, and one such line would otherwise push the whole session out of the
/// file. Truncated by character, not byte, so a multi-byte stack cannot panic the command.
#[tauri::command]
pub fn log_frontend(level: String, message: String) {
    const MAX_CHARS: usize = 4000;
    let level = match level.as_str() {
        "error" => Level::Error,
        "warn" => Level::Warn,
        "info" => Level::Info,
        _ => Level::Debug,
    };
    let trimmed: String = message.chars().take(MAX_CHARS).collect();
    let elided = message.chars().nth(MAX_CHARS).is_some();
    log::log!(
        target: "app_lib::frontend",
        level,
        "{}{}",
        trimmed,
        if elided { " […]" } else { "" }
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata(level: Level, target: &str) -> Metadata<'_> {
        Metadata::builder().level(level).target(target).build()
    }

    #[test]
    fn keeps_app_records_at_the_app_level() {
        assert!(wanted(&metadata(Level::Info, "app_lib::sync")));
        assert!(wanted(&metadata(Level::Error, "app_lib::frontend")));
        assert_eq!(
            wanted(&metadata(Level::Debug, "app_lib::sync")),
            cfg!(debug_assertions),
            "debug records are for dev builds only"
        );
    }

    #[test]
    fn keeps_only_warnings_from_everyone_else() {
        assert!(wanted(&metadata(Level::Warn, "tauri::ipc")));
        assert!(!wanted(&metadata(Level::Info, "tauri::ipc")));
        assert!(!wanted(&metadata(Level::Trace, "tauri::ipc")));
    }

    /// Counts what reaches it, standing in for the plugin's logger.
    struct Collect(std::sync::Arc<Mutex<Vec<String>>>);

    impl Log for Collect {
        fn enabled(&self, _: &Metadata) -> bool {
            true
        }
        fn log(&self, record: &Record) {
            self.0.lock().unwrap().push(record.args().to_string());
        }
        fn flush(&self) {}
    }

    #[test]
    fn holds_early_records_and_replays_them_in_order_once_attached() {
        let deferred = Deferred::new();
        let log_at = |level, target: &str, message: &str| {
            deferred.log(
                &Record::builder()
                    .level(level)
                    .target(target)
                    .args(format_args!("{}", message))
                    .build(),
            )
        };

        log_at(Level::Warn, "app_lib::keychain", "first");
        log_at(Level::Trace, "tauri::ipc", "noise");
        log_at(Level::Info, "app_lib::settings", "second");
        for _ in 0..EARLY_LIMIT + 50 {
            log_at(Level::Warn, "app_lib::loop", "flood");
        }
        assert_eq!(
            deferred.early.lock().unwrap().len(),
            EARLY_LIMIT,
            "early records are bounded and filtered"
        );

        let seen = std::sync::Arc::new(Mutex::new(Vec::new()));
        assert!(deferred.hand_over(Box::new(Collect(seen.clone()))));
        assert!(
            !deferred.hand_over(Box::new(Collect(seen.clone()))),
            "a second logger is refused rather than replacing the first"
        );
        log_at(Level::Info, "app_lib::later", "after");

        let seen = seen.lock().unwrap();
        assert_eq!(&seen[..2], ["first", "second"]);
        assert_eq!(seen.last().map(String::as_str), Some("after"));
        assert!(deferred.early.lock().unwrap().is_empty());
    }
}
