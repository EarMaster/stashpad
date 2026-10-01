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

//! Context import and export.
//!
//! This used to live in the webview: it built the markdown by hand, pulled every
//! attachment's bytes across IPC into JavaScript memory, ran JSZip on the UI thread, and
//! then wrote each imported stash back with one command per stash plus one per file. The
//! zip work and the command storm both blocked the window, so a large import or export
//! froze the app outright.
//!
//! Doing it here means the bytes never leave the Rust side, compression runs off the UI
//! thread, and an import is a single transaction instead of `2N + M` round trips.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;
use temporal_rs::Instant;
use uuid::Uuid;

use crate::models::{Attachment, Context, StashItem};
use crate::stashes::get_stash_cache_path;
use crate::state::DbState;
use crate::time;
use crate::uierror::UiError;
use crate::utils::get_app_dir;

/// Name of the markdown document inside an archive.
const MARKDOWN_ENTRY: &str = "export.md";

/// Folder holding the attachments inside an archive.
const ATTACHMENTS_DIR: &str = "attachments";

/// Two stashes counted as duplicates at or above this Jaccard score.
const DUPLICATE_THRESHOLD: f64 = 0.8;

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

/// Context metadata carried in the YAML frontmatter.
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveMetadata {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub rules: Vec<serde_json::Value>,
}

/// What an archive turned out to contain, for the conflict UI to act on.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub stashes: Vec<StashItem>,
    pub metadata: ArchiveMetadata,
    /// Ids of parsed stashes that look like something the context already holds.
    pub duplicate_ids: Vec<String>,
    /// Handle for the extracted files; pass back to `commit_import` or `discard_import`.
    pub token: String,
    /// How many `###` headings carried a date this build could not read.
    ///
    /// The old importer silently replaced an unreadable date with the current time, so a
    /// bad archive quietly lost every creation date. Reporting the count lets the UI say
    /// so instead.
    pub unreadable_dates: u32,
    /// Attachments the document links to that the archive does not contain.
    ///
    /// They are skipped on import. Said up front because the old importer skipped them
    /// with nothing but a log line, and a stash quietly arriving without its screenshot
    /// is found out much later, if at all.
    pub missing_attachments: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSummary {
    pub stashes: u32,
    pub attachments: u32,
    pub path: String,
}

// ---------------------------------------------------------------------------
// Dates
// ---------------------------------------------------------------------------

/// An instant as `YYYY-MM-DD HH:MM:SS` in UTC: how a `###` heading is written, and the
/// "Exported from Stashpad on" line.
///
/// The previous exporter used JavaScript's `toLocaleString()`, whose output depends on
/// the machine's locale - the same archive read on another machine could not be parsed
/// back reliably. This format is unambiguous, and older builds, which read headings with
/// JavaScript's date parser, still accept it, so archives written here stay readable by
/// them.
fn format_utc(instant: &Instant) -> String {
    let (y, mo, d, h, mi, s) = time::utc_fields(instant);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}:{s:02}")
}

/// Formats a stored RFC3339 timestamp for a heading, falling back to the raw string.
fn format_heading_date(created_at: &str) -> String {
    match time::parse(created_at) {
        Some(instant) => format_utc(&instant),
        None => created_at.to_string(),
    }
}

/// Parse a `###` heading date, accepting both the current format and what earlier
/// versions produced through `toLocaleString()` in the common locales.
///
/// Returns `None` rather than substituting the current time, so the caller can report
/// how much of an archive it could not read.
fn parse_heading_date(raw: &str) -> Option<Instant> {
    let text = raw.trim();

    // RFC 3339, the current `YYYY-MM-DD HH:MM:SS`, ISO without a zone, and a bare
    // `YYYY-MM-DD`. Zoneless forms are treated as UTC: the exporter writes UTC, and an
    // older archive carries no zone at all, so there is nothing better to assume.
    if let Some(instant) = time::parse(text) {
        return Some(instant);
    }

    parse_locale_heading(text)
}

/// The `toLocaleString()` forms older exports used, which no ISO parser reads:
///
/// * `8/20/2026, 10:14:32 AM` and `8/20/2026, 10:14:32` - en-US, month first
/// * `20.8.2026, 10:14:32` and `20.8.2026 10:14:32` - de-DE
/// * `20/8/2026, 10:14:32` and `20/8/2026 10:14:32` - en-GB and similar, day first
///
/// A slash date is tried month first, as en-US is far the more common, and read day first
/// only when that cannot be a date - the same order the formats were tried in before.
fn parse_locale_heading(text: &str) -> Option<Instant> {
    let (date, rest) = text.split_once([',', ' '])?;
    let mut clock = rest.trim_start_matches([',', ' ']).split(' ');
    let hms = clock.next()?;
    let meridiem = clock.next();
    if clock.next().is_some() {
        return None;
    }

    let mut t = hms.split(':').map(|n| n.parse::<u8>().ok());
    let (mut hour, minute, second) = (t.next()??, t.next()??, t.next()??);
    if t.next().is_some() {
        return None;
    }
    match meridiem.map(str::to_ascii_uppercase).as_deref() {
        None => {}
        Some(m @ ("AM" | "PM")) => {
            if !(1..=12).contains(&hour) {
                return None;
            }
            hour = match (m, hour) {
                ("AM", 12) => 0,
                ("PM", 12) => 12,
                ("PM", h) => h + 12,
                (_, h) => h,
            };
        }
        Some(_) => return None,
    }

    let separator = if date.contains('/') { '/' } else { '.' };
    let mut d = date.split(separator).map(|n| n.parse::<u16>().ok());
    let (a, b, year) = (d.next()??, d.next()??, d.next()??);
    if d.next().is_some() {
        return None;
    }
    let year = i32::from(year);
    let at = |month: u16, day: u16| {
        time::from_utc_fields(
            year,
            u8::try_from(month).ok()?,
            u8::try_from(day).ok()?,
            hour,
            minute,
            second,
        )
    };
    match separator {
        '/' => at(a, b).or_else(|| at(b, a)),
        _ => at(b, a),
    }
}

