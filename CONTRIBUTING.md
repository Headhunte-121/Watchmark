# Contributing to WatchMark

Thank you for your interest in contributing to **WatchMark Media Tracker**! 

WatchMark is an open-source project combining a high-performance **Rust backend** with a cinema-grade **React 19 + Tailwind CSS** frontend powered by **Tauri 2.0**. We welcome community contributions, bug reports, and feature proposals.

---

## 📋 Code of Conduct

We are committed to providing a welcoming, inclusive, and harassment-free experience for everyone. Please be respectful, constructive, and considerate in all interactions.

---

## 🛠 Development Setup

### Prerequisites
1. **Node.js**: `v18.0.0` or higher ([Download Node.js](https://nodejs.org/))
2. **Rust & Cargo**: Latest stable toolchain ([Install Rust](https://www.rust-lang.org/tools/install))
3. **VLC Media Player**: Installed on your operating system
4. **Free TMDB API Key**: Required for fetching online media metadata

### Cloning and Running
```bash
# 1. Clone the repository
git clone https://github.com/Headhunte-121/Watchmark.git
cd Watchmark

# 2. Navigate to the Tauri project
cd watchmark-tauri

# 3. Install frontend dependencies
npm install

# 4. Launch the Tauri development environment (with hot reload)
npm run tauri dev
```

---

## 🏛 Architectural Standards & Guidelines

### 1. Pure Rust Backend (`src-tauri/`)
* **Async Concurrency**: All heavy I/O (directory scanning, TMDB API requests, database writes) must be asynchronous using `tokio` or `tokio::task::spawn_blocking`.
* **Zero Background CPU**: Long-running loops (such as the VLC heartbeat in `vlc.rs`) must use `tokio::select!` and async sleep timers. Never introduce busy-spin loops or blocking threads.
* **Error Handling**: Use the centralized `AppError` enum in `src-tauri/src/error.rs`. Never use raw `.unwrap()` in production command code; safely propagate errors using `?`.
* **Database PRAGMAs**: SQLite runs in WAL mode. When modifying PRAGMAs, always use `conn.pragma_update(...)` rather than `conn.execute(...)` to avoid query result panics.

### 2. React + TypeScript Frontend (`src/`)
* **Strict Type Safety**: All data exchanged over Tauri IPC commands must have matching TypeScript interfaces.
* **Centralized Invocations**: Use the custom `useAsyncInvoke` hook for all backend `invoke()` calls to maintain unified error handling, timeouts, and toast notifications.
* **Performance & Memory**: Use `VirtualPoster` for large poster grids to maintain 60 FPS performance and avoid DOM bloat. Respect `useReducedMotion` for all animations.
* **Visual Language**: Follow the established cinema-grade palette:
  * Background: `#0D0F14`
  * Surface Glass: `bg-[#1F222A]/60` with `backdrop-blur-md`
  * Signature Accent: VLC Orange `#FF6B00`

---

## 🤖 AI-Assisted Development & Agent Guidelines

WatchMark was developed using an advanced **Human-in-the-Loop AI Engineering** workflow. If you are using AI coding assistants (e.g. Cursor, Claude, Jules, Copilot) to help draft code:
1. **Adhere to `AGENTS.md`**: Follow the Standard Operating Procedure documented in [`AGENTS.md`](./AGENTS.md).
2. **Mandatory Typecheck**: You must run and pass both `cargo check` and `npx tsc --noEmit` before opening a PR.
3. **Atomic Documentation**: Update `CHANGELOG.md` under the `[Unreleased]` section with a clear summary of your changes.

---

## 🧪 Verification & Pull Request Checklist

Before submitting a Pull Request, please ensure your changes pass all verification gates:

```bash
# 1. Verify Rust compiles without warnings or errors
cd watchmark-tauri/src-tauri
cargo check

# 2. Run Rust unit and integration tests
cargo test

# 3. Verify TypeScript compiles cleanly without type errors
cd ../
npx tsc --noEmit

# 4. Verify production build succeeds
npm run build
```

### PR Submission & Branching Policy:
1. Fork the repository and create a feature branch off `develop` (`git checkout -b feat/your-feature-name develop`).
2. Commit your changes with clear, conventional commit messages (`feat:`, `fix:`, `docs:`, `build:`).
3. Push to your fork and open a Pull Request targeting the **`develop`** branch (active integration).
4. Stable releases are tested and promoted from `develop` into **`main`** (production/stable) after passing all compiler verification gates and integrity audits.
5. Fill out the PR template describing your changes, motivation, and verification steps.

Thank you for helping build WatchMark!
