// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Nico Wiedemann

/** Broad category an attachment falls into, derived from its file extension. */
export type AttachmentKind = "image" | "video" | "text" | "other";

const IMAGE_EXTENSIONS = [
    "png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico",
];

const VIDEO_EXTENSIONS = [
    "mp4", "webm", "ogg", "ogv", "mov", "avi", "mkv", "m4v",
];

const TEXT_EXTENSIONS = [
    "txt", "md", "markdown", "json", "xml", "html", "htm", "css", "js", "mjs",
    "ts", "tsx", "jsx", "py", "rs", "go", "java", "c", "h", "cpp", "hpp", "cc",
    "cs", "rb", "php", "sh", "bash", "zsh", "ps1", "yaml", "yml", "toml", "ini",
    "cfg", "conf", "log", "sql", "svelte", "vue",
];

/**
 * Classify an attachment by its file extension.
 *
 * Single source of truth for the icon shown next to an attachment and for the
 * queue's attachment filter, so a filter chip can never disagree with the icon.
 *
 * @param fileNameOrPath A file name or a full path - only the extension is read
 */
export function getAttachmentKind(fileNameOrPath: string): AttachmentKind {
    const ext = fileNameOrPath.toLowerCase().split(".").pop() || "";
    if (IMAGE_EXTENSIONS.includes(ext)) return "image";
    if (VIDEO_EXTENSIONS.includes(ext)) return "video";
    if (TEXT_EXTENSIONS.includes(ext)) return "text";
    return "other";
}

/**
 * Resizes an image file if it exceeds the maximum dimensions.
 * Maintains aspect ratio.
 * 
 * @param file The image file or blob to resize
 * @param maxWidth The maximum width or height of the image (default 2048)
 * @returns A Promise that resolves to the resized Blob, or the original file if resizing wasn't necessary/possible
 */
export async function resizeImage(file: File | Blob, maxWidth: number = 2048): Promise<Blob> {
    // If it's not an image, return original
    if (!file.type.startsWith('image/')) {
        return file;
    }

    return new Promise((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = (e) => {
            const img = new Image();
            img.onload = () => {
                let width = img.width;
                let height = img.height;

                // Check if resizing is needed
                if (width <= maxWidth && height <= maxWidth) {
                    resolve(file);
                    return;
                }

                // Calculate new dimensions
                if (width > height) {
                    if (width > maxWidth) {
                        height = Math.round(height * (maxWidth / width));
                        width = maxWidth;
                    }
                } else {
                    if (height > maxWidth) {
                        width = Math.round(width * (maxWidth / height));
                        height = maxWidth;
                    }
                }

                // Resize using Canvas
                const canvas = document.createElement('canvas');
                canvas.width = width;
                canvas.height = height;
                const ctx = canvas.getContext('2d');
                if (!ctx) {
                    reject(new Error('Failed to get canvas context'));
                    return;
                }

                ctx.drawImage(img, 0, 0, width, height);

                // Convert back to Blob
                // Attempt to preserve original format, fallback to jpeg/png if not supported
                // Start with original type
                let outputType = file.type;

                // Canvas toBlob supports image/png, image/jpeg, image/webp
                // If original is gif or svg, we might want to keep it or convert to png/jpeg?
                // Git/SVG likely shouldn't be resized this way usually?
                // For now, let's try to stick to input type if supported, else default to jpeg for photos/mixed

                canvas.toBlob((blob) => {
                    if (blob) {
                        resolve(blob);
                    } else {
                        reject(new Error('Failed to create blob from canvas'));
                    }
                }, outputType, 0.9); // 0.9 quality for lossy formats
            };
            img.onerror = () => reject(new Error('Failed to load image'));
            img.src = e.target?.result as string;
        };
        reader.onerror = () => reject(new Error('Failed to read file'));
        reader.readAsDataURL(file);
    });
}