// ---------------------------------------------------------------------------
// Markdown generation
// ---------------------------------------------------------------------------

/// One attachment as it goes into an archive.
struct ArchivedFile {
    /// The attachment's id, or empty for a legacy `files` path, which never had one.
    id: String,
    /// The name the stash shows it under.
    name: String,
    /// Where its bytes are on this device. Empty when they never arrived here.
    source: String,
    /// Its name inside `attachments/`.
    entry: String,
}

/// Name an attachment gets inside an archive: `<attachment id>_<file name>`.
///
/// The id is what keeps it unique. The previous `<8 chars of stash id>_<file name>` gave
/// two attachments of one name in one stash - two pasted `image.png`s, the ordinary case -
/// the same entry, the zip writer refused the second, and the export died part way with
/// one file written. The name is kept after the id so someone looking for a particular
/// file in the archive can still find it by name.
fn archive_entry_name(key: &str, file_name: &str) -> String {
    let name: String = file_name
        .chars()
        .map(|c| {
            // Separators would make a directory of it, and the rest are refused by
            // Windows when someone extracts the archive there.
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let name = if name.trim().is_empty() {
        "attachment".to_string()
    } else {
        name
    };
    format!("{}_{}", key, name)
}

/// Every file a stash refers to, as it will be placed in an archive.
fn archived_files(stash: &StashItem) -> Vec<ArchivedFile> {
    let legacy = stash.files.iter().map(|path| {
        let name = Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone());
        // No id to key on, so a fresh one: it has the same shape, which is what the
        // importer recognises and strips.
        let entry = archive_entry_name(&Uuid::new_v4().to_string(), &name);
        ArchivedFile {
            id: String::new(),
            name,
            source: path.clone(),
            entry,
        }
    });
    let current = stash.attachments.iter().map(|a| ArchivedFile {
        id: a.id.clone(),
        name: a.file_name.clone(),
        source: a.file_path.clone(),
        entry: archive_entry_name(&a.id, &a.file_name),
    });
    legacy.chain(current).collect()
}

/// Every attachment name a stash refers to, legacy `files` paths included.
fn stash_file_names(stash: &StashItem) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();

    for path in &stash.files {
        let name = Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone());
        names.push(name);
    }
    for att in &stash.attachments {
        names.push(att.file_name.clone());
    }

    names
}

