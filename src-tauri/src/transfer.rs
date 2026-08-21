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

use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::models::{Attachment, Context, StashItem};
use crate::stashes::get_stash_cache_path;
use crate::state::DbState;
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

/// How a `###` heading is written from now on.
///
/// The previous exporter used JavaScript's `toLocaleString()`, whose output depends on
/// the machine's locale - the same archive read on another machine could not be parsed
/// back reliably. This format is unambiguous, and `new Date("2026-08-20 10:14:32")`
/// still accepts it, so archives written here stay readable by older builds.
const HEADING_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

/// Formats a stored RFC3339 timestamp for a heading, falling back to the raw string.
fn format_heading_date(created_at: &str) -> String {
    match DateTime::parse_from_rfc3339(created_at) {
        Ok(dt) => dt.with_timezone(&Utc).format(HEADING_FORMAT).to_string(),
        Err(_) => created_at.to_string(),
    }
}

/// Parse a `###` heading date, accepting both the current format and what earlier
/// versions produced through `toLocaleString()` in the common locales.
///
/// Returns `None` rather than substituting the current time, so the caller can report
/// how much of an archive it could not read.
fn parse_heading_date(raw: &str) -> Option<DateTime<Utc>> {
    let text = raw.trim();

    if let Ok(dt) = DateTime::parse_from_rfc3339(text) {
        return Some(dt.with_timezone(&Utc));
    }

    // Naive formats are treated as UTC: the exporter writes UTC, and an older archive
    // carries no zone at all, so there is nothing better to assume.
    const NAIVE_FORMATS: &[&str] = &[
        "%Y-%m-%d %H:%M:%S",     // what this build writes
        "%Y-%m-%dT%H:%M:%S",     // ISO without a zone
        "%m/%d/%Y, %I:%M:%S %p", // en-US
        "%m/%d/%Y, %H:%M:%S",
        "%d.%m.%Y, %H:%M:%S", // de-DE
        "%d.%m.%Y %H:%M:%S",
        "%d/%m/%Y, %H:%M:%S", // en-GB and similar
        "%d/%m/%Y %H:%M:%S",
        "%Y-%m-%d",
    ];

    for format in NAIVE_FORMATS {
        if let Ok(naive) = NaiveDateTime::parse_from_str(text, format) {
            return Some(Utc.from_utc_datetime(&naive));
        }
        // A date-only pattern parses as a date, not a datetime.
        if let Ok(date) = chrono::NaiveDate::parse_from_str(text, format) {
            return Some(Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0)?));
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Markdown generation
// ---------------------------------------------------------------------------

/// Short form of a stash id used to keep attachment names unique inside an archive.
fn stash_prefix(id: &str) -> String {
    id.chars().take(8).collect()
}

/// Name an attachment gets inside the archive: `<8 chars of stash id>_<file name>`.
fn archive_file_name(stash_id: &str, file_name: &str) -> String {
    format!("{}_{}", stash_prefix(stash_id), file_name)
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
pub fn build_markdown(
    context_name: &str,
    metadata: &ArchiveMetadata,
    stashes: &[StashItem],
    include_attachments: bool,
    exported_at: DateTime<Utc>,
) -> String {
    let mut out = String::new();

    let frontmatter =
        serde_yaml::to_string(metadata).unwrap_or_else(|_| "name: ''\n".to_string());
    out.push_str("---\n");
    out.push_str(frontmatter.trim());
    out.push_str("\n---\n\n");

    out.push_str(&format!("# {}\n\n", context_name));
    out.push_str(&format!(
        "Exported from Stashpad on {}\n\n",
        exported_at.format("%Y-%m-%d %H:%M:%S")
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
            out.push_str(&format!("### {}\n\n", format_heading_date(&stash.created_at)));

            if !stash.content.trim().is_empty() {
                out.push_str(stash.content.trim_end());
                out.push_str("\n\n");
            }

            let names = stash_file_names(stash);
            if !names.is_empty() {
                out.push_str("**Attachments:**\n");
                for name in &names {
                    if include_attachments {
                        out.push_str(&format!(
                            "- [{}]({}/{})\n",
                            name,
                            ATTACHMENTS_DIR,
                            archive_file_name(&stash.id, name)
                        ));
                    } else {
                        out.push_str(&format!("- {}\n", name));
                    }
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

/// Strip the `<8 hex chars>_` prefix the exporter adds, recovering the original name.
fn strip_archive_prefix(name: &str) -> String {
    let bytes = name.as_bytes();
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
                Some(dt) => dt.to_rfc3339(),
                None => {
                    unreadable_dates += 1;
                    Utc::now().to_rfc3339()
                }
            };

            current = Some(StashItem {
                id: Uuid::new_v4().to_string(),
                content: String::new(),
                enhanced_content: None,
                files: Vec::new(),
                attachments: Vec::new(),
                created_at,
                context_id: Some(context_id.to_string()),
                completed: section_completed,
                completed_at: if section_completed {
                    Some(Utc::now().to_rfc3339())
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
    } else if let Some(t) = rest.strip_prefix("Completed Stashes (") {
        (true, t)
    } else {
        return None;
    };

    let count = tail.strip_suffix(')')?;
    if count.is_empty() || !count.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(kind)
}

/// `- [name](attachments/prefixed)` or the plain `- name` form.
fn parse_attachment_line(line: &str) -> Option<String> {
    let rest = line.strip_prefix("- ")?;

    if let Some(open) = rest.find("](") {
        if rest.starts_with('[') && rest.ends_with(')') {
            let target = &rest[open + 2..rest.len() - 1];
            let name = target
                .strip_prefix(&format!("{}/", ATTACHMENTS_DIR))
                .unwrap_or(target);
            return Some(strip_archive_prefix(name));
        }
    }

    Some(strip_archive_prefix(rest))
}

// ---------------------------------------------------------------------------
// Duplicate detection
// ---------------------------------------------------------------------------

fn normalise(content: &str) -> String {
    content.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
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
    if candidate
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::RootDir))
    {
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
) -> Result<ExportSummary, String> {
    let wanted: HashSet<String> = stash_ids.into_iter().collect();

    let (context, stashes) = {
        let db = state.lock_db();
        let contexts = db.get_contexts().map_err(|e| e.to_string())?;
        let context = contexts.into_iter().find(|c: &Context| c.id == context_id);

        let all = db.get_stashes().map_err(|e| e.to_string())?;
        let selected: Vec<StashItem> = all
            .into_iter()
            .filter(|s| {
                let owner = s.context_id.clone().unwrap_or_else(|| "default".to_string());
                owner == context_id && wanted.contains(&s.id)
            })
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

    // Only real files count: a row whose path is empty has no bytes on this device.
    let attachment_sources: Vec<(String, String)> = if include_attachments {
        stashes
            .iter()
            .flat_map(|s| {
                let stash_id = s.id.clone();
                let legacy = s.files.iter().cloned().map(move |p| {
                    let name = Path::new(&p)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| p.clone());
                    (p, name)
                });
                let stash_id2 = stash_id.clone();
                let current = s
                    .attachments
                    .iter()
                    .filter(|a| !a.file_path.trim().is_empty())
                    .map(move |a| (a.file_path.clone(), a.file_name.clone()));

                legacy
                    .map(move |(p, n)| (stash_id.clone(), p, n))
                    .chain(current.map(move |(p, n)| (stash_id2.clone(), p, n)))
            })
            .filter(|(_, path, _)| Path::new(path).exists())
            .map(|(stash_id, path, name)| (path, archive_file_name(&stash_id, &name)))
            .collect()
    } else {
        Vec::new()
    };

    let markdown = build_markdown(
        &context_name,
        &metadata,
        &stashes,
        include_attachments && !attachment_sources.is_empty(),
        Utc::now(),
    );

    let dest = PathBuf::from(&dest_path);
    let attachment_count = attachment_sources.len() as u32;
    let stash_count = stashes.len() as u32;

    // Deflating every attachment in a context is the single heaviest thing this app
    // does, so it belongs on the blocking pool rather than an async worker. Tokio does
    // not migrate a task that blocks its worker, so exporting a large context inline
    // took a worker out of circulation for the whole compression pass.
    let write_dest = dest.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        if let Some(parent) = write_dest.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create folder: {}", e))?;
        }

        if attachment_sources.is_empty() {
            fs::write(&write_dest, markdown)
                .map_err(|e| format!("Failed to write export: {}", e))?;
            return Ok(());
        }

        let file =
            fs::File::create(&write_dest).map_err(|e| format!("Failed to write export: {}", e))?;
        let mut zip = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        zip.start_file(MARKDOWN_ENTRY, options)
            .map_err(|e| e.to_string())?;
        zip.write_all(markdown.as_bytes())
            .map_err(|e| e.to_string())?;

        for (source, name) in &attachment_sources {
            let bytes = match fs::read(source) {
                Ok(b) => b,
                Err(e) => {
                    log::warn!("[Export] skipping {}: {}", source, e);
                    continue;
                }
            };
            zip.start_file(format!("{}/{}", ATTACHMENTS_DIR, name), options)
                .map_err(|e| e.to_string())?;
            zip.write_all(&bytes).map_err(|e| e.to_string())?;
        }

        zip.finish().map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
    .map_err(|e| format!("Export task failed: {}", e))??;

    Ok(ExportSummary {
        stashes: stash_count,
        attachments: attachment_count,
        path: dest_path,
    })
}

/// Read an archive and report what importing it would bring in.
#[tauri::command]
pub async fn read_import_archive(
    state: State<'_, Arc<DbState>>,
    path: String,
    context_id: String,
) -> Result<ImportPreview, String> {
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
    let markdown = tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        fs::create_dir_all(&extract_dir)
            .map_err(|e| format!("Failed to prepare import: {}", e))?;
        if is_zip {
            extract_archive(&source, &extract_dir)
        } else {
            fs::read_to_string(&source).map_err(|e| format!("Failed to read file: {}", e))
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
            .filter(|s| {
                s.context_id.clone().unwrap_or_else(|| "default".to_string()) == context_id
            })
            .collect::<Vec<_>>()
    };

    let duplicate_ids = find_duplicates(&parsed.stashes, &existing);

    Ok(ImportPreview {
        stashes: parsed.stashes,
        metadata: parsed.metadata,
        duplicate_ids,
        token,
        unreadable_dates: parsed.unreadable_dates,
    })
}

/// Unpack a zip into `dest`, returning the markdown document it carried.
fn extract_archive(source: &Path, dest: &Path) -> Result<String, String> {
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
            log::warn!("[Import] refusing archive entry with a traversing path: {}", name);
            continue;
        };

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = fs::File::create(&target).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
    }

    markdown.ok_or_else(|| "The archive contains no markdown document".to_string())
}

/// Write the selected stashes and their files into the context, in one transaction.
#[tauri::command]
pub async fn commit_import(
    state: State<'_, Arc<DbState>>,
    context_id: String,
    stashes: Vec<StashItem>,
    token: String,
) -> Result<u32, String> {
    let temp_dir = transfer_temp_root().join(&token);

    // All the file copying happens on the blocking pool: an import can move hundreds of
    // attachments, and every byte of that was previously copied on an async worker.
    let copy_context = context_id.clone();
    let copy_temp = temp_dir.clone();
    let prepared: Vec<StashItem> = tauri::async_runtime::spawn_blocking(
        move || -> Result<Vec<StashItem>, String> {
            let mut prepared: Vec<StashItem> = Vec::with_capacity(stashes.len());

            for mut stash in stashes {
                stash.context_id = Some(copy_context.clone());

                // Copy each referenced file out of the extraction directory and into the
                // stash's own cache folder, building the attachment rows as we go.
                let mut attachments: Vec<Attachment> = Vec::new();
                if !stash.files.is_empty() {
                    let target_dir = get_stash_cache_path(&stash.id, Some(&copy_context));
                    fs::create_dir_all(&target_dir)
                        .map_err(|e| format!("Failed to create attachment folder: {}", e))?;

                    for name in &stash.files {
                        let archived = archive_file_name(&stash.id, name);
                        let candidates = [
                            copy_temp.join(ATTACHMENTS_DIR).join(&archived),
                            copy_temp.join(ATTACHMENTS_DIR).join(name),
                        ];

                        let Some(source) = candidates.iter().find(|p| p.exists()) else {
                            log::warn!("[Import] {} is referenced but not in the archive", name);
                            continue;
                        };

                        let dest = target_dir.join(name);
                        if let Err(e) = fs::copy(source, &dest) {
                            log::warn!("[Import] could not place {}: {}", name, e);
                            continue;
                        }

                        let size = fs::metadata(&dest).map(|m| m.len()).unwrap_or(0) as i64;
                        attachments.push(Attachment {
                            id: Uuid::new_v4().to_string(),
                            stash_id: stash.id.clone(),
                            file_path: dest.to_string_lossy().into_owned(),
                            file_name: name.clone(),
                            file_size: size,
                            mime_type: mime_guess::from_path(&dest).first().map(|m| m.to_string()),
                            syntax: None,
                            created_at: Utc::now().to_rfc3339(),
                        });
                    }
                }

                // The legacy `files` column is not carried forward; attachments replace it.
                stash.files = Vec::new();
                stash.attachments = attachments;
                prepared.push(stash);
            }

            Ok(prepared)
        },
    )
    .await
    .map_err(|e| format!("Import task failed: {}", e))??;

    // One transaction for the lot, rather than the two-commands-per-stash-plus-one-per-file
    // the webview used to issue. insert_local_stashes, not import_stashes: these records
    // are new on this device and have to reach the cloud, so they stay pending.
    state
        .lock_db()
        .insert_local_stashes(&prepared)
        .map_err(|e| e.to_string())?;

    let cleanup_dir = temp_dir.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || fs::remove_dir_all(&cleanup_dir)).await;

    Ok(prepared.len() as u32)
}

/// Drop the files an aborted import had extracted.
#[tauri::command]
pub async fn discard_import(token: String) -> Result<(), String> {
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
            context_id: Some("ctx".to_string()),
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

        let md = build_markdown("Work", &metadata(), &stashes, false, Utc::now());
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
        assert_eq!(strip_archive_prefix("zzzzzzzz_shot.png"), "zzzzzzzz_shot.png");
    }

    #[test]
    fn attachment_names_round_trip_without_their_archive_prefix() {
        let mut item = stash("abcdef12", "has a file", "2026-08-18T10:00:00Z", false);
        item.attachments.push(Attachment {
            id: "att".into(),
            stash_id: "abcdef12".into(),
            file_path: "/cache/ctx/abcdef12/shot.png".into(),
            file_name: "shot.png".into(),
            file_size: 1,
            mime_type: None,
            syntax: None,
            created_at: "2026-08-18T10:00:00Z".into(),
        });

        let md = build_markdown("Work", &metadata(), &[item], true, Utc::now());
        assert!(md.contains("attachments/abcdef12_shot.png"));

        let parsed = parse_markdown(&md, "ctx");
        assert_eq!(parsed.stashes[0].files, vec!["shot.png".to_string()]);
    }

    #[test]
    fn dates_written_by_older_builds_are_still_read() {
        // These are what JavaScript's toLocaleString() produced, which is what every
        // archive exported before this change contains.
        let en_us = parse_heading_date("8/20/2026, 10:14:32 AM").expect("en-US must parse");
        assert_eq!(en_us.format("%Y-%m-%d %H:%M:%S").to_string(), "2026-08-20 10:14:32");

        let de_de = parse_heading_date("20.8.2026, 10:14:32").expect("de-DE must parse");
        assert_eq!(de_de.format("%Y-%m-%d %H:%M:%S").to_string(), "2026-08-20 10:14:32");

        let current = parse_heading_date("2026-08-20 10:14:32").expect("current format must parse");
        assert_eq!(current.format("%Y-%m-%d %H:%M:%S").to_string(), "2026-08-20 10:14:32");
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
        let existing = vec![stash("e1", "buy milk and eggs today", "2026-08-18T10:00:00Z", false)];
        let incoming = vec![
            stash("i1", "buy milk and eggs today", "2026-08-18T10:00:00Z", false),
            stash("i2", "completely unrelated content here", "2026-08-18T10:00:00Z", false),
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
