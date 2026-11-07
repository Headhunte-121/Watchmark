# Changelog

All notable changes to the WatchMark Media Tracker project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [1.2.1] - 2026-11-08

### Added
* **Remove from History**: Remove a single play or a whole binge session from your watch history, with a 5-second **Undo**. Watched status and resume points are not affected.
* **Clear History**: Clear your entire watch history after a confirmation prompt.
* **History Filters**: Filter the timeline by All, TV Shows or Movies.
* **New Activity Prompt**: If you watch something while scrolled down in History, a "New activity" pill lets you jump to the latest entries instead of losing your place.
* **Crash Recovery Screens**: If a page fails to display, WatchMark now shows a recovery panel with **Try again** and **Reload app** instead of a blank window, and the rest of the app keeps working. The error is written to `WatchMark.log`.

### Changed
* **Watch History Redesign**: The timeline is now a single-column list grouped by day, with daily totals, 16:9 thumbnails, clear Watched / Marked as watched / Archived / in-progress states, and a play count for repeat plays.
* Day headings in History stay pinned directly under the top bar while you scroll.
* The top bar now darkens as you scroll on every page, so content no longer shows through behind the search bar.
* Older dates in History now include the year.

### Fixed
* **Blank window on startup**: The app could open to an empty dark window as soon as the Dashboard finished loading. Fixed.
* **Watch progress not saved to History**: Plays recorded from VLC always showed 0% and were never marked as watched in History, and abandoned plays (closed within the first seconds) were never cleaned up. Completion is now recorded when you close VLC.
* History stopped loading older entries after the first page; the full history now loads as you scroll.
* Binge sessions no longer split in two at page boundaries.
* Day headings in History floated in the middle of the list and covered entries.
* The **Archived** badge never appeared, and archived entries could be wrongly grouped with each other.
* A single entry with missing data (such as an unknown runtime) could stop the whole History tab from loading.
* History now refreshes after you finish watching in VLC or delete a show, and shows a clear error with **Try again** instead of an empty page if loading fails.

> **Note on existing history:** plays recorded before 1.2.1 show 0% progress because of the bug above. Their real progress was never saved and can't be recovered automatically.

## [1.2.0] - 2026-09-26

### Added
* **Intelligent Hero Spotlight Recommendation Engine**: 8-dimensional weighted candidate scoring algorithm prioritizing paused in-progress shows, newly aired episodes, and season finales with dynamic library fallback backfill and slide counters.
* **BingeBlock Accordion Timeline**: Dedicated interactive accordion components (`BingeBlock.tsx`) grouping consecutive watch events with fluid Framer Motion auto-height animations.
* **Midnight Crossover Detection**: Dynamic epoch timestamp calculations identifying binge sessions that span past midnight, automatically rendering a `<Moon />` indicator.
* **Distinct Show Session Isolation**: Auto-chains consecutive episodes watched within 6 hours (< 21,600s), but immediately terminates and mints a new session ID if the user switches to a different show.
* **DOM Virtualization (`VirtualPoster`)**: High-performance `IntersectionObserver` windowing engine that unmounts off-screen poster elements, enabling smooth 60 FPS scrolling across 1,000+ media titles with low memory footprint.
* **Edge-to-Edge Hero Banner**: 450px cinematic hero banner with multi-stop diagonal gradient fades and instant **Resume** actions.
* **Global Quick Search**: Floating dark-glass search input in the top navigation bar with instant client-side filtering across Dashboard rows and Library grids.
* **Accessibility & Power Optimizations**: Integrated `useReducedMotion` hooks so heavy layout transitions instantaneously drop to 0s when OS battery-saver or reduced-motion flags are active.

### Changed
* **History Data Transfer Object**: Standardized DTO payload key to `ui_type` (`"SINGLE"` vs `"BINGE"`), eliminating potential SQL and JavaScript reserved keyword collisions.
* **Season Tab Layout**: Evolved season tabs from flex-wrapped buttons into a single-line horizontal pill-style momentum scrolling row.
* **Rating Pill Badges**: Standardized rating badge positioning and styling with frosted dark backing (`bg-[#0D0F14]/60 backdrop-blur-md`) and tabular numeral alignment.