/// Build the markdown document for a set of stashes.
///
/// Mirrors the layout the webview produced, so archives stay mutually readable.
///
/// `archived` is what goes into the archive, per stash id, and `None` for a markdown-only
/// export. The links are written from it rather than worked out again here, so a link and
/// the entry it points at cannot disagree.
fn build_markdown(
    context_name: &str,
    metadata: &ArchiveMetadata,
    stashes: &[StashItem],
    archived: Option<&HashMap<String, Vec<ArchivedFile>>>,
    exported_at: Instant,
) -> String {
    let mut out = String::new();

    let frontmatter = serde_yaml::to_string(metadata).unwrap_or_else(|_| "name: ''\n".to_string());
    out.push_str("---\n");
    out.push_str(frontmatter.trim());
    out.push_str("\n---\n\n");

    out.push_str(&format!("# {}\n\n", context_name));
    out.push_str(&format!(
        "Exported from Stashpad on {}\n\n",
        format_utc(&exported_at)
    ));
    out.push_str(&format!("Total stashes: {}\n\n---\n\n", stashes.len()));

    let mut active: Vec<&StashItem> = stashes.iter().filter(|s| !s.completed).collect();
    let mut completed: Vec<&StashItem> = stashes.iter().filter(|s| s.completed).collect();

    // Newest first, matching the queue's own ordering.
    active.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    completed.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    for (title, group) in [("Active", &active), ("Completed", &completed)] {
        if group.is_empty() {
            continue;
        }
        out.push_str(&format!("## {} Stashes ({})\n\n", title, group.len()));

        for stash in group.iter() {
            out.push_str(&format!(
                "### {}\n\n",
                format_heading_date(&stash.created_at)
            ));
            // The id, so references between stashes in this archive can be pointed at
            // the new ids an import gives them. A comment renders as nothing, so the
            // document still reads cleanly, and older builds import it as invisible text.
            out.push_str(&format!("{}{} -->\n\n", STASH_ID_MARKER, stash.id));

            if !stash.content.trim().is_empty() {
                out.push_str(stash.content.trim_end());
                out.push_str("\n\n");
            }

            let lines: Vec<String> = match archived {
                Some(archived) => archived
                    .get(&stash.id)
                    .map(|files| {
                        files
                            .iter()
                            .map(|f| format!("- [{}]({}/{})", f.name, ATTACHMENTS_DIR, f.entry))
                            .collect()
                    })
                    .unwrap_or_default(),
                None => stash_file_names(stash)
                    .iter()
                    .map(|name| format!("- {}", name))
                    .collect(),
            };
            if !lines.is_empty() {
                out.push_str("**Attachments:**\n");
                for line in &lines {
                    out.push_str(line);
                    out.push('\n');
                }
                out.push('\n');
            }

            out.push_str("---\n\n");
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Markdown parsing
// ---------------------------------------------------------------------------

struct ParsedDocument {
    stashes: Vec<StashItem>,
    metadata: ArchiveMetadata,
    unreadable_dates: u32,
}

/// True for the 36 bytes of a hyphenated UUID.
fn is_uuid(bytes: &[u8]) -> bool {
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => *b == b'-',
            _ => b.is_ascii_hexdigit(),
        })
}

/// Strip the prefix the exporter adds, recovering the original name.
///
/// Two shapes exist: `<attachment id>_` from now on, and the `<8 hex chars of stash id>_`
/// every earlier archive carries.
fn strip_archive_prefix(name: &str) -> String {
    let bytes = name.as_bytes();
    if bytes.len() > 37 && bytes[36] == b'_' && is_uuid(&bytes[..36]) {
        return name[37..].to_string();
    }
    if bytes.len() > 9
        && bytes[8] == b'_'
        // Compared on the raw bytes. The previous `name[..8].chars()` was safe - the
        // `bytes[8] == b'_'` guard above already proves byte 8 starts a character - but
        // it allocated a char iterator over a slice we only ever test for ASCII digits.
        && bytes[..8].iter().all(|b| b.is_ascii_hexdigit())
    {
        return name[9..].to_string();
    }
    name.to_string()
}

/// Parse an exported document back into stashes.
///
/// Deliberately reproduces the previous parser's behaviour, quirks included: blank lines
/// inside a stash's content are dropped, and a bare `---` ends the stash. Changing either
/// would change how already-exported archives import, which is a separate decision.
/// Opens the line under each stash heading that records the stash's id.
const STASH_ID_MARKER: &str = "<!-- stash-id: ";

/// The id recorded on a marker line, if this is one.
fn parse_stash_id_marker(line: &str) -> Option<&str> {
    let id = line
        .trim()
        .strip_prefix(STASH_ID_MARKER)?
        .strip_suffix(" -->")?;
    (!id.is_empty() && !id.contains(char::is_whitespace)).then_some(id)
}

/// Point `stash:<old>` references at the ids an import gave their targets.
///
/// Only targets inside the archive are rewritten. A reference to anything else keeps
/// its id, which still resolves if that stash exists on this device.
fn remap_references(stashes: &mut [StashItem], new_ids: &HashMap<String, String>) {
    if new_ids.is_empty() {
        return;
    }
    for stash in stashes.iter_mut() {
        if !stash.content.contains("stash:") {
            continue;
        }
        let mut content = stash.content.clone();
        for (old, new) in new_ids {
            content = content.replace(&format!("(stash:{})", old), &format!("(stash:{})", new));
        }
        stash.content = content;
    }
}

fn parse_markdown(content: &str, context_id: &str) -> ParsedDocument {
    let mut metadata = ArchiveMetadata::default();
    let mut body = content;

    if let Some(rest) = content.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---") {
            let frontmatter = &rest[..end];
            if let Ok(parsed) = serde_yaml::from_str::<ArchiveMetadata>(frontmatter) {
                metadata = parsed;
            }
            body = &rest[end + 4..];
        }
    }

    let mut stashes: Vec<StashItem> = Vec::new();
    let mut unreadable_dates = 0u32;

    let mut current: Option<StashItem> = None;
    let mut content_lines: Vec<String> = Vec::new();
    let mut files: Vec<String> = Vec::new();
    let mut in_attachments = false;
    let mut section_completed = false;
    let mut new_ids: HashMap<String, String> = HashMap::new();

    // Close the stash under construction and push it.
    macro_rules! flush {
        () => {
            if let Some(mut stash) = current.take() {
                stash.content = content_lines.join("\n").trim().to_string();
                stash.files = std::mem::take(&mut files);
                stashes.push(stash);
                content_lines.clear();
            }
        };
    }

    for line in body.lines() {
        if let Some(section) = parse_section_header(line) {
            flush!();
            section_completed = section;
            in_attachments = false;
            continue;
        }

        if let Some(heading) = line.strip_prefix("### ") {
            flush!();

            let created_at = match parse_heading_date(heading) {
                Some(instant) => time::to_canonical(&instant),
                None => {
                    unreadable_dates += 1;
                    time::now_iso()
                }
            };

            current = Some(StashItem {
                id: Uuid::new_v4().to_string(),
                content: String::new(),
                enhanced_content: None,
                files: Vec::new(),
                attachments: Vec::new(),
                created_at,
                context_id: context_id.to_string(),
                completed: section_completed,
                completed_at: if section_completed {
                    Some(time::now_iso())
                } else {
                    None
                },
                updated_at: None,
                deleted: false,
            });
            content_lines.clear();
            files.clear();
            in_attachments = false;
            continue;
        }

        if current.is_none() {
            continue;
        }

        if content_lines.is_empty() {
            if let (Some(old), Some(stash)) = (parse_stash_id_marker(line), current.as_ref()) {
                new_ids.insert(old.to_lowercase(), stash.id.clone());
                continue;
            }
        }

        if line.starts_with("**Attachments:**") {
            in_attachments = true;
            continue;
        }

        if line == "---" {
            in_attachments = false;
            continue;
        }

        if in_attachments {
            if let Some(name) = parse_attachment_line(line) {
                files.push(name);
            }
            continue;
        }

        if !line.trim().is_empty() {
            content_lines.push(line.to_string());
        }
    }

    flush!();
    remap_references(&mut stashes, &new_ids);

    ParsedDocument {
        stashes,
        metadata,
        unreadable_dates,
    }
}

/// `## Active Stashes (N)` / `## Completed Stashes (N)` → whether it is the completed one.
fn parse_section_header(line: &str) -> Option<bool> {
    let rest = line.strip_prefix("## ")?;
    let (kind, tail) = if let Some(t) = rest.strip_prefix("Active Stashes (") {
        (false, t)
    } else {
        (true, rest.strip_prefix("Completed Stashes (")?)
    };

    let count = tail.strip_suffix(')')?;
    if count.is_empty() || !count.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(kind)
}

