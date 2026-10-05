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

//! Images dragged in from a web browser.
//!
//! Tauri's native drop handler only reports file paths, and on macOS it reads them from
//! `NSFilenamesPboardType` alone. A browser never puts an image there: it offers the image
//! as a promised file, plus some mix of the original bytes, the image URL and a decoded
//! TIFF. So a browser drop arrived as an empty path list and was silently ignored.
//!
//! The promised file cannot be used. By the time the drop event reaches us the drag
//! session is over, and macOS reports the promise as gone ("Couldn't get a copy of an HFS
//! Promise") - checked against Chromium and Safari. What the drag pasteboard still holds
//! is enough, though, so this tries, in order:
//!
//! 1. the original bytes (Chromium browsers put the image file itself on the pasteboard)
//! 2. the image URL, downloaded (Safari offers no original, only the URL and a bitmap)
//! 3. the TIFF bitmap, re-encoded as PNG - offline, but never the original file
//!
//! The result is written to a scratch file and its path handed back, so the interface can
//! treat it exactly like a file dropped from Finder: same resize, same save, same errors.

use std::path::PathBuf;

use crate::uierror::UiError;

/// Scratch folder for the image of the latest browser drop. Emptied on every drop, so at
/// most one stale file lingers - and it is under the app folder the fs scope allows.
fn scratch_dir() -> PathBuf {
    crate::utils::get_app_dir().join("tmp").join("drop")
}

fn nothing_usable() -> UiError {
    UiError::new(
        "drop.nothing_usable",
        "This drop contained nothing Stashpad can attach. Try copying the image and pasting it instead.",
    )
}

/// Turn the image in the current drag into a file and return its path.
#[tauri::command]
pub async fn read_dropped_image() -> Result<String, UiError> {
    #[cfg(target_os = "macos")]
    {
        macos::read().await
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Only the macOS drop handler is known to drop browser content on the floor; on
        // the other platforms there is no pasteboard to read it back from.
        Err(nothing_usable())
    }
}

/// A filename for the image, from the last segment of its URL when there is one.
fn file_name(url: Option<&str>, ext: &str) -> String {
    let stem = url
        .and_then(|u| reqwest::Url::parse(u).ok())
        .and_then(|u| u.path_segments()?.next_back().map(str::to_string))
        .map(|s| urlencoding::decode(&s).map(|d| d.into_owned()).unwrap_or(s))
        .map(|s| match s.rsplit_once('.') {
            Some((stem, _)) if !stem.is_empty() => stem.to_string(),
            _ => s,
        })
        .map(|s| s.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_"))
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "dropped-image".to_string());
    format!("{}.{}", stem, ext)
}

/// Write the image to the scratch folder, replacing whatever the last drop left there.
fn write_scratch(name: &str, bytes: &[u8]) -> Result<String, UiError> {
    let dir = scratch_dir();
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create {}: {}", dir.display(), e))?;
    let path = dir.join(name);
    std::fs::write(&path, bytes)
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
    Ok(path.to_string_lossy().into_owned())
}

#[cfg(target_os = "macos")]
mod macos {
    use std::time::Duration;

    use objc2_app_kit::{
        NSBitmapImageFileType, NSBitmapImageRep, NSPasteboard, NSPasteboardNameDrag,
    };
    use objc2_foundation::{NSData, NSDictionary, NSString};

    use super::{file_name, nothing_usable, write_scratch};
    use crate::uierror::UiError;

    /// Original image formats, as pasteboard type and file extension, best first.
    const ORIGINAL_TYPES: [(&str, &str); 4] = [
        ("public.png", "png"),
        ("public.jpeg", "jpg"),
        ("org.webmproject.webp", "webp"),
        ("com.compuserve.gif", "gif"),
    ];

    /// A dropped image is a picture on a web page, not a download manager; anything
    /// bigger than this is refused rather than held in memory.
    const MAX_DOWNLOAD_BYTES: u64 = 50 * 1024 * 1024;

    /// What the drag pasteboard held, copied out so nothing AppKit crosses an `await`.
    struct DragContents {
        original: Option<(Vec<u8>, &'static str)>,
        url: Option<String>,
        tiff: Option<Vec<u8>>,
    }

