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

6. **Wait for the build**, then confirm every platform succeeded:
// turbo
```bash
gh run watch "$(gh run list --workflow=Release --limit 1 --json databaseId --jq '.[0].databaseId')" --exit-status
```

7. **Publish the release.** The build creates it as a **draft**, so until this step
   nobody can download it: a draft is invisible on the releases page and to the in-app
   updater, which resolves `releases/latest`.
// turbo
```bash
VERSION="v$(node -p "require('./package.json').version")"
gh release edit $VERSION --draft=false --latest
gh release view $VERSION --json isDraft,url --jq '"draft=\(.isDraft)", .url'
```

> [!IMPORTANT]
> Ensure you are on the `main` branch and have no uncommitted changes before starting this process.

> [!WARNING]
> **Do not skip step 7.** Every release from v1.2.1 to v1.2.8 was left as a draft, so
> for months the newest version users could actually get was v1.2.0. Nothing warns you
> about this — the build goes green and the assets exist, they are just not published.

> [!NOTE]
> The draft behaviour comes from `releaseDraft: true` in `.github/workflows/release.yml`,
> not from GitHub itself. Setting it to `false` would publish automatically, at the cost
> of losing the chance to check the assets first.

> [!NOTE]
> **Verifying update signatures.** A correct release includes `latest.json` plus a `.sig`
> next to each updater bundle. If they are missing, the `TAURI_SIGNING_PRIVATE_KEY` /
> `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` secrets are not set and installed apps will refuse
> the update:
> ```bash
> gh release view $VERSION --json assets --jq '.assets[].name' | grep -E 'latest\.json|\.sig$'
> ```