/// `- [name](attachments/entry)` or the plain `- name` form.
///
/// The linked form comes back as its target, `attachments/<entry>`, because that is the
/// only thing that finds the file again. Reducing it to the bare name, which this used to
/// do, left the importer rebuilding the entry from the stash's id - and an imported stash
/// has a new id, so not one file of an exported archive was ever found.
fn parse_attachment_line(line: &str) -> Option<String> {
    let rest = line.strip_prefix("- ")?;

    if let Some(open) = rest.find("](") {
        if rest.starts_with('[') && rest.ends_with(')') {
            let target = &rest[open + 2..rest.len() - 1];
            if target.starts_with(&format!("{}/", ATTACHMENTS_DIR)) {
                return Some(target.to_string());
            }
            return Some(strip_archive_prefix(target));
        }
    }

    Some(strip_archive_prefix(rest))
}

/// Where a parsed reference's bytes should be in an extracted archive, and its real name.
///
/// `None` for the plain `- name` form: an archive exported without its attachments lists
/// them by name only, and there is nothing to look for.
fn archived_reference(extract_dir: &Path, reference: &str) -> Option<(PathBuf, String)> {
    let entry = reference.strip_prefix(&format!("{}/", ATTACHMENTS_DIR))?;
    let source = safe_entry_path(&extract_dir.join(ATTACHMENTS_DIR), entry)?;
    Some((source, strip_archive_prefix(entry)))
}

// ---------------------------------------------------------------------------
// Duplicate detection
// ---------------------------------------------------------------------------