    fn read_pasteboard() -> DragContents {
        let pb = NSPasteboard::pasteboardWithName(unsafe { NSPasteboardNameDrag });
        let data = |t: &str| pb.dataForType(&NSString::from_str(t)).map(|d| d.to_vec());

        let original = ORIGINAL_TYPES
            .iter()
            .find_map(|(t, ext)| data(t).filter(|b| !b.is_empty()).map(|b| (b, *ext)));
        let url = pb
            .stringForType(&NSString::from_str("public.url"))
            .map(|s| s.to_string())
            .filter(|u| u.starts_with("https://") || u.starts_with("http://"));
        let tiff = data("public.tiff").filter(|b| !b.is_empty());

        DragContents {
            original,
            url,
            tiff,
        }
    }

    pub async fn read() -> Result<String, UiError> {
        let contents = tauri::async_runtime::spawn_blocking(read_pasteboard)
            .await
            .map_err(|e| format!("Reading the drop failed: {}", e))?;

        if let Some((bytes, ext)) = &contents.original {
            let name = file_name(contents.url.as_deref(), ext);
            return write_scratch(&name, bytes);
        }

        let mut download_error = None;
        if let Some(url) = &contents.url {
            match download(url).await {
                Ok((bytes, ext)) => return write_scratch(&file_name(Some(url), &ext), &bytes),
                Err(reason) => {
                    log::warn!("Could not download dropped image {}: {}", url, reason);
                    download_error = Some(reason);
                }
            }
        }

        if let Some(tiff) = contents.tiff {
            let url = contents.url.clone();
            let png = tauri::async_runtime::spawn_blocking(move || tiff_to_png(&tiff))
                .await
                .map_err(|e| format!("Converting the drop failed: {}", e))?;
            if let Some(png) = png {
                return write_scratch(&file_name(url.as_deref(), "png"), &png);
            }
        }

        Err(match download_error {
            Some(reason) => UiError::with_values(
                "drop.download_failed",
                format!("Couldn't download the dropped image: {}", reason),
                [("reason", reason)],
            ),
            None => nothing_usable(),
        })
    }

    /// Fetch the image behind a dropped URL, with its file extension.
    async fn download(url: &str) -> Result<(Vec<u8>, String), String> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| e.to_string())?;
        let response = client.get(url).send().await.map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("HTTP {}", response.status()));
        }

        let mime = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            .map(|v| v.trim().to_ascii_lowercase())
            .unwrap_or_default();
        if !mime.starts_with("image/") {
            let shown = if mime.is_empty() {
                "no content type"
            } else {
                mime.as_str()
            };
            return Err(format!("the address returned {}, not an image", shown));
        }
        let ext = match mime.as_str() {
            // mime_guess lists "jpe" first for image/jpeg
            "image/jpeg" => "jpg".to_string(),
            _ => mime_guess::get_mime_extensions_str(&mime)
                .and_then(|exts| exts.first())
                .map(|e| e.to_string())
                .unwrap_or_else(|| mime.trim_start_matches("image/").to_string()),
        };

        if response
            .content_length()
            .is_some_and(|len| len > MAX_DOWNLOAD_BYTES)
        {
            return Err("the image is larger than 50 MB".into());
        }
        let bytes = response.bytes().await.map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_DOWNLOAD_BYTES {
            return Err("the image is larger than 50 MB".into());
        }
        Ok((bytes.to_vec(), ext))
    }

    fn tiff_to_png(tiff: &[u8]) -> Option<Vec<u8>> {
        let rep = NSBitmapImageRep::imageRepWithData(&NSData::with_bytes(tiff))?;
        let png = unsafe {
            rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
        }?;
        Some(png.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::file_name;

    #[test]
    fn names_the_file_after_the_url() {
        assert_eq!(
            file_name(
                Some("https://example.com/uploads/Kaesekuchen-02.jpg"),
                "jpg"
            ),
            "Kaesekuchen-02.jpg"
        );
    }

    #[test]
    fn swaps_the_extension_for_the_actual_format() {
        assert_eq!(
            file_name(
                Some("https://images.example.com/16x9-big/kaese406.webp?width=1920"),
                "png"
            ),
            "kaese406.png"
        );
    }

    #[test]
    fn decodes_and_sanitises_the_last_segment() {
        assert_eq!(
            file_name(Some("https://example.com/a/K%C3%A4se%3Akuchen.png"), "png"),
            "Käse_kuchen.png"
        );
    }

    #[test]
    fn falls_back_when_there_is_no_usable_name() {
        assert_eq!(file_name(None, "png"), "dropped-image.png");
        assert_eq!(
            file_name(Some("https://example.com/"), "gif"),
            "dropped-image.gif"
        );
    }
}
