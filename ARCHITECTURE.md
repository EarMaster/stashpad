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
* **Internationalization (i18n):** All user-facing strings **MUST** come from the i18n system (`$_("key")`).
    * 🚫 **DO NOT** hardcode any user-visible text directly in components.
    * Add new keys to `src/lib/i18n/locales/en.json` and `de.json`.

### Storage Strategy
* **Metadata:** Local JSON file (`~/.stashpad/db.json`).
* **Assets:** Local file system with hierarchical organization:
  * `~/.stashpad/cache/<context-id>/<stash-id>/<filename>` - Assets are stored in context/stash subdirectories
  * This structure prevents file name collisions and enables proper cleanup when stashes or contexts are deleted
  * Files attached to a stash are stored under that stash's directory for isolation
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

### B. The Editor (`Editor.svelte`)

  * **Component:** `Editor.svelte`.
  * **Behavior:** Auto-expanding textarea supporting Markdown.
  * **Features:**
      * **Drop Zone:** Full drag-and-drop support with visual overlay.
      * **Validation:** Save disabled if empty or no files attached.
      * **Shortcuts:** `Cmd/Ctrl+Enter` to save, `Escape` to cancel.
      * **Instant Actions:** "Add File" button always visible.

### B.1 Drag-and-Drop Architecture (Tauri)

> **IMPORTANT:** With `dragDropEnabled: true` in `tauri.conf.json`, Tauri intercepts native drag events at the OS level. HTML5 `ondrop`, `ondragenter`, etc. events **do not fire** in the webview.

#### Implementation Pattern
1. **Tauri Events:** Listen for `tauri://drag-enter`, `tauri://drag-over`, `tauri://drag-leave`, and `tauri://drag-drop` from `@tauri-apps/api/event`.
2. **Position-Based Targeting:** These events include cursor `position: { x, y }`. Use `document.elementFromPoint(x, y)` to find the target element.
3. **Shared State Store:** `src/lib/stores/drag-state.svelte.ts` coordinates state between global listeners and individual components.
4. **File Paths:** Tauri provides file paths directly (not `File` objects), so use `saveAssetFromPath` instead of `saveAsset`.

#### Component Responsibilities
* **`Editor.svelte`:** Listens for Tauri drag events directly via `onMount`. Handles drops when Editor area is targeted.
* **`Queue.svelte`:** Listens for Tauri drag events at the queue level. Uses `findStashAtPosition()` to determine which StashCard is targeted.
* **`StashCard.svelte`:** Uses `isStashHovered(item.id)` from the drag state store to display drop overlay. Does NOT listen for drag events directly.

#### Legacy Note
If `dragDropEnabled` is set to `false` in the future (e.g., for web version), HTML5 drag events can be restored.

### C. Stash Cards (`StashCard.svelte`)

  * **Component:** `StashCard.svelte`.
  * **Instant Actions** (Always Visible):
      * **Add File:** Paperclip icon to attach files quickly.
      * **Complete:** Toggle completion status (left side).
      * **Drag Handle:** Hand icon for dragging content to AI context.
  * **Additional Actions** (Hover/Focus):
      * **Copy:** Copy content to clipboard.
      * **Edit:** Inline editing mode.
      * **Move:** Move request (e.g., to another context).
      * **Delete:** Trash icon (Shift+Click to skip confirmation).
  * **Main Actions:**
      * **Complete:** Mark as done/active.
      * **Drag:** Drag content to external apps.

### D. Settings & Dialogs

  * **Settings (`Settings.svelte`):**
      * **Context Management:** Manage/Switch project contexts.
      * **General:** Language, Auto-Context Detection, New Stash Position (Top/Bottom).
      * **Appearance:** Theme (Light/Dark/System), Visual Effects (Vibrancy).
      * **Shortcuts:** Custom keybindings for toggling app and switching contexts.
  * **Dialogs (`ConfirmationDialog.svelte`):**
      * Unified modal for destructive actions (Delete, etc.).
      * Full, accessible keyboard navigation.

### E. Central Transfer Switch & Smart Dispatch (USP)

A global state (Svelte Rune) determines the transfer mode: `transferMode = 'drag' | 'copy' | 'auto'`.

#### Mode 1: ✋ Drag (GUI Mode)

  * **Use Case:** Web Browsers (Claude.ai), Desktop GUIs (VSCode Chat).
  * **Trigger:** User clicks and holds the "Drag Handle" on a stash item.
  * **Technical Implementation:**
      * **Clipboard:** Write the Markdown `content` to the system clipboard (text/plain).
      * **Drag Payload:** Attach the `assets` (absolute paths) to the native OS drag payload.
      * *Result:* User drops -> App uploads files -> User pastes -> Text appears.

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
    3.  **IF** Terminal detected -> Execute **Copy Mode**.
    4.  **ELSE** -> Execute **Drag Mode**.

-----

## 5. UI/UX Guidelines

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

### Tooltips

**IMPORTANT:** Use the custom tooltip action (`$lib/actions/tooltip`) instead of native HTML `title` attributes for all interactive elements.

#### Usage
```svelte
<script>
  import { tooltip } from "$lib/actions/tooltip";
</script>

<button use:tooltip title="Click to save">Save</button>
```

#### Features
- **Auto-positioning:** Automatically repositions to avoid viewport overflow
- **Accessible:** Shows on hover AND keyboard focus
- **Styled:** Consistent dark styling with arrow indicator
- **Non-native:** Prevents browser's default tooltip, uses custom CSS

#### When to Use
- All `ActionButton` components (already integrated)
- Any interactive element (`<button>`, `<a>`) that needs contextual help
- Icon-only buttons that need labels for accessibility

#### Styles
Defined in `src/app.css` under `.custom-tooltip`. Customizable via CSS.

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
      ConfirmationDialog.svelte
      ContextManager.svelte
      ContextSwitcher.svelte
      Editor.svelte
      FilePreviewModal.svelte
      Header.svelte
      Queue.svelte        # Was StashQueue
      Settings.svelte
      StashCard.svelte    # Contains the Drag Handle logic
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