fn normalise(content: &str) -> String {
    content
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Jaccard similarity over word sets - the same measure the webview used, moved here
/// because it is quadratic in the number of stashes and was running on the UI thread.
fn similarity(a: &str, b: &str) -> f64 {
    let words_a: HashSet<&str> = a.split_whitespace().collect();
    let words_b: HashSet<&str> = b.split_whitespace().collect();

    let union = words_a.union(&words_b).count();
    if union == 0 {
        return 0.0;
    }
    words_a.intersection(&words_b).count() as f64 / union as f64
}

fn find_duplicates(parsed: &[StashItem], existing: &[StashItem]) -> Vec<String> {
    let existing_norm: Vec<String> = existing
        .iter()
        .map(|s| normalise(&s.content))
        .filter(|s| !s.is_empty())
        .collect();

    parsed
        .iter()
        .filter(|candidate| {
            let norm = normalise(&candidate.content);
            if norm.is_empty() {
                return false;
            }
            existing_norm
                .iter()
                .any(|other| *other == norm || similarity(&norm, other) > DUPLICATE_THRESHOLD)
        })
        .map(|s| s.id.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// Archive I/O
// ---------------------------------------------------------------------------

fn transfer_temp_root() -> PathBuf {
    get_app_dir().join("transfer")
}

/// Reject an archive entry whose name would escape the directory it is extracted into.
fn safe_entry_path(base: &Path, name: &str) -> Option<PathBuf> {
    let candidate = Path::new(name);
    if candidate.components().any(|c| {
        matches!(
            c,
            std::path::Component::ParentDir | std::path::Component::RootDir
        )
    }) {
        return None;
    }
    Some(base.join(candidate))
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Write a context's stashes to `dest_path`, as a zip when attachments are included and
/// there is at least one, otherwise as a plain markdown file.
#[tauri::command]
pub async fn export_context_archive(
    state: State<'_, Arc<DbState>>,
    context_id: String,
    stash_ids: Vec<String>,
    include_attachments: bool,
    dest_path: String,
) -> Result<ExportSummary, UiError> {
    let wanted: HashSet<String> = stash_ids.into_iter().collect();

    let (context, stashes) = {
        let db = state.lock_db();
        let contexts = db.get_contexts().map_err(|e| e.to_string())?;
        let context = contexts.into_iter().find(|c: &Context| c.id == context_id);

        let all = db.get_stashes().map_err(|e| e.to_string())?;
        let selected: Vec<StashItem> = all
            .into_iter()
            .filter(|s| s.context_id == context_id && wanted.contains(&s.id))
            .collect();

        (context, selected)
    };

    let context_name = context
        .as_ref()
        .map(|c| c.name.clone())
        .unwrap_or_else(|| context_id.clone());

    let metadata = ArchiveMetadata {
        name: context_name.clone(),
        description: context
            .as_ref()
            .and_then(|c| c.description.clone())
            .unwrap_or_default(),
        rules: context
            .as_ref()
            .map(|c| {
                c.rules
                    .iter()
                    .map(|r| serde_json::to_value(r).unwrap_or(serde_json::Value::Null))
                    .collect()
            })
            .unwrap_or_default(),
    };

    let archived: HashMap<String, Vec<ArchivedFile>> = if include_attachments {
        stashes
            .iter()
            .map(|s| (s.id.clone(), archived_files(s)))
            .filter(|(_, files)| !files.is_empty())
            .collect()
    } else {
        HashMap::new()
    };

    // An archive that says it holds a context's attachments has to hold all of them. This
    // used to drop whatever was not on disk and still link it from the document, so the
    // archive looked complete and was not. Refused instead, naming the files, with the ids
    // the interface needs to fetch them from the cloud and try again.
    let in_order = || stashes.iter().filter_map(|s| archived.get(&s.id)).flatten();
    let missing: Vec<&ArchivedFile> = in_order()
        .filter(|f| f.source.trim().is_empty() || !Path::new(&f.source).exists())
        .collect();
    if !missing.is_empty() {
        let names = missing
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let ids = missing
            .iter()
            .filter(|f| !f.id.is_empty())
            .map(|f| f.id.as_str())
            .collect::<Vec<_>>()
            .join(",");
        return Err(UiError::with_values(
            "transfer.attachments_missing",
            format!("These attachments are not on this device: {}", names),
            [
                ("count", missing.len().to_string()),
                ("names", names),
                ("ids", ids),
            ],
        ));
    }

    let markdown = build_markdown(
        &context_name,
        &metadata,
        &stashes,
        (!archived.is_empty()).then_some(&archived),
        time::now(),
    );

    let dest = PathBuf::from(&dest_path);
    let attachment_count = archived.values().map(Vec::len).sum::<usize>() as u32;
    let stash_count = stashes.len() as u32;

    let files: Vec<(String, String)> = in_order()
        .map(|f| (f.source.clone(), format!("{}/{}", ATTACHMENTS_DIR, f.entry)))
        .collect();

    // Deflating every attachment in a context is the single heaviest thing this app
    // does, so it belongs on the blocking pool rather than an async worker. Tokio does
    // not migrate a task that blocks its worker, so exporting a large context inline
    // took a worker out of circulation for the whole compression pass.
    let write_dest = dest.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), UiError> {
        if let Some(parent) = write_dest.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create folder: {}", e))?;
        }

        if files.is_empty() {
            fs::write(&write_dest, markdown)
                .map_err(|e| format!("Failed to write export: {}", e))?;
            return Ok(());
        }

        // Built beside the destination and renamed into place, so a failure part way
        // leaves no half an archive under the name the user chose.
        let mut partial = write_dest.clone().into_os_string();
        partial.push(".partial");
        let partial = PathBuf::from(partial);

        let written = write_zip(&partial, &markdown, &files).and_then(|()| {
            fs::rename(&partial, &write_dest)
                .map_err(|e| format!("Failed to write export: {}", e).into())
        });
        if written.is_err() {
            let _ = fs::remove_file(&partial);
        }
        written
    })
    .await
    .map_err(|e| format!("Export task failed: {}", e))??;

    Ok(ExportSummary {
        stashes: stash_count,
        attachments: attachment_count,
        path: dest_path,
    })
}

/// Write an export archive to `path`.
///
/// Every file it is given has to make it in, so a read that fails ends the export rather
/// than leaving a gap the document still links to.
fn write_zip(path: &Path, markdown: &str, files: &[(String, String)]) -> Result<(), UiError> {
    let file = fs::File::create(path).map_err(|e| format!("Failed to write export: {}", e))?;
    let mut zip = zip::ZipWriter::new(file);
    let options: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file(MARKDOWN_ENTRY, options)
        .map_err(|e| e.to_string())?;
    zip.write_all(markdown.as_bytes())
        .map_err(|e| e.to_string())?;

    for (source, entry) in files {
        let mut src =
            fs::File::open(source).map_err(|e| format!("Failed to read {}: {}", source, e))?;
        zip.start_file(entry.as_str(), options)
            .map_err(|e| e.to_string())?;
        std::io::copy(&mut src, &mut zip)
            .map_err(|e| format!("Failed to read {}: {}", source, e))?;
    }

    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

/// Read an archive and report what importing it would bring in.
#[tauri::command]
pub async fn read_import_archive(
    state: State<'_, Arc<DbState>>,
    path: String,
    context_id: String,
) -> Result<ImportPreview, UiError> {
    let source = PathBuf::from(&path);
    if !source.exists() {
        return Err("File does not exist".into());
    }

    let token = Uuid::new_v4().to_string();
    let temp_dir = transfer_temp_root().join(&token);

    let is_zip = source
        .extension()
        .map(|e| e.eq_ignore_ascii_case("zip"))
        .unwrap_or(false);

    // Inflating the archive to disk is blocking work, so it runs on the blocking pool.
    let extract_dir = temp_dir.clone();
    let markdown = tauri::async_runtime::spawn_blocking(move || -> Result<String, UiError> {
        fs::create_dir_all(&extract_dir).map_err(|e| format!("Failed to prepare import: {}", e))?;
        if is_zip {
            extract_archive(&source, &extract_dir)
        } else {
            fs::read_to_string(&source)
                .map_err(|e| format!("Failed to read file: {}", e))
                .map_err(UiError::from)
        }
    })
    .await
    .map_err(|e| format!("Import task failed: {}", e))??;

    let parsed = parse_markdown(&markdown, &context_id);

    let existing = {
        let db = state.lock_db();
        db.get_stashes()
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|s| s.context_id == context_id)
            .collect::<Vec<_>>()
    };

    let duplicate_ids = find_duplicates(&parsed.stashes, &existing);

    let missing_attachments = parsed
        .stashes
        .iter()
        .flat_map(|s| s.files.iter())
        .filter_map(|reference| archived_reference(&temp_dir, reference))
        .filter(|(source, _)| !source.is_file())
        .map(|(_, name)| name)
        .collect();

    Ok(ImportPreview {
        stashes: parsed.stashes,
        metadata: parsed.metadata,
        duplicate_ids,
        token,
        unreadable_dates: parsed.unreadable_dates,
        missing_attachments,
    })
}

/// Unpack a zip into `dest`, returning the markdown document it carried.
fn extract_archive(source: &Path, dest: &Path) -> Result<String, UiError> {
    let file = fs::File::open(source).map_err(|e| format!("Failed to open archive: {}", e))?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|e| format!("Not a readable archive: {}", e))?;

    let mut markdown: Option<String> = None;

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();

        if entry.is_dir() {
            continue;
        }

        if name.ends_with(".md") && markdown.is_none() {
            let mut text = String::new();
            entry
                .read_to_string(&mut text)
                .map_err(|e| format!("Failed to read {}: {}", name, e))?;
            markdown = Some(text);
            continue;
        }

        // Attachments are written out under their archive names; the parser refers to
        // them by the same names, minus the stash-id prefix.
        let Some(target) = safe_entry_path(dest, &name) else {
            log::warn!(
                "[Import] refusing archive entry with a traversing path: {}",
                name
            );
            continue;
        };

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = fs::File::create(&target).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
    }

    markdown
        .ok_or_else(|| "The archive contains no markdown document".to_string())
        .map_err(UiError::from)
}

