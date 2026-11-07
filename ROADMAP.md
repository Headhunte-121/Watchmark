# WatchMark Product Roadmap

This document provides a public overview of completed milestones and upcoming features for the **WatchMark Media Tracker**.

---

## 🧭 Project Status Overview

| Version | Focus Area | Status | Target |
| :--- | :--- | :--- | :--- |
| **v1.0.0** | Core Architecture & VLC Telemetry | ✅ Completed | Shipped |
| **v1.1.0** | Filesystem Scanner & Storage Safety | ✅ Completed | Shipped |
| **v1.2.0** | Spotlight Recommendation & Timeline Virtualization | ✅ Completed | Shipped |
| **v1.2.1** | Watch History Overhaul & Stability Patch | 🚀 Current Stable | Shipped |
| **v1.3.0** | Library Intelligence & Deep Metadata | 🔨 In Active Development | Q4 2026 |
| **v1.4.0** | VLC Controls Pro & Background Automation | 📋 Planned | Q1 2027 |
| **v1.5.0** | Viewing Analytics & Data Portability | 📋 Planned | Q2 2027 |
| **v2.0.0** | Ambient Cinema UI & Universal Navigation | 🔮 Future | Q3 2027 |

---

## 🏆 Completed Milestones

### ✅ v1.0.0 — Engine Architecture & Telemetry Bridge
* **Tauri 2.0 Desktop Migration:** Lightweight Rust backend with React 19 + TypeScript frontend.
* **Transactional SQLite Storage:** Multi-table relational schema running in Write-Ahead Logging (`WAL`) mode.
* **Zero-CPU VLC Telemetry:** Asynchronous background polling of VLC's HTTP API with zero idle CPU impact.
* **Smart Playhead Resumption:** Resumes media at the exact second position and marks episodes complete past 90%.
* **Universal TMDB Integration:** Async metadata client with automatic retry backoff and local image caching.

### ✅ v1.1.0 — Filesystem Integration & Storage Safety
* **Multi-Threaded Media Scanner:** High-speed directory traversal supporting complex episode naming patterns.
* **Windows Long Path & Shortcut Support:** Seamless handling of extended Windows paths and `.lnk` shortcuts.
* **Persistent Triage Inbox:** Unmatched video files grouped into a 1-click matching staging interface.
* **Custom Streaming Protocol (`watchmark://`):** Asynchronous protocol supporting partial range headers for instant seeking.
* **Automated Database Safety:** In-app backups with SHA-256 verification and SQLite `VACUUM` optimization.
* **OS Credential Keyring:** Secure storage of API keys using native Windows Credential Manager / macOS Keychain.

### ✅ v1.2.0 — Cinema Spotlight & Timeline Virtualization
* **Intelligent Hero Spotlight:** 8-dimensional weighted recommendation scoring prioritizing in-progress shows, season finales, and new episodes.
* **BingeBlock Accordion Timeline:** Master-detail accordion grouping consecutive episodes watched within 6 hours.
* **Midnight Crossover Detection:** Displays a night icon when viewing sessions cross past midnight.
* **DOM Virtualization:** High-performance intersection observer engine allowing 1,000+ shows to scroll at a smooth 60 FPS.
* **WebView2 GPU Optimization:** Eliminated backdrop-filter composition overhead, reducing idle GPU usage from ~70% to 0.0% – 1.0%.
* **Global Quick Search:** Floating search bar filtering library grids and dashboard rows in real time.

### ✅ v1.2.1 — Watch History Overhaul & Stability Patch (Current Release)
* **Rebuilt Watch History:** Day-grouped timeline with pinned day headings, daily totals, filters (All / TV Shows / Movies), and complete history loading as you scroll.
* **History Management:** Remove a play or a whole binge session with Undo, or clear the full history.
* **Crash Recovery:** Pages that fail to render show a recovery panel instead of a blank window, with errors written to `WatchMark.log`.
* **Playback Tracking Fix:** Completion of VLC plays is recorded in History again.
* **Verified Backups:** SHA-256 checksum files for every backup, verified on restore, with a configurable retention limit (default 5).

---

## 🚀 Upcoming Milestones

### 🔨 v1.3.0 — Library Intelligence & Deep Metadata (Active Development)
*Goal: Elevate the media library into an interactive, highly filterable management hub with rich cast and crew details.*

