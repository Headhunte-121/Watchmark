# Local development

How to build, run, debug and release WatchMark from source. Development and testing have only been done on Windows 10/11.

## Prerequisites

| Tool | Version | Notes |
| :--- | :--- | :--- |
| Node.js | 20 or later | CI uses Node 20. |
| Rust | Current stable | Install with [rustup](https://rustup.rs/). |
| Microsoft C++ Build Tools | | Required by Tauri on Windows (the "Desktop development with C++" workload). |
| WebView2 Runtime | | Preinstalled on Windows 11. |
| VLC Media Player | 3.x | Needed to test playback tracking. |
| TMDB API key | v3 key | Free from your [TMDB account settings](https://www.themoviedb.org/settings/api). |

## Run the app

```bash
git clone https://github.com/Headhunte-121/Watchmark.git
cd Watchmark/watchmark-tauri
npm install
npm run tauri dev
```

This starts the Vite dev server on `http://127.0.0.1:1420` and opens the app window. Frontend changes reload instantly; Rust changes trigger a rebuild and restart of the window.

**TMDB key for development.** If the `TMDB_API_KEY` environment variable is set, WatchMark uses it instead of the key saved in Settings, and won't save it anywhere. That's handy for test machines and CI.

## Where things live

All app data is under `%LOCALAPPDATA%\WatchMark\WatchMark\data\`:

| Path | Contents |
| :--- | :--- |
| `db\watchmark.db` | The SQLite database (plus `-wal` and `-shm` files while the app runs) |
| `backups\` | Automatic backups (`.bak`) and their `.sha256` checksum files |
| `cache\posters`, `cache\backdrops`, `cache\stills` | Downloaded artwork |
| `fonts\` | The Inter font, downloaded on first launch |
| `settings.json` | Settings (the TMDB key is kept in Windows Credential Manager instead, under `WatchMark`) |
| `WatchMark.log` | The log file |

To start from a clean slate, close the app and rename the `data` folder. WatchMark recreates it on the next launch.

## Debugging

**Logs.** Backend and frontend messages both go to `WatchMark.log`; frontend logs are forwarded through a `frontend_log` command. Follow it live:

```powershell
Get-Content "$env:LOCALAPPDATA\WatchMark\WatchMark\data\WatchMark.log" -Wait -Tail 50
```

Log levels can be changed without restarting: the overall level in **Settings → System**, and per backend module (for example `debug` for `vlc` only) in **Settings → Advanced**.

**DevTools.** In development builds, right-click in the window and choose **Inspect**, or press `Ctrl+Shift+I`, to open the WebView DevTools. Rendering errors also appear there.

**Database.** You can inspect the database with the `sqlite3` CLI or a tool like DB Browser for SQLite. Open it read-only while the app is running, or work on a copy:

```bash
sqlite3 -readonly "%LOCALAPPDATA%\WatchMark\WatchMark\data\db\watchmark.db"
```

See [Database schema](../architecture/DATABASE_SCHEMA.md) for the tables.

## Checks before committing

From `watchmark-tauri/`:

```bash
npm run build                 # TypeScript type check + production frontend build
cd src-tauri && cargo check   # Rust compile check
cd src-tauri && cargo test    # Rust unit tests
```

CI on GitHub Actions runs `npm run build` and `cargo check` on Windows for pushes to `main` and `Tauri` and for pull requests into them. It does not run `cargo test`.

## Making a release

1. Update the version in all five places: `package.json`, `package-lock.json` (the two root entries), `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, and `src-tauri/Cargo.lock` (updated automatically by `cargo check`).
2. Add a section for the version to `CHANGELOG.md`. Keep it user-facing.
3. Build:

   ```bash
   npm run tauri build
   ```

   Outputs in `src-tauri/target/release/`:
   - `watchmark-tauri.exe`: the standalone executable
   - `bundle/nsis/WatchMark Media Tracker_<version>_x64-setup.exe`: the installer
   - `bundle/msi/WatchMark Media Tracker_<version>_x64_en-US.msi`: the MSI package
4. Tag the release commit (`git tag -a vX.Y.Z`) and push the tag.
5. Create a GitHub Release from the tag. Upload the installer, MSI and executable (renamed without spaces, for example `WatchMark_<version>_x64-setup.exe` and `WatchMark.exe`) and a `SHA256SUMS.txt` with their checksums.

Built binaries are never committed to the repository; they're only published as release assets.
