# Security Policy

The WatchMark team takes the security and privacy of our users seriously. As a desktop application handling local media files and metadata API connections, we are committed to transparent, responsible security practices.

---

## 🛡️ Supported Versions

We release patches and security fixes for the latest active release version:

| Version | Supported |
| :--- | :--- |
| **1.2.x** | ✅ Active |
| < 1.2.0 | ❌ End of Life |

---

## 🔒 Security Architecture & Threat Model

WatchMark operates under an **offline-first, client-only architecture**:

1. **Local-Only Database**:
   All user watch history, ratings, and media links are stored locally on your machine in SQLite (`%LOCALAPPDATA%\WatchMark\WatchMark\data\db\watchmark.db`). No user data, telemetry, or watch habits are ever transmitted to external servers.

2. **OS Credential Manager (Keyring)**:
   The TMDB API key is stored in **Windows Credential Manager** rather than in the settings file. If the credential store is unavailable, it falls back to an obfuscated (not encrypted) value in `settings.json`.

3. **Loopback-Only Telemetry (VLC Interface)**:
   WatchMark communicates with VLC Media Player exclusively over loopback TCP interfaces (`127.0.0.1`). Each session dynamically binds a free local port and generates a cryptographic one-time password to prevent unauthorized local processes from probing the interface.

4. **Path Traversal Protection**:
   File scanning and custom streaming handlers route through `filesystem_guard.rs`, strictly validating and canonicalizing paths to prevent directory traversal vulnerabilities.

---

## 🚨 Reporting a Vulnerability

If you discover a security vulnerability within WatchMark, please do **not** open a public GitHub issue. 

Instead, report it responsibly:
* Open a private security advisory via GitHub's [Security Advisories](https://github.com/Headhunte-121/Watchmark/security/advisories/new) feature, or
* Contact the project maintainers directly via email.

Please include:
* A detailed description of the vulnerability.
* Steps to reproduce or proof-of-concept code.
* Impact assessment on user data or system integrity.

We will acknowledge receipt of your vulnerability report within **48 hours** and provide an estimated timeline for a patch.
