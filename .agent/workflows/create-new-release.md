---
description: Steps to create and push a new release of Stashpad
---

This workflow guides you through the process of creating a new versioned release. This will trigger the GitHub Actions workflow to build and create a release on GitHub.

1. **Update the version** in `package.json`.
   - Open `package.json` and increment the `"version"` field (e.g., from `1.1.3` to `1.1.4`).

2. **Sync the version** to Tauri and Cargo configuration:
// turbo
```bash
npm run sync-version
```

3. **Commit the changes**:
// turbo
```bash
git add package.json src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "chore: bump version to v$(node -p "require('./package.json').version")"
```

4. **Tag the release**:
// turbo
```bash
VERSION="v$(node -p "require('./package.json').version")"
git tag $VERSION
```

5. **Push everything**:
// turbo
```bash
git push origin main --tags
```

> [!IMPORTANT]
> Ensure you are on the `main` branch and have no uncommitted changes before starting this process.