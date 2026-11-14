# Features

A detailed description of what WatchMark does and the rules it follows. For how the pieces are implemented, see [System design](architecture/SYSTEM_DESIGN.md).

- [Playback tracking](#playback-tracking)
- [Library and metadata](#library-and-metadata)
- [Dashboard](#dashboard)
- [Show and movie pages](#show-and-movie-pages)
- [Scanning local files](#scanning-local-files)
- [The Inbox](#the-inbox)
- [Watch history](#watch-history)
- [Settings and data safety](#settings-and-data-safety)
- [Desktop integration](#desktop-integration)
- [Not yet implemented](#not-yet-implemented)

---

## Playback tracking

WatchMark doesn't play video itself. It starts **VLC Media Player** and watches what VLC reports.

**Starting playback**
- Pressing play on an episode launches VLC with that episode's file and VLC's HTTP interface enabled.
- The interface is bound to `127.0.0.1` only, on the first free port between 8080 and 8090, with a random 16-character password generated for that session.
- If you've watched part of the episode before, VLC starts at the saved position. If the saved position is within 5 seconds of the end, it starts from the beginning instead.
- Only one tracked VLC instance runs at a time; starting a new episode closes the previous one.
- Before launching, WatchMark checks that the file still exists. If it has moved, you're offered a **Locate** button to point at the new location.

**While you watch**
- WatchMark polls VLC every 5 seconds (more often for the first few seconds, and briefly after you skip around).
- Your position is saved when you pause, after a jump of more than 30 seconds, and every 5 minutes of continuous playback, not on every poll.
- If VLC reports a file length that differs from TMDB's runtime by more than 2 minutes, the episode's runtime is corrected from the file.

**When you close VLC**

| What happened | Result |
| :--- | :--- |
| You watched at least 90% (for clips under 5 minutes: all but the last 30 seconds) | Episode marked **Completed**, watch count increased, resume position reset |
| You stopped earlier but watched at least 5% (or 10 seconds for short clips) | Episode marked **Watching**, position saved for resume |
| You watched less than that | Treated as an accidental launch: the play is discarded and any earlier progress is kept |
| You skipped straight to the last few seconds | Counts as completed only if you'd actually reached within 1% of the end |
| VLC crashed or was killed | A "Playback interrupted" notice is shown |

Plays of the same show less than 6 hours apart share a session ID, so they appear together as a binge session in your history.

---

## Library and metadata

**Adding titles**
- Search TMDB from the Search page or the search bar in the header. Searching is debounced while you type; press Enter to search immediately.
- Results include shows and movies, plus the known works of any person you search for, without duplicates. Filter results by All, TV or Movies, and scroll to load more.
- Titles already in your library are marked **In Library**.

**What gets fetched**
- Shows: every season and episode, with titles, synopses, runtimes, air dates, ratings, genres, networks, posters, backdrops and episode stills.
- Movies: details plus collection (franchise) information.
- Seasons are fetched one at a time with a short pause between requests to stay within TMDB's rate limits. If TMDB returns "too many requests", WatchMark waits for the time TMDB asks for. A season TMDB doesn't have is skipped rather than failing the whole import.
- Progress for long imports is shown on the page and in a bar at the top of the window.

**Images**
- Posters, backdrops and episode stills are downloaded to a local cache, so cached artwork keeps working offline.
- Backdrops without text are preferred. If a title has no backdrop, a cropped poster or a gradient is used instead.
- High-DPI displays get higher-resolution images; **High performance mode** in Settings uses smaller ones.
- Episodes without a still fall back to the show's backdrop.
- Stills of episodes that aired in the last two days or haven't aired yet are blurred to avoid spoilers until you hover over them.

**Keeping it current**
- A few seconds after launch, WatchMark checks your tracked shows for new seasons, newly scheduled episodes and changed air dates.
- Manually edited air dates are never overwritten by a refresh.

---

## Dashboard

- **Spotlight:** a large banner suggesting what to watch next. Candidates are scored on whether you're mid-episode, newly aired episodes, season finales, how recently and how steadily you've been watching, and whether a local file exists. It rotates every 8 seconds.
- **Continue Watching:** episodes you've started, with progress bars.
- **Recently Added:** newest titles in your library, with a one-click **Play Next**.
- **Upcoming Airings:** the next scheduled episodes of tracked shows, with "Airing today" highlighted.
- **New Season alerts:** a dismissible banner when a tracked show gets a new season.
- **New Season badges:** posters (here and in the library) show a **New Season** badge for 30 days after a show's season 2 or later premieres. The badge follows each season's earliest episode air date, so it appears whether the season arrived through the background sync or when you added the show, and dismissing the banner doesn't remove it. On the show page, that season's tab is marked **New**.
- **Stats:** shows tracked, hours watched and your average rating.

---

## Show and movie pages

- Backdrop banner, poster, synopsis (with "View more" for long text), genres, networks and air dates.
- **TMDB rating** and **your rating** side by side. Your rating is 0–10 in half-star steps; click the current rating again to clear it.
- Season tabs, defaulting to the first season with unwatched episodes. Specials appear as season 0.
- Each episode shows its number, title, runtime, air date and overview. Unaired episodes are marked and can't be played.
- Per-episode actions: play or resume, toggle watched, and (from the right-click menu) unlink the local file.
- **Play Next** picks the earliest unwatched episode that has a local file and tells you if it had to skip a missing one.
- **Mark season watched** marks a whole season as watched in one go and records it in your history.
- **Remove show** deletes the show and its history (you're told how many history entries will go first). Your video files are never touched.

---

## Scanning local files

Choose a folder in Settings (or press `Ctrl+Shift+S`) and WatchMark scans it in the background. You can pause, resume or cancel a scan.

**What is scanned**
- Folders up to 15 levels deep.
- Only the file extensions listed in Settings (configurable).
- Hidden files, partial downloads (`.part`) and files that are empty or fail a basic header check are skipped.
- `.lnk` shortcuts and symlinks are followed to their targets, with loop detection.
- Windows paths longer than 260 characters and network shares are supported. Folders you don't have permission to read are skipped and logged.

**How files are matched**
- Release-group tags, resolution, codec and other noise are stripped from file names first.
- Recognised patterns include `S01E02`, multi-episode ranges like `S01E01-E02`, `1x02`, `Season 1 Episode 2`, bracketed anime numbering and dated episodes. Movies are matched on title and year.
- If a parsed title doesn't exactly match a show in your library, a fuzzy match above 90% similarity is accepted.
- If a matched episode already has a file, the larger file wins.
- Files that were moved or renamed are recognised by size and name, so their history is kept.
- Files already known from earlier scans are skipped, which makes re-scans fast.

When a scan finishes, a summary shows how many files were matched automatically and how many went to the Inbox.

---

## The Inbox

Files the scanner couldn't match are grouped by their guessed show name.

- **One-click match:** search TMDB for the group and assign it; each file is linked to the episode its name suggests. Files that can't be placed stay in the Inbox.
- **Manual mode:** pick a target show, then set the season and episode for each file yourself.
- **Ignore** a group (with a 5-second undo) or **Clear Inbox**. Ignoring only removes entries from the Inbox; your files are not touched.
- Entries whose files have been deleted are pruned automatically, unless the whole drive is missing (for example an unplugged USB drive).

---

## Watch history

- A timeline grouped by day: "Today", "Yesterday", weekday names for the last week, then full dates (with the year for older entries). Each day shows how many titles you watched and for how long.
- Several episodes of the same show watched within 6 hours of each other appear as one **binge session**, labelled with the time of day (Morning, Afternoon, Evening, Late Night) and marked when it crossed midnight. Expand it to see each episode.
- Each entry shows the time, its state (**Watched**, **Marked as watched**, **Archived**, or how far you got), a play count if you watched it more than once in the session, and how long after airing you watched it.
- Filter by **All**, **TV Shows** or **Movies**.
- Remove a single play or a whole session (with a 5-second **Undo**), or **Clear history**. Removing history doesn't change an episode's watched status or resume position.
- History refreshes after a VLC session ends. If you're scrolled down when that happens, a **New activity** button appears instead of the list jumping.
- Times use your system time zone. If it can't be detected, times are shown in UTC and labelled as such.

---

## Settings and data safety

**Settings** are grouped into General, Playback, Scanner, System, Database and Advanced. Changes are saved only when you press **Save**. Leaving the page with unsaved changes asks whether to save or discard.

**TMDB API key**
- Checked against TMDB as you type, with a manual **Test connection** button.
- Stored in Windows Credential Manager. If that's unavailable, it's stored obfuscated in the settings file.
- If TMDB starts rejecting the key mid-session, features that need it are disabled with an explanation.

**Backups and maintenance**
- An automatic backup is made once a day. Each backup gets a SHA-256 checksum file, and you choose how many to keep (1–50, default 5).
- **Backup DB** exports a copy wherever you like.
- **Import Backup** restores a backup after you type `RESTORE` to confirm. The file is checked first (SQLite integrity check, plus the checksum when one exists), then swapped in on the next start. The previous database is kept as a fallback.
- **Clean Database** compacts the database and refreshes its statistics, after checking there's enough free disk space. Scans and playback are paused while it runs.
- **Repair paths** rewrites stored file paths when you move your media to a new drive letter.

**Other settings** include the VLC path (with **Restore Default** to find a standard install), supported file extensions, high-performance image mode, Cinema Mode animations, and log levels (globally or per backend module).

---

## Desktop integration

- **System tray** with Scan Directory, Resume Last Show and Quit. Double-click the tray icon to bring the window back.
- **Global shortcuts:** `Ctrl+Shift+S` to scan and `Ctrl+Shift+H` to hide or show the window. If another app already uses a shortcut, you're told instead of the app failing.
- **Window position and size** are restored on launch. If the saved position is on a monitor that's no longer connected, the window is centered on the main screen.
- **Notifications** for events such as a missing file when resuming from the tray.
- **Connection status:** WatchMark regularly checks whether TMDB answers, separately from general internet access.
  - **TMDB blocked** (the internet works but TMDB doesn't, as with some providers such as Jio): a message explains this and suggests Cloudflare WARP or 1.1.1.1 DNS, with **Retry** and a link to WARP.
  - **Offline:** a message says what still works.
  - While either lasts, search and refresh are disabled with an explanation, background show updates pause, and TMDB requests fail immediately instead of timing out. The check repeats every 15 seconds; once TMDB answers again, you get a short confirmation, the paused show update runs, and failed image downloads are retried.
- **Crash containment:** if a page fails to display, you get a recovery panel with **Try again** and **Reload app**, the rest of the app keeps working, and the error is written to `WatchMark.log`.

---

## Not yet implemented

These look like features in the UI or older docs but don't work yet:

- The tray's **Check for Updates** item is a placeholder. It always reports that WatchMark is up to date.
- The season-level **Refresh data** button on show pages calls a backend command that doesn't exist yet, so it fails.
- The **In Library** badges in search results don't appear, because the lookup they depend on uses a command that doesn't exist.
- The TV and movie libraries load only their first 50 items.
- Backdated "archive" imports of past viewing exist in the backend but have no button in the UI yet.

These are tracked for upcoming releases. See the [roadmap](../ROADMAP.md).
