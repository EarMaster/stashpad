# Version Management

This project uses `package.json` as the single source of truth for version numbers.

## How it works

1. **Source of Truth**: The version is defined in `package.json`
2. **Sync**: `scripts/sync-version.mjs` writes it into `src-tauri/tauri.conf.json` **and**
   `src-tauri/Cargo.toml`
3. **Display**: The UI displays the version by importing it from `src/lib/utils/version.ts`, which reads directly from `package.json`

## Updating the version

To update the version, edit the `version` field in `package.json`:

```json
{
  "version": "1.0.9"
}
```

Then sync it. The sync runs on:
- `npm run dev` (via the `predev` hook)
- `npm run build` (via the `prebuild` hook)
- `npm run sync-version` (manually)

## The sync does not run in CI

This is the important caveat, and it used to say the opposite.

`tauri-action` invokes `npm run tauri -- build`, whose only npm hook would be `pretauri`.
`tauri.conf.json`'s `beforeBuildCommand` is `npm run vite:build`, whose hook would be
`previte:build`. Neither hook exists, so **`predev` and `prebuild` are never reached on a
runner** and CI builds whatever `tauri.conf.json` and `Cargo.toml` have committed.

A release commit that changed `package.json` without syncing therefore builds the *previous*
version under the new version's release: every asset filename wrong, `latest.json` announcing
the old version, and every installed app concluding it is already up to date. Nothing about
the build looks unusual.

`release.yml`'s `prepare` job now fails when the three files disagree, so this cannot ship —
but the fix is to run `npm run sync-version` and commit all of them together.

## `Cargo.lock` is not synced

`src-tauri/Cargo.lock` also carries the crate's own version:

```bash
grep -A1 'name = "stashpad"' src-tauri/Cargo.lock
```

`sync-version.mjs` does **not** write it, yet it is part of every version-bump commit — it
normally gets refreshed as a side effect of the next `cargo` command. Edit that one line
directly rather than running a cold `cargo check` to rewrite two characters.

## Files involved

- `package.json` — single source of truth for the version
- `scripts/sync-version.mjs` — syncs the version into the two Rust-side files
- `src/lib/utils/version.ts` — exports the version for use in the UI
- `src-tauri/tauri.conf.json` — synced; **must be committed**
- `src-tauri/Cargo.toml` — synced; **must be committed**
- `src-tauri/Cargo.lock` — **not** synced, but part of the bump commit
- `src/lib/components/Settings.svelte` — displays the version in the UI

## Benefits

- ✅ Single source of truth (package.json)
- ✅ One command (`npm run sync-version`) updates every derived file except `Cargo.lock`
- ✅ Version is displayed dynamically in the UI
- ✅ No hardcoded version strings in i18n files
- ✅ `release.yml` refuses to release when the files disagree
