# Testing

How WatchMark is tested today, and where the gaps are.

## Automated

### Rust unit tests
About 90 tests in `watchmark-tauri/src-tauri/src/`, run with:

```bash
cd watchmark-tauri/src-tauri
cargo test
```

| Test module | Tests | Covers |
| :--- | :---: | :--- |
| `db_tests.rs`, `db_tests_append.rs` | 35 | Schema constraints, migrations, cascading deletes, history ordering and grouping, date handling, ratings, file-name parsing |
| `sanitizer_tests.rs` | 10 | Cleaning dates, text and runtimes from TMDB |
| `tmdb_tests_3_4.rs`, `tmdb_tests_3_5.rs` | 10 | Mapping TMDB responses, missing data, movies and collections |
| `network_tests.rs` | 7 | HTTPS-only enforcement and error mapping |
| `commands_tests_*.rs` | 11 | Export, optimize, TMDB key handling, season sync behaviour, NULL-heavy episode data |
| `restore_tests.rs`, `backup_tests.rs` | 5 | Backup retention, checksums and restore staging |
| `task_queue_tests.rs` | 3 | Write queue ordering and retries |
| Inline tests in `commands.rs`, `models.rs`, `logging.rs` | 9 | Time-gap calculation, flexible JSON parsing, log filters |

### Compile and type checks
- `cargo check` (Rust) and `npm run build` (TypeScript type check plus production build) run before every commit.
- GitHub Actions runs both on Windows for pushes and pull requests to `main` and `Tauri`.

### Not automated yet
- **No frontend tests** (components, hooks, stores).
- **No end-to-end tests** of the running app.
- **CI doesn't run `cargo test`**, so unit tests only run when someone runs them locally.
- VLC tracking, scanning real folders and TMDB calls against the live API are tested by hand only.

These gaps let several bugs reach a release; [Development process](../dev/DEVELOPMENT_PROCESS.md#what-didnt-and-what-changed) lists them. Adding `cargo test` to CI and a small set of frontend tests are the first steps planned.

## Manual test checklist

Run these on a Windows machine with VLC installed before a release.

### Playback
| Scenario | Steps | Expected |
| :--- | :--- | :--- |
| Resume | Stop an episode partway, close VLC, press Play again | VLC starts at the saved position |
| Completion | Watch past 90% and close VLC | Episode is Completed; History shows it as Watched |
| Accidental launch | Start an episode and close VLC within a few seconds | No History entry is kept; earlier progress unchanged |
| Port in use | Occupy port 8080 (for example with another program), then play | VLC starts on the next free port and is tracked |
| VLC crash | End `vlc.exe` in Task Manager during playback | A "Playback interrupted" notice appears; the app keeps working |
| Binge session | Play two episodes of the same show back to back | History groups them into one session |
| Different show | Play an episode of another show straight after | A separate History entry, not merged into the previous session |

### Scanning and the Inbox
| Scenario | Steps | Expected |
| :--- | :--- | :--- |
| Long paths | Scan a folder whose file paths exceed 260 characters | Files are found and matched |
| Shortcut loop | Put two `.lnk` files pointing at each other in a scanned folder | The scan finishes; the loop is skipped and logged |
| Non-English names | Scan files with Japanese or accented names | Names display and match correctly |
| Unmatched files | Scan files for a show not in the library | They appear in the Inbox grouped by name; one-click match links them |
| Moved files | Move a matched file and rescan | The link is updated and history is kept |

### History
| Scenario | Steps | Expected |
| :--- | :--- | :--- |
| Full history | Scroll to the end of a long history | All entries load; the end marker appears only at the real end |
| Day headings | Scroll through several days | Each heading sticks under the top bar and never covers an entry |
| Remove and undo | Remove an entry and press Undo | The entry comes back; nothing is deleted |
| Remove | Remove an entry and wait 5 seconds, then restart the app | The entry stays removed |
| Filters | Switch between All, TV Shows and Movies | Only matching entries are shown |

### Settings and data
| Scenario | Steps | Expected |
| :--- | :--- | :--- |
| Corrupt settings | Delete the last few characters of `settings.json` and start the app | It starts with default settings and repairs the file |
| Off-screen window | Set `x` and `y` to `-5000` in `settings.json` and start | The window opens centred on the main screen |
| Backup and restore | Export a backup, change something, restore the backup | The change is undone after the app restarts |
| Damaged backup | Edit a byte in a backup that has a `.sha256` file, then try to restore it | The restore is refused |
| No write access | Make the data folder read-only and start the app | A clear error dialog, then the app exits |

### Interface
| Scenario | Steps | Expected |
| :--- | :--- | :--- |
| Idle GPU | Leave the Dashboard idle with animations on; check Task Manager | GPU usage stays near 0% |
| Render error | Force an error in one view (development build only) | That view shows a recovery panel; the sidebar still works; the error is in `WatchMark.log` |

## Verified for v1.2.1

These were checked for the v1.2.1 release:
- `cargo check` and `npm run build` pass.
- History paging, filters and day headings, checked in a browser against a stand-in backend with 120 sessions: every page loads, and headings sit directly under the top bar at all 601 scroll positions tested.
- A copy of a real database with 985 history rows pages through completely, with no session split across pages.
- The fixed playback-tracking query matches every one of 616 real tracked plays (the old query matched none).
- An automated scan of the frontend for React hooks called after an early return finds no violations.