/// Write the selected stashes and their files into the context, in one transaction.
#[tauri::command]
pub async fn commit_import(
    state: State<'_, Arc<DbState>>,
    context_id: String,
    stashes: Vec<StashItem>,
    token: String,
) -> Result<u32, UiError> {
    let temp_dir = transfer_temp_root().join(&token);

    // All the file copying happens on the blocking pool: an import can move hundreds of
    // attachments, and every byte of that was previously copied on an async worker.
    let copy_context = context_id.clone();
    let copy_temp = temp_dir.clone();
    let (prepared, placed) = tauri::async_runtime::spawn_blocking(
        move || -> Result<(Vec<StashItem>, Vec<PathBuf>), UiError> {
            // Every file placed so far, so a failure can take them back out. Otherwise a
            // half-finished import leaves files in the cache that no row points at.
            let mut placed: Vec<PathBuf> = Vec::new();
            match place_import_files(stashes, &copy_context, &copy_temp, &mut placed) {
                Ok(prepared) => Ok((prepared, placed)),
                Err(e) => {
                    remove_all(&placed);
                    Err(e)
                }
            }
        },
    )
    .await
    .map_err(|e| format!("Import task failed: {}", e))??;

    // One transaction for the lot, rather than the two-commands-per-stash-plus-one-per-file
    // the webview used to issue. insert_local_stashes, not import_stashes: these records
    // are new on this device and have to reach the cloud, so they stay pending.
    let inserted = state.lock_db().insert_local_stashes(&prepared);
    if let Err(e) = inserted {
        remove_all(&placed);
        return Err(e.to_string().into());
    }

    let cleanup_dir = temp_dir.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || fs::remove_dir_all(&cleanup_dir)).await;

    Ok(prepared.len() as u32)
}

/// Take back files an import placed before it failed.
fn remove_all(paths: &[PathBuf]) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

/// Copy each stash's files out of the extraction directory into its own cache folder,
/// building the attachment rows as it goes.
///
/// A file the archive does not contain is skipped - the preview already named it. A file
/// that is there and cannot be copied ends the import instead: it is an error on this
/// machine, and importing the rest without it would lose it without a word.
fn place_import_files(
    stashes: Vec<StashItem>,
    context_id: &str,
    extract_dir: &Path,
    placed: &mut Vec<PathBuf>,
) -> Result<Vec<StashItem>, UiError> {
    let mut prepared: Vec<StashItem> = Vec::with_capacity(stashes.len());

    for mut stash in stashes {
        stash.context_id = context_id.to_string();

        let mut attachments: Vec<Attachment> = Vec::new();
        let references: Vec<(PathBuf, String)> = stash
            .files
            .iter()
            .filter_map(|reference| archived_reference(extract_dir, reference))
            .collect();

        if !references.is_empty() {
            let target_dir = get_stash_cache_path(&stash.id, Some(context_id));
            fs::create_dir_all(&target_dir)
                .map_err(|e| format!("Failed to create attachment folder: {}", e))?;

            for (source, name) in &references {
                if !source.is_file() {
                    log::warn!("[Import] {} is referenced but not in the archive", name);
                    continue;
                }

                // Reserved rather than joined: two attachments of one name in the same
                // stash used to land on the same path, so the second copy replaced the
                // first one's bytes and both rows pointed at the survivor.
                let dest = crate::utils::reserve_unique_path(&target_dir, name)
                    .map_err(|e| format!("Could not place {}: {}", name, e))?;
                placed.push(dest.clone());
                fs::copy(source, &dest).map_err(|e| format!("Could not place {}: {}", name, e))?;

                let size = fs::metadata(&dest).map(|m| m.len()).unwrap_or(0) as i64;
                attachments.push(Attachment {
                    id: Uuid::new_v4().to_string(),
                    stash_id: stash.id.clone(),
                    file_path: dest.to_string_lossy().into_owned(),
                    file_name: name.clone(),
                    file_size: size,
                    mime_type: mime_guess::from_path(&dest).first().map(|m| m.to_string()),
                    syntax: None,
                    created_at: time::now_iso(),
                });
            }
        }

        // The legacy `files` column is not carried forward; attachments replace it.
        stash.files = Vec::new();
        stash.attachments = attachments;
        prepared.push(stash);
    }

    Ok(prepared)
}

