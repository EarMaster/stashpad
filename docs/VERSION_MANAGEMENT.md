# Version Management

This project uses `package.json` as the single source of truth for version numbers.

## How it works

1. **Source of Truth**: The version is defined in `package.json`
2. **Automatic Sync**: The version is automatically synced to `src-tauri/tauri.conf.json` via the `sync-version` script
3. **Display**: The UI displays the version by importing it from `src/lib/utils/version.ts`, which reads directly from `package.json`

## Updating the version

To update the version, simply edit the `version` field in `package.json`:

```json
{
  "version": "1.0.9"
}
```

The version will be automatically synced to `tauri.conf.json` when you run:
- `npm run dev` (via the `predev` hook)
- `npm run build` (via the `prebuild` hook)
- `npm run sync-version` (manually)

## Files involved

- `package.json` - Single source of truth for the version
- `scripts/sync-version.mjs` - Script that syncs the version to Tauri config
- `src/lib/utils/version.ts` - Exports the version for use in the UI
- `src-tauri/tauri.conf.json` - Auto-synced with the version from package.json
- `src/lib/components/Settings.svelte` - Displays the version in the UI

## Benefits

- ✅ Single source of truth (package.json)
- ✅ No manual synchronization needed
- ✅ Version is automatically synced during development and build
- ✅ Version is displayed dynamically in the UI
- ✅ No hardcoded version strings in i18n files
