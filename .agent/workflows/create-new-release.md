---
description: Steps to create and push a new release of Stashpad
---

This workflow guides you through the process of creating a new versioned release. This will trigger the GitHub Actions workflow to build and create a release on GitHub.

1. **Write the changelog section.** In `CHANGELOG.md`, replace the `## [Unreleased]` line with
   an empty `## [Unreleased]`, a blank line, and `## [X.Y.Z] - YYYY-MM-DD` (today, from
   `date +%Y-%m-%d`). The entries that accumulated under Unreleased become this version's
   section, and a fresh empty Unreleased is left for the next change.

   This text is the release body **and** the notes the in-app updater shows in the header
   popover and in Settings > Updates, so write it for a user rather than a reviewer: each
   bullet leads with a bolded sentence stating the user-visible outcome, then unbolded prose
   on what was wrong and why it mattered. No issue, PR or commit references.

2. **Update the version** in `package.json`.
   - Open `package.json` and increment the `"version"` field (e.g., from `1.1.3` to `1.1.4`).

3. **Sync the version** to Tauri and Cargo configuration:
// turbo
```bash
npm run sync-version
```

4. **Check the changelog is there.** The same check gates the release workflow, and a failure
   there means deleting a pushed tag rather than fixing a typo.
// turbo
```bash
npm run check:changelog
```

5. **Commit the changes**:
// turbo
```bash
git add CHANGELOG.md package.json src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "chore: bump version to v$(node -p "require('./package.json').version")"
```

6. **Tag the release**:
// turbo
```bash
VERSION="v$(node -p "require('./package.json').version")"
git tag $VERSION
```

7. **Push everything**:
// turbo
```bash
git push origin main --tags
```

8. **Wait for the build**, then confirm every platform succeeded:
// turbo
```bash
gh run watch "$(gh run list --workflow=Release --limit 1 --json databaseId --jq '.[0].databaseId')" --exit-status
```

9. **Publish the release.** The build creates it as a **draft**, so until this step
   nobody can download it: a draft is invisible on the releases page and to the in-app
   updater, which resolves `releases/latest`.
// turbo
```bash
VERSION="v$(node -p "require('./package.json').version")"
gh release edit $VERSION --draft=false --latest
gh release view $VERSION --json isDraft,url --jq '"draft=\(.isDraft)", .url'
```

> [!NOTE]
> **If the `Release notes exist` gate fails**, the build matrix never starts, so nothing has
> been published and there is nothing to clean up but the tag:
> ```bash
> git tag -d $VERSION && git push --delete origin $VERSION
> # fix CHANGELOG.md, `git commit --amend`, then re-tag and re-push
> ```

> [!IMPORTANT]
> Ensure you are on the `main` branch and have no uncommitted changes before starting this process.

> [!WARNING]
> **Do not skip step 9.** Every release from v1.2.1 to v1.2.8 was left as a draft, so
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

## Regenerating an old release

The release body is composed by `scripts/release-notes.sh`, which reads `CHANGELOG.md` and adds
the boilerplate. Editing an entry in `CHANGELOG.md` and re-running it over a published tag is
therefore enough to correct a release after the fact - the body is rewritten and nothing else
about the release is touched:

```bash
bash scripts/release-notes.sh v1.5.1 | gh release edit v1.5.1 --notes-file -
```

To do that for every release at once - generate, review, then apply:

```bash
OUT="$(mktemp -d)"
unset MAC_SIGNING_IDENTITY   # the condition under which the published bodies were written
TAGS=$(gh release list --limit 50 --json tagName --jq '.[].tagName')

for T in $TAGS; do bash scripts/release-notes.sh "$T" > "$OUT/$T.md" && echo "ok   $T" || echo "MISS $T"; done
for T in $TAGS; do echo "===== $T"; diff <(gh release view "$T" --json body --jq '.body' | tr -d '\r') "$OUT/$T.md"; done
for T in $TAGS; do gh release edit "$T" --notes-file "$OUT/$T.md"; done
```

`MISS` names a version with no `CHANGELOG.md` section. The `tr -d '\r'` is not optional: the
API returns bodies with CRLF, so without it every line reports as different. Note that this
rewrites the release page only - `latest.json` carries a copy of the notes that was baked in at
build time, and the in-app updater reads that, so an old release's update notes stay as they are.
