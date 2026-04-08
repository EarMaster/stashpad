# ![Stashpad](assets/stashpad/Icon_24.webp) Stashpad

![Status](https://img.shields.io/badge/Status-Prototype-blueviolet) ![Stack](https://img.shields.io/badge/Stack-Tauri_v2_|_Svelte_5_|_Rust-orange)

![Don't interrupt the AI. Stash it!](assets/stashpad/Key%20Visual.webp)

Stashpad is a **Local-First Developer Utility** designed for the age of AI coding. It acts as a temporary buffer ("Staging Area") for your thoughts, bugs, and context assets.

Instead of interrupting your AI agent (like Claude, ChatGPT, or GitHub Copilot) while it's generating code, you stash your new ideas or discovered bugs in Stashpad. When the AI is ready, you transfer the entire context in one go.

## ✨ Key Features

* **🟢 Auto-Context Detection:** Automatically detects which project you are working on based on your active window (VSCode, JetBrains, Terminal).
* **🧠 The Stash Queue:** Collect text notes, error logs, and screenshots in a persistent queue.
* **🚀 Smart Transfer System:**
    * **Drag Mode:** Perfect for Web UIs. Drags files natively while copying text to clipboard.
    * **Copy (CLI) Mode:** Formats text and file paths specifically for CLI tools like `claude-code`.
    * **Auto Mode:** Intelligently switches between Drag & Copy based on whether you are using a Terminal or a GUI app.
* **🔒 Local First:** Your data stays on your machine. Stored in `~/.stashpad`.

## 🛠️ Tech Stack

* **Core:** [Tauri v2](https://v2.tauri.app/) (Rust) - Lightweight & Secure.
* **Frontend:** [Svelte 5](https://svelte.dev/) - Reactive & Fast.
* **Styling:** Tailwind CSS.
* **Architecture:** Implements an Adapter Pattern to allow future Web-App compatibility.

## 🚀 Getting Started

### Prerequisites
* Node.js (v20+)
* Rust (latest stable) & Cargo

### Installation

1.  **Clone the repository**
    ```bash
    git clone https://github.com/EarMaster/stashpad.git
    cd stashpad
    ```

2.  **Install dependencies**
    ```bash
    npm install
    ```

3.  **Run in Development Mode**
    ```bash
    npm run tauri dev
    ```

## 🏗️ Project Structure

* `src-tauri/`: The Rust backend. Handles window detection, file system access, and OS integration.
* `src/lib/services/`: Contains the **Storage Adapter Interface**. This separates the UI from the backend logic.
* `src/lib/components/`: Svelte 5 UI components.

## 🧪 Testing

Stashpad includes comprehensive testing infrastructure for reliable code quality:

### Running Tests

```bash
# Run all tests
npm run test

# Run tests in watch mode (for development)
npm run test:watch

# Run tests with interactive UI
npm run test:ui

# Generate coverage report
npm run test:coverage

# Run Rust backend tests
cd src-tauri && cargo test
```

### Test Coverage

- **70+ Frontend Tests**: Unit tests for utilities and services
- **Vitest + Testing Library**: Modern testing framework with Svelte 5 support
- **Mocked Tauri APIs**: Isolated unit testing without backend dependencies

For comprehensive testing documentation, see [TESTING.md](TESTING.md).

## 📝 License

This project's source code is licensed under the **GNU Affero General Public License v3.0 (AGPL-3.0)**.

**© Copyright 2025-2026 Nico Wiedemann**

**⚠️ BRANDING & ASSET EXCEPTION:**
The following directories and their contents are **explicitly excluded** from the AGPL v3.0, as they constitute proprietary branding and design assets:

1.  **`assets/stashpad/`**: visual assets, icons, logos, keyvisuals
2.  **`public/`**: static assets, high-resolution logos, keyvisuals

These visual assets are copyrighted by Nico Wiedemann and are licensed under the **Creative Commons Attribution-NonCommercial-NoDerivatives 4.0 International (CC BY-NC-ND 4.0)**. They may not be used in competing products or services.