/// Drop the files an aborted import had extracted.
#[tauri::command]
pub async fn discard_import(token: String) -> Result<(), UiError> {
    // Guard against a caller handing us something that is not one of our own tokens.
    if Uuid::parse_str(&token).is_err() {
        return Err("Invalid import token".into());
    }
    let _ = fs::remove_dir_all(transfer_temp_root().join(token));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stash(id: &str, content: &str, created_at: &str, completed: bool) -> StashItem {
        StashItem {
            id: id.to_string(),
            content: content.to_string(),
            enhanced_content: None,
            files: Vec::new(),
            attachments: Vec::new(),
            created_at: created_at.to_string(),
            context_id: "ctx".to_string(),
            completed,
            completed_at: None,
            updated_at: None,
            deleted: false,
        }
    }

    fn metadata() -> ArchiveMetadata {
        ArchiveMetadata {
            name: "Work".to_string(),
            description: "notes".to_string(),
            rules: Vec::new(),
        }
    }

    #[test]
    fn a_document_survives_a_round_trip() {
        let stashes = vec![
            stash("a", "first entry", "2026-08-18T10:00:00Z", false),
            stash("b", "second entry", "2026-08-17T09:30:00Z", true),
        ];

        let md = build_markdown("Work", &metadata(), &stashes, None, time::now());
        let parsed = parse_markdown(&md, "ctx");

        assert_eq!(parsed.unreadable_dates, 0);
        assert_eq!(parsed.stashes.len(), 2);
        assert_eq!(parsed.metadata, metadata());

        let active = parsed.stashes.iter().find(|s| !s.completed).unwrap();
        let done = parsed.stashes.iter().find(|s| s.completed).unwrap();

        assert_eq!(active.content, "first entry");
        assert_eq!(done.content, "second entry");
        assert!(active.created_at.starts_with("2026-08-18T10:00:00"));
        assert!(done.created_at.starts_with("2026-08-17T09:30:00"));
    }

    #[test]
    fn references_inside_the_archive_follow_their_targets_to_new_ids() {
        let a = "aaaaaaaa-0000-4000-8000-000000000001";
        let b = "bbbbbbbb-0000-4000-8000-000000000002";
        let outside = "cccccccc-0000-4000-8000-000000000003";
        let stashes = vec![
            stash(
                a,
                &format!("see [b](stash:{b}) and [c](stash:{outside})"),
                "2026-08-18T10:00:00Z",
                false,
            ),
            stash(b, "the target", "2026-08-17T09:30:00Z", true),
        ];

        let md = build_markdown("Work", &metadata(), &stashes, None, time::now());
        let parsed = parse_markdown(&md, "ctx");

        let referrer = parsed.stashes.iter().find(|s| !s.completed).unwrap();
        let target = parsed.stashes.iter().find(|s| s.completed).unwrap();
        assert_ne!(target.id, b, "an import mints new ids");
        assert_eq!(target.content, "the target", "the id marker is not content");
        assert_eq!(
            referrer.content,
            format!("see [b](stash:{}) and [c](stash:{outside})", target.id)
        );
    }

    /// Non-ASCII names are left intact rather than mistaken for a prefixed one.
    #[test]
    fn a_non_ascii_attachment_name_keeps_its_leading_characters() {
        // Each 'ü' is two bytes, so byte 8 lands mid-character in this name.
        assert_eq!(strip_archive_prefix("üüüüüüüüü.png"), "üüüüüüüüü.png");
        // An emoji-led name is longer than nine bytes but has no boundary at 8 either.
        assert_eq!(strip_archive_prefix("🎉🎉🎉_shot.png"), "🎉🎉🎉_shot.png");
        // A genuine prefix is still stripped.
        assert_eq!(strip_archive_prefix("abcdef12_shot.png"), "shot.png");
        // Eight characters that are not hex are left alone.
        assert_eq!(
            strip_archive_prefix("zzzzzzzz_shot.png"),
            "zzzzzzzz_shot.png"
        );
        // The attachment-id prefix archives are written with now.
        assert_eq!(
            strip_archive_prefix("0cdf01cd-f5be-49a2-840b-cd1d12f43a42_shot.png"),
            "shot.png"
        );
        // Something UUID-length that is not one keeps its name.
        assert_eq!(
            strip_archive_prefix("zzzzzzzz-f5be-49a2-840b-cd1d12f43a42_shot.png"),
            "zzzzzzzz-f5be-49a2-840b-cd1d12f43a42_shot.png"
        );
    }

    fn attachment(id: &str, stash_id: &str, name: &str) -> Attachment {
        Attachment {
            id: id.into(),
            stash_id: stash_id.into(),
            file_path: format!("/cache/ctx/{}/{}", stash_id, name),
            file_name: name.into(),
            file_size: 1,
            mime_type: None,
            syntax: None,
            created_at: "2026-08-18T10:00:00Z".into(),
        }
    }

    const ATT_A: &str = "11111111-2222-4333-8444-555555555555";
    const ATT_B: &str = "66666666-7777-4888-8999-aaaaaaaaaaaa";

    fn archived_for(stashes: &[StashItem]) -> HashMap<String, Vec<ArchivedFile>> {
        stashes
            .iter()
            .map(|s| (s.id.clone(), archived_files(s)))
            .collect()
    }

    #[test]
    fn attachment_names_round_trip_without_their_archive_prefix() {
        let mut item = stash("abcdef12", "has a file", "2026-08-18T10:00:00Z", false);
        item.attachments
            .push(attachment(ATT_A, "abcdef12", "shot.png"));

        let items = [item];
        let md = build_markdown(
            "Work",
            &metadata(),
            &items,
            Some(&archived_for(&items)),
            time::now(),
        );
        let entry = format!("attachments/{}_shot.png", ATT_A);
        assert!(md.contains(&format!("- [shot.png]({})", entry)), "{}", md);

        // The importer keeps the link target, because the imported stash has a new id and
        // the target is the only thing that finds the file.
        let parsed = parse_markdown(&md, "ctx");
        assert_eq!(parsed.stashes[0].files, vec![entry.clone()]);

        let (source, name) = archived_reference(Path::new("/tmp/import"), &entry).unwrap();
        assert_eq!(name, "shot.png");
        assert_eq!(
            source,
            Path::new("/tmp/import/attachments").join(format!("{}_shot.png", ATT_A))
        );
    }

    /// Two pasted `image.png`s in one stash used to share an entry, and the zip writer
    /// refused the second one - ending the export with one file written.
    #[test]
    fn two_attachments_of_one_name_get_their_own_entries() {
        let mut item = stash("b53d26f6", "two images", "2026-09-25T12:49:10Z", false);
        item.attachments
            .push(attachment(ATT_A, "b53d26f6", "image.png"));
        item.attachments
            .push(attachment(ATT_B, "b53d26f6", "image.png"));

        let files = archived_files(&item);
        assert_eq!(files.len(), 2);
        assert_ne!(files[0].entry, files[1].entry);
        assert!(files.iter().all(|f| f.entry.ends_with("_image.png")));
    }

    #[test]
    fn an_entry_name_cannot_make_a_directory() {
        assert_eq!(
            archive_entry_name(ATT_A, "../a/b\\c.png"),
            format!("{}_.._a_b_c.png", ATT_A)
        );
        assert_eq!(
            archive_entry_name(ATT_A, "  "),
            format!("{}_attachment", ATT_A)
        );
    }

    /// Archives written before this change link `<8 chars of stash id>_<name>`.
    #[test]
    fn attachments_in_older_archives_are_still_found() {
        let md = concat!(
            "## Active Stashes (1)

### 2026-09-25 12:49:10

old

",
            "**Attachments:**
- [image.png](attachments/b53d26f6_image.png)

---
",
        );
        let parsed = parse_markdown(md, "ctx");
        let reference = &parsed.stashes[0].files[0];

        let (source, name) = archived_reference(Path::new("/x"), reference).unwrap();
        assert_eq!(name, "image.png");
        assert!(source.ends_with("b53d26f6_image.png"));
    }

    /// The name-only list an export without attachments writes has nothing to look for.
    #[test]
    fn a_name_only_reference_is_not_expected_in_the_archive() {
        assert!(archived_reference(Path::new("/x"), "shot.png").is_none());
    }

    #[test]
    fn the_archive_holds_every_file_it_links() {
        let dir = std::env::temp_dir().join(format!("stashpad-export-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let first = dir.join("one.png");
        let second = dir.join("two.png");
        fs::write(&first, b"first").unwrap();
        fs::write(&second, b"second").unwrap();

        let files = vec![
            (
                first.to_string_lossy().into_owned(),
                format!("attachments/{}_image.png", ATT_A),
            ),
            (
                second.to_string_lossy().into_owned(),
                format!("attachments/{}_image.png", ATT_B),
            ),
        ];
        let out = dir.join("export.zip");
        write_zip(&out, "# doc", &files).unwrap();

        let mut zip = zip::ZipArchive::new(fs::File::open(&out).unwrap()).unwrap();
        assert_eq!(zip.len(), 3);
        let mut body = String::new();
        zip.by_name(&files[1].1)
            .unwrap()
            .read_to_string(&mut body)
            .unwrap();
        assert_eq!(body, "second");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_cannot_be_read_fails_the_archive() {
        let dir = std::env::temp_dir().join(format!("stashpad-export-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let files = vec![(
            dir.join("gone.png").to_string_lossy().into_owned(),
            "attachments/x_gone.png".to_string(),
        )];
        assert!(write_zip(&dir.join("export.zip"), "# doc", &files).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn dates_written_by_older_builds_are_still_read() {
        // These are what JavaScript's toLocaleString() produced, which is what every
        // archive exported before this change contains.
        let en_us = parse_heading_date("8/20/2026, 10:14:32 AM").expect("en-US must parse");
        assert_eq!(format_utc(&en_us), "2026-08-20 10:14:32");

        let de_de = parse_heading_date("20.8.2026, 10:14:32").expect("de-DE must parse");
        assert_eq!(format_utc(&de_de), "2026-08-20 10:14:32");

        let current = parse_heading_date("2026-08-20 10:14:32").expect("current format must parse");
        assert_eq!(format_utc(&current), "2026-08-20 10:14:32");
    }

    #[test]
    fn every_locale_form_the_old_formats_list_accepted_still_parses() {
        // The chrono-based parser tried a fixed list of formats; the hand-written one that
        // replaced it has to agree with it on each of them.
        for (raw, expected) in [
            ("8/20/2026, 10:14:32 PM", "2026-08-20 22:14:32"),
            ("8/20/2026, 12:05:00 AM", "2026-08-20 00:05:00"),
            ("8/20/2026, 12:05:00 PM", "2026-08-20 12:05:00"),
            ("8/20/2026, 22:14:32", "2026-08-20 22:14:32"),
            ("20.08.2026 10:14:32", "2026-08-20 10:14:32"),
            // Day first only when month first cannot be a date.
            ("20/8/2026, 10:14:32", "2026-08-20 10:14:32"),
            ("20/8/2026 10:14:32", "2026-08-20 10:14:32"),
            ("3/4/2026, 10:14:32", "2026-03-04 10:14:32"),
            ("2026-08-20T10:14:32", "2026-08-20 10:14:32"),
            ("2026-08-20", "2026-08-20 00:00:00"),
            ("2026-08-20T10:14:32+02:00", "2026-08-20 08:14:32"),
        ] {
            let parsed = parse_heading_date(raw).unwrap_or_else(|| panic!("{raw} must parse"));
            assert_eq!(format_utc(&parsed), expected, "{raw}");
        }

        for raw in [
            "13/13/2026, 10:14:32",
            "8/20/2026, 13:00:00 PM",
            "8/20/2026, 10:14 AM",
            "8/20/2026",
            "20.8.2026, 25:00:00",
        ] {
            assert!(parse_heading_date(raw).is_none(), "{raw} must not parse");
        }
    }

    #[test]
    fn an_unreadable_date_is_reported_rather_than_silently_replaced() {
        // The old importer substituted the current time and said nothing, so an archive
        // it could not read lost every creation date without a word.
        let md = "## Active Stashes (1)\n\n### not a date at all\n\nsome content\n\n---\n";
        let parsed = parse_markdown(md, "ctx");

        assert_eq!(parsed.stashes.len(), 1);
        assert_eq!(
            parsed.unreadable_dates, 1,
            "the caller has to be able to tell the user"
        );
    }

    #[test]
    fn duplicate_detection_matches_only_near_identical_content() {
        let existing = vec![stash(
            "e1",
            "buy milk and eggs today",
            "2026-08-18T10:00:00Z",
            false,
        )];
        let incoming = vec![
            stash(
                "i1",
                "buy milk and eggs today",
                "2026-08-18T10:00:00Z",
                false,
            ),
            stash(
                "i2",
                "completely unrelated content here",
                "2026-08-18T10:00:00Z",
                false,
            ),
        ];

        let dupes = find_duplicates(&incoming, &existing);
        assert_eq!(dupes, vec!["i1".to_string()]);
    }

    #[test]
    fn a_traversing_archive_entry_is_refused() {
        let base = Path::new("/tmp/import");
        assert!(safe_entry_path(base, "attachments/ok.png").is_some());
        assert!(safe_entry_path(base, "../../etc/passwd").is_none());
    }
}
