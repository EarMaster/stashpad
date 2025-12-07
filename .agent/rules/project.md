---
trigger: always_on
---

1. **Tech Stack:**
   - Frontend: Svelte 5 (Runes syntax `$state`, `$derived`, `$effect`). DO NOT use old Svelte 3/4 syntax.
   - Backend: Tauri v2 (Rust).
   - Styles: Tailwind CSS + shadcn-svelte.

2. **Architecture:**
   - Use the "Adapter Pattern" for all data access.
   - Frontend components must NEVER call `invoke()` directly.
   - Use `IStorageService` interface.

3. **Behavior:**
   - Always check `ARCHITECTURE.md` for feature logic.
   - Keep components small and functional.