* **Multi-Dimensional Library Filtering:** Instant filtering by genre combinations, release year range sliders, and watch status (*Watching*, *Completed*, *Unwatched*, *Missing Files*).
* **Flexible Library Sorting:** One-click sorting by Date Added, Community Rating, Release Year, Title, and Runtime.
* **"Quick-View" Hover Cards:** Floating preview cards displaying 16:9 backdrop banners, synopsis teasers, and a 1-click **"Play Next"** button on hover.
* **Rich Cast & Crew Explorer:** Horizontal cast carousel with actor portraits, director credits, and cross-references to other shows in your library.
* **Parental & Content Ratings:** Official rating pills (`TV-MA`, `PG-13`, `R`, etc.) with dark frosted styling.
* **Bulk Season Actions:** One-click actions to mark entire seasons as watched/unwatched or batch-link media files.
* **Personal Organization:** Heart/Favorite toggling, custom tag collections, and private personal notes per show.
* **Display Density Modes:** Toggle between Standard Poster Grid, Compact Grid, and Detailed List View.

---

### 📋 v1.4.0 — VLC Telemetry Pro & Background Automation
*Goal: Deeper operating system integration, background media discovery, and direct playback stream control.*

* **Audio & Subtitle Track Switcher:** In-app selection of embedded audio languages and subtitle tracks prior to launching playback.
* **Playback Quick Shortcuts:** In-app triggers for **Skip Intro (85s)**, **Skip Outro (120s)**, and variable playback speeds (`1.0x` – `2.0x`).
* **OS Window Coordination:** Auto-minimize WatchMark when VLC launches and automatically restore focus when playback ends.
* **In-App Playback HUD:** Real-time playback status toast notifications with volume and mute synchronization.
* **Multi-Directory Watch Folders:** Manage multiple storage drives with individual folder labels and `.watchmarkignore` support.
* **Silent Background Scanner:** Automatic, non-blocking library checks on startup and native OS file-change monitoring.
* **Temporary File Shield:** Intelligent exclusion of active download files (`.crdownload`, `.part`, `.tmp`) until downloads complete.
* **System Tray Daemon:** Run silently in the background with quick tray actions (Scan Now, Playback Status, Show/Hide).

---

### 📋 v1.5.0 — Viewing Analytics & Data Portability
*Goal: Comprehensive personal viewing statistics, historical backdating, and cross-platform import/export.*

* **Personal Analytics Dashboard:** Visual insights into total hours watched, movie-to-TV ratios, top genres, and monthly viewing velocity.
* **Time-of-Day Insights & Flashbacks:** Dynamic contextual greetings and **"On This Day"** historical watch memories.
* **Manual Watch Backdating:** Calendar date/time picker to log past episodes or adjust historic timestamps.
* **History Timeline Search:** Full-text search and filtering across complete watch history with virtualized scrolling.
* **Trakt.tv & Letterboxd Data Import:** CSV import tools to bring in existing watch histories from third-party platforms.
* **Full JSON Data Portability:** One-click export of complete watch records, custom tags, and ratings with checksum verification.
* **Continue Watching Queue Pruning:** Option to hide or dismiss titles from the active queue without deleting history.

---

### 🔮 v2.0.0 — Ambient Cinema UI & Universal Navigation
*Goal: Apple TV-grade visual polish, ambient chromatic backgrounds, and keyboard-first desktop navigation.*

* **Dynamic Chromatic Ambient Lighting:** Real-time extraction of dominant accent colors from poster artwork to cast subtle atmospheric backdrop glow.
* **Universal Command Palette (`Ctrl+K`):** Global fuzzy-search modal for lightning-fast navigation, library queries, and quick actions.
* **Native Hardware Mouse Navigation:** Seamless forward/backward navigation via Mouse Buttons 4 and 5 with exact scroll position restoration.
* **Cinematic Micro-Interactions:** Staggered spring animations on page load, glowing playhead indicators, and interactive star rating hovers.
* **Persistent Local Image Caching:** Full offline poster and backdrop storage with intelligent cache management.
* **Desktop Hardening & Reset Tools:** Granular layout reset options, graceful unannounced-episode states, and frameless window dragging.

---

## 🤝 Tracking & Contributing

WatchMark uses native GitHub features to track milestones and active work:
* **Milestones:** View real-time progress bars for each upcoming version under [GitHub Milestones](../../milestones).
* **Issues:** Each feature is tracked as a standalone issue tagged with its corresponding version.
* **Feature Requests:** Have an idea? Open a suggestion using our [Feature Request Template](.github/ISSUE_TEMPLATE/feature_request.md).

---

<div align="center">
  <sub>WatchMark Media Tracker • Built with Tauri, Rust, and React</sub>
</div>
