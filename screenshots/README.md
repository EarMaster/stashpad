# Screenshots

The pictures of the app on [stashpad.org](https://stashpad.org) are generated from this
folder, once per release. Nobody takes them by hand, and nobody should: a hand-taken
screenshot carries whatever was in the queue that afternoon, ages the moment the UI
changes, and quietly leaks a real path or a customer name.

```bash
npm run screenshots                        # every scene, English and German
npm run screenshots -- --lang en           # one locale
npm run screenshots -- --scene queue,editor
npm run screenshots -- --headed            # watch the browser do it
```

Output lands in `screenshots/out/<locale>/<scene>.webp`, plus a `manifest.json`. The
folder is git-ignored; the website repo is where the images are actually kept.

## How it works

The desktop app talks to its Rust backend through exactly one door — `invoke()` in
`src/lib/services/desktop-adapter.ts`. Replace what is behind that door and the whole UI
runs in an ordinary browser with nothing about the app changed:

| File | What it is |
|------|------------|
| `fixtures.ts` | The invented dataset: contexts, stashes, attachments, settings |
| `mock-backend.ts` | A fake backend over `mockIPC`, answering the commands the UI calls |
| `demo.html`, `demo.ts` | The demo page: installs the mock, freezes the clock, boots the app |
| `capture.mjs` | Starts Vite, drives each scene through the real UI, writes the images |
| `assets/` | Files the fake attachments point at — images only; text previews are inline |

The demo page exists on the dev server and nowhere else: `vite build` takes only
`index.html` as its input, so none of this can reach a shipped binary.

## Keeping it reproducible

Two runs of the same source tree should produce the same images, so that a diff in the
website repo means the UI changed.

- **The clock is frozen** at the fixtures' `NOW`, or every relative timestamp in the queue
  ("6 minutes ago") would differ between runs.
- **The fixtures are fixed**, including the ids. Nothing is randomised.
- **The viewport, the device scale factor and the browser are pinned** in `capture.mjs`.
  Chromium comes from Playwright rather than the system, so the runner and your machine
  agree — though **fonts still differ between operating systems**, which is why the
  release job's Linux output is the set that gets published.

## Adding a scene

1. Add an entry to `SCENES` in `capture.mjs` with an id and a `drive(page)` that leaves
   the app in the state to photograph. Drive it the way a user would — click the button,
   do not reach into component state.
2. Add `showcase.<id>` and `showcase.<id>_desc` to **both** locales in
   `website/src/i18n/ui.ts`. The gallery reads its captions from there; a scene with no
   caption shows up unlabelled.
3. Run the capture and look at the result.

Two things bite when writing a `drive`:

- **Icon buttons have no accessible name.** The tooltip action reads the `title`
  attribute and then removes it, so `getByRole('button', { name })` finds nothing. Select
  on the icon instead — `button:has(svg.lucide-settings)` — which also fails loudly if the
  icon is swapped rather than photographing the wrong button.
- **Settings sections are matched by position**, not by heading, because the headings are
  translated. `SECTION` in `capture.mjs` maps names to indices; inserting a section above
  them means updating it.

## Publishing

`.github/workflows/screenshots.yml` runs on a published release, captures the released
tag, and commits the result into the website repo, which deploys on push. It needs the
`WEBSITE_SYNC_TOKEN` secret — a fine-grained PAT with `contents: write` on
`EarMaster/stashpad-website` — since a workflow's own token cannot reach another
repository. An unchanged UI produces byte-identical files and therefore no commit, so a
release that did not touch the interface does not spend the website's deploy throttle.
