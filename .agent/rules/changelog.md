---
trigger: always_on
---

# CHANGELOG

`CHANGELOG.md` is the single source of the GitHub release body **and** of the update notes the
app shows in the header popover and in Settings > Updates. `scripts/release-notes.sh` extracts
the section whose `## [x.y.z]` heading matches `version` in `package.json`; there is no other
input, and nothing is derived from commit messages.

## Add the entry as part of the change

When a change is user-facing, add its entry under `## [Unreleased]` **in the same commit that
makes the change** — not as a follow-up, and not at release time, when the reasoning behind it
has to be reconstructed from a diff.

Skip it for anything a user of the app cannot see: CI and workflow changes, documentation,
internal tooling, refactors with no behavioural effect, test-only commits.

Create `## [Unreleased]` directly under the file's header paragraph if it is missing, and use
these subsections, in this order where more than one applies:

    ### Added   ### Changed   ### Fixed   ### Removed   ### Security   ### Performance

## How the entries read

Write for the person reading the update popover, deciding whether to install. Lead each bullet
with a **bolded sentence naming the user-visible outcome**, then unbolded prose on what was
wrong, why it mattered and what the limits are. Not a commit subject — a commit subject says
what was changed, an entry says what it means for someone using the app.

- Wrap at about 100 characters, with a 2-space hanging indent on continuation lines
- No trailing full stop
- No issue, PR or commit references: the popover renders bare URLs as one unbreakable word,
  and a reader there cannot follow them anyway
- Lead with the most important change — in the popover the first bolded sentence is what most
  people will actually read

An entry that is genuinely minor can be a single unbolded line; not every bullet needs a
paragraph.

## Checking it

`npm run check:changelog` verifies the current `package.json` version has a non-empty section.
It is also a gate job in front of the release build, so a missing section fails the release in
about 20 seconds rather than after four platform builds.

Stamping `[Unreleased]` with the version and the date is the release procedure's job — see
`.agent/workflows/create-new-release.md`. Do not do it as part of an ordinary change.
