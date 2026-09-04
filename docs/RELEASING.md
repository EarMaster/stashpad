# Releasing Stashpad

Reference material for cutting and repairing a release. The step-by-step preparation lives in
the `/release` slash command at the workspace root; the state checks live in
`/release-status`. This file is what you read when something has already gone wrong, or when
you need to change how releases work.

## Who does what

**An agent prepares the release commit and stops.** It stamps `CHANGELOG.md`, bumps
`package.json`, runs `npm run sync-version`, verifies the changelog gate, and commits. It does
not tag, push a tag, or publish — those are denied at the permission layer in
`.claude/settings.json`.

**Your merge of `develop` into `main` is the release decision.** It starts `release.yml`,
which:

1. `prepare` — asserts `package.json`, `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml`
   agree; refuses if the tag or a release for it already exists; gates on the changelog;
   composes the release notes once.
2. `test` — the full suite from `test.yml`, as a reusable workflow.
3. `create-release` — creates a **draft**. No git tag exists at this point.
4. `build` — four platforms, uploading into that draft by its numeric id.
5. `publish` — verifies the assets, then publishes. **Publishing is what creates the tag.**

A build that fails leaves no tag. `cleanup` deletes the draft and the version stays
re-releasable. This is the whole reason the workflow is shaped this way: v1.6.9 was tagged
before it was built, two of four platforms then failed, and recovery needed a remote tag
deleted by hand.

## Verifying update signatures

A correct release carries `latest.json` plus a `.sig` beside every updater bundle. Without
both, installed apps **refuse** the update.

```bash
gh release view "$VERSION" --json assets --jq '.assets[].name' | grep -E 'latest\.json|\.sig$'
```

If they are missing, `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` are
unset in the repo secrets. Note the failure mode: `release.yml`'s signing step exports those
into `$GITHUB_ENV` **only when they exist**, because a nonexistent secret expands to an empty
string and an empty-but-set key fails the bundler outright. So an unsigned build *succeeds*
with a `::warning::` rather than failing. The `publish` job now checks for the signatures
before publishing, which is what turns that warning into a stop.

The updater endpoint is `plugins.updater.endpoints` in `src-tauri/tauri.conf.json`:

```
https://github.com/EarMaster/stashpad/releases/latest/download/latest.json
```

`releases/latest` is the entire delivery mechanism, which is why `publish` passes `make_latest`
explicitly and then fetches that URL to confirm it serves the new version.

## When the changelog gate fails

The gate runs before anything is created, so nothing was published and there is nothing to
clean up — no tag, no draft. Fix `CHANGELOG.md` on `develop` and merge again.

Under the **old** tag-triggered workflow the recovery was different and worth knowing if you
find an old tag lying around:

```bash
git tag -d "$VERSION" && git push --delete origin "$VERSION"
```

## Regenerating an old release body

`scripts/release-notes.sh` reads `CHANGELOG.md` and adds the boilerplate. Editing an entry and
re-running it over a published tag rewrites the body and touches nothing else:

```bash
bash scripts/release-notes.sh v1.5.1 | gh release edit v1.5.1 --notes-file -
```

To do it for every release at once — generate, review, then apply:

```bash
OUT="$(mktemp -d)"
unset MAC_SIGNING_IDENTITY   # the condition under which the published bodies were written
TAGS=$(gh release list --limit 50 --json tagName --jq '.[].tagName')

for T in $TAGS; do bash scripts/release-notes.sh "$T" > "$OUT/$T.md" && echo "ok   $T" || echo "MISS $T"; done
for T in $TAGS; do echo "===== $T"; diff <(gh release view "$T" --json body --jq '.body' | tr -d '\r') "$OUT/$T.md"; done
for T in $TAGS; do gh release edit "$T" --notes-file "$OUT/$T.md"; done
```

Both caveats are load-bearing. `unset MAC_SIGNING_IDENTITY` reproduces the condition the
published bodies were written under — the script emits the macOS quarantine block only when
that variable is unset, matching `release.yml`. And `tr -d '\r'` is **not optional**: the API
returns bodies with CRLF, so without it every line reports as different. `MISS` names a
version with no `CHANGELOG.md` section.

## The one thing regeneration cannot fix

`latest.json` carries a **copy** of the notes, baked in at build time, and the in-app updater
reads that copy. So rewriting `CHANGELOG.md` corrects the releases page and leaves an old
release's update notes exactly as they shipped.

That is the strongest argument for getting the changelog right *before* the release, and it is
why `/release` reads the stamped section back in popover order before committing.

## `release-notes.sh` is fragile in one specific way

The extracting `awk` exits at the next `## [` heading, so anything piping *into* it takes
`SIGPIPE` once the changelog outgrows the pipe buffer — 64 KB on Linux, 16 KB on macOS.
v1.6.9 built on Windows and Linux and failed on both macOS targets, with the script reporting
a missing section for a section that was plainly there. Fixed in `87a21c6` by having `awk`
read the file itself. **Do not reintroduce an upstream `tr`.**

It also needs the previous tag passed as `$2` under the current workflow. Its default is
`git describe --tags --abbrev=0 "${TAG}^"`, which cannot resolve a tag that does not exist
yet; `|| true` swallows the failure and the `**Full changelog:**` compare link silently
disappears from the body and from every update popover.

## Changing how releases work

- `releaseDraft` in the `tauri-action` step is **inert** while `releaseId` is set — both are
  only read inside the action's `getOrCreateRelease`, which an id bypasses. The draft state is
  set by `create-release`. Editing that input will not change anything.
- `tagName` is passed alongside `releaseId` and is **not** redundant. GitHub reports a draft's
  asset URLs as `…/releases/download/untagged-<hash>/<asset>`; the action rewrites that segment
  to the tag. Drop it and the URLs fall back to `/releases/latest/download/`, which resolves to
  whichever release is newest *at update time* rather than the one those signatures were made
  for — so an older install downloads a newer file and rejects its own manifest's signature.
- `releaseBody` is what becomes `latest.json`'s `notes`. The release page's body is written by
  `create-release` and is not touched from the matrix, so dropping `releaseBody` leaves the
  page reading correctly while the update popover is blank.
- `npm run sync-version` **does not run in CI.** `tauri-action` invokes
  `npm run tauri -- build`, whose npm hook would be `pretauri`; `tauri.conf.json`'s
  `beforeBuildCommand` is `npm run vite:build`, whose hook would be `previte:build`. Neither
  exists, so `predev`/`prebuild` are never reached. CI builds whatever is committed, which is
  why `prepare` asserts the three version fields agree.
- `src-tauri/Cargo.lock` carries the crate's own version and is part of every bump commit, but
  `sync-version.mjs` does not write it.

## Verifying a run

**Never use `gh run watch --exit-status`.** It returned exit 0 on the v1.6.9 run where both
macOS legs had failed. Read the per-job conclusions:

```bash
RUN=$(gh run list --workflow=Release --limit 1 --json databaseId --jq '.[0].databaseId')
gh run view "$RUN" --json jobs --jq '.jobs[] | .name + ": " + .conclusion'
```
