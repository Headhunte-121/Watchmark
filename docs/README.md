# WatchMark documentation

Technical documentation for WatchMark. Start with the [project README](../README.md) for an overview, installation and known limitations.

| Document | What it covers |
| :--- | :--- |
| [Features](FEATURES.md) | Every feature in detail: how tracking, scanning, metadata, history and data safety behave, including the exact rules and thresholds. |
| [System design](architecture/SYSTEM_DESIGN.md) | How the app is put together: processes, the frontend/backend boundary, startup, concurrency, VLC supervision, networking and security notes. |
| [Database schema](architecture/DATABASE_SCHEMA.md) | Tables, relationships, indexes, the migration system and how backups and restores work. |
| [Local development](dev/LOCAL_DEVELOPMENT.md) | Building and running from source, where data and logs live, debugging and producing a release. |
| [Development process](dev/DEVELOPMENT_PROCESS.md) | How the project was planned and built with AI coding agents, the review workflow, and lessons learned from real bugs. |
| [Testing](testing/TESTING.md) | What is tested automatically, what is tested by hand, and known gaps. |

Related files in the repository root:

- [CHANGELOG.md](../CHANGELOG.md): user-facing changes in each release.
- [ROADMAP.md](../ROADMAP.md): planned work.
- [CONTRIBUTING.md](../CONTRIBUTING.md): how to propose changes.
- [SECURITY.md](../SECURITY.md): how to report a security issue.
- [AGENTS.md](../AGENTS.md): the working rules given to AI coding agents on this project.