### Fixed
* **WebView2 70.3% GPU Compositor Spike**: Resolved critical idle GPU load in WebView2 by eliminating continuous infinite scale transforms beneath CSS `backdrop-filter: blur(...)` layers in `SafeImage.tsx`. Replaced with static hardware fade-ins, dropping idle GPU to **0.0% – 1.0%**.
* **Hero Spotlight CSS Repaints**: Replaced infinite `animate-pulse` on spotlight tags with high-contrast static neon styling, eliminating compositor redraw cycles over blurred backdrop banners.
* **Cascading Delete Safety**: Shifted media deletions completely to atomic SQLite `ON DELETE CASCADE` triggers, preventing orphaned episode records.
* **VLC Argument Parsing**: Automatically strips Windows verbatim prefixing (`\\?\`) before passing file paths to VLC to prevent URI encoding crashes.
* **Synopsis Overflow Safety**: Truncates show and episode overviews to 10,000 characters before database insertion to prevent IPC payload lockups on edge cases.


---

## [1.1.0] - 2026-08-15

### Added
* **Smart Media File Scanner**: Multi-threaded recursive directory traversal using Rust's `walkdir` and multi-pattern regex matching (`S01E01`, `1x01`, `Ep 01`, etc.).
* **Windows Long Path & Shortcut Support**: Full compatibility with deep paths via Windows `\\?\` prefixing and automatic Windows shortcut (`.lnk`) resolution using `parselnk`.
* **Persistent Triage Inbox**: Unmatched media files are parsed and grouped by extracted title into a dedicated triage UI for 1-click matching or manual linking.
* **Custom Streaming Protocol (`watchmark://`)**: Native asynchronous URI protocol supporting Range header parsing and percent-decoded streaming to bypass WebView local-asset restrictions.
* **Database Backup & Restore**: One-click in-app database backups with SHA-256 integrity verification, automated backup rotation, and safe restore routines.
* **Database Optimization Engine**: Integrated SQLite `VACUUM` and `PRAGMA optimize` maintenance tools to maintain lightning-fast queries.
* **OS Credential Manager (Keyring)**: Secure storage of TMDB API keys in the Windows Credential Manager, macOS Keychain, or Linux Secret Service, with fallback to `TMDB_API_KEY` environment variables.
* **System Tray & Global Hotkeys**: Native system tray integration with quick-action triggers (Scan Directory, Check for Updates) and global keyboard shortcuts (`Ctrl + Shift + S`).

### Changed
* **Chunked Scanner Streaming**: Scanner emits batches of 50 files (`scan-match-batch`) over the Tauri event bus to eliminate memory spikes during initial scans of large libraries.
* **Centralized Async IPC Hook**: All Tauri backend invocations migrated to `useAsyncInvoke` for standardized timeout handling, retry logic, and toast alerts.

### Fixed
* **SQLite PRAGMA Panic**: Fixed database connection initialization by updating PRAGMAs (`journal_mode = WAL;`) using `conn.pragma_update` rather than `conn.execute`.
* **Corrupted Settings Auto-Repair**: Defensive deserialization resets invalid or malformed JSON settings to defaults without application crashes.

---

## [1.0.0] - 2026-08-01

### Added
* **Tauri 2.0 Architectural Overhaul**: Complete migration from legacy Python/CustomTkinter to a pure Rust backend and React 19 + TypeScript + Tailwind CSS frontend.
* **Zero-CPU VLC Background Telemetry**: Asynchronous child process management using `tokio::process::Command` and `tokio::select!` event loops that consume zero background CPU while watching.
* **Dynamic VLC Port Probing**: Automatically detects free local ports between 8080 and 8090 and generates cryptographic one-time HTTP passwords for secure VLC telemetry.
* **Smart Playhead Resumption**: Tracks exact playhead position in seconds (`last_position`) and resumes playback seamlessly using VLC's `--start-time` flag.
* **Intelligent Completion Thresholds**: Automatically logs episodes as completed and increments rewatch counts when watched past 90% or within 120s of the credits.
* **Relational SQLite Storage**: Multi-table schema (`Media`, `Episodes`, `Local_Files`, `History`, `Unmatched_Files`) running in Write-Ahead Logging mode (`PRAGMA journal_mode = WAL;`).
* **Evolutionary Migrations**: Safe `ALTER TABLE` transactional migration system ensuring seamless schema updates without data loss.
* **Apple TV / Plex Cinema Aesthetic**: Dark-mode visual system (`#0D0F14`), translucent glassmorphism (`bg-[#1F222A]/60` with `backdrop-blur-md`), and signature VLC Orange accents (`#FF6B00`).
* **Universal TMDB Integration**: Asynchronous TMDB v3 API client with exponential backoff retries (`reqwest-retry`) and local offline poster/backdrop caching.

---

<div align="center">
  <sub>WatchMark Media Tracker • Built with Tauri, Rust, and React</sub>
</div>
