# STASHPAD - PROJECT SPECIFICATION & ARCHITECTURE

> **⚠️ AI AGENT INSTRUCTION:**
> This file is the **SOURCE OF TRUTH** for this project.
> Before implementing any feature or changing any file, ALWAYS refer back to this document.
> Do NOT deviate from the Tech Stack (Tauri v2, Svelte 5, Rust) or the "Shared Core" Adapter Pattern defined below.

---

## 1. Project Overview
**Stashpad** is a "Local-First" staging area for developers working with AI. It acts as a buffer to collect thoughts, bugs, logs, and screenshots without interrupting the AI's current task. Ideally, it sits as a floating window "always on top".

**Slogan:** *Don't interrupt the AI. Stash it.*

---

## 2. Tech Stack & Constraints

### Core Frameworks
* **Runtime:** [Tauri v2](https://v2.tauri.app/) (Rust Backend + WebView).
* **Frontend:** Svelte 5 + Vite.
* **Language:** Rust (Backend), TypeScript (Frontend).

### Frontend Specifics (Strict)
* **Reactivity:** Use **Svelte 5 Runes** (`$state`, `$derived`, `$effect`, `$props`).
    * 🚫 **DO NOT** use legacy Svelte 3/4 stores (`writable`, `readable`) unless interacting with a library that strictly requires them.
    * 🚫 **DO NOT** use `export let` for props; use `let { prop } = $props()`.
* **Styling:** Tailwind CSS.
* **UI Library:** `shadcn-svelte` (or accessible primitives using Tailwind).
* **Icons:** Lucide Svelte.

### Storage Strategy
* **Metadata:** Local JSON file (`~/.stashpad/db.json`).
* **Assets:** Local file system (`~/.stashpad/cache/<project-id>/`).
* **Database:** Start with simple JSON serialization. Prepare interfaces for SQLite migration if needed later.

---

## 3. Architecture: The "Shared Core" & Adapter Pattern

**CRITICAL REQUIREMENT:** To enable a future Web App version (SaaS) without rewriting the frontend, the UI components must be decoupled from the Tauri backend.

### The Service Layer Rule
The frontend UI components (Svelte files) must **NEVER** call `invoke()` directly. They must use the `storageService` singleton.

### 3.1 Interface Definition
`src/lib/services/storage.interface.ts`

```typescript
export interface AppInfo {
  executable: string; // e.g., "code", "iterm2", "chrome"
  title: string;      // e.g., "main.rs - ProjectX"
}

export interface StashItem {
  id: string;
  projectId: string; // Derived from window title
  content: string;   // Markdown text
  assets: string[];  // List of absolute file paths
  createdAt: number;
}

export interface IStorageService {
  // Stash Management
  saveStash(stash: StashItem): Promise<void>;
  loadStashes(): Promise<StashItem[]>;
  deleteStash(id: string): Promise<void>;

  // Asset Management
  // Takes a JS File object (from drop), saves it, returns absolute path/url
  saveAsset(file: File, projectId: string): Promise<string>;

  // Context Detection
  // Returns info about the app that was focused BEFORE Stashpad
  getPreviousAppInfo(): Promise<AppInfo>;
}
````

### 3.2 Implementation Strategy

1.  **`src/lib/services/adapters/desktop.adapter.ts`**: Implements `IStorageService` using Tauri `invoke` commands and `tauri-plugin-fs`.
2.  **`src/lib/services/index.ts`**: Detects environment and exports the correct adapter.
    ```typescript
    // Pseudo-code
    import { DesktopAdapter } from './adapters/desktop.adapter';
    // import { WebAdapter } from './adapters/web.adapter'; // Future

    export const storageService = window.__TAURI_INTERNALS__ 
      ? new DesktopAdapter() 
      : new DesktopAdapter(); // Fallback for now
    ```

-----

## 4\. Core Features & Logic

### A. Context-Aware Capture (Rust Backend)

  * **Requirement:** Use a crate like `active-win-pos-rs` to poll the active window.
  * **Polling Loop:** The backend should poll every \~1s (or on window focus change) to track the *previously* active application.
  * **Logic:**
      * Store `last_active_app_executable` (e.g., `iterm2`).
      * Store `last_active_window_title` (e.g., `server.py - my-api`).
      * Expose this via a Tauri Command `get_previous_app_info`.

### B. The Stash Editor

  * **Component:** `StashEditor.svelte`.
  * **Behavior:** Auto-expanding textarea supporting Markdown.
  * **Drop Zone:** The entire editor area is a drop zone.
      * `on:drop`: Prevent default -\> Extract files -\> Call `storageService.saveAsset()`.
      * UI: Display thumbnails of saved assets below the text.

### C. Central Transfer Switch & Smart Dispatch (USP)

A global state (Svelte Rune) determines the transfer mode: `transferMode = 'drag' | 'copy' | 'auto'`.

#### Mode 1: ✋ Drag (GUI Mode)

  * **Use Case:** Web Browsers (Claude.ai), Desktop GUIs (VSCode Chat).
  * **Trigger:** User clicks and holds the "Drag Handle" on a stash item.
  * **Technical Implementation:**
      * **Clipboard:** Write the Markdown `content` to the system clipboard (text/plain).
      * **Drag Payload:** Attach the `assets` (absolute paths) to the native OS drag payload.
      * *Result:* User drops -\> App uploads files -\> User pastes -\> Text appears.

#### Mode 2: 📋 Copy (CLI Mode)

  * **Use Case:** Terminals (Claude Code, Aider).
  * **Trigger:** User *clicks* the handle (or attempts to drag in Copy mode).
  * **Technical Implementation:**
      * Do NOT start a drag event.
      * Construct a specific text block:
        ```text
        [User Markdown Content]

        ---
        # SYSTEM CONTEXT - LOCAL FILES
        /Users/dev/.stashpad/cache/img1.png
        /Users/dev/.stashpad/cache/log.txt
        ```
      * Write this block to system clipboard.
      * Show toast notification: "Copied for Terminal".

#### Mode 3: 🤖 Auto (Smart Mode)

  * **Trigger:** User initiates interaction with the handle.
  * **Logic:**
    1.  Call `storageService.getPreviousAppInfo()`.
    2.  Check `executable` against a hardcoded list of known terminals:
          * *List:* `iterm2`, `terminal`, `warp`, `hyper`, `alacritty`, `kitty`, `powershell`, `cmd`, `wt` (Windows Terminal), `code` (VSCode is tricky, treat as GUI usually, or check strict context if possible).
    3.  **IF** Terminal detected -\> Execute **Copy Mode**.
    4.  **ELSE** -\> Execute **Drag Mode**.

-----

## 5\. UI/UX Guidelines

  * **Theme:** Dark Mode Only.
  * **Palette:**
      * Background: Deep Charcoal (`#18181b`).
      * Surface: Lighter Charcoal (`#27272a`).
      * Card/Secondary: Midnight Graphite (`#2c373d`).
      * Text: Terminal White (`#d8d8d9`).
      * Brand/Accent: Electric Violet (`#8b5cf6`).
      * Status/Highlight: Amber (`#f59e0b`).
  * **Typography:** Monospace for paths/code, Sans-serif for UI.
  * **Header UI:**
      * Left: Green dot + Active Project Name.
      * Right: Segmented Control `[✋ | 📋 | 🤖]`.

-----

## 6\. Directory Structure (Guideline)

```
src-tauri/
  src/
    main.rs               # Window polling & Setup
    commands.rs           # Tauri Commands (save_file, get_context)
    lib.rs
  Cargo.toml

src/
  lib/
    components/
      ui/                 # shadcn-svelte components
      editor/
        StashEditor.svelte
        AttachmentList.svelte
      queue/
        StashQueue.svelte
        StashItem.svelte  # Contains the Drag Handle logic
      layout/
        Header.svelte     # Contains Transfer Switch
    services/
      storage.interface.ts
      adapters/
        desktop.adapter.ts
      index.ts            # Service Locator
    stores/
      app.svelte.ts       # Global State (Runes): transferMode, currentProject
  App.svelte
  main.ts
```