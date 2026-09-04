# Changelog

All notable changes to Stashpad are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versioning
follows [Semantic Versioning](https://semver.org/). The GitHub release body **and the update
notes shown inside the app** are extracted from the `## [x.y.z]` heading matching `version` in
`package.json` — see `scripts/release-notes.sh` and `.github/workflows/release.yml`. Keep the
headings exact for that to keep working, and write the entries for the person reading the update
popover, not for the person who wrote the commit.

## [Unreleased]

## [1.6.9] - 2026-09-04

### Fixed
- **Two attachments with the same name are both kept now.** Adding a second `image.png` to a stash
  quietly replaced the first one's file: both entries stayed in the list but pointed at the same
  file on disk, so one of them showed the wrong size and opened the wrong picture, and removing
  either took out both. Files that share a name get a `(1)` suffix on disk while the list goes on
  showing the name you gave them
- **Cloud sync stops getting stuck on an attachment it can never upload.** The same collision was
  what wedged it: the surviving file no longer matched the size recorded for it, the server
  refused the upload on that basis, and every retry failed the same way, with the panel red and
  no way out short of deleting the attachment. The size is now read from the file at upload time
  and corrected, so an install sitting on "Attachments could not be uploaded" clears itself on the
  next sync with nothing for you to do. When an upload does keep failing, the panel names the file
  and says when it will be tried again, instead of quoting the server's reply
- **A stash added at the top of the queue now arrives at the top on your other devices.** Order
  and content sync on separate channels, so that moving a stash cannot overwrite text edited
  elsewhere - but a newly created stash only ever announced its content. Its place in the queue
  was never sent at all, and every other device fell back to appending it to the bottom, whichever
  end you had stashed it at. Creating a stash, and completing one, which sends it to the other end,
  now carry the position with them. Stashes that already exist keep the order each device has for
  them until you move one
- **The editor's Code button makes a real code block out of several lines.** It wrapped whatever
  you had selected in single backticks, which across line breaks is one inline span rather than a
  block - and a blank line inside it ended the span and left the backticks showing. A selection
  spanning lines now gets a fenced ``` block, with the fence lengthened if the code contains one
  of its own; a selection within one line still gets inline backticks. The same applies to Ctrl+E
- **The other toolbar buttons stop producing markdown that does not render.** Selecting a line
  usually takes its line break with it, and Bold, Italic, Code and Link put their closing marker
  after that break, where it showed up as stray characters instead of formatting. Emphasis also
  cannot cross a blank line, so bolding a selection covering two paragraphs formatted neither -
  each paragraph is now marked up on its own. Numbered lists count 1, 2, 3 rather than repeating
  "1.", and can be switched back off afterwards, which a list that had been renumbered could not
  be. Bullets and headings skip blank lines instead of leaving an empty item behind

## [1.6.8] - 2026-08-26

### Fixed
- **The paste-as-attachment threshold sticks now, and reads as a size.** Whatever you typed came
  back as 8 after a restart: the editor had been measuring pastes in bytes for a while, but the
  backend still treated the field as a line count and reset anything over 1000 while loading it.
  The number reached disk intact and was only wiped on the way back in, which is why it looked
  like a save that silently failed. Both halves now agree on bytes, a value that is merely too
  large is trimmed rather than thrown away, and 0 still means "ask me on every paste". The field
  shows a readable size such as "4.9 KB" at rest and the exact byte figure while you are editing
  it. Existing installs keep whatever is in their settings — most will be sitting at 8 and need
  one visit to Settings, rather than being migrated on a guess about what the stored number meant

## [1.6.7] - 2026-08-23

### Fixed
- **Closing the cloud sync dialog no longer freezes the window.** The window kept looking normal
  and ignored every click, recoverable only by reloading it. The dialog waited for its closing
  animation before unmounting, and a window that is not painting frames never gets there — so the
  closed dialog stayed in place with an invisible backdrop over everything. This is the one dialog
  that hands the foreground to another application the moment it opens, since it launches the
  browser to authorise, which is why it was the one that hit it
- **Your machine stays one machine in the account's installation list.** The device identity lived
  in the webview's storage, which is lost on any profile reset and was never shared between a
  development build and an installed one. Each loss minted a fresh identity, so a single computer
  piled up as one entry per identity, all labelled with the same hostname. It now lives in a file
  beside the rest of the app data, and whatever the webview still holds is carried over, so
  existing installations keep the identity the server already knows them by
- **A desktop login no longer expires after a day.** The replacement session token the server
  hands back on each sync was being discarded instead of stored

## [1.6.6] - 2026-08-22

### Fixed
- **The blurred desktop no longer shows through after a theme change.** Switching theme left a
  band of the window showing a blurred image of whatever was behind it. The window was created
  with a translucent backdrop regardless of the Visual Effects setting while the app separately
  made the window surface clear, so any region the app had not yet painted showed the desktop.
  The window is now only left clear when translucency is actually switched on, and a theme change
  re-syncs the background, the effects and the layout together
- **Signing back in actually resumes syncing.** After a "session expired", signing in again left
  the panel still offering to log in and nothing syncing until the app was restarted. The re-login
  path only acted on a change in whether syncing *should* happen, and an expired session looks
  exactly like an enabled one — so nothing fired, and the stale error stood until the
  fifteen-minute fallback came round. A new token is now treated as an event in its own right,
  which also replaces the connection still holding credentials the server has stopped accepting
- **Signing in again no longer makes the next sync re-send everything.** Refreshing the
  subscription overwrote the whole cloud configuration with the account response, which has never
  carried the sync cursor — and a device with no cursor pushes its entire dataset on the next sync
- Cloud storage usage was only fetched when the settings panel opened, so signing in from an
  already-open panel left the storage block blank until you closed and reopened it

## [1.6.5] - 2026-08-21

### Added
- **The queue filter can find the stashes carrying files.** Alongside the tag chips there are now
  chips for "any attachment" and "no attachments", plus one per kind — images, videos, text and
  code, other files. Chips only appear for kinds something actually has, and combining them with
  tags narrows by both: `#bug` plus Images means tagged `#bug` *and* carrying an image
- **The markdown toolbar has keyboard shortcuts**, so formatting no longer means leaving the
  keyboard mid-sentence: `Mod+B` bold, `Mod+I` italic, `Mod+K` link, `Mod+E` inline code,
  `Mod+Alt+3` heading, `Mod+Shift+8` bullet list and `Mod+Shift+7` ordered list, where Mod is Cmd
  on macOS and Ctrl elsewhere. The tooltips name each one with the platform's own glyphs. AltGr
  combinations on European layouts are left alone, so AltGr+E still types a euro sign

### Fixed
- **Cloud sync no longer churns, and a dead sync no longer shows a green tick.** The settings
  panel shifted and the header icon strobed every couple of seconds while nothing actually
  synced: a per-cycle upload budget was being spent on attachments that were already uploaded, so
  the same backlog was deferred and retried forever, and a sync whose credentials had been
  rejected ran to completion and overwrote its own error with success. Measured on the running
  app, that is 31 layout shifts in 40 seconds before and none after, and one sync at startup
  instead of a permanent cycle. The panel is also laid out so sync activity cannot reflow it, and
  a fast sync no longer flashes a spinner at all
- **A theme change no longer splits the window.** Switching theme left the settings panel across
  the top and a solid dark block filling the rest, because the page itself painted nothing behind
  the app and the raw window backdrop showed through. The page now paints its own background
  unless translucency is on, the effects are only re-applied when visual effects are actually
  enabled, and "System" resolves against the desktop instead of assuming dark. A light theme on a
  dark desktop also stopped leaving tag and AI badges dark against a light panel

### Changed
- **Cloud Sync says that it is in closed beta.** People outside the beta were offered an "Enable
  Cloud Sync" button leading to a device-link flow they cannot complete; there is now a notice
  pointing at the waitlist beside it
- The license notice in each source file said "version 3, or any later version" while the SPDX tag
  beside it said `AGPL-3.0-only`. The prose now matches the tag; `LICENSE` is unchanged

## [1.6.4] - 2026-08-21

### Added
- **Stashpad checks for its own updates now, and the "Check for Updates" button works.** The
  button had never done anything: the update permission was missing, so the check was rejected
  before it started and the rejection was swallowed. Around that fix the rest of the feature is
  built — a check 20 seconds after launch and every 48 hours after that, driven by a stored
  timestamp so sleep and background throttling cannot defeat it, and a failed check does not cost
  you 48 hours. Automatic checks are passive: they light up an indicator in the header and never
  interrupt with a dialog. "Remind me in 7 days" and "Skip this version" are separate choices,
  both reversible from Settings. Release notes are rendered as markdown in the popover and in
  Settings, which is why they are worth writing
- Installs that another program owns are now recognised and left alone. A build from the Mac App
  Store, the Microsoft Store, winget or a package manager is told how to update rather than
  offered a button — overwriting a bundle another installer owns breaks it rather than updating it

### Fixed
- **Four settings were silently discarded on every save.** Image resizing, interface scale and the
  video volume and mute state existed only in the frontend, so the backend dropped them each time
  the settings file was written

## [1.6.3] - 2026-08-21

### Fixed
- **The settings page no longer freezes a few hundred milliseconds after opening.** The
  storage-usage bar was missing quotes around two Tailwind class names, so both branches of it
  threw as soon as the usage figure arrived. With nothing catching the error the whole view
  stopped updating while handlers kept firing, which is why it read as a frozen window rather than
  an error. The quotes are the fix; the reason it survived two releases unreported is that nothing
  was watching, so there is now an error boundary around the view offering retry and reload, and
  uncaught errors are written to the app log — a release build discards the webview console, so
  previously they left no trace on disk at all
- **Typing in the settings panel no longer wedges the app.** Five text fields saved on every
  keystroke, and each save rewrote the whole settings file and could reach the OS credential
  store. Ten characters meant ten concurrent writes, until nothing in the app could answer at all.
  Those five now wait until you pause; switches and pickers still save immediately, and a pending
  save is flushed if you leave the panel mid-word
- **Pasting a large image no longer freezes the window.** The bytes were converted one element at
  a time in the webview before being sent to be written; they now go across as-is
- **One bad character in a server error no longer takes the app down with it.** Two error paths
  cut the response at a fixed byte offset, which crashes whenever that offset lands inside a
  multi-byte character — an umlaut in a proxy page, an emoji in a validation message. A crashing
  command never answers, so the sync that called it stayed "in progress" and refused every later
  sync for the rest of the session. Related: a single crash used to poison the shared locks for
  the life of the process, so everything after it failed too
- **A slow disk or credential store no longer freezes everything.** Settings were saved on every
  successful sync, and each save wrote both secrets to the OS credential store and read them back
  to verify — so merely typing a stash drove credential traffic until the app ran out of workers.
  Secrets are now written only when they change and verified once per run, and the settings file
  is written and renamed into place rather than truncated, which could lose the cloud
  configuration and the sync cursor. Attachment reads and writes, archive packing, import file
  copies and file previews all moved off the interface thread as well
- **A sync that pulled new stashes down but failed an upload left those stashes invisible** until
  something else happened to reload the list
- **Deleting the last failing attachment no longer pins sync to "error" forever**, with nothing
  left to retry that could clear it
- Closing the window no longer occasionally leaves the process running

### Added
- The link-code exchange identifies this installation, so the account page can revoke one
  connected installation rather than all of them. Older versions keep working

## [1.6.2] - 2026-08-20

### Fixed
- **Manual stash order syncs between devices now** — position was in no payload, no server record
  and no migration, so a reorder never left the machine that made it
- **A reorder can no longer destroy an edit made on another device.** Reordering stamped every
  stash it touched as changed and pushed each one as a whole record, and conflicts are resolved
  per record: edit a stash's text on one device, reorder on another before the first has synced,
  and the reorder — carrying the old text under a newer timestamp — won the whole row. Order now
  travels on its own, so it can no longer overwrite anything, and two devices that compute the
  same position independently now agree on the resulting order

## [1.6.1] - 2026-08-20

### Fixed
- no more freezing while another device is syncing, or during import and export
- import and export moved off the UI thread entirely, so large archives no longer lock the window
- export is no longer greyed out for contexts whose stashes are all completed
- large pastes and file previews no longer stall the app

### Added
- sync state is now visible in the header, not just in Settings
- Settings shows how much cloud storage the account is using

### Removed
- removed the attachment re-upload button, which fixed a problem that no longer occurs

## [1.6.0] - 2026-08-20

### Added
- **Attachments that never reached the cloud can be recovered.** Files added before the storage
  endpoint was fixed were never uploaded, and the server cannot help — it never received those
  bytes — so a "Re-upload attachments" action in the cloud settings does it from a machine that
  still holds them. It also re-links files that lost their database row and had become invisible
  to both the app and sync; only files whose stash still exists are adopted, rather than guessed at

### Fixed
- **A stalled transfer no longer holds up all syncing.** None of the HTTP calls had a timeout, so
  one unreachable host wedged stashes and contexts along with it until the operating system gave
  up. There is now a 30 second budget for ordinary calls and 5 minutes for file transfers, both
  giving up on an unreachable host in 10 seconds
- **A failed download is no longer permanent for the session.** It was never retried until the app
  restarted or you clicked the chip; failures now back off and retry on their own, while clicking
  still skips the wait. Failed uploads had the opposite problem — they were re-read and re-sent in
  full on every single sync, forever
- **Downloads cannot leave a half-written file behind.** They are written aside and moved into
  place, so a crash or a full disk no longer leaves a truncated file that every later check reads
  as a complete download
- **Attachments of deleted stashes stopped being uploaded over and over.** Three files never
  uploaded while sixteen others did; what they had in common was belonging to stashes that had
  been deleted, and nothing marks such a file done, so every sync retried all of them
- **An attachment upload that fails now says so.** Failures were caught, written to a console
  nobody has open, and discarded, so sync reported success and there was no way to notice short of
  querying the database by hand. That silence is why the underlying upload bug went unnoticed
- **The completed-stash cleanup settings do something now.** "On close" deleted the rows but left
  the files on disk, and "after N days" was not implemented at all — it could be selected and had
  no effect whatsoever

### Changed
- Deleting or creating a stash no longer freezes the interface about two seconds later. The
  database work in the sync path ran on the thread the window is drawn on, and a sync ran several
  of those in a row across the whole table

## [1.5.1] - 2026-08-19

### Fixed
- **Attachments no longer vanish from their stash a couple of seconds after being added.** Three
  faults compounded: the server withholds a file until its bytes are confirmed, so a sync moments
  after adding one returned that stash with no attachments; the client adopted that empty list
  wholesale; and writing the stash back deleted the existing row first, taking its attachments
  with it. The file was therefore destroyed before it could ever be uploaded, which is why not a
  single attachment had ever been stored. Fixed at each layer, so no single one has to hold —
  attachments now only ever arrive through a merge, and disappear only when you remove them

### Changed
- **A sync sends only what changed.** Every sync uploaded all 453 stashes and 18 contexts
  regardless, and since a sync runs on every local write, a one-character edit cost several
  hundred database round-trips on the server. Records are now tracked through a claimed /
  in-flight / acknowledged handshake, so an edit made while a push is in the air stays queued
  instead of being acknowledged away and lost. The first sync after upgrading is still a full push
- Sync stopped loading the whole stash list three times per cycle and making a round-trip per
  attachment, including for the ones with nothing to upload

## [1.4.1] - 2026-08-19

### Fixed
- **Auto-update can actually verify an update now.** v1.4.0 built and signed correctly, but the
  update manifest was never assembled, so neither it nor the signatures were published — leaving
  installed apps with nothing to check an update against and the whole signing setup inert
- The build was failing on a comment key in the Tauri configuration, which is validated against a
  schema that does not allow one

### Changed
- Release notes are generated from the commits since the previous tag instead of being a fixed
  sentence, with a compare link, a note on which macOS download matches which chip, and the
  quarantine workaround — the last shown only while the macOS builds are unsigned

## [1.4.0] - 2026-08-19

### Added
- **An attachment still downloading is shown as such, and opening one jumps it to the front of the
  queue.** Files arriving from another device used to be indistinguishable from broken ones.
  Pending attachments now render as a dashed stub with a cloud icon, the filename and the size —
  the only things known before the bytes arrive — and a failed download says so and can be
  retried by clicking. Downloads moved out of the sync cycle into a background queue, so a large
  backlog can no longer stall a sync, and the file you actually opened is fetched ahead of the
  backlog you did not ask for and opens the moment it lands

### Fixed
- **Attachments propagate between devices.** Stashes synced instantly and attachments never
  arrived. Confirming an upload published the file but did not touch the timestamp that
  conflict resolution compares, so the receiving device judged the stash unchanged and never
  learned the attachment existed. Nothing woke the other device either, until its fifteen-minute
  fallback poll came round
- The updater signing key generated for v1.4.0 could not sign anything: generating one with an
  empty password produces a key that is still encrypted, and every attempt to use it failed.
  Caught by signing a throwaway file rather than by the release build

### Changed
- Update signatures are configured, so installed apps can verify an update rather than the
  configuration still carrying its placeholder. Apple code signing is wired up but stays off — it
  needs a paid Apple Developer Program membership — and the README now documents the "app is
  damaged and cannot be opened" workaround and which download matches which chip

## [1.3.0] - 2026-08-19

### Fixed
- **Cloud sync propagates between devices.** Linking an account worked and data never arrived, for
  four independent reasons, each enough on its own. An edit re-sent the timestamp the server
  already held, so the server judged it not newer and discarded every edit, completion and context
  move — only brand-new stashes and deletions carried a fresh timestamp, which is why the first
  stash appeared on the other device and nothing after it ever did. Signing in during a session
  never started syncing until the next restart. A device linked by code never learned it was
  entitled to sync, and silently never synced — no error, no status change, still showing as
  signed in. And almost nothing scheduled a sync in the first place: editing, completing,
  deleting, reordering, moving between contexts and attaching a file to an existing stash all
  waited on the fifteen-minute fallback poll

## [1.2.8] - 2026-06-14

### Changed
- Groundwork for the Rust backend and the shared dialog components the following releases build on

## [1.2.7] - 2026-06-14

### Fixed
- **Two devices stopped disagreeing about what is current.** Timestamps were compared in different
  units on either side of the exchange, merges dropped records, and a deletion on one device did
  not stay deleted on the other — it needs to travel as a marker rather than as an absence

## [1.2.6] - 2026-04-08

### Fixed
- Modification times are stored and compared as Unix timestamps throughout, so sync compares like
  with like. Attachment records missing a field no longer fail to load

## [1.2.5] - 2026-04-08

### Added
- **The account page lists the devices connected to it**, rather than an account being an opaque
  thing you either are or are not signed in to
- **Ordered lists in the editor**, and the list and quote buttons now toggle: pressing one on a
  line that already has that prefix removes it instead of adding a second
- **The editor expands to fill the window** and collapses back
- Hex colour codes in markdown render as a colour swatch

### Fixed
- Pasted text is kept as a `.txt` attachment rather than the extension being guessed from content

## [1.2.4] - 2026-03-14

### Added
- **A "Check for Updates" button in Settings**, and a "Check for Updates…" item in the macOS app
  menu after "About". A manual check says so when there is no update rather than doing nothing
  visible, and its messages are translated

## [1.2.3] - 2026-03-13

### Added
- **The completed section of the queue collapses**, and expands on its own when you complete
  something
- **Cloud attachment sync**, so files travel with their stashes rather than staying on the device
  that created them
- Signing in returns you to the app directly instead of leaving you to find your way back
- The AI now answers in the language you wrote in

### Fixed
- **The cloud endpoint moved to `api.stashpad.org`**, and a configuration still pointing at the
  old one is repaired on start rather than silently failing
- Stash cards, markdown tables and the queue handle content that overflows them, and the editor and
  stash cards keep a minimum height instead of collapsing to a sliver
- Account links now point at the portal rather than at a page that is not there

## [1.2.1] - 2026-03-09

### Fixed
- **The editor grows with what you type**, and a stash card keeps its height while being edited
  instead of the queue jumping under the cursor
- Headings render at distinguishable sizes in markdown content
- Cloud sync is available to enterprise-owned accounts, not only to direct subscribers

## [1.2.0] - 2026-03-07

### Changed
- Version bump only; no user-facing changes since 1.1.5

## [1.1.5] - 2026-03-07

### Added
- **Cloud sync**, with a dedicated sign-in flow. Tokens are held by the backend in the operating
  system's credential store rather than anywhere the webview can reach, and the API calls are made
  there too
- Inline tag styling in stash content

## [1.1.4] - 2026-03-06

The first published release. Stashpad is a local-first scratchpad: a queue of stashes, grouped
into contexts, that switch with what you are working on.

### Added
- **Stashes and contexts.** Create, edit, complete, delete and move stashes; reorder them by
  dragging, or send one straight to the top or bottom. Contexts group them, switch by fuzzy search
  from the switcher, can be created from the switcher itself, and can switch automatically from
  the active window — by plain match, case-sensitive match or regular expression
- **Markdown, syntax highlighting and tags** in stash content, with tag suggestions as you type
- **File attachments.** Drag files in, paste them, or paste long text to have it stored as a file;
  images are resized on the way in, and attachments have previews and can be dragged back out to
  other applications
- **AI assistance** for enhancing a stash, with configurable system prompts, an LM Studio preset,
  and Apple Intelligence on supported Macs
- **Import and export** of the whole store as an archive
- **Automatic cleanup** of completed stashes, on close or after a set number of days
- Light and dark themes, launch at startup, window shadows, translations, and fireworks when you
  finish a queue

### Changed
- Data is stored in a local SQLite database
