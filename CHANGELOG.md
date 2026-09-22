# Changelog

All notable changes to Stashpad are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versioning
follows [Semantic Versioning](https://semver.org/). The GitHub release body **and the update
notes shown inside the app** are extracted from the `## [x.y.z]` heading matching `version` in
`package.json` — see `scripts/release-notes.sh` and `.github/workflows/release.yml`. Keep the
headings exact for that to keep working, and write the entries for the person reading the update
popover, not for the person who wrote the commit.

## [Unreleased]

## [1.8.6] - 2026-09-22

### Fixed
- **Encryption really does unlock itself on startup now.** 1.8.5 said it had fixed this and
  had not: the code that reopens your key as the app starts was added to a startup hook that
  never ran, because a second, empty one further down the file had been quietly replacing it
  since long before any of this. So the app still came up unable to read your synced stashes
  until you opened the encryption settings, and still asked for the recovery code you should
  never have needed. Your keys were correct the whole time and nothing needed re-approving -
  they were simply never opened
- **Your window transparency setting applies when the app starts, not only when you change
  it.** It was set from the same hook, so a fresh start ignored what you had saved and the
  window only looked right again after you touched the setting. On macOS the menu took the
  app's language the same way, and had the same problem

## [1.8.5] - 2026-09-22

### Fixed
- **Encryption stops asking for your recovery code every time you open the app.** The key
  that reads your stashes is held in memory only and has to be unlocked again on each start,
  from the installation key already sitting in your system keychain - but nothing did that
  unless you happened to open the encryption settings. Until you did, the app was locked:
  syncing was refused with a message telling you to update Stashpad when you were already on
  the newest version, and the settings page offered the recovery code, asking you to type
  thirteen groups to get out of a lock that need never have happened. It now unlocks as the
  app starts, again when a connection comes back, and once more before any sync, so a
  machine that was offline or slow to get online is not left locked either. Nothing was ever
  sent to the server unencrypted - it refused those syncs rather than storing anything

## [1.8.4] - 2026-09-21

### Fixed
- **Updating no longer asks for your device passphrase on a machine that has a keychain.**
  Installing an update restarts Stashpad while the previous copy is still shutting down, and
  for a moment both were checking whether this machine has a credential store - using the
  same scratch entry, which each one tidied up afterwards. Whichever got there second found
  its own entry already deleted, concluded there was no keychain and asked for a passphrase,
  and the next ordinary start reported the keychain as working again. Nothing was wrong with
  your keychain and nothing was at risk; the check now uses a name of its own each time
- **The encryption settings no longer claim the keychain is holding a key it is not.** The
  line added in 1.8.3 went by whether the credential store answered at all, not by whether
  your key was actually in it, so a machine where the store works but will not keep the key
  was told everything was fine - with no passphrase offered as a way out

## [1.8.3] - 2026-09-21

### Added
- **The encryption settings say where this installation's key is kept.** One line, under
  Verschlüsselung: your system keychain, or your device passphrase because no keychain could
  be used here, or neither - in which case nothing is saved on this machine at all. Until now
  the only clue was whether you had once been asked for a passphrase, which made a machine
  that has a keychain but failed to use it look exactly like one that has none

### Fixed
- **The encryption progress counts down on its own, with a bar.** It showed the number of
  stashes left over from the moment conversion started and then never moved, so the only way
  to see any progress was to leave the settings page and come back - and reopening it showed
  nothing left to do at all. The figure is now read from your own database a few times a
  minute, with a bar and a "x of y done", and it is still right after you close and reopen
  the window
- **The Finish button no longer looks switched off while encryption is running.** It was
  drawn as a faint outline for the whole sweep, which on a dark panel is exactly what a dead
  button looks like. It is now the panel's main button, greyed with a reason while stashes
  are still outstanding and plainly clickable the moment they are not

## [1.8.2] - 2026-09-21

### Fixed
- **Encryption can be switched on now.** v1.8.1 made the panel appear, and then it refused
  with "there is nowhere safe on this machine to keep an encryption key" on every machine
  that *does* have somewhere safe - Windows, macOS and any Linux with a keyring service. The
  app looked for this installation's key in memory but only ever put one there when a device
  passphrase had been set, so the one case that was meant to be ordinary was the only one
  that failed. It now keeps that key in the system credential store, creating it on first
  launch, and nothing about your stashes or your account was affected while it did not work
- **The encryption panel stops saying it is checking when it has stopped checking.** If that
  first look-up failed, the panel showed the error and went on reporting "Checking…"
  underneath it for as long as you left the window open, with nothing to click. It now offers
  a Try again button instead
- **Error messages are in German when the app is.** Anything that went wrong below the
  surface reported itself in English, so a German window would suddenly say "There is nowhere
  safe on this machine to keep an encryption key" in the middle of an otherwise translated
  panel. The messages worth translating now are, starting with everything encryption and the
  device passphrase can tell you. A message nobody has translated yet still appears in
  English rather than as a blank or a code, so this improves from here without anything
  breaking in between

## [1.8.1] - 2026-09-20

### Fixed
- **The encryption settings are actually there now.** Everything v1.8.0 added for encrypting
  your synced stashes shipped behind a panel that never appeared, so the whole feature was
  unreachable: no way to turn it on, no recovery code, no device approval. The panel was shown
  only when a field the app deliberately never gives the interface was set, which is to say
  never, on every platform. Update and you will find it under Settings › Cloud Sync once you
  are signed in. Nothing was wrong with the encryption itself and nothing needs redoing - it
  had no way of being started

## [1.8.0] - 2026-09-20

### Added
- **Your synced stashes can now be encrypted before they leave your computer.** Cloud sync
  stored them on the server in readable form: we did not read them, but we could, and the
  privacy policy had to say so. Turn encryption on under Settings › Cloud Sync and your
  stashes, contexts and their descriptions are encrypted on this machine first, under a key
  the server never keeps a copy of. Existing stashes are converted in the background while
  you keep working, and it picks up where it left off if you close the app. Three things are
  worth knowing before you switch it on, and the setting says all three: you get a recovery
  code, and if you lose every installation *and* that code then nobody can get your stashes
  back, us included; an access key for AI tools carries its own copy of the key, so the
  server can read that account while it is serving such a request, and creating no access
  key means it cannot read anything; and this protects what is on the server, not what is on
  your own disk
- **Attachments are encrypted too, including their file names.** A file name is often as
  revealing as the file — `Q3-layoffs-list.xlsx` tells you what it is without opening it — so
  with encryption on, the name, type and the bytes are all encrypted before they leave your
  machine, and the storage path no longer contains the name either. Nothing changes in how
  attachments look or behave in the app. Files you uploaded before turning encryption on stay
  as they were until they are converted, and Stashpad does not claim otherwise
- **Another computer has to be let in before it can read your stashes.** Signing in on a new
  machine gets you your queue, but not the ability to decrypt it until you approve it from a
  machine that already can — you compare a short code shown on both screens, which is what
  stops anyone slipping a machine of their own into your account. If none of your other
  installations is reachable, your recovery code lets the new one in instead. That is the
  ordinary way in when your other laptop is switched off, not an emergency measure, so keep
  the code where you keep your passwords
- **Access keys for AI tools are created in the app once your stashes are encrypted.** Giving
  a tool access means handing it a copy of your key, and only something that already has that
  key can do it — the account page does not, and deliberately never will. So for an encrypted
  account the key is created here instead, and the account page goes on listing and revoking
  them, which is where people look. Revoking a key now takes its copy of your key with it
- **You can put a Stashpad Cloud export back.** Downloading your data has always worked;
  restoring it never did, which made "export your data" a one-way door. The app now reads an
  `export.json` from the account area, decrypts it if your account is encrypted, tells you
  what it found before it commits anything, and puts the records back — from where they sync
  to your other machines as usual. It also writes your whole account out as Markdown, one
  file per context, in the same format the existing per-context export already uses
- **A machine with no system keychain can now protect Stashpad with a passphrase.** Some Linux
  setups - a server you reach over SSH, a minimal desktop with no keyring service - have nowhere
  safe to keep a secret, and until now Stashpad put the sign-in token there anyway, scrambled
  with a key anyone reading the folder could work out. On those machines Stashpad now asks once:
  set a passphrase, or work locally without one. With a passphrase, your sign-in token and AI
  provider key are protected by it. Without one, nothing is written to disk and the app keeps
  working offline, which it does in full. You can tick "remember the passphrase on this machine"
  to skip the prompt at startup - it says plainly that this leaves the key recoverable by anyone
  who can read your user account, and you can turn it off again under Settings › General.
  Forgetting the passphrase costs you nothing but a re-entry: your stashes are untouched.
  Windows, macOS and any desktop Linux with a keyring service never see this - they use the
  system store and are not prompted

### Security
- **Your sign-in token and AI provider key now go into the system credential store, as they
  were always meant to.** Stashpad asked for Windows Credential Manager, the macOS Keychain or
  the Linux Secret Service, but the library providing them was pulled in without naming any of
  them - so every build quietly used a stand-in that keeps nothing, every read came back empty,
  and both secrets always took the fallback path instead: a file in your Stashpad folder,
  scrambled with a key made from your computer name, the folder's own path and a word written
  in the source code. Anyone who could read that file could work the key out. The real
  credential store is now built in and checked at startup, and the first launch after this
  update moves both secrets into it and clears them from the file. Nothing to do by hand, and
  you stay signed in
- **A failure to reach the credential store no longer downgrades a secret behind your back.**
  If the store was momentarily locked or refused a prompt, Stashpad wrote the weaker file copy
  instead and said nothing, so a passing hiccup permanently lowered how a secret was protected.
  It now treats that as the error it is and leaves the secret in memory for the session rather
  than writing a weaker copy. Machines with no credential store at all - a headless Linux box
  with no keyring service - still use the encrypted file, which is what they have always done

### Removed
- Dropped a long-retired scrambling format that Stashpad fell back to whenever a stored secret
  failed to decrypt. Because it triggered on any failure and its key was a fixed word in the
  source, a damaged or tampered value came back as whatever that produced and was then used as
  though it were the secret. A value in that old format is still read once, during the move into
  the credential store, so nothing is lost

## [1.7.0] - 2026-09-18

### Added
- **Stashpad installs on ARM machines running Linux or Windows now.** Every release carried one
  Linux package set and one Windows installer, both for Intel and AMD processors, and only macOS
  shipped for two architectures - so an ARM Ubuntu machine or a Snapdragon Windows laptop had
  nothing to install at all. Releases now carry arm64 Linux packages and an arm64 Windows
  installer beside the existing ones; pick the file with `arm64` or `aarch64` in its name.
  Machines already running Stashpad are unaffected and keep updating to the build they were
  installed from

### Fixed
- **The last English leftovers in a German app are translated.** The Resize Images switch was the
  visible one: its label, its explanation and the warning shown once you turn it off sat in the
  middle of a list where everything above and below had switched over. Nine more places were
  written straight into the screen the same way and never reached the translations - the
  Subscription heading and its status line under Cloud Sync, the Days unit beside the
  auto-delete field, the CORS hint appended when an AI connection test fails, the attachment
  count in the import summary, the title of the file picker for attachments, and three labels
  only a screen reader reads out. The plan names Pro, Enterprise and Free stay as they are
- **The import conflict dialog and the editor's Clear button speak German too.** Twelve
  translations were missing rather than hardcoded, which shows up as the same thing and is
  easier to miss: the app quietly falls back to the English text, so the conflict dialog you
  get when an imported context disagrees with your current one was English throughout, as were
  the Clear button and its confirmation. Both languages now carry the same set of keys
- **The German app says du throughout.** Sixteen strings had been written in the formal Sie
  while everything around them used du, so the tone changed as you moved between screens - the
  quit prompt, the paste dialog, the error screen, the Apple Intelligence note and most of the
  settings descriptions among them

## [1.6.11] - 2026-09-17

### Fixed
- **A sync arriving while Stashpad is in the background no longer leaves the window wedged.** If
  you also run Stashpad on another machine, its syncs redraw your queue while you are off in
  another application - and a window that is hidden or completely covered is one Chromium has
  stopped drawing frames for. Everything that clears an element away waits on a frame: a fading
  panel ends when its animation reports finished, a card sliding into its new position ticks once
  per frame, a dialog unmounts from inside one. Redrawing the queue then starts animations that
  never end, so nothing is taken back down and the layout is left part way through a move. You
  came back to a window that looked normal and ignored every click, with a reload the only way
  out. A refresh that arrives while the window is off screen now waits until it is back, which is
  the first moment anyone could have seen it anyway
- **Import, export and the confirmation prompts can no longer wedge the window either.** They
  were the last dialogs built on a library that takes its panel back down from inside an
  animation frame, so a close that landed while the window was not being drawn left an
  invisible sheet over the app that swallowed every click. Import and export are the two that
  could arrange this unaided: both hand the screen to the system file picker partway through,
  which is the moment the window stops being drawn. All of them now close outright rather than
  waiting to be animated away. Escape, clicking outside, and tabbing within the panel work as
  before, and closing one now returns you to whatever you were on when it opened

## [1.6.10] - 2026-09-06

### Fixed
- **Leaving Stashpad for another window no longer leaves Ctrl stuck in the context switcher.**
  Holding Ctrl and tapping P cycles through your contexts, and letting Ctrl go picks the one you
  landed on - but Windows never tells a window that has lost focus a key came back up. Alt-tabbing
  away mid-cycle left the app believing Ctrl was still held: the switcher stayed on screen eating
  arrows and Enter, and the next time you released Ctrl for something else entirely, a paste or a
  select-all, it quietly moved you to whichever context happened to be highlighted. Losing focus
  now cancels the pending pick, the way Windows cancels its own Alt+Tab when something takes the
  foreground. A switcher you opened by hand from the header is left where it is
- **A rebound switch-context shortcut no longer answers to Ctrl as well.** Whatever you had bound,
  the app still checked for Ctrl, so an Alt+P binding opened the switcher on Ctrl+P too

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
