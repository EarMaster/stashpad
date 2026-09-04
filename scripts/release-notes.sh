#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only

# Copyright (C) 2026 Nico Wiedemann
#
# This file is part of Stashpad.
# Stashpad is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License, version 3,
# as published by the Free Software Foundation.
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
# See the GNU Affero General Public License for more details.

# Composes the GitHub release body for one tag and writes it to stdout: the CHANGELOG.md
# section for that version, followed by the boilerplate every Stashpad release carries.
#
#   bash scripts/release-notes.sh v1.6.9 [prev-tag]
#
# Used by .github/workflows/release.yml, and by hand when regenerating an old release
# (`bash scripts/release-notes.sh v1.4.1 | gh release edit v1.4.1 --notes-file -`). One
# copy of the boilerplate, so an edit to the macOS block cannot apply to only half the
# releases.
#
# The macOS quarantine block is emitted unless MAC_SIGNING_IDENTITY is set - the same
# condition release.yml uses - so a body produced here matches the one CI produces.
# Errors go to stderr; stdout stays clean enough to pipe straight into `gh`.

set -euo pipefail

TAG="${1:?usage: release-notes.sh <tag> [prev-tag]}"
VERSION="${TAG#v}"
REPO="${GITHUB_REPOSITORY:-EarMaster/stashpad}"
CHANGELOG="$(cd "$(dirname "$0")/.." && pwd)/CHANGELOG.md"

# The tag before this one, for the compare link. Absent on the very first release, and
# overridable because `git describe` needs the tag to exist locally.
if [ -n "${2:-}" ]; then
  PREV_TAG="$2"
else
  PREV_TAG="$(git describe --tags --abbrev=0 "${TAG}^" 2>/dev/null || true)"
fi

# CRLF is stripped inside awk rather than by a `tr` at the head of the pipeline. The
# reason is not style: the extracting awk `exit`s at the next section heading, so with
# `tr` upstream it stops reading while `tr` is still writing, `tr` takes SIGPIPE, and
# `set -o pipefail` turns that into a failure of the whole command substitution. That
# only bites once the file outgrows the pipe buffer - 64 KB on Linux, 16 KB on macOS -
# so v1.6.9 composed fine on Windows and Linux and failed on both macOS targets, the
# script reporting a missing section for a section that was there. Reading the file
# directly leaves nothing upstream to signal.
#
# The strip itself is needed because core.autocrlf=true is the default on Windows and on
# the windows-latest runner, and there is no .gitattributes pinning *.md to LF. Without
# it every extracted line ends in \r, which travels into $GITHUB_ENV, the release body
# and latest.json.
NOTES="$(
  awk -v ver="$VERSION" '
    # A literal prefix test rather than a regex: interpolating a version into one would
    # make every "." match any character, so "1.6.8" would also match "## [1x6y8]".
    BEGIN { head = "## [" ver "]" }
    { gsub(/\r/, "") }
    index($0, head) == 1 { found = 1; next }
    found && /^## \[/  { exit }
    found              { print }
  ' "$CHANGELOG" | awk '
    # Blank lines are buffered rather than printed: leading ones are dropped (nothing has
    # started yet), trailing ones never get flushed, and internal ones survive exactly as
    # written. The notes are rendered as markdown both on the releases page and by
    # UpdateDetails.svelte, where paragraph breaks and fenced code blocks depend on them.
    NF == 0 { if (started) pending++; next }
            { for (; pending > 0; pending--) print ""; started = 1; print }
  '
)"

if [ -z "$NOTES" ] && [ "${ALLOW_MISSING_CHANGELOG:-}" != "1" ]; then
  echo "release-notes.sh: CHANGELOG.md has no content under '## [$VERSION]'." >&2
  echo "Add the section, or set ALLOW_MISSING_CHANGELOG=1 to compose boilerplate only." >&2
  exit 1
fi

printf '%s\n' 'See the assets below to download the latest version of Stashpad.'

if [ -n "$NOTES" ]; then
  printf '\n%s\n' "$NOTES"
fi

if [ -n "$PREV_TAG" ]; then
  printf '\n**Full changelog:** https://github.com/%s/compare/%s...%s\n' \
    "$REPO" "$PREV_TAG" "$TAG"
fi

cat <<'BOILERPLATE'

---

**macOS:** choose `aarch64` for Apple Silicon (M1-M4) or `x64` for Intel.
BOILERPLATE

# Without notarization every macOS user sees "damaged and cannot be opened" and most will
# assume the download is broken, so the fix belongs in the release itself rather than only
# in the README.
if [ -z "${MAC_SIGNING_IDENTITY:-}" ]; then
cat <<'MACOS_UNSIGNED'

### ⚠️ macOS: "Stashpad is damaged and cannot be opened"

The macOS builds are not notarized by Apple. macOS refuses to launch a downloaded app whose signature it cannot verify, and reports it as damaged - **the download is fine**.

After dragging Stashpad to Applications, clear the quarantine flag:

```bash
xattr -dr com.apple.quarantine /Applications/stashpad.app
```

Right-click → Open does not help with this particular error; on Apple Silicon an unsigned binary is refused outright.
MACOS_UNSIGNED
fi
